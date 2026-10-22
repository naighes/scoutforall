use crate::localization::current_labels;
use crate::screens::{file_system_screen::FileSystemAction, screen::AppAction};
use async_trait::async_trait;
use hf::is_hidden;
use scout4all_core::{
    archive::import_team,
    errors::{AppError, IOError},
    providers::{team_reader::TeamReader, team_writer::TeamWriter},
};
use std::{fs, path::Path, sync::Arc};
use uuid::Uuid;

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
        let archive = fs::read(path).map_err(|e| AppError::IO(IOError::from(e)))?;
        import_team(
            &archive,
            team_id,
            &*self.team_reader,
            &*self.team_writer,
            current_labels().domain,
        )
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
}
