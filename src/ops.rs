use chrono::TimeZone;
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use csv::{ReaderBuilder, WriterBuilder};
use dirs::home_dir;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::str::FromStr;
use std::{
    collections::HashMap,
    fs::{File, ReadDir},
    path::PathBuf,
};
use uuid::Uuid;

const MATCH_DESCRIPTOR_FILE_NAME: &str = "match.json";
const TEAM_DESCRIPTOR_FILE_NAME: &str = "team.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Copy)]
pub enum TeamSideEnum {
    Us,
    Them,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "UPPERCASE")]
pub enum EventTypeEnum {
    S,  // Serve
    P,  // Pass
    A,  // Attack
    D,  // Dig
    B,  // Block
    F,  // Fault
    OS, // Opponent Score
    OE, // Opponent Error
}

impl fmt::Display for EventTypeEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            EventTypeEnum::S => "S",
            EventTypeEnum::P => "P",
            EventTypeEnum::A => "A",
            EventTypeEnum::D => "D",
            EventTypeEnum::B => "B",
            EventTypeEnum::F => "F",
            EventTypeEnum::OS => "OS",
            EventTypeEnum::OE => "OE",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for EventTypeEnum {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "S" => Ok(EventTypeEnum::S),
            "P" => Ok(EventTypeEnum::P),
            "A" => Ok(EventTypeEnum::A),
            "D" => Ok(EventTypeEnum::D),
            "B" => Ok(EventTypeEnum::B),
            "F" => Ok(EventTypeEnum::F),
            "OS" => Ok(EventTypeEnum::OS),
            "OE" => Ok(EventTypeEnum::OE),
            _ => Err(format!("Invalid event type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EvalEnum {
    #[serde(rename = "#")]
    Perfect,
    #[serde(rename = "+")]
    Positive,
    #[serde(rename = "!")]
    Exclamative,
    #[serde(rename = "/")]
    Over,
    #[serde(rename = "=")]
    Error,
    #[serde(rename = "-")]
    Negative,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MatchEntry {
    pub opponent: String,
    pub date: DateTime<FixedOffset>,
    #[serde(skip_serializing, skip_deserializing)]
    pub id: Uuid,
    #[serde(skip_serializing)]
    pub team: TeamEntry,
    pub home: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhaseEnum {
    Break,
    SideOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEntry {
    pub timestamp: DateTime<Utc>,
    pub rotation: u8,
    pub event_type: EventTypeEnum,
    pub player: Option<Uuid>,
    pub eval: Option<EvalEnum>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SetEntry {
    #[serde(skip_serializing)]
    pub set_number: u8,
    pub serving_team: TeamSideEnum,
    pub positions: [Uuid; 6],
    pub libero: Uuid,
    pub setter: Uuid,
    #[serde(skip_serializing)]
    pub events: Vec<EventEntry>,
}

pub type Stats = HashMap<EventTypeEnum, HashMap<PhaseEnum, HashMap<Uuid, HashMap<EvalEnum, u32>>>>;

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub phase: PhaseEnum,
    pub previous_phase: Option<PhaseEnum>,
    pub rotation: u8,
    pub score_us: u8,
    pub score_them: u8,
    pub last_event: Option<EventEntry>,
    pub stats: Stats,
    pub new_serve: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerEntry {
    pub id: Uuid,
    pub name: String,
    pub role: RoleEnum,
    pub number: u8,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TeamEntry {
    pub name: String,
    pub league: String,
    pub year: u16,
    pub players: Vec<PlayerEntry>,
    #[serde(skip_serializing, skip_deserializing)]
    pub id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum RoleEnum {
    Libero,
    OppositeHitter,
    Setter,
    OutsideHitter,
    MiddleBlocker,
}

impl fmt::Display for RoleEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            RoleEnum::Libero => "Libero",
            RoleEnum::OppositeHitter => "Opposite Hitter",
            RoleEnum::Setter => "Setter",
            RoleEnum::OutsideHitter => "Outside Hitter",
            RoleEnum::MiddleBlocker => "Middle Blocker",
        };
        write!(f, "{}", label)
    }
}

pub fn get_base_path() -> PathBuf {
    let mut path = home_dir().expect("could not determine home directory");
    path.push(".scoutforall");
    if !path.exists() {
        fs::create_dir_all(&path).expect("could not create base directory");
    }
    path
}

pub fn get_team_folder_path(team_id: &Uuid) -> PathBuf {
    let mut base = get_base_path();
    base.push(team_id.to_string());
    if let Err(e) = fs::create_dir_all(&base) {
        eprintln!("could not create team directory {:?}: {}", base, e);
    }
    base
}

pub fn get_match_folder_path(team: &TeamEntry, match_id: Uuid) -> PathBuf {
    let mut path: PathBuf = get_team_folder_path(&team.id);
    path.push(match_id.to_string());
    path
}

pub fn get_match_descriptor_file_path(team: &TeamEntry, match_id: Uuid) -> PathBuf {
    let path: PathBuf = get_match_folder_path(team, match_id);
    path.join(MATCH_DESCRIPTOR_FILE_NAME)
}

pub fn get_set_descriptor_file_path(team: &TeamEntry, match_id: Uuid, set_number: u8) -> PathBuf {
    let path = get_match_folder_path(&team, match_id);
    path.join(format!("set_{}.json", set_number))
}

pub fn get_set_events_file_path(team: &TeamEntry, match_id: Uuid, set_number: u8) -> PathBuf {
    let path = get_match_folder_path(&team, match_id);
    path.join(format!("set_{}.csv", set_number))
}

fn accumulate_stat(
    stats: &mut Stats,
    phase: PhaseEnum,
    event_type: EventTypeEnum,
    player_id: Option<Uuid>,
    eval: Option<EvalEnum>,
) {
    if let (Some(player_id), Some(eval)) = (player_id, eval) {
        let entry = stats
            .entry(event_type)
            .or_default()
            .entry(phase)
            .or_default()
            .entry(player_id)
            .or_default()
            .entry(eval)
            .or_insert(0);
        *entry += 1;
    }
}

pub fn compute_event(snapshot: &mut Snapshot, event: &EventEntry) -> Vec<EventTypeEnum> {
    let mut next_phase: Option<PhaseEnum> = None;
    let mut available_options: Vec<EventTypeEnum> = vec![];
    snapshot.new_serve = false;
    match event.event_type {
        EventTypeEnum::OS => {
            // opponent scored
            snapshot.score_them += 1;
            if snapshot.phase == PhaseEnum::Break {
                // transition to SideOut
                next_phase = Some(PhaseEnum::SideOut);
            }
            available_options = vec![
                EventTypeEnum::OS,
                EventTypeEnum::OE,
                EventTypeEnum::F,
                EventTypeEnum::P,
            ];
        }
        EventTypeEnum::OE => {
            // opponent error
            snapshot.score_us += 1;
            if snapshot.phase == PhaseEnum::SideOut {
                // transition to break
                next_phase = Some(PhaseEnum::Break);
            }
            snapshot.new_serve = true;
            // after opponent error, I have to serve, so I can just have a fault
            available_options = vec![EventTypeEnum::F];
        }
        EventTypeEnum::B => {
            // block
            accumulate_stat(
                &mut snapshot.stats,
                snapshot.phase,
                event.event_type.clone(),
                event.player,
                event.eval.clone(),
            );
            if event.eval == Some(EvalEnum::Perfect) {
                snapshot.score_us += 1;
                if snapshot.phase == PhaseEnum::SideOut {
                    // transition to break
                    next_phase = Some(PhaseEnum::Break);
                }
                snapshot.new_serve = true;
            } else if event.eval == Some(EvalEnum::Over) {
                // fault
                snapshot.score_them += 1;
                if snapshot.phase == PhaseEnum::Break {
                    // phase transition
                    next_phase = Some(PhaseEnum::SideOut);
                }
            }
        }
        EventTypeEnum::A => {
            // attack
            accumulate_stat(
                &mut snapshot.stats,
                snapshot.phase,
                event.event_type.clone(),
                event.player,
                event.eval.clone(),
            );
            if event.eval == Some(EvalEnum::Perfect) {
                snapshot.score_us += 1;
                if snapshot.phase == PhaseEnum::SideOut {
                    // transition to break
                    next_phase = Some(PhaseEnum::Break);
                }
                snapshot.new_serve = true;
            } else if event.eval == Some(EvalEnum::Error) {
                snapshot.score_them += 1;
                if snapshot.phase == PhaseEnum::Break {
                    // phase transition
                    next_phase = Some(PhaseEnum::SideOut);
                }
            } else if event.eval == Some(EvalEnum::Over) {
                // attack blocked
                snapshot.score_them += 1;
                if snapshot.phase == PhaseEnum::Break {
                    // phase transition
                    next_phase = Some(PhaseEnum::SideOut);
                }
            }
            available_options = match event.eval {
                // attack error
                Some(EvalEnum::Error) => vec![
                    EventTypeEnum::OS,
                    EventTypeEnum::OE,
                    EventTypeEnum::F,
                    EventTypeEnum::P,
                ],
                // on attack score we serve, so we expect the serve evaluation or a fault (e.g. rotation)
                Some(EvalEnum::Perfect) => vec![EventTypeEnum::F],
                // blocked attack => expected an attack, dig or fault
                Some(EvalEnum::Over) => vec![
                    EventTypeEnum::OS,
                    EventTypeEnum::OE,
                    EventTypeEnum::F,
                    EventTypeEnum::P,
                ],
                Some(EvalEnum::Positive) => {
                    vec![EventTypeEnum::OE, EventTypeEnum::F, EventTypeEnum::A]
                }
                // standard attack: expected block or defense
                _ => vec![EventTypeEnum::A],
            }
        }
        EventTypeEnum::D => {
            // dig
            accumulate_stat(
                &mut snapshot.stats,
                snapshot.phase,
                event.event_type.clone(),
                event.player,
                event.eval.clone(),
            );
            if event.eval == Some(EvalEnum::Error) {
                // dig error
                snapshot.score_them += 1;
                if snapshot.phase == PhaseEnum::Break {
                    // phase transition
                    next_phase = Some(PhaseEnum::SideOut);
                }
            }
            available_options = match event.eval {
                // dig error: opponent serves
                Some(EvalEnum::Error) => vec![
                    EventTypeEnum::OS,
                    EventTypeEnum::OE,
                    EventTypeEnum::F,
                    EventTypeEnum::P,
                ],
                // slash dig
                Some(EvalEnum::Over) => vec![
                    EventTypeEnum::OS,
                    EventTypeEnum::OE,
                    EventTypeEnum::B,
                    EventTypeEnum::D,
                ],
                // standard dig: attack or fault
                _ => vec![EventTypeEnum::A, EventTypeEnum::F],
            }
        }
        EventTypeEnum::F => {
            // fault
            snapshot.score_them += 1;
            if snapshot.phase == PhaseEnum::Break {
                // TODO: accumulate faults
                // phase transition
                next_phase = Some(PhaseEnum::SideOut);
            }
            // on fault, we expect opponent score/error, another fault or pass
            available_options = vec![
                EventTypeEnum::OS,
                EventTypeEnum::OE,
                EventTypeEnum::F,
                EventTypeEnum::P,
            ];
        }
        EventTypeEnum::P => {
            // pass: no need to change phase here
            accumulate_stat(
                &mut snapshot.stats,
                snapshot.phase,
                event.event_type.clone(),
                event.player,
                event.eval.clone(),
            );
            available_options = match event.eval {
                // pass error: opponent serves again
                Some(EvalEnum::Error) => vec![
                    EventTypeEnum::OS,
                    EventTypeEnum::OE,
                    EventTypeEnum::F,
                    EventTypeEnum::P,
                ],
                // slash pass
                Some(EvalEnum::Over) => vec![
                    EventTypeEnum::OS,
                    EventTypeEnum::OE,
                    EventTypeEnum::B,
                    EventTypeEnum::D,
                ],
                // standard pass: attack or fault
                _ => vec![EventTypeEnum::A, EventTypeEnum::F],
            }
        }
        EventTypeEnum::S => {
            // serve
            accumulate_stat(
                &mut snapshot.stats,
                snapshot.phase,
                event.event_type.clone(),
                event.player,
                event.eval.clone(),
            );
            if event.eval == Some(EvalEnum::Perfect) {
                // ace: continue to serve
                snapshot.score_us += 1;
                snapshot.new_serve = true;
            } else if event.eval == Some(EvalEnum::Error) {
                snapshot.score_them += 1;
                // phase transition
                next_phase = Some(PhaseEnum::SideOut);
            }
            available_options = match event.eval {
                // serve error
                Some(EvalEnum::Error) => vec![
                    EventTypeEnum::OS,
                    EventTypeEnum::OE,
                    EventTypeEnum::F,
                    EventTypeEnum::P,
                ],
                // on ace, we just expect serve evaluation or fault (e.g. rotation)
                Some(EvalEnum::Perfect) => vec![EventTypeEnum::F],
                // slash serve => expected an attack or fault
                Some(EvalEnum::Over) => vec![EventTypeEnum::A, EventTypeEnum::F],
                // standard serve: expected block, defense or opponent error
                _ => vec![
                    EventTypeEnum::B,
                    EventTypeEnum::D,
                    EventTypeEnum::OE,
                    EventTypeEnum::OS,
                ],
            }
        }
    }
    if let Some(p) = next_phase {
        if snapshot.phase == PhaseEnum::SideOut && p == PhaseEnum::Break {
            snapshot.rotation = (snapshot.rotation + 5) % 6;
        }
        snapshot.previous_phase = Some(snapshot.phase.clone());
        snapshot.phase = p;
    }
    available_options
}

pub fn compute_snapshot(set_entry: &SetEntry) -> (Snapshot, Vec<EventTypeEnum>) {
    let initial_rotation: u8 = match set_entry
        .positions
        .iter()
        .position(|id| *id == set_entry.setter)
    {
        Some(pos) => {
            print!("computing rotation: setter is in {}", pos);
            pos as u8
        }
        None => {
            eprintln!("setter not found in initial lineup");
            0
        }
    };
    let mut snapshot: Snapshot = Snapshot {
        phase: match set_entry.serving_team {
            TeamSideEnum::Us => PhaseEnum::Break,
            TeamSideEnum::Them => PhaseEnum::SideOut,
            _ => PhaseEnum::Break,
        },
        rotation: initial_rotation,
        score_us: 0,
        score_them: 0,
        last_event: None,
        stats: HashMap::new(),
        new_serve: match set_entry.serving_team {
            TeamSideEnum::Us => true,
            _ => false,
        },
        previous_phase: None,
    };
    let mut available_options: Vec<EventTypeEnum> = vec![];

    for event in &set_entry.events {
        available_options = compute_event(&mut snapshot, event);
    }

    (snapshot, available_options)
}

/*
  returns all matches
  grabs directories in $HOME/.scoutforall/<team-name_team-league-team-year>
  then takes all sub folders, searching for match descriptor ($HOME/.scoutforall/<team-name_team-league-team-year>/<match-id>/match.json).
*/
pub fn get_matches(team: &TeamEntry) -> Vec<MatchEntry> {
    let team_path: PathBuf = get_team_folder_path(&team.id);
    let entries = match fs::read_dir(&team_path) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("could not read folder {:?}: {}", team_path, e);
            return Vec::new();
        }
    };
    let result: Vec<MatchEntry> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?;
            let uuid = Uuid::parse_str(name).ok()?;
            let json_path = path.join(MATCH_DESCRIPTOR_FILE_NAME);
            let json_str = fs::read_to_string(&json_path).ok()?;
            let mut entry: MatchEntry = serde_json::from_str(&json_str).ok()?;
            entry.id = uuid;
            entry.team = team.clone();
            Some(entry)
        })
        .collect();
    return result;
}

pub fn get_match(team: &TeamEntry, match_id: Uuid) -> Option<MatchEntry> {
    let matches = get_matches(&team);
    let m = matches.iter().find(|entry| entry.id == match_id);
    return m.cloned();
}

pub fn create_match(
    team: &TeamEntry,
    opponent: String,
    date: NaiveDate,
    home: bool,
) -> Result<MatchEntry, Box<dyn std::error::Error>> {
    let match_id = Uuid::new_v4();
    let match_path: PathBuf = get_match_folder_path(team, match_id);
    fs::create_dir_all(&match_path)?;
    let naive_datetime = date
        .and_hms_opt(0, 0, 0)
        .ok_or("invalid time when creating datetime")?;
    let fixed_offset = FixedOffset::east_opt(0).ok_or("invalid fixed offset")?;
    let datetime = fixed_offset
        .from_local_datetime(&naive_datetime)
        .single()
        .ok_or("failed to convert NaiveDate to DateTime<FixedOffset>")?;
    let m = MatchEntry {
        opponent,
        date: datetime,
        id: match_id,
        team: team.clone(),
        home,
    };
    let file_path = get_match_descriptor_file_path(team, match_id);
    let file = File::create(&file_path)?;
    serde_json::to_writer_pretty(file, &m)?;
    Ok(m)
}

pub fn get_set_winner(snapshot: &Snapshot, set_number: u8) -> Option<TeamSideEnum> {
    let target_score = if set_number == 5 { 15 } else { 25 };

    let us = snapshot.score_us;
    let them = snapshot.score_them;

    if us >= target_score && us >= them + 2 {
        Some(TeamSideEnum::Us)
    } else if them >= target_score && them >= us + 2 {
        Some(TeamSideEnum::Them)
    } else {
        None
    }
}

pub fn get_sets(m: &MatchEntry) -> Vec<SetEntry> {
    let match_path: PathBuf = get_match_folder_path(&m.team, m.id);
    let entries: ReadDir = match fs::read_dir(&match_path) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("could not read folder {:?}: {}", match_path, e);
            return Vec::new();
        }
    };
    let result: Vec<SetEntry> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter_map(|path| {
            let filename = path.file_name()?.to_str()?;
            if filename.starts_with("set_") && filename.ends_with(".json") {
                let re = Regex::new(r"^set_(\d+)\.json$").ok()?;
                let caps = re.captures(filename)?;
                let set_number: u8 = caps.get(1)?.as_str().parse().ok()?;
                let json_str = fs::read_to_string(&path).ok()?;
                let mut entry: SetEntry = serde_json::from_str(&json_str).ok()?;
                entry.set_number = set_number;
                let csv_path = path.with_extension("csv");
                let file = File::open(&csv_path).ok()?;
                let mut reader = ReaderBuilder::new().has_headers(true).from_reader(file);
                entry.events = reader.deserialize().filter_map(Result::ok).collect();
                Some(entry)
            } else {
                None
            }
        })
        .collect();
    result
}

pub fn create_set(
    m: &MatchEntry,
    set_number: u8,
    serving_team: TeamSideEnum,
    positions: [Uuid; 6],
    libero: Uuid,
    setter: Uuid,
) -> Result<SetEntry, Box<dyn Error>> {
    let match_path: PathBuf = get_match_folder_path(&m.team, m.id);
    if !match_path.exists() {
        return Err(format!("match not found: {}", m.id.to_string()).into());
    }
    let file_path = get_set_descriptor_file_path(&m.team, m.id, set_number);
    let s = SetEntry {
        set_number,
        serving_team,
        positions,
        libero,
        setter,
        events: Vec::new(),
    };
    let file = File::create(&file_path)?;
    serde_json::to_writer_pretty(file, &s)?;
    Ok(s)
}

pub fn get_set_snapshot(m: &MatchEntry, set_number: u8) -> Option<(Snapshot, Vec<EventTypeEnum>)> {
    let sets = get_sets(m);
    let s = sets.iter().find(|s| s.set_number == set_number);
    let result = match s {
        Some(q) => Some(compute_snapshot(&q)),
        _ => None,
    };
    result
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

pub fn get_team(team_id: Uuid) -> Option<TeamEntry> {
    let teams: Vec<TeamEntry> = get_teams();
    teams.into_iter().find(|t| t.id == team_id)
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
    match_id: Uuid,
    set_number: u8,
    event: &EventEntry,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = get_set_events_file_path(team, match_id, set_number);
    // open file in append-only
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let mut writer = WriterBuilder::new().has_headers(false).from_writer(file);
    writer.serialize(event)?;
    writer.flush()?;
    Ok(())
}

pub fn get_serving_player(s: &SetEntry, rotation: u8) -> Uuid {
    let initial_rotation = s
        .positions
        .iter()
        .position(|id| *id == s.setter)
        .expect("setter must be in positions array");
    let shift = (6 + rotation as isize - initial_rotation as isize) % 6;
    let serving_index = (6 - shift) % 6;
    s.positions[serving_index as usize]
}

// pub fn get_oh1_position(s: &SetEntry, rotation: u8) -> Uuid {
//     get_role_position(s, rotation, 1)
// }

// pub fn get_mb2_position(s: &SetEntry, rotation: u8) -> Uuid {
//     get_role_position(s, rotation, 2)
// }

// pub fn get_op_position(s: &SetEntry, rotation: u8) -> Uuid {
//     get_role_position(s, rotation, 3)
// }

// pub fn get_oh2_position(s: &SetEntry, rotation: u8) -> Uuid {
//     get_role_position(s, rotation, 4)
// }

// pub fn get_mb1_position(s: &SetEntry, rotation: u8) -> Uuid {
//     get_role_position(s, rotation, 5)
// }

// pub fn get_s_position(s: &SetEntry, rotation: u8) -> Uuid {
//     get_role_position(s, rotation, 0)
// }

pub fn get_oh2(s: &SetEntry) -> Uuid {
    get_role_from_offset(s, 4)
}

pub fn get_oh1(s: &SetEntry) -> Uuid {
    get_role_from_offset(s, 1)
}

pub fn get_mb1(s: &SetEntry) -> Uuid {
    get_role_from_offset(s, 5)
}

pub fn get_mb2(s: &SetEntry) -> Uuid {
    get_role_from_offset(s, 2)
}

pub fn get_opposite(s: &SetEntry) -> Uuid {
    get_role_from_offset(s, 3)
}

pub fn get_setter(s: &SetEntry) -> Uuid {
    get_role_from_offset(s, 6)
}

pub fn get_role_from_offset(s: &SetEntry, offset_from_setter: usize) -> Uuid {
    let setter_index = s
        .positions
        .iter()
        .position(|id| *id == s.setter)
        .expect("setter not found in positions array");
    let index = (setter_index + offset_from_setter) % 6;
    s.positions[index]
}

// pub fn get_role_position(s: &SetEntry, rotation: u8, offset_from_setter: usize) -> Uuid {
//     let setter_index = s
//         .positions
//         .iter()
//         .position(|id| *id == s.setter)
//         .expect("setter not found in positions array");
//     let absolute_index = (setter_index + offset_from_setter) % 6;
//     let rotated_index = (absolute_index + rotation as usize) % 6;
//     s.positions[rotated_index]
// }

pub fn is_back_row_player(set: &SetEntry, rotation: u8, player_id: Uuid) -> bool {
    let initial_index = set.positions.iter().position(|id| *id == player_id);
    let Some(initial_index) = initial_index else {
        return false;
    };
    // current position, taking care of rotation
    let rotated_index = (initial_index + rotation as usize) % 6;
    // check if is a back-row player
    rotated_index == 0 || rotated_index == 4 || rotated_index == 5
}
