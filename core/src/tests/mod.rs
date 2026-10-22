use crate::{
    localization::{Labels, labels},
    shapes::enums::LanguageEnum,
};

mod config_tests;
mod edit_event_tests;
mod event_validation_tests;
mod freeball_tests;
mod legacy_data_tests;
mod lineup_scenario_tests;
mod lineup_tests;
mod point_sequence_tests;
mod report_tests;
mod snapshot_tests;
mod stats_tests;
mod substitution_tests;
mod technique_tests;
mod upload_queue_tests;

/// The core writes in the language it's given: the tests read English.
pub(crate) const EN: &Labels = labels(LanguageEnum::En);
