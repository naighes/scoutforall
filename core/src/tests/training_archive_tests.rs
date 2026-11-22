//! The archive of the training of a team (TODO/021 step 13,
//! `providers::sqlite::training_archive`): what goes, the round trip, the
//! merge (the local wins, what overlaps stays out), the manifest an older
//! version reads, a fuzz of the rows, the team removed with its training.

#[cfg(test)]
mod tests {
    use crate::{
        errors::AppError,
        providers::{
            sqlite::{
                Database,
                archive::{
                    Contents, Kind, MANIFEST, Preview, Subject, export, import, manifest, preview,
                },
                teams::{SqliteTeamReader, SqliteTeamWriter, TeamRemoval},
            },
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{player::PlayerEntry, team::TeamEntry},
        tests::sqlite_archive_tests::tests::{database, en, execute, rows},
    };
    use chrono::{DateTime, Utc};
    use serde::Deserialize;
    use serde_json::Value as Json;
    use std::collections::BTreeMap;
    use uuid::Uuid;

    const EN: &crate::localization::Labels = crate::tests::EN;

    const PROGRAM: &str = "aaaaaaaa-0000-4000-8000-000000000001";
    const MESO: &str = "aaaaaaaa-0000-4000-8000-000000000002";
    const MICRO: &str = "aaaaaaaa-0000-4000-8000-000000000003";
    const S1: &str = "bbbbbbbb-0000-4000-8000-000000000001";
    const S2: &str = "bbbbbbbb-0000-4000-8000-000000000002";
    const S3: &str = "bbbbbbbb-0000-4000-8000-000000000003";
    const GUEST: &str = "cccccccc-0000-4000-8000-000000000001";

    fn player(n: u8) -> PlayerEntry {
        PlayerEntry {
            id: Uuid::new_v4(),
            name: format!("player {n}"),
            role: None,
            number: n,
            deleted: false,
            birth_date: None,
            height_cm: None,
            notes: format!("notes {n}"),
        }
    }

    async fn team(db: &Database) -> TeamEntry {
        SqliteTeamWriter::new(db.clone())
            .save(TeamInput::New {
                id: None,
                name: "Volley Club".into(),
                year: 2026,
                classification: None,
                gender: None,
                country: None,
                players: vec![player(7), player(9)],
            })
            .await
            .unwrap()
    }

    /// A team with a program holding a mesocycle holding a microcycle, a
    /// session in it (an activity, the attendance of a player and of a
    /// guest), a free one, and a removed one.
    async fn trained() -> (Database, TeamEntry) {
        let db = database();
        let t = team(&db).await;
        let (p7, p9) = (t.players[0].id, t.players[1].id);
        execute(
            &db,
            &format!(
                "INSERT INTO persons (id, name, birth_date, height_cm, notes)
                     VALUES ('{GUEST}', 'guest', '2001-02-03', 190, 'secret');
                 INSERT INTO training_programs (id, team_id, title, start_date, end_date)
                     VALUES ('{PROGRAM}', '{t}', 'season', '2026-09-01', '2026-12-31');
                 INSERT INTO mesocycles (id, team_id, program_id, title, start_date, weeks)
                     VALUES ('{MESO}', '{t}', '{PROGRAM}', 'block', '2026-11-02', 4);
                 INSERT INTO microcycles (id, team_id, mesocycle_id, title, start_date, weeks,
                                          objectives)
                     VALUES ('{MICRO}', '{t}', '{MESO}', 'week', '2026-11-09', 1, 'serve');
                 INSERT INTO training_sessions (id, team_id, microcycle_id, start_local,
                         duration_min, location, title, notes, final_remarks, status)
                     VALUES ('{S1}', '{t}', '{MICRO}', '2026-11-10T18:00', 90, 'gym', 'one',
                             'n', 'f', 'completed'),
                            ('{S2}', '{t}', NULL, '2026-11-12T18:00', 60, 'gym', 'two', '', '',
                             'planned'),
                            ('{S3}', '{t}', NULL, '2026-11-13T18:00', 60, 'gym', 'gone', '', '',
                             'planned');
                 UPDATE training_sessions SET deleted = 1 WHERE id = '{S3}';
                 INSERT INTO training_activities (id, session_id, position, name, planned_min)
                     VALUES ('dddddddd-0000-4000-8000-000000000001', '{S1}', 0, 'serving', 20),
                            ('dddddddd-0000-4000-8000-000000000002', '{S1}', 1, 'defense', 30),
                            ('dddddddd-0000-4000-8000-000000000003', '{S3}', 0, 'gone', NULL);
                 INSERT INTO training_attendance (session_id, person_id, status, note, number, guest)
                     VALUES ('{S1}', '{p7}', 'present', 'on time', 7, 0),
                            ('{S1}', '{p9}', 'unavailable', '', 9, 0),
                            ('{S1}', '{GUEST}', 'late', '', 3, 1),
                            ('{S3}', '{p9}', 'present', '', 9, 0);",
                t = t.id
            ),
        );
        (db, t)
    }

    /// A database with the team of `source` (as an archive of the team
    /// gives it) and none of its training.
    async fn with_team(source: &Database, team: &TeamEntry) -> Database {
        let db = database();
        let archive = export(source, Subject::Team(team.id)).unwrap();
        import(&db, &archive, &en()).unwrap();
        db
    }

    fn files(archive: &[u8]) -> BTreeMap<String, Json> {
        crate::legacy::migrate::unzip(archive)
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
        crate::providers::sqlite::archive::zip_files(&files, zip::CompressionMethod::Deflated)
            .unwrap()
    }

    /// The tables of an archive, as text, but the manifest (its time).
    fn tables(archive: &[u8]) -> BTreeMap<String, Json> {
        let mut files = files(archive);
        files.remove(MANIFEST);
        files
    }

    fn message(e: AppError) -> String {
        e.to_string()
    }

    #[tokio::test]
    async fn the_archive_holds_the_training_not_removed() {
        let (db, t) = trained().await;
        let archive = export(&db, Subject::Training(t.id)).unwrap();
        let m = manifest(&archive).unwrap();
        assert_eq!(
            (m.kind, m.contents, m.id),
            (Kind::Team, Some(Contents::Training), t.id)
        );
        let files = files(&archive);
        let count = |name: &str| files[&format!("{name}.json")].as_array().unwrap().len();
        assert_eq!(
            [
                count("training_programs"),
                count("mesocycles"),
                count("microcycles"),
                count("training_sessions"),
                count("training_activities"),
                count("training_attendance"),
                count("teams"),
                count("persons"),
            ],
            [1, 1, 1, 2, 2, 3, 1, 3]
        );
        for absent in ["matches", "match_roster", "team_members", "events", "sets"] {
            assert!(!files.contains_key(&format!("{absent}.json")), "{absent}");
        }
        // the players as they are, the guest with only the name
        let persons = files["persons.json"].as_array().unwrap();
        let of = |name: &str| persons.iter().find(|p| p["name"] == name).unwrap();
        assert_eq!(of("player 7")["notes"], "notes 7");
        assert_eq!(of("guest")["notes"], "");
        assert!(of("guest")["birth_date"].is_null() && of("guest")["height_cm"].is_null());
        // an archive of the team doesn't say it
        let team = files_of_team(&db, &t);
        assert!(team[MANIFEST].get("contents").is_none());
    }

    fn files_of_team(db: &Database, t: &TeamEntry) -> BTreeMap<String, Json> {
        files(&export(db, Subject::Team(t.id)).unwrap())
    }

    /// The manifest as the version before the training reads it: it
    /// ignores `contents`, sees a schema it doesn't know and asks to update.
    #[tokio::test]
    async fn the_version_before_reads_the_manifest_and_asks_to_update() {
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct Before {
            format: u32,
            schema: i64,
            kind: BeforeKind,
            id: Uuid,
            app_version: String,
            exported_at: DateTime<Utc>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum BeforeKind {
            Team,
            Match,
        }
        let (db, t) = trained().await;
        let archive = export(&db, Subject::Training(t.id)).unwrap();
        let read: Before = serde_json::from_value(files(&archive)[MANIFEST].clone()).unwrap();
        assert!(matches!(read.kind, BeforeKind::Team));
        assert_eq!(read.format, 1);
        // the version before had the schema 4
        assert!(read.schema > 4);
        assert_eq!(read.id, t.id);
    }

    #[tokio::test]
    async fn the_training_goes_and_comes_back() {
        let (db, t) = trained().await;
        let archive = export(&db, Subject::Training(t.id)).unwrap();
        let there = with_team(&db, &t).await;
        let before = rows(&there);
        let imported = import(&there, &archive, &en()).unwrap();
        let tr = imported.training;
        assert_eq!(
            [
                tr.cycles.added,
                tr.sessions.added,
                tr.cycles_overlapping,
                tr.sessions_overlapping
            ],
            [3, 2, 0, 0]
        );
        assert_ne!(rows(&there), before);
        // the same training: what it exports is what it was given
        assert_eq!(
            tables(&export(&there, Subject::Training(t.id)).unwrap()),
            tables(&archive)
        );
        // the guest is a person without a team, with only the name
        let guest: (String, Option<i64>) = there
            .read(|tx| {
                tx.query_row(
                    "SELECT name, height_cm FROM persons WHERE id = ?1",
                    [GUEST],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        assert_eq!(guest, ("guest".to_string(), None));
        // again: all the same, nothing written
        let after = rows(&there);
        let again = import(&there, &archive, &en()).unwrap().training;
        assert_eq!(
            [
                again.cycles.same,
                again.sessions.same,
                again.sessions.added,
                again.sessions.kept_local
            ],
            [3, 2, 0, 0]
        );
        assert_eq!(rows(&there), after);
        assert_eq!(
            imported.summary(EN).unwrap(),
            format!("{}: 2", EN.import_sessions_added)
        );
    }

    #[tokio::test]
    async fn the_team_must_be_here_first() {
        let (db, t) = trained().await;
        let archive = export(&db, Subject::Training(t.id)).unwrap();
        let empty = database();
        let e = import(&empty, &archive, &en()).unwrap_err();
        assert!(message(e).contains(EN.import_team_first));
        let before = rows(&empty);
        // a team removed here is not here either
        let there = with_team(&db, &t).await;
        execute(&there, "UPDATE teams SET deleted = 1");
        assert!(import(&there, &archive, &en()).is_err());
        assert_eq!(rows(&empty), before);
    }

    #[tokio::test]
    async fn the_preview_tells_the_team_and_how_much_training() {
        let (db, t) = trained().await;
        let archive = export(&db, Subject::Training(t.id)).unwrap();
        match preview(&archive, &en()).unwrap() {
            Preview::Training(p) => {
                assert_eq!((p.team.id, p.sessions, p.cycles), (t.id, 2, 3));
            }
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn the_local_data_win_and_what_is_removed_here_stays() {
        let (db, t) = trained().await;
        let archive = export(&db, Subject::Training(t.id)).unwrap();
        let there = with_team(&db, &t).await;
        import(&there, &archive, &en()).unwrap();
        execute(
            &there,
            &format!(
                "UPDATE training_sessions SET location = 'elsewhere' WHERE id = '{S2}';
                 UPDATE training_sessions SET deleted = 1 WHERE id = '{S1}';
                 UPDATE microcycles SET title = 'mine' WHERE id = '{MICRO}';"
            ),
        );
        let before = rows(&there);
        let tr = import(&there, &archive, &en()).unwrap().training;
        assert_eq!(
            (tr.sessions.same, tr.sessions.kept_local, tr.sessions.added),
            (0, 2, 0)
        );
        assert_eq!((tr.cycles.same, tr.cycles.kept_local), (2, 1));
        assert_eq!(rows(&there), before);
    }

    #[tokio::test]
    async fn what_overlaps_what_is_here_stays_out() {
        let (db, t) = trained().await;
        let archive = export(&db, Subject::Training(t.id)).unwrap();
        let there = with_team(&db, &t).await;
        // here: another session across the time of S2, another microcycle
        // across the week of the archive's one, and a match in programma
        // across S1's day
        execute(
            &there,
            &format!(
                "INSERT INTO microcycles (id, team_id, title, start_date, weeks)
                     VALUES ('aaaaaaaa-0000-4000-8000-0000000000ff', '{t}', 'mine',
                             '2026-11-09', 1);
                 INSERT INTO training_sessions (id, team_id, start_local, duration_min)
                     VALUES ('bbbbbbbb-0000-4000-8000-0000000000ff', '{t}', '2026-11-12T18:30', 60);",
                t = t.id
            ),
        );
        let imported = import(&there, &archive, &en()).unwrap();
        let tr = imported.training;
        // the microcycle overlaps, so S1 comes in free
        assert_eq!((tr.cycles.added, tr.cycles_overlapping), (2, 1));
        assert_eq!((tr.sessions.added, tr.sessions_overlapping), (1, 1));
        let (micro, count, activities): (Option<String>, i64, i64) = there
            .read(|tx| {
                tx.query_row(
                    "SELECT (SELECT microcycle_id FROM training_sessions WHERE id = ?1),
                            (SELECT count(*) FROM training_sessions WHERE id = ?2),
                            (SELECT count(*) FROM training_activities)",
                    [S1, S2],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        assert_eq!((micro, count, activities), (None, 0, 2));
        assert_eq!(
            imported.summary(EN).unwrap(),
            [
                format!("{}: 1", EN.import_sessions_added),
                format!("{}: 1", EN.import_sessions_overlap),
                format!("{}: 1", EN.import_cycles_overlap),
            ]
            .join("\n")
        );
    }

    #[tokio::test]
    async fn a_scheduled_match_is_in_the_way_a_played_one_is_not() {
        let (db, t) = trained().await;
        let archive = export(&db, Subject::Training(t.id)).unwrap();
        let there = with_team(&db, &t).await;
        execute(
            &there,
            &format!(
                "INSERT INTO matches (id, team_a, team_b_name, home, scouting, start,
                                      max_substitutions)
                     VALUES ('eeeeeeee-0000-4000-8000-000000000001', '{t}', 'x', 'a', 'a',
                             '2026-11-10T18:30:00+01:00', 6);",
                t = t.id
            ),
        );
        let tr = import(&there, &archive, &en()).unwrap().training;
        assert_eq!((tr.sessions.added, tr.sessions_overlapping), (1, 1));
    }

    #[tokio::test]
    async fn rows_of_another_team_refuse_the_archive() {
        let (db, t) = trained().await;
        let archive = export(&db, Subject::Training(t.id)).unwrap();
        let there = with_team(&db, &t).await;
        let mut files = files(&archive);
        files.get_mut("training_sessions.json").unwrap()[0]["team_id"] =
            Json::from(Uuid::new_v4().to_string());
        let before = rows(&there);
        assert!(import(&there, &zipped(&files), &en()).is_err());
        assert_eq!(rows(&there), before);
    }

    /// A tiny generator: the same changes on every run.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % (n.max(1) as u64)) as usize
        }
    }

    /// The format fuzzed: rows of the archive changed at random (a value, a
    /// column, a row dropped or doubled). The import either takes it or
    /// refuses it without changing anything, and never leaves the team with
    /// overlapping sessions or a broken key.
    #[tokio::test]
    async fn a_fuzzed_archive_is_taken_or_refused_whole() {
        let (db, t) = trained().await;
        let archive = export(&db, Subject::Training(t.id)).unwrap();
        let base = files(&archive);
        let names: Vec<String> = base.keys().filter(|n| *n != MANIFEST).cloned().collect();
        let mut rng = Rng(0x5eed);
        let (mut taken, mut refused) = (0, 0);
        for _ in 0..300 {
            let there = with_team(&db, &t).await;
            let mut files = base.clone();
            for _ in 0..=rng.below(3) {
                let name = &names[rng.below(names.len())];
                let Some(rows) = files.get_mut(name).and_then(Json::as_array_mut) else {
                    continue;
                };
                let at = rng.below(rows.len());
                match rng.below(5) {
                    0 if !rows.is_empty() => {
                        rows.remove(at);
                    }
                    1 if !rows.is_empty() => rows.push(rows[at].clone()),
                    2 if !rows.is_empty() => {
                        if let Some(map) = rows[at].as_object_mut() {
                            let keys: Vec<String> = map.keys().cloned().collect();
                            map.remove(&keys[rng.below(keys.len())]);
                        }
                    }
                    _ if !rows.is_empty() => {
                        if let Some(map) = rows[at].as_object_mut() {
                            let keys: Vec<String> = map.keys().cloned().collect();
                            let key = keys[rng.below(keys.len())].clone();
                            let value = [
                                Json::Null,
                                Json::from(0),
                                Json::from(-1),
                                Json::from(1_000_000),
                                Json::from(""),
                                Json::from("x"),
                                Json::from("2026-11-12T18:30"),
                                Json::from("2026-11-12"),
                                Json::from(S2),
                                Json::from(MICRO),
                                Json::from(true),
                                Json::from(vec![1]),
                                Json::from("é".repeat(100)),
                            ][rng.below(13)]
                            .clone();
                            map.insert(key, value);
                        }
                    }
                    _ => {}
                }
            }
            let before = rows(&there);
            match import(&there, &zipped(&files), &en()) {
                Ok(_) => {
                    taken += 1;
                    assert_sound(&there, t.id);
                }
                Err(_) => {
                    refused += 1;
                    assert_eq!(rows(&there), before);
                }
            }
        }
        // both happen: the fuzz isn't all refused nor all taken
        assert!(
            taken > 20 && refused > 20,
            "{taken} taken, {refused} refused"
        );
    }

    /// Keys hold, and no two sessions of the team overlap.
    fn assert_sound(db: &Database, team: Uuid) {
        db.read(|tx| {
            let broken: i64 = tx
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
                    r.get(0)
                })
                .map_err(crate::providers::sqlite::sqlite)?;
            assert_eq!(broken, 0);
            let overlapping: i64 = tx
                .query_row(
                    "SELECT count(*) FROM training_sessions a JOIN training_sessions b
                       ON a.team_id = b.team_id AND a.id < b.id
                     WHERE a.team_id = ?1 AND a.deleted = 0 AND b.deleted = 0
                       AND a.start_local < strftime('%Y-%m-%dT%H:%M', b.start_local,
                                                    '+' || b.duration_min || ' minutes')
                       AND b.start_local < strftime('%Y-%m-%dT%H:%M', a.start_local,
                                                    '+' || a.duration_min || ' minutes')",
                    [team.to_string()],
                    |r| r.get(0),
                )
                .map_err(crate::providers::sqlite::sqlite)?;
            assert_eq!(overlapping, 0);
            Ok(())
        })
        .unwrap();
    }

    #[tokio::test]
    async fn removing_the_team_removes_its_training() {
        let (db, t) = trained().await;
        let reader = SqliteTeamReader::new(db.clone());
        assert_eq!(
            reader.removal(&t.id).unwrap(),
            TeamRemoval {
                sessions: 2,
                ..TeamRemoval::default()
            }
        );
        SqliteTeamWriter::new(db.clone())
            .remove(&t.id)
            .await
            .unwrap();
        let live: i64 = db
            .read(|tx| {
                tx.query_row(
                    "SELECT (SELECT count(*) FROM training_sessions WHERE deleted = 0)
                          + (SELECT count(*) FROM microcycles WHERE deleted = 0)
                          + (SELECT count(*) FROM mesocycles WHERE deleted = 0)
                          + (SELECT count(*) FROM training_programs WHERE deleted = 0)",
                    [],
                    |r| r.get(0),
                )
                .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        assert_eq!(live, 0);
        assert!(export(&db, Subject::Training(t.id)).is_err());
    }
}
