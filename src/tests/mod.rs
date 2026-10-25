mod archive_tests;
mod edit_match_tests;
mod edit_set_events_tests;
mod form_navigation_tests;
mod freeball_tests;
mod layout_tests;
mod live_stats_tests;
mod max_substitutions_tests;
mod navigation_tests;
mod point_sequence_tests;
mod scouting_tests;
mod settings_tests;
mod start_set_tests;
mod team_list_tests;
mod technique_tests;
mod terms_tests;
mod ui;
mod undo_tests;
mod unreadable_data_tests;

use scout4all_core::storage::{Store, fs::FsStore};
use std::{path::Path, sync::Arc};

/// The data in a folder, as the app saves it.
pub(crate) fn fs_store(root: impl AsRef<Path>) -> Arc<dyn Store> {
    Arc::new(FsStore::new(root.as_ref()))
}
