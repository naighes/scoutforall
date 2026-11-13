//! A match between two teams is in the lists of both (TODO/001, step 21):
//! B sees it from its side (A the opponent, its court, the sets the other
//! way), scouted by A only; its statistics are only of the matches it's
//! A of.

#[cfg(test)]
mod tests {
    use crate::{
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        domain::{Home, sql::Sql},
        localization::Language,
        match_form::{MatchInput, Opponent},
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            set_writer::SetWriter,
            sqlite::{
                Database,
                matches::{SqliteMatchReader, SqliteMatchWriter},
                sets::SqliteSetWriter,
                sqlite,
                teams::{SqliteTeamReader, SqliteTeamWriter},
            },
            team_reader::TeamReader,
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{
            enums::{EvalEnum, EventTypeEnum, RoleEnum, TeamSideEnum},
            listed::Listed,
            r#match::{MatchEntry, PlayedMatch},
            player::PlayerEntry,
            snapshot::EventEntry,
            team::TeamEntry,
            technique::{ServeTypeEnum, TechniqueEnum},
        },
    };
    use chrono::{DateTime, Duration, FixedOffset, Utc};
    use rusqlite::params;
    use uuid::Uuid;

    struct P {
        db: Database,
        teams: SqliteTeamReader,
        team_writer: SqliteTeamWriter,
        matches: SqliteMatchReader,
        match_writer: SqliteMatchWriter,
        set_writer: SqliteSetWriter,
    }

    fn providers() -> P {
        let db = Database::new(crate::db::open_in_memory().unwrap());
        let language = Language::default();
        P {
            teams: SqliteTeamReader::new(db.clone()),
            team_writer: SqliteTeamWriter::new(db.clone()),
            matches: SqliteMatchReader::new(db.clone(), language.clone()),
            match_writer: SqliteMatchWriter::new(db.clone()),
            set_writer: SqliteSetWriter::new(db.clone(), language),
            db,
        }
    }

    fn players(team: &str) -> Vec<PlayerEntry> {
        use RoleEnum::*;
        [
            Setter,
            OutsideHitter,
            MiddleBlocker,
            OppositeHitter,
            OutsideHitter,
            MiddleBlocker,
            Libero,
        ]
        .into_iter()
        .zip(1..)
        .map(|(role, n)| PlayerEntry {
            id: Uuid::new_v4(),
            name: format!("{team} {n}"),
            role: Some(role),
            number: n,
            ..PlayerEntry::default()
        })
        .collect()
    }

    async fn team(p: &P, name: &str) -> TeamEntry {
        p.team_writer
            .save(TeamInput::New {
                id: None,
                name: name.into(),
                year: 2026,
                classification: None,
                gender: None,
                country: None,
                players: players(name),
            })
            .await
            .unwrap()
    }

    fn day(n: i64) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-18T18:00:00+02:00").unwrap_or_default()
            + Duration::days(n)
    }

    fn serve(player: Uuid) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type: EventTypeEnum::S,
            player: Some(player),
            eval: Some(EvalEnum::Positive),
            target_player: None,
            technique: Some(TechniqueEnum::Serve(ServeTypeEnum::Float)),
            flags: Default::default(),
        }
    }

    fn their_error() -> EventEntry {
        EventEntry {
            event_type: EventTypeEnum::OE,
            player: None,
            eval: None,
            technique: None,
            ..serve(Uuid::nil())
        }
    }

    /// A match of `a` on day `n` against `opponent`, on `home`: set 1 won
    /// 25–0 by A (B misses every serve of A), set 2 going on.
    async fn played(p: &P, a: &TeamEntry, opponent: Opponent, n: i64, home: Home) -> MatchEntry {
        let m = p
            .match_writer
            .create(
                a,
                MatchInput {
                    opponent,
                    home,
                    ..MatchInput::against("", day(n), true, DEFAULT_MAX_SUBSTITUTIONS, "", vec![])
                },
            )
            .await
            .unwrap();
        let ids: Vec<Uuid> = a.players.iter().map(|p| p.id).collect();
        let lineup: [Uuid; 6] = ids[..6].try_into().unwrap();
        let won: Vec<EventEntry> = (0..25)
            .flat_map(|_| [serve(ids[0]), their_error()])
            .collect();
        for (number, events) in [(1, won), (2, vec![serve(ids[0])])] {
            p.set_writer
                .create(
                    &m,
                    number,
                    TeamSideEnum::Us,
                    lineup,
                    ids[6],
                    None,
                    ids[0],
                    events,
                )
                .await
                .unwrap();
        }
        m
    }

    fn summaries(listed: Vec<Listed<PlayedMatch>>) -> Vec<PlayedMatch> {
        listed.into_iter().map(Result::unwrap).collect()
    }

    #[tokio::test]
    async fn a_match_between_two_teams_is_listed_by_both() {
        let p = providers();
        let (a, b) = (team(&p, "Cortona").await, team(&p, "Cascina").await);
        let m = played(&p, &a, Opponent::Team(b.id), 0, Home::A).await;
        let [of_a] = summaries(p.matches.played(&a).await.unwrap())
            .try_into()
            .unwrap();
        assert_eq!(of_a.scouted_by, None);
        assert_eq!(
            (
                of_a.summary.id.as_str(),
                of_a.summary.opponent.as_str(),
                of_a.summary.home,
                of_a.summary.us_wins,
                of_a.summary.them_wins,
                of_a.summary.resume,
            ),
            (m.id.as_str(), "Cascina", true, 1, 0, Some(2))
        );
        let [of_b] = summaries(p.matches.played(&b).await.unwrap())
            .try_into()
            .unwrap();
        assert_eq!(of_b.scouted_by, Some(a.id));
        assert_eq!(
            (
                of_b.summary.id.as_str(),
                of_b.summary.opponent.as_str(),
                of_b.summary.home,
                of_b.summary.us_wins,
                of_b.summary.them_wins,
                of_b.summary.finished,
                of_b.summary.resume,
                of_b.summary.next_set_number,
                of_b.summary.date,
                of_b.summary.sets,
            ),
            (
                m.id.as_str(),
                "Cortona",
                false,
                0,
                1,
                false,
                None,
                None,
                of_a.summary.date,
                2
            )
        );
        // its statistics: only the matches it's A of
        assert!(p.matches.read_summaries(&b).await.unwrap().is_empty());
        assert!(p.matches.read_all(&b).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn b_sees_the_court_from_its_side() {
        let p = providers();
        let (a, b) = (team(&p, "Cortona").await, team(&p, "Cascina").await);
        for (n, home) in [(0, Home::A), (1, Home::B), (2, Home::Neutral)] {
            played(&p, &a, Opponent::Team(b.id), n, home).await;
        }
        let home = |listed: Vec<PlayedMatch>| -> Vec<bool> {
            listed.iter().map(|m| m.summary.home).collect()
        };
        // the most recent first
        assert_eq!(
            home(summaries(p.matches.played(&a).await.unwrap())),
            [false, false, true]
        );
        assert_eq!(
            home(summaries(p.matches.played(&b).await.unwrap())),
            [false, true, false]
        );
    }

    #[tokio::test]
    async fn the_list_of_a_team_has_both_sides_by_date() {
        let p = providers();
        let (a, b, c) = (
            team(&p, "Cortona").await,
            team(&p, "Cascina").await,
            team(&p, "Arezzo").await,
        );
        let first = played(&p, &b, Opponent::Name("Pisa".into()), 0, Home::A).await;
        let second = played(&p, &a, Opponent::Team(b.id), 1, Home::A).await;
        let third = played(&p, &b, Opponent::Team(c.id), 2, Home::A).await;
        // another match of A: not of B
        played(&p, &a, Opponent::Team(c.id), 3, Home::A).await;
        let listed = summaries(p.matches.played(&b).await.unwrap());
        assert_eq!(
            listed
                .iter()
                .map(|m| (m.summary.id.clone(), m.scouted_by))
                .collect::<Vec<_>>(),
            [(third.id, None), (second.id, Some(a.id)), (first.id, None)]
        );
        let opponents: Vec<_> = summaries(p.matches.played(&c).await.unwrap())
            .into_iter()
            .map(|m| m.summary.opponent)
            .collect();
        assert_eq!(opponents, ["Cortona", "Cascina"]);
    }

    #[tokio::test]
    async fn b_reads_the_name_of_a_as_it_is_now() {
        let p = providers();
        let (a, b) = (team(&p, "Cortona").await, team(&p, "Cascina").await);
        played(&p, &a, Opponent::Team(b.id), 0, Home::A).await;
        // the cache of the list written
        summaries(p.matches.played(&b).await.unwrap());
        p.team_writer
            .save(TeamInput::Existing(TeamEntry {
                name: "Cortona Volley".into(),
                ..a.clone()
            }))
            .await
            .unwrap();
        let [of_b] = summaries(p.matches.played(&b).await.unwrap())
            .try_into()
            .unwrap();
        assert_eq!(of_b.summary.opponent, "Cortona Volley");
        assert_eq!((of_b.summary.us_wins, of_b.summary.them_wins), (0, 1));
    }

    #[tokio::test]
    async fn a_match_that_doesnt_replay_is_unreadable_for_b_too() {
        let p = providers();
        let (a, b) = (team(&p, "Cortona").await, team(&p, "Cascina").await);
        let m = played(&p, &a, Opponent::Team(b.id), 0, Home::A).await;
        p.db.write(|tx| {
            tx.execute(
                "UPDATE events SET event_type = 'P' WHERE match_id = ?1 AND set_number = 1
                 AND seq = 1",
                [&m.id],
            )
            .map(|_| ())
            .map_err(sqlite)
        })
        .unwrap();
        p.db.write(|tx| {
            tx.execute("DELETE FROM match_summary", [])
                .map(|_| ())
                .map_err(sqlite)
        })
        .unwrap();
        let listed = p.matches.played(&b).await.unwrap();
        assert!(matches!(listed.as_slice(), [Err(u)] if u.id == m.id));
    }

    #[tokio::test]
    async fn only_b_plays_the_match_as_b() {
        let p = providers();
        let (a, b, c) = (
            team(&p, "Cortona").await,
            team(&p, "Cascina").await,
            team(&p, "Arezzo").await,
        );
        let m = played(&p, &a, Opponent::Team(b.id), 0, Home::B).await;
        let (team_a, home) = p.matches.played_as_b(&b, &m.id).unwrap().unwrap();
        assert_eq!(
            (team_a.id, team_a.name.as_str(), home),
            (a.id, "Cortona", true)
        );
        assert!(p.matches.played_as_b(&a, &m.id).unwrap().is_none());
        assert!(p.matches.played_as_b(&c, &m.id).unwrap().is_none());
        assert!(p.matches.played_as_b(&b, "not a match").unwrap().is_none());
        p.match_writer.remove(&b.id, &m.id).await.unwrap();
        assert!(p.matches.played_as_b(&b, &m.id).unwrap().is_none());
        assert!(p.matches.played(&b).await.unwrap().is_empty());
        assert!(p.matches.played(&a).await.unwrap().is_empty());
    }

    /// A player of B left out of its roster after the set, with both
    /// teams scouted: B's lineup still tells of it, so it stays in B,
    /// removed.
    #[tokio::test]
    async fn a_player_in_a_lineup_of_b_stays_removed() {
        let p = providers();
        let (a, mut b) = (team(&p, "Cortona").await, team(&p, "Cascina").await);
        let m = played(&p, &a, Opponent::Team(b.id), 0, Home::A).await;
        let ids: Vec<Uuid> = b.players.iter().map(|p| p.id).collect();
        p.db.write(|tx| {
            let run = |r: rusqlite::Result<usize>| r.map(|_| ()).map_err(sqlite);
            run(tx.execute(
                "UPDATE matches SET scouting = 'both' WHERE id = ?1",
                [&m.id],
            ))?;
            b.players[1..].iter().try_for_each(|player| {
                run(tx.execute(
                    "INSERT INTO match_roster (match_id, team_id, person_id, number, role)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![m.id, Sql(b.id), Sql(player.id), player.number, player.role],
                ))
            })?;
            run(tx.execute(
                "INSERT INTO set_lineups (match_id, set_number, side, p1, p2, p3, p4, p5, p6,
                     libero, fallback_libero, setter)
                 VALUES (?1, 1, 'b', ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?2)",
                params![
                    m.id,
                    Sql(ids[0]),
                    Sql(ids[1]),
                    Sql(ids[2]),
                    Sql(ids[3]),
                    Sql(ids[4]),
                    Sql(ids[5]),
                    Sql(ids[6])
                ],
            ))
        })
        .unwrap();
        p.team_writer.remove_player(ids[0], &mut b).await.unwrap();
        let read = p.teams.read_single(&b.id).await.unwrap();
        let kept = read.players.iter().find(|player| player.id == ids[0]);
        assert!(kept.is_some_and(|player| player.deleted));
    }
}
