//! The two sides of a set read from the database (TODO/001, phase 1): the
//! touches of A and B, one sequence in `events`, projected and replayed on
//! each side; scouting only A, B has no side and A replays as today. A set
//! saved on both sides (phase 2): its touches with their sides, A read as
//! the stream projected on its side.
#[cfg(test)]
mod tests {
    use crate::{
        analysis::match_stats,
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        domain::{Event, ScoutingMode, Side, adapters::EventsOfA, sql::Sql},
        localization::Language,
        match_form::{MatchInput, Opponent},
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            set_reader::SetReader,
            set_writer::SetWriter,
            sqlite::{
                rows::{insert_event, next_seq, set_rows},
                sets::SqliteSetReader,
                sqlite,
            },
            team_writer::{TeamInput, TeamWriter},
        },
        rally::project,
        shapes::{
            country::CountryCode,
            enums::{
                EvalEnum, EventTypeEnum, GenderEnum, RoleEnum, ScoutingRulesEnum,
                TeamClassificationEnum, TeamSideEnum,
            },
            flags::EventFlags,
            r#match::MatchEntry,
            player::PlayerEntry,
            set::SetEntry,
            snapshot::EventEntry,
            stats::Stats,
            team::TeamEntry,
        },
        tests::Sqlite,
        video::EventRef,
    };
    use EvalEnum::*;
    use EventTypeEnum::*;
    use chrono::{DateTime, Duration, FixedOffset, TimeZone, Utc};
    use rusqlite::params;
    use uuid::Uuid;

    fn date() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-11-14T18:00:00+01:00").unwrap_or_default()
    }

    /// Six players and the libero (7).
    async fn team(p: &Sqlite, name: &str) -> TeamEntry {
        let players = (1..=7)
            .map(|n| PlayerEntry {
                id: Uuid::new_v4(),
                name: format!("{name} {n}"),
                number: n,
                role: Some(match n {
                    7 => RoleEnum::Libero,
                    1 => RoleEnum::Setter,
                    _ => RoleEnum::OutsideHitter,
                }),
                ..PlayerEntry::default()
            })
            .collect();
        p.team_writer
            .save(TeamInput::New {
                id: None,
                name: name.into(),
                year: 2026,
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Men),
                country: CountryCode::known("IT"),
                players,
            })
            .await
            .unwrap()
    }

    fn positions(t: &TeamEntry) -> [Uuid; 6] {
        std::array::from_fn(|i| t.players[i].id)
    }

    fn libero(t: &TeamEntry) -> Uuid {
        t.players[6].id
    }

    /// A against B, set 1 with A serving and A's lineup; scouted on both
    /// sides with B's lineup too.
    async fn set_one(p: &Sqlite, both: bool) -> (TeamEntry, TeamEntry, MatchEntry) {
        let (a, b) = (team(p, "Cortona").await, team(p, "Cascina").await);
        let m = p
            .match_writer
            .create(
                &a,
                MatchInput {
                    opponent: Opponent::Team(b.id),
                    ..MatchInput::against("", date(), true, DEFAULT_MAX_SUBSTITUTIONS, "", vec![])
                },
            )
            .await
            .unwrap();
        p.set_writer
            .create(
                &m,
                1,
                TeamSideEnum::Us,
                positions(&a),
                libero(&a),
                None,
                a.players[0].id,
                vec![],
            )
            .await
            .unwrap();
        let id = Uuid::parse_str(&m.id).unwrap();
        let [p1, p2, p3, p4, p5, p6] = positions(&b);
        if both {
            p.db.write(|tx| {
                tx.execute("UPDATE matches SET scouting = 'both'", [])
                    .and_then(|_| {
                        tx.execute(
                            "INSERT INTO set_lineups (match_id, set_number, side, p1, p2, p3,
                                 p4, p5, p6, libero, fallback_libero, setter)
                             VALUES (?1, 1, 'b', ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?2)",
                            params![
                                Sql(id),
                                Sql(p1),
                                Sql(p2),
                                Sql(p3),
                                Sql(p4),
                                Sql(p5),
                                Sql(p6),
                                Sql(libero(&b))
                            ],
                        )
                    })
                    .map(|_| ())
                    .map_err(sqlite)
            })
            .unwrap();
        }
        (a, b, m)
    }

    /// The touches appended to set 1, each on its side.
    fn add(p: &Sqlite, m: &MatchEntry, touches: &[(Side, EventTypeEnum, Uuid, EvalEnum)]) {
        let match_id = Uuid::parse_str(&m.id).unwrap();
        let start = Utc.with_ymd_and_hms(2026, 11, 14, 17, 0, 0).unwrap();
        p.db.write(|tx| {
            touches
                .iter()
                .enumerate()
                .try_for_each(|(n, (side, event_type, player, eval))| {
                    insert_event(
                        tx,
                        &Event {
                            id: Uuid::new_v4(),
                            match_id,
                            set_number: 1,
                            seq: next_seq(tx, match_id, 1)?,
                            side: *side,
                            entry: EventEntry {
                                timestamp: start + Duration::seconds(n as i64),
                                event_type: *event_type,
                                player: Some(*player),
                                eval: Some(*eval),
                                target_player: None,
                                technique: None,
                                flags: EventFlags::default(),
                            },
                            deleted: false,
                        },
                    )
                })
        })
        .unwrap();
    }

    fn reader(p: &Sqlite) -> SqliteSetReader {
        SqliteSetReader::new(p.db.clone(), Language::default())
    }

    #[tokio::test]
    async fn both_sides_are_read_and_agree_on_the_score() {
        let p = Sqlite::new();
        let (a, b, m) = set_one(&p, true).await;
        add(
            &p,
            &m,
            &[
                (Side::A, S, a.players[0].id, Positive),
                (Side::B, P, libero(&b), Negative),
                (Side::B, A, b.players[1].id, Perfect),
                // B serves now
                (Side::B, S, b.players[1].id, Error),
            ],
        );
        let sides = reader(&p).read_sides(&m).unwrap();
        let scores: Vec<_> = sides
            .iter()
            .map(|s| {
                (
                    (s.a.score_us, s.a.score_them),
                    s.b.as_ref().map(|b| (b.score_us, b.score_them)),
                )
            })
            .collect();
        assert_eq!(scores, vec![((1, 1), Some((1, 1)))]);
    }

    /// A set the app writes follows the current scouting rules, and says
    /// so.
    #[tokio::test]
    async fn a_new_set_is_saved_with_the_current_scouting_rules() {
        let p = Sqlite::new();
        let (_, _, m) = set_one(&p, false).await;
        let sets =
            p.db.read(|tx| set_rows(tx, Uuid::parse_str(&m.id).unwrap()))
                .unwrap();
        let rules: Vec<_> = sets.iter().map(|s| s.row.scouting_rules).collect();
        assert_eq!(rules, vec![Some(ScoutingRulesEnum::CURRENT)]);
    }

    /// The set of match `m` saved before its scouting rules were (v1).
    fn of_v1(p: &Sqlite, m: &MatchEntry) {
        p.db.write(|tx| {
            tx.execute(
                "UPDATE sets SET scouting_rules = NULL WHERE match_id = ?1",
                params![Sql(Uuid::parse_str(&m.id).unwrap())],
            )
            .map_err(sqlite)
        })
        .unwrap();
    }

    /// The scouting rules of set 1 of `m`, and the evaluations of its rows
    /// as saved.
    fn saved_rows(p: &Sqlite, m: &MatchEntry) -> (Option<ScoutingRulesEnum>, Vec<String>) {
        let sets =
            p.db.read(|tx| set_rows(tx, Uuid::parse_str(&m.id).unwrap()))
                .unwrap();
        let set = sets.first().unwrap();
        let mut rows: Vec<&Event> = set.events.iter().filter(|e| !e.deleted).collect();
        rows.sort_by_key(|e| e.seq);
        (
            set.row.scouting_rules,
            rows.iter()
                .map(|e| {
                    format!(
                        "{}{}",
                        e.entry.event_type,
                        e.entry.eval.map_or(String::new(), |v| v.to_string())
                    )
                })
                .collect(),
        )
    }

    fn told(set: &SetEntry) -> Vec<String> {
        set.events
            .iter()
            .map(|e| {
                format!(
                    "{}{}",
                    e.event_type,
                    e.eval.map_or(String::new(), |v| v.to_string())
                )
            })
            .collect()
    }

    /// Our serve, our block touched (v1: the ball back to them, who attack
    /// again), our dig: a set of v1.
    async fn block_of_v1(p: &Sqlite) -> (TeamEntry, MatchEntry) {
        let (a, _, m) = set_one(p, false).await;
        add(
            p,
            &m,
            &[
                (Side::A, S, a.players[0].id, Positive),
                (Side::A, B, a.players[1].id, Negative),
                (Side::A, D, libero(&a), Positive),
            ],
        );
        of_v1(p, &m);
        (a, m)
    }

    /// A set of v1 is read as the current rules tell it, its rows as they
    /// were.
    #[tokio::test]
    async fn a_set_of_v1_is_read_with_the_current_rules() {
        let p = Sqlite::new();
        let (_, m) = block_of_v1(&p).await;
        let sets = reader(&p).read_all(&m).await.unwrap();
        assert_eq!(told(&sets[0]), vec!["S+", "B!", "D+"]);
        assert!(sets[0].compute_snapshot(m.max_substitutions).is_ok());
        assert_eq!(
            saved_rows(&p, &m),
            (None, vec!["S+".into(), "B-".into(), "D+".into()])
        );
    }

    /// A set with the current rules is read as saved: its block `-` keeps
    /// the ball with the blockers.
    #[tokio::test]
    async fn a_set_of_the_current_rules_is_read_as_saved() {
        let p = Sqlite::new();
        let (a, _, m) = set_one(&p, false).await;
        add(
            &p,
            &m,
            &[
                (Side::A, S, a.players[0].id, Positive),
                (Side::A, B, a.players[1].id, Negative),
                (Side::A, D, libero(&a), Negative),
            ],
        );
        let sets = reader(&p).read_all(&m).await.unwrap();
        assert_eq!(told(&sets[0]), vec!["S+", "B-", "D-"]);
        assert!(sets[0].compute_snapshot(m.max_substitutions).is_ok());
    }

    /// An event joining a set of v1: the set first brought to the current
    /// rules, its rows translated.
    #[tokio::test]
    async fn an_event_appended_to_a_set_of_v1_brings_it_to_the_current_rules() {
        let p = Sqlite::new();
        let (a, m) = block_of_v1(&p).await;
        let attack = EventEntry {
            timestamp: Utc::now(),
            event_type: A,
            player: Some(a.players[1].id),
            eval: Some(Perfect),
            target_player: None,
            technique: None,
            flags: EventFlags::default(),
        };
        p.set_writer.append_event(&m, 1, &attack).await.unwrap();
        assert_eq!(
            saved_rows(&p, &m),
            (
                Some(ScoutingRulesEnum::CURRENT),
                vec!["S+".into(), "B!".into(), "D+".into(), "A#".into()]
            )
        );
        let sets = reader(&p).read_all(&m).await.unwrap();
        assert_eq!(told(&sets[0]), vec!["S+", "B!", "D+", "A#"]);
    }

    /// Undo in a set of v1: the set brought to the current rules, the last
    /// event given back as they tell it.
    #[tokio::test]
    async fn undo_in_a_set_of_v1_brings_it_to_the_current_rules() {
        let p = Sqlite::new();
        let (a, _, m) = set_one(&p, false).await;
        add(
            &p,
            &m,
            &[
                (Side::A, S, a.players[0].id, Positive),
                (Side::A, B, a.players[1].id, Negative),
            ],
        );
        of_v1(&p, &m);
        let removed = p.set_writer.remove_last_event(&m, 1).await.unwrap();
        assert_eq!(
            removed.map(|e| (e.event_type, e.eval)),
            Some((B, Some(Exclamative)))
        );
        assert_eq!(
            saved_rows(&p, &m),
            (Some(ScoutingRulesEnum::CURRENT), vec!["S+".into()])
        );
    }

    /// A set of v1 rewritten (a correction): saved with the current rules.
    #[tokio::test]
    async fn a_set_of_v1_rewritten_is_saved_with_the_current_rules() {
        let p = Sqlite::new();
        let (_, m) = block_of_v1(&p).await;
        let set = reader(&p).read_all(&m).await.unwrap().remove(0);
        p.set_writer.update(&m, &set).await.unwrap();
        assert_eq!(
            saved_rows(&p, &m),
            (
                Some(ScoutingRulesEnum::CURRENT),
                vec!["S+".into(), "B!".into(), "D+".into()]
            )
        );
    }

    /// Our serve `/` (their reception straight back), our block `+` of it
    /// and our dig after the block, our attack: a set of v3, the block of
    /// no attack.
    async fn block_of_v3(p: &Sqlite) -> (TeamEntry, MatchEntry) {
        let (a, _, m) = set_one(p, false).await;
        add(
            p,
            &m,
            &[
                (Side::A, S, a.players[0].id, Over),
                (Side::A, B, a.players[1].id, Positive),
                (Side::A, D, libero(&a), Positive),
                (Side::A, A, a.players[1].id, Perfect),
            ],
        );
        p.db.write(|tx| {
            tx.execute(
                "UPDATE sets SET scouting_rules = 'v3' WHERE match_id = ?1",
                params![Sql(Uuid::parse_str(&m.id).unwrap())],
            )
            .map_err(sqlite)
        })
        .unwrap();
        (a, m)
    }

    /// A set of v3 is read without the touches the current rules leave
    /// out (the block of no attack kept by the blockers, the dig after
    /// it), its rows as they were.
    #[tokio::test]
    async fn a_set_of_v3_is_read_without_the_touches_left_out() {
        let p = Sqlite::new();
        let (_, m) = block_of_v3(&p).await;
        let sets = reader(&p).read_all(&m).await.unwrap();
        assert_eq!(told(&sets[0]), vec!["S/", "A#"]);
        let snapshot = sets[0].compute_snapshot(m.max_substitutions).unwrap();
        assert_eq!((snapshot.score_us, snapshot.score_them), (1, 0));
        assert_eq!(
            saved_rows(&p, &m),
            (
                Some(ScoutingRulesEnum::V3),
                vec!["S/".into(), "B+".into(), "D+".into(), "A#".into()]
            )
        );
    }

    /// An event joining a set of v3: the rows the current rules leave out
    /// are undone, the set brought to the current rules.
    #[tokio::test]
    async fn an_event_appended_to_a_set_of_v3_undoes_the_touches_left_out() {
        let p = Sqlite::new();
        let (a, m) = block_of_v3(&p).await;
        let serve = EventEntry {
            timestamp: Utc::now(),
            event_type: S,
            player: Some(a.players[0].id),
            eval: Some(Perfect),
            target_player: None,
            technique: None,
            flags: EventFlags::default(),
        };
        p.set_writer.append_event(&m, 1, &serve).await.unwrap();
        assert_eq!(
            saved_rows(&p, &m),
            (
                Some(ScoutingRulesEnum::CURRENT),
                vec!["S/".into(), "A#".into(), "S#".into()]
            )
        );
        let sets = reader(&p).read_all(&m).await.unwrap();
        assert_eq!(told(&sets[0]), vec!["S/", "A#", "S#"]);
    }

    /// Undo in a set of v3: the touches left out undone first, then the
    /// last event the current rules tell.
    #[tokio::test]
    async fn undo_in_a_set_of_v3_skips_the_touches_left_out() {
        let p = Sqlite::new();
        let (_, m) = block_of_v3(&p).await;
        let removed = p.set_writer.remove_last_event(&m, 1).await.unwrap();
        assert_eq!(
            removed.map(|e| (e.event_type, e.eval)),
            Some((A, Some(Perfect)))
        );
        assert_eq!(
            saved_rows(&p, &m),
            (Some(ScoutingRulesEnum::CURRENT), vec!["S/".into()])
        );
    }

    /// Scouting only A: no side for B, A as `compute_snapshot` replays it.
    #[tokio::test]
    async fn scouting_a_reads_only_a_as_today() {
        let p = Sqlite::new();
        let (a, _, m) = set_one(&p, false).await;
        add(&p, &m, &[(Side::A, S, a.players[0].id, Perfect)]);
        let sides = reader(&p).read_sides(&m).unwrap();
        let scores: Vec<_> = sides
            .iter()
            .map(|s| ((s.a.score_us, s.a.score_them), s.b.is_none()))
            .collect();
        assert_eq!(scores, vec![((1, 0), true)]);
    }

    /// The match from B's side (phase 2, step 3): B's team against A, B's
    /// score, B's statistics the ones its side replays to.
    #[tokio::test]
    async fn the_match_from_b_side() {
        let p = Sqlite::new();
        let (a, b, m) = set_one(&p, true).await;
        add(
            &p,
            &m,
            &[
                (Side::A, S, a.players[0].id, Positive),
                (Side::B, P, libero(&b), Negative),
                (Side::B, A, b.players[1].id, Perfect),
                (Side::B, S, b.players[1].id, Positive),
                (Side::A, P, libero(&a), Negative),
                (Side::A, A, a.players[1].id, Error),
            ],
        );
        let of_b = p
            .match_reader
            .read_of_b(&a, &m.id)
            .unwrap()
            .expect("B's side");
        assert_eq!(
            (of_b.team.id, of_b.opponent.as_str(), of_b.home),
            (b.id, "Cortona", false)
        );
        let snapshot = of_b.sets[0]
            .compute_snapshot(of_b.max_substitutions)
            .unwrap();
        assert_eq!((snapshot.score_us, snapshot.score_them), (2, 0));
        let sides = reader(&p).read_sides(&m).unwrap();
        let of_sides = sides[0].b.as_ref().map(|b| b.stats.clone());
        assert_eq!(Some(match_stats(&of_b).unwrap()), of_sides);
        assert_ne!(of_sides, Some(Stats::new()));
    }

    /// The event of B born from a touch of A is told with the event of A
    /// of the same touch (the video corrections are saved by A's).
    #[tokio::test]
    async fn b_events_know_the_event_of_a_they_come_from() {
        let p = Sqlite::new();
        let (a, b, m) = set_one(&p, true).await;
        add(
            &p,
            &m,
            &[
                (Side::A, S, a.players[0].id, Positive),
                (Side::B, P, libero(&b), Negative),
                (Side::B, A, b.players[1].id, Perfect),
                (Side::B, S, b.players[1].id, Positive),
                (Side::A, P, libero(&a), Negative),
                (Side::A, A, a.players[1].id, Error),
            ],
        );
        let origins = p.match_reader.origins_of_b(&a, &m.id).unwrap();
        let of_a = p.match_reader.read_single(&a, &m.id).await.unwrap();
        let of_b = p.match_reader.read_of_b(&a, &m.id).unwrap().unwrap();
        // B's own touches before the point it is told: only its attack
        // (the same instant as the point A is told) and the point told by
        // A's attack have an event of A
        assert_eq!(origins, vec![(1, vec![None, Some(1), None, Some(3)])]);
        let (events_a, events_b) = (&of_a.sets[0].events, &of_b.sets[0].events);
        assert_eq!(events_b[1].timestamp, events_a[1].timestamp);
        assert_eq!(events_b[3].timestamp, events_a[3].timestamp);
    }

    /// Scouting only A: no side of B.
    #[tokio::test]
    async fn scouting_a_has_no_b_side() {
        let p = Sqlite::new();
        let (a, _, m) = set_one(&p, false).await;
        add(&p, &m, &[(Side::A, S, a.players[0].id, Perfect)]);
        assert!(p.match_reader.read_of_b(&a, &m.id).unwrap().is_none());
    }

    /// The sides not replaying alike: no side of B, an error naming the
    /// touch, no panic.
    #[tokio::test]
    async fn b_side_of_a_touch_not_following_is_an_error() {
        let p = Sqlite::new();
        let (a, b, m) = set_one(&p, true).await;
        add(&p, &m, &[(Side::B, A, b.players[1].id, Perfect)]);
        let read = p.match_reader.read_of_b(&a, &m.id);
        assert!(
            matches!(&read, Err(e) if e.to_string().contains("touch 0 doesn't follow")),
            "{read:?}"
        );
    }

    /// A touch of B not following (its kill tells A a point before the
    /// serve): an error naming it, no panic.
    #[tokio::test]
    async fn a_touch_not_following_is_an_error() {
        let p = Sqlite::new();
        let (_, b, m) = set_one(&p, true).await;
        // B attacks before A serves
        add(&p, &m, &[(Side::B, A, b.players[1].id, Perfect)]);
        let read = reader(&p).read_sides(&m);
        assert!(
            matches!(&read, Err(e) if e.to_string().contains("touch 0 doesn't follow")),
            "{read:?}"
        );
    }

    fn touch(
        side: Side,
        event_type: EventTypeEnum,
        player: Uuid,
        eval: EvalEnum,
    ) -> (Side, EventEntry) {
        let start = Utc.with_ymd_and_hms(2026, 11, 14, 17, 0, 0).unwrap();
        (
            side,
            EventEntry {
                timestamp: start,
                event_type,
                player: Some(player),
                eval: Some(eval),
                target_player: None,
                technique: None,
                flags: EventFlags::default(),
            },
        )
    }

    /// A against B on both sides, set 1 with A serving: the starts of A
    /// and B as each sees it.
    async fn on_both_sides(p: &Sqlite) -> (TeamEntry, TeamEntry, MatchEntry, SetEntry, SetEntry) {
        let (a, b) = (team(p, "Cortona").await, team(p, "Cascina").await);
        let m = p
            .match_writer
            .create(
                &a,
                MatchInput {
                    opponent: Opponent::Team(b.id),
                    ..MatchInput::against("", date(), true, DEFAULT_MAX_SUBSTITUTIONS, "", vec![])
                },
            )
            .await
            .unwrap();
        p.match_writer
            .save_scouting(&m, ScoutingMode::Both)
            .await
            .unwrap();
        let start = |t: &TeamEntry, serving| {
            SetEntry::new(1, serving, positions(t), libero(t), None, t.players[0].id).unwrap()
        };
        let (start_a, start_b) = (start(&a, TeamSideEnum::Us), start(&b, TeamSideEnum::Them));
        (a, b, m, start_a, start_b)
    }

    /// A rally won by B, then B's serve error.
    fn rallies(a: &TeamEntry, b: &TeamEntry) -> Vec<(Side, EventEntry)> {
        vec![
            touch(Side::A, S, a.players[0].id, Positive),
            touch(Side::B, P, libero(b), Negative),
            touch(Side::B, A, b.players[1].id, Perfect),
            touch(Side::B, S, b.players[1].id, Error),
        ]
    }

    fn ids(p: &Sqlite) -> Vec<String> {
        p.db.read(|tx| {
            tx.prepare("SELECT id FROM events WHERE deleted = 0 ORDER BY seq")
                .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
                .map_err(sqlite)
        })
        .unwrap()
    }

    /// The start of a set on both sides: both lineups saved, no touches;
    /// a set already started can't be started again, a removed one can.
    #[tokio::test]
    async fn a_set_on_both_sides_starts_with_both_lineups() {
        let p = Sqlite::new();
        let (_, _, m, start_a, start_b) = on_both_sides(&p).await;
        p.set_writer
            .create_both(&m, &start_a, &start_b)
            .await
            .unwrap();
        let sides = reader(&p).read_sides(&m).unwrap();
        assert_eq!(sides.len(), 1);
        assert_eq!(
            sides[0]
                .b
                .as_ref()
                .map(|s| s.current_lineup.get_positions_without_libero()),
            Some(start_b.initial_positions)
        );
        assert!(ids(&p).is_empty());
        assert!(
            p.set_writer
                .create_both(&m, &start_a, &start_b)
                .await
                .is_err()
        );
        p.set_writer.remove(&m, 1).await.unwrap();
        assert!(
            p.set_writer
                .create_both(&m, &start_a, &start_b)
                .await
                .is_ok()
        );
    }

    /// Starting with the start of B saves both lineups, B serving as the
    /// other one, and scouts the set.
    #[tokio::test]
    async fn starting_with_the_start_of_b_saves_both_sides() {
        let p = Sqlite::new();
        let (_, team_b, m, start_a, start_b) = on_both_sides(&p).await;
        let side = |s: &SetEntry| crate::set_start::SideInput {
            positions: s.initial_positions,
            setter: s.setter,
            libero: s.libero,
            fallback_libero: None,
        };
        let input = crate::set_start::SetStartInput {
            set_number: 1,
            serving_team: TeamSideEnum::Them,
            positions: start_a.initial_positions,
            setter: start_a.setter,
            libero: start_a.libero,
            fallback_libero: None,
            opponent: Some(side(&start_b)),
        };
        let writer: std::sync::Arc<dyn SetWriter + Send + Sync> =
            std::sync::Arc::new(crate::providers::sqlite::sets::SqliteSetWriter::new(
                p.db.clone(),
                crate::localization::Language::default(),
            ));
        let session = crate::set_start::start(m.clone(), input, writer, Some(team_b))
            .await
            .unwrap();
        assert!(session.is_both());
        let sides = reader(&p).read_sides(&m).unwrap();
        assert!(sides[0].b.is_some());
        let serving =
            p.db.read(|tx| {
                tx.query_row("SELECT serving FROM sets", [], |r| r.get::<_, String>(0))
                    .map_err(sqlite)
            })
            .unwrap();
        assert_eq!(serving, "b");
    }

    /// A session on both sides: the ball decides the side to choose for,
    /// the evaluation of the answer is the one the serve admits, F5 swaps
    /// the side, undo removes the last touch of whichever side, and the
    /// set scouted again from the database is where it was left.
    #[tokio::test]
    async fn a_session_on_both_sides_follows_the_ball() {
        let p = Sqlite::new();
        let (_, team_b, m, start_a, start_b) = on_both_sides(&p).await;
        let side = |s: &SetEntry| crate::set_start::SideInput {
            positions: s.initial_positions,
            setter: s.setter,
            libero: s.libero,
            fallback_libero: None,
        };
        let input = crate::set_start::SetStartInput {
            set_number: 1,
            serving_team: TeamSideEnum::Us,
            positions: start_a.initial_positions,
            setter: start_a.setter,
            libero: start_a.libero,
            fallback_libero: None,
            opponent: Some(side(&start_b)),
        };
        let writer: std::sync::Arc<dyn SetWriter + Send + Sync> =
            std::sync::Arc::new(crate::providers::sqlite::sets::SqliteSetWriter::new(
                p.db.clone(),
                crate::localization::Language::default(),
            ));
        let mut session = crate::set_start::start(
            m.clone(),
            input,
            std::sync::Arc::clone(&writer),
            Some(team_b.clone()),
        )
        .await
        .unwrap();
        // A serves: the serve, and the changes of both sides while the ball is still
        assert_eq!(session.side(), Side::A);
        let choices = session.event_choices();
        assert!(choices.contains(&(Side::A, S)));
        assert!(!choices.contains(&(Side::B, S)));
        assert!(choices.contains(&(Side::B, R)), "{choices:?}");
        session.choose_event_of(Side::A, S).await.unwrap();
        session.choose_eval(Positive).await.unwrap();
        // the ball is on B's side: its reception, the only evaluation the serve admits
        assert_eq!(session.side(), Side::B);
        assert_eq!(session.snapshot().score_us, 0);
        let choices = session.event_choices();
        assert!(choices.contains(&(Side::B, P)));
        assert!(!choices.contains(&(Side::A, P)));
        session.choose_event_of(Side::B, P).await.unwrap();
        assert_eq!(session.available_evals(), vec![Negative]);
        assert!(session.choose_eval(Perfect).await.is_err());
        // F5 swaps the side before anything is chosen
        session.undo().await.unwrap();
        assert_eq!(session.step(), crate::scouting::ScoutingStep::Event);
        assert_eq!(session.swap_side().unwrap(), Side::A);
        assert_eq!(session.swap_side().unwrap(), Side::B);
        session.choose_event_of(Side::B, P).await.unwrap();
        let player = session.player_choices()[0].id;
        session.choose_player(player).await.unwrap();
        session.choose_eval(Negative).await.unwrap();
        assert_eq!(session.touches().len(), 2);
        // undo removes B's touch and goes back to its evaluation
        session.undo().await.unwrap();
        assert_eq!(session.touches().len(), 1);
        assert_eq!(session.side(), Side::B);
        assert_eq!(session.step(), crate::scouting::ScoutingStep::Eval);
        session.choose_eval(Negative).await.unwrap();
        // scouted again from the database
        let touches = reader(&p).read_touches(&m, 1).unwrap().unwrap();
        assert_eq!(touches.touches.len(), 2);
        let again = crate::set_start::resume_both(
            m.clone(),
            session.set().clone(),
            touches,
            team_b,
            writer,
        )
        .unwrap();
        assert_eq!(again.touches().len(), 2);
        assert_eq!(again.side(), session.side());
        assert_eq!(again.event_choices(), session.event_choices());
    }

    /// Each side has its inverted P1, and B's statistics start from its
    /// earlier sets: the inverted P1 of B goes on from its last set.
    #[tokio::test]
    async fn each_side_has_its_inverted_p1_and_its_earlier_stats() {
        let p = Sqlite::new();
        let (_, team_b, m, start_a, start_b) = on_both_sides(&p).await;
        let input = crate::set_start::SetStartInput {
            set_number: 1,
            serving_team: TeamSideEnum::Us,
            positions: start_a.initial_positions,
            setter: start_a.setter,
            libero: start_a.libero,
            fallback_libero: None,
            opponent: Some(crate::set_start::SideInput {
                positions: start_b.initial_positions,
                setter: start_b.setter,
                libero: start_b.libero,
                fallback_libero: None,
            }),
        };
        let writer: std::sync::Arc<dyn SetWriter + Send + Sync> =
            std::sync::Arc::new(crate::providers::sqlite::sets::SqliteSetWriter::new(
                p.db.clone(),
                crate::localization::Language::default(),
            ));
        let session = crate::set_start::start(m.clone(), input, writer, Some(team_b.clone()))
            .await
            .unwrap();
        assert!(!session.inverted_p1_of(Side::B));
        // B's last set (numbered before this one) was inverted and had a reception
        let mut earlier = start_b.clone();
        earlier.set_number = 0;
        let (_, mut reception) = touch(Side::B, P, libero(&team_b), Negative);
        reception.flags = reception.flags.with_inverted_p1(true);
        earlier.events = vec![reception];
        let mut session = session.with_earlier_of_b(&[earlier]);
        assert!(session.inverted_p1_of(Side::B));
        assert!(!session.inverted_p1_of(Side::A));
        // changing the active side's flag leaves the other one
        session.set_inverted_p1(true);
        assert!(session.inverted_p1_of(Side::A));
        assert!(session.inverted_p1_of(Side::B));
        session.set_inverted_p1(false);
        assert!(!session.inverted_p1_of(Side::A));
        assert!(session.inverted_p1_of(Side::B));
        // the earlier reception counts in B's statistics, not in A's
        let b_stats = session.match_stats_of(Side::B).unwrap();
        let a_stats = session.match_stats_of(Side::A).unwrap();
        assert_ne!(format!("{b_stats:?}"), format!("{a_stats:?}"));
    }

    /// Saved on both sides: B with its side, A read as its stream (what
    /// B's touches tell it), each event of A by the row it comes from;
    /// saved again, each touch keeps its row.
    #[tokio::test]
    async fn a_set_on_both_sides_is_saved_with_its_touches() {
        let p = Sqlite::new();
        let (a, b, m, start_a, start_b) = on_both_sides(&p).await;
        let touches = rallies(&a, &b);
        p.set_writer
            .update_sides(&m, &start_a, Some(&start_b), &touches)
            .await
            .unwrap();
        let sides = reader(&p).read_sides(&m).unwrap();
        let scores: Vec<_> = sides
            .iter()
            .map(|s| {
                (
                    (s.a.score_us, s.a.score_them),
                    s.b.as_ref().map(|b| (b.score_us, b.score_them)),
                )
            })
            .collect();
        assert_eq!(scores, vec![((1, 1), Some((1, 1)))]);
        let read = p.match_reader.read_single(&a, &m.id).await.unwrap();
        let stream: Vec<EventEntry> = project(&touches).a.into_iter().map(|p| p.entry).collect();
        let types: Vec<_> = stream.iter().map(|e| e.event_type).collect();
        assert_eq!(types, vec![S, OS, OE]);
        assert_eq!(read.sets[0].events, stream);
        let saved = ids(&p);
        let events = EventsOfA::of(
            &p.db
                .read(|tx| set_rows(tx, Uuid::parse_str(&m.id).unwrap()))
                .unwrap(),
        );
        let of_a: Vec<_> = (0..3)
            .map(|index| {
                events
                    .id(EventRef {
                        set_number: 1,
                        index,
                    })
                    .map(|id| id.to_string())
            })
            .collect();
        // B's kill tells A's point, B's serve error A's
        let told = [&saved[0], &saved[2], &saved[3]].map(|id| Some(id.clone()));
        assert_eq!(of_a, told);
        p.set_writer
            .update_sides(&m, &start_a, Some(&start_b), &touches)
            .await
            .unwrap();
        assert_eq!(ids(&p), saved);
    }

    /// Touches that don't replay on both sides: an error, nothing saved.
    #[tokio::test]
    async fn touches_not_replaying_are_not_saved() {
        let p = Sqlite::new();
        let (_, b, m, start_a, start_b) = on_both_sides(&p).await;
        let touches = [touch(Side::B, A, b.players[1].id, Perfect)];
        let saved = p
            .set_writer
            .update_sides(&m, &start_a, Some(&start_b), &touches)
            .await;
        assert!(saved.is_err(), "{saved:?}");
        assert!(ids(&p).is_empty());
        let without_b = p
            .set_writer
            .update_sides(&m, &start_a, None, &touches)
            .await;
        assert!(without_b.is_err(), "{without_b:?}");
    }

    /// The events of A alone would spoil a set on both sides: changing,
    /// appending or undoing them is an error.
    #[tokio::test]
    async fn the_events_of_a_alone_dont_change_a_set_on_both_sides() {
        let p = Sqlite::new();
        let (a, b, m, start_a, start_b) = on_both_sides(&p).await;
        let touches = rallies(&a, &b);
        p.set_writer
            .update_sides(&m, &start_a, Some(&start_b), &touches)
            .await
            .unwrap();
        let read = p.match_reader.read_single(&a, &m.id).await.unwrap();
        let set = &read.sets[0];
        assert!(p.set_writer.update(&m, set).await.is_err());
        assert!(
            p.set_writer
                .append_event(&m, 1, &touches[0].1)
                .await
                .is_err()
        );
        assert!(p.set_writer.remove_last_event(&m, 1).await.is_err());
        assert_eq!(ids(&p).len(), touches.len());
    }

    /// Scouting only A from now on: B has no side, its touches go, A
    /// saved again with its own stream.
    #[tokio::test]
    async fn scouting_only_a_drops_side_b() {
        let p = Sqlite::new();
        let (a, b, m, start_a, start_b) = on_both_sides(&p).await;
        p.set_writer
            .update_sides(&m, &start_a, Some(&start_b), &rallies(&a, &b))
            .await
            .unwrap();
        p.match_writer
            .save_scouting(&m, ScoutingMode::A)
            .await
            .unwrap();
        assert_eq!(ids(&p).len(), 1);
        let alone = [touch(Side::A, S, a.players[0].id, Perfect)];
        p.set_writer
            .update_sides(&m, &start_a, None, &alone)
            .await
            .unwrap();
        let sides = reader(&p).read_sides(&m).unwrap();
        let scores: Vec<_> = sides
            .iter()
            .map(|s| ((s.a.score_us, s.a.score_them), s.b.is_none()))
            .collect();
        assert_eq!(scores, vec![((1, 0), true)]);
    }

    fn numbers(t: &TeamEntry, except: &[usize]) -> Vec<crate::match_form::RosterNumber> {
        t.players
            .iter()
            .enumerate()
            .filter(|(i, _)| !except.contains(i))
            .map(|(_, pl)| crate::match_form::RosterNumber {
                player: pl.id,
                number: pl.number,
                role: pl.role,
            })
            .collect()
    }

    async fn roster_of_b(p: &Sqlite, m: &MatchEntry) -> Vec<Uuid> {
        let id = Uuid::parse_str(&m.id).unwrap();
        p.db.read(|tx| {
            let mut st = tx
                .prepare("SELECT person_id FROM match_roster WHERE match_id = ?1 AND team_id = (SELECT team_b FROM matches WHERE id = ?1)")
                .map_err(sqlite)?;
            let ids = st
                .query_map(params![Sql(id)], |r| r.get::<_, Sql<Uuid>>(0).map(|s| s.0))
                .map_err(sqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sqlite)?;
            Ok(ids)
        })
        .unwrap()
    }

    /// The roster of B can be written again while no set is on both
    /// sides, and whoever B put on court stays on it.
    #[tokio::test]
    async fn a_player_of_b_in_a_lineup_stays_on_the_roster() {
        let p = Sqlite::new();
        let (a, b, m) = set_one(&p, true).await;
        // all of them, then without the libero of B: it is in the lineup
        p.match_writer
            .save_roster_of_b(&m, &numbers(&b, &[]))
            .await
            .unwrap();
        for out in [0usize, 3, 6] {
            let refused = p
                .match_writer
                .save_roster_of_b(&m, &numbers(&b, &[out]))
                .await;
            assert!(refused.is_err(), "player {out} is on court or libero");
            // nothing changed
            assert_eq!(roster_of_b(&p, &m).await.len(), b.players.len());
        }
        // A's roster isn't B's: the check is of B's side only
        let _ = a;
    }

    /// Whoever touched the ball for B without being in the lineup (the
    /// bench) stays on the roster too; an undone touch holds no one.
    #[tokio::test]
    async fn a_player_of_b_in_a_touch_stays_on_the_roster() {
        let p = Sqlite::new();
        let (_, b, m) = set_one(&p, true).await;
        let mut b = b;
        b.players.push(PlayerEntry {
            id: Uuid::new_v4(),
            name: "Bench".into(),
            number: 8,
            role: Some(RoleEnum::OutsideHitter),
            ..PlayerEntry::default()
        });
        let b = p.team_writer.save(TeamInput::Existing(b)).await.unwrap();
        let bench = b.players[7].id;
        let all = numbers(&b, &[]);
        let without_bench = numbers(&b, &[7]);
        p.match_writer.save_roster_of_b(&m, &all).await.unwrap();
        // nobody touched: the bench can go
        p.match_writer
            .save_roster_of_b(&m, &without_bench)
            .await
            .unwrap();
        p.match_writer.save_roster_of_b(&m, &all).await.unwrap();
        let match_id = Uuid::parse_str(&m.id).unwrap();
        let touch = |deleted: bool, target: bool| Event {
            id: Uuid::new_v4(),
            match_id,
            set_number: 1,
            side: Side::B,
            seq: 0,
            entry: EventEntry {
                timestamp: Utc.with_ymd_and_hms(2026, 11, 14, 17, 5, 0).unwrap(),
                event_type: P,
                player: Some(match target {
                    true => b.players[1].id,
                    false => bench,
                }),
                eval: Some(Negative),
                target_player: target.then_some(bench),
                technique: None,
                flags: EventFlags::default(),
            },
            deleted,
        };
        let write = |e: Event| {
            p.db.write(|tx| {
                insert_event(
                    tx,
                    &Event {
                        seq: next_seq(tx, match_id, 1)?,
                        ..e
                    },
                )
            })
            .unwrap()
        };
        // an undone touch of the bench holds no one
        write(touch(true, false));
        p.match_writer
            .save_roster_of_b(&m, &without_bench)
            .await
            .unwrap();
        p.match_writer.save_roster_of_b(&m, &all).await.unwrap();
        // a live touch does, as the player or as the target
        for target in [false, true] {
            let id = Uuid::new_v4();
            write(Event {
                id,
                ..touch(false, target)
            });
            let refused = p.match_writer.save_roster_of_b(&m, &without_bench).await;
            assert!(refused.is_err(), "touch (target: {target})");
            assert_eq!(roster_of_b(&p, &m).await.len(), 8);
            p.db.write(|tx| {
                tx.execute(
                    "UPDATE events SET deleted = 1 WHERE id = ?1",
                    params![Sql(id)],
                )
                .map(|_| ())
                .map_err(sqlite)
            })
            .unwrap();
        }
        // all of them undone again: the bench can go
        p.match_writer
            .save_roster_of_b(&m, &without_bench)
            .await
            .unwrap();
        assert_eq!(roster_of_b(&p, &m).await.len(), 7);
    }

    /// The form of the roster of B, on the match as B sees it, keeps who
    /// B put on court on the score sheet: it can't take them off, nor
    /// save a roster without them.
    #[tokio::test]
    async fn the_form_of_b_keeps_who_played_on_the_roster() {
        use crate::match_form::{MatchForm, MatchFormError};
        let p = Sqlite::new();
        let (a, b, m) = set_one(&p, true).await;
        let mut b = b;
        b.players.push(PlayerEntry {
            id: Uuid::new_v4(),
            name: "Bench".into(),
            number: 8,
            role: Some(RoleEnum::OutsideHitter),
            ..PlayerEntry::default()
        });
        let b = p.team_writer.save(TeamInput::Existing(b)).await.unwrap();
        p.match_writer
            .save_roster_of_b(&m, &numbers(&b, &[]))
            .await
            .unwrap();
        let of_b = p.match_reader.read_of_b(&a, &m.id).unwrap().unwrap();
        assert_eq!(of_b.played().len(), 7, "the lineup and the libero of B");
        let ids = |except: &[usize]| -> Vec<Uuid> {
            b.players
                .iter()
                .enumerate()
                .filter(|(i, _)| !except.contains(i))
                .map(|(_, pl)| pl.id)
                .collect()
        };
        // toggled, who played stays and the bench goes
        let mut form = MatchForm::default();
        form.toggle(b.players[0].id, &of_b.team, Some(&of_b));
        assert_eq!(form, MatchForm::default());
        form.toggle(b.players[6].id, &of_b.team, Some(&of_b));
        assert_eq!(form, MatchForm::default());
        form.toggle(b.players[7].id, &of_b.team, Some(&of_b));
        assert_eq!(form.roster, Some(ids(&[7])));
        // a roster without them, as no form makes it, is refused
        for out in [0usize, 4, 6] {
            let without = MatchForm {
                roster: Some(ids(&[out])),
                ..MatchForm::default()
            };
            assert_eq!(
                without.roster_of_b_numbers(&of_b.team, Some(&of_b)),
                Err(MatchFormError::RosterWithoutPlayed),
                "player {out}"
            );
        }
        // without the bench it's fine
        assert_eq!(
            form.roster_of_b_numbers(&of_b.team, Some(&of_b))
                .map(|n| n.len()),
            Ok(7)
        );
    }

    /// The roster of B starts from the last match against it that has one
    /// (by date), skipping those scouting only A; with none, nothing.
    #[tokio::test]
    async fn the_roster_of_b_starts_from_the_last_one_written() {
        use crate::match_form::{MatchForm, RosterNumber};
        let p = Sqlite::new();
        let (a, b) = (team(&p, "Cortona").await, team(&p, "Cascina").await);
        let day = |d: i64| date() + chrono::Duration::days(d);
        let create = |d: i64| {
            let (a, b, p) = (a.clone(), b.clone(), &p);
            async move {
                p.match_writer
                    .create(
                        &a,
                        MatchInput {
                            opponent: Opponent::Team(b.id),
                            ..MatchInput::against(
                                "",
                                day(d),
                                true,
                                DEFAULT_MAX_SUBSTITUTIONS,
                                "",
                                vec![],
                            )
                        },
                    )
                    .await
                    .unwrap()
            }
        };
        // the matches, not in the order of their dates
        let (third, first, second) = (create(3).await, create(1).await, create(2).await);
        let previous = |m: &MatchEntry| {
            p.match_reader
                .previous_roster_of_b(&a, &m.id)
                .unwrap()
                .map(|mut n| {
                    n.sort_by_key(|n| n.player);
                    n
                })
        };
        let sorted = |mut n: Vec<RosterNumber>| {
            n.sort_by_key(|n| n.player);
            Some(n)
        };
        assert_eq!(previous(&first), None);
        assert_eq!(previous(&third), None, "none has a roster of B yet");
        // the first has one, with a number changed; the second scouts only A
        p.match_writer
            .save_scouting(&first, ScoutingMode::Both)
            .await
            .unwrap();
        let mut written = numbers(&b, &[0, 6]);
        written[0].number = 40;
        p.match_writer
            .save_roster_of_b(&first, &written)
            .await
            .unwrap();
        assert_eq!(previous(&first), None, "before itself");
        assert_eq!(previous(&second), sorted(written.clone()));
        assert_eq!(
            previous(&third),
            sorted(written.clone()),
            "skips the second"
        );
        // a closer one with another roster wins
        p.match_writer
            .save_scouting(&second, ScoutingMode::Both)
            .await
            .unwrap();
        let closer = numbers(&b, &[1, 2, 3]);
        p.match_writer
            .save_roster_of_b(&second, &closer)
            .await
            .unwrap();
        assert_eq!(previous(&third), sorted(closer.clone()));
        // the form starts from it: only those chosen, with its numbers
        let of_b = {
            p.match_writer
                .save_scouting(&third, ScoutingMode::Both)
                .await
                .unwrap();
            p.match_reader.read_of_b(&a, &third.id).unwrap().unwrap()
        };
        let mut form = MatchForm::default();
        form.start_from(&written, &of_b.team, Some(&of_b));
        let chosen: Vec<RosterNumber> = form
            .roster_choices(&of_b.team, Some(&of_b))
            .iter()
            .filter(|c| c.chosen)
            .map(|c| RosterNumber {
                player: c.player.id,
                number: c.player.number,
                role: c.player.role,
            })
            .collect();
        let mut expected = written.clone();
        expected.sort_by_key(|n| n.number);
        assert_eq!(chosen, expected);
        // written once: a roster already in the form stays as it is
        let before = form.clone();
        form.start_from(&closer, &of_b.team, Some(&of_b));
        assert_eq!(form, before);
        // nobody known: the active players stay
        let mut none = MatchForm::default();
        none.start_from(&[], &of_b.team, Some(&of_b));
        assert_eq!(none, MatchForm::default());
    }
}
