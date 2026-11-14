//! The domain of scout4all, independent of the user interface: teams,
//! matches and sets, the lineup and the replay of the events, the
//! statistics, the PDF report, the persistence and the analytics.
pub mod analysis;
pub mod analytics;
pub mod benchmark;
pub mod competition_form;
pub mod constants;
pub mod db;
pub mod detail;
pub mod domain;
pub mod dvw;
pub mod errors;
pub mod event_edit;
pub mod example;
pub mod legacy;
pub mod localization;
pub mod logging;
pub mod match_form;
pub mod names;
pub mod overview;
pub mod player_form;
pub mod providers;
pub mod reporting;
pub mod scouting;
pub mod season;
pub mod set_start;
pub mod settings;
pub mod shapes;
pub mod storage;
pub mod team_form;
pub mod trend;
pub mod util;
pub mod video;

// the tests may panic on what they don't expect (see clippy.toml)
#[cfg(test)]
#[allow(clippy::unreachable)]
mod tests;
