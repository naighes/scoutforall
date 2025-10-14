use crate::shapes::enums::RoleEnum;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerEntry {
    pub id: Uuid,
    pub name: String,
    pub role: Option<RoleEnum>,
    pub number: u8,
    #[serde(default)]
    pub deleted: bool,
}

impl Default for PlayerEntry {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            role: None,
            number: 0,
            deleted: false,
        }
    }
}

impl std::fmt::Display for PlayerEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, self.number)
    }
}
