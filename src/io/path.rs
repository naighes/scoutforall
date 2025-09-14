use crate::{constants::MATCH_DESCRIPTOR_FILE_NAME, localization::current_labels};
use dirs::home_dir;
use std::{fs::create_dir_all, path::PathBuf};
use uuid::Uuid;

pub fn get_base_path() -> PathBuf {
    let mut path = home_dir().expect(current_labels().could_not_recognize_home_directory);
    path.push(".scout4all");
    if !path.exists() {
        create_dir_all(&path).expect(current_labels().could_not_create_app_directory);
    }
    path
}

pub fn get_team_folder_path(team_id: &Uuid) -> PathBuf {
    let mut base = get_base_path();
    base.push(team_id.to_string());
    create_dir_all(&base).expect(current_labels().could_not_create_team_directory);
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

pub fn get_config_file_path() -> PathBuf {
    let mut path: PathBuf = get_base_path();
    path.push("config.json");
    path
}
