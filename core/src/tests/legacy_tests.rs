//! The reader of the data of the versions before the database and its
//! rows (`legacy`, TODO/001 step 10): it reads what the providers over the
//! files read, writes nothing, tells what it can't read; the rows of every
//! legacy fixture go into the database and read back the same, with one
//! person per player id, unique numbers in the rosters, and no player
//! outside the team.

#[cfg(test)]
mod tests {
    use crate::{
        archive::{Archive, file_team_id, read_archive, teams_of_match},
        domain::{LegacyId, Person, TeamRef, adapters::MatchRows},
        legacy::{
            folder::{LegacyTeam, NotRead, read_folder},
            rows::{FolderRows, SharedTeams, folder_rows, match_rows, team_rows},
        },
        localization::Language,
        providers::{
            match_reader::MatchReader,
            sqlite::{
                Database,
                matches::SqliteMatchReader,
                rows::{insert_match, write_team},
                teams::SqliteTeamReader,
            },
            store::{
                match_reader::StoreMatchReader, set_reader::StoreSetReader,
                team_reader::StoreTeamReader,
            },
            team_reader::TeamReader,
        },
        shapes::{
            enums::LineupRulesEnum, r#match::MatchEntry, player::PlayerEntry, snapshot::EventEntry, team::TeamEntry,
        },
        storage::{Store, fs::FsStore},
        tests::{
            EN, LEGACY_VERSIONS as VERSIONS, LegacyScores, copy_dir, legacy_fixtures as fixtures,
            legacy_scores,
        },
    };
    use std::{
        collections::{BTreeMap, BTreeSet},
        path::{Path, PathBuf},
        sync::Arc,
    };
    use tempfile::TempDir;
    use uuid::Uuid;

    /// Every file under the folder with its content, by path.
    fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut found = BTreeMap::new();
        let mut folders = vec![root.to_path_buf()];
        while let Some(folder) = folders.pop() {
            for entry in std::fs::read_dir(&folder).unwrap() {
                let path = entry.unwrap().path();
                match path.is_dir() {
                    true => folders.push(path),
                    false => {
                        let content = std::fs::read(&path).unwrap();
                        found.insert(path.strip_prefix(root).unwrap().to_path_buf(), content);
                    }
                }
            }
        }
        found
    }

    /// A copy of the data folder of the version.
    fn data(version: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        copy_dir(&fixtures(version).join("data"), dir.path());
        dir
    }

    async fn legacy(dir: &Path) -> Vec<Result<LegacyTeam, NotRead>> {
        read_folder(&FsStore::new(dir), EN).await.unwrap()
    }

    fn readable(teams: Vec<Result<LegacyTeam, NotRead>>) -> Vec<LegacyTeam> {
        teams
            .into_iter()
            .map(|t| t.expect("a readable team"))
            .collect()
    }

    /// The teams and their matches as the providers over the files read
    /// them, by id.
    async fn by_providers(dir: &Path) -> BTreeMap<Uuid, (TeamEntry, Vec<MatchEntry>)> {
        let store: Arc<dyn Store> = Arc::new(FsStore::new(dir));
        let teams = StoreTeamReader::new(Arc::clone(&store), Language::default());
        let matches = StoreMatchReader::new(
            Arc::clone(&store),
            Arc::new(StoreSetReader::new(Arc::clone(&store), Language::default())),
            Language::default(),
        );
        let mut read = BTreeMap::new();
        for team in teams.read_all().await.unwrap() {
            let team = team.expect("a readable team");
            let mut ms: Vec<MatchEntry> = matches
                .read_all(&team)
                .await
                .unwrap()
                .into_iter()
                .map(|m| m.expect("a readable match"))
                .collect();
            ms.sort_by(|a, b| a.id.cmp(&b.id));
            read.insert(team.id, (team, ms));
        }
        read
    }

    #[tokio::test]
    async fn the_reader_reads_what_the_providers_read_and_writes_nothing() {
        for version in VERSIONS {
            let dir = data(version);
            let before = files(dir.path());
            let read = readable(legacy(dir.path()).await);
            assert_eq!(files(dir.path()), before, "{version}: nothing written");
            let read: BTreeMap<Uuid, (TeamEntry, Vec<MatchEntry>)> = read
                .into_iter()
                .map(|t| {
                    let matches = t
                        .matches
                        .into_iter()
                        .map(|m| m.expect("a readable match"))
                        .collect();
                    (t.team.id, (t.team, matches))
                })
                .collect();
            let expected = by_providers(data(version).path()).await;
            assert_eq!(read, expected, "{version}");
            let matches: usize = read.values().map(|(_, ms)| ms.len()).sum();
            assert_eq!(matches, legacy_scores(version).len(), "{version}");
        }
    }

    #[tokio::test]
    async fn what_cant_be_read_is_told_and_the_rest_is_read() {
        let dir = data("v0.18.0");
        let root = dir.path();
        let teams = readable(legacy(root).await);
        let team_of = |name: &str| {
            teams
                .iter()
                .find(|t| t.team.name == name)
                .map(|t| t.team.id.to_string())
                .unwrap()
        };
        let (first, u19, home) = (
            team_of("Fixture Volley"),
            team_of("Fixture Volley U19"),
            team_of("Home"),
        );
        // a folder not named after a team id
        copy_dir(&root.join(&home), &root.join("not-a-team"));
        // a team whose descriptor isn't json: its matches aren't read
        std::fs::write(root.join(&u19).join("team.json"), "{").unwrap();
        // a match without descriptor, a set cut in two, a set missing
        let first_folder = root.join(&first);
        std::fs::remove_file(first_folder.join("2025-10-18_volley_siena/match.json")).unwrap();
        std::fs::write(
            first_folder.join("2025-10-04_rivali_pisa/set_2.json"),
            "{\"serving",
        )
        .unwrap();
        std::fs::rename(
            first_folder.join("2025-10-11_polisportiva_lucca/set_1.json"),
            first_folder.join("2025-10-11_polisportiva_lucca/set_3.json"),
        )
        .unwrap();

        let read = legacy(root).await;
        let not_read: Vec<&NotRead> = read
            .iter()
            .filter_map(|t| t.as_ref().err())
            .chain(
                read.iter()
                    .filter_map(|t| t.as_ref().ok())
                    .flat_map(|t| t.matches.iter().filter_map(|m| m.as_ref().err())),
            )
            .collect();
        let told: BTreeMap<&str, &str> = not_read
            .iter()
            .map(|n| (n.folder.as_str(), n.reason.as_str()))
            .collect();
        let pisa = format!("{first}/2025-10-04_rivali_pisa");
        let lucca = format!("{first}/2025-10-11_polisportiva_lucca");
        let siena = format!("{first}/2025-10-18_volley_siena");
        assert_eq!(
            told.keys().copied().collect::<BTreeSet<_>>(),
            BTreeSet::from(["not-a-team", u19.as_str(), &pisa, &lucca, &siena]),
        );
        assert!(told["not-a-team"].contains(EN.invalid_team_folder_name));
        assert!(told[u19.as_str()].contains("EOF"), "{}", told[u19.as_str()]);
        assert!(told[pisa.as_str()].contains(EN.invalid_set));
        assert!(told[lucca.as_str()].contains(EN.wrong_set_numbering));
        assert!(told[siena.as_str()].contains(EN.match_descriptor_file_not_found));
        // the rest: the team Home with its match, the two matches of the
        // first team still readable
        let ids: BTreeSet<String> = read
            .iter()
            .filter_map(|t| t.as_ref().ok())
            .flat_map(|t| t.matches.iter().filter_map(|m| m.as_ref().ok()))
            .map(|m| m.id.clone())
            .collect();
        assert_eq!(
            ids,
            BTreeSet::from([
                "2025-10-25_visiting".to_string(),
                "2025-11-08_pallavolo_prato".to_string(),
            ])
        );
    }

    /// The sets saved without their rules (up to v0.3.0): V2 when the
    /// match saves the substitutions (from v0.3.0), else none (replayed
    /// with V1, then V2).
    #[tokio::test]
    async fn the_sets_saved_without_rules_get_those_of_their_match() {
        let dir = data("v0.18.0");
        let teams = readable(legacy(dir.path()).await);
        let first = teams
            .iter()
            .find(|t| t.team.name == "Fixture Volley")
            .unwrap();
        let folder = dir
            .path()
            .join(first.team.id.to_string())
            .join("2025-10-04_rivali_pisa");
        let strip = |name: &str, key: &str| {
            let path = folder.join(name);
            let mut json: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            json.as_object_mut().unwrap().remove(key).unwrap();
            std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        };
        let rules = |teams: Vec<LegacyTeam>| {
            teams
                .into_iter()
                .flat_map(|t| t.matches)
                .filter_map(Result::ok)
                .find(|m| m.id == "2025-10-04_rivali_pisa")
                .unwrap()
                .sets
                .iter()
                .map(|s| s.lineup_rules)
                .collect::<Vec<_>>()
        };
        let saved = rules(readable(legacy(dir.path()).await));
        strip("set_2.json", "lineup_rules");
        let read = rules(readable(legacy(dir.path()).await));
        assert_eq!(read[1], Some(LineupRulesEnum::V2));
        assert_eq!(read[0], saved[0]);
        strip("match.json", "max_substitutions");
        let read = rules(readable(legacy(dir.path()).await));
        assert_eq!(read[1], None);
        assert_eq!(read[0], saved[0]);
    }

    fn database() -> Database {
        Database::new(crate::db::open_in_memory().unwrap())
    }

    /// The teams and the matches written to the database, as the migration
    /// will.
    fn write(db: &Database, teams: &SharedTeams, matches: &[&MatchRows]) {
        db.write(|tx| {
            for team in &teams.teams {
                write_team(tx, &team.entry()?)?;
            }
            for m in matches {
                insert_match(tx, m)?;
            }
            Ok(())
        })
        .unwrap();
    }

    /// The teams and their matches read back from the database, the
    /// matches by the id they had in the files.
    async fn read_back(
        db: &Database,
        legacy_ids: &BTreeMap<String, String>,
    ) -> BTreeMap<Uuid, (TeamEntry, Vec<MatchEntry>)> {
        let teams = SqliteTeamReader::new(db.clone());
        let matches = SqliteMatchReader::new(db.clone(), Language::default());
        let mut read = BTreeMap::new();
        for team in teams.read_all().await.unwrap() {
            let team = team.expect("a readable team");
            let mut ms: Vec<MatchEntry> = matches
                .read_all(&team)
                .await
                .unwrap()
                .into_iter()
                .map(|m| {
                    let m = m.expect("a readable match");
                    MatchEntry {
                        id: legacy_ids[&m.id].clone(),
                        ..m
                    }
                })
                .collect();
            ms.sort_by(|a, b| a.id.cmp(&b.id));
            read.insert(team.id, (team, ms));
        }
        read
    }

    /// The match with its roster as a set: the whole active team when it
    /// has none, in the order of the team.
    fn roster_as_set(m: MatchEntry) -> MatchEntry {
        let roster: BTreeSet<Uuid> = match m.roster.as_slice() {
            [] => m.team.active_players().iter().map(|p| p.id).collect(),
            ids => ids.iter().copied().collect(),
        };
        MatchEntry {
            roster: roster.into_iter().collect(),
            ..m
        }
    }

    /// The team without the removed players no match tells of: the
    /// database keeps them (the membership, deleted) but doesn't read them,
    /// as the files drop them when removed (the fixtures keep one).
    fn told_of(team: &TeamEntry, told: &BTreeSet<Uuid>) -> TeamEntry {
        TeamEntry {
            players: team
                .players
                .iter()
                .filter(|p| !p.deleted || told.contains(&p.id))
                .cloned()
                .collect(),
            ..team.clone()
        }
    }

    fn comparable(
        read: BTreeMap<Uuid, (TeamEntry, Vec<MatchEntry>)>,
    ) -> BTreeMap<Uuid, (TeamEntry, Vec<MatchEntry>)> {
        read.into_iter()
            .map(|(id, (team, ms))| {
                let told: BTreeSet<Uuid> = ms
                    .iter()
                    .flat_map(|m| m.played().into_iter().chain(m.roster.iter().copied()))
                    .collect();
                let ms = ms
                    .into_iter()
                    .map(|m| MatchEntry {
                        team: told_of(&m.team, &told),
                        ..roster_as_set(m)
                    })
                    .collect();
                (id, (told_of(&team, &told), ms))
            })
            .collect()
    }

    fn scores(read: &BTreeMap<Uuid, (TeamEntry, Vec<MatchEntry>)>) -> LegacyScores {
        read.values()
            .flat_map(|(_, ms)| ms)
            .map(|m| {
                assert_eq!(m.replay_issue(), None, "{}", m.id);
                (m.id.clone(), m.set_scores().unwrap())
            })
            .collect()
    }

    fn converted(rows: &FolderRows) -> Vec<&MatchRows> {
        rows.matches
            .iter()
            .map(|m| m.as_ref().expect("a converted match"))
            .collect()
    }

    fn legacy_ids(matches: &[&MatchRows]) -> BTreeMap<String, String> {
        matches
            .iter()
            .map(|m| {
                let legacy = m.row.legacy.as_ref().expect("where it was");
                (m.row.id.to_string(), legacy.id.clone())
            })
            .collect()
    }

    #[tokio::test]
    async fn every_data_folder_goes_into_the_database_and_reads_back_the_same() {
        for version in VERSIONS {
            let dir = data(version);
            let teams = readable(legacy(dir.path()).await);
            let rows = folder_rows(&teams).unwrap();
            let matches = converted(&rows);
            assert_eq!(rows.teams.differing, Vec::<Uuid>::new(), "{version}");
            // where each match was
            for m in &matches {
                let legacy = m.row.legacy.as_ref().unwrap();
                let team = teams.iter().find(|t| t.team.id == legacy.team).unwrap();
                assert_eq!(m.row.team_a, team.team.id, "{version}");
                assert!(
                    team.matches
                        .iter()
                        .any(|f| f.as_ref().is_ok_and(|f| f.id == legacy.id
                            && TeamRef::Name(f.opponent.clone()) == m.row.team_b)),
                    "{version} {}",
                    legacy.id
                );
            }
            let db = database();
            write(&db, &rows.teams, &matches);
            let back = read_back(&db, &legacy_ids(&matches)).await;
            assert_eq!(scores(&back), legacy_scores(version), "{version}");
            assert_eq!(
                comparable(back),
                comparable(by_providers(dir.path()).await),
                "{version}"
            );
        }
    }

    /// The players of all the teams, the same id counted once.
    fn persons(db: &Database) -> (i64, i64) {
        db.read(|tx| {
            let count = |sql: &str| {
                tx.query_row(sql, [], |row| row.get::<_, i64>(0))
                    .map_err(crate::providers::sqlite::sqlite)
            };
            Ok((
                count("SELECT count(*) FROM persons")?,
                count("SELECT count(*) FROM team_members")?,
            ))
        })
        .unwrap()
    }

    #[tokio::test]
    async fn a_player_in_two_teams_is_one_person_with_two_memberships() {
        let dir = data("v0.18.0");
        let teams = readable(legacy(dir.path()).await);
        let entries: Vec<&TeamEntry> = teams.iter().map(|t| &t.team).collect();
        let players: usize = entries.iter().map(|t| t.players.len()).sum();
        let ids: BTreeSet<Uuid> = entries
            .iter()
            .flat_map(|t| t.players.iter().map(|p| p.id))
            .collect();
        // players 1–8 of the first team are in the U19 too
        assert_eq!(players - ids.len(), 8);
        let rows = folder_rows(&teams).unwrap();
        let db = database();
        write(&db, &rows.teams, &converted(&rows));
        assert_eq!(persons(&db), (ids.len() as i64, players as i64));
    }

    fn player(name: &str, number: u8) -> PlayerEntry {
        PlayerEntry {
            id: Uuid::new_v4(),
            name: name.into(),
            number,
            ..PlayerEntry::default()
        }
    }

    fn team(id: u128, players: Vec<PlayerEntry>) -> TeamEntry {
        TeamEntry {
            id: Uuid::from_u128(id),
            name: format!("team {id}"),
            year: 2025,
            players,
            ..TeamEntry::default()
        }
    }

    fn person_of(shared: &SharedTeams, team: usize, id: Uuid) -> &Person {
        shared.teams[team]
            .persons
            .iter()
            .find(|p| p.id == id)
            .unwrap()
    }

    #[test]
    fn the_details_of_a_shared_player_are_those_of_an_active_copy() {
        let rossi = PlayerEntry {
            height_cm: Some(190),
            notes: "captain".into(),
            ..player("Mario Rossi", 7)
        };
        let removed = crate::player_form::removed(&PlayerEntry {
            name: "M. Rossi".into(),
            ..rossi.clone()
        });
        // removed in the first team by id, active in the second: listed
        // the other way round
        let teams = [team(2, vec![rossi.clone()]), team(1, vec![removed])];
        let shared = team_rows(&teams).unwrap();
        for i in 0..2 {
            assert_eq!(person_of(&shared, i, rossi.id).name, "Mario Rossi");
            assert_eq!(person_of(&shared, i, rossi.id).notes, "captain");
        }
        // the memberships stay their own
        assert!(shared.teams[1].members[0].deleted);
        assert!(!shared.teams[0].members[0].deleted);
        // a removed copy differing isn't a difference
        assert_eq!(shared.differing, Vec::<Uuid>::new());

        // active in both, different: the first team by id
        let other = PlayerEntry {
            name: "Rossi Mario".into(),
            number: 4,
            ..rossi.clone()
        };
        let teams = [team(2, vec![rossi.clone()]), team(1, vec![other])];
        let shared = team_rows(&teams).unwrap();
        assert_eq!(person_of(&shared, 0, rossi.id).name, "Rossi Mario");
        assert_eq!(person_of(&shared, 1, rossi.id).name, "Rossi Mario");
        assert_eq!(shared.teams[0].members[0].number, 7);
        assert_eq!(shared.teams[1].members[0].number, 4);
        assert_eq!(shared.differing, vec![rossi.id]);

        // the same: no difference
        let teams = [team(2, vec![rossi.clone()]), team(1, vec![rossi.clone()])];
        assert_eq!(team_rows(&teams).unwrap().differing, Vec::<Uuid>::new());
    }

    /// The finished match of the first team of v0.18.0 with its team.
    async fn finished() -> (TeamEntry, MatchEntry) {
        let dir = data("v0.18.0");
        let teams = readable(legacy(dir.path()).await);
        teams
            .into_iter()
            .find(|t| t.team.name == "Fixture Volley")
            .and_then(|t| {
                let m = t
                    .matches
                    .iter()
                    .filter_map(|m| m.as_ref().ok())
                    .find(|m| m.id == "2025-10-04_rivali_pisa")
                    .cloned();
                m.map(|m| (t.team, m))
            })
            .unwrap()
    }

    fn with_player(team: &TeamEntry, id: Uuid, f: impl Fn(&mut PlayerEntry)) -> TeamEntry {
        let mut team = team.clone();
        team.players.iter_mut().filter(|p| p.id == id).for_each(f);
        team
    }

    fn roster_of(rows: &MatchRows) -> BTreeMap<Uuid, u8> {
        rows.roster
            .iter()
            .map(|r| (r.person_id, r.number))
            .collect()
    }

    fn rows(team: &TeamEntry, m: &MatchEntry) -> MatchRows {
        match_rows(team, m, Uuid::new_v4(), None).unwrap()
    }

    /// The match written with its team and read back: the scores of its
    /// sets.
    async fn written(team: &TeamEntry, rows: &MatchRows) -> (MatchEntry, Vec<(u8, u8)>) {
        let db = database();
        let shared = team_rows(std::slice::from_ref(team)).unwrap();
        write(&db, &shared, &[rows]);
        let back = SqliteMatchReader::new(db.clone(), Language::default())
            .read_single(team, &rows.row.id.to_string())
            .await
            .unwrap();
        let scores = back.set_scores().unwrap();
        (back, scores)
    }

    #[tokio::test]
    async fn a_roster_keeps_one_player_per_number() {
        let (team, m) = finished().await;
        let played = m.played();
        let in_roster = |p: &&PlayerEntry| m.roster.contains(&p.id);
        let scorer = team
            .players
            .iter()
            .filter(in_roster)
            .find(|p| played.contains(&p.id))
            .unwrap();
        let out = team.players.iter().find(|p| !in_roster(p)).unwrap();
        assert!(!played.contains(&out.id));
        // no number twice: the roster as it is
        assert_eq!(
            roster_of(&rows(&team, &m)).keys().collect::<BTreeSet<_>>(),
            m.roster.iter().collect()
        );

        // one played, the other didn't (listed first): who played
        let number = scorer.number;
        let shared = with_player(&team, out.id, |p| p.number = number);
        let listed = MatchEntry {
            roster: [vec![out.id], m.roster.clone()].concat(),
            ..m.clone()
        };
        let roster = roster_of(&rows(&shared, &listed));
        assert_eq!(roster.get(&scorer.id), Some(&number));
        assert_eq!(roster.get(&out.id), None);
        assert_eq!(roster.len(), m.roster.len());
        // also when who played was removed since
        let gone = with_player(&shared, scorer.id, |p| p.deleted = true);
        let roster = roster_of(&rows(&gone, &listed));
        assert_eq!(roster.get(&scorer.id), Some(&number));
        assert_eq!(roster.get(&out.id), None);

        // neither played: who's still in the team, then the first listed
        let siena = MatchEntry {
            sets: vec![],
            ..listed.clone()
        };
        let removed = with_player(&shared, scorer.id, |p| p.deleted = true);
        let roster = roster_of(&rows(&removed, &siena));
        assert_eq!(roster.get(&out.id), Some(&number));
        assert_eq!(roster.get(&scorer.id), None);
        let roster = roster_of(&rows(&shared, &siena));
        assert_eq!(roster.get(&out.id), Some(&number));
        assert_eq!(roster.get(&scorer.id), None);
        // the removed one listed first
        let first_removed = MatchEntry {
            roster: [m.roster.clone(), vec![out.id]].concat(),
            ..siena.clone()
        };
        let roster = roster_of(&rows(&removed, &first_removed));
        assert_eq!(roster.get(&out.id), Some(&number));
        assert_eq!(roster.get(&scorer.id), None);

        // the match without roster (up to v0.10.1): the active team, the
        // removed player sharing a number left out as before
        let old = MatchEntry {
            roster: vec![],
            ..m.clone()
        };
        let roster = roster_of(&rows(&removed, &old));
        let active: BTreeSet<Uuid> = removed.active_players().iter().map(|p| p.id).collect();
        assert_eq!(roster.keys().copied().collect::<BTreeSet<_>>(), active);
    }

    #[tokio::test]
    async fn two_players_who_played_with_one_number_keep_the_match_as_it_was() {
        let (team, m) = finished().await;
        let played = m.played();
        let mut both = team
            .players
            .iter()
            .filter(|p| m.roster.contains(&p.id) && played.contains(&p.id));
        let (kept, left) = (both.next().unwrap().clone(), both.next().unwrap().clone());
        // `left` removed since, and its number given to `kept`
        let team = with_player(&team, left.id, |p| p.deleted = true);
        let team = with_player(&team, kept.id, |p| p.number = left.number);
        let rows = rows(&team, &m);
        let roster = roster_of(&rows);
        assert_eq!(roster.get(&kept.id), Some(&left.number));
        assert_eq!(roster.get(&left.id), None);
        let (back, scores) = written(&team, &rows).await;
        assert_eq!(scores, m.set_scores().unwrap());
        assert_eq!(back.sets, m.sets);
        // still in the team of the match, with the number of the team
        let shown = back.team.find_player(left.id).unwrap();
        assert_eq!(shown.number, left.number);
    }

    #[tokio::test]
    async fn a_player_outside_the_team_is_told_and_the_rest_converts() {
        let dir = data("v0.18.0");
        let mut teams = readable(legacy(dir.path()).await);
        let stranger = Uuid::new_v4();
        let first = teams
            .iter_mut()
            .find(|t| t.team.name == "Fixture Volley")
            .unwrap();
        let team_id = first.team.id;
        let pisa = first
            .matches
            .iter_mut()
            .filter_map(|m| m.as_mut().ok())
            .find(|m| m.id == "2025-10-04_rivali_pisa")
            .unwrap();
        let event = pisa.sets[1]
            .events
            .iter_mut()
            .find(|e| e.target_player.is_none() && e.player.is_some())
            .unwrap();
        *event = EventEntry {
            player: Some(stranger),
            ..event.clone()
        };
        let rows = folder_rows(&teams).unwrap();
        let not_converted: Vec<&NotRead> =
            rows.matches.iter().filter_map(|m| m.as_ref().err()).collect();
        assert_eq!(not_converted.len(), 1);
        assert_eq!(
            not_converted[0].folder,
            format!("{team_id}/2025-10-04_rivali_pisa")
        );
        assert!(
            not_converted[0]
                .reason
                .contains(&format!("player {stranger} not in team {team_id}")),
            "{}",
            not_converted[0].reason
        );
        assert_eq!(
            rows.matches.iter().filter(|m| m.is_ok()).count(),
            legacy_scores("v0.18.0").len() - 1
        );
    }

    /// The archives of a version (with the old ones of v0.0.1/v0.2.0 for
    /// `None`): the teams and the matches.
    fn archives(version: Option<&str>) -> (Vec<TeamEntry>, Vec<MatchEntry>) {
        let folder = match version {
            Some(version) => fixtures(version).join("export"),
            None => Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tests/fixtures"),
        };
        let mut teams = vec![];
        let mut matches = vec![];
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".zip") {
                continue;
            }
            match read_archive(&std::fs::read(&path).unwrap(), EN).expect("an archive") {
                Archive::Team { team, id } => teams.push(TeamEntry {
                    id: id.or(file_team_id(&name)).expect("the id of the team"),
                    ..team
                }),
                Archive::Match(m) => matches.push(m),
            }
        }
        teams.sort_by_key(|t| t.id);
        matches.sort_by(|a, b| a.id.cmp(&b.id));
        (teams, matches)
    }

    #[tokio::test]
    async fn every_archive_converts_with_its_team_and_reads_back_as_scouted() {
        let versions = VERSIONS.iter().map(|v| Some(*v)).chain([None]);
        for version in versions {
            let (teams, matches) = archives(version);
            let shared = team_rows(&teams).unwrap();
            let rows: Vec<(MatchRows, &TeamEntry)> = matches
                .iter()
                .map(|m| {
                    // the last one the match fits, as the import, with its
                    // roster (a match without sets fits every team)
                    let team = teams_of_match(m, &teams)
                        .into_iter()
                        .rfind(|t| m.roster.iter().all(|p| t.find_player(*p).is_some()))
                        .unwrap();
                    let id = Uuid::new_v4();
                    let legacy = LegacyId {
                        id: m.id.clone(),
                        team: team.id,
                    };
                    (match_rows(team, m, id, Some(legacy)).unwrap(), team)
                })
                .collect();
            let db = database();
            write(&db, &shared, &rows.iter().map(|(r, _)| r).collect::<Vec<_>>());
            let reader = SqliteMatchReader::new(db.clone(), Language::default());
            let mut scores = LegacyScores::new();
            for (row, team) in &rows {
                let back = reader
                    .read_single(team, &row.row.id.to_string())
                    .await
                    .unwrap();
                let archived = matches
                    .iter()
                    .find(|m| Some(&m.id) == row.row.legacy.as_ref().map(|l| &l.id))
                    .unwrap();
                assert_eq!(back.replay_issue(), None, "{version:?} {}", archived.id);
                assert_eq!(back.sets, archived.sets, "{version:?} {}", archived.id);
                scores.insert(archived.id.clone(), back.set_scores().unwrap());
            }
            match version {
                Some(version) => assert_eq!(scores, legacy_scores(version), "{version}"),
                None => assert_eq!(scores.len(), 1),
            }
        }
    }
}
