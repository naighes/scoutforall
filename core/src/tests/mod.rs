use crate::{
    localization::{Labels, labels},
    shapes::enums::LanguageEnum,
    storage::{Store, fs::FsStore},
};
use std::{path::Path, sync::Arc};

mod analysis_tests;
mod archive_tests;
mod ball_tests;
mod benchmark_tests;
mod config_tests;
mod country_tests;
mod db_tests;
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
mod report_tests;
mod scouting_tests;
mod set_start_tests;
mod situation_tests;
mod snapshot_tests;
mod sqlite_tests;
mod stats_tests;
mod storage_tests;
mod substitution_tests;
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
