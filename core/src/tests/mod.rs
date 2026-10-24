use crate::{
    localization::{Labels, labels},
    shapes::enums::LanguageEnum,
    storage::{Store, fs::FsStore},
};
use std::{path::Path, sync::Arc};

mod ball_tests;
mod benchmark_tests;
mod config_tests;
mod edit_event_tests;
mod event_validation_tests;
mod freeball_tests;
mod language_tests;
mod legacy_data_tests;
mod lineup_scenario_tests;
mod lineup_tests;
mod match_form_tests;
mod player_form_tests;
mod point_sequence_tests;
mod report_tests;
mod scouting_tests;
mod set_start_tests;
mod snapshot_tests;
mod stats_tests;
mod storage_tests;
mod substitution_tests;
mod team_form_tests;
mod technique_tests;
mod upload_queue_tests;

/// The core writes in the language it's given: the tests read English.
pub(crate) const EN: &Labels = labels(LanguageEnum::En);

/// The data in a folder, laid out as the app saves it.
pub(crate) fn fs_store(root: impl AsRef<Path>) -> Arc<dyn Store> {
    Arc::new(FsStore::new(root.as_ref()))
}
