use crate::{
    errors::AppError,
    providers::{
        store::write_json,
        team_writer::{PlayerInput, TeamInput, TeamWriter},
    },
    shapes::{player::PlayerEntry, team::TeamEntry},
    storage::{Store, layout},
};
use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

pub struct StoreTeamWriter(Arc<dyn Store>);

impl StoreTeamWriter {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self(store)
    }

    async fn save_team_file(&self, team: &TeamEntry) -> Result<(), AppError> {
        write_json(&*self.0, &layout::team_descriptor(&team.id), team).await
    }
}

#[async_trait]
impl TeamWriter for StoreTeamWriter {
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

        self.save_team_file(&team).await?;
        Ok(team)
    }

    async fn save_player(
        &self,
        input: PlayerInput,
        team: &mut TeamEntry,
    ) -> Result<PlayerEntry, AppError> {
        let player = match input {
            PlayerInput::New { name, role, number } => PlayerEntry {
                name,
                role: Some(role),
                number,
                ..Default::default()
            },
            PlayerInput::Existing(existing) => existing,
        };
        if let Some(existing) = team.players.iter_mut().find(|p| p.id == player.id) {
            *existing = player.clone();
        } else {
            team.players.push(player.clone());
        }
        self.save_team_file(team).await?;
        Ok(player)
    }
}
