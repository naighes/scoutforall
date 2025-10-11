use crate::{
    errors::AppError,
    shapes::{
        enums::{EventTypeEnum, TeamSideEnum},
        set::SetEntry,
        snapshot::Snapshot,
        team::TeamEntry,
    },
};
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct MatchStatus {
    pub us_wins: u8,
    pub them_wins: u8,
    pub next_set_number: Option<u8>,
    pub last_incomplete_set: Option<SetEntry>,
    pub match_finished: bool,
    pub last_serving_team: Option<TeamSideEnum>,
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
    #[serde(skip_serializing, skip_deserializing)]
    pub sets: Vec<SetEntry>,
}

impl MatchEntry {
    pub fn get_status(&self) -> Result<MatchStatus, AppError> {
        let mut us_wins = 0;
        let mut them_wins = 0;
        let mut next_set_number = 1;
        let mut last_incomplete_set = None;
        let mut last_serving_team: Option<TeamSideEnum> = None;
        // scan all sets
        for set_number in 1..=5 {
            if let Some(set_entry) = self.sets.iter().find(|s| s.set_number == set_number) {
                // set found
                let snapshot = &self.get_set_snapshot(set_number)?;
                if let Some((snapshot, _)) = snapshot {
                    match snapshot.get_set_winner(set_number) {
                        Some(TeamSideEnum::Us) => {
                            last_serving_team = Some(set_entry.serving_team);
                            us_wins += 1;
                        }
                        Some(TeamSideEnum::Them) => {
                            last_serving_team = Some(set_entry.serving_team);
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
                            last_serving_team,
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
            last_serving_team,
        })
    }

    pub fn get_set_snapshot(
        &self,
        set_number: u8,
    ) -> Result<Option<(Snapshot, Vec<EventTypeEnum>)>, AppError> {
        let s = match self.sets.iter().find(|s| s.set_number == set_number) {
            Some(set) => set,
            None => return Ok(None),
        };
        let snapshot = s.compute_snapshot()?;
        Ok(Some(snapshot))
    }
}
