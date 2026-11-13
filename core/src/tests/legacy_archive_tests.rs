//! The archives of the versions before the database imported into it
//! (`providers::sqlite::legacy_archive`, TODO/001 step 13): the archives
//! of every version of the legacy fixtures give what the migration of its
//! data folder gives; a match before or after its team; a match already
//! here, migrated or imported, never again; a team already here merged
//! as a team archive of today; a match that doesn't replay or of another
//! team changes nothing.

#[cfg(test)]
mod tests {
    use crate::{
        archive::{Archive, file_team_id, read_archive},
        errors::{AppError, IOError},
        providers::{
            match_reader::MatchReader,
            sqlite::{
                Database,
                archive::{Imported, Kind, Merged, Subject, export},
                legacy_archive::{Opened, import_legacy_match, import_legacy_team, open},
                matches::SqliteMatchReader,
                teams::SqliteTeamReader,
            },
            team_reader::TeamReader,
        },
        shapes::{enums::LineupRulesEnum, r#match::MatchEntry, team::TeamEntry},
        tests::{
            EN, LEGACY_VERSIONS as VERSIONS, legacy_fixtures, legacy_scores,
            sqlite_archive_tests::tests::{contents, database, en, execute, migrated, rows},
        },
    };
    use std::{collections::BTreeMap, io::Write};
    use uuid::Uuid;

    /// Volley Club, saved by v0.0.1: its id only in the name of the file.
    const OLD_TEAM: &[u8] = include_bytes!("fixtures/45a589d5-004a-4a50-99f5-6d78fd052a3c.zip");
    const OLD_TEAM_ID: &str = "45a589d5-004a-4a50-99f5-6d78fd052a3c";
    /// The first set of the match against Club Arezzo, scouted up to
    /// v0.2.0: it replays only with the rules it was scouted with.
    const OLD_MATCH: &[u8] = include_bytes!("fixtures/2025-09-20_club_arezzo.zip");
    /// Middle blocker 2, who serves in the set of the old match.
    const MB2: &str = "00000000-0000-4000-8000-000000000003";

    /// The ids of the legacy fixtures.
    const FIXTURE_VOLLEY_V0_18_0: &str = "f79ccc8b-326d-4b74-bc04-8bac9d08178e";
    const U19_V0_18_0: &str = "abae2039-fa19-4da6-9021-8499a3aaccd6";
    const FINISHED: &str = "2025-10-04_rivali_pisa";
    const IN_PROGRESS: &str = "2025-10-11_polisportiva_lucca";
    const U19_MATCH: &str = "2025-11-15_rivali_pisa_u19";

    /// The archives exported by a version: the teams with their id (from
    /// the name of the file when the archive hasn't it), the matches with
    /// the team whose data folder has them.
    struct Exported {
        teams: Vec<(Vec<u8>, Option<Uuid>)>,
        matches: Vec<(String, Vec<u8>, Uuid)>,
    }

    fn exported(version: &str) -> Exported {
        let fixtures = legacy_fixtures(version);
        let team_of = |match_id: &str| -> Uuid {
            std::fs::read_dir(fixtures.join("data"))
                .unwrap()
                .map(|e| e.unwrap().path())
                .find(|team| team.join(match_id).join("match.json").exists())
                .and_then(|team| file_team_id(&team.to_string_lossy()))
                .unwrap()
        };
        let mut all = Exported {
            teams: vec![],
            matches: vec![],
        };
        let mut paths: Vec<_> = std::fs::read_dir(fixtures.join("export"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        paths.sort();
        for path in paths {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let data = std::fs::read(&path).unwrap();
            match file_team_id(&name) {
                Some(id) => all.teams.push((data, Some(id))),
                None => {
                    let id = name.trim_end_matches(".zip").to_string();
                    let team = team_of(&id);
                    all.matches.push((id, data, team));
                }
            }
        }
        all
    }

    fn archive_of<'a>(all: &'a Exported, match_id: &str) -> &'a [u8] {
        &all.matches
            .iter()
            .find(|(id, ..)| id == match_id)
            .unwrap()
            .1
    }

    fn uuid(id: &str) -> Uuid {
        Uuid::parse_str(id).unwrap()
    }

    /// The teams and their matches, but the ids of the matches (each
    /// database gives its own), by day and opponent.
    async fn without_ids(db: &Database) -> BTreeMap<Uuid, (TeamEntry, Vec<MatchEntry>)> {
        contents(db)
            .await
            .into_iter()
            .map(|(id, (team, matches))| {
                let mut matches: Vec<MatchEntry> = matches
                    .into_iter()
                    .map(|m| MatchEntry {
                        id: String::new(),
                        ..m
                    })
                    .collect();
                matches.sort_by(|a, b| (a.date, &a.opponent).cmp(&(b.date, &b.opponent)));
                (id, (team, matches))
            })
            .collect()
    }

    /// The scores of every match, by its id before the database.
    async fn scores(db: &Database) -> BTreeMap<String, Vec<(u8, u8)>> {
        let ids: Vec<(String, String, String)> = db
            .read(|tx| {
                tx.prepare("SELECT id, team_a, legacy_id FROM matches WHERE deleted = 0")
                    .and_then(|mut s| {
                        s.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                            .collect()
                    })
                    .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        let matches = SqliteMatchReader::new(db.clone(), en());
        let teams = SqliteTeamReader::new(db.clone());
        let mut scores = BTreeMap::new();
        for (id, team, legacy) in ids {
            let team = teams.read_single(&uuid(&team)).await.unwrap();
            let m = matches.read_single(&team, &id).await.unwrap();
            scores.insert(legacy, m.set_scores().unwrap());
        }
        scores
    }

    fn message<T: std::fmt::Debug>(result: Result<T, AppError>) -> String {
        match result {
            Err(AppError::IO(IOError::Msg(m))) => m,
            other => panic!("expected a message, got {other:?}"),
        }
    }

    fn only_matches(added: usize) -> Imported {
        Imported {
            kind: Kind::Match,
            id: Uuid::nil(),
            persons: Merged::default(),
            teams: Merged::default(),
            members: Merged::default(),
            competitions: Merged::default(),
            matches: added,
            matches_here: 0,
        }
    }

    /// What a match import did, but the counts of the rows that agree.
    fn of_match(imported: &Imported) -> Imported {
        Imported {
            id: Uuid::nil(),
            persons: Merged {
                same: 0,
                ..imported.persons
            },
            teams: Merged {
                same: 0,
                ..imported.teams
            },
            members: Merged {
                same: 0,
                ..imported.members
            },
            ..imported.clone()
        }
    }

    /// The archive with the file `name` changed by `edit`, as by hand.
    fn edited(archive: &[u8], name: &str, edit: impl Fn(String) -> String) -> Vec<u8> {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).unwrap();
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for i in 0..zip.len() {
            let file = zip.by_index(i).unwrap();
            let file_name = file.name().unwrap().to_string();
            let content = std::io::read_to_string(file).unwrap();
            let content = match file_name == name {
                true => edit(content),
                false => content,
            };
            out.start_file::<_, ()>(file_name.as_str(), zip::write::FileOptions::default())
                .unwrap();
            out.write_all(content.as_bytes()).unwrap();
        }
        out.finish().unwrap().into_inner()
    }

    /// The archives of every version, teams first, give what the migration
    /// of its data folder gives: the same teams, persons shared, the same
    /// matches with their rosters, sets and scores, the cache written.
    #[tokio::test]
    async fn every_version_imports_as_its_data_folder_migrates() {
        for version in VERSIONS {
            let all = exported(version);
            let db = database();
            for (archive, file_id) in &all.teams {
                let imported = import_legacy_team(&db, archive, *file_id, &en()).unwrap();
                assert_eq!((imported.kind, imported.matches), (Kind::Team, 0));
            }
            for (id, archive, team) in &all.matches {
                let Opened::LegacyMatch { archived, teams } = open(&db, archive, EN).unwrap()
                else {
                    panic!("{version} {id}: expected a match");
                };
                assert_eq!(&archived.id, id, "{version}");
                assert!(teams.iter().any(|t| t.id == *team), "{version} {id}");
                let imported = import_legacy_match(&db, archive, *team, &en()).unwrap();
                // nothing of the team changes
                assert_eq!(of_match(&imported), only_matches(1), "{version} {id}");
            }
            let folder = migrated(version).await;
            assert_eq!(
                without_ids(&db).await,
                without_ids(&folder).await,
                "{version}"
            );
            assert_eq!(scores(&db).await, legacy_scores(version), "{version}");
            let cached: i64 = db
                .read(|tx| {
                    tx.query_row("SELECT count(*) FROM match_summary", [], |r| r.get(0))
                        .map_err(crate::providers::sqlite::sqlite)
                })
                .unwrap();
            assert_eq!(
                cached,
                i64::try_from(all.matches.len()).unwrap(),
                "{version}"
            );
        }
    }

    /// The archives of v0.0.1 and v0.2.0: the team with its id in the name
    /// of its file, the match with the rules of then.
    #[tokio::test]
    async fn the_oldest_archives_replay_as_scouted() {
        let db = database();
        let team = uuid(OLD_TEAM_ID);
        import_legacy_team(
            &db,
            OLD_TEAM,
            file_team_id(&format!("{OLD_TEAM_ID}.zip")),
            &en(),
        )
        .unwrap();
        let imported = import_legacy_match(&db, OLD_MATCH, team, &en()).unwrap();
        let (_, matches) = contents(&db).await.remove(&team).unwrap();
        let [m] = matches.as_slice() else {
            panic!("expected one match");
        };
        assert_eq!(m.id, imported.id.to_string());
        assert_eq!(m.opponent, "Club Arezzo");
        // saved without its rules, as from its folder: those it replays with
        assert_eq!(m.sets[0].lineup_rules, None);
        assert_eq!(
            m.sets[0].find_lineup_rules(m.max_substitutions).unwrap(),
            LineupRulesEnum::V1
        );
        // no roster up to v0.10.1: the active team
        let active: Vec<Uuid> = m.team.active_players().iter().map(|p| p.id).collect();
        assert_eq!(m.roster, active);
        let snapshot = m.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!((snapshot.score_us, snapshot.score_them), (3, 1));
        assert_eq!(snapshot.current_lineup.get_setter(), Some(uuid(MB2)));
    }

    #[tokio::test]
    async fn a_team_without_its_id_takes_the_one_of_its_file() {
        let db = database();
        let without = import_legacy_team(&db, OLD_TEAM, None, &en());
        assert_eq!(message(without), EN.team_archive_without_id);
        assert!(contents(&db).await.is_empty());
        // one with its id keeps it, whatever the name of its file
        let all = exported("v0.10.1");
        let (archive, Some(id)) = &all.teams[0] else {
            panic!("expected a team");
        };
        let other = Uuid::new_v4();
        let imported = import_legacy_team(&db, archive, Some(other), &en()).unwrap();
        assert_eq!(imported.id, *id);
        assert_eq!(contents(&db).await.into_keys().collect::<Vec<_>>(), [*id]);
    }

    /// A match imported before its team has nowhere to go; after it, it
    /// goes there, and only once.
    #[tokio::test]
    async fn a_match_before_and_after_its_team() {
        let all = exported("v0.18.0");
        let db = database();
        let archive = archive_of(&all, FINISHED);
        let team = uuid(FIXTURE_VOLLEY_V0_18_0);
        assert_eq!(message(open(&db, archive, EN)), EN.match_of_another_team);
        let nowhere = import_legacy_match(&db, archive, team, &en());
        assert!(message(nowhere).contains(&format!("team {team}")));
        assert!(rows(&db).is_empty());
        let (team_archive, _) = all.teams.iter().find(|(_, id)| *id == Some(team)).unwrap();
        import_legacy_team(&db, team_archive, None, &en()).unwrap();
        let Opened::LegacyMatch { teams, .. } = open(&db, archive, EN).unwrap() else {
            panic!("expected a match");
        };
        assert_eq!(teams.iter().map(|t| t.id).collect::<Vec<_>>(), [team]);
        import_legacy_match(&db, archive, team, &en()).unwrap();
        let before = rows(&db);
        assert_eq!(message(open(&db, archive, EN)), EN.match_already_exists);
        let again = import_legacy_match(&db, archive, team, &en());
        assert_eq!(message(again), EN.match_already_exists);
        assert_eq!(rows(&db), before);
    }

    /// The team archive of a team migrated with its matches: nothing
    /// changes, everything agrees.
    #[tokio::test]
    async fn a_team_after_its_matches() {
        for version in VERSIONS {
            let db = migrated(version).await;
            let before = rows(&db);
            for (archive, file_id) in &exported(version).teams {
                let imported = import_legacy_team(&db, archive, *file_id, &en()).unwrap();
                assert_eq!(
                    imported.teams,
                    Merged {
                        same: 1,
                        ..Merged::default()
                    },
                    "{version}"
                );
                assert_eq!(imported.persons.added, 0, "{version}");
                assert_eq!(imported.persons.kept_local, 0, "{version}");
                assert_eq!(imported.members.added, 0, "{version}");
                assert_eq!(imported.members.kept_local, 0, "{version}");
                assert_eq!(imported.matches, 0, "{version}");
            }
            assert_eq!(rows(&db), before, "{version}");
        }
    }

    /// A match migrated from the data folder is the match of its archive:
    /// not imported again; removed, it comes back, with its id.
    #[tokio::test]
    async fn a_migrated_match_is_not_imported_again() {
        for version in VERSIONS {
            let db = migrated(version).await;
            let all = exported(version);
            let before = rows(&db);
            for (id, archive, team) in &all.matches {
                // one without sets, or of players in two teams (U19), may
                // go into the others
                match open(&db, archive, EN) {
                    Ok(Opened::LegacyMatch { archived, teams }) => {
                        assert!(
                            archived.sets.is_empty() || id == U19_MATCH,
                            "{version} {id}"
                        );
                        assert!(teams.iter().all(|t| t.id != *team), "{version} {id}");
                    }
                    other => assert_eq!(message(other), EN.match_already_exists),
                }
                let again = import_legacy_match(&db, archive, *team, &en());
                assert_eq!(message(again), EN.match_already_exists, "{version} {id}");
            }
            assert_eq!(rows(&db), before, "{version}");
            let (id, archive, team) = &all.matches[0];
            let migrated_id: String = db
                .read(|tx| {
                    tx.query_row("SELECT id FROM matches WHERE legacy_id = ?1", [id], |r| {
                        r.get(0)
                    })
                    .map_err(crate::providers::sqlite::sqlite)
                })
                .unwrap();
            execute(
                &db,
                &format!("UPDATE matches SET deleted = 1 WHERE id = '{migrated_id}'"),
            );
            let imported = import_legacy_match(&db, archive, *team, &en()).unwrap();
            assert_eq!(imported.id.to_string(), migrated_id, "{version}");
            assert_eq!(imported.matches, 1, "{version}");
            assert_eq!(rows(&db), before, "{version}");
        }
    }

    /// The match of the U19 has only players of both teams: it may go into
    /// either, and into the other after one.
    #[tokio::test]
    async fn a_match_of_players_in_two_teams_goes_into_either() {
        let all = exported("v0.18.0");
        let db = database();
        for (archive, file_id) in &all.teams {
            import_legacy_team(&db, archive, *file_id, &en()).unwrap();
        }
        let archive = archive_of(&all, U19_MATCH);
        let ids = |opened: Opened| match opened {
            Opened::LegacyMatch { teams, .. } => {
                let mut ids: Vec<Uuid> = teams.iter().map(|t| t.id).collect();
                ids.sort();
                ids
            }
            other => panic!("expected a match, got {other:?}"),
        };
        let both = [uuid(U19_V0_18_0), uuid(FIXTURE_VOLLEY_V0_18_0)];
        assert_eq!(ids(open(&db, archive, EN).unwrap()), both);
        import_legacy_match(&db, archive, both[1], &en()).unwrap();
        assert_eq!(ids(open(&db, archive, EN).unwrap()), [both[0]]);
        import_legacy_match(&db, archive, both[0], &en()).unwrap();
        assert_eq!(message(open(&db, archive, EN)), EN.match_already_exists);
    }

    /// A team with other players.
    fn other_team(db: &Database) -> Uuid {
        let id = Uuid::new_v4();
        execute(
            db,
            &format!(
                "INSERT INTO teams (id, name, year) VALUES ('{id}', 'Other Club', 2025);
                 INSERT INTO persons (id, name) VALUES ('{p}', 'Someone');
                 INSERT INTO team_members (team_id, person_id, position, number, role)
                 VALUES ('{id}', '{p}', 0, 1, 'setter');",
                p = Uuid::new_v4()
            ),
        );
        id
    }

    #[tokio::test]
    async fn a_match_of_another_team_is_refused() {
        let db = database();
        let other = other_team(&db);
        assert_eq!(message(open(&db, OLD_MATCH, EN)), EN.match_of_another_team);
        let refused = import_legacy_match(&db, OLD_MATCH, other, &en());
        assert_eq!(message(refused), EN.match_of_another_team);
        // with its team here, only its team
        import_legacy_team(&db, OLD_TEAM, Some(uuid(OLD_TEAM_ID)), &en()).unwrap();
        let before = rows(&db);
        let Opened::LegacyMatch { teams, .. } = open(&db, OLD_MATCH, EN).unwrap() else {
            panic!("expected a match");
        };
        assert_eq!(
            teams.iter().map(|t| t.id).collect::<Vec<_>>(),
            [uuid(OLD_TEAM_ID)]
        );
        let refused = import_legacy_match(&db, OLD_MATCH, other, &en());
        assert_eq!(message(refused), EN.match_of_another_team);
        // a stranger brought in, e.g. by hand
        let stranger = Uuid::new_v4().to_string();
        let archive = edited(OLD_MATCH, "set_1.csv", |csv| {
            csv.replace("00000000-0000-4000-8000-000000000008", &stranger)
        });
        assert_eq!(message(open(&db, &archive, EN)), EN.match_of_another_team);
        let refused = import_legacy_match(&db, &archive, uuid(OLD_TEAM_ID), &en());
        assert_eq!(message(refused), EN.match_of_another_team);
        assert_eq!(rows(&db), before);
    }

    /// A player removed here since, whom no match here tells of (the
    /// database doesn't show them), played the match of the archive.
    #[tokio::test]
    async fn a_player_removed_here_played_the_archived_match() {
        let all = exported("v0.11.0");
        let db = database();
        for (archive, file_id) in &all.teams {
            import_legacy_team(&db, archive, *file_id, &en()).unwrap();
        }
        let archive = archive_of(&all, FINISHED);
        let Opened::LegacyMatch { archived, teams } = open(&db, archive, EN).unwrap() else {
            panic!("expected a match");
        };
        let team = teams[0].id;
        let played = archived.played().into_iter().min().unwrap();
        execute(
            &db,
            &format!("UPDATE team_members SET deleted = 1 WHERE person_id = '{played}'"),
        );
        let (shown, _) = contents(&db).await.remove(&team).unwrap();
        assert!(shown.find_player(played).is_none());
        let Opened::LegacyMatch { teams, .. } = open(&db, archive, EN).unwrap() else {
            panic!("expected a match");
        };
        assert_eq!(teams.iter().map(|t| t.id).collect::<Vec<_>>(), [team]);
        import_legacy_match(&db, archive, team, &en()).unwrap();
        let (shown, matches) = contents(&db).await.remove(&team).unwrap();
        assert!(shown.find_player(played).is_some_and(|p| p.deleted));
        assert_eq!(
            matches[0].set_scores().unwrap(),
            legacy_scores("v0.11.0")[FINISHED]
        );
    }

    /// A set this version doesn't replay: the import says which and writes
    /// nothing.
    #[tokio::test]
    async fn a_match_that_does_not_replay_is_refused() {
        let all = exported("v0.18.0");
        let db = database();
        for (archive, file_id) in &all.teams {
            import_legacy_team(&db, archive, *file_id, &en()).unwrap();
        }
        let before = rows(&db);
        let in_progress = archive_of(&all, IN_PROGRESS);
        let Ok(Archive::Match(m)) = read_archive(in_progress, EN) else {
            panic!("expected a match");
        };
        let rows_before = m.sets[1].events.len();
        // their error gives us the serve: they can't score an ace
        let archive = edited(in_progress, "set_2.csv", |csv| {
            format!("{csv}2025-10-11T20:01:00.000000Z,OE,,,\n2025-10-11T20:01:01.000000Z,OS,,,\n")
        });
        let refused = import_legacy_match(&db, &archive, uuid(FIXTURE_VOLLEY_V0_18_0), &en());
        let why = EN
            .set_event_not_valid
            .replace("{set}", "2")
            .replace("{row}", &(rows_before + 2).to_string())
            .replace("{event}", "OS")
            .replace("{previous}", "OE");
        assert_eq!(message(refused), why);
        assert_eq!(rows(&db), before);
    }

    /// A roster written by hand, as no version saves it: without who came
    /// in and with a player of another team. The match keeps the players
    /// of the team and who played, as the match form does.
    #[tokio::test]
    async fn the_roster_of_the_archive_keeps_who_played() {
        let db = database();
        let team = uuid(OLD_TEAM_ID);
        import_legacy_team(&db, OLD_TEAM, Some(team), &en()).unwrap();
        let numbered = |n: u128| Uuid::from_u128(0x00000000_0000_4000_8000_000000000000 + n);
        let roster: Vec<Uuid> = (1..=7).map(numbered).chain([Uuid::new_v4()]).collect();
        let archive = edited(OLD_MATCH, "match.json", |json| {
            let mut descriptor: serde_json::Value = serde_json::from_str(&json).unwrap();
            descriptor["roster"] = serde_json::json!(roster);
            descriptor.to_string()
        });
        import_legacy_match(&db, &archive, team, &en()).unwrap();
        let (_, matches) = contents(&db).await.remove(&team).unwrap();
        assert_eq!(matches[0].roster, (1..=8).map(numbered).collect::<Vec<_>>());
        assert_eq!(matches[0].team.active_players().len(), 8);
    }

    /// A team archive of a team here merges as one of today: its name here
    /// wins, a player missing here goes after those here, a team removed
    /// here comes back.
    #[tokio::test]
    async fn a_team_here_merges_as_a_team_archive_of_today() {
        let db = database();
        let team = uuid(OLD_TEAM_ID);
        import_legacy_team(&db, OLD_TEAM, Some(team), &en()).unwrap();
        execute(
            &db,
            &format!("UPDATE teams SET name = 'Renamed' WHERE id = '{team}'"),
        );
        let newcomer = Uuid::new_v4();
        let archive = edited(OLD_TEAM, "team.json", |json| {
            let mut team: serde_json::Value = serde_json::from_str(&json).unwrap();
            let mut player = team["players"][0].clone();
            player["id"] = newcomer.to_string().into();
            player["number"] = 99.into();
            team["players"].as_array_mut().unwrap().insert(0, player);
            team.to_string()
        });
        let imported = import_legacy_team(&db, &archive, Some(team), &en()).unwrap();
        assert_eq!(imported.teams.kept_local, 1);
        assert_eq!((imported.persons.added, imported.members.added), (1, 1));
        let (back, _) = contents(&db).await.remove(&team).unwrap();
        assert_eq!(back.name, "Renamed");
        assert_eq!(
            back.players.last().map(|p| (p.id, p.number)),
            Some((newcomer, 99))
        );
        execute(
            &db,
            &format!("UPDATE teams SET deleted = 1 WHERE id = '{team}'"),
        );
        let imported = import_legacy_team(&db, OLD_TEAM, Some(team), &en()).unwrap();
        assert_eq!(imported.teams.added, 1);
        assert!(contents(&db).await.contains_key(&team));
    }

    /// The archives of the database are told by their manifest; a team is
    /// no match and a match no team.
    #[tokio::test]
    async fn the_archive_tells_what_it_is() {
        let db = migrated("v0.18.0").await;
        let team = uuid(FIXTURE_VOLLEY_V0_18_0);
        let archive = export(&db, Subject::Team(team)).unwrap();
        let Opened::Database(manifest) = open(&db, &archive, EN).unwrap() else {
            panic!("expected an archive of the database");
        };
        assert_eq!((manifest.kind, manifest.id), (Kind::Team, team));
        assert!(matches!(
            open(&db, OLD_TEAM, EN).unwrap(),
            Opened::LegacyTeam { id: None, .. }
        ));
        let empty = database();
        let not_a_team = import_legacy_team(&empty, OLD_MATCH, Some(team), &en());
        assert_eq!(message(not_a_team), EN.invalid_archive);
        let not_a_match = import_legacy_match(&db, OLD_TEAM, team, &en());
        assert_eq!(message(not_a_match), EN.invalid_archive);
        assert_eq!(message(open(&db, b"not a zip", EN)), EN.invalid_archive);
    }
}
