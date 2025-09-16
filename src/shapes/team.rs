use crate::shapes::{
    enums::{GenderEnum, TeamClassificationEnum},
    player::PlayerEntry,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TeamEntry {
    pub name: String,
    pub year: u16,
    pub players: Vec<PlayerEntry>,
    #[serde(skip_serializing, skip_deserializing)]
    pub id: Uuid,
    #[serde(default)]
    pub classification: Option<TeamClassificationEnum>,
    #[serde(default)]
    pub gender: Option<GenderEnum>,
}

impl TeamEntry {
    pub fn find_player(&self, player_id: Uuid) -> Option<&PlayerEntry> {
        self.players.iter().find(|p| p.id == player_id)
    }
}
