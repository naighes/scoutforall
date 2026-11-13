//! The archives of the database (`providers::sqlite::archive`, TODO/001
//! step 12, D10): every team and match of the legacy fixtures goes and
//! comes back the same; what's removed doesn't go; the local data win, a
//! match here is never overwritten, what's removed here comes back; an
//! archive the schema refuses, that doesn't replay, or newer, changes
//! nothing.

#[cfg(test)]
mod tests {
    use crate::{
        archive::zip_files,
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        errors::{AppError, IOError},
        legacy::migrate::{Backup, migrate, unzip},
        localization::Language,
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            set_writer::SetWriter,
            sqlite::{
                Database,
                archive::{
                    FORMAT, Imported, Kind, MANIFEST, Manifest, Merged, Subject, export, import,
                    manifest,
                },
                matches::{SqliteMatchReader, SqliteMatchWriter},
                sets::SqliteSetWriter,
                teams::{SqliteTeamReader, SqliteTeamWriter},
            },
            team_reader::TeamReader,
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{
            enums::{EvalEnum, EventTypeEnum, LanguageEnum, RoleEnum, TeamSideEnum},
            r#match::MatchEntry,
            player::PlayerEntry,
            snapshot::EventEntry,
            team::TeamEntry,
            technique::{ServeTypeEnum, TechniqueEnum},
        },
        storage::fs::FsStore,
        tests::{EN, LEGACY_VERSIONS as VERSIONS, copy_dir, legacy_fixtures},
    };
    use chrono::Utc;
    use serde_json::Value as Json;
    use std::collections::BTreeMap;
    use tempfile::TempDir;
    use uuid::Uuid;
    use zip::CompressionMethod::Deflated;

    fn en() -> Language {
        Language::new(LanguageEnum::En)
    }

    fn database() -> Database {
        Database::new(crate::db::open_in_memory().unwrap())
    }

    /// The data folder of a version of the legacy fixtures, migrated.
    async fn migrated(version: &str) -> Database {
        let dir = TempDir::new().unwrap();
        copy_dir(
            &legacy_fixtures(version).join("data"),
            &dir.path().join("data"),
        );
        let db = database();
        let backup = Backup {
            dir: dir.path().to_path_buf(),
            prefix: "backup".into(),
        };
        migrate(&FsStore::new(&dir.path().join("data")), &db, &backup, EN)
            .await
            .unwrap();
        db
    }

    /// Every team with its matches, as the providers read them; every
    /// match replays.
    async fn contents(db: &Database) -> BTreeMap<Uuid, (TeamEntry, Vec<MatchEntry>)> {
        let teams = SqliteTeamReader::new(db.clone());
        let matches = SqliteMatchReader::new(db.clone(), en());
        let mut all = BTreeMap::new();
        for team in teams.read_all().await.unwrap() {
            let team = team.unwrap();
            let mut read: Vec<MatchEntry> = matches
                .read_all_with_status(&team)
                .await
                .unwrap()
                .into_iter()
                .map(|m| m.unwrap().0)
                .collect();
            read.sort_by(|a, b| a.id.cmp(&b.id));
            all.insert(team.id, (team, read));
        }
        all
    }

    /// Every row of every table, as text: to tell nothing changed.
    fn rows(db: &Database) -> Vec<String> {
        db.read(|tx| {
            let tables: Vec<String> = tx
                .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
                .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
                .map_err(crate::providers::sqlite::sqlite)?;
            let mut all = Vec::new();
            for table in tables {
                let mut s = tx
                    .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
                    .map_err(crate::providers::sqlite::sqlite)?;
                let n = s.column_count();
                let read: Vec<String> = s
                    .query_map([], |r| {
                        (0..n)
                            .map(|i| r.get_ref(i).map(|v| format!("{v:?}")))
                            .collect::<Result<Vec<_>, _>>()
                            .map(|v| format!("{table}: {}", v.join(", ")))
                    })
                    .and_then(|rows| rows.collect())
                    .map_err(crate::providers::sqlite::sqlite)?;
                all.extend(read);
            }
            Ok(all)
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

    /// The files of an archive, parsed.
    fn files(archive: &[u8]) -> BTreeMap<String, Json> {
        unzip(archive)
            .unwrap()
            .into_iter()
            .map(|(name, data)| (name, serde_json::from_slice(&data).unwrap()))
            .collect()
    }

    fn zipped(files: &BTreeMap<String, Json>) -> Vec<u8> {
        let files: Vec<(String, Vec<u8>)> = files
            .iter()
            .map(|(name, json)| (name.clone(), serde_json::to_vec(json).unwrap()))
            .collect();
        zip_files(&files, Deflated).unwrap()
    }

    /// The rows of a table of an archive.
    fn table<'a>(files: &'a mut BTreeMap<String, Json>, name: &str) -> &'a mut Vec<Json> {
        files
            .get_mut(&format!("{name}.json"))
            .unwrap()
            .as_array_mut()
            .unwrap()
    }

    fn message(e: AppError) -> String {
        match e {
            AppError::IO(IOError::Msg(m)) => m,
            other => format!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn every_team_and_match_goes_and_comes_back() {
        for version in VERSIONS {
            let db = migrated(version).await;
            let all = contents(&db).await;
            assert!(!all.is_empty(), "{version}");
            // all the teams into one database: the persons shared
            let together = database();
            for (id, (team, matches)) in &all {
                let archive = export(&db, Subject::TeamWithMatches(*id)).unwrap();
                // alone: the team and its matches (its opponents are names)
                let alone = database();
                let imported = import(&alone, &archive, &en()).unwrap();
                assert_eq!(imported.kind, Kind::Team, "{version}");
                assert_eq!(imported.matches, matches.len(), "{version}");
                let back = contents(&alone).await;
                assert_eq!(back.len(), 1, "{version}");
                assert_eq!(
                    back.get(id),
                    Some(&(team.clone(), matches.clone())),
                    "{version}"
                );
                import(&together, &archive, &en()).unwrap();
                // each match on its own, with its team
                for m in matches {
                    let uuid = Uuid::parse_str(&m.id).unwrap();
                    let archive = export(&db, Subject::Match(uuid)).unwrap();
                    let alone = database();
                    let imported = import(&alone, &archive, &en()).unwrap();
                    assert_eq!((imported.kind, imported.id), (Kind::Match, uuid));
                    let back = contents(&alone).await;
                    let (team_back, matches_back) = back.get(id).unwrap();
                    assert_eq!(team_back, team, "{version} {}", m.id);
                    assert_eq!(matches_back, &vec![m.clone()], "{version} {}", m.id);
                }
                // the team without them
                let archive = export(&db, Subject::Team(*id)).unwrap();
                let alone = database();
                let imported = import(&alone, &archive, &en()).unwrap();
                assert_eq!(imported.matches, 0);
                let back = contents(&alone).await;
                // the players removed that no match tells of aren't shown
                // by the database: the archive doesn't have the matches
                let active = |t: &TeamEntry| {
                    t.players
                        .iter()
                        .filter(|p| !p.deleted)
                        .cloned()
                        .collect::<Vec<_>>()
                };
                let (team_back, matches_back) = back.get(id).unwrap();
                assert_eq!(active(team_back), active(team), "{version}");
                assert!(matches_back.is_empty());
            }
            assert_eq!(contents(&together).await, all, "{version}");
            // the summaries of the lists: written by the import
            let cached = |db: &Database| -> i64 {
                db.read(|tx| {
                    tx.query_row("SELECT count(*) FROM match_summary", [], |r| r.get(0))
                        .map_err(crate::providers::sqlite::sqlite)
                })
                .unwrap()
            };
            let matches: usize = all.values().map(|(_, m)| m.len()).sum();
            assert_eq!(
                cached(&together),
                i64::try_from(matches).unwrap(),
                "{version}"
            );
        }
    }

    #[tokio::test]
    async fn the_manifest_tells_what_the_archive_is() {
        let db = migrated("v0.18.0").await;
        let (id, (_, matches)) = contents(&db).await.into_iter().next().unwrap();
        let archive = export(&db, Subject::TeamWithMatches(id)).unwrap();
        let read = files(&archive);
        let names: Vec<&str> = read.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            [
                "competitions.json",
                "events.json",
                MANIFEST,
                "match_roster.json",
                "matches.json",
                "persons.json",
                "set_lineups.json",
                "sets.json",
                "team_members.json",
                "teams.json",
            ]
        );
        let m: Manifest = manifest(&archive).unwrap();
        assert_eq!(
            (m.format, m.schema, m.kind, m.id, m.app_version.as_str()),
            (
                FORMAT,
                crate::db::schema_version(),
                Kind::Team,
                id,
                env!("CARGO_PKG_VERSION")
            )
        );
        assert!((Utc::now() - m.exported_at).num_seconds() < 60);
        // readable: the words of the database
        let first = &read["matches.json"][0];
        assert_eq!(first["scouting"], "a");
        assert_eq!(first["team_a"], id.to_string());
        assert_eq!(
            read["matches.json"].as_array().unwrap().len(),
            matches.len()
        );
        let m = Uuid::parse_str(&matches[0].id).unwrap();
        let archive = export(&db, Subject::Match(m)).unwrap();
        assert_eq!(
            manifest(&archive).map(|m| (m.kind, m.id)),
            Some((Kind::Match, m))
        );
        // the archives of the versions before the database have none
        let old = legacy_fixtures("v0.18.0").join("export");
        for entry in std::fs::read_dir(old).unwrap() {
            let archive = std::fs::read(entry.unwrap().path()).unwrap();
            assert_eq!(manifest(&archive), None);
            let e = message(import(&database(), &archive, &en()).unwrap_err());
            assert!(e.starts_with(EN.invalid_archive), "{e}");
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
            OutsideHitter,
        ]
        .into_iter()
        .zip(1..)
        .map(|(role, n)| PlayerEntry {
            id: Uuid::new_v4(),
            name: format!("player {n}"),
            role: Some(role),
            number: n,
            deleted: false,
            birth_date: chrono::NaiveDate::from_ymd_opt(2000, 1, n.into()),
            height_cm: Some(180),
            notes: format!("notes {n}"),
        })
        .collect()
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

    struct Scouted {
        db: Database,
        team: TeamEntry,
        m: MatchEntry,
        /// the players as created
        ids: Vec<Uuid>,
    }

    impl Scouted {
        fn match_id(&self) -> Uuid {
            Uuid::parse_str(&self.m.id).unwrap()
        }
    }

    /// A team and a match: set 1 with an event undone, set 2 removed, a
    /// match removed; player 2 played and was removed since, player 8
    /// removed and never on the roster.
    async fn scouted() -> Scouted {
        let db = database();
        let teams = SqliteTeamWriter::new(db.clone());
        let matches = SqliteMatchWriter::new(db.clone());
        let sets = SqliteSetWriter::new(db.clone(), en());
        let mut team = teams
            .save(TeamInput::New {
                id: None,
                name: "Volley Club".into(),
                year: 2025,
                classification: None,
                gender: None,
                country: None,
                players: players(),
            })
            .await
            .unwrap();
        let ids: Vec<Uuid> = team.players.iter().map(|p| p.id).collect();
        let create = |opponent: &str| {
            matches.create(
                &team,
                opponent.into(),
                Utc::now().fixed_offset(),
                true,
                DEFAULT_MAX_SUBSTITUTIONS,
                String::new(),
                ids[..7].to_vec(),
            )
        };
        let m = create("Cortona Volley").await.unwrap();
        let gone = create("Removed Volley").await.unwrap();
        for set in [1, 2] {
            sets.create(
                &m,
                set,
                TeamSideEnum::Us,
                ids[..6].try_into().unwrap(),
                ids[6],
                None,
                ids[0],
                vec![serve(ids[0]), their_error(), serve(ids[0])],
            )
            .await
            .unwrap();
        }
        sets.remove_last_event(&m, 1).await.unwrap();
        sets.remove(&m, 2).await.unwrap();
        matches.remove(&team.id, &gone.id).await.unwrap();
        teams.remove_player(ids[1], &mut team).await.unwrap();
        teams.remove_player(ids[7], &mut team).await.unwrap();
        let team = SqliteTeamReader::new(db.clone())
            .read_single(&team.id)
            .await
            .unwrap();
        let m = SqliteMatchReader::new(db.clone(), en())
            .read_single(&team, &m.id)
            .await
            .unwrap();
        Scouted { db, team, m, ids }
    }

    #[tokio::test]
    async fn what_is_removed_does_not_go() {
        let s = scouted().await;
        let read = files(&export(&s.db, Subject::TeamWithMatches(s.team.id)).unwrap());
        let column = |name: &str, column: &str| -> Vec<Json> {
            read[&format!("{name}.json")]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row[column].clone())
                .collect()
        };
        assert_eq!(column("matches", "id"), [Json::from(s.m.id.clone())]);
        assert_eq!(column("sets", "number"), [Json::from(1)]);
        assert_eq!(column("set_lineups", "set_number"), [Json::from(1)]);
        // the event undone keeps its place, out of the archive
        assert_eq!(column("events", "seq"), [Json::from(0), Json::from(1)]);
        assert!(column("events", "deleted").iter().all(|d| d == 0));
        assert_eq!(s.team.players.len(), 7);
        let player = |n: usize| Json::from(s.ids[n].to_string());
        // player 8, removed and never played, isn't there
        let members = column("team_members", "person_id");
        assert_eq!(members.len(), 7);
        assert!(!members.contains(&player(7)));
        assert!(members.contains(&player(1)));
        let persons = read["persons.json"].as_array().unwrap();
        assert_eq!(persons.len(), 7);
        let person = |n: usize| persons.iter().find(|p| p["id"] == player(n)).unwrap();
        // player 2, removed: only the name
        assert_eq!(
            (
                &person(1)["name"],
                &person(1)["birth_date"],
                &person(1)["height_cm"],
                &person(1)["notes"]
            ),
            (
                &Json::from("player 2"),
                &Json::Null,
                &Json::Null,
                &Json::from("")
            )
        );
        assert_eq!(person(0)["notes"], "notes 1");
        assert_eq!(person(0)["birth_date"], "2000-01-01");
        // and back: the same, nothing is told different
        let archive = export(&s.db, Subject::TeamWithMatches(s.team.id)).unwrap();
        let before = rows(&s.db);
        let imported = import(&s.db, &archive, &en()).unwrap();
        let same = |n: usize| Merged {
            added: 0,
            same: n,
            kept_local: 0,
        };
        assert_eq!(
            imported,
            Imported {
                kind: Kind::Team,
                id: s.team.id,
                persons: same(7),
                teams: same(1),
                members: same(7),
                competitions: same(0),
                matches: 0,
                matches_here: 1,
            }
        );
        assert_eq!(rows(&s.db), before);
        // a set removed whose events aren't (as the sync may bring them)
        execute(&s.db, "UPDATE sets SET deleted = 1");
        let read = files(&export(&s.db, Subject::Match(s.match_id())).unwrap());
        assert!(read["events.json"].as_array().unwrap().is_empty());
        assert!(read["set_lineups.json"].as_array().unwrap().is_empty());
        execute(&s.db, "UPDATE sets SET deleted = 0 WHERE number = 1");
        // elsewhere: the removed player only with the name
        let other = database();
        import(&other, &archive, &en()).unwrap();
        let (team, matches) = contents(&other).await.remove(&s.team.id).unwrap();
        assert_eq!(team, s.team);
        assert_eq!(matches, vec![s.m.clone()]);
    }

    #[tokio::test]
    async fn a_match_here_is_never_overwritten() {
        let s = scouted().await;
        let archive = export(&s.db, Subject::Match(s.match_id())).unwrap();
        let other = database();
        import(&other, &archive, &en()).unwrap();
        let before = rows(&other);
        // the archive of the match: refused
        for db in [&s.db, &other] {
            let e = message(import(db, &archive, &en()).unwrap_err());
            assert_eq!(e, EN.match_already_exists);
        }
        assert_eq!(rows(&other), before);
        // also changed there since: still refused, nothing changes
        let mut changed = files(&archive);
        table(&mut changed, "matches")[0]["notes"] = Json::from("changed");
        table(&mut changed, "persons")[0]["name"] = Json::from("someone else");
        let e = message(import(&other, &zipped(&changed), &en()).unwrap_err());
        assert_eq!(e, EN.match_already_exists);
        assert_eq!(rows(&other), before);
        // the archive of a team: the match counted and left
        let archive = export(&s.db, Subject::TeamWithMatches(s.team.id)).unwrap();
        let imported = import(&other, &archive, &en()).unwrap();
        assert_eq!((imported.matches, imported.matches_here), (0, 1));
        assert_eq!(rows(&other), before);
    }

    #[tokio::test]
    async fn where_it_was_tells_the_same_match() {
        // two devices migrated the same folder: other ids, the same match
        let one = migrated("v0.18.0").await;
        let two = migrated("v0.18.0").await;
        let (id, (_, matches)) = contents(&one).await.into_iter().next().unwrap();
        let m = Uuid::parse_str(&matches[0].id).unwrap();
        let (_, there) = contents(&two).await.remove(&id).unwrap();
        assert!(there.iter().all(|t| t.id != matches[0].id));
        let before = rows(&two);
        let archive = export(&one, Subject::Match(m)).unwrap();
        let e = message(import(&two, &archive, &en()).unwrap_err());
        assert_eq!(e, EN.match_already_exists);
        assert_eq!(rows(&two), before);
        let archive = export(&one, Subject::TeamWithMatches(id)).unwrap();
        let imported = import(&two, &archive, &en()).unwrap();
        assert_eq!(
            (imported.matches, imported.matches_here),
            (0, matches.len())
        );
    }

    #[tokio::test]
    async fn the_local_data_win() {
        let s = scouted().await;
        let archive = export(&s.db, Subject::TeamWithMatches(s.team.id)).unwrap();
        let other = database();
        import(&other, &archive, &en()).unwrap();
        let player = s.team.players[0].id;
        execute(
            &other,
            &format!(
                "UPDATE teams SET name = 'Renamed' WHERE id = '{t}';
                 UPDATE persons SET name = 'Someone' WHERE id = '{player}';
                 UPDATE team_members SET number = 99 WHERE person_id = '{p3}';",
                t = s.team.id,
                p3 = s.team.players[2].id,
            ),
        );
        let before = rows(&other);
        let imported = import(&other, &archive, &en()).unwrap();
        assert_eq!(
            (imported.teams, imported.persons, imported.members),
            (
                Merged {
                    added: 0,
                    same: 0,
                    kept_local: 1
                },
                Merged {
                    added: 0,
                    same: 6,
                    kept_local: 1
                },
                Merged {
                    added: 0,
                    same: 6,
                    kept_local: 1
                },
            )
        );
        assert_eq!(rows(&other), before);
        // a person with more details here agrees with one with fewer
        execute(
            &other,
            &format!(
                "UPDATE persons SET notes = 'more', height_cm = 190, birth_date = '1999-09-09'
                 WHERE id = '{}'",
                s.team.players[1].id
            ),
        );
        let imported = import(&other, &archive, &en()).unwrap();
        assert_eq!(imported.persons.kept_local, 1);
        // but not the other way around
        execute(
            &other,
            &format!(
                "UPDATE persons SET notes = 'here' WHERE id = '{}'",
                s.team.players[3].id
            ),
        );
        let imported = import(&other, &archive, &en()).unwrap();
        assert_eq!(imported.persons.kept_local, 2);
    }

    #[tokio::test]
    async fn new_members_go_after_the_players_here() {
        let s = scouted().await;
        let other = database();
        import(
            &other,
            &export(&s.db, Subject::Team(s.team.id)).unwrap(),
            &en(),
        )
        .unwrap();
        let add = |db: &Database, name: &str| {
            let db = db.clone();
            let name = name.to_string();
            let team = s.team.id;
            async move {
                let reader = SqliteTeamReader::new(db.clone());
                let mut team = reader.read_single(&team).await.unwrap();
                let mut player = players().remove(7);
                player.name = name;
                player.number = 50 + u8::try_from(team.players.len()).unwrap();
                team.players.push(player);
                SqliteTeamWriter::new(db)
                    .save(TeamInput::Existing(team.clone()))
                    .await
                    .unwrap()
            }
        };
        add(&s.db, "new there 1").await;
        add(&s.db, "new there 2").await;
        add(&other, "new here 1").await;
        add(&other, "new here 2").await;
        let imported = import(
            &other,
            &export(&s.db, Subject::Team(s.team.id)).unwrap(),
            &en(),
        )
        .unwrap();
        assert_eq!(imported.members.added, 2);
        let team = SqliteTeamReader::new(other.clone())
            .read_single(&s.team.id)
            .await
            .unwrap();
        let names: Vec<&str> = team
            .players
            .iter()
            .filter(|p| !p.deleted)
            .map(|p| p.name.as_str())
            .rev()
            .take(3)
            .collect();
        assert_eq!(names, ["new there 2", "new there 1", "new here 2"]);
    }

    #[tokio::test]
    async fn what_is_removed_here_comes_back() {
        let s = scouted().await;
        let team_archive = export(&s.db, Subject::TeamWithMatches(s.team.id)).unwrap();
        let match_archive = export(&s.db, Subject::Match(s.match_id())).unwrap();
        let all = contents(&s.db).await;
        // the match
        let matches = SqliteMatchWriter::new(s.db.clone());
        matches.remove(&s.team.id, &s.m.id).await.unwrap();
        let imported = import(&s.db, &match_archive, &en()).unwrap();
        assert_eq!(imported.matches, 1);
        assert_eq!(contents(&s.db).await, all);
        // the team, with its matches
        SqliteTeamWriter::new(s.db.clone())
            .remove(&s.team.id)
            .await
            .unwrap();
        assert!(contents(&s.db).await.is_empty());
        let imported = import(&s.db, &team_archive, &en()).unwrap();
        assert_eq!((imported.teams.added, imported.matches), (1, 1));
        assert_eq!(contents(&s.db).await, all);
        // a person removed: back with the archive's details
        let person = s.team.players[0].id;
        execute(
            &s.db,
            &format!(
                "UPDATE persons SET deleted = 1, name = '', notes = '', birth_date = NULL,
                     height_cm = NULL WHERE id = '{person}'"
            ),
        );
        let imported = import(&s.db, &team_archive, &en()).unwrap();
        assert_eq!(imported.persons.added, 1);
        assert_eq!(contents(&s.db).await, all);
        // a player removed from the team here stays removed
        let mut team = s.team.clone();
        SqliteTeamWriter::new(s.db.clone())
            .remove_player(s.team.players[3].id, &mut team)
            .await
            .unwrap();
        let imported = import(&s.db, &team_archive, &en()).unwrap();
        assert_eq!(imported.members.kept_local, 1);
        let team = SqliteTeamReader::new(s.db.clone())
            .read_single(&s.team.id)
            .await
            .unwrap();
        assert!(team.find_player(s.team.players[3].id).unwrap().deleted);
    }

    #[tokio::test]
    async fn a_removed_match_changed_since_stays_removed() {
        let s = scouted().await;
        let archive = export(&s.db, Subject::Match(s.match_id())).unwrap();
        SqliteSetWriter::new(s.db.clone(), en())
            .append_event(&s.m, 1, &serve(s.team.players[0].id))
            .await
            .unwrap();
        SqliteMatchWriter::new(s.db.clone())
            .remove(&s.team.id, &s.m.id)
            .await
            .unwrap();
        let before = rows(&s.db);
        let e = message(import(&s.db, &archive, &en()).unwrap_err());
        assert_eq!(e, EN.match_already_exists);
        assert_eq!(rows(&s.db), before);
        let imported = import(
            &s.db,
            &export(&s.db, Subject::TeamWithMatches(s.team.id)).unwrap(),
            &en(),
        )
        .unwrap();
        assert_eq!((imported.matches, imported.matches_here), (0, 0));
        let mut changed = files(&archive);
        changed.insert(
            "manifest.json".into(),
            serde_json::to_value(Manifest {
                kind: Kind::Team,
                id: s.team.id,
                ..manifest(&archive).unwrap()
            })
            .unwrap(),
        );
        let imported = import(&s.db, &zipped(&changed), &en()).unwrap();
        assert_eq!((imported.matches, imported.matches_here), (0, 1));
        assert_eq!(rows(&s.db), before);
    }

    #[tokio::test]
    async fn a_team_b_and_a_competition_go_with_the_match() {
        let s = scouted().await;
        let b = SqliteTeamWriter::new(s.db.clone())
            .save(TeamInput::New {
                id: None,
                name: "Team B".into(),
                year: 2025,
                classification: None,
                gender: None,
                country: None,
                players: players(),
            })
            .await
            .unwrap();
        let competition = Uuid::new_v4();
        execute(
            &s.db,
            &format!(
                "INSERT INTO competitions (id, name, format, season)
                     VALUES ('{competition}', 'Serie C', 'league', '2025/2026');
                 UPDATE matches SET team_b = '{b}', team_b_name = NULL,
                     competition_id = '{competition}', stage = 'playoff', round = 'Gara 2',
                     home = NULL
                 WHERE id = '{m}'",
                b = b.id,
                m = s.m.id
            ),
        );
        // from B, with its matches: A too
        let archive = export(&s.db, Subject::TeamWithMatches(b.id)).unwrap();
        let read = files(&archive);
        assert_eq!(read["teams.json"].as_array().unwrap().len(), 2);
        assert_eq!(read["competitions.json"][0]["name"], "Serie C");
        let other = database();
        let imported = import(&other, &archive, &en()).unwrap();
        assert_eq!(
            (
                imported.teams.added,
                imported.competitions.added,
                imported.matches
            ),
            (2, 1, 1)
        );
        let all = contents(&s.db).await;
        assert_eq!(contents(&other).await, all);
        let m = &all[&s.team.id].1[0];
        assert_eq!(m.opponent, "Team B");
        assert!(!m.home);
        let row = |db: &Database| -> Vec<String> {
            db.read(|tx| {
                tx.query_row(
                    "SELECT c.name, c.season, m.stage, m.round, m.home IS NULL
                     FROM matches m JOIN competitions c ON c.id = m.competition_id",
                    [],
                    |r| {
                        Ok(vec![
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, bool>(4)?.to_string(),
                        ])
                    },
                )
                .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap()
        };
        assert_eq!(row(&other), row(&s.db));
        // the match alone: both teams
        let archive = export(&s.db, Subject::Match(s.match_id())).unwrap();
        assert_eq!(files(&archive)["teams.json"].as_array().unwrap().len(), 2);
        let alone = database();
        import(&alone, &archive, &en()).unwrap();
        assert_eq!(contents(&alone).await.len(), 2);
        // B alone: not A nor the match
        let read = files(&export(&s.db, Subject::Team(b.id)).unwrap());
        assert_eq!(read["teams.json"].as_array().unwrap().len(), 1);
        assert!(read["matches.json"].as_array().unwrap().is_empty());
        assert!(read["competitions.json"].as_array().unwrap().is_empty());
        assert_eq!(read["persons.json"].as_array().unwrap().len(), 8);
    }

    #[tokio::test]
    async fn the_subject_must_be_there() {
        let s = scouted().await;
        let removed: String =
            s.db.read(|tx| {
                tx.query_row("SELECT id FROM matches WHERE deleted = 1", [], |r| r.get(0))
                    .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        let removed = Uuid::parse_str(&removed).unwrap();
        for subject in [
            Subject::Match(Uuid::new_v4()),
            Subject::Match(removed),
            Subject::Team(Uuid::new_v4()),
            Subject::TeamWithMatches(Uuid::new_v4()),
        ] {
            let e = message(export(&s.db, subject).unwrap_err());
            assert!(e.starts_with("not found"), "{subject:?}: {e}");
        }
        // an archive without its subject
        let mut changed = files(&export(&s.db, Subject::Match(s.match_id())).unwrap());
        changed.insert(
            "manifest.json".into(),
            serde_json::to_value(Manifest {
                id: Uuid::new_v4(),
                ..serde_json::from_value(changed["manifest.json"].clone()).unwrap()
            })
            .unwrap(),
        );
        let e = message(import(&database(), &zipped(&changed), &en()).unwrap_err());
        assert!(e.starts_with(EN.invalid_archive), "{e}");
    }

    /// The archive, changed by `change`, refused by an import that changes
    /// nothing: the error.
    fn refused(s: &Scouted, change: impl FnOnce(&mut BTreeMap<String, Json>)) -> AppError {
        let mut changed = files(&export(&s.db, Subject::Match(s.match_id())).unwrap());
        change(&mut changed);
        let other = database();
        let before = rows(&other);
        let e = import(&other, &zipped(&changed), &en()).unwrap_err();
        assert_eq!(rows(&other), before);
        e
    }

    fn set_manifest(files: &mut BTreeMap<String, Json>, field: &str, value: i64) {
        files.get_mut(MANIFEST).unwrap()[field] = Json::from(value);
    }

    #[tokio::test]
    async fn a_newer_archive_is_not_read() {
        let s = scouted().await;
        let known = crate::db::schema_version();
        let newer = |e: AppError| match e {
            AppError::IO(IOError::DatabaseNewer { found, known: k }) => (found, k),
            other => panic!("{other:?}"),
        };
        let e = refused(&s, |f| set_manifest(f, "schema", known + 1));
        assert_eq!(newer(e), (known + 1, known));
        let e = refused(&s, |f| set_manifest(f, "format", i64::from(FORMAT) + 1));
        assert_eq!(newer(e).0, i64::from(FORMAT) + 1);
        for (field, value) in [("format", 0), ("schema", 0), ("schema", -1)] {
            let e = message(refused(&s, |f| set_manifest(f, field, value)));
            assert!(e.starts_with(EN.invalid_archive), "{field} {value}: {e}");
        }
        let e = message(refused(&s, |f| {
            f.remove(MANIFEST);
        }));
        assert!(e.starts_with(EN.invalid_archive), "{e}");
        let e = message(refused(&s, |f| {
            f.insert(MANIFEST.into(), Json::from("nothing"));
        }));
        assert!(e.starts_with(EN.invalid_archive), "{e}");
    }

    #[tokio::test]
    async fn rows_the_schema_refuses_refuse_the_archive() {
        let s = scouted().await;
        let cases: Vec<(&str, Box<dyn FnOnce(&mut BTreeMap<String, Json>)>)> = vec![
            (
                "unknown column",
                Box::new(|f| table(f, "teams")[0]["colour"] = Json::from("red")),
            ),
            (
                "role",
                Box::new(|f| table(f, "team_members")[0]["role"] = Json::from("Setter")),
            ),
            (
                "boolean",
                Box::new(|f| table(f, "matches")[0]["friendly"] = Json::from(true)),
            ),
            (
                "side b, scouting a",
                Box::new(|f| table(f, "set_lineups")[0]["side"] = Json::from("b")),
            ),
            (
                "person missing",
                Box::new(|f| {
                    table(f, "persons").remove(0);
                }),
            ),
            (
                "rows not an array",
                Box::new(|f| {
                    f.insert("events.json".into(), Json::from(1));
                }),
            ),
            (
                "event code",
                Box::new(|f| table(f, "events")[0]["event_type"] = Json::from("ZZ")),
            ),
        ];
        for (case, change) in cases {
            let e = message(refused(&s, change));
            assert!(
                e.starts_with(EN.invalid_archive) || e.contains("not valid"),
                "{case}: {e}"
            );
        }
    }

    #[tokio::test]
    async fn a_match_that_does_not_replay_refuses_the_archive() {
        let s = scouted().await;
        // we serve and they miss, again and again: touches after the end
        let e = refused(&s, |f| {
            let events = table(f, "events");
            let rally = events.clone();
            for seq in 2..80 {
                let mut event = rally[seq % 2].clone();
                event["id"] = Json::from(Uuid::new_v4().to_string());
                event["seq"] = Json::from(seq);
                events.push(event);
            }
        });
        assert!(matches!(e, AppError::Match(_)), "{e:?}");
    }

    #[tokio::test]
    async fn a_failed_import_changes_nothing() {
        // a new match whose event has the id of one here: refused halfway,
        // after its teams and persons
        let s = scouted().await;
        let mut changed = files(&export(&s.db, Subject::TeamWithMatches(s.team.id)).unwrap());
        let new_id = Json::from(Uuid::new_v4().to_string());
        for name in ["match_roster", "sets", "set_lineups", "events"] {
            for row in table(&mut changed, name) {
                row["match_id"] = new_id.clone();
            }
        }
        table(&mut changed, "matches")[0]["id"] = new_id.clone();
        table(&mut changed, "teams")[0]["name"] = Json::from("changed");
        let other = database();
        let player = s.team.players[0].id;
        execute(
            &other,
            &format!("INSERT INTO persons (id, name) VALUES ('{player}', 'here');"),
        );
        import(
            &other,
            &export(&s.db, Subject::TeamWithMatches(s.team.id)).unwrap(),
            &en(),
        )
        .unwrap();
        let before = rows(&other);
        let e = import(&other, &zipped(&changed), &en()).unwrap_err();
        assert!(message(e).contains("events"));
        assert_eq!(rows(&other), before);
    }
}
