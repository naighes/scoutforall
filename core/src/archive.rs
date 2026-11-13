//! The archives to share a team or a match, as zip bytes: each app saves
//! them where it likes, e.g. in a file the user chooses or through the
//! share sheet of a phone. An archive holds the blobs as they're stored,
//! and every version reads the archives of the others.

use crate::{
    constants::TEAM_DESCRIPTOR_FILE_NAME,
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
    shapes::{
        r#match::{MatchEntry, MatchSource},
        player::PlayerEntry,
        team::TeamEntry,
    },
    storage::{Store, StoreEntry, layout},
    video::VIDEO_PROFILE_FILE_NAME,
};
// the archives of the versions before the database
pub use crate::legacy::archive::{Archive, file_team_id, read_archive};
use std::io::{Cursor, Write};
use uuid::Uuid;
use zip::{
    CompressionMethod::{Deflated, Stored},
    ZipWriter,
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

/// The descriptor of the match and its sets (not its video).
pub async fn export_match(
    store: &dyn Store,
    team_id: &Uuid,
    match_id: &str,
) -> Result<Vec<u8>, AppError> {
    let folder = layout::match_folder(team_id, match_id);
    let mut files = Vec::new();
    for entry in store.list(&folder).await? {
        // the video has paths of this computer
        if let StoreEntry::Blob(name) = entry
            && name != VIDEO_PROFILE_FILE_NAME
        {
            let data = read_blob(store, &layout::child(&folder, &name)).await?;
            files.push((name, data));
        }
    }
    zip_files(&files, Deflated)
}

pub(crate) fn zip_files(
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

/// The team of the archive, with the id it's imported with: the one in the
/// archive or, for the archives of the older versions, `file_id`. Unless
/// there's already a team with that id.
pub async fn team_to_import(
    archive: &[u8],
    file_id: Option<Uuid>,
    team_reader: &(impl TeamReader + Sync + ?Sized),
    labels: &Labels,
) -> Result<TeamEntry, AppError> {
    let message = |label: &str| AppError::IO(IOError::Msg(label.to_string()));
    let (mut team, id) = match read_archive(archive, labels)? {
        Archive::Team { team, id } => Ok((team, id.or(file_id))),
        Archive::Match(_) => Err(message(labels.invalid_archive)),
    }?;
    team.id = id.ok_or_else(|| message(labels.team_archive_without_id))?;
    match team_reader.exists(&team.id).await? {
        true => Err(message(labels.team_already_exists)),
        false => Ok(team),
    }
}

/// Adds the team of the archive, as `team_to_import` finds it.
pub async fn import_team(
    archive: &[u8],
    file_id: Option<Uuid>,
    team_reader: &(impl TeamReader + Sync + ?Sized),
    team_writer: &(impl TeamWriter + Sync + ?Sized),
    labels: &Labels,
) -> Result<TeamEntry, AppError> {
    let team = team_to_import(archive, file_id, team_reader, labels).await?;
    team_writer
        .save(TeamInput::New {
            id: Some(team.id),
            name: team.name,
            year: team.year,
            classification: team.classification,
            gender: team.gender,
            country: team.country,
            players: team.players,
        })
        .await
}

/// Whether the match was scouted for the team: its players, in its lineups
/// and its events, are the team's.
pub fn is_match_of(m: &MatchEntry, team: &TeamEntry) -> bool {
    m.played().iter().all(|id| team.find_player(*id).is_some())
}

/// The roster of the archive, as the match form would save it for the
/// team: its players of the team and, after them by number, who played.
/// None if the archive has none (up to v0.10.1).
pub(crate) fn roster_for(m: &MatchEntry, team: &TeamEntry) -> Vec<Uuid> {
    let played = m.played();
    let mut players: Vec<&PlayerEntry> = team.players.iter().collect();
    players.sort_by_key(|p| p.number);
    let kept = m
        .roster
        .iter()
        .copied()
        .filter(|id| team.find_player(*id).is_some());
    let missing = players
        .into_iter()
        .map(|p| p.id)
        .filter(|id| played.contains(id) && !m.roster.contains(id));
    match m.roster.is_empty() {
        true => vec![],
        false => kept.chain(missing).collect(),
    }
}

/// The teams the match may be imported into, e.g. when the archive is
/// opened outside of any team: usually one, all of them for a match
/// without sets.
pub fn teams_of_match<'a>(m: &MatchEntry, teams: &'a [TeamEntry]) -> Vec<&'a TeamEntry> {
    teams.iter().filter(|team| is_match_of(m, team)).collect()
}

/// The match of the archive and the teams it may be imported into: those
/// of its players that don't have it yet. Unless there's none.
pub async fn match_to_import<'a>(
    archive: &[u8],
    teams: &'a [TeamEntry],
    match_reader: &(impl MatchReader + Sync + ?Sized),
    labels: &Labels,
) -> Result<(MatchEntry, Vec<&'a TeamEntry>), AppError> {
    let message = |label: &str| AppError::IO(IOError::Msg(label.to_string()));
    let m = match read_archive(archive, labels)? {
        Archive::Match(m) => Ok(m),
        Archive::Team { .. } => Err(message(labels.invalid_archive)),
    }?;
    let ours = teams_of_match(&m, teams);
    let mut free = Vec::new();
    for team in &ours {
        if !match_reader.exists(team, &m.id).await? {
            free.push(*team);
        }
    }
    match (ours.is_empty(), free.is_empty()) {
        (true, _) => Err(message(labels.match_of_another_team)),
        (false, true) => Err(message(labels.match_already_exists)),
        (false, false) => Ok((m, free)),
    }
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
    // a set this version doesn't replay: nothing is written
    if let Some(issue) = archived.replay_issue() {
        return Err(message(&issue.describe(labels)));
    }
    let roster = roster_for(&archived, team);
    let m = match_writer
        .create(
            team,
            archived.opponent,
            archived.date,
            archived.home,
            archived.max_substitutions,
            archived.notes,
            roster,
        )
        .await?;
    // an imported match exported, and imported again: still imported
    let m = match (archived.source, archived.video_clock) {
        (MatchSource::Scout4all, false) => m,
        (source, video_clock) => match_writer.save_source(&m, source, video_clock).await?,
    };
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
