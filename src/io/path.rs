use crate::{
    constants::MATCH_DESCRIPTOR_FILE_NAME,
    errors::{AppError, IOError},
    localization::current_labels,
};
use dirs::home_dir;
use std::{fs::create_dir_all, path::PathBuf};
use uuid::Uuid;

pub fn get_base_path() -> Result<PathBuf, AppError> {
    let mut path = home_dir().ok_or(AppError::IO(IOError::Msg(
        current_labels()
            .could_not_recognize_home_directory
            .to_string(),
    )))?;
    path.push(".scout4all");
    if !path.exists() {
        create_dir_all(&path).map_err(|_| {
            AppError::IO(IOError::Msg(
                current_labels().could_not_create_app_directory.to_string(),
            ))
        })?;
    }
    Ok(path)
}

pub fn get_team_folder_path(team_id: &Uuid) -> Result<PathBuf, AppError> {
    let mut base = get_base_path()?;
    base.push(team_id.to_string());
    create_dir_all(&base).map_err(|_| {
        AppError::IO(IOError::Msg(
            current_labels().could_not_create_team_directory.to_string(),
        ))
    })?;
    Ok(base)
}

pub fn get_match_folder_path(team_id: &Uuid, match_id: &str) -> Result<PathBuf, AppError> {
    let mut path: PathBuf = get_team_folder_path(team_id)?;
    path.push(match_id);
    Ok(path)
}

pub fn get_match_descriptor_file_path(team_id: &Uuid, match_id: &str) -> Result<PathBuf, AppError> {
    let path: PathBuf = get_match_folder_path(team_id, match_id)?;
    Ok(path.join(MATCH_DESCRIPTOR_FILE_NAME))
}

pub fn get_set_descriptor_file_path(
    team_id: &Uuid,
    match_id: &str,
    set_number: u8,
) -> Result<PathBuf, AppError> {
    let path = get_match_folder_path(team_id, match_id)?;
    Ok(path.join(format!("set_{}.json", set_number)))
}

pub fn get_set_events_file_path(
    team_id: &Uuid,
    match_id: &str,
    set_number: u8,
) -> Result<PathBuf, AppError> {
    let path = get_match_folder_path(team_id, match_id)?;
    Ok(path.join(format!("set_{}.csv", set_number)))
}

pub fn get_config_file_path() -> Result<PathBuf, AppError> {
    let mut path: PathBuf = get_base_path()?;
    path.push("config.json");
    Ok(path)
}
