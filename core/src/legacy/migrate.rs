//! The data folder of the versions before the database moved into it, at
//! the start of every app (TODO/001, D4): nothing to run by hand.
//!
//! - **Repeatable**: each start looks for what's still in the folder (a
//!   version before the database may write there again on the same
//!   device) and moves it; with nothing to move, nothing is done.
//! - **A backup first**: before changing anything, the whole folder in a
//!   zip beside it (`<prefix>-backup-<date>.zip`), read back before going
//!   on; without it nothing changes.
//! - **Safe halfway**: a team is written in one transaction, each match in
//!   one with its check (read back the same rows as written); only then
//!   its folder is removed. A stop anywhere leaves at most a match both in
//!   the database and in the folder: the next start finds it by where it
//!   was (`LegacyId`) and, the same, removes the folder.
//! - **The database wins**: a team already there keeps its details, only
//!   its missing players are added (a person already there keeps hers); a
//!   match already there but different (changed since, here or there) is
//!   left in its folder, and told.
//! - What can't be read or converted stays where it is, with why; the rest
//!   moves. So does a folder with files the database doesn't know (or a
//!   `video.json` that can't be read): the match moves, its folder stays.
//! - The video of a match (`video.json`) moves with it, in its
//!   transaction. A match already there gets the video of its folder if it
//!   has none (a folder left for its video by a version before TODO/001
//!   step 15); with a different one it's left, and told.
//! - The settings (`config.json`, `clips.json`) aren't data of the teams:
//!   they stay.
//! - The queue of the uploads (`upload_queue.json`) moves after the
//!   matches, in its order, after the queue of the database: a match
//!   moved by where it was; a match still in its folder stays in the file,
//!   for the next run; the others (removed, gone) leave it, as the upload
//!   would drop them.

use crate::{
    analytics::queue::UploadQueue,
    constants::{MATCH_DESCRIPTOR_FILE_NAME, TEAM_DESCRIPTOR_FILE_NAME},
    domain::adapters::VideoRows,
    domain::{
        LegacyId, Match,
        adapters::{MatchRows, SetRows, TeamRows},
        sql::Sql,
    },
    errors::{AppError, IOError},
    legacy::{
        folder::{LegacyTeam, NotRead, read_folder, read_video, team_folders},
        format::set_file,
        rows::{FolderRows, folder_rows, video_rows},
    },
    localization::Labels,
    logging::logger::log_info,
    providers::sqlite::{
        Database, not_found,
        rows::{insert_match, match_rows},
        sqlite, summary,
        video::{self, write_video},
    },
    shapes::set::SetEntry,
    storage::{Store, StoreEntry, layout, write_json},
    video::{VIDEO_PROFILE_FILE_NAME, VideoProfile},
};
use chrono::{Local, NaiveDate};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::from_slice;
use std::{
    io::{Cursor, Read, Write},
    path::PathBuf,
};
use uuid::Uuid;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::FileOptions};

/// Where the backup of the folder goes: `<dir>/<prefix>-backup-<date>.zip`
/// (`-2`, `-3`… when taken), e.g. `~/.scout4all-backup-2026-11-13.zip`.
#[derive(Debug, Clone)]
pub struct Backup {
    pub dir: PathBuf,
    pub prefix: String,
}

/// What a migration did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Migrated {
    /// the backup taken, when something was to change
    pub backup: Option<PathBuf>,
    /// teams written: new, or with players added
    pub teams: usize,
    /// matches written
    pub matches: usize,
    /// videos written, with their matches or to matches already there
    pub videos: usize,
    /// folders of matches already in the database, removed
    pub already: usize,
    /// the folders left in the data folder, with why
    pub left: Vec<NotRead>,
}

impl Migrated {
    /// What to tell at the start: what stayed in the data folder, a line
    /// each with why, and the backup when one was taken; nothing when all
    /// moved.
    pub fn notice(&self, labels: &Labels) -> Option<String> {
        let left = self
            .left
            .iter()
            .map(|n| format!("- {}: {}", n.folder, n.reason));
        let backup = self.backup.iter().map(|path| {
            labels
                .migration_backup
                .replace("{}", &path.display().to_string())
        });
        match self.left.is_empty() {
            true => None,
            false => Some(
                std::iter::once(labels.migration_left.to_string())
                    .chain(left)
                    .chain(backup)
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
        }
    }
}

/// What's done with a match of the folder; then its folder is removed if
/// `remove` (it holds only what's moved).
enum Step {
    /// written to the database, with its video if any
    Write {
        rows: Box<MatchRows>,
        video: Option<VideoRows>,
        remove: bool,
    },
    /// already there: the video written to it, if any
    Already {
        video: Option<VideoRows>,
        remove: bool,
    },
}

/// What's to do with a match of the folder, by the database.
enum Plan {
    /// not in the database: written with its video, then its folder
    /// removed
    New {
        rows: Box<MatchRows>,
        video: Option<VideoRows>,
    },
    /// in the database (or removed there) and the same: the video written
    /// if the database has none, then its folder removed
    Already { video: Option<VideoRows> },
    /// in the database but different (or its video): left, told
    Differs,
}

/// The video in the folder of a match.
enum FolderVideo {
    None,
    /// moved to the database with the match
    Moves(VideoProfile),
    /// can't be read: it stays in the folder (why in the log)
    Stays,
}

impl FolderVideo {
    fn profile(&self) -> Option<&VideoProfile> {
        match self {
            FolderVideo::Moves(profile) => Some(profile),
            FolderVideo::None | FolderVideo::Stays => None,
        }
    }

    /// Whether its `video.json`, if any, goes with the folder.
    fn moves(&self) -> bool {
        matches!(self, FolderVideo::Moves(_))
    }
}

/// The video of the match in the folder, if any.
async fn folder_video(store: &dyn Store, team: &Uuid, match_id: &str) -> FolderVideo {
    match read_video(store, team, match_id).await {
        Ok(None) => FolderVideo::None,
        Ok(Some(profile)) => FolderVideo::Moves(profile),
        Err(e) => {
            log_info(&format!(
                "{}: {VIDEO_PROFILE_FILE_NAME} not moved: {e}",
                layout::match_folder(team, match_id)
            ));
            FolderVideo::Stays
        }
    }
}

/// Moves what's in the data folder `store` into `db`: see the module.
/// An error only when nothing could be done (the folder not listed, the
/// backup or the teams not written); what's left of the rest is told in
/// `Migrated::left`.
pub async fn migrate(
    store: &dyn Store,
    db: &Database,
    backup: &Backup,
    labels: &Labels,
) -> Result<Migrated, AppError> {
    let read = read_folder(store, labels).await?;
    let mut left: Vec<NotRead> = read.iter().filter_map(|t| t.clone().err()).collect();
    let teams: Vec<LegacyTeam> = read.into_iter().filter_map(Result::ok).collect();
    let FolderRows {
        teams: shared,
        matches,
    } = folder_rows(&teams)?;
    let mut folders: Vec<(String, FolderVideo)> = Vec::new();
    for t in &teams {
        for m in &t.matches {
            folders.push(match m {
                Ok(m) => (
                    layout::match_folder(&t.team.id, &m.id),
                    folder_video(store, &t.team.id, &m.id).await,
                ),
                Err(e) => (e.folder.clone(), FolderVideo::None),
            });
        }
    }
    let (to_write, plans) = db.read(|tx| {
        let to_write: Vec<&TeamRows> = shared
            .teams
            .iter()
            .map(|t| missing(tx, t).map(|m| (t, m)))
            .filter_map(|r| match r {
                Ok((t, true)) => Some(Ok(t)),
                Ok((_, false)) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<_, _>>()?;
        let plans: Vec<Result<Plan, NotRead>> = matches
            .into_iter()
            .zip(&folders)
            .map(|(m, (_, video))| match m {
                Ok(rows) => plan(tx, rows, video.profile()),
                Err(e) => Ok(Err(e)),
            })
            .collect::<Result<_, _>>()?;
        Ok((to_write, plans))
    })?;
    let mut steps: Vec<(String, Step, bool)> = Vec::new();
    for ((folder, video), plan) in folders.into_iter().zip(plans) {
        let video_moves = video.moves();
        let kept = match &plan {
            Ok(Plan::New { .. } | Plan::Already { .. }) => {
                kept_files(store, &folder, video_moves, labels).await
            }
            _ => Ok(None),
        };
        let told = |reason: String| NotRead {
            folder: folder.clone(),
            reason,
        };
        match (plan, kept) {
            (Err(e), _) => left.push(e),
            (Ok(Plan::Differs), _) => left.push(told(labels.migration_differs.to_string())),
            (Ok(_), Err(e)) => left.push(e),
            (Ok(Plan::New { rows, video }), Ok(kept)) => {
                let remove = kept.is_none();
                left.extend(kept);
                steps.push((
                    folder,
                    Step::Write {
                        rows,
                        video,
                        remove,
                    },
                    video_moves,
                ));
            }
            (Ok(Plan::Already { video: None }), Ok(Some(kept))) => left.push(kept),
            (Ok(Plan::Already { video }), Ok(kept)) => {
                let remove = kept.is_none();
                left.extend(kept);
                steps.push((folder, Step::Already { video, remove }, video_moves));
            }
        }
    }
    let going: Vec<&str> = steps
        .iter()
        .filter(|(_, step, _)| {
            matches!(
                step,
                Step::Already { remove: true, .. } | Step::Write { remove: true, .. }
            )
        })
        .map(|(folder, _, _)| folder.as_str())
        .collect();
    let empty_teams = teams_to_remove(store, &teams, &going).await;
    if to_write.is_empty() && steps.is_empty() && empty_teams.is_empty() {
        // a run stopped before it moved the queue or removed the index
        move_queue(store, db, &mut left).await;
        remove_index(store, &mut left).await;
        return Ok(Migrated {
            left,
            ..Migrated::default()
        });
    }
    let backup = take_backup(store, backup).await?;
    log_info(&format!("data folder backed up in {}", backup.display()));
    db.write(|tx| {
        to_write
            .iter()
            .try_for_each(|team| insert_team_rows(tx, team))
    })?;
    let mut migrated = Migrated {
        backup: Some(backup),
        teams: to_write.len(),
        ..Migrated::default()
    };
    for (folder, step, video_moves) in steps {
        let written = match &step {
            Step::Write { rows, video, .. } => db.write(|tx| {
                insert_checked(tx, rows)?;
                video
                    .as_ref()
                    .map_or(Ok(()), |video| write_video_checked(tx, video))
            }),
            Step::Already { video, .. } => video.as_ref().map_or(Ok(()), |video| {
                db.write(|tx| write_video_checked(tx, video))
            }),
        };
        let remove = match (written, step) {
            (Err(e), _) => {
                left.push(NotRead {
                    folder: folder.clone(),
                    reason: e.to_string(),
                });
                false
            }
            (Ok(()), Step::Write { video, remove, .. }) => {
                migrated.matches += 1;
                migrated.videos += usize::from(video.is_some());
                remove
            }
            (Ok(()), Step::Already { video, remove }) => {
                migrated.already += 1;
                migrated.videos += usize::from(video.is_some());
                remove
            }
        };
        if remove && let Err(e) = remove_match_folder(store, &folder, video_moves, labels).await {
            left.push(e);
        }
    }
    for team in teams_to_remove(store, &teams, &[]).await {
        if let Err(e) = store.remove_folder(&team).await {
            left.push(NotRead {
                folder: team,
                reason: e.to_string(),
            });
        }
    }
    move_queue(store, db, &mut left).await;
    remove_index(store, &mut left).await;
    log_info(&format!(
        "data folder migrated: {} teams, {} matches, {} videos, {} already there, {} left",
        migrated.teams,
        migrated.matches,
        migrated.videos,
        migrated.already,
        left.len()
    ));
    Ok(Migrated { left, ..migrated })
}

/// Moves the queue of the uploads into the database: see the module. What
/// fails is told in `left`, the file stays.
async fn move_queue(store: &dyn Store, db: &Database, left: &mut Vec<NotRead>) {
    if let Err(e) = try_move_queue(store, db).await {
        left.push(NotRead {
            folder: layout::UPLOAD_QUEUE.to_string(),
            reason: e.to_string(),
        });
    }
}

async fn try_move_queue(store: &dyn Store, db: &Database) -> Result<(), AppError> {
    let Some(saved) = store.read(layout::UPLOAD_QUEUE).await? else {
        return Ok(());
    };
    let queue: UploadQueue = from_slice(&saved).map_err(|e| AppError::IO(IOError::from(e)))?;
    let found = db.read(|tx| {
        queue
            .pending()
            .iter()
            .map(|p| {
                already(
                    tx,
                    &LegacyId {
                        id: p.match_id.clone(),
                        team: p.team_id,
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()
    })?;
    let mut moving = Vec::new();
    let mut staying = Vec::new();
    for (pending, found) in queue.pending().iter().zip(found) {
        match found {
            Some((id, false)) => moving.push((id, pending)),
            Some((_, true)) => log_info(&format!(
                "upload of match {} dropped: removed",
                pending.match_id
            )),
            None => match store
                .exists(&layout::match_descriptor(
                    &pending.team_id,
                    &pending.match_id,
                ))
                .await?
            {
                true => staying.push(pending.clone()),
                false => log_info(&format!(
                    "upload of match {} dropped: not found",
                    pending.match_id
                )),
            },
        }
    }
    db.write(|tx| {
        moving.iter().try_for_each(|(id, p)| {
            tx.execute(
                "INSERT INTO upload_queue (match_id, position, queued_at, retry_count)
                 VALUES (?1, (SELECT coalesce(max(position) + 1, 0) FROM upload_queue), ?2, ?3)
                 ON CONFLICT (match_id) DO NOTHING",
                params![Sql(*id), p.queued_at, p.retry_count],
            )
            .map(|_| ())
            .map_err(sqlite)
        })
    })?;
    match (staying.is_empty(), staying.len() == queue.pending().len()) {
        (true, _) => store.remove(layout::UPLOAD_QUEUE).await,
        // nothing moved: the file as it is
        (false, true) => Ok(()),
        (false, false) => write_json(store, layout::UPLOAD_QUEUE, &UploadQueue::of(staying)).await,
    }
}

/// Removes the index of the matches of the files, if any: derived from
/// the folders, which changed.
async fn remove_index(store: &dyn Store, left: &mut Vec<NotRead>) {
    if let Err(e) = store.remove(layout::MATCH_INDEX).await {
        left.push(NotRead {
            folder: layout::MATCH_INDEX.to_string(),
            reason: e.to_string(),
        });
    }
}

/// Whether the team, or one of its players, isn't in the database yet.
fn missing(tx: &Transaction, team: &TeamRows) -> Result<bool, AppError> {
    let exists = |sql: &str, params: &[&dyn rusqlite::ToSql]| -> Result<bool, AppError> {
        tx.query_row(sql, params, |row| row.get(0)).map_err(sqlite)
    };
    let team_there = exists(
        "SELECT EXISTS (SELECT 1 FROM teams WHERE id = ?1)",
        &[&Sql(team.row.id)],
    )?;
    team.members.iter().try_fold(!team_there, |any, m| {
        exists(
            "SELECT EXISTS (SELECT 1 FROM team_members WHERE team_id = ?1 AND person_id = ?2)",
            &[&Sql(m.team_id), &Sql(m.person_id)],
        )
        .map(|there| any || !there)
    })
}

/// The team as the folder has it, where the database doesn't have it: the
/// team, its persons, its memberships (after those there, in the order of
/// the folder). What's there stays as it is. The cache of its matches
/// holds: players added don't change how they replay.
pub(crate) fn insert_team_rows(tx: &Transaction, team: &TeamRows) -> Result<(), AppError> {
    let t = &team.row;
    tx.execute(
        "INSERT INTO teams (id, name, year, classification, gender, country)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT (id) DO NOTHING",
        params![
            Sql(t.id),
            t.name,
            t.year,
            t.classification,
            t.gender,
            t.country
        ],
    )
    .map_err(sqlite)?;
    team.persons.iter().try_for_each(|p| {
        tx.execute(
            "INSERT INTO persons (id, name, birth_date, height_cm, notes)
             VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT (id) DO NOTHING",
            params![
                Sql(p.id),
                p.name,
                p.birth_date.map(Sql),
                p.height_cm,
                p.notes
            ],
        )
        .map(|_| ())
        .map_err(sqlite)
    })?;
    let mut members: Vec<_> = team.members.iter().collect();
    members.sort_by_key(|m| m.position);
    members.iter().try_for_each(|m| {
        tx.execute(
            "INSERT INTO team_members (team_id, person_id, position, number, role, deleted)
             VALUES (?1, ?2,
                     (SELECT coalesce(max(position) + 1, 0) FROM team_members WHERE team_id = ?1),
                     ?3, ?4, ?5)
             ON CONFLICT (team_id, person_id) DO NOTHING",
            params![
                Sql(m.team_id),
                Sql(m.person_id),
                m.number,
                m.role,
                m.deleted
            ],
        )
        .map(|_| ())
        .map_err(sqlite)
    })
}

/// What's to do with the match and its video: by where it was, whether
/// the database has them, and the same.
fn plan(
    tx: &Transaction,
    rows: MatchRows,
    video: Option<&VideoProfile>,
) -> Result<Result<Plan, NotRead>, AppError> {
    let folder = rows
        .row
        .legacy
        .as_ref()
        .map_or_else(String::new, |l| layout::match_folder(&l.team, &l.id));
    let not_read = |e: AppError| NotRead {
        folder: folder.clone(),
        reason: e.to_string(),
    };
    let new = |rows: MatchRows| match video.map(|v| video_rows(v, &rows)).transpose() {
        Ok(video) => Ok(Plan::New {
            rows: Box::new(rows),
            video,
        }),
        Err(e) => Err(not_read(e)),
    };
    let Some(legacy) = rows.row.legacy.clone() else {
        return Ok(new(rows));
    };
    match already(tx, &legacy)? {
        None => Ok(new(rows)),
        // removed in the database: not brought back, nor its video
        Some((_, true)) => Ok(Ok(Plan::Already { video: None })),
        Some((id, false)) => {
            let Some(there) = match_rows(tx, id)? else {
                return Ok(Err(not_read(not_found(&format!("match {id}")))));
            };
            let video = video.map(|v| video_rows(v, &there)).transpose();
            Ok(match (same(&rows, &there), video) {
                (Err(e), _) | (_, Err(e)) => Err(not_read(e)),
                (Ok(false), _) => Ok(Plan::Differs),
                (Ok(true), Ok(None)) => Ok(Plan::Already { video: None }),
                (Ok(true), Ok(Some(video))) => match video::video_rows(tx, id)? {
                    None => Ok(Plan::Already { video: Some(video) }),
                    Some(saved) if saved == video => Ok(Plan::Already { video: None }),
                    Some(_) => Ok(Plan::Differs),
                },
            })
        }
    }
}

/// The match moved from where it was, and whether it's removed.
pub(crate) fn already(
    tx: &Transaction,
    legacy: &LegacyId,
) -> Result<Option<(Uuid, bool)>, AppError> {
    tx.query_row(
        "SELECT id, deleted FROM matches WHERE legacy_id = ?1 AND legacy_team = ?2",
        params![legacy.id, Sql(legacy.team)],
        |row| Ok((row.get::<_, Sql<Uuid>>(0)?.0, row.get(1)?)),
    )
    .optional()
    .map_err(sqlite)
}

/// Whether the match of the folder is the one in the database: the same
/// match (but its id), roster and sets (but the ids of the events).
pub(crate) fn same(folder: &MatchRows, there: &MatchRows) -> Result<bool, AppError> {
    let entries = |rows: &MatchRows| -> Result<Vec<SetEntry>, AppError> {
        rows.sets.iter().map(SetRows::entry).collect()
    };
    let roster = |rows: &MatchRows| {
        let mut roster: Vec<_> = rows
            .roster
            .iter()
            .map(|r| (r.team_id, r.person_id, r.number, r.role))
            .collect();
        roster.sort_by_key(|(team, person, number, _)| (*team, *person, *number));
        roster
    };
    let row = Match {
        id: there.row.id,
        ..folder.row.clone()
    };
    Ok(row == there.row && roster(folder) == roster(there) && entries(folder)? == entries(there)?)
}

/// The match written, then read back: the same rows, or nothing is written.
fn insert_checked(tx: &Transaction, rows: &MatchRows) -> Result<(), AppError> {
    insert_match(tx, rows)?;
    let sorted = |rows: &MatchRows| {
        let mut rows = rows.clone();
        rows.roster.sort_by_key(|r| (r.team_id, r.number));
        rows
    };
    match match_rows(tx, rows.row.id)? {
        Some(back) if sorted(&back) == sorted(rows) => summary::refresh(tx, rows.row.id),
        _ => Err(AppError::IO(IOError::Msg(format!(
            "match {}: not read back as written",
            rows.row.id
        )))),
    }
}

/// The video written, then read back: the same rows, or nothing is
/// written.
fn write_video_checked(tx: &Transaction, rows: &VideoRows) -> Result<(), AppError> {
    write_video(tx, rows)?;
    match video::video_rows(tx, rows.row.match_id)? {
        Some(back) if back == *rows => Ok(()),
        _ => Err(AppError::IO(IOError::Msg(format!(
            "video of match {}: not read back as written",
            rows.row.match_id
        )))),
    }
}

/// Whether a blob of a match folder is moved to the database: its video
/// only if `video_moves`.
fn moved(name: &str, video_moves: bool) -> bool {
    name == MATCH_DESCRIPTOR_FILE_NAME
        || (video_moves && name == VIDEO_PROFILE_FILE_NAME)
        || matches!(set_file(name), Some((Some(_), "json" | "csv")))
}

/// What in the folder of a match isn't moved to the database: told as why
/// the folder stays, `None` when there's nothing.
async fn kept_files(
    store: &dyn Store,
    folder: &str,
    video_moves: bool,
    labels: &Labels,
) -> Result<Option<NotRead>, NotRead> {
    let not_read = |reason: String| NotRead {
        folder: folder.to_string(),
        reason,
    };
    let entries = store
        .list(folder)
        .await
        .map_err(|e| not_read(e.to_string()))?;
    let kept: Vec<String> = entries
        .into_iter()
        .filter_map(|entry| match entry {
            StoreEntry::Blob(name) if moved(&name, video_moves) => None,
            StoreEntry::Blob(name) | StoreEntry::Folder(name) => Some(name),
        })
        .collect();
    Ok(match kept.is_empty() {
        true => None,
        false => Some(not_read(
            labels.migration_files_kept.replace("{}", &kept.join(", ")),
        )),
    })
}

/// Removes the folder of a match moved to the database, unless it holds
/// what isn't moved (looked at again: it may have changed meanwhile).
async fn remove_match_folder(
    store: &dyn Store,
    folder: &str,
    video_moves: bool,
    labels: &Labels,
) -> Result<(), NotRead> {
    match kept_files(store, folder, video_moves, labels).await? {
        Some(kept) => Err(kept),
        None => store.remove_folder(folder).await.map_err(|e| NotRead {
            folder: folder.to_string(),
            reason: e.to_string(),
        }),
    }
}

/// The folders of the teams read that would hold nothing but their
/// `team.json` once the folders `going` are removed.
async fn teams_to_remove(store: &dyn Store, teams: &[LegacyTeam], going: &[&str]) -> Vec<String> {
    let mut empty = Vec::new();
    for team in teams {
        let folder = layout::team_folder(&team.team.id);
        let Ok(entries) = store.list(&folder).await else {
            continue;
        };
        let holds = entries.iter().any(|entry| match entry {
            StoreEntry::Blob(name) => name != TEAM_DESCRIPTOR_FILE_NAME,
            StoreEntry::Folder(name) => !going.contains(&layout::child(&folder, name).as_str()),
        });
        if !holds {
            empty.push(folder);
        }
    }
    empty
}

/// The whole data folder in a zip at a free name of `backup`, read back
/// before it's there: its path.
async fn take_backup(store: &dyn Store, backup: &Backup) -> Result<PathBuf, AppError> {
    let blobs = all_blobs(store).await?;
    let zip = zip_blobs(&blobs)?;
    let path = free_path(backup, Local::now().date_naive());
    let part = path.with_extension("zip.part");
    let io = |e: std::io::Error| AppError::IO(IOError::from(e));
    std::fs::File::create(&part)
        .and_then(|mut file| {
            file.write_all(&zip)?;
            file.sync_all()
        })
        .map_err(io)?;
    let written = std::fs::read(&part).map_err(io)?;
    match unzip(&written)? == blobs {
        true => std::fs::rename(&part, &path).map_err(io).map(|()| path),
        false => {
            let _ = std::fs::remove_file(&part);
            Err(AppError::IO(IOError::Msg(format!(
                "backup {} not read back as written",
                part.display()
            ))))
        }
    }
}

/// `<dir>/<prefix>-backup-<date>.zip`, or with `-2`, `-3`… if taken.
fn free_path(backup: &Backup, date: NaiveDate) -> PathBuf {
    let name = |n: u32| match n {
        1 => format!("{}-backup-{date}.zip", backup.prefix),
        n => format!("{}-backup-{date}-{n}.zip", backup.prefix),
    };
    (1..)
        .map(|n| backup.dir.join(name(n)))
        .find(|path| !path.exists() && !path.with_extension("zip.part").exists())
        .unwrap_or_else(|| backup.dir.join(name(0)))
}

/// Every blob of the data folder, by key: the hidden ones aren't data,
/// the database isn't what's moved (nor can it be read whole while open),
/// a folder of the root that isn't a team isn't of the app (the
/// recordings of the desktop, when it keeps them there).
async fn all_blobs(store: &dyn Store) -> Result<Vec<(String, Vec<u8>)>, AppError> {
    let mut blobs = Vec::new();
    let mut folders = team_folders(store).await?;
    for entry in store.list(layout::ROOT).await? {
        if let StoreEntry::Blob(name) = entry
            && !layout::is_database_file(&name)
        {
            let data = store.read(&name).await?.ok_or_else(|| {
                AppError::IO(IOError::Msg(format!("{name}: gone while backed up")))
            })?;
            blobs.push((name, data));
        }
    }
    while let Some(folder) = folders.pop() {
        for entry in store.list(&folder).await? {
            match entry {
                StoreEntry::Folder(name) => folders.push(child(&folder, &name)),
                StoreEntry::Blob(name) => {
                    let key = child(&folder, &name);
                    let data = store.read(&key).await?.ok_or_else(|| {
                        AppError::IO(IOError::Msg(format!("{key}: gone while backed up")))
                    })?;
                    blobs.push((key, data));
                }
            }
        }
    }
    blobs.sort();
    Ok(blobs)
}

fn child(folder: &str, name: &str) -> String {
    match folder {
        "" => name.to_string(),
        _ => layout::child(folder, name),
    }
}

fn zip_blobs(blobs: &[(String, Vec<u8>)]) -> Result<Vec<u8>, AppError> {
    let zip_error = |e: zip::result::ZipError| AppError::IO(IOError::from(e));
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (key, data) in blobs {
        zip.start_file::<_, ()>(
            key.as_str(),
            FileOptions::default()
                .compression_method(CompressionMethod::Deflated)
                .large_file(data.len() >= u32::MAX as usize),
        )
        .map_err(zip_error)?;
        zip.write_all(data)
            .map_err(|e| AppError::IO(IOError::from(e)))?;
    }
    zip.finish().map(Cursor::into_inner).map_err(zip_error)
}

/// The files of a zip, by name.
pub(crate) fn unzip(zip: &[u8]) -> Result<Vec<(String, Vec<u8>)>, AppError> {
    let zip_error = |e: zip::result::ZipError| AppError::IO(IOError::from(e));
    let mut archive = ZipArchive::new(Cursor::new(zip)).map_err(zip_error)?;
    let mut files = Vec::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(zip_error)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        let name = file.name().map_err(zip_error)?.to_string();
        files.push((name, data));
    }
    files.sort();
    Ok(files)
}
