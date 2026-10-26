//! Where the data lives: every store keeps the blobs the same way, and the
//! providers and the archives work on any of them.

#[cfg(test)]
mod tests {
    use crate::{
        archive::{export_match, export_team, import_match, import_team},
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        localization::Language,
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            set_writer::SetWriter,
            store::{
                match_reader::StoreMatchReader, match_writer::StoreMatchWriter,
                set_reader::StoreSetReader, set_writer::StoreSetWriter,
                team_reader::StoreTeamReader, team_writer::StoreTeamWriter,
            },
            team_reader::TeamReader,
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
        storage::{Store, StoreEntry, fs::FsStore, layout, memory::MemoryStore},
        tests::{EN, fs_store},
    };
    use chrono::Utc;
    use std::sync::Arc;
    use tempfile::TempDir;
    use uuid::Uuid;

    async fn blobs_and_folders(store: &dyn Store) {
        assert_eq!(store.read("a/b.json").await.unwrap(), None);
        assert_eq!(store.list("a").await.unwrap(), vec![]);
        assert!(!store.exists("a").await.unwrap());
        store.write("a/b.json", b"{}").await.unwrap();
        store.write("a/b.json", b"[]").await.unwrap();
        assert_eq!(store.read("a/b.json").await.unwrap(), Some(b"[]".to_vec()));
        store.append("a/c/d.csv", b"1\n").await.unwrap();
        store.append("a/c/d.csv", b"2\n").await.unwrap();
        assert_eq!(
            store.read("a/c/d.csv").await.unwrap(),
            Some(b"1\n2\n".to_vec())
        );
        store.write("top.json", b"").await.unwrap();
        assert!(store.exists("a").await.unwrap());
        assert!(store.exists("a/c").await.unwrap());
        assert!(store.exists("a/b.json").await.unwrap());
        assert!(!store.exists("a/e").await.unwrap());
        assert_eq!(
            store.list("a").await.unwrap(),
            vec![
                StoreEntry::Blob("b.json".into()),
                StoreEntry::Folder("c".into())
            ]
        );
        assert_eq!(
            store.list(layout::ROOT).await.unwrap(),
            vec![
                StoreEntry::Blob("top.json".into()),
                StoreEntry::Folder("a".into())
            ]
        );
    }

    #[tokio::test]
    async fn the_stores_keep_blobs_and_folders_alike() {
        let dir = TempDir::new().unwrap();
        blobs_and_folders(&FsStore::new(dir.path())).await;
        blobs_and_folders(&MemoryStore::new()).await;
    }

    /// A write goes to a hidden file first, then in place of the old one:
    /// stopped halfway, the old one is still there, whole; what's left of
    /// it is never data.
    #[tokio::test]
    async fn a_write_stopped_halfway_leaves_the_old_file() {
        let dir = TempDir::new().unwrap();
        let store = FsStore::new(dir.path());
        store.write("a/b.json", b"old").await.unwrap();
        assert!(!dir.path().join("a/.b.json.tmp").exists());
        // a write stopped halfway: the new file never took the place of
        // the old one
        std::fs::write(dir.path().join("a/.b.json.tmp"), b"ne").unwrap();
        std::fs::write(dir.path().join("a/.DS_Store"), b"").unwrap();
        assert_eq!(store.read("a/b.json").await.unwrap(), Some(b"old".to_vec()));
        assert_eq!(
            store.list("a").await.unwrap(),
            vec![StoreEntry::Blob("b.json".into())]
        );
        // the next write goes over it
        store.write("a/b.json", b"new").await.unwrap();
        assert_eq!(store.read("a/b.json").await.unwrap(), Some(b"new".to_vec()));
        assert!(!dir.path().join("a/.b.json.tmp").exists());
        // the new file can't be written: the old one stays
        std::fs::create_dir(dir.path().join("a/.b.json.tmp")).unwrap();
        assert!(store.write("a/b.json", b"newer").await.is_err());
        assert_eq!(store.read("a/b.json").await.unwrap(), Some(b"new".to_vec()));
    }

    /// A key can't reach out of the folder of the data.
    #[tokio::test]
    async fn the_file_system_store_stays_in_its_folder() {
        let dir = TempDir::new().unwrap();
        let store = FsStore::new(&dir.path().join("data"));
        for key in [
            "../out.json",
            "a/../../out.json",
            "a//b",
            "/abs",
            "a\\b",
            "c:x",
        ] {
            assert!(store.write(key, b"x").await.is_err(), "{key}");
            assert!(store.read(key).await.is_err(), "{key}");
        }
        assert!(!dir.path().join("out.json").exists());
    }

    struct Providers {
        team_reader: StoreTeamReader,
        team_writer: StoreTeamWriter,
        match_reader: StoreMatchReader,
        match_writer: StoreMatchWriter,
        set_writer: StoreSetWriter,
    }

    impl Providers {
        fn new(store: Arc<dyn Store>) -> Self {
            Self {
                team_reader: StoreTeamReader::new(Arc::clone(&store), Language::default()),
                team_writer: StoreTeamWriter::new(Arc::clone(&store)),
                match_reader: StoreMatchReader::new(
                    Arc::clone(&store),
                    Arc::new(StoreSetReader::new(Arc::clone(&store), Language::default())),
                    Language::default(),
                ),
                match_writer: StoreMatchWriter::new(Arc::clone(&store)),
                set_writer: StoreSetWriter::new(store, Language::default()),
            }
        }
    }

    /// A team, and a match with a set: we serve and they miss.
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
        .enumerate()
        .map(|(i, role)| PlayerEntry {
            id: Uuid::new_v4(),
            name: format!("player {}", i + 1),
            role: Some(role),
            number: i as u8 + 1,
            deleted: false,
            birth_date: None,
            height_cm: None,
            notes: String::new(),
        })
        .collect();
        let ids: Vec<Uuid> = players.iter().map(|p| p.id).collect();
        let team = p
            .team_writer
            .save(TeamInput::New {
                id: None,
                name: "Volley Club".into(),
                year: 2025,
                classification: None,
                gender: None,
                players,
            })
            .await
            .unwrap();
        let m = p
            .match_writer
            .create(
                &team,
                "Cortona Volley".into(),
                Utc::now().fixed_offset(),
                true,
                DEFAULT_MAX_SUBSTITUTIONS,
                String::new(),
            )
            .await
            .unwrap();
        let events = vec![
            EventEntry {
                timestamp: Utc::now(),
                event_type: EventTypeEnum::S,
                player: Some(ids[0]),
                eval: Some(EvalEnum::Positive),
                target_player: None,
                technique: Some(TechniqueEnum::Serve(ServeTypeEnum::Float)),
            },
            EventEntry {
                timestamp: Utc::now(),
                event_type: EventTypeEnum::OE,
                player: None,
                eval: None,
                target_player: None,
                technique: None,
            },
        ];
        p.set_writer
            .create(
                &m,
                1,
                TeamSideEnum::Us,
                ids[..6].try_into().unwrap(),
                ids[6],
                None,
                ids[0],
                events,
            )
            .await
            .unwrap();
        let m = p.match_reader.read_single(&team, &m.id).await.unwrap();
        (team, m)
    }

    /// The set as replayed: who's on court, the events and the score.
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

    /// The app works the same with the data in memory.
    #[tokio::test]
    async fn the_providers_work_on_any_store() {
        let p = Providers::new(Arc::new(MemoryStore::new()));
        let (team, m) = scouted_match(&p).await;
        assert_eq!(
            summary(&m),
            vec![(vec![EventTypeEnum::S, EventTypeEnum::OE], 1, 0)]
        );
        let teams = p.team_reader.read_all().await.unwrap();
        assert_eq!(teams.len(), 1);
        assert_eq!(teams[0].as_ref().unwrap().players.len(), 7);
        let matches = p.match_reader.read_all(&team).await.unwrap();
        assert_eq!(matches.len(), 1);
        assert!(p.match_reader.exists(&team, &m.id).await.unwrap());
        p.set_writer.remove_last_event(&m, 1).await.unwrap();
        let m = p.match_reader.read_single(&team, &m.id).await.unwrap();
        assert_eq!(summary(&m), vec![(vec![EventTypeEnum::S], 0, 0)]);
    }

    /// On the file system the data keeps the folders of the older versions.
    #[tokio::test]
    async fn the_file_system_keeps_the_layout() {
        let dir = TempDir::new().unwrap();
        let (team, m) = scouted_match(&Providers::new(fs_store(dir.path()))).await;
        let folder = dir.path().join(team.id.to_string());
        assert!(folder.join("team.json").is_file());
        let match_folder = folder.join(&m.id);
        for file in ["match.json", "set_1.json", "set_1.csv"] {
            assert!(match_folder.join(file).is_file(), "{file}");
        }
        let csv = std::fs::read_to_string(match_folder.join("set_1.csv")).unwrap();
        assert_eq!(csv.lines().count(), 2);
    }

    /// A team and a match exported from one store load from another one.
    #[tokio::test]
    async fn the_archives_move_teams_and_matches() {
        let from = Arc::new(MemoryStore::new());
        let source = Providers::new(from.clone());
        let (team, m) = scouted_match(&source).await;
        let team_zip = export_team(&*from, &team.id).await.unwrap();
        let match_zip = export_match(&*from, &team.id, &m.id).await.unwrap();

        let target = Providers::new(Arc::new(MemoryStore::new()));
        let imported_team = import_team(
            &team_zip,
            None,
            &target.team_reader,
            &target.team_writer,
            EN,
        )
        .await
        .unwrap();
        assert_eq!(imported_team.players.len(), team.players.len());
        let imported = import_match(
            &match_zip,
            &imported_team,
            &target.match_reader,
            &target.match_writer,
            &target.set_writer,
            EN,
        )
        .await
        .unwrap();
        let loaded = target
            .match_reader
            .read_single(&imported_team, &imported.id)
            .await
            .unwrap();
        assert_eq!(summary(&loaded), summary(&m));
        assert_eq!(
            format!("{:?}", loaded.sets[0].events),
            format!("{:?}", m.sets[0].events)
        );

        // not twice
        let again = import_team(
            &team_zip,
            None,
            &target.team_reader,
            &target.team_writer,
            EN,
        );
        assert!(again.await.is_err());
        let again = import_match(
            &match_zip,
            &imported_team,
            &target.match_reader,
            &target.match_writer,
            &target.set_writer,
            EN,
        );
        assert!(again.await.is_err());
    }
}
