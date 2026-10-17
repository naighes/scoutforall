use crate::{
    constants::MATCH_DESCRIPTOR_FILE_NAME,
    errors::{AppError, IOError},
    localization::current_labels,
    logging::logger::log_error,
    providers::{fs::path::get_team_folder_path, match_reader::MatchReader, set_reader::SetReader},
    shapes::{
        enums::LineupRulesEnum,
        listed::{Listed, Unreadable},
        r#match::MatchEntry,
        team::TeamEntry,
    },
};
use async_trait::async_trait;
use futures::{TryStreamExt, future::ready};
use serde_json::{Value, from_str, from_value};
use std::{
    cmp::Ordering,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs::{ReadDir, read_dir, read_to_string, try_exists};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReadDirStream;

pub struct FileSystemMatchReader {
    base_path: PathBuf,
    set_reader: Arc<dyn SetReader + Send + Sync>,
}

#[async_trait]
impl MatchReader for FileSystemMatchReader {
    async fn read_all(&self, team: &TeamEntry) -> Result<Vec<Listed<MatchEntry>>, AppError> {
        let team_path = get_team_folder_path(&self.base_path, &team.id)?;
        let dir: ReadDir = read_dir(&team_path)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        let mut entries: Vec<Listed<MatchEntry>> = ReadDirStream::new(dir)
            .try_filter(|e| ready(e.path().is_dir()))
            .then(|res| async {
                let path = res.map_err(|e| AppError::IO(IOError::from(e)))?.path();
                Ok::<_, AppError>(self.map_entry(&path, team).await.map_err(|e| {
                    let unreadable = Unreadable {
                        id: path
                            .file_name()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    };
                    log_error(&format!(
                        "match {:?} of team {} is unreadable: {}",
                        unreadable.id, team.id, e
                    ));
                    unreadable
                }))
            })
            .try_collect()
            .await?;
        // the most recent first, then the unreadable ones by name
        entries.sort_by(|a, b| match (a, b) {
            (Ok(a), Ok(b)) => b.date.cmp(&a.date),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(a), Err(b)) => a.id.cmp(&b.id),
        });
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
        let descriptor: Value = from_str(&content).map_err(|e| AppError::IO(IOError::from(e)))?;
        // only matches created since v0.3.0 save the substitutions
        let created_since_v0_3_0 = descriptor.get("max_substitutions").is_some();
        let mut entry: MatchEntry =
            from_value(descriptor).map_err(|e| AppError::IO(IOError::from(e)))?;
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
        if created_since_v0_3_0 {
            // v0.3.0 didn't save the lineup rules of its sets, but scouted
            // them with these ones
            for set in entry.sets.iter_mut() {
                set.lineup_rules.get_or_insert(LineupRulesEnum::V2);
            }
        }
        // a match whose sets don't replay can't be shown nor scouted
        entry.get_status()?;
        Ok(entry)
    }
}
