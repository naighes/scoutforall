//! The archives exported by the versions before the database, only read:
//! a team (`team.json`, with its id from v0.10.1, only in the name of the
//! file up to v0.9.0) or a match (`match.json` and its sets, without its
//! team: its players tell which one it is).

use crate::{
    constants::{MATCH_DESCRIPTOR_FILE_NAME, TEAM_DESCRIPTOR_FILE_NAME},
    errors::{AppError, IOError},
    legacy::format::{fill_lineup_rules, read_descriptor, read_events, read_lineup, set_file},
    localization::Labels,
    shapes::{r#match::MatchEntry, set::SetEntry, snapshot::EventEntry, team::TeamEntry},
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    path::Path,
};
use uuid::Uuid;
use zip::ZipArchive;

/// What an archive holds, e.g. to show it before importing it.
#[derive(Debug, Clone)]
pub enum Archive {
    /// The team, with its id when the archive has it (not up to v0.9.0).
    Team { team: TeamEntry, id: Option<Uuid> },
    /// The match with its sets, without its team.
    Match(MatchEntry),
}

/// The id written in the team descriptor of an archive.
#[derive(Deserialize)]
struct ArchivedTeamId {
    id: Option<Uuid>,
}

/// The team or the match of the archive.
pub fn read_archive(archive: &[u8], labels: &Labels) -> Result<Archive, AppError> {
    let invalid = || AppError::IO(IOError::Msg(labels.invalid_archive.to_string()));
    let files = archive_files(archive).map_err(|_| invalid())?;
    let descriptor = |name: &str| {
        files
            .iter()
            .find(|(file, _)| file.eq_ignore_ascii_case(name))
            .map(|(_, content)| content)
    };
    match (
        descriptor(TEAM_DESCRIPTOR_FILE_NAME),
        descriptor(MATCH_DESCRIPTOR_FILE_NAME),
    ) {
        (Some(json), _) => {
            let team: TeamEntry = serde_json::from_str(json).map_err(|_| invalid())?;
            let id = serde_json::from_str::<ArchivedTeamId>(json)
                .ok()
                .and_then(|t| t.id);
            Ok(Archive::Team { team, id })
        }
        (None, Some(json)) => match_entry(json, &files, labels).map(Archive::Match),
        (None, None) => Err(invalid()),
    }
}

/// The id of a team archive in the name of its file, the only place the
/// archives exported up to v0.9.0 have it.
pub fn file_team_id(file_name: &str) -> Option<Uuid> {
    Path::new(file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| Uuid::parse_str(stem).ok())
}

/// The name and the content of each file in the archive.
fn archive_files(archive: &[u8]) -> Result<Vec<(String, String)>, AppError> {
    let mut archive =
        ZipArchive::new(Cursor::new(archive)).map_err(|e| AppError::IO(IOError::from(e)))?;
    (0..archive.len())
        .map(|i| file_content(&mut archive, i))
        .collect()
}

fn file_content(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    i: usize,
) -> Result<(String, String), AppError> {
    let mut entry = archive
        .by_index(i)
        .map_err(|e| AppError::IO(IOError::from(e)))?;
    let name = entry
        .name()
        .map_err(|e| AppError::IO(IOError::from(e)))?
        .to_string();
    let mut content = String::new();
    entry
        .read_to_string(&mut content)
        .map_err(|e| AppError::IO(IOError::from(e)))?;
    Ok((name, content))
}

/// The lineup and the events of each set, by number.
type SetMap = BTreeMap<u8, (Option<SetEntry>, Option<Vec<EventEntry>>)>;

/// The match of the archive, with the id its folder had (opponent and
/// day): every set with both its lineup and its events.
fn match_entry(
    descriptor: &str,
    files: &[(String, String)],
    labels: &Labels,
) -> Result<MatchEntry, AppError> {
    let import_error = || AppError::IO(IOError::Msg(labels.import_match_error.to_string()));
    let mut set_map: SetMap = BTreeMap::new();
    for (name, content) in files {
        if let Some((Some(number), extension)) = set_file(name) {
            let entry = set_map.entry(number).or_insert((None, None));
            match extension {
                "json" => entry.0 = read_lineup(number, content.as_bytes()),
                "csv" => entry.1 = read_events(content.as_bytes()).ok(),
                _ => {}
            }
        }
    }
    let len = set_map.len();
    let (mut match_entry, rules) =
        read_descriptor(descriptor.as_bytes()).map_err(|_| import_error())?;
    match_entry.id = MatchEntry::id_of(&match_entry.opponent, &match_entry.date);
    for (set, events) in set_map.into_values() {
        if let (Some(set), Some(events)) = (set, events) {
            match_entry.sets.push(SetEntry { events, ..set });
        }
    }
    fill_lineup_rules(&mut match_entry.sets, rules);
    match match_entry.sets.len() == len {
        true => Ok(match_entry),
        false => Err(import_error()),
    }
}
