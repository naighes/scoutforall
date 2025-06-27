use crate::shapes::player::PlayerEntry;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TeamEntry {
    pub name: String,
    pub league: String,
    pub year: u16,
    pub players: Vec<PlayerEntry>,
    #[serde(skip_serializing, skip_deserializing)]
    pub id: Uuid,
}
