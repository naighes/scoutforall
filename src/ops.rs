use crate::constants::{MATCH_DESCRIPTOR_FILE_NAME, TEAM_DESCRIPTOR_FILE_NAME};
use crate::errors::{AppError, IOError};
use crate::io::path::{
    get_base_path, get_match_descriptor_file_path, get_match_folder_path,
    get_set_descriptor_file_path, get_set_events_file_path, get_team_folder_path,
};
use crate::shapes::enums::{RoleEnum, TeamSideEnum};
use crate::shapes::player::PlayerEntry;
use crate::shapes::r#match::MatchEntry;
use crate::shapes::set::SetEntry;
use crate::shapes::snapshot::EventEntry;
use crate::shapes::team::TeamEntry;
use crate::util::sanitize_filename;
use chrono::DateTime;
use chrono::FixedOffset;
use csv::{ReaderBuilder, WriterBuilder};
use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::{fs::File, path::PathBuf};
use uuid::Uuid;

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

// TODO: load single match from the file system (do not depend on `get_matches`)
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
    let s = SetEntry::new(set_number, serving_team, positions, libero, setter)?;
    let json_file = File::create(&file_path)?;
    serde_json::to_writer_pretty(json_file, &s)?;
    File::create(get_set_events_file_path(&m.team.id, &m.id, set_number))?;
    Ok(s)
}

pub fn load_teams() -> Result<Vec<TeamEntry>, AppError> {
    let path = get_base_path();
    let entries = fs::read_dir(&path).map_err(|_| {
        IOError::Error(format!(
            "could not load teams: directory error at path {:?}",
            path
        ))
    })?;
    let mut teams = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|_| IOError::Error("could not load teams: directory error".to_string()))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let descriptor_path = path.join(TEAM_DESCRIPTOR_FILE_NAME);
        let json_str =
            fs::read_to_string(&descriptor_path).map_err(|_| IOError::SerializationError)?;
        let mut team: TeamEntry =
            serde_json::from_str(&json_str).map_err(|_| IOError::SerializationError)?;
        let name = path
            .file_name()
            .and_then(|os| os.to_str())
            .ok_or_else(|| IOError::Error("could not load teamd: invalid file name".to_string()))?;
        let uuid = Uuid::parse_str(name)
            .map_err(|_| IOError::Error("could not load teamd: invalid uuid".to_string()))?;
        team.id = uuid;
        teams.push(team);
    }
    Ok(teams)
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
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|_| IOError::Error("could not open file".to_string()))?;
    let mut writer = WriterBuilder::new().has_headers(false).from_writer(file);
    writer
        .serialize(event)
        .map_err(|_| IOError::SerializationError)?;
    writer
        .flush()
        .map_err(|_| IOError::Error("could not flush content".to_string()))?;
    Ok(())
}

pub fn remove_last_event(
    team: &TeamEntry,
    match_id: &str,
    set_number: u8,
) -> Result<Option<EventEntry>, AppError> {
    let path = get_set_events_file_path(&team.id, match_id, set_number);
    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .from_path(path)
        .map_err(|_| IOError::Error("could not open file".to_string()))?;
    let mut records: Vec<EventEntry> = reader
        .deserialize()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| IOError::Error("could not parse csv".to_string()))?;
    let removed = records.pop();
    if removed.is_none() {
        return Ok(None);
    }
    let path = get_set_events_file_path(&team.id, match_id, set_number);
    let file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|_| IOError::Error("could not open file for writing".to_string()))?;
    let mut writer = WriterBuilder::new().has_headers(false).from_writer(file);
    for rec in records {
        writer
            .serialize(rec)
            .map_err(|_| IOError::Error("could not serialize csv".to_string()))?;
    }
    writer
        .flush()
        .map_err(|_| IOError::Error("could not flush csv".to_string()))?;
    Ok(removed)
}
