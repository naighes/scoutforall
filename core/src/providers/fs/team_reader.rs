use crate::{
    constants::TEAM_DESCRIPTOR_FILE_NAME,
    errors::{AppError, IOError},
    localization::{Labels, Language},
    logging::logger::log_error,
    providers::team_reader::TeamReader,
    shapes::{
        listed::{Listed, Unreadable},
        team::TeamEntry,
    },
};
use async_trait::async_trait;
use futures::{TryStreamExt, future::ready};
use serde_json::from_str;
use std::path::{Path, PathBuf};
use tokio::fs::{read_dir, read_to_string};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReadDirStream;
use uuid::Uuid;

pub struct FileSystemTeamReader {
    base_path: PathBuf,
    language: Language,
}

impl FileSystemTeamReader {
    pub fn new(base_path: &Path, language: Language) -> Self {
        Self {
            base_path: base_path.to_path_buf(),
            language,
        }
    }

    async fn map_entry(path: &Path, labels: &Labels) -> Result<TeamEntry, AppError> {
        let content = read_to_string(path.join(TEAM_DESCRIPTOR_FILE_NAME))
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        let team: TeamEntry = from_str(&content).map_err(|e| AppError::IO(IOError::from(e)))?;
        let uuid = path
            .file_name()
            .and_then(|os| os.to_str())
            .and_then(|name| Uuid::parse_str(name).ok())
            .ok_or_else(|| {
                AppError::IO(IOError::Msg(labels.invalid_team_folder_name.to_string()))
            })?;

        Ok(TeamEntry { id: uuid, ..team })
    }
}

#[async_trait]
impl TeamReader for FileSystemTeamReader {
    async fn read_all(&self) -> Result<Vec<Listed<TeamEntry>>, AppError> {
        let base_path = &self.base_path;
        let labels = self.language.labels();
        let dir = read_dir(&base_path)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        let entries = ReadDirStream::new(dir)
            .try_filter(|e| ready(e.path().is_dir()))
            .then(|res| async {
                let path = res.map_err(|e| AppError::IO(IOError::from(e)))?.path();
                Ok::<_, AppError>(
                    FileSystemTeamReader::map_entry(&path, labels)
                        .await
                        .map_err(|e| {
                            let unreadable = Unreadable {
                                id: path
                                    .file_name()
                                    .map(|s| s.to_string_lossy().into_owned())
                                    .unwrap_or_default(),
                            };
                            log_error(&format!("team {:?} is unreadable: {}", unreadable.id, e));
                            unreadable
                        }),
                )
            })
            .try_collect::<Vec<_>>()
            .await?;
        Ok(entries)
    }

    async fn read_single(&self, team_id: &Uuid) -> Result<TeamEntry, AppError> {
        let path = &self.base_path.join(team_id.to_string());
        FileSystemTeamReader::map_entry(path, self.language.labels()).await
    }

    async fn exists(&self, team_id: &Uuid) -> Result<bool, AppError> {
        let path = &self.base_path.join(team_id.to_string());
        Ok(path.exists())
    }
}
