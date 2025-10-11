use crate::{
    errors::{AppError, IOError, MatchError},
    providers::{fs::path::get_match_descriptor_file_path, match_writer::MatchWriter},
    shapes::{r#match::MatchEntry, team::TeamEntry},
    util::sanitize_filename,
};
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset};
use serde_json::to_vec_pretty;
use std::path::{Path, PathBuf};
use tokio::{
    fs::{try_exists, File},
    io::AsyncWriteExt,
};

pub struct FileSystemMatchWriter(PathBuf);

impl FileSystemMatchWriter {
    pub fn new(base_path: &Path) -> Self {
        Self(base_path.to_path_buf())
    }
}

#[async_trait]
impl MatchWriter for FileSystemMatchWriter {
    async fn create(
        &self,
        team: &TeamEntry,
        opponent: String,
        date: DateTime<FixedOffset>,
        home: bool,
    ) -> Result<MatchEntry, AppError> {
        let opponent_clean = sanitize_filename(&opponent);
        let date_str = date.format("%Y-%m-%d").to_string();
        let match_id = format!("{}_{}", date_str, opponent_clean);
        let m = MatchEntry {
            opponent,
            date,
            id: match_id.clone(),
            team: team.clone(),
            home,
            sets: vec![],
        };
        let path = get_match_descriptor_file_path(&self.0, &team.id, &match_id)?;
        if try_exists(&path)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?
        {
            return Err(AppError::Match(MatchError::MatchAlreadyExists(match_id)));
        }
        let mut file = File::create(&path)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        let json = to_vec_pretty(&m).map_err(|e| AppError::IO(IOError::from(e)))?;
        file.write_all(&json)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        Ok(m)
    }
}
