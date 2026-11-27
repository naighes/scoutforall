use crate::shapes::{country::CountryCode, enums::RoleEnum};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DominantHand {
    Right,
    Left,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nationality: Option<CountryCode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dominant_hand: Option<DominantHand>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub federation_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    /// in E.164 (`+393331234567`)
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub birth_place: String,
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
            nationality: None,
            dominant_hand: None,
            federation_id: String::new(),
            email: String::new(),
            phone: String::new(),
            birth_place: String::new(),
        }
    }
}

impl std::fmt::Display for PlayerEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.name, self.number)
    }
}
