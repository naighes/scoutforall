use crate::shapes::{set::SetEntry, team::TeamEntry};
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

pub struct MatchStatus {
    pub us_wins: u8,
    pub them_wins: u8,
    pub next_set_number: Option<u8>,
    pub last_incomplete_set: Option<SetEntry>,
    pub match_finished: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MatchEntry {
    pub opponent: String,
    pub date: DateTime<FixedOffset>,
    #[serde(skip_serializing, skip_deserializing)]
    pub id: String,
    #[serde(skip_serializing, skip_deserializing)]
    pub team: TeamEntry,
    pub home: bool,
}
