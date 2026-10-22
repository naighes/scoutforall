//! Settings baked into the binary at build time. Up to v0.2.0 they were read
//! from the environment of the user running the app, where they're never
//! set: released builds uploaded no statistics and reported no issues.

#[cfg(test)]
mod tests {
    use crate::{
        analytics::upload::create_match_archive,
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        localization::Language,
        providers::{
            fs::{match_writer::FileSystemMatchWriter, team_writer::FileSystemTeamWriter},
            match_writer::MatchWriter,
            team_writer::{TeamInput, TeamWriter},
        },
        util::config_var,
    };
    use chrono::Utc;
    use std::io::Read;
    use tempfile::TempDir;

    // a name per test: tests run in parallel and share the environment
    #[test]
    fn built_in_value_without_the_environment() {
        let name = "SCOUT4ALL_TEST_BUILT_IN";
        assert_eq!(
            config_var(name, Some("https://built.in")),
            Some("https://built.in".to_string())
        );
    }

    #[test]
    fn environment_overrides_the_built_in_value() {
        let name = "SCOUT4ALL_TEST_OVERRIDE";
        // SAFETY: no other test reads or writes this variable
        unsafe { std::env::set_var(name, "https://from.env") };
        assert_eq!(
            config_var(name, Some("https://built.in")),
            Some("https://from.env".to_string())
        );
        assert_eq!(config_var(name, None), Some("https://from.env".to_string()));
    }

    /// A variable unset in CI is injected empty.
    #[test]
    fn empty_or_missing_values() {
        assert_eq!(config_var("SCOUT4ALL_TEST_MISSING", None), None);
        assert_eq!(config_var("SCOUT4ALL_TEST_EMPTY", Some("")), None);
    }

    #[tokio::test]
    async fn uploaded_metadata_has_the_substitutions() {
        let dir = TempDir::new().unwrap();
        let base = dir.path().join("data");
        std::fs::create_dir_all(&base).unwrap();
        let team = FileSystemTeamWriter::new(&base, Language::default())
            .save(TeamInput::New {
                id: None,
                name: "Volley Club".to_string(),
                year: 2025,
                classification: None,
                gender: None,
                players: vec![],
            })
            .await
            .unwrap();
        let m = FileSystemMatchWriter::new(&base, Language::default())
            .create(
                &team,
                "Cortona Volley".to_string(),
                Utc::now().fixed_offset(),
                true,
                DEFAULT_MAX_SUBSTITUTIONS,
            )
            .await
            .unwrap();
        let zip_path = dir.path().join("upload.zip");
        create_match_archive(&m, &base, &zip_path).await.unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
        let mut content = String::new();
        archive
            .by_name("match.json")
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        let metadata: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(metadata["max_substitutions"], 8);
    }
}
