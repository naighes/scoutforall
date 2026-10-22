//! The archives to share a team or a match, as zip bytes: each app saves
//! them where it likes, e.g. in a file the user chooses or through the
//! share sheet of a phone. An archive holds the blobs as they're stored.

use crate::{
    constants::{MATCH_DESCRIPTOR_FILE_NAME, TEAM_DESCRIPTOR_FILE_NAME},
    errors::{AppError, IOError},
    localization::Labels,
    providers::{
        match_reader::MatchReader,
        match_writer::MatchWriter,
        set_writer::SetWriter,
        store::read_blob,
        team_reader::TeamReader,
        team_writer::{TeamInput, TeamWriter},
    },
    shapes::{r#match::MatchEntry, set::SetEntry, snapshot::EventEntry, team::TeamEntry},
    storage::{Store, StoreEntry, layout},
};
use csv::ReaderBuilder;
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};
use uuid::Uuid;
use zip::{
    CompressionMethod::{Deflated, Stored},
    ZipArchive, ZipWriter,
    write::FileOptions,
};

/// The descriptor of the team.
pub async fn export_team(store: &dyn Store, team_id: &Uuid) -> Result<Vec<u8>, AppError> {
    let team_json = read_blob(store, &layout::team_descriptor(team_id)).await?;
    zip_files(
        &[(TEAM_DESCRIPTOR_FILE_NAME.to_string(), team_json)],
        Stored,
    )
}

/// The descriptor of the match and its sets.
pub async fn export_match(
    store: &dyn Store,
    team_id: &Uuid,
    match_id: &str,
) -> Result<Vec<u8>, AppError> {
    let folder = layout::match_folder(team_id, match_id);
    let mut files = Vec::new();
    for entry in store.list(&folder).await? {
        if let StoreEntry::Blob(name) = entry {
            let data = read_blob(store, &layout::child(&folder, &name)).await?;
            files.push((name, data));
        }
    }
    zip_files(&files, Deflated)
}

fn zip_files(
    files: &[(String, Vec<u8>)],
    compression: zip::CompressionMethod,
) -> Result<Vec<u8>, AppError> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        zip.start_file::<_, ()>(
            name.as_str(),
            FileOptions::default().compression_method(compression),
        )
        .map_err(|e| AppError::IO(IOError::from(e)))?;
        zip.write_all(data)
            .map_err(|e| AppError::IO(IOError::from(e)))?;
    }
    zip.finish()
        .map(Cursor::into_inner)
        .map_err(|e| AppError::IO(IOError::from(e)))
}

/// Adds the team of the archive, with the given id, unless there's one.
pub async fn import_team(
    archive: &[u8],
    team_id: Uuid,
    team_reader: &(impl TeamReader + Sync + ?Sized),
    team_writer: &(impl TeamWriter + Sync + ?Sized),
    labels: &Labels,
) -> Result<TeamEntry, AppError> {
    if team_reader.exists(&team_id).await? {
        return Err(AppError::IO(IOError::Msg(
            labels.team_already_exists.to_string(),
        )));
    }
    let json = team_json(archive)?;
    let team: TeamEntry =
        serde_json::from_str(&json).map_err(|e| AppError::IO(IOError::from(e)))?;
    team_writer
        .save(TeamInput::New {
            id: Some(team_id),
            name: team.name,
            year: team.year,
            classification: team.classification,
            gender: team.gender,
            players: team.players,
        })
        .await
}

fn team_json(archive: &[u8]) -> Result<String, AppError> {
    let mut archive =
        ZipArchive::new(Cursor::new(archive)).map_err(|e| AppError::IO(IOError::from(e)))?;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        let is_team_json = file
            .name()
            .map_err(|e| AppError::IO(IOError::from(e)))?
            .eq_ignore_ascii_case(TEAM_DESCRIPTOR_FILE_NAME);
        if is_team_json {
            let mut content = String::new();
            file.read_to_string(&mut content)
                .map_err(|e| AppError::IO(IOError::from(e)))?;
            return Ok(content);
        }
    }
    Err(AppError::IO(IOError::Msg(
        "unexpected content within the zip archive".to_string(),
    )))
}

/// Adds the match of the archive to the team, unless there's one with the
/// given id. The new match is named after its date and opponent, whatever
/// the id of the archive.
pub async fn import_match(
    archive: &[u8],
    match_id: &str,
    team: &TeamEntry,
    match_reader: &(impl MatchReader + Sync + ?Sized),
    match_writer: &(impl MatchWriter + Sync + ?Sized),
    set_writer: &(impl SetWriter + Sync + ?Sized),
    labels: &Labels,
) -> Result<MatchEntry, AppError> {
    if match_reader.exists(team, match_id).await? {
        return Err(AppError::IO(IOError::Msg(
            labels.match_already_exists.to_string(),
        )));
    }
    let match_entry = match_content(archive, team, match_id, labels)?;
    let m = match_writer
        .create(
            &match_entry.team,
            match_entry.opponent,
            match_entry.date,
            match_entry.home,
            match_entry.max_substitutions,
        )
        .await?;
    for set in match_entry.sets {
        set_writer
            .create(
                &m,
                set.set_number,
                set.serving_team,
                set.initial_positions,
                set.libero,
                set.fallback_libero,
                set.setter,
                set.events,
            )
            .await?;
    }
    Ok(m)
}

/// The lineup and the events of each set, by number.
type SetMap = BTreeMap<u8, (Option<SetEntry>, Option<Vec<EventEntry>>)>;

fn match_content(
    archive: &[u8],
    team: &TeamEntry,
    match_id: &str,
    labels: &Labels,
) -> Result<MatchEntry, AppError> {
    let import_error = || AppError::IO(IOError::Msg(labels.import_match_error.to_string()));
    let mut archive =
        ZipArchive::new(Cursor::new(archive)).map_err(|e| AppError::IO(IOError::from(e)))?;
    let mut set_map: SetMap = BTreeMap::new();
    let mut match_entry_content = String::new();
    for i in 0..archive.len() {
        let (name, content) = file_content(&mut archive, i)?;
        if name.eq_ignore_ascii_case(MATCH_DESCRIPTOR_FILE_NAME) {
            match_entry_content = content;
        } else if let Some(caps) = name.strip_prefix("set_")
            && let Some((num_str, ext)) = caps.split_once('.')
            && let Ok(num) = num_str.parse::<u8>()
        {
            let entry = set_map.entry(num).or_insert((None, None));
            match ext {
                "json" => entry.0 = parse_set_json(num, &content),
                "csv" => entry.1 = parse_set_csv(&content),
                _ => {}
            }
        }
    }
    let len = set_map.len();
    let mut match_entry: MatchEntry =
        serde_json::from_str(&match_entry_content).map_err(|_| import_error())?;
    match_entry.id = match_id.to_string();
    match_entry.team = team.clone();
    for (set, events) in set_map.into_values() {
        if let (Some(mut set), Some(events)) = (set, events) {
            set.events = events;
            match_entry.sets.push(set);
        }
    }
    match match_entry.sets.len() == len {
        true => Ok(match_entry),
        false => Err(import_error()),
    }
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

fn parse_set_json(set_number: u8, content: &str) -> Option<SetEntry> {
    serde_json::from_str(content)
        .ok()
        .map(|mut entry: SetEntry| {
            entry.set_number = set_number;
            entry
        })
}

fn parse_set_csv(content: &str) -> Option<Vec<EventEntry>> {
    ReaderBuilder::new()
        .has_headers(false)
        .from_reader(content.as_bytes())
        .deserialize()
        .collect::<Result<Vec<EventEntry>, csv::Error>>()
        .ok()
}
