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
