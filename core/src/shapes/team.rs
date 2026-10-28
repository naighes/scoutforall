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
    pub fn active_players(&self) -> Vec<&PlayerEntry> {
        self.players.iter().filter(|p| !p.deleted).collect()
    }

    /// The team as a match with `roster` sees it: the players of the roster
    /// active, also when removed since, the others as removed; still all
    /// there, for the events of the match. The team itself without a
    /// roster (a match saved before v0.11).
    pub fn with_roster(&self, roster: &[Uuid]) -> TeamEntry {
        match roster {
            [] => self.clone(),
            _ => TeamEntry {
                players: self
                    .players
                    .iter()
                    .map(|p| PlayerEntry {
                        deleted: !roster.contains(&p.id),
                        ..p.clone()
                    })
                    .collect(),
                ..self.clone()
            },
        }
    }
}
