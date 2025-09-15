use crate::constants::{DEFAULT_LANGUAGE, MATCH_DESCRIPTOR_FILE_NAME, TEAM_DESCRIPTOR_FILE_NAME};
use crate::errors::{AppError, IOError};
use crate::io::path::{
    get_base_path, get_config_file_path, get_match_descriptor_file_path, get_match_folder_path,
    get_set_descriptor_file_path, get_set_events_file_path, get_team_folder_path,
};
use crate::shapes::enums::{RoleEnum, TeamSideEnum};
use crate::shapes::player::PlayerEntry;
use crate::shapes::r#match::MatchEntry;
use crate::shapes::set::SetEntry;
use crate::shapes::settings::Settings;
use crate::shapes::snapshot::EventEntry;
use crate::shapes::team::TeamEntry;
use crate::util::sanitize_filename;
use chrono::DateTime;
use chrono::FixedOffset;
use csv::{ReaderBuilder, WriterBuilder};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::{fs::File, path::PathBuf};
use uuid::Uuid;

trait ResultOptionExt<T, E> {
    fn and_then_option<U>(self, f: impl FnOnce(T) -> Result<Option<U>, E>) -> Result<Option<U>, E>;
}

impl<T, E> ResultOptionExt<T, E> for Result<Option<T>, E> {
    fn and_then_option<U>(self, f: impl FnOnce(T) -> Result<Option<U>, E>) -> Result<Option<U>, E> {
        match self? {
            Some(v) => f(v),
            None => Ok(None),
        }
    }
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
) -> Result<SetEntry, AppError> {
    let match_path: PathBuf = get_match_folder_path(&m.team.id, &m.id);
    if !match_path.exists() {
        return Err(AppError::IO(IOError::Msg(format!(
            "could not create set: match folder does not exist at path {}",
            match_path.display()
        ))));
    }
    let file_path = get_set_descriptor_file_path(&m.team.id, &m.id, set_number);
    let s = SetEntry::new(set_number, serving_team, positions, libero, setter)?;
    File::create(&file_path)
        .and_then(|json_file| serde_json::to_writer_pretty(json_file, &s).map_err(|e| e.into()))
        .and_then(|_| File::create(get_set_events_file_path(&m.team.id, &m.id, set_number)))
        .map(|_| s)
        .map_err(|e| AppError::IO(IOError::from(e)))
}

pub fn load_teams() -> Result<Vec<TeamEntry>, AppError> {
    let path = get_base_path();
    let entries = fs::read_dir(&path).map_err(|_| {
        IOError::Msg(format!(
            "could not load teams: directory error at path {:?}",
            path
        ))
    })?;
    let mut teams = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|_| IOError::Msg("could not load teams: directory error".to_string()))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let descriptor_path = path.join(TEAM_DESCRIPTOR_FILE_NAME);
        let team = fs::read_to_string(&descriptor_path)
            .map_err(IOError::from)
            .and_then(|s| serde_json::from_str::<TeamEntry>(&s).map_err(IOError::from))
            .and_then(|team| {
                path.file_name()
                    .and_then(|os| os.to_str())
                    .and_then(|name| Uuid::parse_str(name).ok())
                    .map(|uuid| TeamEntry { id: uuid, ..team })
                    .ok_or_else(|| IOError::Msg("invalid file name for uuid".to_string()))
            })?;
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
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|_| AppError::IO(IOError::Msg("could not open file".to_string())))
        .map(|file| WriterBuilder::new().has_headers(false).from_writer(file))
        .and_then(|mut writer| {
            writer
                .serialize(event)
                .map_err(|e| AppError::IO(IOError::Msg(format!("could not serialize event: {e}"))))
                .and_then(|_| {
                    writer.flush().map_err(|_| {
                        AppError::IO(IOError::Msg("could not flush content".to_string()))
                    })
                })
        })
}

pub fn remove_last_event(
    team: &TeamEntry,
    match_id: &str,
    set_number: u8,
) -> Result<Option<EventEntry>, AppError> {
    let path = get_set_events_file_path(&team.id, match_id, set_number);
    ReaderBuilder::new()
        .has_headers(false)
        .from_path(&path)
        .and_then(|mut reader| reader.deserialize().collect::<Result<Vec<EventEntry>, _>>())
        .map(|records| {
            records
                .split_first()
                .map(|(first, rest)| (first.clone(), rest.to_vec()))
        })
        .and_then_option(|(first, rest)| {
            let path = get_set_events_file_path(&team.id, match_id, set_number);
            let mut writer = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(path)
                .map(|file| WriterBuilder::new().has_headers(false).from_writer(file))?;
            for rec in rest {
                writer.serialize(rec)?;
            }
            writer.flush()?;
            Ok(Some(first))
        })
        .map_err(|e| AppError::IO(IOError::from(e)))
}

pub fn save_settings(language: String) -> Result<Settings, Box<dyn std::error::Error>> {
    let config_file_path: PathBuf = get_config_file_path();
    let config = Settings { language };
    let file = File::create(&config_file_path)?;
    serde_json::to_writer_pretty(file, &config)?;
    Ok(config)
}

pub fn load_settings() -> Result<Settings, Box<dyn std::error::Error>> {
    let config_file_path: PathBuf = get_config_file_path();
    match fs::read_to_string(&config_file_path) {
        Ok(content) => {
            let settings: Settings = serde_json::from_str(&content)?;
            Ok(settings)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            save_settings(DEFAULT_LANGUAGE.to_string())
        }
        Err(e) => Err(Box::new(e)),
    }
}
