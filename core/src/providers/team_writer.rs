use crate::{
    errors::AppError,
    shapes::{
        country::CountryCode,
        enums::{GenderEnum, RoleEnum, TeamClassificationEnum},
        player::PlayerEntry,
        team::TeamEntry,
    },
};
use async_trait::async_trait;
use chrono::NaiveDate;
use uuid::Uuid;

pub enum TeamInput {
    New {
        id: Option<Uuid>,
        name: String,
        year: u16,
        classification: Option<TeamClassificationEnum>,
        gender: Option<GenderEnum>,
        country: Option<CountryCode>,
        players: Vec<PlayerEntry>,
    },
    Existing(TeamEntry),
}

/// What a player has beyond name, role and number: all optional.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerDetails {
    pub birth_date: Option<NaiveDate>,
    /// in centimetres
    pub height_cm: Option<u16>,
    pub notes: String,
}

pub enum PlayerInput {
    New {
        name: String,
        role: RoleEnum,
        number: u8,
        details: PlayerDetails,
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

    /// Removes the player from the team: out of its file when no match
    /// tells of them, otherwise kept for those matches as
    /// `player_form::removed` leaves them.
    async fn remove_player(&self, player_id: Uuid, team: &mut TeamEntry) -> Result<(), AppError>;

    /// Removes the team with its matches, for good.
    async fn remove(&self, team_id: &Uuid) -> Result<(), AppError>;
}
