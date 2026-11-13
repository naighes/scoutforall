//! The profile of the video of a match in the database (TODO/001, step
//! 15, `providers::sqlite::video`): saved and read back, and what only
//! the database does (events referred to by their
//! ids, a profile with an event that isn't there refused, replaced as a
//! whole, only for a match of its team A, never exported).

#[cfg(test)]
mod tests {
    use crate::{
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        domain::adapters::{EventsOfA, SetRows, VideoRows},
        legacy::migrate::unzip,
        localization::Language,
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            set_writer::SetWriter,
            sqlite::{
                Database,
                archive::{Subject, export},
                matches::{SqliteMatchReader, SqliteMatchWriter},
                sets::SqliteSetWriter,
                teams::SqliteTeamWriter,
                video::SqliteVideoProfiles,
            },
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{
            enums::{EvalEnum, EventTypeEnum, RoleEnum, TeamSideEnum},
            r#match::MatchEntry,
            player::PlayerEntry,
            snapshot::EventEntry,
            team::TeamEntry,
            technique::{ServeTypeEnum, TechniqueEnum},
        },
        video::{
            Anchor, Correction, EventRef, StoredProfile, VideoFile, VideoProfile, VideoProfiles,
            anchor_on,
        },
    };
    use chrono::{DateTime, Duration, FixedOffset, Utc};
    use uuid::Uuid;

    struct Providers {
        team_writer: Box<dyn TeamWriter + Send + Sync>,
        match_writer: Box<dyn MatchWriter + Send + Sync>,
        match_reader: Box<dyn MatchReader + Send + Sync>,
        set_writer: Box<dyn SetWriter + Send + Sync>,
        videos: Box<dyn VideoProfiles>,
    }

    fn database() -> Database {
        Database::new(crate::db::open_in_memory().unwrap())
    }

    fn sqlite(db: &Database) -> Providers {
        let language = Language::default();
        Providers {
            team_writer: Box::new(SqliteTeamWriter::new(db.clone())),
            match_writer: Box::new(SqliteMatchWriter::new(db.clone())),
            match_reader: Box::new(SqliteMatchReader::new(db.clone(), language.clone())),
            set_writer: Box::new(SqliteSetWriter::new(db.clone(), language)),
            videos: Box::new(SqliteVideoProfiles(db.clone())),
        }
    }

    fn time(text: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(text).unwrap()
    }

    fn start() -> DateTime<Utc> {
        time("2025-11-08T18:30:00+01:00").to_utc()
    }

    /// Our serve and their error, again and again: we keep serving.
    fn rally(player: Uuid, i: i64) -> [EventEntry; 2] {
        let at = |s: i64| start() + Duration::seconds(s);
        [
            EventEntry {
                timestamp: at(20 * i),
                event_type: EventTypeEnum::S,
                player: Some(player),
                eval: Some(EvalEnum::Positive),
                target_player: None,
                technique: Some(TechniqueEnum::Serve(ServeTypeEnum::Float)),
                flags: Default::default(),
            },
            EventEntry {
                timestamp: at(20 * i + 8),
                event_type: EventTypeEnum::OE,
                player: None,
                eval: None,
                target_player: None,
                technique: None,
                flags: Default::default(),
            },
        ]
    }

    /// A team and a match with a set of 8 events (`rally` 4 times).
    async fn scouted_match(p: &Providers) -> (TeamEntry, MatchEntry) {
        use RoleEnum::*;
        let players: Vec<PlayerEntry> = [
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
        .collect();
        let team = p
            .team_writer
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
            .unwrap();
        let ids: Vec<Uuid> = team.players.iter().map(|p| p.id).collect();
        let m = p
            .match_writer
            .create(
                &team,
                "Cortona Volley".into(),
                start().fixed_offset(),
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
                (0..4).flat_map(|i| rally(ids[0], i)).collect(),
            )
            .await
            .unwrap();
        let m = p.match_reader.read_single(&team, &m.id).await.unwrap();
        (team, m)
    }

    fn event(index: usize) -> EventRef {
        EventRef {
            set_number: 1,
            index,
        }
    }

    /// Two files (one in the data folder, from its name; one elsewhere),
    /// tied to the first event, two events put at their own time.
    fn profile() -> VideoProfile {
        VideoProfile {
            files: vec![
                VideoFile {
                    path: "videos/20251108 182500.mp4".into(),
                    start: Some(time("2025-11-08T18:25:00.123456789+01:00")),
                },
                VideoFile {
                    path: "/Volumes/camera/parte 2.mp4".into(),
                    start: None,
                },
            ],
            anchor: Some(anchor_on(event(0), start(), 312_000)),
            ..Default::default()
        }
        .corrected(event(3), -1500)
        .corrected(event(5), 800)
    }

    async fn read(p: &Providers, team: &TeamEntry, m: &MatchEntry) -> StoredProfile {
        p.videos.read(&team.id, &m.id).await.unwrap()
    }

    fn count(db: &Database, sql: &str) -> i64 {
        db.read(|tx| Ok(tx.query_row(sql, [], |r| r.get(0)).unwrap()))
            .unwrap()
    }

    /// Rows of the three tables of the video.
    fn video_rows(db: &Database) -> (i64, i64, i64) {
        (
            count(db, "SELECT count(*) FROM match_videos"),
            count(db, "SELECT count(*) FROM match_video_files"),
            count(db, "SELECT count(*) FROM match_video_corrections"),
        )
    }

    #[tokio::test]
    async fn the_same_on_files_and_database() {
        let p = sqlite(&database());
        let (team, m) = scouted_match(&p).await;
        assert_eq!(read(&p, &team, &m).await, StoredProfile::Missing);

        let saved = profile();
        p.videos.write(&team.id, &m.id, &saved).await.unwrap();
        assert_eq!(
            read(&p, &team, &m).await,
            StoredProfile::Profile(saved.clone()),
        );

        // tied to another event: the files in another order, the
        // corrections gone
        let changed = VideoProfile {
            files: saved.files.iter().rev().cloned().collect(),
            ..saved.tied_to(event(7), start() + Duration::seconds(68), 900)
        }
        .automatic()
        .corrected(event(1), 250);
        p.videos.write(&team.id, &m.id, &changed).await.unwrap();
        assert_eq!(
            read(&p, &team, &m).await,
            StoredProfile::Profile(changed.clone()),
        );

        // the anchor of the user without an event, no files
        let bare = VideoProfile {
            anchor: Some(Anchor {
                at: start(),
                offset_ms: -40,
                event: None,
            }),
            ..Default::default()
        };
        p.videos.write(&team.id, &m.id, &bare).await.unwrap();
        assert_eq!(read(&p, &team, &m).await, StoredProfile::Profile(bare),);
    }

    #[tokio::test]
    async fn the_profile_is_replaced_as_a_whole() {
        let db = database();
        let p = sqlite(&db);
        let (team, m) = scouted_match(&p).await;
        let three = VideoProfile {
            files: [profile().files, vec![profile().files[0].clone()]].concat(),
            ..profile()
        };
        p.videos.write(&team.id, &m.id, &three).await.unwrap();
        assert_eq!(video_rows(&db), (1, 3, 2));

        let one = VideoProfile {
            files: vec![profile().files[1].clone()],
            ..VideoProfile::default()
        };
        p.videos.write(&team.id, &m.id, &one).await.unwrap();
        assert_eq!(video_rows(&db), (1, 1, 0));
        assert_eq!(
            count(
                &db,
                "SELECT count(*) FROM match_videos WHERE anchor_at IS NULL
                 AND anchor_offset_ms IS NULL AND anchor_event IS NULL"
            ),
            1
        );
        assert_eq!(read(&p, &team, &m).await, StoredProfile::Profile(one));

        // as written: the offset of the start and every digit of it
        p.videos.write(&team.id, &m.id, &profile()).await.unwrap();
        let start: String = db
            .read(|tx| {
                Ok(tx
                    .query_row(
                        "SELECT start FROM match_video_files WHERE position = 0",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap())
            })
            .unwrap();
        assert_eq!(start, "2025-11-08T18:25:00.123456789+01:00");
    }

    /// The events by their ids: the anchor and the corrections are of the
    /// event, wherever it goes; an event undone, or of a removed set, has
    /// none, and the one scouted in its place doesn't get them.
    #[tokio::test]
    async fn the_video_follows_its_events() {
        let db = database();
        let p = sqlite(&db);
        let (team, m) = scouted_match(&p).await;
        let saved = VideoProfile {
            files: profile().files,
            ..VideoProfile::default()
        }
        .tied_to(event(7), start() + Duration::seconds(68), 1000)
        .corrected(event(2), 300)
        .corrected(event(6), 500);
        p.videos.write(&team.id, &m.id, &saved).await.unwrap();
        let reading = |m: MatchEntry| {
            let p = &p;
            let team = &team;
            async move { read(p, team, &m).await }
        };

        // an event changed in its row keeps its correction
        let mut set = m.sets[0].clone();
        set.events[2].timestamp += Duration::seconds(1);
        p.set_writer.update(&m, &set).await.unwrap();
        assert_eq!(
            reading(m.clone()).await,
            StoredProfile::Profile(saved.clone())
        );

        // an event after them: the same
        let player = team.players[0].id;
        p.set_writer
            .append_event(&m, 1, &rally(player, 4)[0])
            .await
            .unwrap();
        assert_eq!(
            reading(m.clone()).await,
            StoredProfile::Profile(saved.clone())
        );
        p.set_writer.remove_last_event(&m, 1).await.unwrap();

        // the event of the anchor undone: the anchor keeps its time
        p.set_writer.remove_last_event(&m, 1).await.unwrap();
        let untied = VideoProfile {
            anchor: saved.anchor.clone().map(|a| Anchor { event: None, ..a }),
            ..saved.clone()
        };
        assert_eq!(
            reading(m.clone()).await,
            StoredProfile::Profile(untied.clone())
        );

        // a corrected event undone, then another in its place: not
        // corrected (with the files, the same index was)
        p.set_writer.remove_last_event(&m, 1).await.unwrap();
        p.set_writer
            .append_event(&m, 1, &rally(player, 3)[0])
            .await
            .unwrap();
        let without = VideoProfile {
            corrections: vec![Correction {
                event: event(2),
                by_ms: 300,
            }],
            ..untied.clone()
        };
        assert_eq!(
            reading(m.clone()).await,
            StoredProfile::Profile(without.clone())
        );
        // still in the database, of the event undone
        assert_eq!(video_rows(&db), (1, 2, 2));

        // the set removed: nothing of it
        p.set_writer.remove(&m, 1).await.unwrap();
        assert_eq!(
            reading(m.clone()).await,
            StoredProfile::Profile(VideoProfile {
                corrections: vec![],
                ..without
            })
        );
    }

    #[tokio::test]
    async fn a_profile_with_an_event_not_there_isnt_written() {
        let db = database();
        let p = sqlite(&db);
        let (team, m) = scouted_match(&p).await;
        p.videos.write(&team.id, &m.id, &profile()).await.unwrap();
        let rows = video_rows(&db);
        for wrong in [
            profile().corrected(event(8), 100),
            profile().corrected(
                EventRef {
                    set_number: 2,
                    index: 0,
                },
                100,
            ),
            VideoProfile {
                anchor: Some(anchor_on(event(8), start(), 0)),
                ..VideoProfile::default()
            },
            VideoProfile {
                anchor: Some(anchor_on(
                    EventRef {
                        set_number: 0,
                        index: 0,
                    },
                    start(),
                    0,
                )),
                ..VideoProfile::default()
            },
        ] {
            let e = p.videos.write(&team.id, &m.id, &wrong).await.unwrap_err();
            assert!(e.to_string().contains("no event"), "{e}");
            assert_eq!(video_rows(&db), rows);
            assert_eq!(read(&p, &team, &m).await, StoredProfile::Profile(profile()));
        }
    }

    /// A correction of 0 isn't one; of two for an event the first counts,
    /// as `VideoProfile::correction` reads them; their order doesn't count.
    #[tokio::test]
    async fn corrections_are_one_per_event() {
        let db = database();
        let p = sqlite(&db);
        let (team, m) = scouted_match(&p).await;
        let c = |index: usize, by_ms: i64| Correction {
            event: event(index),
            by_ms,
        };
        let written = VideoProfile {
            corrections: vec![
                c(6, 40),
                c(4, 0),
                c(7, 10),
                c(1, -70),
                c(6, 90),
                c(3, 20),
                c(0, -30),
                c(5, 50),
                c(2, 60),
            ],
            ..VideoProfile::default()
        };
        assert_eq!(written.correction(event(6)), Some(40));
        p.videos.write(&team.id, &m.id, &written).await.unwrap();
        assert_eq!(video_rows(&db), (1, 0, 7));
        let StoredProfile::Profile(back) = read(&p, &team, &m).await else {
            panic!("no profile");
        };
        // in the order of their events, whatever the order of their ids
        assert_eq!(
            back.corrections,
            vec![
                c(0, -30),
                c(1, -70),
                c(2, 60),
                c(3, 20),
                c(5, 50),
                c(6, 40),
                c(7, 10)
            ]
        );
        assert_eq!(back.correction(event(4)), None);
    }

    #[tokio::test]
    async fn only_a_match_of_its_team_a_has_a_video() {
        let db = database();
        let p = sqlite(&db);
        let (team, m) = scouted_match(&p).await;
        let (other, theirs) = scouted_match(&p).await;
        p.videos.write(&team.id, &m.id, &profile()).await.unwrap();

        for (team_id, match_id) in [
            (other.id, m.id.clone()),
            (team.id, theirs.id.clone()),
            (team.id, Uuid::new_v4().to_string()),
            (team.id, m.id.to_uppercase()),
            (team.id, "2025-11-08_cortona_volley".into()),
        ] {
            assert!(p.videos.read(&team_id, &match_id).await.is_err());
            assert!(
                p.videos
                    .write(&team_id, &match_id, &VideoProfile::default())
                    .await
                    .is_err()
            );
        }
        assert_eq!(video_rows(&db), (1, 2, 2));
        assert_eq!(read(&p, &other, &theirs).await, StoredProfile::Missing);

        // a match removed: its video goes with it (of this device, D16),
        // no longer read, nor saved
        p.match_writer.remove(&team.id, &m.id).await.unwrap();
        assert!(p.videos.read(&team.id, &m.id).await.is_err());
        assert!(
            p.videos
                .write(&team.id, &m.id, &VideoProfile::default())
                .await
                .is_err()
        );
        assert_eq!(video_rows(&db), (0, 0, 0));
    }

    /// Its paths are of this computer: an archive leaves it out.
    #[tokio::test]
    async fn the_video_isnt_exported() {
        let db = database();
        let p = sqlite(&db);
        let (team, m) = scouted_match(&p).await;
        p.videos.write(&team.id, &m.id, &profile()).await.unwrap();
        let id = Uuid::parse_str(&m.id).unwrap();
        for subject in [Subject::Match(id), Subject::TeamWithMatches(team.id)] {
            let names: Vec<String> = unzip(&export(&db, subject).unwrap())
                .unwrap()
                .into_iter()
                .map(|(name, _)| name)
                .collect();
            assert!(names.iter().any(|n| n == "events.json"), "{names:?}");
            assert!(!names.iter().any(|n| n.contains("video")), "{names:?}");
        }
    }

    /// The adapter alone, on rows not read from the database: the events
    /// of a removed set aren't there, the files in the order of their
    /// places whatever the order of the rows.
    #[tokio::test]
    async fn the_adapter_on_rows_as_given() {
        let p = sqlite(&database());
        let (_, m) = scouted_match(&p).await;
        let id = Uuid::new_v4();
        let set = SetRows::of(&m.sets[0], id).unwrap();
        let events = EventsOfA::of(std::slice::from_ref(&set));
        let mut rows = VideoRows::of(&profile(), id, &events).unwrap();
        assert_eq!(rows.profile(&events), profile());

        rows.files.reverse();
        assert_eq!(rows.profile(&events), profile());

        let mut removed = set;
        removed.row.deleted = true;
        let none = EventsOfA::of(&[removed]);
        assert_eq!(none.id(event(0)), None);
        assert!(VideoRows::of(&profile(), id, &none).is_err());
    }
}
