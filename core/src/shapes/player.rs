use crate::shapes::enums::RoleEnum;
use chrono::NaiveDate;
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
    // since v0.10, all optional and written only when given: older files
    // read as before, and are written as before while unused. An older
    // version reads a team with them and drops them if it saves the team
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth_date: Option<NaiveDate>,
    /// in centimetres
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height_cm: Option<u16>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

impl Default for PlayerEntry {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            role: None,
            number: 0,
            deleted: false,
            birth_date: None,
            height_cm: None,
            notes: String::new(),
        }
    }
}

impl std::fmt::Display for PlayerEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, self.number)
    }
}
