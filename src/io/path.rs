use crate::constants::MATCH_DESCRIPTOR_FILE_NAME;
use dirs::home_dir;
use std::{fs::create_dir_all, path::PathBuf};
use uuid::Uuid;

pub fn get_base_path() -> PathBuf {
    let mut path = home_dir().expect("could not determine home directory");
    path.push(".scoutforall");
    if !path.exists() {
        create_dir_all(&path).expect("could not create base directory");
    }
    path
}

pub fn get_team_folder_path(team_id: &Uuid) -> PathBuf {
    let mut base = get_base_path();
    base.push(team_id.to_string());
    if let Err(e) = create_dir_all(&base) {
        eprintln!("could not create team directory {:?}: {}", base, e);
    }
    base
}

pub fn get_match_folder_path(team_id: &Uuid, match_id: &str) -> PathBuf {
    let mut path: PathBuf = get_team_folder_path(&team_id);
    path.push(match_id);
    path
}

pub fn get_match_descriptor_file_path(team_id: &Uuid, match_id: &str) -> PathBuf {
    let path: PathBuf = get_match_folder_path(team_id, match_id);
    path.join(MATCH_DESCRIPTOR_FILE_NAME)
}

pub fn get_set_descriptor_file_path(team_id: &Uuid, match_id: &str, set_number: u8) -> PathBuf {
    let path = get_match_folder_path(team_id, match_id);
    path.join(format!("set_{}.json", set_number))
}

pub fn get_set_events_file_path(team_id: &Uuid, match_id: &str, set_number: u8) -> PathBuf {
    let path = get_match_folder_path(team_id, match_id);
    path.join(format!("set_{}.csv", set_number))
}
