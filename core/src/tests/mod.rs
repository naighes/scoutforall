use crate::{
    localization::{Labels, labels},
    shapes::enums::LanguageEnum,
    storage::{Store, fs::FsStore},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

mod analysis_tests;
mod archive_tests;
mod ball_tests;
mod benchmark_tests;
mod config_tests;
mod country_tests;
mod db_tests;
mod domain_tests;
mod dvw_import_tests;
mod dvw_tests;
mod dvw_translate_tests;
mod edit_event_tests;
mod event_validation_tests;
mod example_tests;
mod fault_tests;
mod finished_match_tests;
mod flags_tests;
mod freeball_tests;
mod language_tests;
mod legacy_data_tests;
mod legacy_fixtures_tests;
mod libero_for_tests;
mod libero_out_tests;
mod lineup_scenario_tests;
mod lineup_tests;
mod match_form_tests;
mod match_index_tests;
mod pinned_teams_tests;
mod player_form_tests;
mod point_sequence_tests;
mod provider_tests;
mod report_tests;
mod schema_tests;
mod scouting_tests;
mod set_start_tests;
mod situation_tests;
mod snapshot_tests;
mod sqlite_tests;
mod stats_tests;
mod storage_tests;
mod substitution_tests;
mod summary_tests;
mod team_form_tests;
mod technique_tests;
mod upload_queue_tests;
mod upload_tests;
mod video_seconds_tests;
mod video_tests;

/// The core writes in the language it's given: the tests read English.
pub(crate) const EN: &Labels = labels(LanguageEnum::En);

/// The data in a folder, laid out as the app saves it.
pub(crate) fn fs_store(root: impl AsRef<Path>) -> Arc<dyn Store> {
    Arc::new(FsStore::new(root.as_ref()))
}

/// The versions of the legacy fixtures (`fixtures/legacy`, see its
/// README): each one where the saved data changed (v0.10.0, v0.14.0 and
/// v0.17.0 save as the one before them).
pub(crate) const LEGACY_VERSIONS: [&str; 8] = [
    "v0.9.0", "v0.10.1", "v0.11.0", "v0.12.0", "v0.13.0", "v0.15.0", "v0.16.0", "v0.18.0",
];

/// The fixtures of a version: `data/` and `export/`.
pub(crate) fn legacy_fixtures(version: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/tests/fixtures/legacy")
        .join(version)
}

pub(crate) fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        match entry.file_type().unwrap().is_dir() {
            true => copy_dir(&entry.path(), &target),
            false => {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
}
