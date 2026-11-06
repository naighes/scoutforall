//! The video of a match: its files, played one after the other as a single
//! video, and where each event is in it.
//!
//! The profile is saved in `video.json`, next to the other files of the
//! match: the older versions don't read it (they read `set_<n>.*` only), and
//! the exports leave it out, since its paths are of this computer.
//!
//! Where an event is: `offset = event.timestamp − anchor.at +
//! anchor.offset_ms`. The anchor is chosen by the user (an event and the
//! moment of the video it happens at) or found by itself:
//! - a match imported from DataVolley with the scout linked to its video:
//!   the times of the events are the seconds in the video from midnight
//!   (`MatchEntry::video_clock`);
//! - otherwise, the start of the first file, from its name (e.g.
//!   `20261101 175252.mp4`): the events scouted here have the time of the
//!   clock.

use crate::{
    errors::{AppError, IOError},
    providers::store::{read_json, write_json},
    shapes::r#match::MatchEntry,
    storage::{Store, layout},
};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// The version of `video.json` this version writes and reads: a newer one
/// is left as it is.
pub const VIDEO_PROFILE_VERSION: u8 = 1;

/// The name of the profile, in the folder of its match.
pub const VIDEO_PROFILE_FILE_NAME: &str = "video.json";

/// The files of the video of a match and how its events line up with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoProfile {
    pub version: u8,
    /// In the order they're played.
    pub files: Vec<VideoFile>,
    /// Chosen by the user; none: found by itself (`anchor`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
}

impl Default for VideoProfile {
    fn default() -> Self {
        VideoProfile {
            version: VIDEO_PROFILE_VERSION,
            files: vec![],
            anchor: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoFile {
    /// Relative to the folder of the data when in it (`stored_path`), so
    /// that the folder can be moved; absolute otherwise.
    pub path: String,
    /// When it starts: from its name or given by the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<DateTime<FixedOffset>>,
}

/// The time of an event and the moment of the video it happens at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub at: DateTime<Utc>,
    /// From the start of the first file, in milliseconds.
    pub offset_ms: i64,
    /// The event chosen by the user, if any: the time is its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<EventRef>,
}

/// An event of the match: the set and its index among the events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventRef {
    pub set_number: u8,
    pub index: usize,
}

/// What the folder of a match has.
#[derive(Debug, Clone, PartialEq)]
pub enum StoredProfile {
    Missing,
    /// Written by a newer version: not read, and not to overwrite.
    Newer(u8),
    Profile(VideoProfile),
}

#[derive(Deserialize)]
struct Versioned {
    version: u8,
}

pub fn profile_key(team_id: &Uuid, match_id: &str) -> String {
    layout::child(
        &layout::match_folder(team_id, match_id),
        VIDEO_PROFILE_FILE_NAME,
    )
}

/// The profile of the match.
pub async fn read_profile(
    store: &dyn Store,
    team_id: &Uuid,
    match_id: &str,
) -> Result<StoredProfile, AppError> {
    let key = profile_key(team_id, match_id);
    match store.exists(&key).await? {
        false => Ok(StoredProfile::Missing),
        true => match read_json::<Versioned>(store, &key).await?.version {
            v if v > VIDEO_PROFILE_VERSION => Ok(StoredProfile::Newer(v)),
            _ => read_json(store, &key).await.map(StoredProfile::Profile),
        },
    }
}

/// Saves the profile of the match, unless a newer version wrote it.
pub async fn write_profile(
    store: &dyn Store,
    team_id: &Uuid,
    match_id: &str,
    profile: &VideoProfile,
) -> Result<(), AppError> {
    match read_profile(store, team_id, match_id).await? {
        StoredProfile::Newer(v) => Err(AppError::IO(IOError::Msg(format!(
            "{} written by a newer version ({v})",
            VIDEO_PROFILE_FILE_NAME
        )))),
        _ => {
            let saved = VideoProfile {
                version: VIDEO_PROFILE_VERSION,
                ..profile.clone()
            };
            write_json(store, &profile_key(team_id, match_id), &saved).await
        }
    }
}

/// When the file starts, from its name: the date and the time, as
/// `YYYYMMDD HHMMSS` (e.g. `20261101 175252.mp4`, `VID_20261101_175252.mp4`),
/// in the time zone of `offset` (the cameras write the local time).
pub fn start_from_name(path: &Path, offset: FixedOffset) -> Option<DateTime<FixedOffset>> {
    let stem = path.file_stem()?.to_str()?;
    let pattern = Regex::new(r"(?:^|\D)(\d{8})[ _\-T]?(\d{6})(?:\D|$)").ok()?;
    pattern
        .captures_iter(stem)
        .find_map(|c| local_time(c.get(1)?.as_str(), c.get(2)?.as_str(), offset))
}

fn local_time(date: &str, time: &str, offset: FixedOffset) -> Option<DateTime<FixedOffset>> {
    let date = NaiveDate::parse_from_str(date, "%Y%m%d").ok()?;
    let time = NaiveTime::parse_from_str(time, "%H%M%S").ok()?;
    offset
        .from_local_datetime(&NaiveDateTime::new(date, time))
        .single()
}

/// The files in the order to play them: by start, the ones without it
/// after, in the order given.
pub fn in_order(mut files: Vec<VideoFile>) -> Vec<VideoFile> {
    files.sort_by_key(|f| (f.start.is_none(), f.start));
    files
}

/// Where the anchor comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSource {
    /// the user's
    Chosen,
    /// the times of the events are the seconds in the video (DataVolley)
    VideoClock,
    /// the start of the first file, from its name
    FileStart(DateTime<FixedOffset>),
}

/// Where the anchor the events line up with comes from, if any: the user's,
/// or found by itself (see the module).
pub fn anchor_source(profile: &VideoProfile, m: &MatchEntry) -> Option<AnchorSource> {
    match (&profile.anchor, m.video_clock, profile.files.first()) {
        (Some(_), _, _) => Some(AnchorSource::Chosen),
        (None, true, _) => Some(AnchorSource::VideoClock),
        (None, false, Some(VideoFile { start: Some(s), .. })) => Some(AnchorSource::FileStart(*s)),
        (None, false, _) => None,
    }
}

/// The anchor the events line up with, if any (`anchor_source`).
pub fn anchor(profile: &VideoProfile, m: &MatchEntry) -> Option<Anchor> {
    let found = |at: DateTime<Utc>| Anchor {
        at,
        offset_ms: 0,
        event: None,
    };
    match anchor_source(profile, m)? {
        AnchorSource::Chosen => profile.anchor.clone(),
        AnchorSource::VideoClock => Some(found(video_midnight(m))),
        AnchorSource::FileStart(start) => Some(found(start.to_utc())),
    }
}

/// The time of the start of the video for a match on the video clock: the
/// midnight its import counts the seconds from (`dvw::translate`).
pub fn video_midnight(m: &MatchEntry) -> DateTime<Utc> {
    m.date.date_naive().and_time(NaiveTime::MIN).and_utc()
}

/// Where the event of the time is in the video, in milliseconds from the
/// start of the first file: before it when negative.
pub fn offset_ms(anchor: &Anchor, timestamp: DateTime<Utc>) -> i64 {
    (timestamp - anchor.at).num_milliseconds() + anchor.offset_ms
}

/// Where the event of the time is in the video (the files one after the
/// other, each `durations_ms` long), as `anchor_source` lines it up; none
/// when not lined up. From the starts of the files, each event in the file
/// recording at its time: the pauses between the files don't count, a time
/// in one of them at the start of the next file. Otherwise from the anchor,
/// as a single video.
pub fn place(
    profile: &VideoProfile,
    m: &MatchEntry,
    durations_ms: &[u64],
    timestamp: DateTime<Utc>,
) -> Option<i64> {
    let starts: Option<Vec<DateTime<Utc>>> = profile
        .files
        .iter()
        .map(|f| f.start.map(|s| s.to_utc()))
        .collect();
    match (anchor_source(profile, m)?, starts) {
        (AnchorSource::FileStart(_), Some(starts)) if starts.len() == durations_ms.len() => {
            Some(by_file(&starts, durations_ms, timestamp))
        }
        _ => anchor(profile, m).map(|a| offset_ms(&a, timestamp)),
    }
}

// the time in the file recording at it: before the first, before the
// video; past the last, past its end
fn by_file(starts: &[DateTime<Utc>], durations_ms: &[u64], timestamp: DateTime<Utc>) -> i64 {
    let ms = |d: u64| i64::try_from(d).unwrap_or(i64::MAX);
    let mut before = 0i64;
    let mut last = None;
    for (start, duration) in starts.iter().zip(durations_ms) {
        let into = (timestamp - *start).num_milliseconds();
        match (into < 0, into <= ms(*duration)) {
            // in a pause (or before the first): the start of this file
            (true, _) if last.is_some() => return before,
            (true, _) => return into,
            (false, true) => return before + into,
            (false, false) => {}
        }
        last = Some((before, into));
        before += ms(*duration);
    }
    last.map_or(0, |(start, into)| start + into)
}

/// The anchor of the user: the event of the time happens at `offset_ms`.
pub fn anchor_on(event: EventRef, timestamp: DateTime<Utc>, offset_ms: i64) -> Anchor {
    Anchor {
        at: timestamp,
        offset_ms,
        event: Some(event),
    }
}

/// The path to save: relative to the folder of the data when in it.
pub fn stored_path(data_folder: &Path, file: &Path) -> String {
    file.strip_prefix(data_folder)
        .unwrap_or(file)
        .to_string_lossy()
        .into_owned()
}

/// The file of a saved path: in the folder of the data unless absolute.
pub fn resolved_path(data_folder: &Path, stored: &str) -> PathBuf {
    match Path::new(stored) {
        p if p.is_absolute() => p.to_path_buf(),
        p => data_folder.join(p),
    }
}
