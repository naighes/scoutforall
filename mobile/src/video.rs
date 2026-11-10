//! The video of a match, for the apps (001): where each event is in it, the
//! event being seen, how the match stands at a moment, the actions picked
//! in the statistics as clips and the pieces of their montage. The apps
//! play the files (the desktop with its own decoder, Android with its
//! player) and ask the review the rest: the logic is the same on both.

use crate::{
    AnalysisPeriod, ClipPick, MobileError, Scout4All, SetEvent, VideoEvent, clips_of, count_of,
    strip::{ClipSpan, clip_spans},
};
use chrono::{FixedOffset, Local, Offset};
use scout4all_core::{
    shapes::{enums::PhaseEnum, r#match::MatchEntry},
    video::{self, Anchor, AnchorSource, EventRef, StoredProfile, VideoFile, VideoProfile},
};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

/// The seconds of a clip before and after its action, at most.
pub const MAX_MARGIN_SECONDS: u32 = 30;

/// Where the margins of the clips are saved, in the folder of the data.
const MARGINS_FILE: &str = "clips.json";

/// The seconds of a clip before its action and after it, and whether the
/// match is shown over the video: saved in `clips.json` (the same for every
/// match, on the desktop as on Android).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct ClipMargins {
    pub before_s: u32,
    pub after_s: u32,
    #[serde(default = "on")]
    pub overlay: bool,
}

fn on() -> bool {
    true
}

impl Default for ClipMargins {
    fn default() -> Self {
        ClipMargins {
            before_s: 5,
            after_s: 5,
            overlay: true,
        }
    }
}

impl ClipMargins {
    /// In their bounds: before 0 to 30 s, after 1 to 30 s.
    pub fn bounded(self) -> ClipMargins {
        ClipMargins {
            before_s: self.before_s.min(MAX_MARGIN_SECONDS),
            after_s: self.after_s.clamp(1, MAX_MARGIN_SECONDS),
            ..self
        }
    }
}

/// A file chosen to add to the video of a match: where it is (a path, or
/// the `content://` of Android) and its name, which may tell when it
/// starts (`20261101 175252.mp4`).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NewVideoFile {
    pub path: String,
    pub name: String,
}

/// A file of the video, in the order played.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct VideoFileRow {
    /// as saved: what the player opens
    pub path: String,
    /// as the user named it, when told; the end of the path otherwise
    pub name: String,
    /// `HH:MM:SS`, when it starts, if known
    pub start: Option<String>,
    /// once read by the player
    pub duration_ms: Option<u64>,
}

/// Where the events are lined up with the video from.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum VideoAlignment {
    /// no way to line them up yet: an event to tie by hand
    Missing,
    /// the seconds in the video of DataVolley
    DataVolley,
    /// the start of the first file, `HH:MM:SS`, from its name
    FileName { start: String },
    /// tied by hand
    Chosen,
}

/// An event of the match beside its video.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct VideoEventRow {
    pub set_number: u8,
    pub event: SetEvent,
    /// where it is in the video, from the start of the first file: none
    /// when not lined up, before the start when negative
    pub offset_ms: Option<i64>,
    /// in the video (not before its start nor after its end)
    pub inside: bool,
    /// the event tied to the video by hand
    pub anchored: bool,
    /// put at its own time by the user: how far from where the anchor puts
    /// it
    pub corrected_ms: Option<i64>,
}

/// How the match stands at a moment of the video: after the last event
/// passed (the score turns when the point is scouted, as on TV after the
/// rally), with the sets won before its set.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct VideoStanding {
    pub set_number: u8,
    /// the sets won before this one
    pub sets_us: u8,
    pub sets_them: u8,
    pub score_us: u8,
    pub score_them: u8,
    /// after the event, for the rally to come: ours, "-" without the
    /// setter on the court
    pub rotation: String,
    pub phase: PhaseEnum,
}

/// A piece of the video of the actions picked: from its margin before to
/// its margin after, the actions overlapping as one (the first one's card,
/// the others counted).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MontagePiece {
    pub from_ms: u64,
    pub to_ms: u64,
    /// the first action in it
    pub event: VideoEventRow,
    /// the score before it, in its set
    pub before_us: u8,
    pub before_them: u8,
    /// which piece, from 1, of how many
    pub number: u32,
    pub count: u32,
    /// the actions in it, 1 at least
    pub actions: u32,
}

/// A stretch of a file: a piece of the video, in the file it's in.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FileSpan {
    /// the file, from 0, in the order played
    pub file: u32,
    /// in the file
    pub from_ms: u64,
    pub to_ms: u64,
}

/// A match of the period with actions picked in the statistics of its
/// team.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct TeamClipMatch {
    pub match_id: String,
    pub opponent: String,
    /// `YYYY-MM-DD`
    pub day: String,
    pub actions: u32,
    /// the match has files of its video: the others can't be watched
    pub has_video: bool,
}

/// The video of a match with its events: the files and how they line up,
/// changed and saved here (`video.json`), and what the player asks. With
/// the actions picked in the statistics, only them.
#[derive(uniffi::Object)]
pub struct VideoReview {
    core: Arc<Scout4All>,
    state: Mutex<Review>,
}

struct Review {
    the_match: MatchEntry,
    /// none: written by a newer version, left as it is
    profile: Option<VideoProfile>,
    /// the names of the files added here, by their path (a `content://`
    /// has no name in it)
    names: Vec<(String, String)>,
    durations: Vec<u64>,
    /// all of them, for how the match stands
    all: Vec<VideoEvent>,
    /// the actions picked, if any
    picked: Option<Vec<EventRef>>,
}

#[uniffi::export]
impl Scout4All {
    /// The video of the match: all its events, or only the ones the pick
    /// keeps (the actions of a statistic).
    pub fn video_review(
        self: Arc<Self>,
        team_id: String,
        match_id: String,
        pick: Option<ClipPick>,
    ) -> Result<Arc<VideoReview>, MobileError> {
        let (the_match, stored) = self.video(&team_id, &match_id)?;
        let profile = match stored {
            StoredProfile::Missing => Some(VideoProfile::default()),
            StoredProfile::Profile(p) => Some(p),
            StoredProfile::Newer(_) => None,
        };
        let picked = pick.map(|p| clips_of(&the_match, &p)).transpose()?;
        let mut review = Review {
            the_match,
            profile,
            names: vec![],
            durations: vec![],
            all: vec![],
            picked,
        };
        review.all = self.video_events(
            &review.the_match,
            &review.current_profile(),
            &review.durations,
        )?;
        Ok(Arc::new(VideoReview {
            core: self,
            state: Mutex::new(review),
        }))
    }

    /// How many actions of the match the pick keeps.
    pub fn match_clip_count(
        &self,
        team_id: String,
        match_id: String,
        pick: ClipPick,
    ) -> Result<u32, MobileError> {
        self.match_clips(&team_id, &match_id, &pick)
            .map(|c| count_of(c.len()))
    }

    /// The matches of the period with actions the pick keeps (every set),
    /// in the order of the analysis.
    pub fn team_clip_matches(
        &self,
        team_id: String,
        period: AnalysisPeriod,
        pick: ClipPick,
    ) -> Result<Vec<TeamClipMatch>, MobileError> {
        Ok(self
            .team_clips(&team_id, &period, &pick)?
            .into_iter()
            .map(|m| TeamClipMatch {
                actions: count_of(m.events.len()),
                match_id: m.match_id,
                opponent: m.opponent,
                day: m.day,
                has_video: m.has_video,
            })
            .collect())
    }

    /// Whether the match has files of its video.
    pub fn has_video(&self, team_id: String, match_id: String) -> Result<bool, MobileError> {
        let (_, stored) = self.video(&team_id, &match_id)?;
        Ok(matches!(stored, StoredProfile::Profile(p) if !p.files.is_empty()))
    }

    /// The seconds of the clips before and after their action, and whether
    /// the match is shown over the video.
    pub fn clip_margins(&self) -> ClipMargins {
        self.runtime
            .block_on(self.store.read(MARGINS_FILE))
            .ok()
            .flatten()
            .and_then(|bytes| serde_json::from_slice::<ClipMargins>(&bytes).ok())
            .map(ClipMargins::bounded)
            .unwrap_or_default()
    }

    /// Saves the margins of the clips, in their bounds: the ones saved.
    pub fn save_clip_margins(&self, margins: ClipMargins) -> Result<ClipMargins, MobileError> {
        let bounded = margins.bounded();
        let failed = |reason: String| MobileError::Failed { reason };
        let json = serde_json::to_vec_pretty(&bounded).map_err(|e| failed(e.to_string()))?;
        self.runtime
            .block_on(self.store.write(MARGINS_FILE, &json))
            .map_err(|e| failed(e.to_string()))?;
        Ok(bounded)
    }
}

#[uniffi::export]
impl VideoReview {
    /// Whether a newer version wrote the profile: shown, not changed.
    pub fn newer(&self) -> bool {
        self.state().profile.is_none()
    }

    pub fn opponent(&self) -> String {
        self.state().the_match.opponent.clone()
    }

    pub fn team(&self) -> String {
        self.state().the_match.team.name.clone()
    }

    /// Whether only the actions picked in the statistics are shown.
    pub fn is_clips(&self) -> bool {
        self.state().picked.is_some()
    }

    pub fn files(&self) -> Vec<VideoFileRow> {
        let st = self.state();
        st.current_profile()
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| VideoFileRow {
                path: f.path.clone(),
                name: st.name_of(&f.path),
                start: f.start.map(|t| t.format("%H:%M:%S").to_string()),
                duration_ms: st.durations.get(i).copied(),
            })
            .collect()
    }

    pub fn alignment(&self) -> VideoAlignment {
        let st = self.state();
        match video::anchor_source(&st.current_profile(), &st.the_match) {
            None => VideoAlignment::Missing,
            Some(AnchorSource::Chosen) => VideoAlignment::Chosen,
            Some(AnchorSource::VideoClock) => VideoAlignment::DataVolley,
            Some(AnchorSource::FileStart(start)) => VideoAlignment::FileName {
                start: start.format("%H:%M:%S").to_string(),
            },
        }
    }

    /// How long each file is, once the player read them: the events placed
    /// in them.
    pub fn set_durations(&self, durations_ms: Vec<u64>) -> Result<(), MobileError> {
        let mut st = self.state();
        st.durations = durations_ms;
        self.reload(&mut st)
    }

    /// The whole video, the files one after the other.
    pub fn total_ms(&self) -> u64 {
        self.state().total()
    }

    /// The events shown: all of them, or the actions picked.
    pub fn events(&self) -> Vec<VideoEventRow> {
        let st = self.state();
        st.shown().map(|e| st.row(e)).collect()
    }

    /// Where the first event of the match is in the video, once lined up:
    /// the video before it (the warm-up) left out.
    pub fn first_ms(&self) -> Option<i64> {
        self.state().all.iter().filter_map(|e| e.offset_ms).min()
    }

    /// The files chosen added, the ones already there skipped, all in the
    /// order of their starts (from their names, in the time zone of the
    /// phone, `utc_offset_s` east of UTC).
    pub fn add_files(
        &self,
        files: Vec<NewVideoFile>,
        utc_offset_s: i32,
    ) -> Result<(), MobileError> {
        let offset =
            FixedOffset::east_opt(utc_offset_s).unwrap_or_else(|| Local::now().offset().fix());
        let mut st = self.state();
        let Some(profile) = st.profile.clone() else {
            return Ok(());
        };
        let fresh: Vec<VideoFile> = files
            .iter()
            .filter(|f| !profile.files.iter().any(|there| there.path == f.path))
            .map(|f| VideoFile {
                path: f.path.clone(),
                start: video::start_from_name(Path::new(&f.name), offset),
            })
            .collect();
        st.names.extend(files.into_iter().map(|f| (f.path, f.name)));
        let files = video::in_order([profile.files.clone(), fresh].concat());
        // the durations read again by the player
        st.durations.clear();
        self.save(&mut st, VideoProfile { files, ..profile })
    }

    /// The file at `index` one place earlier (`earlier`) or later.
    pub fn move_file(&self, index: u32, earlier: bool) -> Result<(), MobileError> {
        let mut st = self.state();
        let Some(profile) = st.profile.clone() else {
            return Ok(());
        };
        let i = index as usize;
        let other = match earlier {
            true => i.checked_sub(1),
            false => Some(i + 1).filter(|j| *j < profile.files.len()),
        };
        let Some(j) = other.filter(|_| i < profile.files.len()) else {
            return Ok(());
        };
        let mut files = profile.files.clone();
        files.swap(i, j);
        if i < st.durations.len() && j < st.durations.len() {
            st.durations.swap(i, j);
        }
        self.save(&mut st, VideoProfile { files, ..profile })
    }

    pub fn remove_file(&self, index: u32) -> Result<(), MobileError> {
        let mut st = self.state();
        let Some(profile) = st.profile.clone() else {
            return Ok(());
        };
        let i = index as usize;
        if i >= profile.files.len() {
            return Ok(());
        }
        let mut files = profile.files.clone();
        files.remove(i);
        if i < st.durations.len() {
            st.durations.remove(i);
        }
        self.save(&mut st, VideoProfile { files, ..profile })
    }

    /// The event shown at `index` happens at `ms` in the video: all of
    /// them lined up with it (its own correction gone).
    pub fn anchor_here(&self, index: u32, ms: u64) -> Result<(), MobileError> {
        self.with_event(index, |profile, e| {
            profile.tied_to(event_ref(e), e.timestamp, millis(ms))
        })
    }

    /// The event shown at `index` alone at `ms` in the video, the others
    /// where they are.
    pub fn correct_here(&self, index: u32, ms: u64) -> Result<(), MobileError> {
        self.with_event(index, |profile, e| match uncorrected(e) {
            Some(placed) => profile.corrected(event_ref(e), millis(ms) - placed),
            None => profile.clone(),
        })
    }

    /// The event shown at `index` back where the anchor puts it.
    pub fn uncorrect(&self, index: u32) -> Result<(), MobileError> {
        self.with_event(index, |profile, e| profile.corrected(event_ref(e), 0))
    }

    /// Whether a time written for the event shown at `index` moves it
    /// alone (the video tied to another event) rather than all of them.
    pub fn time_only_this(&self, index: u32) -> bool {
        let st = self.state();
        let profile = st.current_profile();
        st.shown()
            .nth(index as usize)
            .is_some_and(|e| profile.time_effect(event_ref(e)) == video::TimeEffect::OnlyThis)
    }

    /// The time written of the event at `index` (`m:ss`, `m:ss.s`,
    /// `h:mm:ss` or seconds): the first ties the video, then the event
    /// alone (`VideoProfile::with_time`); false when it isn't a time.
    pub fn anchor_written(&self, index: u32, written: String) -> Result<bool, MobileError> {
        match parse_time(&written) {
            Some(ms) => self
                .with_event(index, |profile, e| {
                    profile.with_time(event_ref(e), e.timestamp, uncorrected(e), millis(ms))
                })
                .map(|()| true),
            None => Ok(false),
        }
    }

    /// The time of the event at `index` to write: where it is, or where the
    /// video is (`position_ms`).
    pub fn written_time(&self, index: u32, position_ms: u64) -> String {
        let st = self.state();
        let at = st
            .shown()
            .nth(index as usize)
            .and_then(|e| e.offset_ms)
            .filter(|ms| *ms >= 0)
            .map_or(position_ms, i64::unsigned_abs);
        written(at)
    }

    /// All the events `ms` later in the video (earlier when negative): the
    /// sync found by itself becomes the user's.
    pub fn shift(&self, ms: i64) -> Result<(), MobileError> {
        let mut st = self.state();
        let current = st
            .profile
            .as_ref()
            .and_then(|p| video::anchor(p, &st.the_match));
        match current {
            Some(anchor) => self.save_anchor(
                &mut st,
                Some(Anchor {
                    offset_ms: anchor.offset_ms + ms,
                    ..anchor
                }),
            ),
            None => Ok(()),
        }
    }

    /// Back to the sync found by itself, no event corrected.
    pub fn automatic(&self) -> Result<(), MobileError> {
        let mut st = self.state();
        match st.profile.clone() {
            Some(profile) => self.save(&mut st, profile.automatic()),
            None => Ok(()),
        }
    }

    /// Where the video goes for the event shown at `index`: `lead_ms` before
    /// it, not before the start; none when not lined up.
    pub fn target(&self, index: u32, lead_ms: i64) -> Option<u64> {
        let st = self.state();
        st.shown()
            .nth(index as usize)
            .and_then(|e| target(e, lead_ms))
    }

    /// The event shown being seen at `position_ms` (`current`).
    pub fn current(&self, position_ms: u64, lead_ms: i64) -> Option<u32> {
        let st = self.state();
        let shown: Vec<&VideoEvent> = st.shown().collect();
        current(&shown, position_ms, lead_ms).map(count_of)
    }

    /// With the actions picked, the one after `index` to play at the end of
    /// its clip: in the video, not at the same time as it (DataVolley gives
    /// the touches of a rally the same second, the clip already seen).
    pub fn next_clip(&self, index: u32, lead_ms: i64) -> Option<u32> {
        let st = self.state();
        let total = Some(st.total()).filter(|t| *t > 0);
        let shown: Vec<&VideoEvent> = st.shown().collect();
        let seen = shown.get(index as usize).and_then(|e| target(e, lead_ms));
        (index as usize + 1..shown.len())
            .find(|i| {
                shown
                    .get(*i)
                    .is_some_and(|e| inside(e.offset_ms, total) && target(e, lead_ms) != seen)
            })
            .map(count_of)
    }

    /// With the actions picked, where their clips are in the video, from
    /// `lead_ms` before each to `after_ms` after it, for the strip under
    /// the bar (`strip`): the one of the action `selected` playing.
    pub fn clip_spans(&self, lead_ms: i64, after_ms: u64, selected: u32) -> Vec<ClipSpan> {
        let st = self.state();
        match st.picked {
            Some(_) => {
                let shown: Vec<&VideoEvent> = st.shown().collect();
                clip_spans(&shown, lead_ms, after_ms, selected as usize)
            }
            None => vec![],
        }
    }

    /// How the match stands at `position_ms`: from all its events, also
    /// with the actions picked.
    pub fn standing(&self, position_ms: u64) -> Option<VideoStanding> {
        let st = self.state();
        standing(&st.all, position_ms)
    }

    /// The pieces of the video of the actions shown: each from `before_s`
    /// before it to `after_s` after it, the ones overlapping as one.
    pub fn pieces(&self, before_s: u32, after_s: u32) -> Vec<MontagePiece> {
        let st = self.state();
        let shown: Vec<&VideoEvent> = st.shown().collect();
        pieces(&shown, &st.all, before_s, after_s)
            .into_iter()
            .map(|p| MontagePiece {
                event: st.row(p.event),
                from_ms: p.from_ms,
                to_ms: p.to_ms,
                before_us: p.before.0,
                before_them: p.before.1,
                number: p.number,
                count: p.count,
                actions: p.actions,
            })
            .collect()
    }

    /// The stretch of the video from `from_ms` to `to_ms` in its files: in
    /// two when it crosses from one to the next.
    pub fn spans(&self, from_ms: u64, to_ms: u64) -> Vec<FileSpan> {
        spans(&self.state().durations, from_ms, to_ms)
    }

    /// Where the video at `ms` is: the file and the time in it (the last
    /// file past the end).
    pub fn locate(&self, ms: u64) -> FileSpan {
        let durations = &self.state().durations;
        let mut before = 0u64;
        for (i, d) in durations.iter().enumerate() {
            if ms < before + d || i + 1 == durations.len() {
                let into = ms.saturating_sub(before).min(*d);
                return FileSpan {
                    file: count_of(i),
                    from_ms: into,
                    to_ms: into,
                };
            }
            before += d;
        }
        FileSpan {
            file: 0,
            from_ms: ms,
            to_ms: ms,
        }
    }
}

impl VideoReview {
    fn state(&self) -> MutexGuard<'_, Review> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The events placed again, as the profile and the durations say.
    fn reload(&self, st: &mut Review) -> Result<(), MobileError> {
        st.all = self
            .core
            .video_events(&st.the_match, &st.current_profile(), &st.durations)?;
        Ok(())
    }

    /// The profile changed for the event shown at `index`, saved.
    fn with_event(
        &self,
        index: u32,
        change: impl FnOnce(&VideoProfile, &VideoEvent) -> VideoProfile,
    ) -> Result<(), MobileError> {
        let mut st = self.state();
        let (Some(profile), Some(e)) =
            (st.profile.clone(), st.shown().nth(index as usize).cloned())
        else {
            return Ok(());
        };
        let changed = change(&profile, &e);
        self.save(&mut st, changed)
    }

    fn save(&self, st: &mut Review, profile: VideoProfile) -> Result<(), MobileError> {
        self.core.save_video(&st.the_match, &profile)?;
        st.profile = Some(profile);
        self.reload(st)
    }

    fn save_anchor(&self, st: &mut Review, anchor: Option<Anchor>) -> Result<(), MobileError> {
        match st.profile.clone() {
            Some(profile) => self.save(st, VideoProfile { anchor, ..profile }),
            None => Ok(()),
        }
    }
}

impl Review {
    fn current_profile(&self) -> VideoProfile {
        self.profile.clone().unwrap_or_default()
    }

    fn total(&self) -> u64 {
        self.durations.iter().sum()
    }

    fn shown(&self) -> impl Iterator<Item = &VideoEvent> {
        self.all.iter().filter(|e| {
            self.picked
                .as_ref()
                .is_none_or(|picked| picked.contains(&event_ref(e)))
        })
    }

    fn anchored(&self) -> Option<EventRef> {
        self.profile
            .as_ref()
            .and_then(|p| p.anchor.as_ref())
            .and_then(|a| a.event)
    }

    fn row(&self, e: &VideoEvent) -> VideoEventRow {
        let total = Some(self.total()).filter(|t| *t > 0);
        VideoEventRow {
            set_number: e.set_number,
            event: e.event.clone(),
            offset_ms: e.offset_ms,
            inside: inside(e.offset_ms, total),
            anchored: self.anchored() == Some(event_ref(e)),
            corrected_ms: e.corrected_ms,
        }
    }

    fn name_of(&self, path: &str) -> String {
        self.names
            .iter()
            .rev()
            .find(|(p, _)| p == path)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| file_name(path))
    }
}

/// The end of a path, or of a `content://` (its last part, decoded).
fn file_name(path: &str) -> String {
    let last = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let decoded = percent_decoded(last);
    // a document of Android: "primary:Movies/20261101 175252.mp4"
    decoded
        .rsplit(['/', ':'])
        .next()
        .unwrap_or(&decoded)
        .to_string()
}

fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes.get(i), hex) {
            (Some(b'%'), Some(b)) => {
                out.push(b);
                i += 3;
            }
            (Some(b), _) => {
                out.push(*b);
                i += 1;
            }
            (None, _) => break,
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The event as an anchor names it.
/// Where the anchor puts the event, without its correction.
pub fn uncorrected(e: &VideoEvent) -> Option<i64> {
    e.offset_ms.map(|ms| ms - e.corrected_ms.unwrap_or(0))
}

fn millis(ms: u64) -> i64 {
    i64::try_from(ms).unwrap_or(i64::MAX)
}

pub fn event_ref(e: &VideoEvent) -> EventRef {
    EventRef {
        set_number: e.set_number,
        index: e.event.index as usize,
    }
}

/// Where the video goes for the event: `lead_ms` before it, not before the
/// start; none when not lined up.
pub fn target(e: &VideoEvent, lead_ms: i64) -> Option<u64> {
    e.offset_ms.map(|ms| (ms - lead_ms).max(0).unsigned_abs())
}

/// The event being seen at `position_ms`: the latest whose target the video
/// passed (the one gone to stays the one seen); the later in the list when
/// two have the same time. None before the first.
pub fn current<E: std::borrow::Borrow<VideoEvent>>(
    events: &[E],
    position_ms: u64,
    lead_ms: i64,
) -> Option<usize> {
    events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            target(e.borrow(), lead_ms)
                .filter(|t| *t <= position_ms)
                .map(|t| (t, i))
        })
        .max()
        .map(|(_, i)| i)
}

/// Whether the event is in the video, `duration_ms` long when known.
pub fn inside(offset_ms: Option<i64>, duration_ms: Option<u64>) -> bool {
    match (offset_ms, duration_ms) {
        (Some(ms), Some(d)) => ms >= 0 && ms.unsigned_abs() <= d,
        (Some(ms), None) => ms >= 0,
        (None, _) => false,
    }
}

/// How the match stands at `position_ms`, from all its events (see
/// `VideoStanding`); none before the first.
pub fn standing(events: &[VideoEvent], position_ms: u64) -> Option<VideoStanding> {
    let at = events.get(current(events, position_ms, 0)?)?;
    // the last event of each set before: who won it
    let sets = events
        .iter()
        .zip(events.iter().skip(1).map(Some).chain([None]))
        .filter(|(e, next)| {
            e.set_number < at.set_number && next.is_none_or(|n| n.set_number != e.set_number)
        })
        .fold((0, 0), |(us, them), (e, _)| {
            match e.event.score_us.cmp(&e.event.score_them) {
                std::cmp::Ordering::Greater => (us + 1, them),
                std::cmp::Ordering::Less => (us, them + 1),
                std::cmp::Ordering::Equal => (us, them),
            }
        });
    Some(VideoStanding {
        set_number: at.set_number,
        sets_us: sets.0,
        sets_them: sets.1,
        score_us: at.event.score_us,
        score_them: at.event.score_them,
        rotation: at.rotation.clone(),
        phase: at.phase,
    })
}

/// A piece of the montage, before it's a record.
pub struct Piece<'a> {
    pub from_ms: u64,
    pub to_ms: u64,
    pub event: &'a VideoEvent,
    /// the score before it, in its set
    pub before: (u8, u8),
    pub number: u32,
    pub count: u32,
    pub actions: u32,
}

/// The pieces of the actions `picked`: each from `before_s` before it to
/// `after_s` after it, the ones overlapping as one; the score before each
/// from `all` the events.
pub fn pieces<'a>(
    picked: &[&'a VideoEvent],
    all: &[VideoEvent],
    before_s: u32,
    after_s: u32,
) -> Vec<Piece<'a>> {
    let (before, after) = (i64::from(before_s) * 1000, u64::from(after_s) * 1000);
    let mut sorted: Vec<(u64, u64, &VideoEvent)> = picked
        .iter()
        .filter_map(|e| {
            Some((
                target(e, before)?,
                e.offset_ms?.max(0).unsigned_abs() + after,
                *e,
            ))
        })
        .collect();
    sorted.sort_by_key(|(from, _, _)| *from);
    let mut merged: Vec<(u64, u64, &VideoEvent, u32)> = vec![];
    for (from, to, e) in sorted {
        match merged.last_mut() {
            Some(last) if from <= last.1 => {
                last.1 = last.1.max(to);
                last.3 += 1;
            }
            _ => merged.push((from, to, e, 1)),
        }
    }
    let count = count_of(merged.len());
    merged
        .into_iter()
        .enumerate()
        .map(|(i, (from_ms, to_ms, event, actions))| Piece {
            from_ms,
            to_ms,
            event,
            before: score_before(all, event),
            number: count_of(i + 1),
            count,
            actions,
        })
        .collect()
}

/// The score before the event: after the one before it in its set.
fn score_before(all: &[VideoEvent], e: &VideoEvent) -> (u8, u8) {
    all.iter()
        .take_while(|other| event_ref(other) != event_ref(e))
        .filter(|other| other.set_number == e.set_number)
        .last()
        .map_or((0, 0), |other| {
            (other.event.score_us, other.event.score_them)
        })
}

/// The stretch from `from_ms` to `to_ms` of the files one after the other,
/// `durations_ms` long, in each file it crosses.
pub fn spans(durations_ms: &[u64], from_ms: u64, to_ms: u64) -> Vec<FileSpan> {
    let mut before = 0u64;
    let mut found = vec![];
    for (i, d) in durations_ms.iter().enumerate() {
        let (start, end) = (before, before + d);
        let (from, to) = (from_ms.max(start), to_ms.min(end));
        if from < to {
            found.push(FileSpan {
                file: count_of(i),
                from_ms: from - start,
                to_ms: to - start,
            });
        }
        before = end;
    }
    found
}

/// The video of the actions picked in the statistics, as asked: Android
/// saves it with the screen, to open it again after the system closed the
/// app in the background.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct SavedClips {
    pub team_id: String,
    pub pick: ClipPick,
    /// what they are, over the video
    pub title: String,
    /// with a video
    pub matches: Vec<TeamClipMatch>,
    /// without a video: left out, said
    pub left_out: Vec<TeamClipMatch>,
}

/// The actions picked written to be saved (empty if they can't be: then
/// nothing is restored).
#[uniffi::export]
pub fn save_clips(clips: SavedClips) -> String {
    serde_json::to_string(&clips).unwrap_or_default()
}

/// The actions picked as `save_clips` wrote them; none when they can't be
/// read (written empty, by another version).
#[uniffi::export]
pub fn restore_clips(saved: &str) -> Option<SavedClips> {
    serde_json::from_str(saved).ok()
}

/// The name of the video of the actions picked, from what they are: the
/// letters, the digits and the dashes, the rest a space ("scout4all" when
/// nothing's left).
#[uniffi::export]
pub fn montage_name(title: &str) -> String {
    let name = title
        .chars()
        .map(|c| match c.is_alphanumeric() || c == ' ' || c == '-' {
            true => c,
            false => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    match name.is_empty() {
        true => "scout4all".to_string(),
        false => name,
    }
}

/// `h:mm:ss`, or `m:ss` under the hour.
#[uniffi::export]
pub fn video_clock(ms: u64) -> String {
    let seconds = ms / 1000;
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match h {
        0 => format!("{m}:{s:02}"),
        h => format!("{h}:{m:02}:{s:02}"),
    }
}

/// The time written in the video: `m:ss`, `h:mm:ss` or seconds, with
/// tenths after a point or a comma; none when it isn't one.
pub fn parse_time(written: &str) -> Option<u64> {
    let written = written.trim().replace(',', ".");
    let parts: Vec<&str> = written.split(':').collect();
    let (whole, seconds) = match parts.split_last()? {
        (seconds, whole) if whole.len() <= 2 => (whole, *seconds),
        _ => return None,
    };
    let seconds: f64 = match seconds {
        s if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.') => {
            s.parse().ok()?
        }
        _ => return None,
    };
    // the seconds under a minute when minutes are written
    let bounded = whole.is_empty() || seconds < 60.0;
    let units = whole
        .iter()
        .map(
            |p| match p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() {
                true => p.parse::<u64>().ok(),
                false => None,
            },
        )
        .collect::<Option<Vec<u64>>>()?;
    let minutes_bounded = match units.as_slice() {
        [_, minutes] => *minutes < 60,
        _ => true,
    };
    let whole_seconds = units.iter().fold(0u64, |total, u| total * 60 + u) * 60;
    match bounded && minutes_bounded && seconds.is_finite() {
        true => Some(whole_seconds * 1000 + (seconds * 1000.0).round() as u64),
        false => None,
    }
}

/// The time to write in the field: with the tenths.
pub fn written(ms: u64) -> String {
    let tenths = ms / 100 % 10;
    format!("{}.{tenths}", video_clock(ms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DetailFilter, DvwChoice, DvwSide, StatsScope};
    use scout4all_core::shapes::{
        country::CountryCode,
        enums::{EventTypeEnum, GenderEnum, TeamClassificationEnum},
    };

    /// The actions picked, saved by Android with the screen, come back as
    /// they were: every filter, the matches with and without a video;
    /// something else, none.
    #[test]
    fn the_clips_saved_come_back() {
        use scout4all_core::shapes::{
            court_zone::{CourtZoneEnum, ServeStartEnum},
            enums::{EvalEnum, RoleEnum},
            flags::ReceptionSchemeEnum,
            technique::SetTypeEnum,
        };
        let filter = DetailFilter {
            phase: Some(PhaseEnum::SideOut),
            rotation: Some(3),
            player: Some("p-18".to_string()),
            role: Some(RoleEnum::OppositeHitter),
            reception: vec![EvalEnum::Perfect, EvalEnum::Positive],
            previous: vec![EvalEnum::Exclamative],
            set_types: vec![SetTypeEnum::Quick],
            scheme: Some(ReceptionSchemeEnum::Four),
            inverted_p1: Some(true),
            late: Some(false),
            close: Some(true),
            serve_start: Some(ServeStartEnum::Z6),
        };
        let clips = SavedClips {
            team_id: "t".to_string(),
            pick: ClipPick {
                scope: StatsScope::Set { number: 2 },
                event: EventTypeEnum::A,
                filter,
                evals: vec![EvalEnum::Perfect],
                zone: Some(4),
                target: Some(CourtZoneEnum::Z43),
            },
            title: "Attack · #".to_string(),
            matches: vec![TeamClipMatch {
                match_id: "m1".to_string(),
                opponent: "Brasile".to_string(),
                day: "2026-11-01".to_string(),
                actions: 7,
                has_video: true,
            }],
            left_out: vec![TeamClipMatch {
                match_id: "m2".to_string(),
                opponent: "Japan".to_string(),
                day: "2026-11-02".to_string(),
                actions: 2,
                has_video: false,
            }],
        };
        assert_eq!(restore_clips(&save_clips(clips.clone())), Some(clips));
        assert_eq!(restore_clips(""), None);
        assert_eq!(restore_clips("{\"team_id\": 3}"), None);
    }

    const THREE_SETS: &[u8] = include_bytes!("../../core/src/tests/fixtures/three_sets.dvw");

    /// The file of the fixture with a time to each line of the scout (it
    /// has none), ten seconds apart: the events at different moments.
    fn timed() -> Vec<u8> {
        let text = String::from_utf8_lossy(THREE_SETS);
        let mut n = 0u32;
        let lines: Vec<String> = text
            .lines()
            .map(
                |line| match line.starts_with('*') || line.starts_with('a') {
                    true if line.split(';').count() > 8 => {
                        let mut fields: Vec<String> = line.split(';').map(str::to_string).collect();
                        let t = 18 * 3600 + n * 10;
                        n += 1;
                        fields[7] = format!("{:02}.{:02}.{:02}", t / 3600, t / 60 % 60, t % 60);
                        fields.join(";")
                    }
                    _ => line.to_string(),
                },
            )
            .collect();
        lines
            .join(
                "
",
            )
            .into_bytes()
    }

    /// The match of the fixture imported, its team and its id.
    fn imported(dir: &tempfile::TempDir) -> (Arc<Scout4All>, String, String) {
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let choice = DvwChoice {
            side: DvwSide::Home,
            day: "2015-01-25".into(),
            max_substitutions: 6,
            team_id: None,
            classification: Some(TeamClassificationEnum::Regional),
            gender: Some(GenderEnum::Women),
            country: CountryCode::known("SI"),
        };
        let done = app.import_dvw(timed(), choice, false).unwrap();
        (app, done.team_id, done.match_id)
    }

    fn file(path: &str) -> NewVideoFile {
        NewVideoFile {
            path: format!("content://docs/document/{path}"),
            name: path.to_string(),
        }
    }

    /// Files added by their `content://`, in the order of the times in
    /// their names, kept in `video.json`; moved and removed.
    #[test]
    fn the_files_of_the_video() {
        let dir = tempfile::tempdir().unwrap();
        let (app, team, m) = imported(&dir);
        let review = app
            .clone()
            .video_review(team.clone(), m.clone(), None)
            .unwrap();
        assert!(!review.newer());
        assert!(review.files().is_empty());
        review
            .add_files(
                vec![file("20150125 185442.mp4"), file("20150125 175252.mp4")],
                3600,
            )
            .unwrap();
        let names: Vec<String> = review.files().into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["20150125 175252.mp4", "20150125 185442.mp4"]);
        assert_eq!(review.files()[0].start.as_deref(), Some("17:52:52"));
        // the same again: skipped
        review
            .add_files(vec![file("20150125 175252.mp4")], 3600)
            .unwrap();
        assert_eq!(review.files().len(), 2);
        assert!(app.has_video(team.clone(), m.clone()).unwrap());
        // read again: the names from the paths
        let again = app
            .clone()
            .video_review(team.clone(), m.clone(), None)
            .unwrap();
        assert_eq!(again.files()[1].name, "20150125 185442.mp4");
        assert_eq!(
            again.files()[1].path,
            "content://docs/document/20150125 185442.mp4"
        );
        again.move_file(1, true).unwrap();
        assert_eq!(again.files()[0].name, "20150125 185442.mp4");
        again.move_file(0, true).unwrap();
        assert_eq!(
            again.files()[0].name,
            "20150125 185442.mp4",
            "no place before"
        );
        again.remove_file(0).unwrap();
        assert_eq!(again.files().len(), 1);
        again.remove_file(5).unwrap();
        assert_eq!(again.files().len(), 1);
    }

    /// Tied by hand, the events line up with the video; shifted, back to
    /// automatic; the event seen as the video goes, how the match stands.
    #[test]
    fn the_events_lined_up_with_the_video() {
        let dir = tempfile::tempdir().unwrap();
        let (app, team, m) = imported(&dir);
        let review = app.clone().video_review(team, m, None).unwrap();
        review.add_files(vec![file("partita.mp4")], 0).unwrap();
        review.set_durations(vec![3 * 3600 * 1000]).unwrap();
        assert_eq!(review.alignment(), VideoAlignment::Missing);
        let events = review.events();
        assert!(events.len() > 20, "{}", events.len());
        assert!(events.iter().all(|e| e.offset_ms.is_none() && !e.inside));
        assert_eq!(review.target(0, 2000), None);
        review.anchor_here(0, 60_000).unwrap();
        assert_eq!(review.alignment(), VideoAlignment::Chosen);
        let events = review.events();
        assert_eq!(events[0].offset_ms, Some(60_000));
        assert!(events[0].anchored && !events[1].anchored);
        assert!(events.iter().all(|e| e.offset_ms.is_some()));
        assert_eq!(review.target(0, 2000), Some(58_000));
        assert!(review.first_ms().is_some_and(|first| first <= 60_000));
        // the first event seen from its target on, not before
        assert_eq!(review.current(57_999, 2000), None);
        let seen = review.current(58_000, 2000).unwrap();
        assert_eq!(review.target(seen, 2000), Some(58_000));
        let last = u32::try_from(events.len() - 1).unwrap();
        let end = u64::try_from(events.last().unwrap().offset_ms.unwrap()).unwrap();
        assert_eq!(review.current(end + 10_000, 2000), Some(last));
        // over the video: the match at its end, the sets before the last
        let standing = review.standing(end + 10_000).unwrap();
        assert_eq!(standing.set_number, 3);
        assert_eq!(standing.sets_us + standing.sets_them, 2);
        let after = events.last().unwrap();
        assert_eq!(
            (standing.score_us, standing.score_them),
            (after.event.score_us, after.event.score_them)
        );
        assert_eq!(review.standing(0), None);
        // shifted half a second later
        review.shift(500).unwrap();
        assert_eq!(review.events()[0].offset_ms, Some(60_500));
        // the time written
        assert!(!review.anchor_written(0, "6:6x".into()).unwrap());
        assert!(review.anchor_written(0, "1:30.5".into()).unwrap());
        assert_eq!(review.events()[0].offset_ms, Some(90_500));
        assert_eq!(review.written_time(0, 0), "1:30.5");
        review.automatic().unwrap();
        assert_eq!(review.alignment(), VideoAlignment::Missing);
    }

    /// The first time written ties the video; then a time written, or the
    /// moment of the video, puts the event alone there; it moves with the
    /// video, comes back where the video puts it, goes with automatic.
    #[test]
    fn an_event_put_at_its_own_time() {
        let dir = tempfile::tempdir().unwrap();
        let (app, team, m) = imported(&dir);
        let review = app.clone().video_review(team, m, None).unwrap();
        review.add_files(vec![file("partita.mp4")], 0).unwrap();
        review.set_durations(vec![3 * 3600 * 1000]).unwrap();
        let at = |i: usize| review.events()[i].offset_ms;
        assert!(!review.time_only_this(1));
        assert!(review.anchor_written(0, "1:00".into()).unwrap());
        assert_eq!(at(0), Some(60_000));
        let placed: Vec<Option<i64>> = review.events().iter().map(|e| e.offset_ms).collect();
        // the video tied to the first: a time for another, it alone
        assert!(review.time_only_this(1) && !review.time_only_this(0));
        assert!(review.anchor_written(1, "1:05".into()).unwrap());
        let events = review.events();
        assert_eq!(events[1].offset_ms, Some(65_000));
        assert_eq!(
            events[1].corrected_ms,
            placed[1].map(|p| 65_000 - p),
            "how far from where the video put it"
        );
        assert_eq!((at(0), at(2)), (placed[0], placed[2]));
        assert!(events[0].anchored && events[0].corrected_ms.is_none());
        // at the moment of the video
        review.correct_here(2, 80_000).unwrap();
        assert_eq!(at(2), Some(80_000));
        // shifted: the ones corrected with the others
        review.shift(500).unwrap();
        assert_eq!((at(0), at(1)), (Some(60_500), Some(65_500)));
        // back where the video puts it
        review.uncorrect(1).unwrap();
        assert_eq!(at(1), placed[1].map(|p| p + 500));
        assert_eq!(review.events()[1].corrected_ms, None);
        // the video tied to a corrected one: its correction gone
        review.anchor_here(2, 90_000).unwrap();
        assert_eq!(at(2), Some(90_000));
        assert!(review.events()[2].anchored && review.events()[2].corrected_ms.is_none());
        review.correct_here(3, 100_000).unwrap();
        review.automatic().unwrap();
        assert!(review.events().iter().all(|e| e.corrected_ms.is_none()));
    }

    /// The actions of a statistic: only them, the next clip not at the
    /// same time, the pieces of their montage, in the files.
    #[test]
    fn the_actions_picked_as_clips() {
        let dir = tempfile::tempdir().unwrap();
        let (app, team, m) = imported(&dir);
        let whole = app
            .clone()
            .video_review(team.clone(), m.clone(), None)
            .unwrap();
        whole
            .add_files(vec![file("a.mp4"), file("b.mp4")], 0)
            .unwrap();
        whole.set_durations(vec![1_800_000, 9_000_000]).unwrap();
        whole.anchor_here(0, 10_000).unwrap();
        let pick = ClipPick {
            scope: StatsScope::Match,
            event: EventTypeEnum::S,
            filter: DetailFilter::default(),
            evals: vec![],
            zone: None,
            target: None,
        };
        let count = app
            .match_clip_count(team.clone(), m.clone(), pick.clone())
            .unwrap();
        let period = AnalysisPeriod::LastMatches { count: 4 };
        let matches = app
            .team_clip_matches(team.clone(), period, pick.clone())
            .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!((matches[0].actions, matches[0].has_video), (count, true));
        let review = app.clone().video_review(team, m, Some(pick)).unwrap();
        review.set_durations(vec![1_800_000, 9_000_000]).unwrap();
        assert!(review.is_clips());
        let events = review.events();
        assert_eq!(u32::try_from(events.len()).unwrap(), count);
        assert!(
            events
                .iter()
                .all(|e| e.event.event.event == EventTypeEnum::S)
        );
        assert!(events.len() > 10);
        let next = review.next_clip(0, 5000).unwrap();
        // their clips on the strip: in order, apart, the one chosen playing
        let spans = review.clip_spans(5000, 5000, next);
        assert!(spans.len() > 5);
        assert!(spans.windows(2).all(|w| w[0].to_ms < w[1].from_ms));
        assert_eq!(spans.iter().filter(|sp| sp.playing).count(), 1, "{spans:?}");
        let playing = spans.iter().find(|sp| sp.playing).unwrap();
        assert_eq!(Some(playing.from_ms), review.target(playing.event, 5000));
        assert!(playing.event <= next);
        // margins wider than the gaps: one clip
        assert_eq!(review.clip_spans(600_000, 600_000, 0).len(), 1);
        assert!(
            whole.clip_spans(5000, 5000, 0).is_empty(),
            "the whole match: no strip"
        );
        assert!(next > 0);
        assert_ne!(review.target(next, 5000), review.target(0, 5000));
        let pieces = review.pieces(5, 5);
        assert!(!pieces.is_empty());
        assert_eq!(pieces.iter().map(|p| p.actions).sum::<u32>(), count);
        assert!(pieces.windows(2).all(|w| w[0].to_ms < w[1].from_ms));
        let first = &pieces[0];
        assert_eq!(
            (first.number, first.count),
            (1, u32::try_from(pieces.len()).unwrap())
        );
        assert!(first.to_ms - first.from_ms >= 10_000);
        // a piece across the two files: in two
        let across = review.spans(1_795_000, 1_805_000);
        assert_eq!(across.len(), 2);
        assert_eq!(
            (across[0].file, across[0].from_ms, across[0].to_ms),
            (0, 1_795_000, 1_800_000)
        );
        assert_eq!(
            (across[1].file, across[1].from_ms, across[1].to_ms),
            (1, 0, 5_000)
        );
        let at = review.locate(1_805_000);
        assert_eq!((at.file, at.from_ms), (1, 5_000));
        assert_eq!(review.locate(99_999_999).file, 1);
    }

    #[test]
    fn the_margins_saved_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let (app, _, _) = imported(&dir);
        assert_eq!(app.clip_margins(), ClipMargins::default());
        let saved = app
            .save_clip_margins(ClipMargins {
                before_s: 8,
                after_s: 0,
                overlay: false,
            })
            .unwrap();
        assert_eq!((saved.before_s, saved.after_s), (8, 1));
        assert_eq!(app.clip_margins(), saved);
        // the desktop writes it as well
        std::fs::write(
            dir.path().join(MARGINS_FILE),
            r#"{"before_s": 90, "after_s": 3}"#,
        )
        .unwrap();
        let read = app.clip_margins();
        assert_eq!(
            (read.before_s, read.after_s, read.overlay),
            (MAX_MARGIN_SECONDS, 3, true)
        );
    }

    #[test]
    fn the_time_in_the_video() {
        assert_eq!(video_clock(0), "0:00");
        assert_eq!(video_clock(378_400), "6:18");
        assert_eq!(video_clock(5_686_000), "1:34:46");
    }

    #[test]
    fn the_times_written() {
        assert_eq!(parse_time("6:18"), Some(378_000));
        assert_eq!(parse_time(" 6:18.5 "), Some(378_500));
        assert_eq!(parse_time("6:18,25"), Some(378_250));
        assert_eq!(parse_time("1:34:46"), Some(5_686_000));
        assert_eq!(parse_time("378"), Some(378_000));
        assert_eq!(parse_time("378.5"), Some(378_500));
        assert_eq!(parse_time("0:00"), Some(0));
        for wrong in [
            "", "6:", ":18", "6:60", "1:60:00", "a:18", "6:1x", "1:2:3:4", "-5", "6:18.5.1",
        ] {
            assert_eq!(parse_time(wrong), None, "{wrong:?}");
        }
        assert_eq!(written(378_560), "6:18.5");
        assert_eq!(parse_time(&written(378_560)), Some(378_500));
    }

    #[test]
    fn the_name_of_the_video_made() {
        assert_eq!(montage_name("Battuta · #"), "Battuta");
        assert_eq!(
            montage_name("Attacco · # · 4 Giovanni Sposato"),
            "Attacco 4 Giovanni Sposato"
        );
        assert_eq!(montage_name("#/"), "scout4all");
    }

    #[test]
    fn the_margins_in_their_bounds() {
        let m = ClipMargins::default();
        assert_eq!((m.before_s, m.after_s, m.overlay), (5, 5, true));
        let wide = ClipMargins {
            before_s: 99,
            after_s: 0,
            overlay: false,
        }
        .bounded();
        assert_eq!((wide.before_s, wide.after_s), (MAX_MARGIN_SECONDS, 1));
    }

    #[test]
    fn a_stretch_in_its_files() {
        let durations = [10_000, 5_000, 20_000];
        assert_eq!(
            spans(&durations, 2_000, 4_000),
            vec![FileSpan {
                file: 0,
                from_ms: 2_000,
                to_ms: 4_000
            }]
        );
        assert_eq!(
            spans(&durations, 8_000, 17_000),
            vec![
                FileSpan {
                    file: 0,
                    from_ms: 8_000,
                    to_ms: 10_000
                },
                FileSpan {
                    file: 1,
                    from_ms: 0,
                    to_ms: 5_000
                },
                FileSpan {
                    file: 2,
                    from_ms: 0,
                    to_ms: 2_000
                },
            ]
        );
        assert!(spans(&durations, 40_000, 50_000).is_empty());
    }

    #[test]
    fn the_names_of_the_files_of_android() {
        assert_eq!(
            file_name(
                "content://com.android.externalstorage.documents/document/primary%3ADownload%2F20261101%20175252.mp4"
            ),
            "20261101 175252.mp4"
        );
        assert_eq!(file_name("/Users/me/Movies/a.mp4"), "a.mp4");
    }
}
