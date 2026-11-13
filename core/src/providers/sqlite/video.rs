//! The profiles of the videos of the matches in the database (TODO/001,
//! step 15): `match_videos`, `match_video_files`,
//! `match_video_corrections`, turned into the profile of today by
//! `VideoRows`, from A's side.
//!
//! - The video is of this device (its files are paths of this computer):
//!   the profile is replaced as a whole when saved, as the roster of a
//!   match, and never synced or exported.
//! - Its events are referred to by their ids: an event corrected in its
//!   row (`SetWriter::update`) keeps its correction; an event undone, or
//!   of a set removed, no longer has one (with the files, the same index
//!   pointed to the event after it).
//! - A removed match, or one of another team A, has no video to read or
//!   save.

use crate::{
    domain::{
        MatchVideo, MatchVideoFile, VideoAnchor, VideoCorrection,
        adapters::{EventsOfA, VideoRows},
        sql::Sql,
    },
    errors::{AppError, IOError},
    providers::sqlite::{Database, matches::match_uuid, not_found, rows::set_rows, sqlite},
    video::{StoredProfile, VideoProfile, VideoProfiles},
};
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, Utc};
use rusqlite::{OptionalExtension, Transaction, params};
use uuid::Uuid;

pub struct SqliteVideoProfiles(pub Database);

#[async_trait]
impl VideoProfiles for SqliteVideoProfiles {
    async fn read(&self, team_id: &Uuid, match_id: &str) -> Result<StoredProfile, AppError> {
        self.0.read(|tx| {
            let id = match_of(tx, team_id, match_id)?;
            let events = EventsOfA::of(&set_rows(tx, id)?);
            Ok(match video_rows(tx, id)? {
                Some(rows) => StoredProfile::Profile(rows.profile(&events)),
                None => StoredProfile::Missing,
            })
        })
    }

    async fn write(
        &self,
        team_id: &Uuid,
        match_id: &str,
        profile: &VideoProfile,
    ) -> Result<(), AppError> {
        self.0.write(|tx| {
            let id = match_of(tx, team_id, match_id)?;
            let events = EventsOfA::of(&set_rows(tx, id)?);
            write_video(tx, &VideoRows::of(profile, id, &events)?)
        })
    }
}

/// The id of the match of team A `team_id`, not removed.
fn match_of(tx: &Transaction, team_id: &Uuid, match_id: &str) -> Result<Uuid, AppError> {
    let id = match_uuid(match_id)?;
    tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM matches WHERE id = ?1 AND team_a = ?2 AND deleted = 0)",
        params![Sql(id), Sql(*team_id)],
        |row| row.get::<_, bool>(0),
    )
    .map_err(sqlite)
    .and_then(|found| match found {
        true => Ok(id),
        false => Err(not_found(&format!("match {match_id} of team {team_id}"))),
    })
}

/// The video of the match, if any: the files in their order, the
/// corrections in the order of the ids of their events.
pub(crate) fn video_rows(tx: &Transaction, match_id: Uuid) -> Result<Option<VideoRows>, AppError> {
    type AnchorColumns = (Option<Sql<DateTime<Utc>>>, Option<i64>, Option<Sql<Uuid>>);
    let anchor: Option<AnchorColumns> = tx
        .query_row(
            "SELECT anchor_at, anchor_offset_ms, anchor_event FROM match_videos
             WHERE match_id = ?1",
            [Sql(match_id)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(sqlite)?;
    let Some(anchor) = anchor else {
        return Ok(None);
    };
    let anchor = match anchor {
        (Some(at), Some(offset_ms), event) => Some(VideoAnchor {
            at: at.0,
            offset_ms,
            event: event.map(|e| e.0),
        }),
        (None, None, None) => None,
        (at, offset, event) => {
            return Err(AppError::IO(IOError::Msg(format!(
                "video of match {match_id}: anchor not valid: {:?}, {offset:?}, {:?}",
                at.map(|a| a.0),
                event.map(|e| e.0)
            ))));
        }
    };
    let files = tx
        .prepare(
            "SELECT position, path, start FROM match_video_files
             WHERE match_id = ?1 ORDER BY position",
        )
        .and_then(|mut statement| {
            statement
                .query_map([Sql(match_id)], |r| {
                    Ok(MatchVideoFile {
                        match_id,
                        position: r.get(0)?,
                        path: r.get(1)?,
                        start: r
                            .get::<_, Option<Sql<DateTime<FixedOffset>>>>(2)?
                            .map(|s| s.0),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(sqlite)?;
    let corrections = tx
        .prepare(
            "SELECT event_id, by_ms FROM match_video_corrections
             WHERE match_id = ?1 ORDER BY event_id",
        )
        .and_then(|mut statement| {
            statement
                .query_map([Sql(match_id)], |r| {
                    Ok(VideoCorrection {
                        match_id,
                        event_id: r.get::<_, Sql<Uuid>>(0)?.0,
                        by_ms: r.get(1)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()
        })
        .map_err(sqlite)?;
    Ok(Some(VideoRows {
        row: MatchVideo { match_id, anchor },
        files,
        corrections,
    }))
}

/// No video for the match: of this device, it goes with the match.
pub(crate) fn forget_video(tx: &Transaction, match_id: Uuid) -> Result<(), AppError> {
    let id = Sql(match_id);
    tx.execute(
        "DELETE FROM match_video_corrections WHERE match_id = ?1",
        [id],
    )
    .and_then(|_| tx.execute("DELETE FROM match_video_files WHERE match_id = ?1", [id]))
    .and_then(|_| tx.execute("DELETE FROM match_videos WHERE match_id = ?1", [id]))
    .map(|_| ())
    .map_err(sqlite)
}

/// The video of the match, in place of the one it had.
pub(crate) fn write_video(tx: &Transaction, video: &VideoRows) -> Result<(), AppError> {
    let id = Sql(video.row.match_id);
    tx.execute(
        "DELETE FROM match_video_corrections WHERE match_id = ?1",
        [id],
    )
    .and_then(|_| tx.execute("DELETE FROM match_video_files WHERE match_id = ?1", [id]))
    .map_err(sqlite)?;
    let anchor = video.row.anchor;
    tx.execute(
        "INSERT INTO match_videos (match_id, anchor_at, anchor_offset_ms, anchor_event)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (match_id) DO UPDATE SET anchor_at = excluded.anchor_at,
             anchor_offset_ms = excluded.anchor_offset_ms,
             anchor_event = excluded.anchor_event",
        params![
            id,
            anchor.map(|a| Sql(a.at)),
            anchor.map(|a| a.offset_ms),
            anchor.and_then(|a| a.event).map(Sql),
        ],
    )
    .map_err(sqlite)?;
    video.files.iter().try_for_each(|f| {
        tx.execute(
            "INSERT INTO match_video_files (match_id, position, path, start)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, f.position, f.path, f.start.map(Sql)],
        )
        .map(|_| ())
        .map_err(sqlite)
    })?;
    video.corrections.iter().try_for_each(|c| {
        tx.execute(
            "INSERT INTO match_video_corrections (match_id, event_id, by_ms)
             VALUES (?1, ?2, ?3)",
            params![id, Sql(c.event_id), c.by_ms],
        )
        .map(|_| ())
        .map_err(sqlite)
    })
}
