use crate::{
    errors::{AppError, IOError},
    localization::current_labels,
    providers::{
        team_reader::TeamReader,
        team_writer::{TeamInput, TeamWriter},
    },
    screens::{file_system_screen::FileSystemAction, screen::AppAction},
    shapes::team::TeamEntry,
};
use async_trait::async_trait;
use hf::is_hidden;
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
    sync::Arc,
};
use uuid::Uuid;
use zip::ZipArchive;

pub struct ImportTeamAction<TR: TeamReader + Send + Sync, TW: TeamWriter + Send + Sync> {
    team_reader: Arc<TR>,
    team_writer: Arc<TW>,
}

#[async_trait]
impl<TR: TeamReader + Send + Sync, TW: TeamWriter + Send + Sync> FileSystemAction
    for ImportTeamAction<TR, TW>
{
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

    async fn on_selected(&mut self, path: &Path) -> Result<AppAction, AppError> {
        // selected file name
        let stem = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
            AppError::IO(IOError::Msg(current_labels().invalid_file_name.to_string()))
        })?;
        // file name must be a valid UUID
        let team_id = Uuid::parse_str(stem).map_err(|_| {
            AppError::IO(IOError::Msg(current_labels().invalid_file_name.to_string()))
        })?;
        // check if the team already exists
        let exists = self.team_reader.exists(&team_id).await?;
        if exists {
            return Err(AppError::IO(IOError::Msg(
                current_labels().team_already_exists.to_string(),
            )));
        }
        // attempt to open the file
        let mut file = fs::File::open(path).map_err(|e| AppError::IO(IOError::from(e)))?;
        let json = Self::json_content(&mut file)?;
        let team: TeamEntry =
            serde_json::from_str(&json).map_err(|e| AppError::IO(IOError::from(e)))?;
        self.team_writer
            .save(TeamInput::New {
                id: Some(team_id),
                name: team.name,
                year: team.year,
                classification: team.classification,
                gender: team.gender,
                players: team.players,
            })
            .await?;
        Ok(AppAction::Back(true, Some(1)))
    }
}

impl<TR: TeamReader + Send + Sync, TW: TeamWriter + Send + Sync> ImportTeamAction<TR, TW> {
    pub fn new(team_reader: Arc<TR>, team_writer: Arc<TW>) -> Self {
        Self {
            team_reader,
            team_writer,
        }
    }

    fn json_content(file: &mut File) -> Result<String, AppError> {
        let mut archive = ZipArchive::new(file).map_err(|e| AppError::IO(IOError::from(e)))?;
        let mut team_json_content = String::new();
        let mut found = false;
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| AppError::IO(IOError::from(e)))?;
            // look for the team.json file
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
