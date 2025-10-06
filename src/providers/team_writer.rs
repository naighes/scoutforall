use crate::{
    errors::AppError,
    shapes::{
        enums::{GenderEnum, RoleEnum, TeamClassificationEnum},
        player::PlayerEntry,
        team::TeamEntry,
    },
};
use async_trait::async_trait;
use uuid::Uuid;

pub enum TeamInput {
    New {
        id: Option<Uuid>,
        name: String,
        year: u16,
        classification: Option<TeamClassificationEnum>,
        gender: Option<GenderEnum>,
        players: Vec<PlayerEntry>,
    },
    Existing(TeamEntry),
}

pub enum PlayerInput {
    New {
        name: String,
        role: RoleEnum,
        number: u8,
    },
    Existing(PlayerEntry),
}

#[async_trait]
pub trait TeamWriter {
    async fn save(&self, team: TeamInput) -> Result<TeamEntry, AppError>;
    async fn save_player(
        &self,
        player: PlayerInput,
        team: &mut TeamEntry,
    ) -> Result<PlayerEntry, AppError>;
}
