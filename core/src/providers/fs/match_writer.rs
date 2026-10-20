use crate::{
    errors::{AppError, IOError, MatchError},
    providers::{
        fs::path::{get_match_descriptor_file_path, get_set_descriptor_file_path},
        match_writer::MatchWriter,
    },
    shapes::{r#match::MatchEntry, set::SetEntry, team::TeamEntry},
    util::sanitize_filename,
};
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset};
use serde_json::to_vec_pretty;
use std::path::{Path, PathBuf};
use tokio::fs::{try_exists, write};

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
        max_substitutions: u8,
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
            max_substitutions,
            sets: vec![],
        };
        let path = get_match_descriptor_file_path(&self.0, &team.id, &match_id)?;
        if try_exists(&path)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?
        {
            return Err(AppError::Match(MatchError::MatchAlreadyExists(match_id)));
        }
        let json = to_vec_pretty(&m).map_err(|e| AppError::IO(IOError::from(e)))?;
        // written when awaited: a tokio file isn't, unless flushed
        write(&path, json)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        Ok(m)
    }

    async fn update(
        &self,
        m: &MatchEntry,
        opponent: String,
        date: DateTime<FixedOffset>,
        home: bool,
        max_substitutions: u8,
    ) -> Result<(), AppError> {
        // the reader replays the sets saved without the lineup rules with
        // those of v0.3.0 once the descriptor saves the substitutions: they
        // get the rules they replay with, before the descriptor is written
        for set in m.sets.iter().filter(|s| s.lineup_rules.is_none()) {
            let rules = set.find_lineup_rules(m.max_substitutions)?;
            let with_rules = SetEntry {
                lineup_rules: Some(rules),
                ..set.clone()
            };
            let path = get_set_descriptor_file_path(&self.0, &m.team.id, &m.id, set.set_number)?;
            let json = to_vec_pretty(&with_rules).map_err(|e| AppError::IO(IOError::from(e)))?;
            write(&path, json)
                .await
                .map_err(|e| AppError::IO(IOError::from(e)))?;
        }
        let updated = MatchEntry {
            opponent,
            date,
            home,
            max_substitutions,
            id: m.id.clone(),
            team: m.team.clone(),
            sets: m.sets.clone(),
        };
        let path = get_match_descriptor_file_path(&self.0, &m.team.id, &m.id)?;
        let json = to_vec_pretty(&updated).map_err(|e| AppError::IO(IOError::from(e)))?;
        write(&path, json)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        Ok(())
    }
}
