//! The keys of the data, the same layout as the folders of `~/.scout4all`:
//!
//! ```text
//! <team id>/team.json
//! <team id>/<match id>/match.json
//! <team id>/<match id>/set_<n>.json   the lineup
//! <team id>/<match id>/set_<n>.csv    the events
//! upload_queue.json
//! ```

use crate::constants::{
    MATCH_DESCRIPTOR_FILE_NAME, TEAM_DESCRIPTOR_FILE_NAME, UPLOAD_QUEUE_FILE_NAME,
};
use uuid::Uuid;

/// The root folder, where the teams are.
pub const ROOT: &str = "";

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
