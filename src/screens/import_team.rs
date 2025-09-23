use crate::{
    errors::{AppError, IOError},
    io::path::get_team_folder_path,
    localization::current_labels,
    ops::{save_team, TeamInput},
    screens::{file_system_screen::FileSystemAction, screen::AppAction},
    shapes::team::TeamEntry,
};
use hf::is_hidden;
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};
use uuid::Uuid;
use zip::ZipArchive;

pub struct ImportTeamAction;

impl FileSystemAction for ImportTeamAction {
    fn is_selectable(&self, path: &Path) -> bool {
        !is_hidden(path).unwrap_or_default()
            && path
                .extension()
                .and_then(|s| s.to_str())
                .map(|e| e.eq_ignore_ascii_case("zip"))
                .unwrap_or(false)
    }

    fn is_visible(&self, path: &Path) -> bool {
        !is_hidden(path).unwrap_or_default()
            && (path.is_dir()
                || path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|e| e.eq_ignore_ascii_case("zip"))
                    .unwrap_or(false))
    }

    fn on_selected(&mut self, path: &Path) -> Result<AppAction, AppError> {
        path.file_stem()
            .and_then(|s| s.to_str())
            .ok_or(AppError::IO(IOError::Msg(
                current_labels().invalid_file_name.to_string(),
            )))
            .and_then(|stem| {
                Uuid::parse_str(stem).map_err(|_| {
                    AppError::IO(IOError::Msg(current_labels().invalid_file_name.to_string()))
                })
            })
            .and_then(|team_id| {
                if get_team_folder_path(&team_id)?.exists() {
                    Err(AppError::IO(IOError::Msg(
                        current_labels().team_already_exists.to_string(),
                    )))
                } else {
                    Ok(team_id)
                }
            })
            .and_then(|_| fs::File::open(path).map_err(|e| AppError::IO(IOError::from(e))))
            .and_then(|mut f| ImportTeamAction::json_content(&mut f))
            .and_then(|json| {
                serde_json::from_str(&json).map_err(|e| AppError::IO(IOError::from(e)))
            })
            .and_then(|team: TeamEntry| {
                save_team(TeamInput::New {
                    name: team.name,
                    year: team.year,
                    classification: team.classification,
                    gender: team.gender,
                })
                .map_err(|e| AppError::IO(IOError::from(e)))
            })?;
        Ok(AppAction::Back(true, Some(1)))
    }
}

impl ImportTeamAction {
    fn json_content(file: &mut File) -> Result<String, AppError> {
        let mut archive = ZipArchive::new(file).map_err(|e| AppError::IO(IOError::from(e)))?;
        let mut team_json_content = String::new();
        let mut found = false;
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| AppError::IO(IOError::from(e)))?;
            if file.name().eq_ignore_ascii_case("team.json") {
                file.read_to_string(&mut team_json_content)
                    .map_err(|e| AppError::IO(IOError::from(e)))?;
                found = true;
                break;
            }
        }
        match found {
            true => Ok(team_json_content),
            false => Err(AppError::IO(IOError::Msg(
                "unexpected content within the zip archive".to_string(),
            ))),
        }
    }
}
