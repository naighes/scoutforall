use crate::{
    errors::AppError,
    player_form::removed,
    providers::{
        store::write_json,
        team_writer::{PlayerInput, TeamInput, TeamWriter},
    },
    shapes::{player::PlayerEntry, team::TeamEntry},
    storage::{Store, StoreEntry, layout},
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

    /// Whether a file of a match of the team tells of the player: the
    /// roster, a lineup, an event. Read as text, so that a match that
    /// can't be read, or written by another version, counts all the same.
    async fn tells_of(&self, team_id: &Uuid, player_id: &Uuid) -> Result<bool, AppError> {
        let id = player_id.to_string();
        let team_folder = layout::team_folder(team_id);
        for folder in self.0.list(&team_folder).await? {
            let StoreEntry::Folder(folder) = folder else {
                continue;
            };
            let folder = layout::child(&team_folder, &folder);
            let files = self
                .0
                .list(&folder)
                .await?
                .into_iter()
                .filter_map(|f| match f {
                    StoreEntry::Blob(file) => Some(layout::child(&folder, &file)),
                    StoreEntry::Folder(_) => None,
                });
            for file in files {
                let data = self.0.read(&file).await?;
                if data.is_some_and(|d| String::from_utf8_lossy(&d).contains(&id)) {
                    return Ok(true);
                }
            }
        }
        Ok(false)
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
                country,
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
                country,
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
            PlayerInput::New {
                name,
                role,
                number,
                details,
            } => PlayerEntry {
                name,
                role: Some(role),
                number,
                birth_date: details.birth_date,
                height_cm: details.height_cm,
                notes: details.notes,
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

    async fn remove_player(&self, player_id: Uuid, team: &mut TeamEntry) -> Result<(), AppError> {
        let told = self.tells_of(&team.id, &player_id).await?;
        let players = std::mem::take(&mut team.players);
        team.players = players
            .into_iter()
            .filter_map(|p| match (p.id == player_id, told) {
                (false, _) => Some(p),
                (true, true) => Some(removed(&p)),
                (true, false) => None,
            })
            .collect();
        self.save_team_file(team).await
    }

    async fn remove(&self, team_id: &Uuid) -> Result<(), AppError> {
        self.0.remove_folder(&layout::team_folder(team_id)).await
    }
}
