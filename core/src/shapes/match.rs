use crate::{
    constants::LEGACY_MAX_SUBSTITUTIONS,
    errors::AppError,
    shapes::{enums::TeamSideEnum, set::SetEntry, snapshot::Snapshot, team::TeamEntry},
    util::sanitize_filename,
};
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug)]
pub struct MatchStatus {
    pub us_wins: u8,
    pub them_wins: u8,
    pub next_set_number: Option<u8>,
    pub last_incomplete_set: Option<SetEntry>,
    pub match_finished: bool,
    /// the last set won or lost, if any
    pub last_set: Option<SetEntry>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MatchEntry {
    pub opponent: String,
    pub date: DateTime<FixedOffset>,
    #[serde(skip_serializing, skip_deserializing)]
    pub id: String,
    /// The team as the match sees it (`TeamEntry::with_roster`).
    #[serde(skip_serializing, skip_deserializing)]
    pub team: TeamEntry,
    pub home: bool,
    // matches saved before the number was configurable used the old rules
    #[serde(default = "legacy_max_substitutions")]
    pub max_substitutions: u8,
    // since v0.10, written only when given: see `PlayerEntry::notes`
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    // since v0.11, the players on the score sheet; written only when given:
    // the older versions read the file as before, with the whole team, and
    // the matches saved before have none
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roster: Vec<Uuid>,
    #[serde(skip_serializing, skip_deserializing)]
    pub sets: Vec<SetEntry>,
}

fn legacy_max_substitutions() -> u8 {
    LEGACY_MAX_SUBSTITUTIONS
}

impl MatchEntry {
    /// The id of a match against the opponent on the date: one a day.
    pub fn id_of(opponent: &str, date: &DateTime<FixedOffset>) -> String {
        format!(
            "{}_{}",
            date.format("%Y-%m-%d"),
            sanitize_filename(opponent)
        )
    }

    pub fn get_status(&self) -> Result<MatchStatus, AppError> {
        let mut us_wins = 0;
        let mut them_wins = 0;
        let mut next_set_number = 1;
        let mut last_incomplete_set = None;
        let mut last_set: Option<SetEntry> = None;
        // scan all sets
        for set_number in 1..=5 {
            if let Some(set_entry) = self.sets.iter().find(|s| s.set_number == set_number) {
                // set found
                let snapshot = &self.get_set_snapshot(set_number)?;
                if let Some(snapshot) = snapshot {
                    match snapshot.get_set_winner(set_number) {
                        Some(TeamSideEnum::Us) => {
                            last_set = Some(set_entry.clone());
                            us_wins += 1;
                        }
                        Some(TeamSideEnum::Them) => {
                            last_set = Some(set_entry.clone());
                            them_wins += 1;
                        }
                        None => {
                            last_incomplete_set = Some(set_entry.clone());
                            break;
                        }
                    }
                    if us_wins == 3 || them_wins == 3 {
                        return Ok(MatchStatus {
                            us_wins,
                            them_wins,
                            next_set_number: None,
                            last_incomplete_set: None,
                            match_finished: true,
                            last_set,
                        });
                    }
                } else {
                    last_incomplete_set = Some(set_entry.clone());
                    break;
                }
            } else {
                next_set_number = set_number;
                break;
            }
            next_set_number = set_number + 1;
        }
        Ok(MatchStatus {
            us_wins,
            them_wins,
            next_set_number: Some(next_set_number),
            last_incomplete_set,
            match_finished: false,
            last_set,
        })
    }

    /// Who played in a set of the match: on court, the liberos, in an
    /// event.
    pub fn played(&self) -> HashSet<Uuid> {
        self.sets
            .iter()
            .flat_map(|s| {
                s.initial_positions
                    .iter()
                    .copied()
                    .chain([s.libero, s.setter])
                    .chain(s.fallback_libero)
                    .chain(
                        s.events
                            .iter()
                            .flat_map(|e| e.player.into_iter().chain(e.target_player)),
                    )
            })
            .collect()
    }

    pub fn get_set_snapshot(&self, set_number: u8) -> Result<Option<Snapshot>, AppError> {
        let s = match self.sets.iter().find(|s| s.set_number == set_number) {
            Some(set) => set,
            None => return Ok(None),
        };
        let snapshot = s.compute_snapshot(self.max_substitutions)?;
        Ok(Some(snapshot))
    }
}
