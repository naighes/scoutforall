use crate::{
    constants::MATCH_DESCRIPTOR_FILE_NAME,
    errors::{AppError, IOError},
    localization::current_labels,
    providers::{fs::path::get_team_folder_path, match_reader::MatchReader, set_reader::SetReader},
    shapes::{r#match::MatchEntry, team::TeamEntry},
};
use async_trait::async_trait;
use futures::{future::ready, TryStreamExt};
use serde_json::from_str;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs::{read_dir, read_to_string, try_exists, ReadDir};
use tokio_stream::wrappers::ReadDirStream;
use tokio_stream::StreamExt;

pub struct FileSystemMatchReader {
    base_path: PathBuf,
    set_reader: Arc<dyn SetReader + Send + Sync>,
}

#[async_trait]
impl MatchReader for FileSystemMatchReader {
    async fn read_all(&self, team: &TeamEntry) -> Result<Vec<MatchEntry>, AppError> {
        let team_path = get_team_folder_path(&self.base_path, &team.id)?;
        let dir: ReadDir = read_dir(&team_path)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        let mut entries: Vec<_> = ReadDirStream::new(dir)
            .try_filter(|e| ready(e.path().is_dir()))
            .then(|res| async {
                match res {
                    Ok(e) => self.map_entry(&e.path(), team).await,
                    Err(err) => Err(AppError::IO(IOError::from(err))),
                }
            })
            .try_collect::<Vec<_>>()
            .await?;
        entries.sort_by(|a, b| b.date.cmp(&a.date));
        Ok(entries)
    }

    async fn read_single(&self, team: &TeamEntry, match_id: &str) -> Result<MatchEntry, AppError> {
        let path = get_team_folder_path(&self.base_path, &team.id)?.join(match_id);
        self.map_entry(&path, team).await
    }

    async fn exists(&self, team: &TeamEntry, match_id: &str) -> Result<bool, AppError> {
        let team_path = get_team_folder_path(&self.base_path, &team.id)?.join(match_id);
        try_exists(&team_path)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))
    }
}

impl FileSystemMatchReader {
    pub fn new(base_path: &Path, set_reader: Arc<dyn SetReader + Send + Sync>) -> Self {
        Self {
            base_path: base_path.to_path_buf(),
            set_reader,
        }
    }

    async fn map_entry(&self, path: &Path, team: &TeamEntry) -> Result<MatchEntry, AppError> {
        let descriptor_path = path.join(MATCH_DESCRIPTOR_FILE_NAME);
        if !descriptor_path.exists() {
            return Err(AppError::IO(IOError::Msg(
                current_labels().match_descriptor_file_not_found.to_string(),
            )));
        }
        let content = read_to_string(&descriptor_path)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        let mut entry: MatchEntry =
            from_str(&content).map_err(|e| AppError::IO(IOError::from(e)))?;
        entry.id = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| {
                AppError::IO(IOError::Msg(
                    current_labels().invalid_match_folder_name.to_string(),
                ))
            })?
            .into();
        entry.team = team.clone();
        entry.sets = self.set_reader.read_all(&entry).await?;
        Ok(entry)
    }
}
