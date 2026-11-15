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
                EvalEnum, EventTypeEnum, GenderEnum, RoleEnum, TeamClassificationEnum, TeamSideEnum,
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
}
