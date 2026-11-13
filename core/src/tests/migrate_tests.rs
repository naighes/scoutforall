//! The migration of the data folder into the database at the start
//! (`legacy::migrate`, TODO/001 step 11): every version of the legacy
//! fixtures moves with its scores; a backup first, read back; a stop at
//! any point and a run after it end as one run alone; a second run does
//! nothing; what an old version writes again moves, once; what can't be
//! read, converted or written stays, told, and the rest moves.

#[cfg(test)]
mod tests {
    use crate::{
        errors::{AppError, IOError},
        legacy::{
            folder::NotRead,
            migrate::{Backup, Migrated, migrate, unzip},
        },
        localization::Language,
        providers::{
            match_reader::MatchReader,
            sqlite::{Database, matches::SqliteMatchReader, teams::SqliteTeamReader},
            store::{
                match_reader::StoreMatchReader, set_reader::StoreSetReader,
                team_reader::StoreTeamReader,
            },
            team_reader::TeamReader,
        },
        shapes::{set::SetEntry, team::TeamEntry},
        storage::{Stamps, Store, StoreEntry, fs::FsStore},
        tests::{
            EN, LEGACY_VERSIONS as VERSIONS, LegacyScores, copy_dir, legacy_fixtures, legacy_scores,
        },
    };
    use async_trait::async_trait;
    use std::{
        collections::{BTreeMap, BTreeSet},
        path::{Path, PathBuf},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };
    use tempfile::TempDir;
    use uuid::Uuid;

    /// A data folder (`data/`) and where its backups go (`backups/`).
    struct Device {
        dir: TempDir,
    }

    impl Device {
        fn of(version: &str) -> Self {
            let dir = TempDir::new().unwrap();
            copy_dir(
                &legacy_fixtures(version).join("data"),
                &dir.path().join("data"),
            );
            std::fs::create_dir(dir.path().join("backups")).unwrap();
            Device { dir }
        }

        fn data(&self) -> PathBuf {
            self.dir.path().join("data")
        }

        fn store(&self) -> FsStore {
            FsStore::new(&self.data())
        }

        fn backup(&self) -> Backup {
            Backup {
                dir: self.dir.path().join("backups"),
                prefix: ".scout4all".into(),
            }
        }

        fn backups(&self) -> Vec<PathBuf> {
            let mut found: Vec<PathBuf> = std::fs::read_dir(self.dir.path().join("backups"))
                .unwrap()
                .map(|e| e.unwrap().path())
                .collect();
            // in the order taken: `x.zip`, `x-2.zip`, `x-3.zip`…
            found.sort_by_key(|p| (p.as_os_str().len(), p.clone()));
            found
        }

        async fn migrate(&self, db: &Database) -> Migrated {
            migrate(&self.store(), db, &self.backup(), EN)
                .await
                .unwrap()
        }
    }

    fn database() -> Database {
        Database::new(crate::db::open_in_memory().unwrap())
    }

    /// Every file under the folder with its content, by path from it.
    fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
        let mut found = BTreeMap::new();
        let mut folders = vec![root.to_path_buf()];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(&folder).unwrap() {
                let path = entry.unwrap().path();
                match path.is_dir() {
                    true => folders.push(path),
                    false => {
                        let key = path
                            .strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned();
                        found.insert(key, std::fs::read(&path).unwrap());
                    }
                }
            }
        }
        found
    }

    /// The files that aren't hidden (not data: the index, the system's).
    fn data_files(root: &Path) -> BTreeMap<String, Vec<u8>> {
        files(root)
            .into_iter()
            .filter(|(key, _)| !key.split('/').any(|part| part.starts_with('.')))
            .collect()
    }

    /// What a match is made of, to compare the files and the database:
    /// its opponent, its sets and their scores.
    type Matches = BTreeMap<(Uuid, String), (String, Vec<SetEntry>, Vec<(u8, u8)>)>;

    /// The matches by team and id in the files, as the providers over the
    /// files read them.
    async fn in_files(root: &Path) -> Matches {
        let store: Arc<dyn Store> = Arc::new(FsStore::new(root));
        let teams = StoreTeamReader::new(Arc::clone(&store), Language::default());
        let matches = StoreMatchReader::new(
            Arc::clone(&store),
            Arc::new(StoreSetReader::new(Arc::clone(&store), Language::default())),
            Language::default(),
        );
        let mut read = Matches::new();
        for team in teams.read_all().await.unwrap() {
            let team = team.unwrap();
            for m in matches.read_all(&team).await.unwrap() {
                let m = m.unwrap();
                read.insert(
                    (team.id, m.id.clone()),
                    (m.opponent.clone(), m.sets.clone(), m.set_scores().unwrap()),
                );
            }
        }
        read
    }

    /// The matches in the database by where they were (team, folder).
    async fn in_database(db: &Database) -> Matches {
        let legacy: BTreeMap<String, (Uuid, String)> = db
            .read(|tx| {
                let mut statement = tx
                    .prepare("SELECT id, legacy_team, legacy_id FROM matches WHERE deleted = 0")
                    .unwrap();
                let rows = statement
                    .query_map([], |r| {
                        let team: String = r.get(1)?;
                        Ok((r.get(0)?, (Uuid::parse_str(&team).unwrap(), r.get(2)?)))
                    })
                    .unwrap()
                    .collect::<Result<_, _>>()
                    .unwrap();
                Ok(rows)
            })
            .unwrap();
        let teams = SqliteTeamReader::new(db.clone());
        let matches = SqliteMatchReader::new(db.clone(), Language::default());
        let mut read = Matches::new();
        for team in teams.read_all().await.unwrap() {
            let team = team.unwrap();
            for m in matches.read_all(&team).await.unwrap() {
                let m = m.unwrap();
                read.insert(
                    legacy[&m.id].clone(),
                    (m.opponent.clone(), m.sets.clone(), m.set_scores().unwrap()),
                );
            }
        }
        read
    }

    fn scores(matches: &Matches) -> LegacyScores {
        matches
            .iter()
            .map(|((_, id), (_, _, scores))| (id.clone(), scores.clone()))
            .collect()
    }

    fn count(db: &Database, sql: &str) -> i64 {
        db.read(|tx| Ok(tx.query_row(sql, [], |r| r.get(0)).unwrap()))
            .unwrap()
    }

    /// The rows of the database, every table (but the cache), to tell
    /// that a run changed nothing or two runs end the same.
    fn rows(db: &Database) -> BTreeMap<&'static str, i64> {
        [
            ("teams", "SELECT count(*) FROM teams"),
            ("persons", "SELECT count(*) FROM persons"),
            ("team_members", "SELECT count(*) FROM team_members"),
            ("matches", "SELECT count(*) FROM matches"),
            ("match_roster", "SELECT count(*) FROM match_roster"),
            ("sets", "SELECT count(*) FROM sets"),
            ("set_lineups", "SELECT count(*) FROM set_lineups"),
            ("events", "SELECT count(*) FROM events"),
        ]
        .into_iter()
        .map(|(table, sql)| (table, count(db, sql)))
        .collect()
    }

    /// The folder of the match with the video (from v0.15.0): its
    /// `video.json` isn't moved yet (step 15), so it stays.
    const VIDEO: &str = "2025-11-08_pallavolo_prato";

    fn has_video(version: &str) -> bool {
        matches!(version, "v0.15.0" | "v0.16.0" | "v0.18.0")
    }

    fn folders_left(migrated: &Migrated) -> BTreeSet<String> {
        migrated.left.iter().map(|n| n.folder.clone()).collect()
    }

    #[tokio::test]
    async fn every_version_moves_into_the_database_with_its_scores() {
        for version in VERSIONS {
            let device = Device::of(version);
            let before = data_files(&device.data());
            let expected = in_files(&device.data()).await;
            let db = database();
            let migrated = device.migrate(&db).await;

            let teams = before.keys().filter(|k| k.ends_with("/team.json")).count();
            let video = has_video(version);
            assert_eq!(migrated.teams, teams, "{version}");
            assert_eq!(migrated.matches, legacy_scores(version).len(), "{version}");
            assert_eq!(migrated.already, 0, "{version}");
            let read = in_database(&db).await;
            assert_eq!(read, expected, "{version}");
            assert_eq!(scores(&read), legacy_scores(version), "{version}");
            // a cache row for each match that replays (all of them)
            assert_eq!(
                count(&db, "SELECT count(*) FROM match_summary"),
                legacy_scores(version).len() as i64,
                "{version}"
            );

            // the backup: the whole folder, but what's hidden
            let backups = device.backups();
            assert_eq!(backups.len(), 1, "{version}");
            let name = backups[0]
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            assert!(
                name.starts_with(".scout4all-backup-") && name.ends_with(".zip"),
                "{name}"
            );
            let zipped: BTreeMap<String, Vec<u8>> = unzip(&std::fs::read(&backups[0]).unwrap())
                .unwrap()
                .into_iter()
                .collect();
            assert_eq!(zipped, before, "{version}");

            // what's left: the settings and the queue, and the match with
            // the video, whole, in its team
            let after = data_files(&device.data());
            let kept: BTreeMap<String, Vec<u8>> = before
                .iter()
                .filter(|(key, _)| {
                    !key.contains('/')
                        || (video && key.contains(VIDEO))
                        || (video
                            && key.ends_with("/team.json")
                            && before.keys().any(|k| {
                                k.contains(VIDEO) && k.split('/').next() == key.split('/').next()
                            }))
                })
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            assert_eq!(after, kept, "{version}");
            assert!(!device.data().join(".index.json").exists(), "{version}");
            match video {
                true => {
                    assert_eq!(migrated.left.len(), 1, "{version}");
                    assert!(migrated.left[0].folder.ends_with(VIDEO));
                    assert_eq!(
                        migrated.left[0].reason,
                        EN.migration_files_kept.replace("{}", "video.json")
                    );
                }
                false => assert_eq!(migrated.left, vec![], "{version}"),
            }
        }
    }

    #[tokio::test]
    async fn a_second_run_does_nothing() {
        for version in VERSIONS {
            let device = Device::of(version);
            let db = database();
            let first = device.migrate(&db).await;
            let files_then = files(&device.data());
            let rows_then = rows(&db);
            let second = device.migrate(&db).await;
            assert_eq!(
                second,
                Migrated {
                    left: first.left.clone(),
                    ..Migrated::default()
                },
                "{version}"
            );
            assert_eq!(device.backups().len(), 1, "{version}: no other backup");
            assert_eq!(files(&device.data()), files_then, "{version}");
            assert_eq!(rows(&db), rows_then, "{version}");
        }
    }

    #[tokio::test]
    async fn nothing_to_move_takes_no_backup() {
        let device = Device::of("v0.18.0");
        std::fs::remove_dir_all(device.data()).unwrap();
        std::fs::create_dir(device.data()).unwrap();
        std::fs::write(device.data().join("config.json"), "{}").unwrap();
        let db = database();
        assert_eq!(device.migrate(&db).await, Migrated::default());
        assert_eq!(device.backups(), Vec::<PathBuf>::new());
        assert_eq!(rows(&db).values().sum::<i64>(), 0);
        // no data folder at all
        std::fs::remove_dir_all(device.data()).unwrap();
        assert_eq!(device.migrate(&db).await, Migrated::default());
    }

    #[tokio::test]
    async fn without_the_backup_nothing_changes() {
        let device = Device::of("v0.18.0");
        let before = files(&device.data());
        let db = database();
        let backup = Backup {
            dir: device.dir.path().join("missing"),
            prefix: ".scout4all".into(),
        };
        let e = migrate(&device.store(), &db, &backup, EN)
            .await
            .unwrap_err();
        assert!(matches!(e, AppError::IO(IOError::Io(_))), "{e:?}");
        assert_eq!(files(&device.data()), before);
        assert_eq!(rows(&db).values().sum::<i64>(), 0);
        // nor a part of it left
        assert!(!device.dir.path().join("missing").exists());
    }

    #[tokio::test]
    async fn a_backup_taken_the_same_day_isnt_overwritten() {
        let device = Device::of("v0.18.0");
        let db = database();
        device.migrate(&db).await;
        // an old version writes a match again
        let fresh = Device::of("v0.18.0");
        let (team, _) = first_match(&fresh.data());
        copy_dir(
            &fresh.data().join(&team).join("2025-10-04_rivali_pisa"),
            &device.data().join(&team).join("2025-12-01_again"),
        );
        let second = device.migrate(&db).await;
        let backups = device.backups();
        assert_eq!(backups.len(), 2);
        assert_eq!(Some(&backups[1]), second.backup.as_ref());
        assert!(
            backups[1]
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("-2.zip"),
            "{backups:?}"
        );
    }

    /// The team of the match `2025-10-04_rivali_pisa` and its folder.
    fn first_match(root: &Path) -> (String, PathBuf) {
        let team = std::fs::read_dir(root)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.join("2025-10-04_rivali_pisa").exists())
            .unwrap();
        let name = team.file_name().unwrap().to_string_lossy().into_owned();
        (name, team.join("2025-10-04_rivali_pisa"))
    }

    /// A store that stops working after `alive` calls, as an app stopped
    /// there: every call after fails.
    #[derive(Debug)]
    struct Stopping {
        store: FsStore,
        alive: usize,
        calls: AtomicUsize,
    }

    impl Stopping {
        fn call(&self) -> Result<(), AppError> {
            match self.calls.fetch_add(1, Ordering::SeqCst) < self.alive {
                true => Ok(()),
                false => Err(AppError::IO(IOError::Msg("stopped".into()))),
            }
        }
    }

    #[async_trait]
    impl Store for Stopping {
        async fn read(&self, key: &str) -> Result<Option<Vec<u8>>, AppError> {
            self.call()?;
            self.store.read(key).await
        }
        async fn write(&self, key: &str, data: &[u8]) -> Result<(), AppError> {
            self.call()?;
            self.store.write(key, data).await
        }
        async fn append(&self, key: &str, data: &[u8]) -> Result<(), AppError> {
            self.call()?;
            self.store.append(key, data).await
        }
        async fn exists(&self, key: &str) -> Result<bool, AppError> {
            self.call()?;
            self.store.exists(key).await
        }
        async fn list(&self, folder: &str) -> Result<Vec<StoreEntry>, AppError> {
            self.call()?;
            self.store.list(folder).await
        }
        async fn remove(&self, key: &str) -> Result<(), AppError> {
            self.call()?;
            self.store.remove(key).await
        }
        async fn remove_folder(&self, folder: &str) -> Result<(), AppError> {
            self.call()?;
            self.store.remove_folder(folder).await
        }
        async fn stamps(&self, folder: &str) -> Result<Stamps, AppError> {
            self.call()?;
            self.store.stamps(folder).await
        }
        async fn folder_stamps(&self, folder: &str) -> Result<Vec<(String, Stamps)>, AppError> {
            self.call()?;
            self.store.folder_stamps(folder).await
        }
    }

    /// A run stopped after each call to the store (reading, backing up,
    /// writing to the database, removing), then a whole run: always the
    /// end of a run alone, every match once.
    #[tokio::test]
    async fn a_run_stopped_anywhere_then_another_end_as_one_run() {
        let version = "v0.18.0";
        let reference = Device::of(version);
        let db = database();
        let counted = Stopping {
            store: reference.store(),
            alive: usize::MAX,
            calls: AtomicUsize::new(0),
        };
        let whole = migrate(&counted, &db, &reference.backup(), EN)
            .await
            .unwrap();
        let calls = counted.calls.load(Ordering::SeqCst);
        assert!(calls > 50, "{calls}");
        let (files_after, rows_after, matches) =
            (files(&reference.data()), rows(&db), in_database(&db).await);

        for alive in 0..calls {
            let device = Device::of(version);
            let db = database();
            let stopping = Stopping {
                store: device.store(),
                alive,
                calls: AtomicUsize::new(0),
            };
            // as it ends: an error, or something left
            let _ = migrate(&stopping, &db, &device.backup(), EN).await;
            let then = device.migrate(&db).await;
            assert_eq!(
                folders_left(&then),
                folders_left(&whole),
                "stopped at {alive}"
            );
            assert_eq!(files(&device.data()), files_after, "stopped at {alive}");
            assert_eq!(rows(&db), rows_after, "stopped at {alive}");
            assert_eq!(in_database(&db).await, matches, "stopped at {alive}");
            // a backup before anything changed: of the whole folder
            let first = device.backups();
            if let Some(first) = first.first() {
                let zipped: BTreeMap<String, Vec<u8>> = unzip(&std::fs::read(first).unwrap())
                    .unwrap()
                    .into_iter()
                    .collect();
                let fresh = Device::of(version);
                assert_eq!(zipped, data_files(&fresh.data()), "stopped at {alive}");
            }
            assert!(
                !device
                    .backups()
                    .iter()
                    .any(|b| b.to_string_lossy().ends_with(".part")),
                "stopped at {alive}"
            );
        }
    }

    /// Fails the writes of the match at `legacy_id` in the database.
    fn fail_writes_of(db: &Database, legacy_id: &str) {
        db.write(|tx| {
            tx.execute_batch(&format!(
                "CREATE TEMP TRIGGER fail BEFORE INSERT ON matches
                 WHEN NEW.legacy_id = '{legacy_id}'
                 BEGIN SELECT RAISE(ABORT, 'injected'); END;"
            ))
            .unwrap();
            Ok(())
        })
        .unwrap();
    }

    fn drop_trigger(db: &Database, name: &str) {
        db.write(|tx| {
            tx.execute_batch(&format!("DROP TRIGGER {name}")).unwrap();
            Ok(())
        })
        .unwrap();
    }

    #[tokio::test]
    async fn a_match_not_written_stays_and_moves_the_next_time() {
        let device = Device::of("v0.18.0");
        let db = database();
        fail_writes_of(&db, "2025-10-04_rivali_pisa");
        let migrated = device.migrate(&db).await;
        let (team, folder) = first_match(&device.data());
        let pisa = format!("{team}/2025-10-04_rivali_pisa");
        assert_eq!(migrated.matches, legacy_scores("v0.18.0").len() - 1);
        let failed: Vec<&NotRead> = migrated.left.iter().filter(|n| n.folder == pisa).collect();
        assert_eq!(failed.len(), 1, "{:?}", migrated.left);
        assert!(
            failed[0].reason.contains("injected"),
            "{}",
            failed[0].reason
        );
        assert!(folder.join("match.json").exists());
        assert_eq!(
            count(
                &db,
                "SELECT count(*) FROM matches WHERE legacy_id = '2025-10-04_rivali_pisa'"
            ),
            0
        );
        drop_trigger(&db, "fail");
        let again = device.migrate(&db).await;
        assert_eq!((again.matches, again.already), (1, 0));
        assert!(!folder.exists());
        let read = in_database(&db).await;
        assert_eq!(scores(&read), legacy_scores("v0.18.0"));
    }

    #[tokio::test]
    async fn a_match_not_read_back_as_written_isnt_written() {
        let device = Device::of("v0.18.0");
        let db = database();
        // the first event of the match changed while written
        db.write(|tx| {
            tx.execute_batch(
                "CREATE TEMP TRIGGER change AFTER INSERT ON events
                 WHEN NEW.seq = 0 AND NEW.set_number = 1
                  AND (SELECT legacy_id FROM matches WHERE id = NEW.match_id)
                      = '2025-10-04_rivali_pisa'
                 BEGIN UPDATE events SET flags = flags + 1 WHERE id = NEW.id; END;",
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
        let migrated = device.migrate(&db).await;
        let (team, folder) = first_match(&device.data());
        let pisa = format!("{team}/2025-10-04_rivali_pisa");
        let failed: Vec<&NotRead> = migrated.left.iter().filter(|n| n.folder == pisa).collect();
        assert_eq!(failed.len(), 1, "{:?}", migrated.left);
        assert!(
            failed[0].reason.contains("not read back"),
            "{}",
            failed[0].reason
        );
        assert!(folder.join("match.json").exists());
        assert_eq!(
            count(
                &db,
                "SELECT count(*) FROM matches WHERE legacy_id = '2025-10-04_rivali_pisa'"
            ),
            0
        );
        assert_eq!(migrated.matches, legacy_scores("v0.18.0").len() - 1);
    }

    #[tokio::test]
    async fn what_cant_be_read_stays_and_the_rest_moves() {
        let device = Device::of("v0.18.0");
        let root = device.data();
        let (team, _) = first_match(&root);
        // a folder not named after a team, a set cut in two
        std::fs::create_dir(root.join("not-a-team")).unwrap();
        std::fs::write(root.join("not-a-team/team.json"), "{}").unwrap();
        std::fs::write(
            root.join(&team)
                .join("2025-10-11_polisportiva_lucca/set_1.json"),
            "{\"serving",
        )
        .unwrap();
        let db = database();
        let migrated = device.migrate(&db).await;
        let lucca = format!("{team}/2025-10-11_polisportiva_lucca");
        let told: BTreeMap<&str, &str> = migrated
            .left
            .iter()
            .map(|n| (n.folder.as_str(), n.reason.as_str()))
            .collect();
        assert!(told["not-a-team"].contains(EN.invalid_team_folder_name));
        assert!(told[lucca.as_str()].contains(EN.invalid_set));
        assert_eq!(migrated.matches, legacy_scores("v0.18.0").len() - 1);
        assert!(root.join(&lucca).join("match.json").exists());
        // its team stays, with it
        assert!(root.join(&team).join("team.json").exists());
        assert!(!root.join(&team).join("2025-10-04_rivali_pisa").exists());
        // nothing more to do: no backup, the same told
        let again = device.migrate(&db).await;
        assert_eq!(again.backup, None);
        assert_eq!(again.left, migrated.left);
        assert_eq!(device.backups().len(), 1);
    }

    #[tokio::test]
    async fn a_match_an_old_version_writes_again_moves_once() {
        let device = Device::of("v0.18.0");
        let db = database();
        device.migrate(&db).await;
        let rows_after = rows(&db);
        let matches_after = in_database(&db).await;
        let fresh = Device::of("v0.18.0");
        let (team, folder) = first_match(&fresh.data());
        let again = device.data().join(&team).join("2025-10-04_rivali_pisa");

        // the same match, the same: its folder removed, nothing written
        copy_dir(&fresh.data().join(&team), &device.data().join(&team));
        let migrated = device.migrate(&db).await;
        assert_eq!((migrated.teams, migrated.matches), (0, 0));
        // every match of the team (but the video's) was already there
        assert!(migrated.already >= 1);
        assert!(!again.exists());
        assert_eq!(rows(&db), rows_after);
        assert_eq!(in_database(&db).await, matches_after);
        assert_eq!(device.backups().len(), 2);

        // the same match, different (an event less): left, told
        copy_dir(&folder, &again);
        let csv = again.join("set_1.csv");
        let content = std::fs::read_to_string(&csv).unwrap();
        let mut lines: Vec<&str> = content.lines().collect();
        lines.pop();
        std::fs::write(&csv, lines.join("\n") + "\n").unwrap();
        let migrated = device.migrate(&db).await;
        assert!(migrated.left.contains(&NotRead {
            folder: format!("{team}/2025-10-04_rivali_pisa"),
            reason: EN.migration_differs.to_string(),
        }));
        assert!(again.join("set_1.csv").exists());
        assert_eq!(rows(&db), rows_after);
        assert_eq!(in_database(&db).await, matches_after);
        std::fs::remove_dir_all(&again).unwrap();

        // the same match with another opponent: left, told
        copy_dir(&folder, &again);
        let descriptor = again.join("match.json");
        let mut json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&descriptor).unwrap()).unwrap();
        json["opponent"] = "Renamed".into();
        std::fs::write(&descriptor, serde_json::to_vec(&json).unwrap()).unwrap();
        let migrated = device.migrate(&db).await;
        assert!(migrated.left.contains(&NotRead {
            folder: format!("{team}/2025-10-04_rivali_pisa"),
            reason: EN.migration_differs.to_string(),
        }));
        assert_eq!(rows(&db), rows_after);
        std::fs::remove_dir_all(&again).unwrap();

        // the same match with a player less in its roster: left, told
        copy_dir(&folder, &again);
        let descriptor = again.join("match.json");
        let mut json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&descriptor).unwrap()).unwrap();
        json["roster"].as_array_mut().unwrap().pop().unwrap();
        std::fs::write(&descriptor, serde_json::to_vec(&json).unwrap()).unwrap();
        let migrated = device.migrate(&db).await;
        assert!(migrated.left.contains(&NotRead {
            folder: format!("{team}/2025-10-04_rivali_pisa"),
            reason: EN.migration_differs.to_string(),
        }));
        assert_eq!(rows(&db), rows_after);
        std::fs::remove_dir_all(&again).unwrap();

        // a match new to the database: moves, with its team's new player
        // (the team keeps its details)
        let new = device.data().join(&team).join("2025-12-01_rivali_pisa");
        copy_dir(&folder, &new);
        let descriptor = device.data().join(&team).join("team.json");
        let mut json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&descriptor).unwrap()).unwrap();
        json["name"] = "Renamed by the old version".into();
        let mut player = json["players"][0].clone();
        player["id"] = Uuid::new_v4().to_string().into();
        player["number"] = 77.into();
        json["players"].as_array_mut().unwrap().push(player);
        std::fs::write(&descriptor, serde_json::to_vec(&json).unwrap()).unwrap();
        let migrated = device.migrate(&db).await;
        assert_eq!((migrated.teams, migrated.matches), (1, 1));
        assert!(!new.exists());
        let team_id = Uuid::parse_str(&team).unwrap();
        let read: TeamEntry = SqliteTeamReader::new(db.clone())
            .read_single(&team_id)
            .await
            .unwrap();
        assert_ne!(read.name, "Renamed by the old version");
        assert!(read.players.iter().any(|p| p.number == 77 && !p.deleted));
        let read = in_database(&db).await;
        assert_eq!(
            read[&(team_id, "2025-12-01_rivali_pisa".to_string())],
            read[&(team_id, "2025-10-04_rivali_pisa".to_string())],
        );
        assert_eq!(read.len(), matches_after.len() + 1);
    }

    #[tokio::test]
    async fn a_match_removed_in_the_database_doesnt_come_back() {
        let device = Device::of("v0.18.0");
        let db = database();
        device.migrate(&db).await;
        db.write(|tx| {
            tx.execute(
                "UPDATE matches SET deleted = 1 WHERE legacy_id = '2025-10-04_rivali_pisa'",
                [],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
        let rows_after = rows(&db);
        let fresh = Device::of("v0.18.0");
        let (team, folder) = first_match(&fresh.data());
        let again = device.data().join(&team).join("2025-10-04_rivali_pisa");
        std::fs::create_dir_all(again.parent().unwrap()).unwrap();
        std::fs::copy(
            fresh.data().join(&team).join("team.json"),
            device.data().join(&team).join("team.json"),
        )
        .unwrap();
        copy_dir(&folder, &again);
        let migrated = device.migrate(&db).await;
        assert_eq!((migrated.matches, migrated.already), (0, 1));
        assert!(!again.exists());
        assert_eq!(rows(&db), rows_after);
        assert!(!in_database(&db).await.contains_key(&(
            Uuid::parse_str(&team).unwrap(),
            "2025-10-04_rivali_pisa".into()
        )));
    }

    #[tokio::test]
    async fn a_person_already_in_the_database_keeps_her_details() {
        let device = Device::of("v0.18.0");
        let db = database();
        device.migrate(&db).await;
        let fresh = Device::of("v0.18.0");
        let (team, _) = first_match(&fresh.data());
        let json: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fresh.data().join(&team).join("team.json")).unwrap(),
        )
        .unwrap();
        let player = json["players"][0]["id"].as_str().unwrap().to_string();
        db.write(|tx| {
            tx.execute(
                "UPDATE persons SET name = 'Changed in the app' WHERE id = ?1",
                [&player],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
        // an old version makes a new team with that player
        let other = Uuid::new_v4().to_string();
        std::fs::create_dir(device.data().join(&other)).unwrap();
        std::fs::copy(
            fresh.data().join(&team).join("team.json"),
            device.data().join(&other).join("team.json"),
        )
        .unwrap();
        let migrated = device.migrate(&db).await;
        assert_eq!(migrated.teams, 1);
        assert!(!device.data().join(&other).exists());
        let read = SqliteTeamReader::new(db.clone())
            .read_single(&Uuid::parse_str(&other).unwrap())
            .await
            .unwrap();
        let person = read
            .players
            .iter()
            .find(|p| p.id.to_string() == player)
            .unwrap();
        assert_eq!(person.name, "Changed in the app");
    }

    /// The migration of a copy of a real data folder: `S4A_MIGRATE_DIR`
    /// (e.g. the inputs of `~/scout4all-riferimento-001`), never the
    /// folder itself. Run by hand:
    /// `S4A_MIGRATE_DIR=… cargo test -p scout4all-core migrate_a_copy -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn migrate_a_copy_of_a_real_folder() {
        let Ok(source) = std::env::var("S4A_MIGRATE_DIR") else {
            panic!("S4A_MIGRATE_DIR not set");
        };
        let dir = TempDir::new().unwrap();
        let data = dir.path().join("data");
        copy_dir(Path::new(&source), &data);
        std::fs::create_dir(dir.path().join("backups")).unwrap();
        let expected = in_files(&data).await;
        let db = Database::new(crate::db::open(&dir.path().join("scout4all.db")).unwrap());
        let backup = Backup {
            dir: dir.path().join("backups"),
            prefix: "data".into(),
        };
        let started = std::time::Instant::now();
        let migrated = migrate(&FsStore::new(&data), &db, &backup, EN)
            .await
            .unwrap();
        println!(
            "{source}: {} teams, {} matches, {} left in {:?}",
            migrated.teams,
            migrated.matches,
            migrated.left.len(),
            started.elapsed()
        );
        for left in &migrated.left {
            println!("  left {}: {}", left.folder, left.reason);
        }
        let read = in_database(&db).await;
        assert_eq!(read, expected);
        assert_eq!(scores(&read), scores(&expected));
    }
}
