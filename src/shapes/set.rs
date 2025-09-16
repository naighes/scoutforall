use crate::{
    errors::{AppError, MatchError},
    shapes::{
        enums::{EventTypeEnum, PhaseEnum, TeamSideEnum},
        snapshot::{EventEntry, Snapshot},
    },
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SetEntry {
    #[serde(skip_serializing, skip_deserializing)]
    pub set_number: u8,
    pub serving_team: TeamSideEnum,
    pub initial_positions: [Uuid; 6],
    pub libero: Uuid,
    pub fallback_libero: Option<Uuid>,
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
        fallback_libero: Option<Uuid>,
        setter: Uuid,
    ) -> Result<Self, AppError> {
        if !(1..=5).contains(&set_number) {
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
                    fallback_libero,
                    setter: *s,
                    events: vec![],
                }),
            }
        }
    }

    pub fn has_events(&self) -> bool {
        !self.events.is_empty()
    }

    pub fn compute_snapshot(&self) -> Result<(Snapshot, Vec<EventTypeEnum>), AppError> {
        // prepare the initial snapshot
        let mut snapshot = Snapshot::new(self)?;
        // prepare initial available options
        let mut available_options: Vec<EventTypeEnum> = vec![];
        if self.has_events() {
            for event in &self.events {
                available_options = snapshot.add_event(event, available_options.clone())?;
            }
        } else {
            // there are no events: set is just started
            available_options = match snapshot.current_lineup.get_current_phase() {
                PhaseEnum::SideOut => vec![
                    EventTypeEnum::P,
                    EventTypeEnum::OS,
                    EventTypeEnum::OE,
                    EventTypeEnum::F,
                    EventTypeEnum::R,
                ],
                _ => vec![
                    EventTypeEnum::S,
                    EventTypeEnum::F,
                    EventTypeEnum::R,
                    EventTypeEnum::OE,
                ],
            }
        }
        Ok((snapshot, available_options))
    }
}
