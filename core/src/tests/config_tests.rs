//! Settings baked into the binary at build time. Up to v0.2.0 they were read
//! from the environment of the user running the app, where they're never
//! set: released builds uploaded no statistics and reported no issues.

#[cfg(test)]
mod tests {
    use crate::{
        analytics::upload::create_match_archive,
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        providers::{
            match_writer::MatchWriter,
            store::{match_writer::StoreMatchWriter, team_writer::StoreTeamWriter},
            team_writer::{TeamInput, TeamWriter},
        },
        storage::{Store, memory::MemoryStore},
        util::{config_var, config_var_in},
    };
    use chrono::Utc;
    use std::{
        io::{Cursor, Read},
        sync::Arc,
    };

    // no test changes the environment of the process: tests run in parallel
    // and share it
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
        let environment = |n: &str| (n == name).then(|| "https://from.env".to_string());
        assert_eq!(
            config_var_in(environment, name, Some("https://built.in")),
            Some("https://from.env".to_string())
        );
        assert_eq!(
            config_var_in(environment, name, None),
            Some("https://from.env".to_string())
        );
    }

    /// A variable unset in CI is injected empty.
    #[test]
    fn empty_or_missing_values() {
        assert_eq!(config_var("SCOUT4ALL_TEST_MISSING", None), None);
        assert_eq!(config_var("SCOUT4ALL_TEST_EMPTY", Some("")), None);
    }

    #[tokio::test]
    async fn uploaded_metadata_has_the_substitutions() {
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let team = StoreTeamWriter::new(Arc::clone(&store))
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
        let m = StoreMatchWriter::new(Arc::clone(&store))
            .create(
                &team,
                "Cortona Volley".to_string(),
                Utc::now().fixed_offset(),
                true,
                DEFAULT_MAX_SUBSTITUTIONS,
            )
            .await
            .unwrap();
        let zip = create_match_archive(&m, &*store).await.unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(zip)).unwrap();
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
