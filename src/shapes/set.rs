use crate::{
    errors::{AppError, MatchError},
    shapes::{enums::TeamSideEnum, snapshot::EventEntry},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone)]
pub struct SetEntry {
    #[serde(skip_serializing, skip_deserializing)]
    pub set_number: u8,
    pub serving_team: TeamSideEnum,
    pub initial_positions: [Uuid; 6],
    pub libero: Uuid,
    pub setter: Uuid,
    #[serde(skip_serializing, skip_deserializing)]
    pub events: Vec<EventEntry>,
}

impl SetEntry {
    pub fn new(
        set_number: u8,
        serving_team: TeamSideEnum,
        initial_positions: [Uuid; 6],
        libero: Uuid,
        setter: Uuid,
    ) -> Result<Self, AppError> {
        if set_number < 1 || set_number > 5 {
            Err(AppError::Match(MatchError::SetEntryError(format!(
                "{} is not a valid set number",
                set_number
            ))))
        } else {
            match initial_positions.iter().find(|p| **p == setter) {
                None => Err(AppError::Match(MatchError::SetEntryError(format!(
                    "setter {} is not into the lineup",
                    setter
                )))),
                Some(s) => Ok(SetEntry {
                    set_number,
                    serving_team,
                    initial_positions,
                    libero,
                    setter: *s,
                    events: vec![],
                }),
            }
        }
    }

    // TODO: remove?
    pub fn initial_rotation(&self) -> Option<u8> {
        self.initial_positions
            .iter()
            .position(|id| *id == self.setter)
            .map(|x| x as u8)
    }

    pub fn has_events(&self) -> bool {
        self.events.len() > 0
    }
}
