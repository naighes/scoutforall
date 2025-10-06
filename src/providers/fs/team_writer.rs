use std::path::{Path, PathBuf};

use crate::{
    constants::TEAM_DESCRIPTOR_FILE_NAME,
    errors::{AppError, IOError},
    providers::{
        fs::path::get_team_folder_path,
        team_writer::{PlayerInput, TeamInput, TeamWriter},
    },
    shapes::{player::PlayerEntry, team::TeamEntry},
};
use async_trait::async_trait;
use serde_json::to_vec_pretty;
use tokio::fs::{create_dir_all, write};
use uuid::Uuid;

pub struct FileSystemTeamWriter(PathBuf);

impl FileSystemTeamWriter {
    pub fn new(base_path: &Path) -> Self {
        Self(base_path.to_path_buf())
    }

    async fn save_team_file(team: &TeamEntry, base_path: &Path) -> Result<(), AppError> {
        let folder = get_team_folder_path(base_path, &team.id)?;
        create_dir_all(&folder)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        let path = folder.join(TEAM_DESCRIPTOR_FILE_NAME);
        let json = to_vec_pretty(team).map_err(|e| AppError::IO(IOError::from(e)))?;
        write(&path, json)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        Ok(())
    }
}

#[async_trait]
impl TeamWriter for FileSystemTeamWriter {
    async fn save(&self, input: TeamInput) -> Result<TeamEntry, AppError> {
        let team = match input {
            TeamInput::New {
                id,
                name,
                classification,
                gender,
                year,
                players,
            } => TeamEntry {
                id: if let Some(id) = id {
                    id
                } else {
                    Uuid::new_v4()
                },
                name,
                classification,
                gender,
                year,
                players,
            },
            TeamInput::Existing(team) => team,
        };

        Self::save_team_file(&team, &self.0).await?;
        Ok(team)
    }

    async fn save_player(
        &self,
        input: PlayerInput,
        team: &mut TeamEntry,
    ) -> Result<PlayerEntry, AppError> {
        let player = match input {
            PlayerInput::New { name, role, number } => PlayerEntry {
                id: Uuid::new_v4(),
                name,
                role,
                number,
            },
            PlayerInput::Existing(existing) => existing,
        };
        if let Some(existing) = team.players.iter_mut().find(|p| p.id == player.id) {
            *existing = player.clone();
        } else {
            team.players.push(player.clone());
        }
        Self::save_team_file(team, &self.0).await?;
        Ok(player)
    }
}
