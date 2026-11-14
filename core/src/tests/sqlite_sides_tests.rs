//! The two sides of a set read from the database (TODO/001, phase 1): the
//! touches of A and B, one sequence in `events`, projected and replayed on
//! each side; scouting only A, B has no side and A replays as today.
#[cfg(test)]
mod tests {
    use crate::{
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        domain::{Event, Side, sql::Sql},
        localization::Language,
        match_form::{MatchInput, Opponent},
        providers::{
            match_writer::MatchWriter,
            set_writer::SetWriter,
            sqlite::{
                rows::{insert_event, next_seq},
                sets::SqliteSetReader,
                sqlite,
            },
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{
            country::CountryCode,
            enums::{
                EvalEnum, EventTypeEnum, GenderEnum, RoleEnum, TeamClassificationEnum, TeamSideEnum,
            },
            flags::EventFlags,
            r#match::MatchEntry,
            player::PlayerEntry,
            snapshot::EventEntry,
            team::TeamEntry,
        },
        tests::Sqlite,
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
}
