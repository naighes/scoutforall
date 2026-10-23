//! The domain of scout4all, independent of the user interface: teams,
//! matches and sets, the lineup and the replay of the events, the
//! statistics, the PDF report, the persistence and the analytics.
pub mod analytics;
pub mod archive;
pub mod constants;
pub mod errors;
pub mod localization;
pub mod logging;
pub mod player_form;
pub mod providers;
pub mod reporting;
pub mod scouting;
pub mod shapes;
pub mod storage;
pub mod team_form;
pub mod util;

// the tests may panic on what they don't expect (see clippy.toml)
#[cfg(test)]
#[allow(clippy::unreachable)]
mod tests;
