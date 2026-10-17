use crate::{
    errors::{AppError, MatchError},
    shapes::{
        enums::{EventTypeEnum, LineupRulesEnum, TeamSideEnum},
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
    pub fn compute_snapshot(
        &self,
        max_substitutions: u8,
    ) -> Result<(Snapshot, Vec<EventTypeEnum>), AppError> {
        match self.lineup_rules {
            Some(rules) => self.replay(max_substitutions, rules),
            // saved without the rules: scouted up to v0.2.0, unless only the
            // rules of v0.3.0 (which didn't save them) replay the set
            None => self
                .replay(max_substitutions, LineupRulesEnum::V1)
                .or_else(|_| self.replay(max_substitutions, LineupRulesEnum::V2)),
        }
    }

    fn replay(
        &self,
        max_substitutions: u8,
        rules: LineupRulesEnum,
    ) -> Result<(Snapshot, Vec<EventTypeEnum>), AppError> {
        // prepare the initial snapshot
        let mut snapshot = Snapshot::with_rules(self, max_substitutions, rules)?;
        // the events available before the first serve: also the ones after a
        // substitution, setter or libero change opening the set
        let mut available_options =
            snapshot.get_rally_start_options(snapshot.current_lineup.get_current_phase());
        for event in &self.events {
            available_options = snapshot.add_event(event, &available_options)?;
        }
        Ok((snapshot, available_options))
    }
}
