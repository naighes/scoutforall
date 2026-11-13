//! The keys of the data, the same layout as the folders of `~/.scout4all`:
//!
//! ```text
//! <team id>/team.json
//! <team id>/<match id>/match.json
//! <team id>/<match id>/set_<n>.json   the lineup
//! <team id>/<match id>/set_<n>.csv    the events
//! config.json
//! upload_queue.json
//! .index.json                         derived, see `MATCH_INDEX`
//! scout4all.db                        the database (TODO/001), and
//!                                     its -wal and -shm
//! ```
//!
//! Since the database the folders of the teams are only what the
//! versions before it wrote, moved into it at the start
//! (`legacy::migrate`).

use crate::constants::{
    MATCH_DESCRIPTOR_FILE_NAME, TEAM_DESCRIPTOR_FILE_NAME, UPLOAD_QUEUE_FILE_NAME,
};
use uuid::Uuid;

/// The root folder, where the teams are.
pub const ROOT: &str = "";

/// The settings of the app: the same file for every app, each keeps the
/// fields of the others.
pub const CONFIG: &str = "config.json";

/// The settings that couldn't be read, set aside as they were.
pub const UNREADABLE_CONFIG: &str = "config.json.unreadable";

/// What the lists show of each match, with the stamps of the files it
/// comes from (see `providers::store::match_index`): derived from the
/// matches, rebuilt when missing, never backed up nor exported. Hidden, as
/// what `Store::list` leaves out: not a team, nor data.
pub const MATCH_INDEX: &str = ".index.json";

/// The database of the apps (TODO/001), with its files beside it
/// (`-wal`, `-shm`, `-journal`): not a team, nor a file of the versions
/// before it.
pub const DATABASE: &str = "scout4all.db";

/// Whether the blob `name` of the root is the database or one of its
/// files.
pub fn is_database_file(name: &str) -> bool {
    name.strip_prefix(DATABASE)
        .is_some_and(|rest| matches!(rest, "" | "-wal" | "-shm" | "-journal"))
}

/// The matches waiting to be uploaded.
pub const UPLOAD_QUEUE: &str = UPLOAD_QUEUE_FILE_NAME;

pub fn team_folder(team_id: &Uuid) -> String {
    team_id.to_string()
}

pub fn team_descriptor(team_id: &Uuid) -> String {
    format!("{}/{}", team_id, TEAM_DESCRIPTOR_FILE_NAME)
}

pub fn match_folder(team_id: &Uuid, match_id: &str) -> String {
    format!("{}/{}", team_id, match_id)
}

pub fn match_descriptor(team_id: &Uuid, match_id: &str) -> String {
    format!(
        "{}/{}",
        match_folder(team_id, match_id),
        MATCH_DESCRIPTOR_FILE_NAME
    )
}

/// The name of the lineup of a set, in the folder of its match.
pub fn set_descriptor_name(set_number: u8) -> String {
    format!("set_{}.json", set_number)
}

/// The name of the events of a set, in the folder of its match.
pub fn set_events_name(set_number: u8) -> String {
    format!("set_{}.csv", set_number)
}

pub fn set_descriptor(team_id: &Uuid, match_id: &str, set_number: u8) -> String {
    format!(
        "{}/{}",
        match_folder(team_id, match_id),
        set_descriptor_name(set_number)
    )
}

pub fn set_events(team_id: &Uuid, match_id: &str, set_number: u8) -> String {
    format!(
        "{}/{}",
        match_folder(team_id, match_id),
        set_events_name(set_number)
    )
}

/// The key of an entry of a folder.
pub fn child(folder: &str, name: &str) -> String {
    match folder {
        ROOT => name.to_string(),
        _ => format!("{}/{}", folder, name),
    }
}
