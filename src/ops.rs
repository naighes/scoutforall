use crate::constants::{MATCH_DESCRIPTOR_FILE_NAME, TEAM_DESCRIPTOR_FILE_NAME};
use crate::errors::{AppError, IOError, MatchError};
use crate::io::path::{
    get_base_path, get_match_descriptor_file_path, get_match_folder_path,
    get_set_descriptor_file_path, get_set_events_file_path, get_team_folder_path,
};
use crate::shapes::enums::{EventTypeEnum, PhaseEnum, RoleEnum, TeamSideEnum};
use crate::shapes::player::PlayerEntry;
use crate::shapes::r#match::{MatchEntry, MatchStatus};
use crate::shapes::set::SetEntry;
use crate::shapes::snapshot::{EventEntry, Snapshot};
use crate::shapes::team::TeamEntry;
use crate::util::sanitize_filename;
use chrono::DateTime;
use chrono::FixedOffset;
use csv::WriterBuilder;
use regex::Regex;
use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::{fs::File, path::PathBuf};
use uuid::Uuid;

pub fn compute_snapshot(set_entry: &SetEntry) -> Result<(Snapshot, Vec<EventTypeEnum>), AppError> {
    // prepare the initial snapshot
    let mut snapshot = Snapshot::new(&set_entry)?;
    // prepare initial available options
    let mut available_options: Vec<EventTypeEnum> = vec![];

    if set_entry.has_events() {
        for event in &set_entry.events {
            available_options = snapshot.compute_event(event, available_options.clone())?;
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
            _ => vec![EventTypeEnum::F, EventTypeEnum::R, EventTypeEnum::OE],
        }
    }

    Ok((snapshot, available_options))
}

pub fn get_matches(team: &TeamEntry) -> Result<Vec<MatchEntry>, Box<dyn std::error::Error>> {
    let team_path: PathBuf = get_team_folder_path(&team.id);
    let entries = fs::read_dir(&team_path)?;
    let mut result: Vec<MatchEntry> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| {
            let id = path.file_name()?.to_str()?;
            let json_path = path.join(MATCH_DESCRIPTOR_FILE_NAME);
            if !json_path.exists() {
                eprintln!("descriptor file '{}' does not exist", json_path.display());
            }
            let json_str = fs::read_to_string(&json_path).ok()?;
            let entry: Result<MatchEntry, serde_json::Error> = serde_json::from_str(&json_str);
            match entry {
                Ok(mut e) => {
                    e.id = id.into();
                    e.team = team.clone();
                    Some(e)
                }
                Err(e) => {
                    eprintln!("error on deserialization: {}", e);
                    None
                }
            }
        })
        .collect();
    result.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(result)
}

pub fn get_match(
    team: &TeamEntry,
    match_id: &str,
) -> Result<Option<MatchEntry>, Box<dyn std::error::Error>> {
    let matches = get_matches(team)?;
    let m = matches.iter().find(|entry| entry.id == match_id).cloned();
    Ok(m)
}

#[derive(Debug)]
pub enum CreateMatchError {
    MatchAlreadyExists(String),
    Other(Box<dyn std::error::Error>),
}

impl fmt::Display for CreateMatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CreateMatchError::MatchAlreadyExists(msg) => write!(f, "match already exists: {}", msg),
            CreateMatchError::Other(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for CreateMatchError {}

pub fn create_match(
    team: &TeamEntry,
    opponent: String,
    date: DateTime<FixedOffset>,
    home: bool,
) -> Result<MatchEntry, CreateMatchError> {
    let opponent_clean = sanitize_filename(&opponent);
    let date_str = date.format("%Y-%m-%d").to_string();
    let match_id = format!("{}_{}", date_str, opponent_clean);
    let match_path: PathBuf = get_match_folder_path(&team.id, &match_id);
    // check if match already exists
    if match_path.exists() {
        return Err(CreateMatchError::MatchAlreadyExists(match_id));
    }
    fs::create_dir_all(&match_path).map_err(|e| CreateMatchError::Other(Box::new(e)))?;
    let m = MatchEntry {
        opponent,
        date,
        id: match_id.clone(),
        team: team.clone(),
        home,
    };
    let file_path = get_match_descriptor_file_path(&team.id, &match_id);
    let file = File::create(&file_path).map_err(|e| CreateMatchError::Other(Box::new(e)))?;
    serde_json::to_writer_pretty(file, &m).map_err(|e| CreateMatchError::Other(Box::new(e)))?;
    Ok(m)
}

pub fn get_match_status(m: &MatchEntry) -> Result<MatchStatus, AppError> {
    let sets = get_sets(m)?;
    let mut us_wins = 0;
    let mut them_wins = 0;
    let mut next_set_number = 1;
    let mut last_incomplete_set = None;
    for set_number in 1..=5 {
        if let Some(set_entry) = sets.iter().find(|s| s.set_number == set_number) {
            let snapshot = get_set_snapshot(m, set_number)?;
            if let Some((snapshot, _)) = snapshot {
                match snapshot.get_set_winner(set_number) {
                    Some(TeamSideEnum::Us) => us_wins += 1,
                    Some(TeamSideEnum::Them) => them_wins += 1,
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
    })
}

pub fn get_sets(m: &MatchEntry) -> Result<Vec<SetEntry>, MatchError> {
    let match_path: PathBuf = get_match_folder_path(&m.team.id, &m.id);
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
            let events: Vec<EventEntry> = reader
                .deserialize()
                .filter_map(|r| match r {
                    Ok(ev) => Some(ev),
                    Err(e) => None,
                })
                .collect();
            set.events = events;
        }
        sets.push(set);
    }
    // no more than 5 sets
    if sets.len() > 5 {
        return Err(MatchError::LoadSetError(
            format!("found more than 5 sets in match {}", m.id).into(),
        ));
    }
    // order by set numbet
    sets.sort_by_key(|s| s.set_number);
    // check continuity
    for (i, set) in sets.iter().enumerate() {
        if set.set_number as usize != i + 1 {
            return Err(MatchError::LoadSetError(
                format!("expected set {} but found set {}", i + 1, set.set_number).into(),
            ));
        }
    }
    Ok(sets)
}

pub fn create_set(
    m: &MatchEntry,
    set_number: u8,
    serving_team: TeamSideEnum,
    positions: [Uuid; 6],
    libero: Uuid,
    setter: Uuid,
) -> Result<SetEntry, Box<dyn Error>> {
    let match_path: PathBuf = get_match_folder_path(&m.team.id, &m.id);
    if !match_path.exists() {
        return Err(format!("match not found: {}", m.id).into());
    }
    let file_path = get_set_descriptor_file_path(&m.team.id, &m.id, set_number);
    let s = SetEntry {
        set_number,
        serving_team,
        initial_positions: positions,
        libero,
        setter,
        events: Vec::new(),
    };
    let json_file = File::create(&file_path)?;
    serde_json::to_writer_pretty(json_file, &s)?;
    File::create(get_set_events_file_path(&m.team.id, &m.id, set_number))?;
    Ok(s)
}

pub fn get_set_snapshot(
    m: &MatchEntry,
    set_number: u8,
) -> Result<Option<(Snapshot, Vec<EventTypeEnum>)>, AppError> {
    let sets = get_sets(m)?;
    let s = match sets.iter().find(|s| s.set_number == set_number) {
        Some(set) => set,
        None => return Ok(None),
    };
    let snapshot = compute_snapshot(s)?;
    Ok(Some(snapshot))
}

/* TEAMS */

pub fn get_teams() -> Vec<TeamEntry> {
    let path = get_base_path();
    let entries = match fs::read_dir(&path) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("could not read folder {:?}: {}", path, e);
            return Vec::new();
        }
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| {
            let descriptor_path = path.join(TEAM_DESCRIPTOR_FILE_NAME);
            let json_str = fs::read_to_string(&descriptor_path).ok()?;
            let entry_result: Result<TeamEntry, _> = serde_json::from_str(&json_str);
            match entry_result {
                Ok(mut entry) => {
                    let name = path.file_name()?.to_str()?;
                    let uuid = Uuid::parse_str(name).ok()?;
                    entry.id = uuid;
                    Some(entry)
                }
                Err(e) => {
                    eprintln!(
                        "failed to deserialize {:?}: {}\ncontent:\n{}",
                        descriptor_path, e, json_str
                    );
                    None
                }
            }
        })
        .collect()
}

pub fn create_team(
    name: String,
    league: String,
    year: u16,
) -> Result<TeamEntry, Box<dyn std::error::Error>> {
    let team_id = Uuid::new_v4();
    let team_path: PathBuf = get_team_folder_path(&team_id);
    let team_descriptor_file_path = team_path.join(TEAM_DESCRIPTOR_FILE_NAME);
    let file = File::create(&team_descriptor_file_path)?;
    let t = TeamEntry {
        name,
        league,
        id: team_id,
        year,
        players: Vec::new(),
    };
    serde_json::to_writer_pretty(file, &t)?;
    Ok(t)
}

pub fn create_player(
    name: String,
    role: RoleEnum,
    number: u8,
    team: &mut TeamEntry,
) -> Result<PlayerEntry, Box<dyn std::error::Error>> {
    let player_id = Uuid::new_v4();
    let player = PlayerEntry {
        id: player_id,
        name,
        role,
        number,
    };
    let result = player.clone();
    team.players.push(player);
    let team_path: PathBuf = get_team_folder_path(&team.id);
    let team_descriptor_file_path = team_path.join(TEAM_DESCRIPTOR_FILE_NAME);
    let file = File::create(&team_descriptor_file_path)?;
    serde_json::to_writer_pretty(file, &team)?;
    Ok(result)
}

pub fn append_event(
    team: &TeamEntry,
    match_id: &str,
    set_number: u8,
    event: &EventEntry,
) -> Result<(), AppError> {
    let path = get_set_events_file_path(&team.id, match_id, set_number);
    // open file in append-only
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| IOError::Error(e))?;
    let mut writer = WriterBuilder::new().has_headers(false).from_writer(file);
    writer
        .serialize(event)
        .map_err(|_| IOError::SerializationError)?;
    writer.flush().map_err(|e| IOError::Error(e))?;
    Ok(())
}
