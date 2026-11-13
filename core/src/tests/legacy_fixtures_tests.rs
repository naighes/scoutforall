//! The data and the archives of the older versions (`fixtures/legacy`, see
//! its README): every data folder reads and every archive imports into the
//! database, and their sets replay to the scores each version scouted them
//! with. The base of the migration to the database and of the import of
//! the old archives (TODO/001).

#[cfg(test)]
mod tests {
    use crate::{
        legacy::{
            archive::{Archive, file_team_id, read_archive},
            folder::read_folder,
        },
        localization::Language,
        providers::sqlite::legacy_archive::{
            Opened, import_legacy_match, import_legacy_team, open,
        },
        shapes::r#match::MatchEntry,
        storage::fs::FsStore,
        tests::{
            EN, LEGACY_VERSIONS as VERSIONS, LegacyScores as Scores, copy_dir, database,
            legacy_archive_tests::tests::scores, legacy_fixtures as fixtures,
            legacy_scores as expected,
        },
    };
    use tempfile::TempDir;

    /// The scores of the sets, which replay without issues.
    fn replayed(m: &MatchEntry) -> Vec<(u8, u8)> {
        assert_eq!(m.replay_issue(), None, "{}", m.id);
        m.set_scores().expect("the scores")
    }

    #[tokio::test]
    async fn every_data_folder_reads_and_replays_as_scouted() {
        for version in VERSIONS {
            let dir = TempDir::new().unwrap();
            copy_dir(&fixtures(version).join("data"), dir.path());
            let mut scores = Scores::new();
            for team in read_folder(&FsStore::new(dir.path()), EN).await.unwrap() {
                for m in team.expect("a readable team").matches {
                    let m = m.expect("a readable match");
                    scores.insert(m.id.clone(), replayed(&m));
                }
            }
            assert_eq!(scores, expected(version), "{version}");
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
        let language = Language::default();
        for version in VERSIONS {
            let db = database();
            let (teams, matches) = archives(version);
            for (name, bytes) in &teams {
                import_legacy_team(&db, bytes, file_team_id(name), &language)
                    .unwrap_or_else(|e| panic!("{version} {name}: {e}"));
            }
            for (name, bytes) in &matches {
                let Ok(Opened::LegacyMatch {
                    archived,
                    teams: owners,
                }) = open(&db, bytes, EN)
                else {
                    panic!("{version} {name}: expected a match");
                };
                // the match of the U19 has only players of both teams
                let expected_owners = match (name.as_str(), archived.sets.is_empty()) {
                    ("2025-11-15_rivali_pisa_u19.zip", _) => 2,
                    // a match without sets fits every team
                    (_, true) => teams.len(),
                    _ => 1,
                };
                assert_eq!(owners.len(), expected_owners, "{version} {name}");
                let team = owners.last().expect("a team");
                import_legacy_match(&db, bytes, team.id, &language)
                    .unwrap_or_else(|e| panic!("{version} {name}: {e}"));
            }
            assert_eq!(scores(&db).await, expected(version), "{version}");
        }
    }
}
