//! The uploads read from the database (`providers::sqlite::upload`,
//! TODO/001 step 14, D14): every match of the legacy fixtures, migrated,
//! uploads what its files uploaded, under the same name; the queue of the
//! files moves into the database with the migration; B's side and a
//! neutral venue don't change the archive.

#[cfg(test)]
mod tests {
    use crate::{
        analytics::{
            queue::{PendingUpload, UploadQueue},
            queue_manager::QueueManager,
            upload::{
                ToUpload, UploadSource, UploadStep, keys_request, match_archive, upload_next,
            },
        },
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        errors::AppError,
        legacy::{
            folder::{read_folder, read_match, read_team},
            format::read_events,
            migrate::{Backup, Migrated, migrate, unzip},
        },
        providers::{
            match_writer::MatchWriter,
            queue_reader::QueueReader,
            queue_writer::QueueWriter,
            sqlite::{
                Database,
                matches::SqliteMatchWriter,
                queue::SqliteQueue,
                teams::{SqliteTeamReader, SqliteTeamWriter},
                upload::SqliteUploads,
            },
            team_reader::TeamReader,
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{player::PlayerEntry, set::SetEntry},
        storage::{Store, fs::FsStore, layout},
        tests::{
            EN, LEGACY_VERSIONS as VERSIONS, copy_dir, legacy_fixtures,
            sqlite_archive_tests::tests::{database, en, execute},
            upload_tests::tests::FakeApi,
        },
    };
    use async_trait::async_trait;
    use rusqlite::params;
    use std::{
        collections::BTreeMap,
        path::PathBuf,
        sync::{Arc, Mutex},
    };
    use tempfile::TempDir;
    use uuid::Uuid;

    /// A data folder of a version of the legacy fixtures (`data/`), and
    /// where the backups go.
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
            Device { dir }
        }

        fn data(&self) -> PathBuf {
            self.dir.path().join("data")
        }

        fn store(&self) -> Arc<dyn Store> {
            Arc::new(FsStore::new(&self.data()))
        }

        fn backups(&self) -> usize {
            std::fs::read_dir(self.dir.path())
                .unwrap()
                .filter(|e| e.as_ref().unwrap().path().is_file())
                .count()
        }

        async fn migrate(&self, db: &Database) -> Migrated {
            let backup = Backup {
                dir: self.dir.path().to_path_buf(),
                prefix: "backup".into(),
            };
            migrate(&FsStore::new(&self.data()), db, &backup, EN)
                .await
                .unwrap()
        }

        fn queue_file(&self) -> PathBuf {
            self.data().join("upload_queue.json")
        }

        fn write_queue(&self, pending: Vec<PendingUpload>) {
            std::fs::write(
                self.queue_file(),
                serde_json::to_vec_pretty(&UploadQueue::of(pending)).unwrap(),
            )
            .unwrap();
        }

        fn read_queue(&self) -> Vec<(Uuid, String, i64, u8)> {
            let queue: UploadQueue =
                serde_json::from_slice(&std::fs::read(self.queue_file()).unwrap()).unwrap();
            queued(&queue)
        }
    }

    fn queued(queue: &UploadQueue) -> Vec<(Uuid, String, i64, u8)> {
        queue
            .pending()
            .iter()
            .map(|p| (p.team_id, p.match_id.clone(), p.queued_at, p.retry_count))
            .collect()
    }

    fn pending(team_id: Uuid, match_id: &str, queued_at: i64, retry_count: u8) -> PendingUpload {
        PendingUpload {
            team_id,
            match_id: match_id.to_string(),
            queued_at,
            retry_count,
        }
    }

    /// The matches of a data folder of the versions before the database,
    /// as they uploaded them: the files of the sets as saved.
    struct FileUploads(Arc<dyn Store>);

    #[async_trait]
    impl UploadSource for FileUploads {
        async fn read(&self, team_id: &Uuid, match_id: &str) -> Result<ToUpload, AppError> {
            let store = &*self.0;
            let team = read_team(store, &team_id.to_string(), EN).await?;
            let entry = read_match(store, &team, match_id, EN).await?;
            let mut set_files = Vec::new();
            for set in &entry.sets {
                let n = set.set_number;
                let files = [
                    (
                        layout::set_descriptor(team_id, match_id, n),
                        format!("set_{n}_descriptor.json"),
                    ),
                    (
                        layout::set_events(team_id, match_id, n),
                        format!("set_{n}_events.csv"),
                    ),
                ];
                for (key, name) in files {
                    if let Some(contents) = store.read(&key).await? {
                        set_files.push((name, contents));
                    }
                }
            }
            Ok(ToUpload {
                file_id: entry.id.clone(),
                set_files,
                entry,
            })
        }
    }

    /// The queue of a data folder of the versions before the database: its
    /// file, kept in memory.
    struct FileQueue(Mutex<Vec<u8>>);

    impl FileQueue {
        fn of(device: &Device) -> Self {
            FileQueue(Mutex::new(std::fs::read(device.queue_file()).unwrap()))
        }
    }

    #[async_trait]
    impl QueueReader for FileQueue {
        async fn load(&self) -> Result<UploadQueue, AppError> {
            Ok(serde_json::from_slice(&self.0.lock().unwrap()).unwrap())
        }
    }

    #[async_trait]
    impl QueueWriter for FileQueue {
        async fn save(&self, queue: &UploadQueue) -> Result<(), AppError> {
            *self.0.lock().unwrap() = serde_json::to_vec(queue).unwrap();
            Ok(())
        }
    }

    /// Every match of the files, by team and folder, as uploaded from
    /// them.
    async fn uploads_of_files(store: Arc<dyn Store>) -> BTreeMap<(Uuid, String), ToUpload> {
        let source = FileUploads(Arc::clone(&store));
        let mut all = BTreeMap::new();
        for team in read_folder(&*store, EN).await.unwrap() {
            let team = team.unwrap();
            for m in team.matches {
                let m = m.unwrap();
                let upload = source.read(&team.team.id, &m.id).await.unwrap();
                all.insert((team.team.id, m.id), upload);
            }
        }
        all
    }

    /// The match moved from the folder `legacy_id` of team `team`.
    fn moved(db: &Database, team: Uuid, legacy_id: &str) -> Uuid {
        db.read(|tx| {
            tx.query_row(
                "SELECT id FROM matches WHERE legacy_team = ?1 AND legacy_id = ?2",
                params![team.to_string(), legacy_id],
                |r| r.get::<_, String>(0),
            )
            .map_err(crate::providers::sqlite::sqlite)
        })
        .map(|id| Uuid::parse_str(&id).unwrap())
        .unwrap()
    }

    /// The files of the archive of an upload, in their order.
    fn archive(upload: &ToUpload) -> Vec<(String, Vec<u8>)> {
        let zip = match_archive(&upload.entry, upload.set_files.clone(), None).unwrap();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
        (0..zip.len())
            .map(|i| {
                let mut file = zip.by_index(i).unwrap();
                let mut data = Vec::new();
                std::io::Read::read_to_end(&mut file, &mut data).unwrap();
                (file.name().unwrap().to_string(), data)
            })
            .collect()
    }

    /// What the archive tells: `match.json`, then each set as read back
    /// (the lineup, the events).
    fn told(files: &[(String, Vec<u8>)]) -> Vec<(String, String)> {
        files
            .iter()
            .map(|(name, data)| {
                let read = match name.rsplit_once('.') {
                    Some((_, "csv")) => format!("{:?}", read_events(data).unwrap()),
                    _ if name.starts_with("set_") => {
                        format!("{:?}", serde_json::from_slice::<SetEntry>(data).unwrap())
                    }
                    _ => String::from_utf8(data.clone()).unwrap(),
                };
                (name.clone(), read)
            })
            .collect()
    }

    /// The versions that saved the events without the flags.
    fn without_flags(version: &str) -> bool {
        matches!(version, "v0.9.0" | "v0.10.1")
    }

    fn with_empty_flags(csv: &[u8]) -> Vec<u8> {
        String::from_utf8(csv.to_vec())
            .unwrap()
            .lines()
            .map(|line| format!("{line},\n"))
            .collect::<String>()
            .into_bytes()
    }

    #[tokio::test]
    async fn every_migrated_match_uploads_what_its_files_uploaded() {
        let (mut total, mut padded) = (0, 0);
        for version in VERSIONS {
            let device = Device::of(version);
            let from_files = uploads_of_files(device.store()).await;
            assert!(!from_files.is_empty(), "{version}");
            let db = database();
            device.migrate(&db).await;
            let source = SqliteUploads::new(db.clone(), en());
            for ((team, folder), file) in &from_files {
                let id = moved(&db, *team, folder);
                let upload = source.read(team, &id.to_string()).await.unwrap();
                let at = format!("{version} {folder}");
                // the same name of the file, the same names to the API
                assert_eq!(upload.file_id, *folder, "{at}");
                assert_eq!(upload.entry.team.id, *team, "{at}");
                assert_eq!(
                    format!("{:?}", keys_request(&upload.entry)),
                    format!("{:?}", keys_request(&file.entry)),
                    "{at}"
                );
                let (ours, theirs) = (archive(&upload), archive(file));
                assert_eq!(told(&ours), told(&theirs), "{at}");
                // byte for byte, but the rows saved without the flags (up
                // to v0.10.1): they get the column, empty
                assert_eq!(ours.len(), theirs.len(), "{at}");
                for ((name, ours), (_, theirs)) in ours.iter().zip(&theirs) {
                    match (ours == theirs, without_flags(version)) {
                        (true, _) => {}
                        (false, true) => {
                            assert!(name.ends_with(".csv"), "{at} {name}");
                            assert_eq!(*ours, with_empty_flags(theirs), "{at} {name}");
                            padded += 1;
                        }
                        (false, false) => panic!("{at} {name}: not the same bytes"),
                    }
                }
                total += 1;
            }
        }
        assert_eq!(total, 33);
        assert!(padded > 0);
    }

    #[tokio::test]
    async fn the_queue_of_the_files_moves_with_the_migration() {
        for version in VERSIONS {
            let device = Device::of(version);
            let saved = match device.queue_file().exists() {
                true => device.read_queue(),
                false => vec![],
            };
            // since v0.12.0
            assert_eq!(
                saved.is_empty(),
                matches!(version, "v0.9.0" | "v0.10.1" | "v0.11.0"),
                "{version}"
            );
            let db = database();
            device.migrate(&db).await;
            assert!(!device.queue_file().exists(), "{version}");
            let expected: Vec<_> = saved
                .iter()
                .map(|(team, folder, at, retry)| {
                    (*team, moved(&db, *team, folder).to_string(), *at, *retry)
                })
                .collect();
            let queue = SqliteQueue::new(db.clone()).load().await.unwrap();
            assert_eq!(queued(&queue), expected, "{version}");
        }
    }

    const TEAM: &str = "f79ccc8b-326d-4b74-bc04-8bac9d08178e";
    const U19_TEAM: &str = "abae2039-fa19-4da6-9021-8499a3aaccd6";
    const VISITING_TEAM: &str = "15695ad8-cfbc-418f-b7b1-0d3d9860e9be";

    fn id(text: &str) -> Uuid {
        Uuid::parse_str(text).unwrap()
    }

    /// The queue of the database: team, match, when, attempts.
    async fn in_database(db: &Database) -> Vec<(Uuid, String, i64, u8)> {
        queued(&SqliteQueue::new(db.clone()).load().await.unwrap())
    }

    /// A match moved goes after the queue of the database, once; one
    /// still in its folder stays in the file until it moves; one gone or
    /// removed leaves the queue. Without a backup when nothing else moves.
    #[tokio::test]
    async fn each_upload_of_the_file_moves_once_or_stays_or_goes() {
        let device = Device::of("v0.18.0");
        let (team, visiting) = (id(TEAM), id(VISITING_TEAM));
        device.write_queue(vec![
            pending(team, "2025-10-04_rivali_pisa", 100, 2),
            pending(team, "2020-01-01_gone", 101, 0),
            pending(team, "2025-10-11_polisportiva_lucca", 102, 1),
            pending(visiting, "2025-10-25_visiting", 103, 0),
        ]);
        // a match that can't be read stays in its folder
        let lucca = device
            .data()
            .join(TEAM)
            .join("2025-10-11_polisportiva_lucca")
            .join("match.json");
        let lucca_saved = std::fs::read(&lucca).unwrap();
        std::fs::write(&lucca, b"{").unwrap();
        let db = database();
        let first = device.migrate(&db).await;
        assert!(first.left.iter().any(|n| n.folder.ends_with("lucca")));
        let pisa = moved(&db, team, "2025-10-04_rivali_pisa").to_string();
        let visiting_id = moved(&db, visiting, "2025-10-25_visiting").to_string();
        assert_eq!(
            in_database(&db).await,
            vec![
                (team, pisa.clone(), 100, 2),
                (visiting, visiting_id.clone(), 103, 0)
            ]
        );
        assert_eq!(
            device.read_queue(),
            vec![(team, "2025-10-11_polisportiva_lucca".into(), 102, 1)]
        );
        // a run with nothing to move leaves the file as it is
        let file = std::fs::read(device.queue_file()).unwrap();
        device.migrate(&db).await;
        assert_eq!(std::fs::read(device.queue_file()).unwrap(), file);

        // readable again: it moves, after the others
        std::fs::write(&lucca, lucca_saved).unwrap();
        device.migrate(&db).await;
        let lucca_id = moved(&db, team, "2025-10-11_polisportiva_lucca").to_string();
        let queue = vec![
            (team, pisa.clone(), 100, 2),
            (visiting, visiting_id, 103, 0),
            (team, lucca_id, 102, 1),
        ];
        assert_eq!(in_database(&db).await, queue);
        assert!(!device.queue_file().exists());

        // an old version writes the queue again: a match already queued
        // keeps its place and attempts, a match removed in the database
        // goes; no backup, nothing else to move
        let backups = device.backups();
        let u19 = moved(&db, id(U19_TEAM), "2025-11-15_rivali_pisa_u19");
        execute(
            &db,
            &format!("UPDATE matches SET deleted = 1 WHERE id = '{u19}'"),
        );
        device.write_queue(vec![
            pending(team, "2025-10-04_rivali_pisa", 200, 9),
            pending(id(U19_TEAM), "2025-11-15_rivali_pisa_u19", 201, 0),
        ]);
        let migrated = device.migrate(&db).await;
        assert_eq!(migrated.backup, None);
        assert_eq!(device.backups(), backups);
        assert_eq!(in_database(&db).await, queue);
        assert!(!device.queue_file().exists());
    }

    /// A queue that can't be read stays, told; the rest moves.
    #[tokio::test]
    async fn a_queue_that_cant_be_read_stays() {
        let device = Device::of("v0.18.0");
        std::fs::write(device.queue_file(), b"[").unwrap();
        let db = database();
        let migrated = device.migrate(&db).await;
        assert_eq!(migrated.matches, 6);
        assert!(
            migrated
                .left
                .iter()
                .any(|n| n.folder == "upload_queue.json"),
            "{:?}",
            migrated.left
        );
        assert_eq!(std::fs::read(device.queue_file()).unwrap(), b"[");
        assert_eq!(in_database(&db).await, vec![]);
    }

    /// Everything in the queue, uploaded through the API: the requests and
    /// the archives.
    async fn upload_all<QR, QW>(
        source: &impl UploadSource,
        queue: &QueueManager<QR, QW>,
    ) -> (Vec<String>, Vec<Vec<(String, Vec<u8>)>>)
    where
        QR: QueueReader + Send + Sync,
        QW: QueueWriter + Send + Sync,
    {
        let api = FakeApi::start().await;
        loop {
            match upload_next(source, queue, &api.url).await.unwrap() {
                UploadStep::Uploaded => continue,
                UploadStep::Empty => break,
                step => panic!("{step:?}"),
            }
        }
        let archives = api
            .archives
            .lock()
            .unwrap()
            .iter()
            .map(|zip| unzip(zip).unwrap())
            .collect();
        (api.requests(), archives)
    }

    #[tokio::test]
    async fn the_queue_of_the_database_uploads_as_the_files_did() {
        let files = Device::of("v0.18.0");
        let queue = Arc::new(FileQueue::of(&files));
        let queue = QueueManager::new(Arc::clone(&queue), queue);
        let (requests, archives) = upload_all(&FileUploads(files.store()), &queue).await;
        assert_eq!(archives.len(), 2);

        let device = Device::of("v0.18.0");
        let db = database();
        device.migrate(&db).await;
        let queue = QueueManager::new(
            Arc::new(SqliteQueue::new(db.clone())),
            Arc::new(SqliteQueue::new(db.clone())),
        );
        let (db_requests, db_archives) =
            upload_all(&SqliteUploads::new(db.clone(), en()), &queue).await;
        // the same files, by name, in the same order, with the same content
        assert_eq!(db_requests, requests);
        assert_eq!(db_archives, archives);
        assert_eq!(in_database(&db).await, vec![]);
    }

    #[tokio::test]
    async fn a_match_created_in_the_database_is_named_by_its_id() {
        let db = migrated_v018().await;
        let team = SqliteTeamReader::new(db.clone())
            .read_single(&id(TEAM))
            .await
            .unwrap();
        let m = SqliteMatchWriter::new(db.clone())
            .create(
                &team,
                crate::match_form::MatchInput::against(
                    "Cortona Volley",
                    chrono::Utc::now().fixed_offset(),
                    true,
                    DEFAULT_MAX_SUBSTITUTIONS,
                    "",
                    team.players.iter().map(|p| p.id).collect(),
                ),
            )
            .await
            .unwrap();
        let upload = SqliteUploads::new(db.clone(), en())
            .read(&team.id, &m.id)
            .await
            .unwrap();
        assert_eq!(upload.file_id, m.id);
        assert!(upload.set_files.is_empty());
        // of another team A: not read
        assert!(
            SqliteUploads::new(db.clone(), en())
                .read(&id(VISITING_TEAM), &m.id)
                .await
                .is_err()
        );
    }

    async fn migrated_v018() -> Database {
        let device = Device::of("v0.18.0");
        let db = database();
        device.migrate(&db).await;
        db
    }

    #[tokio::test]
    async fn a_match_removed_leaves_the_queue_unsent() {
        let db = migrated_v018().await;
        let queue = QueueManager::new(
            Arc::new(SqliteQueue::new(db.clone())),
            Arc::new(SqliteQueue::new(db.clone())),
        );
        let first = in_database(&db).await[0].1.clone();
        execute(
            &db,
            &format!("UPDATE matches SET deleted = 1 WHERE id = '{first}'"),
        );
        let api = FakeApi::start().await;
        let source = SqliteUploads::new(db.clone(), en());
        assert_eq!(
            upload_next(&source, &queue, &api.url).await.unwrap(),
            UploadStep::Dropped
        );
        assert_eq!(api.requests(), Vec::<String>::new());
        assert_eq!(in_database(&db).await.len(), 1);
    }

    /// Team B scouted too (`"both"`), and a neutral venue: the archive is
    /// A's, as before; `home` is false.
    #[tokio::test]
    async fn the_side_of_b_and_a_neutral_venue_dont_go() {
        let db = migrated_v018().await;
        let team = id(TEAM);
        let m = moved(&db, team, "2025-10-04_rivali_pisa");
        let source = SqliteUploads::new(db.clone(), en());
        let before = archive(&source.read(&team, &m.to_string()).await.unwrap());
        let b = SqliteTeamWriter::new(db.clone())
            .save(TeamInput::New {
                id: None,
                name: "Team B".into(),
                year: 2025,
                classification: None,
                gender: None,
                country: None,
                players: (1..=7)
                    .map(|n| PlayerEntry {
                        id: Uuid::new_v4(),
                        name: format!("b {n}"),
                        role: None,
                        number: n,
                        deleted: false,
                        birth_date: None,
                        height_cm: None,
                        notes: String::new(),
                    })
                    .collect(),
            })
            .await
            .unwrap();
        let p = |n: usize| b.players[n].id;
        execute(
            &db,
            &format!(
                "UPDATE matches SET team_b = '{b}', team_b_name = NULL, scouting = 'both',
                     home = NULL WHERE id = '{m}';
                 INSERT INTO set_lineups (match_id, set_number, side, p1, p2, p3, p4, p5, p6,
                                          libero, setter)
                     SELECT match_id, number, 'b', '{p0}', '{p1}', '{p2}', '{p3}', '{p4}',
                            '{p5}', '{p6}', '{p0}'
                     FROM sets WHERE match_id = '{m}';
                 INSERT INTO events (id, match_id, set_number, seq, side, timestamp, event_type,
                                     player, eval, technique, flags)
                     SELECT lower(printf('%s-%s-%s-%s-%s', hex(randomblob(4)),
                                         hex(randomblob(2)), hex(randomblob(2)),
                                         hex(randomblob(2)), hex(randomblob(6)))),
                            match_id, set_number, seq + 100000, 'b', timestamp, 'S', '{p0}',
                            '#', 'F', 0
                     FROM events WHERE match_id = '{m}';",
                b = b.id,
                p0 = p(0),
                p1 = p(1),
                p2 = p(2),
                p3 = p(3),
                p4 = p(4),
                p5 = p(5),
                p6 = p(6),
            ),
        );
        let b_events: i64 = db
            .read(|tx| {
                tx.query_row("SELECT count(*) FROM events WHERE side = 'b'", [], |r| {
                    r.get(0)
                })
                .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        assert!(b_events > 0);
        let upload = source.read(&team, &m.to_string()).await.unwrap();
        assert_eq!(upload.entry.opponent, "Team B");
        let after = archive(&upload);
        let metadata = |files: &[(String, Vec<u8>)]| -> serde_json::Value {
            serde_json::from_slice(&files[0].1).unwrap()
        };
        let mut expected = metadata(&before);
        assert_eq!(expected["home"], true);
        expected["home"] = serde_json::Value::Bool(false);
        assert_eq!(metadata(&after), expected);
        assert_eq!(after[1..], before[1..]);
    }
}
