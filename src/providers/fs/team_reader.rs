use crate::{
    constants::TEAM_DESCRIPTOR_FILE_NAME,
    errors::{AppError, IOError},
    providers::team_reader::TeamReader,
    shapes::team::TeamEntry,
};
use async_trait::async_trait;
use futures::{future::ready, TryStreamExt};
use serde_json::from_str;
use std::path::{Path, PathBuf};
use tokio::fs::{read_dir, read_to_string};
use tokio_stream::wrappers::ReadDirStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

pub struct FileSystemTeamReader(PathBuf);

impl FileSystemTeamReader {
    pub fn new(base_path: &Path) -> Self {
        Self(base_path.to_path_buf())
    }

    async fn map_entry(path: &Path) -> Result<TeamEntry, AppError> {
        let content = read_to_string(path.join(TEAM_DESCRIPTOR_FILE_NAME))
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        let team: TeamEntry = from_str(&content).map_err(|e| AppError::IO(IOError::from(e)))?;
        let uuid = path
            .file_name()
            .and_then(|os| os.to_str())
            .and_then(|name| Uuid::parse_str(name).ok())
            .ok_or_else(|| {
                AppError::IO(IOError::Msg(format!(
                    "invalid team folder name '{}'",
                    path.display()
                )))
            })?;

        Ok(TeamEntry { id: uuid, ..team })
    }
}

#[async_trait]
impl TeamReader for FileSystemTeamReader {
    async fn read_all(&self) -> Result<Vec<TeamEntry>, AppError> {
        let base_path = &self.0;
        let dir = read_dir(&base_path)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        let entries = ReadDirStream::new(dir)
            .try_filter(|e| ready(e.path().is_dir()))
            .then(|res| async {
                match res {
                    Ok(e) => FileSystemTeamReader::map_entry(&e.path()).await,
                    Err(err) => Err(AppError::IO(IOError::from(err))),
                }
            })
            .try_collect::<Vec<_>>()
            .await?;
        Ok(entries)
    }

    async fn read_single(&self, team_id: &Uuid) -> Result<TeamEntry, AppError> {
        let path = &self.0.join(team_id.to_string());
        FileSystemTeamReader::map_entry(path).await
    }

    async fn exists(&self, team_id: &Uuid) -> Result<bool, AppError> {
        let path = &self.0.join(team_id.to_string());
        Ok(path.exists())
    }
}
