//! The data and the archives of the older versions (`fixtures/legacy`, see
//! its README): every data folder loads and every archive imports, and
//! their sets replay to the scores each version scouted them with. The
//! base of the migration to the database and of the import of the old
//! archives (TODO/001).

#[cfg(test)]
mod tests {
    use crate::{
        archive::{Archive, file_team_id, import_match, import_team, read_archive, teams_of_match},
        localization::Language,
        providers::{
            match_reader::MatchReader,
            store::{
                match_reader::StoreMatchReader, match_writer::StoreMatchWriter,
                set_reader::StoreSetReader, set_writer::StoreSetWriter,
                team_reader::StoreTeamReader, team_writer::StoreTeamWriter,
            },
            team_reader::TeamReader,
        },
        shapes::{r#match::MatchEntry, team::TeamEntry},
        storage::{Store, fs::FsStore, memory::MemoryStore},
        tests::{
            EN, LEGACY_VERSIONS as VERSIONS, LegacyScores as Scores, copy_dir,
            legacy_fixtures as fixtures, legacy_scores as expected,
        },
    };
    use std::sync::Arc;
    use tempfile::TempDir;

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
                set_writer: StoreSetWriter::new(Arc::clone(&store), Language::default()),
            }
        }

        async fn teams(&self) -> Vec<TeamEntry> {
            self.team_reader
                .read_all()
                .await
                .expect("the teams")
                .into_iter()
                .map(|t| t.expect("a readable team"))
                .collect()
        }

        /// The scores of the sets of every match of every team, by match id.
        async fn scores(&self) -> Scores {
            let mut scores = Scores::new();
            for team in self.teams().await {
                for m in self
                    .match_reader
                    .read_all(&team)
                    .await
                    .expect("the matches")
                {
                    let m = m.expect("a readable match");
                    scores.insert(m.id.clone(), replayed(&m));
                }
            }
            scores
        }
    }

    /// The scores of the sets, which replay without issues.
    fn replayed(m: &MatchEntry) -> Vec<(u8, u8)> {
        assert_eq!(m.replay_issue(), None, "{}", m.id);
        m.set_scores().expect("the scores")
    }

    #[tokio::test]
    async fn every_data_folder_loads_and_replays_as_scouted() {
        for version in VERSIONS {
            // the reading may write the index of the matches: on a copy
            let dir = TempDir::new().unwrap();
            copy_dir(&fixtures(version).join("data"), dir.path());
            let providers = Providers::new(Arc::new(FsStore::new(dir.path())));
            assert_eq!(providers.scores().await, expected(version), "{version}");
        }
    }

    /// The archives of the version, teams first: the file name and its
    /// content.
    fn archives(version: &str) -> (Vec<(String, Vec<u8>)>, Vec<(String, Vec<u8>)>) {
        let mut teams = vec![];
        let mut matches = vec![];
        for entry in std::fs::read_dir(fixtures(version).join("export")).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().to_string();
            let bytes = std::fs::read(entry.path()).unwrap();
            match read_archive(&bytes, EN).expect("an archive") {
                Archive::Team { .. } => teams.push((name, bytes)),
                Archive::Match(_) => matches.push((name, bytes)),
            }
        }
        teams.sort();
        matches.sort();
        (teams, matches)
    }

    #[tokio::test]
    async fn every_archive_imports_and_replays_as_scouted() {
        for version in VERSIONS {
            let providers = Providers::new(Arc::new(MemoryStore::new()));
            let (teams, matches) = archives(version);
            for (name, bytes) in &teams {
                import_team(
                    bytes,
                    file_team_id(name),
                    &providers.team_reader,
                    &providers.team_writer,
                    EN,
                )
                .await
                .unwrap_or_else(|e| panic!("{version} {name}: {e}"));
            }
            let all = providers.teams().await;
            for (name, bytes) in &matches {
                let archived = match read_archive(bytes, EN).expect("an archive") {
                    Archive::Match(m) => m,
                    Archive::Team { .. } => unreachable!(),
                };
                let owners = teams_of_match(&archived, &all);
                // the match of the U19 has only players of both teams
                let expected_owners = match (name.as_str(), archived.sets.is_empty()) {
                    ("2025-11-15_rivali_pisa_u19.zip", _) => 2,
                    // a match without sets fits every team
                    (_, true) => all.len(),
                    _ => 1,
                };
                assert_eq!(owners.len(), expected_owners, "{version} {name}");
                let team = owners.last().copied().expect("a team");
                import_match(
                    bytes,
                    team,
                    &providers.match_reader,
                    &providers.match_writer,
                    &providers.set_writer,
                    EN,
                )
                .await
                .unwrap_or_else(|e| panic!("{version} {name}: {e}"));
            }
            assert_eq!(providers.scores().await, expected(version), "{version}");
        }
    }
}
