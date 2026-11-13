//! The data folder (`~/.scout4all`) as the versions before the database
//! left it, only read: nothing is written, not even the index of the
//! matches the providers over the files keep.
//!
//! ```text
//! <team id>/team.json
//! <team id>/<match id>/match.json
//! <team id>/<match id>/set_<n>.json, set_<n>.csv
//! <team id>/<match id>/video.json (from v0.15.0)
//! ```
//!
//! A folder of the root is a team when it's named after an id or has a
//! `team.json` (`team_folders`): any other (the recordings of the desktop,
//! when it keeps them there) isn't data of the app, and isn't read.
//!
//! A team or a match that can't be read is told with why (`NotRead`), the
//! others are read all the same. The sets aren't replayed: whether a match
//! replays is for who uses it to tell.

use crate::{
    constants::TEAM_DESCRIPTOR_FILE_NAME,
    errors::{AppError, IOError, MatchError},
    legacy::format::{fill_lineup_rules, read_descriptor, read_events, read_lineup, set_file},
    localization::Labels,
    providers::store::read_json,
    shapes::{r#match::MatchEntry, set::SetEntry, team::TeamEntry},
    storage::{Store, StoreEntry, layout},
    video::{StoredProfile, VIDEO_PROFILE_FILE_NAME, VideoProfile, read_profile},
};
use futures::future::{join_all, try_join_all};
use uuid::Uuid;

/// A team or a match that couldn't be read: its folder (the key in the
/// store), and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotRead {
    pub folder: String,
    pub reason: String,
}

impl NotRead {
    fn of(folder: &str, e: AppError) -> Self {
        NotRead {
            folder: folder.to_string(),
            reason: e.to_string(),
        }
    }
}

/// A team of the folder with its matches, each read or not.
#[derive(Debug, Clone)]
pub struct LegacyTeam {
    pub team: TeamEntry,
    /// by the name of their folders, which is their id
    pub matches: Vec<Result<MatchEntry, NotRead>>,
}

/// The teams of the data folder, by the name of their folders.
pub async fn read_folder(
    store: &dyn Store,
    labels: &Labels,
) -> Result<Vec<Result<LegacyTeam, NotRead>>, AppError> {
    let folders = team_folders(store).await?;
    Ok(join_all(folders.iter().map(|folder| async move {
        let team = read_team(store, folder, labels)
            .await
            .map_err(|e| NotRead::of(folder, e))?;
        let matches = folders_of(store, &team).await?;
        Ok(LegacyTeam {
            matches: join_all(matches.iter().map(|id| async {
                read_match(store, &team, id, labels)
                    .await
                    .map_err(|e| NotRead::of(&layout::match_folder(&team.id, id), e))
            }))
            .await,
            team,
        })
    }))
    .await)
}

async fn folders_of(store: &dyn Store, team: &TeamEntry) -> Result<Vec<String>, NotRead> {
    let folder = layout::team_folder(&team.id);
    folders(store, &folder)
        .await
        .map_err(|e| NotRead::of(&folder, e))
}

/// The folders of the root that are teams (see the module), by name.
pub(crate) async fn team_folders(store: &dyn Store) -> Result<Vec<String>, AppError> {
    let mut teams = Vec::new();
    for folder in folders(store, layout::ROOT).await? {
        let named = Uuid::parse_str(&folder).is_ok();
        let described = || async {
            store.list(&folder).await.map(|entries| {
                entries.contains(&StoreEntry::Blob(TEAM_DESCRIPTOR_FILE_NAME.into()))
            })
        };
        if named || described().await? {
            teams.push(folder);
        }
    }
    Ok(teams)
}

/// The folders right in `folder`, by name.
async fn folders(store: &dyn Store, folder: &str) -> Result<Vec<String>, AppError> {
    Ok(store
        .list(folder)
        .await?
        .into_iter()
        .filter_map(|entry| match entry {
            StoreEntry::Folder(name) => Some(name),
            StoreEntry::Blob(_) => None,
        })
        .collect())
}

/// The team in the folder, named after its id: the id in `team.json`, if
/// any, doesn't count.
pub async fn read_team(
    store: &dyn Store,
    folder: &str,
    labels: &Labels,
) -> Result<TeamEntry, AppError> {
    let id = Uuid::parse_str(folder)
        .map_err(|_| AppError::IO(IOError::Msg(labels.invalid_team_folder_name.to_string())))?;
    let team: TeamEntry = read_json(store, &layout::team_descriptor(&id)).await?;
    Ok(TeamEntry { id, ..team })
}

/// The match `match_id` of the team, with its sets: its team is the team
/// as the match sees it (`TeamEntry::with_roster`).
pub async fn read_match(
    store: &dyn Store,
    team: &TeamEntry,
    match_id: &str,
    labels: &Labels,
) -> Result<MatchEntry, AppError> {
    let content = store
        .read(&layout::match_descriptor(&team.id, match_id))
        .await?
        .ok_or_else(|| {
            AppError::IO(IOError::Msg(
                labels.match_descriptor_file_not_found.to_string(),
            ))
        })?;
    let (entry, rules) = read_descriptor(&content)?;
    let mut entry = MatchEntry {
        id: match_id.to_string(),
        team: team.with_roster(&entry.roster),
        ..entry
    };
    entry.sets = read_sets(store, &layout::match_folder(&team.id, match_id), labels).await?;
    fill_lineup_rules(&mut entry.sets, rules);
    Ok(entry)
}

/// The profile of the video of the match, if it has one: unless written
/// by a newer version.
pub async fn read_video(
    store: &dyn Store,
    team_id: &Uuid,
    match_id: &str,
) -> Result<Option<VideoProfile>, AppError> {
    match read_profile(store, team_id, match_id).await? {
        StoredProfile::Missing => Ok(None),
        StoredProfile::Profile(profile) => Ok(Some(profile)),
        StoredProfile::Newer(version) => Err(AppError::IO(IOError::Msg(format!(
            "{VIDEO_PROFILE_FILE_NAME} written by a newer version ({version})"
        )))),
    }
}

/// The sets in the folder of a match, by number: a lineup without events
/// is a set just started. Unless one can't be read, they're more than 5,
/// or a number is missing.
pub async fn read_sets(
    store: &dyn Store,
    folder: &str,
    labels: &Labels,
) -> Result<Vec<SetEntry>, AppError> {
    let entries = store.list(folder).await.map_err(|_| {
        AppError::Match(MatchError::LoadSetError(
            labels.could_not_read_folder.replace("{}", folder),
        ))
    })?;
    let lineups = entries.iter().filter_map(|entry| match entry {
        StoreEntry::Blob(name) => match set_file(name) {
            Some((number, "json")) => Some((name, number)),
            _ => None,
        },
        StoreEntry::Folder(_) => None,
    });
    let mut sets: Vec<SetEntry> = try_join_all(lineups.map(|(name, number)| async move {
        read_set(store, folder, name, number)
            .await
            .ok_or_else(|| AppError::IO(IOError::Msg(labels.invalid_set.into())))
    }))
    .await?;
    if sets.len() > 5 {
        let match_id = folder.rsplit('/').next().unwrap_or(folder);
        return Err(AppError::Match(MatchError::LoadSetError(
            labels
                .found_more_than_5_sets_in_match
                .replace("{}", match_id),
        )));
    }
    sets.sort_by_key(|s| s.set_number);
    match sets
        .iter()
        .enumerate()
        .all(|(i, set)| set.set_number as usize == i + 1)
    {
        true => Ok(sets),
        false => Err(AppError::Match(MatchError::LoadSetError(
            labels.wrong_set_numbering.into(),
        ))),
    }
}

/// The set whose lineup is in the blob `name`, with its events if any.
async fn read_set(
    store: &dyn Store,
    folder: &str,
    name: &str,
    number: Option<u8>,
) -> Option<SetEntry> {
    let number = number?;
    let json = store.read(&layout::child(folder, name)).await.ok()??;
    let set = read_lineup(number, &json)?;
    let events_key = layout::child(folder, &layout::set_events_name(number));
    match store.read(&events_key).await.ok()? {
        Some(csv) => read_events(&csv)
            .ok()
            .map(|events| SetEntry { events, ..set }),
        None => Some(set),
    }
}
