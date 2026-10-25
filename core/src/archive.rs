//! The archives to share a team or a match, as zip bytes: each app saves
//! them where it likes, e.g. in a file the user chooses or through the
//! share sheet of a phone. An archive holds the blobs as they're stored,
//! and every version reads the archives of the others.

use crate::{
    constants::{MATCH_DESCRIPTOR_FILE_NAME, TEAM_DESCRIPTOR_FILE_NAME},
    errors::{AppError, IOError},
    localization::Labels,
    providers::{
        match_reader::MatchReader,
        match_writer::MatchWriter,
        set_writer::SetWriter,
        store::{read_blob, set_reader::read_events},
        team_reader::TeamReader,
        team_writer::{TeamInput, TeamWriter},
    },
    shapes::{r#match::MatchEntry, set::SetEntry, snapshot::EventEntry, team::TeamEntry},
    storage::{Store, StoreEntry, layout},
};
use serde::Deserialize;
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

/// The descriptor of the team, with its id: the archives exported up to
/// v0.9.0 have it only in their file name, which may change on the way.
/// The older versions ignore it.
pub async fn export_team(store: &dyn Store, team_id: &Uuid) -> Result<Vec<u8>, AppError> {
    let blob = read_blob(store, &layout::team_descriptor(team_id)).await?;
    let mut json: serde_json::Value =
        serde_json::from_slice(&blob).map_err(|e| AppError::IO(IOError::from(e)))?;
    let team_json = match json.as_object_mut() {
        Some(team) => {
            team.insert("id".to_string(), team_id.to_string().into());
            serde_json::to_vec_pretty(&json).map_err(|e| AppError::IO(IOError::from(e)))
        }
        None => Err(AppError::IO(IOError::Msg(format!(
            "the team {team_id} is not a json object"
        )))),
    }?;
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

/// Adds the team of the archive, unless there's one with its id: the one in
/// the archive or, for the archives of the older versions, `file_id`, taken
/// from the name of the file.
pub async fn import_team(
    archive: &[u8],
    file_id: Option<Uuid>,
    team_reader: &(impl TeamReader + Sync + ?Sized),
    team_writer: &(impl TeamWriter + Sync + ?Sized),
    labels: &Labels,
) -> Result<TeamEntry, AppError> {
    let (team, id) = match read_archive(archive, labels)? {
        Archive::Team { team, id } => Ok((team, id.or(file_id))),
        Archive::Match(_) => Err(AppError::IO(IOError::Msg(
            labels.invalid_archive.to_string(),
        ))),
    }?;
    let team_id =
        id.ok_or_else(|| AppError::IO(IOError::Msg(labels.team_archive_without_id.to_string())))?;
    if team_reader.exists(&team_id).await? {
        return Err(AppError::IO(IOError::Msg(
            labels.team_already_exists.to_string(),
        )));
    }
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

/// The players the match refers to, in its lineups and its events.
fn match_players(m: &MatchEntry) -> impl Iterator<Item = Uuid> + '_ {
    m.sets.iter().flat_map(|set| {
        set.initial_positions
            .iter()
            .copied()
            .chain([set.libero, set.setter])
            .chain(set.fallback_libero)
            .chain(
                set.events
                    .iter()
                    .flat_map(|e| e.player.into_iter().chain(e.target_player)),
            )
    })
}

/// Whether the match was scouted for the team: its players are the team's.
pub fn is_match_of(m: &MatchEntry, team: &TeamEntry) -> bool {
    match_players(m).all(|id| team.find_player(id).is_some())
}

/// The teams the match may be imported into, e.g. when the archive is
/// opened outside of any team: usually one, all of them for a match
/// without sets.
pub fn teams_of_match<'a>(m: &MatchEntry, teams: &'a [TeamEntry]) -> Vec<&'a TeamEntry> {
    teams.iter().filter(|team| is_match_of(m, team)).collect()
}

/// Adds the match of the archive to the team, unless the team has one
/// against the same opponent on the same day, or the match is of another
/// team. The sets keep the rules they replay with.
pub async fn import_match(
    archive: &[u8],
    team: &TeamEntry,
    match_reader: &(impl MatchReader + Sync + ?Sized),
    match_writer: &(impl MatchWriter + Sync + ?Sized),
    set_writer: &(impl SetWriter + Sync + ?Sized),
    labels: &Labels,
) -> Result<MatchEntry, AppError> {
    let message = |label: &str| AppError::IO(IOError::Msg(label.to_string()));
    let archived = match read_archive(archive, labels)? {
        Archive::Match(m) => Ok(m),
        Archive::Team { .. } => Err(message(labels.invalid_archive)),
    }?;
    let match_id = MatchEntry::id_of(&archived.opponent, &archived.date);
    match (
        is_match_of(&archived, team),
        match_reader.exists(team, &match_id).await?,
    ) {
        (false, _) => Err(message(labels.match_of_another_team)),
        (true, true) => Err(message(labels.match_already_exists)),
        (true, false) => Ok(()),
    }?;
    let m = match_writer
        .create(
            team,
            archived.opponent,
            archived.date,
            archived.home,
            archived.max_substitutions,
        )
        .await?;
    for mut set in archived.sets {
        // the new descriptor has the substitutions, so the sets saved
        // without their rules, up to v0.3.0, save the ones they replay with
        set.lineup_rules = Some(set.find_lineup_rules(archived.max_substitutions)?);
        set_writer
            .create(
                &m,
                set.set_number,
                set.serving_team,
                set.initial_positions,
                set.libero,
                set.fallback_libero,
                set.setter,
                vec![],
            )
            .await?;
        // the rules and the rows of the archive, not the current ones
        set_writer.update(&m, &set).await?;
    }
    Ok(m)
}

/// The lineup and the events of each set, by number.
type SetMap = BTreeMap<u8, (Option<SetEntry>, Option<Vec<EventEntry>>)>;

fn match_entry(
    descriptor: &str,
    files: &[(String, String)],
    labels: &Labels,
) -> Result<MatchEntry, AppError> {
    let import_error = || AppError::IO(IOError::Msg(labels.import_match_error.to_string()));
    let mut set_map: SetMap = BTreeMap::new();
    for (name, content) in files {
        if let Some(caps) = name.strip_prefix("set_")
            && let Some((num_str, ext)) = caps.split_once('.')
            && let Ok(num) = num_str.parse::<u8>()
        {
            let entry = set_map.entry(num).or_insert((None, None));
            match ext {
                "json" => entry.0 = parse_set_json(num, content),
                "csv" => entry.1 = read_events(content.as_bytes()).ok(),
                _ => {}
            }
        }
    }
    let len = set_map.len();
    let mut match_entry: MatchEntry =
        serde_json::from_str(descriptor).map_err(|_| import_error())?;
    match_entry.id = MatchEntry::id_of(&match_entry.opponent, &match_entry.date);
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

fn parse_set_json(set_number: u8, content: &str) -> Option<SetEntry> {
    serde_json::from_str(content)
        .ok()
        .map(|mut entry: SetEntry| {
            entry.set_number = set_number;
            entry
        })
}
