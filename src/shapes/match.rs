use crate::{
    errors::{AppError, MatchError},
    io::path::get_match_folder_path,
    shapes::{
        enums::{EventTypeEnum, TeamSideEnum},
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        team::TeamEntry,
    },
};
use chrono::{DateTime, FixedOffset};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::PathBuf,
};

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
}

impl MatchEntry {
    pub fn get_status(&self) -> Result<MatchStatus, AppError> {
        let sets = self.load_sets()?;
        let mut us_wins = 0;
        let mut them_wins = 0;
        let mut next_set_number = 1;
        let mut last_incomplete_set = None;
        let mut last_serving_team: Option<TeamSideEnum> = None;
        // scan all sets
        for set_number in 1..=5 {
            if let Some(set_entry) = sets.iter().find(|s| s.set_number == set_number) {
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

    pub fn load_sets(&self) -> Result<Vec<SetEntry>, MatchError> {
        let match_path: PathBuf = get_match_folder_path(&self.team.id, &self.id);
        let entries = fs::read_dir(&match_path).map_err(|e| {
            MatchError::LoadSetError(format!("could not read folder {:?}: {}", match_path, e))
        })?;
        let set_file_regex = Regex::new(r"^set_(\d+)\.json$").map_err(|e| {
            MatchError::LoadSetError(format!("could not compile regex {:?}: {}", match_path, e))
        })?;
        let mut sets: Vec<SetEntry> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            // just search for files
            if !path.is_file() {
                continue;
            }
            // grab the file name
            let filename = match path.file_name().and_then(|s| s.to_str()) {
                Some(name) => name,
                None => {
                    continue;
                }
            };
            // captures token via regex
            let caps = match set_file_regex.captures(filename) {
                Some(caps) => caps,
                None => continue,
            };
            // parse the set number
            let set_number: u8 = match caps.get(1).unwrap().as_str().parse() {
                Ok(n) => n,
                Err(_) => {
                    continue;
                }
            };
            let json_str = match fs::read_to_string(&path)
                .map_err(|e| format!("failed to read {}: {}", filename, e))
            {
                Ok(x) => x,
                Err(_) => {
                    continue;
                }
            };
            let mut set: SetEntry = match serde_json::from_str(&json_str) {
                Ok(x) => x,
                Err(_) => {
                    continue;
                }
            };
            set.set_number = set_number;
            // read events
            let csv_path = path.with_extension("csv");
            if csv_path.exists() {
                let file = match File::open(&csv_path) {
                    Ok(x) => x,
                    Err(_) => {
                        continue;
                    }
                };
                let mut reader = csv::ReaderBuilder::new()
                    .has_headers(false)
                    .from_reader(file);
                let events: Vec<EventEntry> = reader.deserialize().filter_map(|r| r.ok()).collect();
                set.events = events;
            }
            sets.push(set);
        }
        // no more than 5 sets
        if sets.len() > 5 {
            return Err(MatchError::LoadSetError(format!(
                "found more than 5 sets in match {}",
                &self.id
            )));
        }
        // order by set numbet
        sets.sort_by_key(|s| s.set_number);
        // check continuity
        for (i, set) in sets.iter().enumerate() {
            if set.set_number as usize != i + 1 {
                return Err(MatchError::LoadSetError(format!(
                    "expected set {} but found set {}",
                    i + 1,
                    set.set_number
                )));
            }
        }
        Ok(sets)
    }

    pub fn get_set_snapshot(
        &self,
        set_number: u8,
    ) -> Result<Option<(Snapshot, Vec<EventTypeEnum>)>, AppError> {
        let sets = &self.load_sets()?;
        let s = match sets.iter().find(|s| s.set_number == set_number) {
            Some(set) => set,
            None => return Ok(None),
        };
        let snapshot = s.compute_snapshot()?;
        Ok(Some(snapshot))
    }
}
