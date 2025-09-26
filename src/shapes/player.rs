use crate::shapes::enums::RoleEnum;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerEntry {
    pub id: Uuid,
    pub name: String,
    pub role: RoleEnum,
    pub number: u8,
}

impl std::fmt::Display for PlayerEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, self.number)
    }
}
