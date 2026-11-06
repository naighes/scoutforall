//! The video of a match (001): its file played in the app, Space plays and
//! pauses, ← → go back and forward a few seconds, `-` `+` change the speed,
//! the bar goes anywhere; `c` adds files, played one after the other as a
//! single video (in the order of their starts, then as moved). Beside it the events of the
//! match, at their time in the video when it lines up with them (`video`
//! in the core: DataVolley, or the start of the file from its name): ↑ ↓
//! choose one, Enter (or a click) goes a little before it. By hand (when
//! the clocks differ): `a` ties the event chosen to the moment of the
//! video, `t` to a time written, `[` `]` shift all by half a second, `r`
//! goes back to the sync found by itself. Only the desktop app has it: the
//! profile (`video.json`) is saved next to the match.

mod files;
mod list;
mod margins;
mod player;
pub use player::Frame as Picture;
pub mod source;
mod sync;

use {
    crate::{
        strings::Strings,
        theme::{self, Colors, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, ContentFit, Element, Length,
        widget::{column, container, image, row, slider, space, text},
    },
    iced_runtime::image::Allocation,
    margins::{Margin, Margins},
    player::{Player, Status},
    scout4all_core::{
        shapes::r#match::MatchEntry,
        video::{self, Anchor, AnchorSource, EventRef, StoredProfile, VideoFile, VideoProfile},
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{MatchClips, MobileError, Scout4All, VideoEvent},
    std::{
        path::PathBuf,
        sync::Arc,
        time::{Duration, Instant},
    },
};

/// How far ← and → go.
const STEP_SECONDS: u64 = 5;

/// How long before an event the video goes to it: the serve is seen
/// coming.
const LEAD_MS: i64 = 2000;

/// The fields of the seconds of a clip before and after its action.
const BEFORE_FIELD: &str = "video-clip-before";
const AFTER_FIELD: &str = "video-clip-after";

/// How far `[` and `]` shift the sync.
const SHIFT_MS: i64 = 500;

/// The field of the time of the event.
const TIME_FIELD: &str = "video-anchor-time";

/// The speeds, `-` and `+` among them.
const SPEEDS: [f32; 4] = [0.5, 1.0, 1.5, 2.0];

/// After a command, the frames of the window keep coming this long: the
/// frame asked for is decoded meanwhile.
const AWAKE: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone)]
pub enum Message {
    /// a frame of the window: the frame of the video at its time
    Tick,
    PlayPause,
    Step(bool),
    Seek(f64),
    Speed(f32),
    /// clip mode on or off
    Clip,
    /// the video in its own window, or back in the screen
    Detach,
    Choose,
    /// the files chosen, to add
    Chosen(Vec<PathBuf>),
    /// the file one place earlier (`true`) or later
    MoveFile(usize, bool),
    RemoveFile(usize),
    /// a frame ready to be drawn at once, if it could be
    Allocated(Arc<player::Frame>, image::Handle, Option<Allocation>),
    /// an event of the list: chosen, and the video at it
    Event(usize),
    /// the event chosen tied to the moment of the video
    AnchorHere,
    /// the sync shifted, the events later in the video when positive
    Shift(i64),
    /// back to the sync found by itself
    Automatic,
    /// the time of the event chosen, to write
    EditTime,
    TimeInput(String),
    TimeSubmit,
    /// the actions one after the other, or each stopping
    Sequence,
    /// with the actions of a team, the match after (`true`) or before
    OtherMatch(bool),
    /// the seconds of the clip before or after its action, one more (`1`)
    /// or less
    MarginStep(Margin, i32),
    /// the seconds being written
    MarginInput(Margin, String),
    /// the seconds written, done
    MarginSubmit,
}

pub enum Outcome {
    Stay,
    Back,
    /// the files to choose (`Message::Chosen`)
    Choose,
    /// a new frame to allocate before showing it (`Message::Allocated`):
    /// iced draws a handle only once it's allocated, so a new handle at
    /// every frame would never be drawn
    Allocate(Arc<player::Frame>, image::Handle),
}

/// A frame of the video and its handle, to allocate.
pub type Frame = (Arc<player::Frame>, image::Handle);

/// The frame allocated, to show.
pub fn allocate(frame: Arc<player::Frame>, handle: image::Handle) -> iced::Task<Message> {
    iced_runtime::image::allocate(handle.clone())
        .map(move |allocated| Message::Allocated(frame.clone(), handle.clone(), allocated.ok()))
}

/// The frame on screen and the one it was made from.
struct Shown {
    frame: Arc<player::Frame>,
    handle: image::Handle,
    /// keeps it drawn
    _allocation: Option<Allocation>,
}

pub struct VideoScreen {
    core: Arc<Scout4All>,
    /// the folder of the data: the paths in it are saved relative to it
    dir: PathBuf,
    pub team_id: String,
    pub match_id: String,
    the_match: MatchEntry,
    /// none: written by a newer version, left as it is
    profile: Option<VideoProfile>,
    player: Option<Player>,
    status: Option<Status>,
    shown: Option<Shown>,
    /// the last frame asked to allocate: several at once, as allocating
    /// takes a few frames of the window
    requested: u64,
    awake_until: Instant,
    error: Option<String>,
    /// the events of the match, at their time in the video
    events: Vec<VideoEvent>,
    selected: usize,
    /// the list to scroll to the event chosen
    scroll: bool,
    /// the event chosen follows the video: off once chosen with the keys,
    /// on again when the video is moved or played
    follow: bool,
    /// clip mode: going to an event plays it and stops a little after
    clip: bool,
    /// where the clip playing stops
    clip_end: Option<u64>,
    /// the video in its own window (the app opens and closes it)
    detached: bool,
    /// the time of the event chosen, being written
    editing: Option<String>,
    /// the time written isn't one
    invalid_time: bool,
    /// only some actions of the match, picked in its statistics
    clips: Option<Clips>,
    /// with the actions picked, at the end of a clip the next one starts
    sequence: bool,
    /// the actions picked of a team, match after match: this one of them
    tour: Option<Tour>,
    /// the seconds of a clip before and after its action, as chosen
    margins: Margins,
    /// the seconds of one of them being written
    margin_text: Option<(Margin, String)>,
}

/// The matches of a team with actions picked in its statistics, each with
/// a video, and the one shown.
struct Tour {
    matches: Vec<MatchClips>,
    at: usize,
}

/// The actions of the match picked in its statistics, to watch one after
/// the other.
struct Clips {
    /// what they are: the fundamental, the filters
    title: String,
    events: Vec<EventRef>,
}

impl VideoScreen {
    pub fn new(
        core: Arc<Scout4All>,
        dir: PathBuf,
        team_id: String,
        match_id: String,
    ) -> Result<VideoScreen, MobileError> {
        let (the_match, stored) = core.video(&team_id, &match_id)?;
        let profile = match stored {
            StoredProfile::Missing => Some(VideoProfile::default()),
            StoredProfile::Profile(p) => Some(p),
            StoredProfile::Newer(_) => None,
        };
        let mut screen = VideoScreen {
            core,
            dir,
            team_id,
            match_id,
            the_match,
            profile,
            player: None,
            status: None,
            shown: None,
            requested: 0,
            awake_until: Instant::now(),
            error: None,
            events: vec![],
            selected: 0,
            scroll: false,
            follow: true,
            clip: false,
            clip_end: None,
            detached: false,
            editing: None,
            invalid_time: false,
            clips: None,
            sequence: true,
            tour: None,
            margins: Margins::default(),
            margin_text: None,
        };
        screen.margins = Margins::read(&screen.dir);
        // the durations of the files first: the events in them
        screen.open_player();
        screen.load_events()?;
        Ok(screen)
    }

    /// Only the actions picked in the statistics (`events`, `title` says
    /// what they are), in clip mode, one after the other: the first one
    /// played at once.
    pub fn with_clips(
        core: Arc<Scout4All>,
        dir: PathBuf,
        team_id: String,
        match_id: String,
        events: Vec<EventRef>,
        title: String,
    ) -> Result<VideoScreen, MobileError> {
        let mut screen = VideoScreen::new(core, dir, team_id, match_id)?;
        screen.clips = Some(Clips { title, events });
        screen.clip = true;
        screen.load_events()?;
        if let Some(first) = (0..screen.events.len()).find(|i| screen.event_target(*i).is_some()) {
            screen.go_to_event(first);
        }
        Ok(screen)
    }

    /// The actions picked in the statistics of a team, match after match
    /// (`matches`, each with a video): the first one played at once.
    pub fn with_tour(
        core: Arc<Scout4All>,
        dir: PathBuf,
        team_id: String,
        matches: Vec<MatchClips>,
        title: String,
    ) -> Result<VideoScreen, MobileError> {
        let first = matches
            .first()
            .cloned()
            .ok_or_else(|| MobileError::Failed {
                reason: "no match with actions to watch".to_string(),
            })?;
        let mut screen =
            VideoScreen::with_clips(core, dir, team_id, first.match_id, first.events, title)?;
        screen.tour = Some(Tour { matches, at: 0 });
        Ok(screen)
    }

    /// The match `at` of the tour in place of this one, its first action
    /// played; the mode, the speed and the own window kept.
    fn open_match(&mut self, at: usize) {
        let (Some(tour), Some(clips)) = (self.tour.take(), &self.clips) else {
            return;
        };
        let Some(m) = tour.matches.get(at).cloned() else {
            self.tour = Some(tour);
            return;
        };
        let opened = VideoScreen::with_clips(
            self.core.clone(),
            self.dir.clone(),
            self.team_id.clone(),
            m.match_id,
            m.events,
            clips.title.clone(),
        );
        match opened {
            Ok(mut next) => {
                next.tour = Some(Tour {
                    matches: tour.matches,
                    at,
                });
                next.sequence = self.sequence;
                next.detached = self.detached;
                next.speed(self.current_speed());
                *self = next;
            }
            Err(e) => {
                self.tour = Some(tour);
                self.error = Some(e.to_string());
            }
        }
    }

    /// The match of the tour after (or before) this one, if any.
    fn other_match(&self, after: bool) -> Option<usize> {
        let tour = self.tour.as_ref()?;
        match after {
            true => Some(tour.at + 1).filter(|i| *i < tour.matches.len()),
            false => tour.at.checked_sub(1),
        }
    }

    /// Whether only the actions picked in the statistics are shown.
    pub fn is_clips(&self) -> bool {
        self.clips.is_some()
    }

    /// The events at their time in the video, as the profile lines them up:
    /// only the ones picked, with some.
    fn load_events(&mut self) -> Result<(), MobileError> {
        let profile = self.profile.clone().unwrap_or_default();
        let durations = self.durations();
        self.events = self
            .core
            .video_events(&self.the_match, &profile, &durations)?;
        if let Some(clips) = &self.clips {
            self.events.retain(|e| clips.events.contains(&event_ref(e)));
        }
        self.selected = self.selected.min(self.events.len().saturating_sub(1));
        Ok(())
    }

    /// Where the list goes, to keep the event chosen in sight, once the
    /// keys moved it.
    pub fn take_scroll(&mut self) -> Option<(iced::widget::Id, f32)> {
        let last = self.events.len().saturating_sub(1).max(1);
        std::mem::take(&mut self.scroll).then(|| {
            (
                iced::widget::Id::new(list::LIST),
                self.selected as f32 / last as f32,
            )
        })
    }

    fn select(&mut self, i: usize) {
        self.selected = i.min(self.events.len().saturating_sub(1));
        self.scroll = true;
    }

    /// Where the video goes for the event: a little before it, not before
    /// the start; none when not lined up.
    fn event_target(&self, i: usize) -> Option<u64> {
        self.events.get(i).and_then(|e| target(e, self.lead_ms()))
    }

    /// How long before an event the video goes to it: in clip mode (always
    /// with the actions picked), the seconds chosen.
    fn lead_ms(&self) -> i64 {
        match self.clip || self.clips.is_some() {
            true => i64::from(self.margins.before_s) * 1000,
            false => LEAD_MS,
        }
    }

    /// In clip mode, how long after the event gone to the video stops.
    fn after_ms(&self) -> u64 {
        u64::from(self.margins.after_s) * 1000
    }

    /// The seconds before or after the actions at `seconds`, saved.
    fn set_margin(&mut self, margin: Margin, seconds: u32) {
        self.margins = self.margins.with(margin, seconds);
        if let Err(e) = self.margins.write(&self.dir) {
            self.error = Some(e);
        }
    }

    fn step_margin(&mut self, margin: Margin, by: i32) {
        self.margin_text = None;
        let seconds = self.margins.get(margin).saturating_add_signed(by);
        self.set_margin(margin, seconds);
    }

    /// The seconds written: kept while they're a number (empty: being
    /// written), the field showing them.
    fn write_margin(&mut self, margin: Margin, text: String) {
        let digits: String = text.chars().filter(char::is_ascii_digit).take(2).collect();
        if let Ok(seconds) = digits.parse::<u32>() {
            self.set_margin(margin, seconds);
        }
        self.margin_text = Some((margin, digits));
    }

    /// The event being seen chosen, the list after it, while following.
    fn follow_video(&mut self, position_ms: u64) {
        let seen = current(&self.events, position_ms, self.lead_ms());
        // the one chosen at the same time as the one seen stays
        let same_time =
            seen.is_some_and(|i| self.event_target(i) == self.event_target(self.selected));
        match (self.follow && !same_time, seen) {
            (true, Some(i)) if i != self.selected => {
                self.selected = i;
                self.scroll = true;
            }
            _ => {}
        }
    }

    /// The field the keys go to, while the time is written.
    pub fn focus_id(&self) -> Option<iced::widget::Id> {
        match (&self.editing, &self.margin_text) {
            (Some(_), _) => Some(iced::widget::Id::new(TIME_FIELD)),
            (None, Some((Margin::Before, _))) => Some(iced::widget::Id::new(BEFORE_FIELD)),
            (None, Some((Margin::After, _))) => Some(iced::widget::Id::new(AFTER_FIELD)),
            (None, None) => None,
        }
    }

    /// The event of the list tied to the video, if any.
    fn anchored(&self) -> Option<EventRef> {
        self.profile
            .as_ref()
            .and_then(|p| p.anchor.as_ref())
            .and_then(|a| a.event)
    }

    /// The anchor saved in place of the one there: the events lined up
    /// again.
    fn save_anchor(&mut self, anchor: Option<Anchor>) {
        let Some(profile) = &self.profile else {
            return;
        };
        let profile = VideoProfile {
            anchor,
            ..profile.clone()
        };
        match self.core.save_video(&self.the_match, &profile) {
            Ok(()) => {
                self.profile = Some(profile);
                if let Err(e) = self.load_events() {
                    self.error = Some(e.to_string());
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// The event chosen happens at `ms` in the video.
    fn anchor_event(&mut self, ms: u64) {
        let Some(e) = self.events.get(self.selected) else {
            return;
        };
        let at = i64::try_from(ms).unwrap_or(i64::MAX);
        self.save_anchor(Some(video::anchor_on(event_ref(e), e.timestamp, at)));
    }

    /// All the events shifted by `ms` in the video: the sync found by
    /// itself becomes the user's.
    fn shift(&mut self, ms: i64) {
        let current = self
            .profile
            .as_ref()
            .and_then(|p| video::anchor(p, &self.the_match));
        if let Some(anchor) = current {
            self.save_anchor(Some(Anchor {
                offset_ms: anchor.offset_ms + ms,
                ..anchor
            }));
        }
    }

    /// Writing the time of the event chosen: where it is now, or where the
    /// video is.
    fn edit_time(&mut self) {
        let now = self
            .events
            .get(self.selected)
            .and_then(|e| e.offset_ms)
            .filter(|ms| *ms >= 0)
            .map(i64::unsigned_abs)
            .unwrap_or_else(|| self.position());
        if !self.events.is_empty() {
            self.editing = Some(sync::written(now));
        }
    }

    fn submit_time(&mut self) {
        match self.editing.as_deref().map(sync::parse_time) {
            Some(Some(ms)) => {
                self.editing = None;
                self.invalid_time = false;
                self.anchor_event(ms);
                self.go_to_event(self.selected);
            }
            Some(None) => self.invalid_time = true,
            None => {}
        }
    }

    fn go_to_event(&mut self, i: usize) {
        self.selected = i.min(self.events.len().saturating_sub(1));
        if let Some(ms) = self.event_target(self.selected) {
            self.seek(ms);
            self.play_clip();
        }
    }

    /// In clip mode, the event gone to played up to a little after it.
    fn play_clip(&mut self) {
        let end = self
            .events
            .get(self.selected)
            .and_then(|e| e.offset_ms)
            .map(|ms| ms.max(0).unsigned_abs() + self.after_ms());
        if let (true, Some(end)) = (self.clip, end) {
            self.clip_end = Some(end);
            if let Some(player) = &self.player {
                player.play();
            }
        }
    }

    /// Whether the clip playing is over at `position_ms`: then no more
    /// clip, the video to stop.
    fn clip_over(&mut self, position_ms: u64) -> bool {
        match self.clip_end {
            Some(end) if position_ms >= end => {
                self.clip_end = None;
                true
            }
            _ => false,
        }
    }

    /// The video at `position_ms`: the event seen chosen; the clip over,
    /// the next one played (in sequence) or the video stopped.
    /// With the actions picked, the one whose clip is playing stays chosen
    /// (the next one may start before it ends): the next one is the one
    /// after it.
    fn reached(&mut self, position_ms: u64) {
        let playing = self.clips.is_some() && self.clip_end.is_some();
        match (self.clip_over(position_ms), playing) {
            (true, _) => match (self.next_clip(), self.next_match()) {
                (Some(next), _) => self.go_to_event(next),
                (None, Some(at)) => self.open_match(at),
                (None, None) => {
                    if let Some(player) = &self.player {
                        player.pause();
                    }
                }
            },
            (false, true) => {}
            (false, false) => self.follow_video(position_ms),
        }
    }

    /// With the actions of a team in sequence, the match after this one
    /// when its last action is over.
    fn next_match(&self) -> Option<usize> {
        match self.sequence {
            true => self.other_match(true),
            false => None,
        }
    }

    /// The action after the one chosen, in the video, to play at the end
    /// of its clip: only with the actions picked, in sequence. The ones at
    /// the same time as it skipped, the clip already seen (DataVolley gives
    /// the touches of a rally the same second of the video).
    fn next_clip(&self) -> Option<usize> {
        let duration = self.status.as_ref().map(|st| st.duration_ms);
        let seen = self.event_target(self.selected);
        match (self.clips.is_some(), self.sequence) {
            (true, true) => (self.selected + 1..self.events.len()).find(|i| {
                self.events.get(*i).is_some_and(|e| {
                    list::inside(e.offset_ms, duration) && self.event_target(*i) != seen
                })
            }),
            _ => None,
        }
    }

    /// Whether the video wants its own window.
    pub fn detached(&self) -> bool {
        self.detached
    }

    /// Its window closed: the video back in the screen.
    pub fn attach(&mut self) {
        self.detached = false;
    }

    fn toggle_clip(&mut self) {
        self.clip = !self.clip;
        self.clip_end = None;
    }

    pub fn title(&self, s: &Strings) -> String {
        format!("{} – {}", s.video, self.the_match.opponent)
    }

    /// Of each file, once read by the player.
    fn durations(&self) -> Vec<u64> {
        self.player
            .as_ref()
            .map(|p| p.durations_ms().to_vec())
            .unwrap_or_default()
    }

    fn paths(&self) -> Vec<PathBuf> {
        self.profile
            .iter()
            .flat_map(|p| &p.files)
            .map(|f| video::resolved_path(&self.dir, &f.path))
            .collect()
    }

    /// The player of the files of the profile, if they're there.
    fn open_player(&mut self) {
        self.player = None;
        self.status = None;
        self.shown = None;
        self.requested = 0;
        let paths = self.paths();
        if paths.is_empty() {
            return;
        }
        self.error = None;
        match paths.iter().find(|p| !p.is_file()) {
            Some(missing) => self.error = Some(missing.display().to_string()),
            None => match Player::open(&paths) {
                Ok(player) => {
                    self.player = Some(player);
                    self.wake();
                }
                Err(e) => self.error = Some(e),
            },
        }
    }

    /// Whether the frames of the window are wanted: playing, or a frame on
    /// its way.
    pub fn awake(&self) -> bool {
        self.player.is_some()
            && (self.status.as_ref().is_some_and(|s| s.playing)
                || Instant::now() < self.awake_until)
    }

    fn wake(&mut self) {
        self.awake_until = Instant::now() + AWAKE;
    }

    /// The frame of the player, to allocate once it's a new one.
    fn tick(&mut self) -> Outcome {
        self.status = self.player.as_ref().and_then(Player::status);
        if let Some(position) = self.status.as_ref().map(|s| s.position_ms) {
            self.reached(position);
        }
        match self.status.as_ref().and_then(|s| s.frame.clone()) {
            Some(frame) if frame.serial > self.requested => {
                self.requested = frame.serial;
                let handle =
                    image::Handle::from_rgba(frame.width, frame.height, frame.rgba.clone());
                Outcome::Allocate(frame, handle)
            }
            _ => Outcome::Stay,
        }
    }

    /// A frame allocated: on screen, unless a newer one is already.
    fn allocated(
        &mut self,
        frame: Arc<player::Frame>,
        handle: image::Handle,
        allocation: Option<Allocation>,
    ) {
        let newer = self
            .shown
            .as_ref()
            .is_none_or(|shown| shown.frame.serial < frame.serial);
        if newer {
            self.shown = Some(Shown {
                frame,
                handle,
                _allocation: allocation,
            });
        }
    }

    fn position(&self) -> u64 {
        self.status.as_ref().map_or(0, |s| s.position_ms)
    }

    fn play_pause(&mut self) {
        self.follow = true;
        if let Some(player) = &self.player {
            match self.status.as_ref().is_some_and(|s| s.playing) {
                true => player.pause(),
                false => player.play(),
            }
        }
        self.wake();
    }

    fn step(&mut self, forward: bool) {
        let step = STEP_SECONDS * 1000;
        let to = match forward {
            true => self.position() + step,
            false => self.position().saturating_sub(step),
        };
        self.seek(to);
    }

    fn seek(&mut self, ms: u64) {
        self.follow = true;
        // moved by hand: no clip to stop at
        self.clip_end = None;
        if let Some(player) = &self.player {
            player.seek(ms);
        }
        // the bar follows at once
        if let Some(status) = &mut self.status {
            status.position_ms = ms.min(status.duration_ms);
        }
        self.wake();
    }

    fn speed(&mut self, speed: f32) {
        if let Some(player) = &self.player {
            player.speed(speed);
        }
        self.wake();
    }

    fn current_speed(&self) -> f32 {
        self.status.as_ref().map_or(1.0, |s| s.speed)
    }

    /// The next speed, or the one before: none past the ends.
    fn next_speed(&self, faster: bool) -> Option<f32> {
        let current = self.current_speed();
        match faster {
            true => SPEEDS.iter().find(|s| **s > current + 0.01).copied(),
            false => SPEEDS.iter().rev().find(|s| **s < current - 0.01).copied(),
        }
    }

    /// The files saved in place of the ones there: played, the events
    /// lined up with them.
    fn save_files(&mut self, files: Vec<VideoFile>) {
        let Some(profile) = &self.profile else {
            return;
        };
        let profile = VideoProfile {
            files,
            ..profile.clone()
        };
        match self.core.save_video(&self.the_match, &profile) {
            Ok(()) => {
                self.profile = Some(profile);
                self.open_player();
                if let Err(e) = self.load_events() {
                    self.error = Some(e.to_string());
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn files(&self) -> Vec<VideoFile> {
        self.profile
            .as_ref()
            .map(|p| p.files.clone())
            .unwrap_or_default()
    }

    /// The files chosen, with when each starts from its name.
    fn chosen(&mut self, paths: &[PathBuf]) {
        let offset = *self.the_match.date.offset();
        let new = paths
            .iter()
            .map(|path| VideoFile {
                path: video::stored_path(&self.dir, path),
                start: video::start_from_name(path, offset),
            })
            .collect();
        let files = files::added(&self.files(), new);
        self.save_files(files);
    }

    pub fn update(&mut self, message: Message) -> Outcome {
        match message {
            Message::Tick => return self.tick(),
            Message::Allocated(frame, handle, allocation) => {
                self.allocated(frame, handle, allocation)
            }
            Message::PlayPause => self.play_pause(),
            Message::Step(forward) => self.step(forward),
            Message::Seek(ms) => self.seek(ms.max(0.0) as u64),
            Message::Speed(speed) => self.speed(speed),
            Message::Clip => self.toggle_clip(),
            Message::Detach => self.detached = !self.detached,
            Message::Choose => return self.choose(),
            Message::Chosen(paths) if paths.is_empty() => {}
            Message::Chosen(paths) => self.chosen(&paths),
            Message::MoveFile(i, earlier) => {
                let files = files::moved(&self.files(), i, earlier);
                self.save_files(files);
            }
            Message::RemoveFile(i) => {
                let mut files = self.files();
                if i < files.len() {
                    files.remove(i);
                    self.save_files(files);
                }
            }
            Message::Event(i) => self.go_to_event(i),
            Message::AnchorHere => self.anchor_event(self.position()),
            Message::Shift(ms) => self.shift(ms),
            Message::Automatic => self.save_anchor(None),
            Message::EditTime => self.edit_time(),
            Message::TimeInput(text) => {
                self.invalid_time = false;
                self.editing = Some(text);
            }
            Message::TimeSubmit => self.submit_time(),
            Message::Sequence => self.sequence = !self.sequence,
            Message::OtherMatch(after) => {
                if let Some(at) = self.other_match(after) {
                    self.open_match(at);
                }
            }
            Message::MarginStep(margin, by) => self.step_margin(margin, by),
            Message::MarginInput(margin, text) => self.write_margin(margin, text),
            Message::MarginSubmit => self.margin_text = None,
        }
        Outcome::Stay
    }

    fn choose(&self) -> Outcome {
        match self.profile {
            Some(_) => Outcome::Choose,
            None => Outcome::Stay,
        }
    }

    pub fn key(&mut self, ck: KeyCombination) -> Outcome {
        // the seconds being written take the keys; Esc leaves them
        if self.margin_text.is_some() {
            if ck == BACK_KEY {
                self.margin_text = None;
            }
            return Outcome::Stay;
        }
        // the time being written takes the keys; Esc leaves it
        if self.editing.is_some() {
            if ck == BACK_KEY {
                self.editing = None;
                self.invalid_time = false;
            }
            return Outcome::Stay;
        }
        match ck {
            k if k == BACK_KEY => return Outcome::Back,
            k if k == key!(space) => self.play_pause(),
            k if k == key!(left) => self.step(false),
            k if k == key!(right) => self.step(true),
            k if k == key!('-') => {
                if let Some(speed) = self.next_speed(false) {
                    self.speed(speed);
                }
            }
            k if k == key!('+') || k == key!('=') => {
                if let Some(speed) = self.next_speed(true) {
                    self.speed(speed);
                }
            }
            k if k == key!(c) => return self.choose(),
            k if k == key!(up) => {
                self.follow = false;
                self.select(self.selected.saturating_sub(1));
            }
            k if k == key!(down) => {
                self.follow = false;
                self.select(self.selected + 1);
            }
            k if k == key!(enter) => self.go_to_event(self.selected),
            k if k == key!(a) => self.anchor_event(self.position()),
            k if k == key!(t) => self.edit_time(),
            k if k == key!('[') => self.shift(-SHIFT_MS),
            k if k == key!(']') => self.shift(SHIFT_MS),
            k if k == key!(r) => self.save_anchor(None),
            k if k == key!(k) => self.toggle_clip(),
            k if k == key!(w) => self.detached = !self.detached,
            k if k == key!(s) && self.clips.is_some() => self.sequence = !self.sequence,
            k if k == key!(pageup) || k == key!(pagedown) => {
                if let Some(at) = self.other_match(k == key!(pagedown)) {
                    self.open_match(at);
                }
            }
            _ => {}
        }
        Outcome::Stay
    }

    /// The keys, the short way: the ones with a button of their own on
    /// screen (the speed `-` `+`, the file `c` once chosen, `r`) left out,
    /// for the bar to fit.
    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        if self.margin_text.is_some() {
            return vec![
                ("0–9".to_string(), s.clip_margins_explained.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ];
        }
        if self.editing.is_some() {
            return vec![
                ("Enter".to_string(), s.anchor_time.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ];
        }
        let hint = |key: &str, what: String| (key.to_string(), what);
        let playing = match self.player {
            Some(_) => vec![
                hint(s.space, format!("{} / {}", s.play, s.pause)),
                hint("← →", format!("±{STEP_SECONDS} s")),
            ],
            None => vec![hint("c", s.choose_video.to_string())],
        };
        let events = match (self.events.is_empty(), &self.player) {
            (true, _) => vec![],
            (false, None) => vec![hint("↑ ↓ Enter", s.go_to_event.to_string())],
            // with the actions picked, the sync left to its buttons
            (false, Some(_)) if self.clips.is_some() => vec![
                hint("↑ ↓ Enter", s.go_to_event.to_string()),
                hint("k", s.clip.to_string()),
            ],
            (false, Some(_)) => vec![
                hint("↑ ↓ Enter", s.go_to_event.to_string()),
                hint("a", s.anchor_here.to_string()),
                hint("t", s.anchor_time.to_string()),
                hint("[ ]", format!("±{} s", SHIFT_MS as f64 / 1000.0)),
                hint("k", s.clip.to_string()),
            ],
        };
        let sequence = match (&self.clips, &self.player) {
            (Some(_), Some(_)) => vec![hint("s", s.in_sequence.to_string())],
            _ => vec![],
        };
        let matches = match &self.tour {
            Some(tour) if tour.matches.len() > 1 => {
                vec![hint("PgUp PgDn", s.other_match.to_string())]
            }
            _ => vec![],
        };
        let window = match (self.events.is_empty(), &self.player) {
            (false, Some(_)) => vec![hint(
                "w",
                match self.detached {
                    true => s.attach_video,
                    false => s.detach_video,
                }
                .to_string(),
            )],
            _ => vec![],
        };
        [
            playing,
            events,
            sequence,
            matches,
            window,
            vec![hint("Esc", s.back.to_string())],
        ]
        .concat()
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let notice: Option<String> = match (&self.profile, &self.error, &self.status) {
            (None, _, _) => Some(s.video_newer.to_string()),
            (_, Some(e), _) => Some(match self.paths().iter().any(|p| !p.is_file()) {
                true => ui::fill(s.video_missing, std::slice::from_ref(e)),
                false => format!("{}: {e}", s.video_unplayable),
            }),
            (_, None, Some(Status { error: Some(e), .. })) => {
                Some(format!("{}: {e}", s.video_unplayable))
            }
            _ => None,
        };
        let screen: Element<'a, Message> = match self.detached {
            true => container(
                column![
                    ui::body(s.video_in_window).color(c.muted),
                    ui::pill(s.attach_video, true, Some(Message::Detach), c),
                ]
                .spacing(12)
                .align_x(Alignment::Center),
            )
            .center(Length::Fill)
            .into(),
            false => self.frame_view(s, c),
        };
        let screen = black(screen);
        let files = self.files();
        let add = ui::pill(
            match files.is_empty() {
                true => s.choose_video,
                false => s.add_videos,
            },
            self.player.is_none(),
            Some(Message::Choose),
            c,
        );
        let file_line: Element<'a, Message> = match files.is_empty() {
            true => container(add)
                .width(Length::Fill)
                .align_x(Alignment::End)
                .into(),
            false => column![
                row![
                    text(s.video_files_title)
                        .size(SIZE_SMALL)
                        .color(c.muted)
                        .width(Length::Fill),
                    add,
                ]
                .align_y(Alignment::Center),
                files::view(&files, &self.durations(), s, c),
            ]
            .spacing(6)
            .into(),
        };
        let controls: Element<'a, Message> = match &self.status {
            Some(status) => self.controls(status, s, c),
            None => space::vertical().height(0).into(),
        };
        let notice: Element<'a, Message> = match notice {
            Some(n) => ui::body(n).color(c.danger).into(),
            None => space::vertical().height(0).into(),
        };
        let video = column![notice, screen, controls, file_line].spacing(10);
        let events: Element<'a, Message> = match (self.events.is_empty(), &self.clips) {
            (true, None) => space::horizontal().width(0).into(),
            (true, Some(_)) => iced::widget::container(ui::panel(
                Some(s.clips_title),
                ui::body(s.clips_none).color(c.muted),
                c,
            ))
            .width(Length::Fixed(400.0))
            .into(),
            (false, None) => {
                iced::widget::container(ui::panel(Some(s.match_events), self.events_view(s, c), c))
                    .width(Length::Fixed(400.0))
                    .into()
            }
            (false, Some(clips)) => iced::widget::container(ui::panel(
                Some(s.clips_title),
                column![
                    text(clips.title.clone())
                        .size(theme::SIZE_BODY)
                        .font(theme::fonts().bold),
                    self.tour_view(s, c),
                    text(ui::fill(s.clips_count, &[self.events.len().to_string()]))
                        .size(SIZE_SMALL)
                        .color(c.muted),
                    self.events_view(s, c),
                ]
                .spacing(6),
                c,
            ))
            .width(Length::Fixed(400.0))
            .into(),
        };
        row![video.width(Length::Fill), events].spacing(14).into()
    }

    /// With the actions of a team: which match of how many, against whom
    /// and when, and the ones before and after.
    fn tour_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let Some(tour) = &self.tour else {
            return space::vertical().height(0).into();
        };
        let day = tour
            .matches
            .get(tour.at)
            .map(|m| m.day.clone())
            .unwrap_or_default();
        let which = ui::fill(
            s.clips_match_of,
            &[(tour.at + 1).to_string(), tour.matches.len().to_string()],
        );
        let arrow = |label: &str, what: &str, after: bool| {
            ui::tip(
                ui::pill(
                    label,
                    false,
                    self.other_match(after).map(|_| Message::OtherMatch(after)),
                    c,
                ),
                what,
                c,
            )
        };
        row![
            arrow("◀", s.previous_match, false),
            column![
                text(which).size(SIZE_SMALL).font(theme::fonts().bold),
                text(format!("{} · {day}", self.the_match.opponent))
                    .size(SIZE_SMALL)
                    .color(c.muted),
            ]
            .width(Length::Fill),
            arrow("▶", s.next_match, true),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    /// How the events line up with the video, and how many are outside it.
    fn alignment(&self, s: &Strings) -> (String, bool) {
        let profile = self.profile.clone().unwrap_or_default();
        let source = video::anchor_source(&profile, &self.the_match);
        let said = match source {
            Some(AnchorSource::VideoClock) => s.aligned_dvw.to_string(),
            Some(AnchorSource::Chosen) => s.aligned_chosen.to_string(),
            Some(AnchorSource::FileStart(start)) => ui::fill(
                s.aligned_file,
                &[start.format("%Y-%m-%d %H:%M:%S").to_string()],
            ),
            None => s.not_aligned.to_string(),
        };
        let duration = self.status.as_ref().map(|st| st.duration_ms);
        let outside = self
            .events
            .iter()
            .filter(|e| e.offset_ms.is_some() && !list::inside(e.offset_ms, duration))
            .count();
        match (source, outside) {
            (None, _) => (said, false),
            (Some(_), 0) => (said, true),
            (Some(_), n) => (
                format!("{said} · {}", ui::fill(s.outside_video, &[n.to_string()])),
                false,
            ),
        }
    }

    fn events_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let (said, fine) = self.alignment(s);
        let said = text(said).size(SIZE_SMALL).color(match fine {
            true => c.muted,
            false => c.danger,
        });
        let duration = self.status.as_ref().map(|st| st.duration_ms);
        let anchored = self.anchored();
        column![
            said,
            list::view(&self.events, self.selected, anchored, duration, s, c),
            self.sync_view(s, c),
        ]
        .spacing(10)
        .height(Length::Fill)
        .into()
    }

    /// The frame shown, as large as it fits.
    fn frame_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        match (&self.shown, &self.player) {
            (Some(shown), _) => image(shown.handle.clone())
                .content_fit(ContentFit::Contain)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            (None, Some(_)) => space::vertical().height(Length::Fill).into(),
            (None, None) => container(ui::body(s.no_video).color(c.muted))
                .center(Length::Fill)
                .into(),
        }
    }

    /// The own window of the video: the frame alone, the keys as in the
    /// screen.
    pub fn window_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        black(self.frame_view(s, c)).into()
    }

    /// Whether the user chose the sync.
    fn anchored_by_hand(&self) -> bool {
        self.profile.as_ref().is_some_and(|p| p.anchor.is_some())
    }

    /// The sync by hand, under the events.
    fn sync_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let time: Element<'a, Message> = match &self.editing {
            Some(written) => column![
                iced::widget::text_input("m:ss.s", written)
                    .id(iced::widget::Id::new(TIME_FIELD))
                    .on_input(Message::TimeInput)
                    .on_submit(Message::TimeSubmit)
                    .size(theme::SIZE_BODY)
                    .padding(iced::Padding::from([6, 10]))
                    .width(Length::Fixed(140.0))
                    .style(move |_, _| iced::widget::text_input::Style {
                        background: c.background.into(),
                        border: iced::Border {
                            color: c.accent,
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        icon: c.muted,
                        placeholder: c.muted,
                        value: c.text,
                        selection: iced::Color {
                            a: 0.35,
                            ..c.accent
                        },
                    }),
                match self.invalid_time {
                    true => text(s.invalid_time).size(SIZE_SMALL).color(c.danger),
                    false => text(s.anchor_time).size(SIZE_SMALL).color(c.muted),
                },
            ]
            .spacing(4)
            .into(),
            None => ui::pill(s.anchor_time, false, Some(Message::EditTime), c),
        };
        let half = SHIFT_MS as f64 / 1000.0;
        let shift = row![
            text(s.shift_sync).size(SIZE_SMALL).color(c.muted),
            ui::pill(
                &format!("−{half} s"),
                false,
                Some(Message::Shift(-SHIFT_MS)),
                c
            ),
            ui::pill(
                &format!("+{half} s"),
                false,
                Some(Message::Shift(SHIFT_MS)),
                c
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let automatic: Element<'a, Message> = match self.anchored_by_hand() {
            true => ui::pill(s.automatic_sync, false, Some(Message::Automatic), c),
            false => space::horizontal().width(0).into(),
        };
        let here: Element<'a, Message> = match self.player {
            Some(_) => ui::pill(s.anchor_here, true, Some(Message::AnchorHere), c),
            None => space::horizontal().width(0).into(),
        };
        column![
            row![here, time].spacing(8).align_y(Alignment::Center),
            shift,
            automatic,
        ]
        .spacing(8)
        .into()
    }

    fn controls<'a>(
        &'a self,
        status: &Status,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let (play_icon, play) = match status.playing {
            true => (theme::icons::PAUSE, s.pause),
            false => (theme::icons::PLAY, s.play),
        };
        let step = STEP_SECONDS.to_string();
        let duration = status.duration_ms as f64;
        let bar = slider(
            0.0..=duration.max(1.0),
            status.position_ms as f64,
            Message::Seek,
        )
        .step(100.0);
        let speeds = SPEEDS.iter().map(|speed| {
            let chosen = (status.speed - speed).abs() < 0.01;
            ui::pill(
                &format!("{speed}×"),
                chosen,
                Some(Message::Speed(*speed)),
                c,
            )
        });
        let time = format!(
            "{} / {}",
            clock(status.position_ms),
            clock(status.duration_ms)
        );
        column![
            bar,
            files::band(&self.files(), &self.durations(), c),
            row![
                ui::icon_button(
                    theme::icons::REWIND,
                    &ui::fill(s.back_seconds, std::slice::from_ref(&step)),
                    Message::Step(false),
                    false,
                    c
                ),
                ui::icon_button(play_icon, play, Message::PlayPause, true, c),
                ui::icon_button(
                    theme::icons::FORWARD,
                    &ui::fill(s.forward_seconds, std::slice::from_ref(&step)),
                    Message::Step(true),
                    false,
                    c
                ),
                text(time).size(SIZE_SMALL).font(theme::fonts().bold),
                space::horizontal(),
                ui::pill(
                    match self.detached {
                        true => s.attach_video,
                        false => s.detach_video,
                    },
                    self.detached,
                    Some(Message::Detach),
                    c
                ),
                match self.clips {
                    Some(_) => ui::tip(
                        ui::pill(s.in_sequence, self.sequence, Some(Message::Sequence), c),
                        s.in_sequence_explained,
                        c,
                    ),
                    None => space::horizontal().width(0).into(),
                },
                ui::tip(
                    ui::pill(s.clip, self.clip, Some(Message::Clip), c),
                    &ui::fill(s.clip_explained, &[self.margins.after_s.to_string()]),
                    c
                ),
                text(s.speed).size(SIZE_SMALL).color(c.muted),
            ]
            .extend(speeds)
            .spacing(10)
            .align_y(Alignment::Center),
            self.margins_view(s, c),
        ]
        .spacing(8)
        .into()
    }

    /// In clip mode, the seconds of each clip before and after its action:
    /// written, or one more or less.
    fn margins_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        if !(self.clip || self.clips.is_some()) {
            return space::vertical().height(0).into();
        }
        let field = |margin: Margin, id: &'static str, name: &str| -> Element<'a, Message> {
            let shown = match &self.margin_text {
                Some((m, text)) if *m == margin => text.clone(),
                _ => self.margins.get(margin).to_string(),
            };
            row![
                text(name.to_string()).size(SIZE_SMALL).color(c.muted),
                ui::pill("−", false, Some(Message::MarginStep(margin, -1)), c),
                iced::widget::text_input("0", &shown)
                    .id(iced::widget::Id::new(id))
                    .on_input(move |t| Message::MarginInput(margin, t))
                    .on_submit(Message::MarginSubmit)
                    .size(theme::SIZE_BODY)
                    .align_x(Alignment::Center)
                    .padding(iced::Padding::from([4, 6]))
                    .width(Length::Fixed(48.0))
                    .style(move |_, _| iced::widget::text_input::Style {
                        background: c.background.into(),
                        border: iced::Border {
                            color: c.border,
                            width: 1.0,
                            radius: 8.0.into(),
                        },
                        icon: c.muted,
                        placeholder: c.muted,
                        value: c.text,
                        selection: iced::Color {
                            a: 0.35,
                            ..c.accent
                        },
                    }),
                ui::pill("+", false, Some(Message::MarginStep(margin, 1)), c),
                text("s").size(SIZE_SMALL).color(c.muted),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
        };
        ui::tip(
            row![
                text(s.clip).size(SIZE_SMALL).font(theme::fonts().bold),
                field(Margin::Before, BEFORE_FIELD, s.clip_before),
                field(Margin::After, AFTER_FIELD, s.clip_after),
            ]
            .spacing(16)
            .align_y(Alignment::Center),
            s.clip_margins_explained,
            c,
        )
    }
}

/// The files of the video, chosen by the user: none when given up.
pub async fn pick(filter: &'static str) -> Vec<PathBuf> {
    rfd::AsyncFileDialog::new()
        .add_filter(filter, &["mp4", "MP4", "mov", "MOV", "m4v", "M4V"])
        .pick_files()
        .await
        .unwrap_or_default()
        .iter()
        .map(|f| f.path().to_path_buf())
        .collect()
}

/// The video on black, filling its place.
fn black<'a>(content: Element<'a, Message>) -> container::Container<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(iced::Color::BLACK.into()),
            ..container::Style::default()
        })
}

/// Where the video goes for the event: `lead_ms` before it, not before
/// the start; none when not lined up.
fn target(e: &VideoEvent, lead_ms: i64) -> Option<u64> {
    e.offset_ms.map(|ms| (ms - lead_ms).max(0).unsigned_abs())
}

/// The event being seen at `position_ms`: the latest whose target the video
/// passed (the one gone to stays the one seen); the later in the list when
/// two have the same time. None before the first.
fn current(events: &[VideoEvent], position_ms: u64, lead_ms: i64) -> Option<usize> {
    events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            target(e, lead_ms)
                .filter(|t| *t <= position_ms)
                .map(|t| (t, i))
        })
        .max()
        .map(|(_, i)| i)
}

/// The event as an anchor names it.
fn event_ref(e: &VideoEvent) -> EventRef {
    EventRef {
        set_number: e.set_number,
        index: e.event.index as usize,
    }
}

/// `h:mm:ss`, or `m:ss` under the hour.
pub fn clock(ms: u64) -> String {
    let seconds = ms / 1000;
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match h {
        0 => format!("{m}:{s:02}"),
        h => format!("{h}:{m:02}:{s:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::clock;

    #[test]
    fn the_time_in_the_video() {
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(378_400), "6:18");
        assert_eq!(clock(5_686_000), "1:34:46");
    }
}

#[cfg(test)]
pub(crate) mod screen_tests {
    use {
        super::*,
        scout4all_core::video::{offset_ms, start_from_name},
        scout4all_mobile::{DvwChoice, DvwSide},
    };

    /// The match of the tests of the TUI, three sets, imported: the file
    /// has no video column, the events are on the clock.
    pub(crate) fn imported(dir: &tempfile::TempDir) -> (Arc<Scout4All>, String, String) {
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let bytes = include_bytes!("../../../core/src/tests/fixtures/three_sets.dvw").to_vec();
        let choice = DvwChoice {
            side: DvwSide::Home,
            day: "2015-01-25".into(),
            max_substitutions: 6,
            team_id: None,
            classification: core.classifications().first().map(|c| c.value),
            gender: core.genders().first().map(|g| g.value),
            country: core.countries("Ital".into()).first().map(|c| c.value),
        };
        let imported = core.import_dvw(bytes, choice, false).unwrap();
        (core, imported.team_id, imported.match_id)
    }

    fn with_file(dir: &tempfile::TempDir, name: &str) -> VideoScreen {
        let (core, team, m) = imported(dir);
        let (the_match, _) = core.video(&team, &m).unwrap();
        let path = dir.path().join(name);
        let profile = VideoProfile {
            files: vec![VideoFile {
                path: video::stored_path(dir.path(), &path),
                start: start_from_name(&path, *the_match.date.offset()),
            }],
            ..Default::default()
        };
        core.save_video(&the_match, &profile).unwrap();
        VideoScreen::new(core, dir.path().to_path_buf(), team, m).unwrap()
    }

    /// The file starting at midnight of the match: each event at its time
    /// from it, the video a little before it.
    #[test]
    fn the_events_lined_up_from_the_start_of_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "20150125 000000.mp4");
        let m = &screen.the_match;
        let start = start_from_name(
            std::path::Path::new("20150125 000000.mp4"),
            *m.date.offset(),
        )
        .unwrap();
        let anchor = video::anchor(screen.profile.as_ref().unwrap(), m).unwrap();
        assert_eq!(anchor.at, start.to_utc());
        let mut sets: Vec<_> = m.sets.iter().collect();
        sets.sort_by_key(|s| s.set_number);
        let expected: Vec<Option<i64>> = sets
            .iter()
            .flat_map(|s| {
                s.events
                    .iter()
                    .map(|e| Some(offset_ms(&anchor, e.timestamp)))
            })
            .collect();
        let offsets: Vec<Option<i64>> = screen.events.iter().map(|e| e.offset_ms).collect();
        assert_eq!(offsets, expected);
        assert_eq!(screen.events.first().map(|e| e.set_number), Some(1));
        assert_eq!(screen.events.last().map(|e| e.set_number), Some(3));
        // the keys choose, Enter goes 2 s before (not before the start)
        screen.key(key!(down));
        screen.key(key!(down));
        assert_eq!(screen.selected, 2);
        assert!(screen.take_scroll().is_some());
        assert!(screen.take_scroll().is_none());
        let at = expected[2].unwrap();
        assert_eq!(
            screen.event_target(2),
            Some((at - 2000).max(0).unsigned_abs())
        );
        screen.key(key!(up));
        screen.key(key!(up));
        screen.key(key!(up));
        assert_eq!(screen.selected, 0);
        // the file isn't there: said, no player
        assert!(screen.player.is_none());
        assert!(screen.error.is_some());
        let s = crate::strings::strings(scout4all_core::shapes::enums::LanguageEnum::It);
        assert_eq!(
            screen.alignment(s).0,
            ui::fill(
                s.aligned_file,
                &[start.format("%Y-%m-%d %H:%M:%S").to_string()]
            )
        );
    }

    /// By hand: the event chosen at the time written, the others the same
    /// distance from it as their times; shifted, then back to the start of
    /// the file. Kept when the screen opens again.
    #[test]
    fn the_events_lined_up_by_hand() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "20150125 000000.mp4");
        let automatic: Vec<Option<i64>> = screen.events.iter().map(|e| e.offset_ms).collect();
        let distance = |events: &[VideoEvent]| -> Vec<i64> {
            events
                .iter()
                .map(|e| (e.timestamp - events[0].timestamp).num_milliseconds())
                .collect()
        };
        screen.key(key!(down));
        screen.key(key!(down));
        screen.key(key!(t));
        assert!(screen.focus_id().is_some());
        // while written, the keys don't choose
        screen.key(key!(down));
        assert_eq!(screen.selected, 2);
        screen.update(Message::TimeInput("1:0x".into()));
        screen.update(Message::TimeSubmit);
        assert!(screen.invalid_time && screen.editing.is_some());
        screen.update(Message::TimeInput("1:00.5".into()));
        screen.update(Message::TimeSubmit);
        assert!(screen.editing.is_none() && !screen.invalid_time);
        assert_eq!(screen.events[2].offset_ms, Some(60_500));
        assert_eq!(screen.anchored(), Some(event_ref(&screen.events[2])));
        let first = screen.events[0].offset_ms.unwrap();
        let offsets: Vec<i64> = screen
            .events
            .iter()
            .map(|e| e.offset_ms.unwrap() - first)
            .collect();
        assert_eq!(offsets, distance(&screen.events));
        // half a second later, all of them
        screen.key(key!(']'));
        assert_eq!(screen.events[2].offset_ms, Some(61_000));
        screen.key(key!('['));
        screen.key(key!('['));
        assert_eq!(screen.events[2].offset_ms, Some(60_000));
        // opened again: as left
        let (core, team, m) = (
            screen.core.clone(),
            screen.team_id.clone(),
            screen.match_id.clone(),
        );
        let mut again = VideoScreen::new(core, dir.path().to_path_buf(), team, m).unwrap();
        assert_eq!(again.events[2].offset_ms, Some(60_000));
        again.key(key!(r));
        let back: Vec<Option<i64>> = again.events.iter().map(|e| e.offset_ms).collect();
        assert_eq!(back, automatic);
        assert_eq!(again.anchored(), None);
    }

    /// Files added at once in the order of their starts, then moved and
    /// taken out: saved each time.
    #[test]
    fn the_files_of_the_video() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "20150125 190000.mp4");
        let names = |screen: &VideoScreen| -> Vec<String> {
            screen.files().iter().map(|f| f.path.clone()).collect()
        };
        screen.update(Message::Chosen(vec![
            dir.path().join("20150125 200000.mp4"),
            dir.path().join("20150125 180000.mp4"),
            dir.path().join("20150125 190000.mp4"),
        ]));
        assert_eq!(
            names(&screen),
            [
                "20150125 180000.mp4",
                "20150125 190000.mp4",
                "20150125 200000.mp4"
            ]
        );
        screen.update(Message::MoveFile(2, true));
        screen.update(Message::RemoveFile(0));
        let saved = ["20150125 200000.mp4", "20150125 190000.mp4"];
        assert_eq!(names(&screen), saved);
        let (core, team, m) = (
            screen.core.clone(),
            screen.team_id.clone(),
            screen.match_id.clone(),
        );
        let again = VideoScreen::new(core, dir.path().to_path_buf(), team, m).unwrap();
        assert_eq!(names(&again), saved);
        // the first file starts the video: the events from it
        let first = video::anchor(again.profile.as_ref().unwrap(), &again.the_match).unwrap();
        let start = start_from_name(
            std::path::Path::new(saved[0]),
            *again.the_match.date.offset(),
        );
        assert_eq!(Some(first.at), start.map(|t| t.to_utc()));
    }

    /// The event seen chosen as the video goes, from the one gone to; the
    /// keys stop it, going to an event starts it again.
    #[test]
    fn the_event_chosen_follows_the_video() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "20150125 000000.mp4");
        let rally = screen
            .events
            .iter()
            .position(|e| e.offset_ms > screen.events[0].offset_ms)
            .unwrap();
        let at = |screen: &VideoScreen, i: usize| screen.event_target(i).unwrap();
        // gone to: that one, even with an event 2 s before
        screen.go_to_event(rally);
        screen.follow_video(at(&screen, rally));
        assert_eq!(screen.selected, rally);
        // a little before its time: the latest one before it
        screen.follow_video(at(&screen, rally) - 1);
        let before = screen.selected;
        assert_ne!(before, rally);
        assert!(at(&screen, before) < at(&screen, rally));
        assert!(
            (0..screen.events.len())
                .all(|i| at(&screen, i) <= at(&screen, before)
                    || at(&screen, i) >= at(&screen, rally))
        );
        assert!(screen.take_scroll().is_some());
        // at its time again: one at that time (the fixture has events of
        // other sets at the same times)
        screen.follow_video(at(&screen, rally));
        assert_eq!(at(&screen, screen.selected), at(&screen, rally));
        // chosen with the keys: the video doesn't move it
        screen.key(key!(down));
        let chosen = screen.selected;
        screen.follow_video(0);
        assert_eq!(screen.selected, chosen);
        // going to an event follows again
        screen.key(key!(enter));
        screen.follow_video(at(&screen, rally));
        assert_eq!(at(&screen, screen.selected), at(&screen, rally));
    }

    #[test]
    fn the_event_seen() {
        let dir = tempfile::tempdir().unwrap();
        let screen = with_file(&dir, "20150125 000000.mp4");
        let events = &screen.events;
        let first = target(&events[0], LEAD_MS).unwrap();
        // the later in the list of the ones at the same time
        let same = events
            .iter()
            .rposition(|e| e.offset_ms == events[0].offset_ms)
            .unwrap();
        assert_eq!(current(events, first, LEAD_MS), Some(same));
        let unplaced = with_file(&tempfile::tempdir().unwrap(), "match.mp4");
        assert_eq!(current(&unplaced.events, 10_000_000, LEAD_MS), None);
    }

    /// In clip mode, the event gone to stops a little after it; moved by
    /// hand, no stop; mode off, no clip.
    #[test]
    fn a_clip_of_the_event() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "20150125 000000.mp4");
        let i = 3;
        let end = screen.events[i].offset_ms.unwrap().unsigned_abs() + screen.after_ms();
        screen.key(key!(enter));
        assert_eq!(screen.clip_end, None);
        screen.key(key!(k));
        screen.go_to_event(i);
        assert!(!screen.clip_over(end - 1));
        assert!(screen.clip_over(end));
        // once
        assert!(!screen.clip_over(end + 1));
        screen.go_to_event(i);
        screen.update(Message::Step(true));
        assert!(!screen.clip_over(end));
        screen.go_to_event(i);
        screen.update(Message::Clip);
        assert!(!screen.clip && !screen.clip_over(end));
    }

    /// The serves of the match picked in its statistics (the fixture has
    /// only aces): only those in
    /// the list, the first one played at once in clip mode, then the next
    /// one at the end of each clip; out of sequence (`s`), it stops.
    #[test]
    fn the_actions_picked_one_after_the_other() {
        let dir = tempfile::tempdir().unwrap();
        let screen = with_file(&dir, "20150125 000000.mp4");
        let (core, team, m) = (
            screen.core.clone(),
            screen.team_id.clone(),
            screen.match_id.clone(),
        );
        let pick = scout4all_mobile::ClipPick {
            scope: scout4all_mobile::StatsScope::Match,
            event: scout4all_core::shapes::enums::EventTypeEnum::S,
            filter: scout4all_mobile::DetailFilter::default(),
            evals: vec![],
            zone: None,
            target: None,
        };
        let refs = core.match_clips(&team, &m, &pick).unwrap();
        assert_eq!(refs.len(), 72, "the aces of the match");
        let mut screen = VideoScreen::with_clips(
            core,
            dir.path().to_path_buf(),
            team,
            m,
            refs.clone(),
            "Serve".into(),
        )
        .unwrap();
        assert!(screen.is_clips());
        assert_eq!(
            screen.events.iter().map(event_ref).collect::<Vec<_>>(),
            refs
        );
        // the first one played, in clip mode, from 5 s before it
        assert!(screen.clip);
        assert_eq!(screen.selected, 0);
        let first = screen.events[0].offset_ms.unwrap();
        assert_eq!(
            screen.event_target(0),
            Some((first - 5000).max(0).unsigned_abs())
        );
        let end = |screen: &VideoScreen| screen.clip_end.unwrap();
        let first_end = end(&screen);
        // its clip over: the next one
        screen.reached(first_end);
        assert_eq!(screen.selected, 1);
        assert!(end(&screen) > first_end);
        // out of sequence: the clip over, it stays and stops
        screen.key(key!(s));
        assert!(!screen.sequence);
        screen.reached(end(&screen));
        assert_eq!(screen.selected, 1);
        assert_eq!(screen.clip_end, None);
        // the last one: nothing after it
        screen.key(key!(s));
        let last = screen.events.len() - 1;
        screen.go_to_event(last);
        screen.reached(end(&screen));
        assert_eq!(screen.selected, last);
        assert_eq!(screen.clip_end, None);
        // two at the same time (DataVolley, a rally): seen once
        screen.events[2].offset_ms = screen.events[1].offset_ms;
        screen.go_to_event(1);
        screen.reached(end(&screen));
        assert_eq!(screen.selected, 3);
        // `s` does nothing with the whole match
        let mut whole = with_file(&tempfile::tempdir().unwrap(), "20150125 000000.mp4");
        whole.key(key!(s));
        assert!(whole.sequence && !whole.is_clips());
    }

    /// The real match (TODO/001, files in ~/Downloads): the attacks from 4
    /// of the outside hitter with the most, their time in the video, to
    /// look at the frames with ffmpeg. `cargo test -p scout4all-desktop --
    /// --ignored real_clips --nocapture`
    #[test]
    #[ignore]
    fn real_clips() {
        let home = std::env::var("HOME").unwrap();
        let downloads = PathBuf::from(home).join("Downloads");
        // `S4A_CLIPS_DIR`: the data kept there, to open in the app
        let temp = tempfile::tempdir().unwrap();
        let kept = std::env::var("S4A_CLIPS_DIR").map(PathBuf::from).ok();
        let dir = kept.as_deref().unwrap_or(temp.path());
        let core = Scout4All::new(dir.to_string_lossy().into_owned(), "it".into()).unwrap();
        let bytes = std::fs::read(downloads.join("&And03 CORTONA V vs POLISPORTIV.dvw")).unwrap();
        let day = core.preview_dvw(bytes.clone()).unwrap().days[0].clone();
        let choice = DvwChoice {
            side: DvwSide::Home,
            day,
            max_substitutions: 6,
            team_id: None,
            classification: core.classifications().first().map(|c| c.value),
            gender: core.genders().first().map(|g| g.value),
            country: core.countries("Ital".into()).first().map(|c| c.value),
        };
        let imported = core.import_dvw(bytes, choice.clone(), false).unwrap();
        let (team, m) = (imported.team_id, imported.match_id);
        // another match of the team, without its video
        let cascina = std::fs::read(downloads.join("&And01 CORTONA V vs CASCINA 2.dvw")).unwrap();
        let other = DvwChoice {
            team_id: Some(team.clone()),
            day: core.preview_dvw(cascina.clone()).unwrap().days[0].clone(),
            ..choice
        };
        if let Err(e) = core.import_dvw(cascina, other, false) {
            println!("Cascina not imported: {e}");
        }
        let (the_match, _) = core.video(&team, &m).unwrap();
        let files = ["20261101 175252.mp4", "20261101 185442.mp4"]
            .iter()
            .map(|name| {
                let path = downloads.join(name);
                VideoFile {
                    path: video::stored_path(dir, &path),
                    start: start_from_name(&path, *the_match.date.offset()),
                }
            })
            .collect();
        let profile = VideoProfile {
            files,
            ..Default::default()
        };
        core.save_video(&the_match, &profile).unwrap();
        use scout4all_core::shapes::enums::EventTypeEnum::A;
        use scout4all_mobile::{ClipPick, DetailFilter, StatsScope};
        let detail = core
            .match_detail(
                team.clone(),
                m.clone(),
                StatsScope::Match,
                A,
                DetailFilter::default(),
            )
            .unwrap();
        let f = detail.fundamental.unwrap();
        for p in &f.players {
            println!(
                "{} {} {}: {} attacks",
                p.player.number, p.player.name, p.player.role, p.total
            );
        }
        for set in &the_match.sets {
            let all = set.events.len();
            let inverted = set.events.iter().filter(|e| e.flags.inverted_p1()).count();
            println!(
                "set {}: {inverted} of {all} events with the P1 inverted",
                set.set_number
            );
        }
        let zone4 = f.zones.iter().find(|z| z.zone == 4).map(|z| z.share);
        println!("zone 4: {zone4:?}");
        let best = f
            .players
            .iter()
            .filter_map(|p| {
                let pick = ClipPick {
                    scope: StatsScope::Match,
                    event: A,
                    filter: DetailFilter {
                        player: Some(p.player.id.clone()),
                        ..DetailFilter::default()
                    },
                    evals: vec![],
                    zone: Some(4),
                    target: None,
                };
                let n = core.match_clips(&team, &m, &pick).ok()?.len();
                Some((n, p.player.name.clone(), pick))
            })
            .max_by_key(|(n, _, _)| *n)
            .unwrap();
        println!("from 4: {} by {}", best.0, best.1);
        let refs = core.match_clips(&team, &m, &best.2).unwrap();
        let screen =
            VideoScreen::with_clips(core, dir.to_path_buf(), team, m, refs, best.1.clone())
                .unwrap();
        for e in &screen.events {
            println!(
                "set {} #{} {} {:?} {}-{} at {:?} ms",
                e.set_number,
                e.event.index,
                e.event.event.name,
                e.event.event.symbol,
                e.event.score_us,
                e.event.score_them,
                e.offset_ms
            );
        }
    }

    /// The serves of a team in two matches, each with its video: the first
    /// match played, at the end of its last clip the next match opened
    /// (in sequence), PgUp back to the first.
    #[test]
    fn the_actions_of_a_team_match_after_match() {
        use crate::stats::tests::{match_against, team_with_two_matches, with_a_video};
        let dir = tempfile::tempdir().unwrap();
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = team_with_two_matches(&core);
        let (arezzo, siena) = (
            match_against(&core, &team, "Arezzo"),
            match_against(&core, &team, "Siena"),
        );
        with_a_video(&core, &team, &arezzo);
        with_a_video(&core, &team, &siena);
        let pick = scout4all_mobile::ClipPick {
            scope: scout4all_mobile::StatsScope::Match,
            event: scout4all_core::shapes::enums::EventTypeEnum::S,
            filter: scout4all_mobile::DetailFilter::default(),
            evals: vec![],
            zone: None,
            target: None,
        };
        let period = scout4all_mobile::AnalysisPeriod::LastMatches { count: 4 };
        let matches = core.team_clips(&team, &period, &pick).unwrap();
        assert_eq!(matches.len(), 2);
        let mut screen = VideoScreen::with_tour(
            core,
            dir.path().to_path_buf(),
            team,
            matches,
            "Serve".into(),
        )
        .unwrap();
        assert_eq!(screen.match_id, arezzo);
        assert_eq!(screen.events.len(), 3);
        // the clips of Arezzo one after the other, then Siena
        for _ in 0..3 {
            let end = screen.clip_end.unwrap();
            screen.reached(end);
        }
        assert_eq!(screen.match_id, siena);
        assert_eq!(screen.events.len(), 2);
        assert_eq!(screen.selected, 0);
        assert!(screen.clip_end.is_some(), "its first clip playing");
        // the last one: nothing after
        assert_eq!(screen.other_match(true), None);
        screen.key(key!(pageup));
        assert_eq!(screen.match_id, arezzo);
        assert!(screen.is_clips() && screen.sequence);
        // out of sequence, the last clip of Arezzo stays there
        screen.key(key!(s));
        screen.go_to_event(2);
        screen.reached(screen.clip_end.unwrap());
        assert_eq!(screen.match_id, arezzo);
        // by hand, PgDn
        screen.key(key!(pagedown));
        assert_eq!(screen.match_id, siena);
        assert!(!screen.sequence, "the mode kept");
    }

    /// In clip mode the clip starts and ends the seconds chosen before and
    /// after the event: − and +, or written; saved for the next time; Esc
    /// leaves the field, not the screen. Out of clip mode, 2 s before.
    #[test]
    fn the_margins_of_the_clips() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "20150125 000000.mp4");
        let i = 3;
        let at = screen.events[i].offset_ms.unwrap();
        let target = |screen: &VideoScreen| screen.event_target(i).unwrap();
        assert_eq!(target(&screen), (at - LEAD_MS).unsigned_abs());
        screen.key(key!(k));
        assert_eq!(target(&screen), (at - 5000).unsigned_abs());
        screen.update(Message::MarginStep(Margin::Before, 1));
        screen.update(Message::MarginStep(Margin::After, -1));
        assert_eq!(target(&screen), (at - 6000).unsigned_abs());
        screen.go_to_event(i);
        assert_eq!(screen.clip_end, Some(at.unsigned_abs() + 4000));
        // written: only digits, the field kept while empty
        screen.update(Message::MarginInput(Margin::After, String::new()));
        assert_eq!(screen.margins.after_s, 4);
        screen.update(Message::MarginInput(Margin::After, "1x2".into()));
        assert_eq!(screen.margins.after_s, 12);
        assert!(screen.focus_id().is_some());
        assert!(matches!(screen.key(BACK_KEY), Outcome::Stay));
        assert!(screen.focus_id().is_none());
        // the next time, as chosen
        let again = with_file(&dir, "20150125 000000.mp4");
        assert_eq!((again.margins.before_s, again.margins.after_s), (6, 12));
    }

    /// `w` (or the pill) asks for the own window and back; its window
    /// closed, the video is back in the screen.
    #[test]
    fn the_video_in_its_own_window() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "20150125 000000.mp4");
        assert!(!screen.detached());
        screen.key(key!(w));
        assert!(screen.detached());
        screen.update(Message::Detach);
        assert!(!screen.detached());
        screen.key(key!(w));
        screen.attach();
        assert!(!screen.detached());
    }

    /// Esc leaves the time being written, not the screen.
    #[test]
    fn esc_leaves_the_time_written() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "20150125 000000.mp4");
        screen.key(key!(t));
        assert!(matches!(screen.key(BACK_KEY), Outcome::Stay));
        assert!(screen.editing.is_none());
        assert!(matches!(screen.key(BACK_KEY), Outcome::Back));
    }

    /// The sync found by itself shifted becomes the user's; without any,
    /// nothing to shift.
    #[test]
    fn a_shift_without_a_sync_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "match.mp4");
        screen.key(key!(']'));
        assert!(screen.events.iter().all(|e| e.offset_ms.is_none()));
        let other = tempfile::tempdir().unwrap();
        let mut screen = with_file(&other, "20150125 000000.mp4");
        let before = screen.events[0].offset_ms.unwrap();
        screen.key(key!(']'));
        assert_eq!(screen.events[0].offset_ms, Some(before + 500));
        assert!(screen.anchored_by_hand());
    }

    /// A name without the time: the events not lined up.
    #[test]
    fn no_events_lined_up_without_the_start() {
        let dir = tempfile::tempdir().unwrap();
        let screen = with_file(&dir, "match.mp4");
        assert!(!screen.events.is_empty());
        assert!(screen.events.iter().all(|e| e.offset_ms.is_none()));
        assert_eq!(screen.event_target(0), None);
        let s = crate::strings::strings(scout4all_core::shapes::enums::LanguageEnum::It);
        assert_eq!(screen.alignment(s), (s.not_aligned.to_string(), false));
    }
}
