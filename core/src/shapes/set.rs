use crate::{
    errors::{AppError, MatchError},
    shapes::{
        enums::{LineupRulesEnum, TeamSideEnum},
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
    // missing in the sets saved up to v0.3.0
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineup_rules: Option<LineupRulesEnum>,
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
                    lineup_rules: Some(LineupRulesEnum::CURRENT),
                    events: vec![],
                }),
            }
        }
    }

    /// Replays the events of the set; `max_substitutions` comes from the
    /// match.
    pub fn compute_snapshot(&self, max_substitutions: u8) -> Result<Snapshot, AppError> {
        match self.lineup_rules {
            Some(rules) => self.replay(max_substitutions, rules),
            // saved without the rules: scouted up to v0.2.0, unless only the
            // rules of v0.3.0 (which didn't save them) replay the set
            None => self
                .replay(max_substitutions, LineupRulesEnum::V1)
                .or_else(|_| self.replay(max_substitutions, LineupRulesEnum::V2)),
        }
    }

    /// The rules the set replays with, found as `compute_snapshot` does: to
    /// save them with the sets saved without.
    pub fn find_lineup_rules(&self, max_substitutions: u8) -> Result<LineupRulesEnum, AppError> {
        match self.lineup_rules {
            Some(rules) => Ok(rules),
            None => self
                .replay(max_substitutions, LineupRulesEnum::V1)
                .map(|_| LineupRulesEnum::V1)
                .or_else(|_| {
                    self.replay(max_substitutions, LineupRulesEnum::V2)
                        .map(|_| LineupRulesEnum::V2)
                }),
        }
    }

    /// Replays the events with the lineup rules: every event is validated,
    /// as when it's scouted.
    fn replay(&self, max_substitutions: u8, rules: LineupRulesEnum) -> Result<Snapshot, AppError> {
        let mut snapshot = Snapshot::with_rules(self, max_substitutions, rules)?;
        for event in &self.events {
            snapshot.add_event(event)?;
        }
        Ok(snapshot)
    }
}
