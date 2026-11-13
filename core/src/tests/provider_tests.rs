//! The providers over the database (TODO/001, step 7): what the apps read
//! and write, and what only the database does (nothing removed, persons
//! apart from the teams, places of the events, numbers of the roster).

#[cfg(test)]
mod tests {
    use crate::{
        analytics::queue::UploadQueue,
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        localization::Language,
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            queue_reader::QueueReader,
            queue_writer::QueueWriter,
            set_reader::SetReader,
            set_writer::SetWriter,
            sqlite::{
                Database,
                archive::{self, Subject},
                matches::{SqliteMatchReader, SqliteMatchWriter},
                queue::SqliteQueue,
                sets::{SqliteSetReader, SqliteSetWriter},
                teams::{SqliteTeamReader, SqliteTeamWriter},
            },
            team_reader::TeamReader,
            team_writer::{PlayerInput, TeamInput, TeamWriter},
        },
        shapes::{
            enums::{EvalEnum, EventTypeEnum, RoleEnum, TeamSideEnum},
            r#match::{MatchEntry, MatchSource},
            player::PlayerEntry,
            snapshot::EventEntry,
            team::TeamEntry,
            technique::{ServeTypeEnum, TechniqueEnum},
        },
    };
    use chrono::{Duration, Utc};
    use uuid::Uuid;

    struct Providers {
        team_reader: Box<dyn TeamReader + Send + Sync>,
        team_writer: Box<dyn TeamWriter + Send + Sync>,
        match_reader: Box<dyn MatchReader + Send + Sync>,
        match_writer: Box<dyn MatchWriter + Send + Sync>,
        set_reader: Box<dyn SetReader + Send + Sync>,
        set_writer: Box<dyn SetWriter + Send + Sync>,
        queue_reader: Box<dyn QueueReader + Send + Sync>,
        queue_writer: Box<dyn QueueWriter + Send + Sync>,
    }

    fn database() -> Database {
        Database::new(crate::db::open_in_memory().unwrap())
    }

    fn sqlite(db: &Database) -> Providers {
        let language = Language::default();
        Providers {
            team_reader: Box::new(SqliteTeamReader::new(db.clone())),
            team_writer: Box::new(SqliteTeamWriter::new(db.clone())),
            match_reader: Box::new(SqliteMatchReader::new(db.clone(), language.clone())),
            match_writer: Box::new(SqliteMatchWriter::new(db.clone())),
            set_reader: Box::new(SqliteSetReader::new(db.clone(), language.clone())),
            set_writer: Box::new(SqliteSetWriter::new(db.clone(), language.clone())),
            queue_reader: Box::new(SqliteQueue::new(db.clone())),
            queue_writer: Box::new(SqliteQueue::new(db.clone())),
        }
    }

    fn players() -> Vec<PlayerEntry> {
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
            name: format!("player {n}"),
            role: Some(role),
            number: n,
            deleted: false,
            birth_date: None,
            height_cm: None,
            notes: String::new(),
        })
        .collect()
    }

    async fn team(p: &Providers, players: Vec<PlayerEntry>) -> TeamEntry {
        p.team_writer
            .save(TeamInput::New {
                id: None,
                name: "Volley Club".into(),
                year: 2025,
                classification: None,
                gender: None,
                country: None,
                players,
            })
            .await
            .unwrap()
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
            timestamp: Utc::now(),
            event_type: EventTypeEnum::OE,
            player: None,
            eval: None,
            target_player: None,
            technique: None,
            flags: Default::default(),
        }
    }

    /// A team, and a match with a set: we serve and they miss.
    async fn scouted_match(p: &Providers) -> (TeamEntry, MatchEntry) {
        let team = team(p, players()).await;
        let ids: Vec<Uuid> = team.players.iter().map(|p| p.id).collect();
        let m = p
            .match_writer
            .create(
                &team,
                "Cortona Volley".into(),
                Utc::now().fixed_offset(),
                true,
                DEFAULT_MAX_SUBSTITUTIONS,
                String::new(),
                vec![],
            )
            .await
            .unwrap();
        p.set_writer
            .create(
                &m,
                1,
                TeamSideEnum::Us,
                ids[..6].try_into().unwrap(),
                ids[6],
                None,
                ids[0],
                vec![serve(ids[0]), their_error()],
            )
            .await
            .unwrap();
        let m = p.match_reader.read_single(&team, &m.id).await.unwrap();
        (team, m)
    }

    /// The set as replayed: the events and the score.
    fn summary(m: &MatchEntry) -> Vec<(Vec<EventTypeEnum>, u8, u8)> {
        m.sets
            .iter()
            .map(|set| {
                let snapshot = set.compute_snapshot(m.max_substitutions).unwrap();
                (
                    set.events.iter().map(|e| e.event_type).collect(),
                    snapshot.score_us,
                    snapshot.score_them,
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn a_scouted_match_reads_back() {
        let p = sqlite(&database());
        let (team, m) = scouted_match(&p).await;
        assert_eq!(
            summary(&m),
            vec![(vec![EventTypeEnum::S, EventTypeEnum::OE], 1, 0)],
        );
        let teams = p.team_reader.read_all().await.unwrap();
        assert_eq!(teams.len(), 1);
        assert_eq!(teams[0].as_ref().unwrap(), &team);
        assert!(p.team_reader.exists(&team.id).await.unwrap());
        assert!(!p.team_reader.exists(&Uuid::new_v4()).await.unwrap());
        let matches = p.match_reader.read_all(&team).await.unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].as_ref().unwrap(), &m);
        assert!(p.match_reader.exists(&team, &m.id).await.unwrap());
        assert!(!p.match_reader.exists(&team, "nope").await.unwrap());
        assert_eq!(p.set_reader.read_all(&m).await.unwrap(), m.sets);
        let summaries = p.match_reader.read_summaries(&team).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].as_ref().unwrap().id, m.id);
        let some = p
            .match_reader
            .read_some(&team, &[m.id.clone(), "nope".into()])
            .await;
        assert!(some[0].is_ok() && some[1].is_err());
    }

    #[tokio::test]
    async fn events_are_appended_and_undone() {
        let p = sqlite(&database());
        let (team, m) = scouted_match(&p).await;
        let server = team.players[0].id;
        p.set_writer
            .append_event(&m, 1, &serve(server))
            .await
            .unwrap();
        let m = p.match_reader.read_single(&team, &m.id).await.unwrap();
        assert_eq!(
            summary(&m),
            vec![(
                vec![EventTypeEnum::S, EventTypeEnum::OE, EventTypeEnum::S],
                1,
                0
            )],
        );
        let undone = p.set_writer.remove_last_event(&m, 1).await.unwrap();
        assert_eq!(undone.map(|e| e.event_type), Some(EventTypeEnum::S));
        p.set_writer.remove_last_event(&m, 1).await.unwrap();
        let m = p.match_reader.read_single(&team, &m.id).await.unwrap();
        assert_eq!(summary(&m), vec![(vec![EventTypeEnum::S], 0, 0)]);
        p.set_writer.remove_last_event(&m, 1).await.unwrap();
        assert_eq!(p.set_writer.remove_last_event(&m, 1).await.unwrap(), None,);
    }

    /// A set saved again: its events changed, cut, added; a set removed
    /// and scouted again.
    #[tokio::test]
    async fn sets_are_updated_removed_and_scouted_again() {
        let p = sqlite(&database());
        let (team, m) = scouted_match(&p).await;
        let mut set = m.sets[0].clone();
        set.events[0].eval = Some(EvalEnum::Perfect);
        set.events.truncate(1);
        p.set_writer.update(&m, &set).await.unwrap();
        let read = p.set_reader.read_all(&m).await.unwrap();
        assert_eq!(read, vec![set.clone()]);
        set.events.push(their_error());
        set.events.push(serve(team.players[0].id));
        p.set_writer.update(&m, &set).await.unwrap();
        assert_eq!(p.set_reader.read_all(&m).await.unwrap(), vec![set],);

        p.set_writer.remove(&m, 1).await.unwrap();
        assert!(p.set_reader.read_all(&m).await.unwrap().is_empty(),);
        let ids: Vec<Uuid> = team.players.iter().map(|p| p.id).collect();
        let again = p
            .set_writer
            .create(
                &m,
                1,
                TeamSideEnum::Them,
                ids[..6].try_into().unwrap(),
                ids[6],
                None,
                ids[0],
                vec![],
            )
            .await
            .unwrap();
        assert_eq!(p.set_reader.read_all(&m).await.unwrap(), vec![again],);
        // not twice
        let twice = p.set_writer.create(
            &m,
            1,
            TeamSideEnum::Them,
            ids[..6].try_into().unwrap(),
            ids[6],
            None,
            ids[0],
            vec![],
        );
        assert!(twice.await.is_err());
    }

    /// What the form of the match changes, and where it came from.
    #[tokio::test]
    async fn a_match_is_updated() {
        let p = sqlite(&database());
        let (team, m) = scouted_match(&p).await;
        let date = m.date - Duration::days(3);
        let roster: Vec<Uuid> = team.players[..6].iter().map(|p| p.id).collect();
        p.match_writer
            .update(
                &m,
                "Arezzo".into(),
                date,
                false,
                4,
                "notes".into(),
                roster.clone(),
            )
            .await
            .unwrap();
        let read = p.match_reader.read_single(&team, &m.id).await.unwrap();
        assert_eq!(
            (
                read.opponent.as_str(),
                read.date,
                read.home,
                read.max_substitutions,
                read.notes.as_str()
            ),
            ("Arezzo", date, false, 4, "notes"),
        );
        assert_eq!(read.roster, roster);
        assert_eq!(read.team.active_players().len(), 6);
        assert_eq!(summary(&read), summary(&m));
        let saved = p
            .match_writer
            .save_source(&read, MatchSource::Dvw, true)
            .await
            .unwrap();
        let read = p.match_reader.read_single(&team, &m.id).await.unwrap();
        assert_eq!((read.source, read.video_clock), (MatchSource::Dvw, true));
        assert_eq!(saved, read);
    }

    /// A player no match tells of leaves the team; one a match tells of (in
    /// a lineup, in an event) stays for it, without the personal details,
    /// out of the active players.
    #[tokio::test]
    async fn a_removed_player_leaves_the_team_unless_a_match_tells_of_them() {
        let p = sqlite(&database());
        let (mut team, m) = scouted_match(&p).await;
        // the roster tells of every player: only the lineup and the events
        let roster: Vec<Uuid> = team.players[..6].iter().map(|p| p.id).collect();
        p.match_writer
            .update(
                &m,
                m.opponent.clone(),
                m.date,
                m.home,
                m.max_substitutions,
                String::new(),
                roster,
            )
            .await
            .unwrap();
        let bench = p
            .team_writer
            .save_player(
                PlayerInput::New {
                    name: "bench".into(),
                    role: RoleEnum::Setter,
                    number: 10,
                    details: Default::default(),
                },
                &mut team,
            )
            .await
            .unwrap();
        let mut server = team.players[0].clone();
        server.birth_date = chrono::NaiveDate::from_ymd_opt(2010, 1, 2);
        server.height_cm = Some(180);
        server.notes = "notes".into();
        p.team_writer
            .save_player(PlayerInput::Existing(server.clone()), &mut team)
            .await
            .unwrap();
        assert_eq!(p.team_reader.read_single(&team.id).await.unwrap(), team,);
        // in the lineup only
        let libero = team.players[6].clone();
        for id in [bench.id, server.id, libero.id] {
            p.team_writer.remove_player(id, &mut team).await.unwrap();
        }
        let read = p.team_reader.read_single(&team.id).await.unwrap();
        assert_eq!(read, team);
        assert!(read.find_player(bench.id).is_none());
        let kept = read.find_player(server.id).unwrap();
        assert!(kept.deleted);
        assert_eq!((kept.name.as_str(), kept.number), ("player 1", 1));
        assert_eq!((kept.birth_date, kept.height_cm), (None, None));
        assert!(kept.notes.is_empty());
        assert!(read.find_player(libero.id).unwrap().deleted);
        assert_eq!(read.active_players().len(), 5);
        let loaded = p.match_reader.read_single(&read, &m.id).await.unwrap();
        assert_eq!(summary(&loaded), summary(&m));
    }

    /// A match removed: no longer read, the other matches are; a team
    /// removed: no longer read with its matches, the other teams are.
    #[tokio::test]
    async fn teams_and_matches_are_removed() {
        let p = sqlite(&database());
        let (team, m) = scouted_match(&p).await;
        let (other_team, other_match) = scouted_match(&p).await;
        let second = p
            .match_writer
            .create(
                &team,
                "B".into(),
                Utc::now().fixed_offset(),
                true,
                6,
                String::new(),
                vec![],
            )
            .await
            .unwrap();
        p.match_writer.remove(&team.id, &m.id).await.unwrap();
        let left = p.match_reader.read_all(&team).await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].as_ref().unwrap().id, second.id);
        assert!(!p.match_reader.exists(&team, &m.id).await.unwrap());
        assert!(p.match_reader.read_single(&team, &m.id).await.is_err());
        for id in ["", "x/..", "a/b"] {
            assert!(p.match_writer.remove(&team.id, id).await.is_err(), "{id}");
        }
        p.team_writer.remove(&team.id).await.unwrap();
        let teams = p.team_reader.read_all().await.unwrap();
        assert_eq!(teams.len(), 1);
        let ids: Vec<Uuid> = teams.iter().flatten().map(|t| t.id).collect();
        assert_eq!(ids, vec![other_team.id]);
        assert!(!p.team_reader.exists(&team.id).await.unwrap());
        assert!(p.team_reader.read_single(&team.id).await.is_err());
        assert!(p.match_reader.read_all(&team).await.unwrap().is_empty());
        let left = p.match_reader.read_all(&other_team).await.unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].as_ref().unwrap().id, other_match.id);
    }

    /// The queue reads back in its order, a match that keeps failing last.
    #[tokio::test]
    async fn the_upload_queue_keeps_its_order() {
        let p = sqlite(&database());
        assert!(p.queue_reader.load().await.unwrap().is_empty());
        let (team, first) = scouted_match(&p).await;
        let second = p
            .match_writer
            .create(
                &team,
                "B".into(),
                Utc::now().fixed_offset(),
                true,
                6,
                String::new(),
                vec![],
            )
            .await
            .unwrap();
        let mut queue = UploadQueue::default();
        queue.enqueue(team.id, first.id.clone());
        queue.enqueue(team.id, second.id.clone());
        queue.mark_retry(&team.id, &first.id);
        p.queue_writer.save(&queue).await.unwrap();
        let read = p.queue_reader.load().await.unwrap();
        let pending: Vec<(Uuid, &str, u8)> = read
            .pending()
            .iter()
            .map(|p| (p.team_id, p.match_id.as_str(), p.retry_count))
            .collect();
        assert_eq!(
            pending,
            vec![
                (team.id, second.id.as_str(), 0),
                (team.id, first.id.as_str(), 1)
            ],
        );
        assert_eq!(read.pending()[0].queued_at, queue.pending()[0].queued_at,);
        let mut queue = read;
        queue.dequeue(&team.id, &second.id);
        p.queue_writer.save(&queue).await.unwrap();
        assert_eq!(p.queue_reader.load().await.unwrap().pending().len(), 1);
    }

    // What only the database does.

    fn count(db: &Database, sql: &str) -> i64 {
        db.read(|tx| {
            tx.query_row(sql, [], |row| row.get(0))
                .map_err(crate::providers::sqlite::sqlite)
        })
        .unwrap()
    }

    fn execute(db: &Database, sql: &str) {
        db.write(|tx| {
            tx.execute_batch(sql)
                .map_err(crate::providers::sqlite::sqlite)
        })
        .unwrap()
    }

    /// Nothing is removed (D7): teams, matches, sets, events, members are
    /// marked; the persons stay as they were.
    #[tokio::test]
    async fn the_database_marks_what_is_removed() {
        let db = database();
        let p = sqlite(&db);
        let (mut team, m) = scouted_match(&p).await;
        let bench = p
            .team_writer
            .save_player(
                PlayerInput::New {
                    name: "bench".into(),
                    role: RoleEnum::Setter,
                    number: 10,
                    details: crate::providers::team_writer::PlayerDetails {
                        notes: "notes".into(),
                        ..Default::default()
                    },
                },
                &mut team,
            )
            .await
            .unwrap();
        p.team_writer
            .remove_player(bench.id, &mut team)
            .await
            .unwrap();
        p.set_writer.remove_last_event(&m, 1).await.unwrap();
        p.set_writer.remove(&m, 1).await.unwrap();
        p.match_writer.remove(&team.id, &m.id).await.unwrap();
        p.team_writer.remove(&team.id).await.unwrap();
        // the members of a removed team stay as they were: only the bench
        // player was removed from it
        for (table, rows, kept) in [
            ("teams", 1, 0),
            ("team_members", 8, 7),
            ("matches", 1, 0),
            ("sets", 1, 0),
            ("events", 2, 0),
        ] {
            let not_removed = format!("SELECT count(*) FROM {table} WHERE deleted = 0");
            assert_eq!(
                (
                    count(&db, &format!("SELECT count(*) FROM {table}")),
                    count(&db, &not_removed)
                ),
                (rows, kept),
                "{table}"
            );
        }
        assert_eq!(
            count(
                &db,
                "SELECT count(*) FROM persons WHERE deleted = 0 AND name <> ''"
            ),
            8
        );
        assert_eq!(
            count(&db, "SELECT count(*) FROM persons WHERE notes = 'notes'"),
            1
        );
    }

    /// An undone event keeps its place: the next one goes after it.
    #[tokio::test]
    async fn an_undone_event_keeps_its_place() {
        let db = database();
        let p = sqlite(&db);
        let (team, m) = scouted_match(&p).await;
        p.set_writer.remove_last_event(&m, 1).await.unwrap();
        p.set_writer
            .append_event(&m, 1, &serve(team.players[0].id))
            .await
            .unwrap();
        let places = |deleted: i64| -> String {
            db.read(|tx| {
                tx.query_row(
                    &format!(
                        "SELECT group_concat(seq) FROM (SELECT seq FROM events
                         WHERE deleted = {deleted} ORDER BY seq)"
                    ),
                    [],
                    |row| row.get(0),
                )
                .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap()
        };
        assert_eq!(places(0), "0,2");
        assert_eq!(places(1), "1");
        // the set without events, appended again from the place after
        p.set_writer.remove(&m, 1).await.unwrap();
        assert!(
            p.set_writer
                .append_event(&m, 1, &serve(team.players[0].id))
                .await
                .is_err()
        );
    }

    /// A corrected event keeps its row (and its id, for the video).
    #[tokio::test]
    async fn a_corrected_event_keeps_its_row() {
        let db = database();
        let p = sqlite(&db);
        let (_, m) = scouted_match(&p).await;
        let id = |seq: i64| -> String {
            db.read(|tx| {
                tx.query_row("SELECT id FROM events WHERE seq = ?1", [seq], |row| {
                    row.get(0)
                })
                .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap()
        };
        let before = (id(0), id(1));
        let mut set = m.sets[0].clone();
        set.events[1].timestamp += Duration::seconds(1);
        p.set_writer.update(&m, &set).await.unwrap();
        assert_eq!((id(0), id(1)), before);
        assert_eq!(count(&db, "SELECT count(*) FROM events"), 2);
    }

    /// A person is one in all its teams (D5): the details changed in one
    /// team show in the other; removed from one, the person keeps them.
    #[tokio::test]
    async fn a_person_is_one_in_all_its_teams() {
        let db = database();
        let p = sqlite(&db);
        let mut first = team(&p, players()).await;
        let mut second = team(&p, first.players[..1].to_vec()).await;
        let mut player = first.players[0].clone();
        player.notes = "mancina".into();
        player.height_cm = Some(181);
        p.team_writer
            .save_player(PlayerInput::Existing(player.clone()), &mut first)
            .await
            .unwrap();
        let read = p.team_reader.read_single(&second.id).await.unwrap();
        assert_eq!(read.players[0].notes, "mancina");
        p.team_writer
            .remove_player(player.id, &mut second)
            .await
            .unwrap();
        assert!(second.players.is_empty());
        let read = p.team_reader.read_single(&first.id).await.unwrap();
        assert_eq!(read.players[0], player);
        assert_eq!(
            p.team_reader.read_single(&second.id).await.unwrap().players,
            vec![]
        );
        // a removed player saved with the team doesn't wipe the person
        let (mut team, _) = scouted_match(&p).await;
        let mut server = team.players[0].clone();
        server.notes = "notes".into();
        p.team_writer
            .save_player(PlayerInput::Existing(server.clone()), &mut team)
            .await
            .unwrap();
        p.team_writer
            .remove_player(server.id, &mut team)
            .await
            .unwrap();
        assert!(team.players[0].notes.is_empty());
        p.team_writer
            .save(TeamInput::Existing(team.clone()))
            .await
            .unwrap();
        assert_eq!(
            count(
                &db,
                &format!(
                    "SELECT count(*) FROM persons WHERE id = '{}' AND notes = 'notes'",
                    server.id
                )
            ),
            1
        );
    }

    /// The players keep their order: a new one after the last, a removed
    /// one in its place.
    #[tokio::test]
    async fn the_players_keep_their_order() {
        let db = database();
        let p = sqlite(&db);
        let (mut team, _) = scouted_match(&p).await;
        let order = |team: &TeamEntry| -> Vec<String> {
            team.players.iter().map(|p| p.name.clone()).collect()
        };
        p.team_writer
            .remove_player(team.players[2].id, &mut team)
            .await
            .unwrap();
        p.team_writer
            .save_player(
                PlayerInput::New {
                    name: "new".into(),
                    role: RoleEnum::Setter,
                    number: 10,
                    details: Default::default(),
                },
                &mut team,
            )
            .await
            .unwrap();
        let read = p.team_reader.read_single(&team.id).await.unwrap();
        assert_eq!(order(&read), order(&team));
        assert_eq!(read.players[2].name, "player 3");
        assert!(read.players[2].deleted);
        assert_eq!(read.players[7].name, "new");
        assert_eq!(count(&db, "SELECT max(position) FROM team_members"), 7);
    }

    /// The roster keeps the numbers of the match (D9): a number changed in
    /// the team since doesn't change the match; a player added takes the
    /// number of the team.
    #[tokio::test]
    async fn the_roster_keeps_the_numbers_of_the_match() {
        let db = database();
        let p = sqlite(&db);
        let (mut team, m) = scouted_match(&p).await;
        let roster: Vec<Uuid> = team.players[..6].iter().map(|p| p.id).collect();
        p.match_writer
            .update(
                &m,
                m.opponent.clone(),
                m.date,
                m.home,
                6,
                String::new(),
                roster.clone(),
            )
            .await
            .unwrap();
        for i in [0, 6] {
            let mut player = team.players[i].clone();
            player.number += 20;
            p.team_writer
                .save_player(PlayerInput::Existing(player), &mut team)
                .await
                .unwrap();
        }
        let all: Vec<Uuid> = team.players.iter().map(|p| p.id).collect();
        p.match_writer
            .update(
                &m,
                m.opponent.clone(),
                m.date,
                m.home,
                6,
                String::new(),
                all,
            )
            .await
            .unwrap();
        let read = p.match_reader.read_single(&team, &m.id).await.unwrap();
        let number = |i: usize| read.team.find_player(team.players[i].id).unwrap().number;
        assert_eq!((number(0), number(6)), (1, 27));
        // the team itself has the new numbers
        let team = p.team_reader.read_single(&team.id).await.unwrap();
        assert_eq!((team.players[0].number, team.players[6].number), (21, 27));
    }

    /// A match sees the players of its roster as they were: one removed
    /// from the team since is still active, one added since is not; the
    /// roster goes with the match in an archive.
    #[tokio::test]
    async fn a_match_keeps_its_roster() {
        let db = database();
        let p = sqlite(&db);
        let (mut team, m) = scouted_match(&p).await;
        let roster: Vec<Uuid> = team.players.iter().map(|p| p.id).collect();
        p.match_writer
            .update(
                &m,
                m.opponent.clone(),
                m.date,
                m.home,
                m.max_substitutions,
                m.notes.clone(),
                roster.clone(),
            )
            .await
            .unwrap();
        let first = team.players[0].clone();
        p.team_writer
            .remove_player(first.id, &mut team)
            .await
            .unwrap();
        let added = p
            .team_writer
            .save_player(
                PlayerInput::New {
                    name: "new".into(),
                    role: RoleEnum::Setter,
                    number: 10,
                    details: Default::default(),
                },
                &mut team,
            )
            .await
            .unwrap();
        let active = |m: &MatchEntry| -> Vec<Uuid> {
            m.team.active_players().iter().map(|p| p.id).collect()
        };
        let loaded = p.match_reader.read_single(&team, &m.id).await.unwrap();
        assert_eq!(loaded.roster, roster);
        assert_eq!(active(&loaded), roster);
        assert!(loaded.team.find_player(added.id).is_some());

        // exported and imported elsewhere
        let uuid = Uuid::parse_str(&m.id).unwrap();
        let zip = archive::export(&db, Subject::Match(uuid)).unwrap();
        let target = database();
        archive::import(&target, &zip, &Language::default()).unwrap();
        let target = sqlite(&target);
        let team = target.team_reader.read_single(&team.id).await.unwrap();
        let loaded = target.match_reader.read_single(&team, &m.id).await.unwrap();
        assert_eq!(loaded.roster, roster);
        assert_eq!(active(&loaded), roster);
        assert_eq!(active(&m).len(), 7);
    }

    /// The role of a match (D9) is where the player stands in the lineup of
    /// the set, not the role in the team nor in the roster: an outside
    /// hitter of the team playing opposite attacks as an opposite (from 2
    /// in break), the opposite of the team in their place from 4. Not
    /// something to add in step 9: `Snapshot` never sees the team.
    #[tokio::test]
    async fn the_role_of_a_match_is_its_place_in_the_lineup() {
        use crate::{
            analysis::match_stats,
            shapes::{enums::ZoneEnum, stats::Touch},
        };
        let db = database();
        let p = sqlite(&db);
        let team = team(&p, players()).await;
        let ids: Vec<Uuid> = team.players.iter().map(|p| p.id).collect();
        let (setter, oh1, mb2, opposite, oh2, mb1, libero) =
            (ids[0], ids[1], ids[2], ids[3], ids[4], ids[5], ids[6]);
        let act = |event_type, player, eval| EventEntry {
            timestamp: Utc::now(),
            event_type,
            player: Some(player),
            eval: Some(eval),
            target_player: None,
            technique: None,
            flags: Default::default(),
        };
        // rotation 0, break: the attacks of the two hitters
        let scouted = |positions: [Uuid; 6]| {
            let p = &p;
            let team = &team;
            async move {
                let m = p
                    .match_writer
                    .create(
                        team,
                        "Cortona Volley".into(),
                        Utc::now().fixed_offset(),
                        true,
                        DEFAULT_MAX_SUBSTITUTIONS,
                        String::new(),
                        vec![],
                    )
                    .await
                    .unwrap();
                p.set_writer
                    .create(
                        &m,
                        1,
                        TeamSideEnum::Us,
                        positions,
                        libero,
                        None,
                        setter,
                        vec![
                            act(EventTypeEnum::S, setter, EvalEnum::Negative),
                            act(EventTypeEnum::D, oh2, EvalEnum::Positive),
                            act(EventTypeEnum::A, oh1, EvalEnum::Negative),
                            // their error: our point, the same rotation
                            their_error(),
                            act(EventTypeEnum::S, setter, EvalEnum::Negative),
                            act(EventTypeEnum::D, oh2, EvalEnum::Positive),
                            act(EventTypeEnum::A, opposite, EvalEnum::Negative),
                        ],
                    )
                    .await
                    .unwrap();
                p.match_reader.read_single(team, &m.id).await.unwrap()
            }
        };
        let attack = |m: &MatchEntry, player: Uuid| {
            match_stats(m)
                .unwrap()
                .events
                .iter(|k| k.event_type == EventTypeEnum::A && k.player == Some(player))
                .map(|(k, _)| (k.zone, k.touch.as_ref().and_then(|t: &Touch| t.role)))
                .collect::<Vec<_>>()
        };
        use RoleEnum::*;
        let as_in_the_team = scouted([setter, oh1, mb2, opposite, oh2, mb1]).await;
        assert_eq!(
            attack(&as_in_the_team, oh1),
            vec![(Some(ZoneEnum::Four), Some(OutsideHitter))]
        );
        assert_eq!(
            attack(&as_in_the_team, opposite),
            vec![(Some(ZoneEnum::Two), Some(OppositeHitter))]
        );
        // the two swapped: each attacks as the place they play
        let swapped = scouted([setter, opposite, mb2, oh1, oh2, mb1]).await;
        let role = |player| swapped.team.find_player(player).unwrap().role;
        assert_eq!(
            (role(oh1), role(opposite)),
            (Some(OutsideHitter), Some(OppositeHitter))
        );
        assert_eq!(
            attack(&swapped, oh1),
            vec![(Some(ZoneEnum::Two), Some(OppositeHitter))]
        );
        assert_eq!(
            attack(&swapped, opposite),
            vec![(Some(ZoneEnum::Four), Some(OutsideHitter))]
        );
    }

    /// A neutral court stays neutral when the form says "not at home".
    #[tokio::test]
    async fn a_neutral_court_stays_neutral() {
        let db = database();
        let p = sqlite(&db);
        let (team, m) = scouted_match(&p).await;
        execute(&db, "UPDATE matches SET home = NULL");
        let update = |home: bool| {
            p.match_writer.update(
                &m,
                m.opponent.clone(),
                m.date,
                home,
                6,
                String::new(),
                m.roster.clone(),
            )
        };
        update(false).await.unwrap();
        assert_eq!(
            count(&db, "SELECT count(*) FROM matches WHERE home IS NULL"),
            1
        );
        assert!(!p.match_reader.read_single(&team, &m.id).await.unwrap().home);
        update(true).await.unwrap();
        assert_eq!(
            count(&db, "SELECT count(*) FROM matches WHERE home = 'a'"),
            1
        );
    }

    /// The ids of the matches are UUIDs: two matches against the same
    /// opponent on the same day are two matches.
    #[tokio::test]
    async fn matches_have_ids_of_their_own() {
        let p = sqlite(&database());
        let (team, m) = scouted_match(&p).await;
        let again = p
            .match_writer
            .create(
                &team,
                m.opponent.clone(),
                m.date,
                true,
                6,
                String::new(),
                vec![],
            )
            .await
            .unwrap();
        assert_ne!(again.id, m.id);
        assert!(Uuid::parse_str(&m.id).is_ok());
        assert_eq!(p.match_reader.read_all(&team).await.unwrap().len(), 2);
        // spelled otherwise: not the same id
        let upper = m.id.to_uppercase();
        assert!(p.match_reader.read_single(&team, &upper).await.is_err());
        // a match of another team
        let other = team_for(&p).await;
        assert!(p.match_reader.read_single(&other, &m.id).await.is_err());
        assert!(!p.match_reader.exists(&other, &m.id).await.unwrap());
    }

    async fn team_for(p: &Providers) -> TeamEntry {
        team(p, players()).await
    }

    /// A row the code can't read makes its team or its match unreadable,
    /// not the others.
    #[tokio::test]
    async fn an_unreadable_row_hides_only_its_team_or_match() {
        let db = database();
        let p = sqlite(&db);
        let (team, m) = scouted_match(&p).await;
        let (other, _) = scouted_match(&p).await;
        let second = p
            .match_writer
            .create(&team, "B".into(), m.date, true, 6, String::new(), vec![])
            .await
            .unwrap();
        execute(
            &db,
            &format!(
                "UPDATE events SET eval = '?' WHERE match_id = '{}' AND seq = 0",
                m.id
            ),
        );
        let matches = p.match_reader.read_all(&team).await.unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].as_ref().unwrap().id, second.id);
        assert_eq!(matches[1].as_ref().unwrap_err().id, m.id);
        assert!(p.match_reader.read_single(&team, &m.id).await.is_err());
        // a set that doesn't replay: why
        execute(
            &db,
            &format!(
                "UPDATE events SET eval = '+', player = '{}' WHERE match_id = '{}'
                 AND seq = 0",
                team.players[2].id, m.id
            ),
        );
        let matches = p.match_reader.read_all(&team).await.unwrap();
        assert!(matches[1].as_ref().unwrap_err().reason.is_some());

        // the schema doesn't check the spelling of the country, reading does
        execute(
            &db,
            &format!("UPDATE teams SET country = 'it' WHERE id = '{}'", other.id),
        );
        let teams = p.team_reader.read_all().await.unwrap();
        assert_eq!(teams.len(), 2);
        let unreadable: Vec<&str> = teams
            .iter()
            .filter_map(|t| t.as_ref().err())
            .map(|u| u.id.as_str())
            .collect();
        assert_eq!(unreadable, vec![other.id.to_string()]);
        assert!(teams.iter().flatten().all(|t| t.id == team.id));
        assert!(p.team_reader.read_single(&other.id).await.is_err());
    }

    /// A player added after the match was created, in none of its rosters
    /// nor lineups, told of by an event only: removed, they stay.
    #[tokio::test]
    async fn an_event_alone_keeps_a_removed_player() {
        let p = sqlite(&database());
        let (mut team, m) = scouted_match(&p).await;
        let bench = p
            .team_writer
            .save_player(
                PlayerInput::New {
                    name: "bench".into(),
                    role: RoleEnum::Setter,
                    number: 10,
                    details: Default::default(),
                },
                &mut team,
            )
            .await
            .unwrap();
        let mut touch = serve(team.players[0].id);
        touch.target_player = Some(bench.id);
        p.set_writer.append_event(&m, 1, &touch).await.unwrap();
        p.team_writer
            .remove_player(bench.id, &mut team)
            .await
            .unwrap();
        let read = p.team_reader.read_single(&team.id).await.unwrap();
        assert!(read.find_player(bench.id).unwrap().deleted);
        // the event undone: no longer told of
        p.set_writer.remove_last_event(&m, 1).await.unwrap();
        let read = p.team_reader.read_single(&team.id).await.unwrap();
        assert!(read.find_player(bench.id).is_none());
    }

    /// The sets of a match go from 1 without gaps: a set removed before
    /// the next one makes the match unreadable, not the others.
    #[tokio::test]
    async fn a_gap_in_the_sets_is_unreadable() {
        let p = sqlite(&database());
        let (team, m) = scouted_match(&p).await;
        let ids: Vec<Uuid> = team.players.iter().map(|p| p.id).collect();
        p.set_writer
            .create(
                &m,
                2,
                TeamSideEnum::Them,
                ids[..6].try_into().unwrap(),
                ids[6],
                None,
                ids[0],
                vec![],
            )
            .await
            .unwrap();
        assert_eq!(p.set_reader.read_all(&m).await.unwrap().len(), 2);
        p.set_writer.remove(&m, 1).await.unwrap();
        assert!(p.set_reader.read_all(&m).await.is_err());
        let matches = p.match_reader.read_all(&team).await.unwrap();
        assert_eq!(matches.len(), 1);
        assert!(matches[0].is_err());
    }
}
