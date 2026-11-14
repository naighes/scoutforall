//! The video of a match (001): its file played in the app, Space plays and
//! pauses, ← → go back and forward a few seconds, `-` `+` change the speed,
//! the bar goes anywhere; `c` adds files, played one after the other as a
//! single video (in the order of their starts, then as moved). Beside it the events of the
//! match, at their time in the video when it lines up with them (`video`
//! in the core: DataVolley, or the start of the file from its name): ↑ ↓
//! choose one, Enter (or a click) goes a little before it. By hand (when
//! the clocks differ): `a` ties the event chosen to the moment of the
//! video, `t` to a time written (while they don't line up), `[` `]` shift
//! all by half a second, `r`
//! goes back to the sync found by itself. With the actions picked, their
//! clips on a strip under the bar: a click on one plays it (`strip`).
//! Once the events line up, an event can be put alone at its time (`t`, or
//! `e` at the moment of the video), the others where they are; Backspace
//! puts it back. The commands of the sync and of clip mode are in panels
//! over the video (`Panel`), opened by their buttons: the video keeps its
//! size. The profile (`video.json`) is saved next to the match.

mod files;
mod list;
mod margins;
mod montage;
mod player;
pub use player::Frame as Picture;
pub mod source;
mod strip;

use {
    crate::{
        strings::Strings,
        theme::{self, Colors, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, ContentFit, Element, Length,
        widget::{column, container, image, row, slider, space, stack, text},
    },
    iced_runtime::image::Allocation,
    margins::{Margin, Margins},
    player::{Player, Status},
    scout4all_core::{
        settings::VideoSeconds,
        shapes::{enums::PhaseEnum, r#match::MatchEntry},
        video::{
            self, Anchor, AnchorSource, EventRef, StoredProfile, TimeEffect, VideoFile,
            VideoProfile,
        },
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{
        MatchClips, MobileError, Scout4All, VideoEvent,
        strip::{ClipSpan, StripClick, clip_spans},
        video::{
            Piece, VideoStanding as Standing, current, event_ref, inside, parse_time, pieces,
            standing, target, uncorrected, written,
        },
    },
    std::{
        path::PathBuf,
        sync::Arc,
        time::{Duration, Instant},
    },
};

/// The fields of the seconds of a clip before and after its action.
const BEFORE_FIELD: &str = "video-clip-before";
const AFTER_FIELD: &str = "video-clip-after";

/// How far `[` and `]` shift the sync.
const SHIFT_MS: i64 = 500;

/// The width of a panel over the video.
const PANEL_WIDTH: f32 = 380.0;

/// The field of the time of the event.
const TIME_FIELD: &str = "video-anchor-time";

/// The speeds, `-` and `+` among them.
const SPEEDS: [f32; 4] = [0.5, 1.0, 1.5, 2.0];

/// After a command, the frames of the window keep coming this long: the
/// frame asked for is decoded meanwhile.
const AWAKE: Duration = Duration::from_millis(1500);

/// A panel over the video, opened by its button: the video keeps its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    /// how the events line up with the video, by hand
    Sync,
    /// clip mode and the seconds of each clip
    Clip,
}

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
    /// a panel over the video opened, or closed when it's the one open
    Panel(Panel),
    /// how the match stands over the video, or not
    Overlay,
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
    /// the event chosen alone at the moment of the video
    CorrectHere,
    /// the event chosen back where the sync puts it
    Uncorrect,
    /// the sync shifted, the events later in the video when positive
    Shift(i64),
    /// back to the sync found by itself
    Automatic,
    /// the video before the first event shown, to tie it again, or not
    WholeVideo,
    /// the time of the event chosen, to write
    EditTime,
    TimeInput(String),
    TimeSubmit,
    /// the actions one after the other, or each stopping
    Sequence,
    /// the actions picked made a video, to share
    Montage,
    /// the video of the actions of the team made, as summed up
    MakeMontage,
    /// the video not made, or no longer
    StopMontage,
    /// with the actions of a team, the match after (`true`) or before
    OtherMatch(bool),
    /// the seconds of the clip before or after its action, one more (`1`)
    /// or less
    MarginStep(Margin, i32),
    /// the seconds being written
    MarginInput(Margin, String),
    /// the seconds written, done
    MarginSubmit,
    /// the strip of the clips clicked
    Strip(StripClick),
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
    /// the video of the actions picked to make (`start_montage`)
    Montage,
    /// the video of the actions picked made: to share, what it shows
    Share(PathBuf, String),
}

/// The matches left out of the video of the actions of the team, and why;
/// none when none is.
fn left_out_said(left_out: &[(String, LeftOut)], s: &Strings) -> Option<String> {
    let said: Vec<String> = left_out
        .iter()
        .map(|(opponent, why)| {
            let why = match why {
                LeftOut::NoVideo => s.left_out_no_video,
                LeftOut::Unreadable => s.left_out_unreadable,
                LeftOut::NotLinedUp => s.left_out_not_lined_up,
            };
            format!("{opponent} ({why})")
        })
        .collect();
    (!said.is_empty()).then(|| ui::fill(s.montage_left_out, &[said.join(", ")]))
}

/// The piece of the video of an action, after its card: what it is, who,
/// the set and the score before it, which (`n`) of how many.
fn piece(p: &Piece, n: usize, count: usize, s: &Strings) -> montage::Piece {
    let e = p.event;
    let name = ui::capitalized(&e.event.event.name);
    let title = [Some(name), e.event.event.technique.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let title = match &e.event.event.symbol {
        Some(symbol) => format!("{title}  {symbol}"),
        None => title,
    };
    let which = match p.actions {
        0 | 1 => format!("{n} / {count}"),
        more => format!("{n} / {count}  (+{})", more - 1),
    };
    let lines = e
        .event
        .event
        .player
        .as_ref()
        .map(|p| format!("{} {}", p.number, p.name))
        .into_iter()
        .chain([
            format!(
                "{} {} · {}–{}",
                s.set_n, e.set_number, p.before.0, p.before.1
            ),
            which,
        ])
        .collect();
    montage::Piece {
        from_ms: p.from_ms,
        to_ms: p.to_ms,
        card: montage::Card { title, lines },
    }
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
    /// the events of the match, at their time in the video: the ones
    /// picked, with some
    events: Vec<VideoEvent>,
    /// all of them, for how the match stands over the video
    all_events: Vec<VideoEvent>,
    selected: usize,
    /// the list to scroll to the event chosen
    scroll: bool,
    /// the event chosen follows the video: off once chosen with the keys,
    /// on again when the video is moved or played
    follow: bool,
    /// the time of the event gone to (the video a little before it): that
    /// one stays chosen until the video reaches an event after it (also
    /// when the seek lands on the frame a little before, or the position
    /// read is still the old one)
    gone_to: Option<i64>,
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
    /// how far ← and → go, and how long before an event the video goes to
    /// it out of clip mode (the serve seen coming): from the settings
    seconds: VideoSeconds,
    /// the seconds of one of them being written
    margin_text: Option<(Margin, String)>,
    /// where the first event of the match is in the video, when lined up:
    /// the video before it (the warm-up) left out
    first_ms: Option<i64>,
    /// the video before the first event shown, to tie the events again
    whole: bool,
    /// the panel open over the video, if any
    panel: Option<Panel>,
    /// the video of the actions picked being made
    montage: Option<montage::Montage>,
    /// the video of the actions of the team, before making it
    summary: Option<Summary>,
    /// why the video of the actions picked wasn't made
    montage_error: Option<String>,
}

/// The matches of a team with actions picked in its statistics, each with
/// a video, and the one shown.
struct Tour {
    matches: Vec<MatchClips>,
    at: usize,
    /// the ones without a video, left out
    without_video: Vec<MatchClips>,
}

/// Why a match of the team is left out of the video of its actions.
#[derive(Debug, Clone, Copy, PartialEq)]
enum LeftOut {
    NoVideo,
    /// a file of its video not there, or not one to read
    Unreadable,
    /// the events not lined up with the video: no action in it
    NotLinedUp,
}

/// A match of the team in the video of its actions.
struct TourMatch {
    the_match: MatchEntry,
    day: String,
    files: Vec<PathBuf>,
    /// all its events in its video, for the score
    all: Vec<VideoEvent>,
    picked: Vec<VideoEvent>,
}

/// The video of the actions of a team, made once seen what's in it: the
/// actions and the matches, the ones left out and why.
struct Summary {
    plan: montage::Plan,
    actions: usize,
    left_out: Vec<(String, LeftOut)>,
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
        let seconds = core.video_seconds();
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
            all_events: vec![],
            selected: 0,
            scroll: false,
            follow: true,
            gone_to: None,
            clip: false,
            clip_end: None,
            detached: false,
            editing: None,
            invalid_time: false,
            clips: None,
            sequence: true,
            tour: None,
            margins: Margins::default(),
            seconds,
            margin_text: None,
            first_ms: None,
            whole: false,
            panel: None,
            montage: None,
            summary: None,
            montage_error: None,
        };
        screen.margins = Margins::read(&screen.dir);
        // the durations of the files first: the events in them
        screen.open_player();
        screen.load_events()?;
        screen.after_start();
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
    /// (`matches`, the ones with a video watched): the first one played at
    /// once.
    pub fn with_tour(
        core: Arc<Scout4All>,
        dir: PathBuf,
        team_id: String,
        matches: Vec<MatchClips>,
        title: String,
    ) -> Result<VideoScreen, MobileError> {
        let (matches, without_video): (Vec<MatchClips>, Vec<MatchClips>) =
            matches.into_iter().partition(|m| m.has_video);
        let first = matches
            .first()
            .cloned()
            .ok_or_else(|| MobileError::Failed {
                reason: "no match with actions to watch".to_string(),
            })?;
        let mut screen =
            VideoScreen::with_clips(core, dir, team_id, first.match_id, first.events, title)?;
        screen.tour = Some(Tour {
            matches,
            at: 0,
            without_video,
        });
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
                next.tour = Some(Tour { at, ..tour });
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
        self.first_ms = self.events.iter().filter_map(|e| e.offset_ms).min();
        self.all_events = self.events.clone();
        if let Some(clips) = &self.clips {
            self.events.retain(|e| clips.events.contains(&event_ref(e)));
        }
        self.selected = self.selected.min(self.events.len().saturating_sub(1));
        Ok(())
    }

    /// What the actions picked are, as the statistics named them.
    fn clips_title(&self) -> String {
        self.clips
            .as_ref()
            .map(|c| c.title.clone())
            .unwrap_or_default()
    }

    /// The video of the actions picked of this match: their pieces, each
    /// from its margin before to its margin after, the ones overlapping as
    /// one (the card of the first, the others counted), as on Android.
    fn match_plan(&self, s: &Strings) -> montage::MatchPlan {
        let picked: Vec<&VideoEvent> = self.events.iter().collect();
        let pieces = pieces(
            &picked,
            &self.all_events,
            self.margins.before_s,
            self.margins.after_s,
        );
        let count = pieces.len();
        montage::MatchPlan {
            files: self.paths(),
            pieces: pieces
                .iter()
                .enumerate()
                .map(|(i, p)| piece(p, i + 1, count, s))
                .collect(),
            board: self.board(&self.the_match, self.all_events.clone(), s),
            title: None,
        }
    }

    /// How the match stands over the video made, if shown.
    fn board(
        &self,
        the_match: &MatchEntry,
        events: Vec<VideoEvent>,
        s: &Strings,
    ) -> Option<montage::Board> {
        self.margins.overlay.then(|| montage::Board {
            team: the_match.team.name.clone(),
            opponent: the_match.opponent.clone(),
            set_n: s.set_n,
            side_out: s.side_out_phase,
            break_point: s.break_phase,
            at: Box::new(move |ms| standing(&events, ms)),
        })
    }

    /// The video of the actions of each match of the team, one after the
    /// other, each after its card, the actions counted across all; the
    /// matches without a video, with its files gone or not lined up left
    /// out.
    fn tour_summary(&self, tour: &Tour, s: &Strings) -> Summary {
        let mut left_out: Vec<(String, LeftOut)> = tour
            .without_video
            .iter()
            .map(|m| (m.opponent.clone(), LeftOut::NoVideo))
            .collect();
        let mut kept = vec![];
        for m in &tour.matches {
            match self.tour_match(m) {
                Ok(t) if self.tour_pieces(&t).is_empty() => {
                    left_out.push((m.opponent.clone(), LeftOut::NotLinedUp));
                }
                Ok(t) => kept.push(t),
                Err(why) => left_out.push((m.opponent.clone(), why)),
            }
        }
        let count: usize = kept.iter().map(|t| self.tour_pieces(t).len()).sum();
        let mut n = 0;
        let mut actions = 0;
        let matches = kept
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let pieces = self.tour_pieces(t);
                let of_match: usize = pieces.iter().map(|p| p.actions as usize).sum();
                actions += of_match;
                montage::MatchPlan {
                    files: t.files.clone(),
                    title: Some(montage::Card {
                        title: t.the_match.opponent.clone(),
                        lines: vec![
                            s.day(&t.day),
                            format!(
                                "{} · {}",
                                ui::fill(
                                    s.clips_match_of,
                                    &[(i + 1).to_string(), kept.len().to_string()]
                                ),
                                ui::fill(s.clips_count, &[of_match.to_string()]),
                            ),
                        ],
                    }),
                    pieces: pieces
                        .iter()
                        .map(|p| {
                            n += 1;
                            piece(p, n, count, s)
                        })
                        .collect(),
                    board: self.board(&t.the_match, t.all.clone(), s),
                }
            })
            .collect();
        Summary {
            plan: montage::Plan {
                matches,
                output: montage::output(&self.clips_title()),
            },
            actions,
            left_out,
        }
    }

    /// The pieces of the actions picked of a match of the team.
    fn tour_pieces<'a>(&self, t: &'a TourMatch) -> Vec<Piece<'a>> {
        let picked: Vec<&VideoEvent> = t.picked.iter().collect();
        pieces(&picked, &t.all, self.margins.before_s, self.margins.after_s)
    }

    /// A match of the team: its files, all its events in its video and the
    /// ones picked; or why it's left out.
    fn tour_match(&self, m: &MatchClips) -> Result<TourMatch, LeftOut> {
        let (the_match, stored) = self
            .core
            .video(&self.team_id, &m.match_id)
            .map_err(|_| LeftOut::Unreadable)?;
        let profile = match stored {
            StoredProfile::Profile(p) if !p.files.is_empty() => p,
            StoredProfile::Missing | StoredProfile::Profile(_) => return Err(LeftOut::NoVideo),
            StoredProfile::Newer(_) => return Err(LeftOut::Unreadable),
        };
        let files: Vec<PathBuf> = profile
            .files
            .iter()
            .map(|f| video::resolved_path(&self.dir, &f.path))
            .collect();
        let durations = files
            .iter()
            .map(|f| source::Source::open(f).map(|s| s.duration_ms))
            .collect::<Result<Vec<u64>, String>>()
            .map_err(|_| LeftOut::Unreadable)?;
        let all = self
            .core
            .video_events(&the_match, &profile, &durations)
            .map_err(|_| LeftOut::Unreadable)?;
        let picked = all
            .iter()
            .filter(|e| m.events.contains(&event_ref(e)))
            .cloned()
            .collect();
        Ok(TourMatch {
            the_match,
            day: m.day.clone(),
            files,
            all,
            picked,
        })
    }

    /// The video of the actions picked to make, when some are and it isn't
    /// being made; once summed up, made.
    fn montage_asked(&mut self) -> Outcome {
        match (&self.clips, &self.montage, &self.summary) {
            (Some(_), None, None) => Outcome::Montage,
            (Some(_), None, Some(_)) => {
                self.make_montage();
                Outcome::Stay
            }
            _ => Outcome::Stay,
        }
    }

    /// The video of the actions picked: of this match made at once on its
    /// thread; of the matches of a team summed up first (what's in, what's
    /// left out), made once asked (`make_montage`).
    pub fn start_montage(&mut self, s: &'static Strings) {
        self.montage_error = None;
        let several = self
            .tour
            .as_ref()
            .filter(|t| t.matches.len() + t.without_video.len() > 1);
        match several {
            Some(tour) => self.summary = Some(self.tour_summary(tour, s)),
            None => {
                let plan = montage::Plan {
                    matches: vec![self.match_plan(s)],
                    output: montage::output(&self.clips_title()),
                };
                self.make(plan);
            }
        }
    }

    /// The video of the actions of the team made, as summed up.
    fn make_montage(&mut self) {
        if let Some(summary) = self.summary.take() {
            self.make(summary.plan);
        }
    }

    fn make(&mut self, plan: montage::Plan) {
        if plan.matches.iter().all(|m| m.pieces.is_empty()) {
            return;
        }
        if let Err(e) = montage::prepare(&plan.output) {
            self.montage_error = Some(e);
            return;
        }
        self.montage = Some(montage::Montage::start(plan));
    }

    /// How the match stands at the moment of the video: from all its
    /// events, not only the ones picked.
    fn standing(&self) -> Option<Standing> {
        standing(&self.all_events, self.position())
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
            false => self.seconds.lead_ms(),
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

    /// How the match stands over the video, or not: kept for the next
    /// time.
    fn toggle_overlay(&mut self) {
        self.margins.overlay = !self.margins.overlay;
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
    /// The event seen is the last whose time the video reached: at its time,
    /// not before (going to it, the video starts a little before).
    fn follow_video(&mut self, position_ms: u64) {
        let seen = current(&self.events, position_ms, 0);
        let offset = |i: usize| self.events.get(i).and_then(|e| e.offset_ms);
        let past = |i: usize, gone: i64| offset(i).is_some_and(|t| t > gone);
        let seen = match (self.gone_to, seen) {
            (Some(gone), Some(i)) if past(i, gone) => {
                self.gone_to = None;
                Some(i)
            }
            (Some(_), _) => None,
            (None, seen) => seen,
        };
        // the one chosen at the same time as the one seen stays
        let same_time = seen.is_some_and(|i| offset(i) == offset(self.selected));
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

    /// The profile changed saved: the events lined up again.
    fn save_profile(&mut self, change: impl FnOnce(&VideoProfile) -> VideoProfile) {
        let Some(profile) = &self.profile else {
            return;
        };
        let profile = change(profile);
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

    /// The profile changed for the event chosen, saved.
    fn save_for_event(&mut self, change: impl FnOnce(&VideoProfile, &VideoEvent) -> VideoProfile) {
        if let Some(e) = self.events.get(self.selected).cloned() {
            self.save_profile(|profile| change(profile, &e));
        }
    }

    /// The video tied to the event chosen at `ms`: all of them move.
    fn anchor_event(&mut self, ms: u64) {
        self.save_for_event(|profile, e| profile.tied_to(event_ref(e), e.timestamp, millis(ms)));
        self.whole = false;
    }

    /// The event chosen alone at `ms`, the others where they are.
    fn correct_event(&mut self, ms: u64) {
        self.save_for_event(|profile, e| match uncorrected(e) {
            Some(placed) => profile.corrected(event_ref(e), millis(ms) - placed),
            None => profile.clone(),
        });
    }

    /// The event chosen back where the sync puts it.
    fn uncorrect_event(&mut self) {
        self.save_for_event(|profile, e| profile.corrected(event_ref(e), 0));
    }

    /// What a time for the event chosen does: it alone once the events
    /// line up, the video tied to it while they don't; none without events.
    fn time_effect(&self) -> Option<TimeEffect> {
        self.events.get(self.selected)?;
        Some(TimeEffect::of(self.chosen_placed()))
    }

    /// Where the sync puts the event chosen, its correction aside: none
    /// while the events don't line up.
    fn chosen_placed(&self) -> Option<i64> {
        self.events.get(self.selected).and_then(uncorrected)
    }

    /// How far the event chosen was put from where the sync puts it.
    fn chosen_correction(&self) -> Option<i64> {
        self.events.get(self.selected).and_then(|e| e.corrected_ms)
    }

    /// Back to the sync found by itself: no anchor, no correction.
    fn automatic(&mut self) {
        self.save_profile(VideoProfile::automatic);
        self.after_start();
    }

    /// All the events shifted by `ms` in the video: the sync found by
    /// itself becomes the user's.
    fn shift(&mut self, ms: i64) {
        let current = self
            .profile
            .as_ref()
            .and_then(|p| video::anchor(p, &self.the_match));
        if let Some(anchor) = current {
            self.save_profile(|profile| VideoProfile {
                anchor: Some(Anchor {
                    offset_ms: anchor.offset_ms + ms,
                    ..anchor
                }),
                ..profile.clone()
            });
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
            self.editing = Some(written(now));
        }
    }

    fn submit_time(&mut self) {
        match self.editing.as_deref().map(parse_time) {
            Some(Some(ms)) => {
                self.editing = None;
                self.invalid_time = false;
                self.save_for_event(|profile, e| {
                    profile.with_time(event_ref(e), e.timestamp, uncorrected(e), millis(ms))
                });
                self.whole = false;
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
            self.gone_to = self.events.get(self.selected).and_then(|e| e.offset_ms);
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
                self.events
                    .get(*i)
                    .is_some_and(|e| inside(e.offset_ms, duration) && self.event_target(*i) != seen)
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

    fn toggle_panel(&mut self, panel: Panel) {
        self.panel = match self.panel {
            Some(open) if open == panel => None,
            _ => Some(panel),
        };
        self.editing = None;
        self.invalid_time = false;
        self.margin_text = None;
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
        self.montage.is_some()
            || self.player.is_some()
                && (self.status.as_ref().is_some_and(|s| s.playing)
                    || Instant::now() < self.awake_until)
    }

    fn wake(&mut self) {
        self.awake_until = Instant::now() + AWAKE;
    }

    /// The frame of the player, to allocate once it's a new one.
    fn tick(&mut self) -> Outcome {
        if let Some(made) = self.montage.as_ref().and_then(|m| m.progress().result) {
            self.montage = None;
            match made {
                Ok(path) => return Outcome::Share(path, self.clips_title()),
                Err(e) => self.montage_error = Some(e),
            }
        }
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
        let step = self.seconds.step_ms();
        let to = match forward {
            true => self.position() + step,
            false => self.position().saturating_sub(step),
        };
        self.seek(to);
    }

    /// Where the video starts: a little before the first event of the
    /// match once lined up (the warm-up left out), unless the whole video
    /// is asked for, to tie the events again.
    fn start_ms(&self) -> u64 {
        match (self.whole, self.first_ms) {
            (false, Some(first)) => (first - self.lead_ms()).max(0).unsigned_abs(),
            _ => 0,
        }
    }

    /// The video before its start taken to it.
    fn after_start(&mut self) {
        let start = self.start_ms();
        if self.player.is_some() && self.position() < start {
            self.seek(start);
        }
    }

    fn seek(&mut self, ms: u64) {
        let ms = ms.max(self.start_ms());
        self.follow = true;
        self.gone_to = None;
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
                self.after_start();
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
            Message::Panel(panel) => self.toggle_panel(panel),
            Message::Overlay => self.toggle_overlay(),
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
            Message::CorrectHere => self.correct_event(self.position()),
            Message::Uncorrect => self.uncorrect_event(),
            Message::Shift(ms) => self.shift(ms),
            Message::Automatic => self.automatic(),
            Message::WholeVideo => {
                self.whole = !self.whole;
                self.after_start();
            }
            Message::EditTime => self.edit_time(),
            Message::TimeInput(text) => {
                self.invalid_time = false;
                self.editing = Some(text);
            }
            Message::TimeSubmit => self.submit_time(),
            Message::Sequence => self.sequence = !self.sequence,
            Message::Montage => return self.montage_asked(),
            Message::MakeMontage => self.make_montage(),
            Message::StopMontage => {
                self.montage = None;
                self.summary = None;
            }
            Message::OtherMatch(after) => {
                if let Some(at) = self.other_match(after) {
                    self.open_match(at);
                }
            }
            Message::MarginStep(margin, by) => self.step_margin(margin, by),
            Message::MarginInput(margin, text) => self.write_margin(margin, text),
            Message::MarginSubmit => self.margin_text = None,
            Message::Strip(StripClick::Clip { event }) => self.go_to_event(event as usize),
            Message::Strip(StripClick::At { ms }) => self.seek(ms),
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
        match (ck, self.panel) {
            (k, Some(_)) if k == BACK_KEY => {
                self.panel = None;
                return Outcome::Stay;
            }
            _ => {}
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
            k if k == key!(t) => {
                self.edit_time();
                if self.editing.is_some() {
                    self.panel = Some(Panel::Sync);
                }
            }
            k if k == key!('[') => self.shift(-SHIFT_MS),
            k if k == key!(']') => self.shift(SHIFT_MS),
            k if k == key!(r) => self.automatic(),
            k if k == key!(e) && self.chosen_placed().is_some() => {
                self.correct_event(self.position());
            }
            k if (k == key!(backspace) || k == key!(delete))
                && self.chosen_correction().is_some() =>
            {
                self.uncorrect_event();
            }
            k if k == key!(k) => self.toggle_clip(),
            k if k == key!(i) => self.toggle_overlay(),
            k if k == key!(w) => self.detached = !self.detached,
            k if k == key!(s) && self.clips.is_some() => self.sequence = !self.sequence,
            k if k == key!(x) => return self.montage_asked(),
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
    /// screen (the speed `-` `+`, the file `c` once chosen, `r`, `e` and
    /// Backspace, said in the tips of their buttons) left out,
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
                hint("← →", format!("±{} s", self.seconds.step_s)),
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
            (Some(_), Some(_)) => vec![
                hint("s", s.in_sequence.to_string()),
                hint("x", s.share_video.to_string()),
            ],
            _ => vec![],
        };
        let matches = match &self.tour {
            Some(tour) if tour.matches.len() > 1 => {
                vec![hint("PgUp PgDn", s.other_match.to_string())]
            }
            _ => vec![],
        };
        let window = match (self.events.is_empty(), &self.player) {
            (false, Some(_)) => vec![
                hint("i", s.match_overlay.to_string()),
                hint(
                    "w",
                    match self.detached {
                        true => s.attach_video,
                        false => s.detach_video,
                    }
                    .to_string(),
                ),
            ],
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
        let screen: Element<'a, Message> = match self.panel_view(s, c) {
            Some(panel) => stack![black(screen), panel].into(),
            None => black(screen).into(),
        };
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
            .map(|m| s.day(&m.day))
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
            Some(AnchorSource::FileStart(start)) => {
                ui::fill(s.aligned_file, &[s.date_time(&start)])
            }
            None => s.not_aligned.to_string(),
        };
        let duration = self.status.as_ref().map(|st| st.duration_ms);
        let outside = self
            .events
            .iter()
            .filter(|e| e.offset_ms.is_some() && !inside(e.offset_ms, duration))
            .count();
        let said = match profile.corrections.len() {
            0 => said,
            n => format!(
                "{said} · {}",
                ui::fill(s.corrections_count, &[n.to_string()])
            ),
        };
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
        ]
        .spacing(10)
        .height(Length::Fill)
        .into()
    }

    /// The frame shown, as large as it fits.
    fn frame_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        match (&self.shown, &self.player) {
            (Some(shown), _) => {
                let frame = image(shown.handle.clone())
                    .content_fit(ContentFit::Contain)
                    .width(Length::Fill)
                    .height(Length::Fill);
                match self.standing().filter(|_| self.margins.overlay) {
                    Some(st) => stack![
                        frame,
                        container(overlay_view(st, &self.the_match, s, c))
                            .padding(16)
                            .width(Length::Fill)
                            .height(Length::Fill)
                    ]
                    .into(),
                    None => frame.into(),
                }
            }
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

    /// The panel open over the video, on its right: its title, a button to
    /// close it, what it holds; none when closed or with nothing to hold.
    fn panel_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Option<Element<'a, Message>> {
        let (panel, title, body) = match (self.panel?, self.events.is_empty(), &self.clips) {
            (Panel::Sync, false, None) => (Panel::Sync, s.sync_title, self.sync_view(s, c)),
            (Panel::Sync, _, _) => return None,
            (Panel::Clip, _, _) => (Panel::Clip, s.clip, self.clip_view(s, c)),
        };
        let head = row![
            text(title)
                .size(theme::SIZE_TITLE)
                .font(theme::fonts().bold)
                .width(Length::Fill),
            ui::tip(
                ui::pill("×", false, Some(Message::Panel(panel)), c),
                s.close_panel,
                c
            ),
        ]
        .align_y(Alignment::Center);
        let card = ui::panel(
            None,
            iced::widget::scrollable(column![head, body].spacing(14).padding([0, 12])),
            c,
        );
        Some(
            container(iced::widget::opaque(
                container(card).width(Length::Fixed(PANEL_WIDTH)),
            ))
            .padding(12)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::End)
            .align_y(Alignment::Start)
            .into(),
        )
    }

    /// Clip mode on or off, what it does, the seconds of each clip.
    fn clip_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let toggle: Element<'a, Message> = match self.clips {
            // the actions picked play as clips anyway
            Some(_) => space::vertical().height(0).into(),
            None => column![
                ui::pill(s.clip_on, self.clip, Some(Message::Clip), c),
                text(ui::fill(
                    s.clip_explained,
                    &[self.margins.after_s.to_string()]
                ))
                .size(SIZE_SMALL)
                .color(c.muted),
            ]
            .spacing(6)
            .into(),
        };
        column![toggle, self.margins_view(s, c)].spacing(14).into()
    }

    /// The sync by hand: how the events line up, then what moves them all
    /// and what moves only the event chosen.
    fn sync_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let explained = |what: &str| text(what.to_string()).size(SIZE_SMALL).color(c.muted);
        let nothing = || -> Element<'a, Message> { space::vertical().height(0).into() };
        let (said, fine) = self.alignment(s);
        let automatic: Element<'a, Message> = match self.anchored_by_hand()
            || self
                .profile
                .as_ref()
                .is_some_and(|p| !p.corrections.is_empty())
        {
            true => column![
                ui::pill(s.automatic_sync, false, Some(Message::Automatic), c),
                explained(s.automatic_sync_explained),
            ]
            .spacing(4)
            .into(),
            false => nothing(),
        };
        let status = column![
            text(said).size(SIZE_SMALL).color(match fine {
                true => c.muted,
                false => c.danger,
            }),
            automatic,
        ]
        .spacing(8);
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
                match (self.invalid_time, self.time_effect()) {
                    (true, _) => text(s.invalid_time).size(SIZE_SMALL).color(c.danger),
                    (false, Some(TimeEffect::OnlyThis)) => {
                        text(s.time_only_this).size(SIZE_SMALL).color(c.muted)
                    }
                    (false, _) => text(s.time_aligns_all).size(SIZE_SMALL).color(c.muted),
                },
            ]
            .spacing(4)
            .into(),
            None => ui::pill(s.anchor_time, false, Some(Message::EditTime), c),
        };
        let placed = self.chosen_placed();
        let lined_up = self
            .profile
            .as_ref()
            .and_then(|p| video::anchor(p, &self.the_match))
            .is_some();
        // the whole match: the video tied to the event chosen, all shifted,
        // the video before the first event to find it again
        let here: Element<'a, Message> = match self.player {
            Some(_) => ui::pill(s.anchor_here, true, Some(Message::AnchorHere), c),
            None => nothing(),
        };
        // the time written ties the video while the events don't line up
        let (time_whole, time_event): (Element<'a, Message>, Element<'a, Message>) = match placed {
            Some(_) => (nothing(), time),
            None => (time, nothing()),
        };
        let here_explained: Element<'a, Message> = match self.player {
            Some(_) => explained(s.anchor_here_explained).into(),
            None => nothing(),
        };
        let half = SHIFT_MS as f64 / 1000.0;
        let shift: Element<'a, Message> = match lined_up {
            true => column![
                text(s.shift_sync).size(SIZE_SMALL),
                row![
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
                .align_y(Alignment::Center)
                .wrap(),
                explained(s.shift_sync_explained),
            ]
            .spacing(4)
            .into(),
            false => nothing(),
        };
        let redo: Element<'a, Message> = match (self.player.is_some(), self.first_ms) {
            (true, Some(first)) if first > 0 => column![
                ui::pill(s.redo_anchor, self.whole, Some(Message::WholeVideo), c),
                explained(s.redo_anchor_explained),
            ]
            .spacing(4)
            .into(),
            _ => nothing(),
        };
        let whole = column![
            ui::section(s.sync_whole, c),
            row![here, time_whole]
                .spacing(8)
                .align_y(Alignment::Center)
                .wrap(),
            here_explained,
            shift,
            redo,
        ]
        .spacing(8);
        // only the event chosen: put at the moment of the video or at a
        // time written, and back
        let only: Element<'a, Message> = match (&self.player, placed) {
            (Some(_), Some(_)) => ui::pill(s.correct_here, false, Some(Message::CorrectHere), c),
            _ => nothing(),
        };
        let corrected: Element<'a, Message> = match self.chosen_correction() {
            Some(by) => row![
                text(ui::fill(s.corrected_by, &[list::seconds(by)]))
                    .size(SIZE_SMALL)
                    .color(c.muted),
                ui::tip(
                    ui::pill(s.uncorrect, false, Some(Message::Uncorrect), c),
                    "⌫",
                    c,
                ),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .wrap()
            .into(),
            None => nothing(),
        };
        let event_explained: Element<'a, Message> = match placed {
            Some(_) => explained(s.correct_here_explained).into(),
            None => explained(s.sync_event_unplaced).into(),
        };
        let event = column![
            ui::section(s.sync_event, c),
            row![only, time_event]
                .spacing(8)
                .align_y(Alignment::Center)
                .wrap(),
            event_explained,
            corrected,
        ]
        .spacing(8);
        column![status, whole, event].spacing(18).into()
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
        let step = self.seconds.step_s.to_string();
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
            self.spans_view(status.duration_ms, c),
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
                ui::tip(
                    ui::pill(
                        s.match_overlay,
                        self.margins.overlay,
                        Some(Message::Overlay),
                        c
                    ),
                    s.match_overlay_explained,
                    c
                ),
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
                match (self.events.is_empty(), &self.clips) {
                    (false, None) => ui::pill(
                        s.sync_title,
                        self.panel == Some(Panel::Sync),
                        Some(Message::Panel(Panel::Sync)),
                        c,
                    ),
                    _ => space::horizontal().width(0).into(),
                },
                ui::pill(
                    &match self.clip || self.clips.is_some() {
                        true => format!("{} ✓", s.clip),
                        false => s.clip.to_string(),
                    },
                    self.panel == Some(Panel::Clip),
                    Some(Message::Panel(Panel::Clip)),
                    c
                ),
                text(s.speed).size(SIZE_SMALL).color(c.muted),
            ]
            .extend(speeds)
            .spacing(10)
            .align_y(Alignment::Center),
            self.montage_view(s, c),
        ]
        .spacing(8)
        .into()
    }

    /// What the video of the actions of the team holds before making it:
    /// how many actions of how many matches, the ones left out and why.
    fn summary_view<'a>(
        &'a self,
        summary: &'a Summary,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let held = ui::fill(
            s.clips_in_matches,
            &[
                summary.actions.to_string(),
                summary.plan.matches.len().to_string(),
            ],
        );
        let make = ui::pill(
            s.make_video,
            true,
            (summary.actions > 0).then_some(Message::MakeMontage),
            c,
        );
        let line = row![
            text(held)
                .size(SIZE_SMALL)
                .font(theme::fonts().bold)
                .color(c.accent),
            make,
            ui::pill(s.cancel, false, Some(Message::StopMontage), c),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let left_out: Element<'a, Message> = match left_out_said(&summary.left_out, s) {
            Some(said) => text(said).size(SIZE_SMALL).color(c.muted).into(),
            None => space::vertical().height(0).into(),
        };
        column![line, left_out].spacing(4).into()
    }

    /// With the actions picked: their video to share, or how far it got
    /// being made, or why it wasn't.
    fn montage_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        if let (Some(_), None, Some(summary)) = (&self.clips, &self.montage, &self.summary) {
            return self.summary_view(summary, s, c);
        }
        let line: Element<'a, Message> = match (&self.clips, &self.montage) {
            (None, _) => return space::vertical().height(0).into(),
            (Some(_), Some(montage)) => row![
                text(ui::fill(
                    s.making_video,
                    &[montage.progress().percent().to_string()]
                ))
                .size(SIZE_SMALL)
                .font(theme::fonts().bold)
                .color(c.accent),
                ui::pill(s.cancel, false, Some(Message::StopMontage), c),
            ]
            .spacing(10)
            .align_y(Alignment::Center)
            .into(),
            (Some(_), None) => ui::action(
                theme::icons::SHARE,
                s.share_video,
                Message::Montage,
                false,
                c,
            )
            .into(),
        };
        let error: Element<'a, Message> = match &self.montage_error {
            Some(e) => text(format!("{}: {e}", s.video_not_made))
                .size(SIZE_SMALL)
                .color(c.danger)
                .into(),
            None => space::horizontal().width(0).into(),
        };
        row![line, error]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
    }

    /// With the actions picked, where their clips are in the video, from
    /// its start: the ones overlapping as one, in order. The one playing
    /// told apart (`clip_spans`, the same on Android).
    fn spans(&self) -> Vec<ClipSpan> {
        match &self.clips {
            Some(_) => clip_spans(&self.events, self.lead_ms(), self.after_ms(), self.selected),
            None => vec![],
        }
    }

    /// The clips of the actions picked, under the bar of the video, to
    /// click (`strip`).
    fn spans_view<'a>(&self, duration_ms: u64, c: Colors) -> Element<'a, Message> {
        let spans = self.spans();
        if spans.is_empty() || duration_ms == 0 {
            return space::vertical().height(0).into();
        }
        let strip = iced::widget::canvas(strip::Strip {
            spans,
            duration_ms,
            colors: c,
        })
        .width(Length::Fill)
        .height(Length::Fixed(strip::HEIGHT));
        // the handle of the bar goes from half its width to the other end
        container(Element::from(strip).map(Message::Strip))
            .padding(iced::Padding::from([0.0, HANDLE_HALF]))
            .width(Length::Fill)
            .into()
    }

    /// The seconds of each clip before and after its action: written, or
    /// one more or less.
    fn margins_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
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
        column![
            ui::section(s.clip_margins_explained, c),
            field(Margin::Before, BEFORE_FIELD, s.clip_before),
            field(Margin::After, AFTER_FIELD, s.clip_after),
        ]
        .spacing(8)
        .into()
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

/// Half the width of the handle of the bar of the video (iced's default):
/// where its start and its end are.
const HANDLE_HALF: f32 = 7.0;

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

/// How the match stands, over the corner of the video: on a veil, not a
/// box, the letters as large as the body's or more. A line a team, ours
/// first, as on TV: the ball by the one to serve, its name, the sets won
/// and the points; beside them the set, the phase in its color and with
/// its icon, as while scouting, and the rotation.
fn overlay_view<'a>(
    st: Standing,
    m: &MatchEntry,
    s: &'static Strings,
    c: Colors,
) -> Element<'a, Message> {
    let white = iced::Color::WHITE;
    let soft = iced::Color { a: 0.8, ..white };
    let (color, icon, name) = match st.phase {
        PhaseEnum::Break => (c.break_point, theme::icons::BREAK, s.break_phase),
        PhaseEnum::SideOut => (c.side_out, theme::icons::SIDE_OUT, s.side_out_phase),
    };
    let bold = theme::fonts().bold;
    let names = [overlay_name(&m.team.name), overlay_name(&m.opponent)];
    // the font is monospaced: the names, the sets and the points in
    // columns, as long as the longer
    let letters = names.iter().map(|n| n.chars().count()).max().unwrap_or(0);
    let letter = theme::SIZE_BODY * 0.6;
    let score_letter = (theme::SIZE_TITLE + 4.0) * 0.6;
    let team = |serving: bool, name: String, sets: u8, points: u8| {
        let ball: Element<'a, Message> = match serving {
            true => ui::icon(theme::icons::VOLLEYBALL, 16.0, white).into(),
            false => space::horizontal().width(Length::Fixed(16.0)).into(),
        };
        row![
            container(ball).width(Length::Fixed(16.0)),
            text(name)
                .size(theme::SIZE_BODY)
                .font(bold)
                .color(white)
                .width(Length::Fixed(letter * letters as f32 + 2.0)),
            text(sets.to_string())
                .size(theme::SIZE_BODY)
                .color(soft)
                .width(Length::Fixed(letter * 2.0))
                .align_x(Alignment::End),
            text(points.to_string())
                .size(theme::SIZE_TITLE + 4.0)
                .font(bold)
                .color(white)
                .width(Length::Fixed(score_letter * 2.0 + 2.0))
                .align_x(Alignment::End),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
    };
    let [us, them] = names;
    let serving_us = st.phase == PhaseEnum::Break;
    let divider = container(space::vertical().height(Length::Fixed(44.0)))
        .width(Length::Fixed(1.0))
        .style(move |_| container::Style {
            background: Some(iced::Color { a: 0.35, ..white }.into()),
            ..container::Style::default()
        });
    container(
        row![
            column![
                team(serving_us, us, st.sets_us, st.score_us),
                team(!serving_us, them, st.sets_them, st.score_them),
            ]
            .spacing(2),
            divider,
            column![
                text(format!("{} {}", s.set_n, st.set_number))
                    .size(theme::SIZE_BODY)
                    .color(soft),
                row![
                    ui::icon(icon, 18.0, color),
                    text(name).size(theme::SIZE_BODY).font(bold).color(color),
                    ui::tip(
                        text(st.rotation)
                            .size(theme::SIZE_BODY)
                            .font(bold)
                            .color(white),
                        s.rotation,
                        c
                    ),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(4),
        ]
        .spacing(14)
        .align_y(Alignment::Center),
    )
    .padding(iced::Padding::from([8, 14]))
    .style(move |_| container::Style {
        background: Some(
            iced::Color {
                a: 0.55,
                ..iced::Color::BLACK
            }
            .into(),
        ),
        border: iced::Border {
            radius: 10.0.into(),
            ..iced::Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// The name of a team over the video: whole up to a point, then cut, so
/// that the corner stays a corner.
fn overlay_name(name: &str) -> String {
    const ROOM: usize = 22;
    match name.chars().count() > ROOM {
        true => format!(
            "{}…",
            name.chars().take(ROOM - 1).collect::<String>().trim_end()
        ),
        false => name.to_string(),
    }
}

pub use scout4all_mobile::video::video_clock as clock;

fn millis(ms: u64) -> i64 {
    i64::try_from(ms).unwrap_or(i64::MAX)
}

#[cfg(test)]
pub(crate) mod screen_tests {
    use {
        super::*,
        scout4all_core::video::{offset_ms, start_from_name},
        scout4all_mobile::{DvwChoice, DvwOpponent, DvwSide},
    };

    /// How long before an event the video goes to it, by default.
    const LEAD_MS: i64 = 2000;

    /// The match of the tests of the TUI, three sets, imported: the file
    /// has no video column, the events are on the clock.
    pub(crate) fn imported(dir: &tempfile::TempDir) -> (Arc<Scout4All>, String, String) {
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let bytes = include_bytes!("../../../core/src/tests/fixtures/three_sets.dvw").to_vec();
        let choice = DvwChoice {
            opponent: DvwOpponent::Name,
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

    /// The time written for the event chosen, as with the field.
    fn write_time(screen: &mut VideoScreen, written: &str) {
        screen.update(Message::EditTime);
        screen.update(Message::TimeInput(written.into()));
        screen.update(Message::TimeSubmit);
    }

    /// The case of the user: the serve at 0:00 ties the video; the block
    /// written at 0:06 moves alone, the serve stays at 0:00 (it went to
    /// before the video, the block tied in its place); `e` at the moment of
    /// the video, Backspace back; once lined up a time written moves only
    /// its event, the one tied too; automatic without corrections.
    #[test]
    fn the_first_time_ties_the_video_then_each_event_alone() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "match.mp4");
        let s = crate::strings::strings(scout4all_core::shapes::enums::LanguageEnum::It);
        assert!(screen.events.iter().all(|e| e.offset_ms.is_none()));
        write_time(&mut screen, "0:00.0");
        assert_eq!(screen.events[0].offset_ms, Some(0));
        let block = screen
            .events
            .iter()
            .position(|e| e.offset_ms > Some(6_000))
            .unwrap();
        let placed: Vec<Option<i64>> = screen.events.iter().map(|e| e.offset_ms).collect();
        screen.go_to_event(block);
        assert_eq!(screen.time_effect(), Some(TimeEffect::OnlyThis));
        write_time(&mut screen, "0:6.0");
        assert_eq!(screen.events[block].offset_ms, Some(6_000));
        assert_eq!(screen.events[0].offset_ms, Some(0), "the serve stays");
        assert_eq!(screen.anchored(), Some(event_ref(&screen.events[0])));
        assert_eq!(screen.events[block + 1].offset_ms, placed[block + 1]);
        assert_eq!(screen.chosen_correction(), placed[block].map(|p| 6_000 - p));
        let (said, _) = screen.alignment(s);
        assert!(
            said.contains(&ui::fill(s.corrections_count, &["1".into()])),
            "{said}"
        );
        // back where the video puts it
        screen.key(key!(backspace));
        assert_eq!(screen.events[block].offset_ms, placed[block]);
        assert_eq!(screen.chosen_correction(), None);
        // `e`: at the moment of the video (no file here: its start)
        screen.key(key!(e));
        assert_eq!(screen.events[block].offset_ms, Some(0));
        // the serve chosen: its time moves all, the correction with them
        screen.key(key!(up));
        while screen.selected > 0 {
            screen.key(key!(up));
        }
        assert_eq!(screen.time_effect(), Some(TimeEffect::OnlyThis));
        write_time(&mut screen, "0:01.0");
        assert_eq!(screen.events[0].offset_ms, Some(1_000));
        assert_eq!(screen.events[block].offset_ms, Some(0), "only the serve");
        assert_eq!(screen.anchored(), Some(event_ref(&screen.events[0])));
        // back to automatic: no correction (and here not lined up)
        screen.key(key!(r));
        assert!(screen.profile.as_ref().unwrap().corrections.is_empty());
        assert!(screen.events.iter().all(|e| e.corrected_ms.is_none()));
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
            ui::fill(s.aligned_file, &[s.date_time(&start)])
        );
    }

    /// By hand: a time written moves only the event chosen; tied to a
    /// moment of the video, the others the same distance from it as their
    /// times; shifted, then back to the start of the file. Kept when the
    /// screen opens again.
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
        assert_eq!(
            screen.events[1].offset_ms, automatic[1],
            "only the one chosen"
        );
        assert_eq!(screen.anchored(), None);
        screen.key(key!(backspace));
        assert_eq!(screen.events[2].offset_ms, automatic[2]);
        // tied: all of them move
        screen.anchor_event(60_500);
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

    /// The event seen chosen as the video goes, at its time (not the 2 s
    /// before the video goes to), from the one gone to; the keys stop it,
    /// going to an event starts it again.
    #[test]
    fn the_event_chosen_follows_the_video() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "20150125 000000.mp4");
        let rally = screen
            .events
            .iter()
            .position(|e| e.offset_ms > screen.events[0].offset_ms)
            .unwrap();
        let at =
            |screen: &VideoScreen, i: usize| screen.events[i].offset_ms.unwrap().unsigned_abs();
        // gone to: that one, even with an event 2 s before
        screen.go_to_event(rally);
        screen.follow_video(at(&screen, rally));
        assert_eq!(screen.selected, rally);
        // played from 2 s before it (moved by hand): not yet it, the one
        // before; at its time, it
        screen.seek(at(&screen, rally) - LEAD_MS.unsigned_abs());
        screen.follow_video(at(&screen, rally) - LEAD_MS.unsigned_abs());
        assert!(at(&screen, screen.selected) < at(&screen, rally));
        screen.follow_video(at(&screen, rally));
        assert_eq!(at(&screen, screen.selected), at(&screen, rally));
        // the seek on the frame a little before, or the old position still
        // read: the one gone to stays chosen (clicked, it wasn't chosen)
        screen.go_to_event(rally);
        screen.follow_video(at(&screen, rally) - 16);
        assert_eq!(screen.selected, rally);
        screen.follow_video(0);
        assert_eq!(screen.selected, rally);
        // the video past an event after it: that one
        let next = (0..screen.events.len())
            .filter(|i| at(&screen, *i) > at(&screen, rally))
            .min_by_key(|i| at(&screen, *i))
            .unwrap();
        screen.follow_video(at(&screen, next));
        assert_eq!(at(&screen, screen.selected), at(&screen, next));
        // moved by hand a little before its time: the latest one before it
        screen.seek(at(&screen, rally) - 1);
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
        // going to an event follows again, from it
        screen.key(key!(enter));
        screen.follow_video(at(&screen, rally));
        assert_eq!(screen.selected, chosen);
        let later = (0..screen.events.len())
            .filter(|i| at(&screen, *i) > at(&screen, chosen))
            .min_by_key(|i| at(&screen, *i))
            .unwrap();
        screen.follow_video(at(&screen, later));
        assert_eq!(at(&screen, screen.selected), at(&screen, later));
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
    /// With the actions picked, their clips on the bar: in order, from the
    /// margin before to the margin after, the overlapping ones as one, the
    /// one playing told apart.
    #[test]
    fn the_clips_of_the_actions_picked_on_the_bar() {
        let dir = tempfile::tempdir().unwrap();
        let screen = with_file(&dir, "20150125 000000.mp4");
        assert!(screen.spans().is_empty(), "no actions picked: no clips");
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
        let mut screen = VideoScreen::with_clips(
            core,
            dir.path().to_path_buf(),
            team,
            m,
            refs,
            "Serve".into(),
        )
        .unwrap();
        let spans = screen.spans();
        assert!(!spans.is_empty());
        // in order, apart, each from its margin before to its margin after
        assert!(spans.windows(2).all(|w| w[0].to_ms < w[1].from_ms));
        let first = screen
            .events
            .iter()
            .filter_map(|e| e.offset_ms)
            .min()
            .unwrap();
        // from the margin before, not before the start of the video
        assert_eq!(spans[0].from_ms, first.unsigned_abs().saturating_sub(5000));
        assert!(spans[0].to_ms >= first.unsigned_abs() + 5000);
        // the clips playing: the one with the action chosen in it
        let playing = |screen: &VideoScreen, at: u64| {
            let playing: Vec<ClipSpan> =
                screen.spans().into_iter().filter(|sp| sp.playing).collect();
            playing.len() == 1 && playing[0].from_ms <= at && at <= playing[0].to_ms
        };
        let chosen = screen.event_target(screen.selected).unwrap();
        assert!(playing(&screen, chosen), "the one chosen playing");
        // the next one playing
        let next = screen.next_clip().unwrap();
        screen.go_to_event(next);
        let chosen = screen.event_target(next).unwrap();
        assert!(playing(&screen, chosen));
        // the margins chosen
        screen.update(Message::MarginInput(Margin::Before, "30".into()));
        screen.update(Message::MarginInput(Margin::After, "20".into()));
        let wider = screen.spans();
        assert_eq!(
            wider[0].from_ms,
            first.unsigned_abs().saturating_sub(30_000)
        );
        assert!(wider[0].to_ms >= first.unsigned_abs() + 20_000);
    }

    /// With the actions picked, a click on the strip: on a clip, it goes
    /// to its start and plays it, the one playing on the strip; between
    /// them, to that point of the video, no clip to stop at. On wide and
    /// narrow windows.
    #[test]
    fn the_strip_clicked() {
        use scout4all_mobile::strip::{MOUSE_REACH, strip_bars, strip_clicked};
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
        let mut screen = VideoScreen::with_clips(
            core,
            dir.path().to_path_buf(),
            team,
            m,
            refs,
            "Serve".into(),
        )
        .unwrap();
        // the actions 2 s apart: clips of 1 s, apart
        screen.update(Message::MarginInput(Margin::Before, "0".into()));
        screen.update(Message::MarginInput(Margin::After, "1".into()));
        let spans = screen.spans();
        assert!(spans.len() > 2);
        let duration = spans.iter().map(|sp| sp.to_ms).max().unwrap() + 60_000;
        screen.status = Some(Status {
            frame: None,
            position_ms: 0,
            duration_ms: duration,
            playing: false,
            speed: 1.0,
            error: None,
        });
        assert_eq!(screen.selected, 0);
        for width in [1600.0_f32, 700.0, 240.0] {
            let bars = strip_bars(&spans, duration, width);
            // the middle of each clip: the clip there
            for (sp, bar) in spans.iter().zip(&bars) {
                let middle = (bar.from + bar.to) / 2.0;
                let click = strip_clicked(&spans, duration, width, middle, MOUSE_REACH);
                let StripClick::Clip { event } = click else {
                    panic!("{width}: {click:?} on the clip of {}", sp.event)
                };
                let near = bars[spans.iter().position(|s| s.event == event).unwrap()];
                assert!(near.from <= middle + 4.0 && middle - 4.0 <= near.to);
            }
        }
        // the third clip clicked: chosen, from its start, played to its end
        let third = spans[2];
        let bar = strip_bars(&spans, duration, 1600.0)[2];
        let click = strip_clicked(
            &spans,
            duration,
            1600.0,
            (bar.from + bar.to) / 2.0,
            MOUSE_REACH,
        );
        assert_eq!(click, StripClick::Clip { event: third.event });
        screen.update(Message::Strip(click));
        assert_eq!(screen.selected, third.event as usize);
        assert_eq!(screen.position(), third.from_ms);
        assert!(screen.clip_end.is_some(), "played to its end");
        let now = screen.spans();
        assert_eq!(
            now.iter()
                .filter(|sp| sp.playing)
                .map(|sp| sp.event)
                .collect::<Vec<_>>(),
            [third.event],
            "the one playing on the strip"
        );
        // between the first two: the video there, no clip to stop at
        let bars = strip_bars(&spans, duration, 1600.0);
        let x = (bars[0].to + bars[1].from) / 2.0;
        assert!(bars[1].from - bars[0].to > 10.0, "room between them");
        let StripClick::At { ms } = strip_clicked(&spans, duration, 1600.0, x, MOUSE_REACH) else {
            panic!("between the clips")
        };
        assert!(spans[0].to_ms < ms && ms < spans[1].from_ms);
        screen.update(Message::Strip(StripClick::At { ms }));
        assert_eq!(screen.position(), ms);
        assert!(screen.clip_end.is_none());
    }

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
            opponent: DvwOpponent::Name,
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
                        players: vec![p.player.id.clone()],
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

    /// The video of the serves of a team in its matches: summed up before
    /// making it, each match after its card, the serves counted across the
    /// matches; a match without a video, with its file gone or not lined up
    /// left out and said why.
    #[test]
    fn the_video_of_the_actions_of_a_team() {
        use crate::stats::tests::{match_against, team_with_two_matches};
        let s = crate::strings::strings(scout4all_core::shapes::enums::LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = team_with_two_matches(&core);
        let (arezzo, siena) = (
            match_against(&core, &team, "Arezzo"),
            match_against(&core, &team, "Siena"),
        );
        // a file of the video starting just before the first event, or not
        // lined up (no start); the file there or not
        let video = |m: &str, lined_up: bool, there: bool| {
            let (the_match, _) = core.video(&team, m).unwrap();
            let first = the_match
                .sets
                .iter()
                .flat_map(|set| set.events.iter().map(|e| e.timestamp))
                .min()
                .unwrap();
            let path = dir.path().join(format!("{m}.mp4"));
            let _ = std::fs::remove_file(&path);
            if there {
                std::fs::copy(
                    concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/src/video/testdata/two_seconds.mp4"
                    ),
                    &path,
                )
                .unwrap();
            }
            let start =
                lined_up.then(|| (first - chrono::TimeDelta::milliseconds(500)).fixed_offset());
            let profile = VideoProfile {
                files: vec![VideoFile {
                    path: video::stored_path(dir.path(), &path),
                    start,
                }],
                ..Default::default()
            };
            core.save_video(&the_match, &profile).unwrap();
        };
        let pick = scout4all_mobile::ClipPick {
            scope: scout4all_mobile::StatsScope::Match,
            event: scout4all_core::shapes::enums::EventTypeEnum::S,
            filter: scout4all_mobile::DetailFilter::default(),
            evals: vec![],
            zone: None,
            target: None,
        };
        let period = scout4all_mobile::AnalysisPeriod::LastMatches { count: 4 };
        let summed = |core: &Arc<Scout4All>| {
            let matches = core.team_clips(&team, &period, &pick).unwrap();
            let mut screen = VideoScreen::with_tour(
                core.clone(),
                dir.path().to_path_buf(),
                team.clone(),
                matches,
                "Battute".into(),
            )
            .unwrap();
            assert!(matches!(screen.key(key!(x)), Outcome::Montage));
            screen.start_montage(s);
            assert!(screen.montage.is_none(), "summed up first");
            screen
        };
        // both lined up: Arezzo then Siena, each after its card; its serves
        // a second apart at most, one piece a match, counted across
        video(&arezzo, true, true);
        video(&siena, true, true);
        let screen = summed(&core);
        let summary = screen.summary.as_ref().unwrap();
        assert_eq!(summary.actions, 5);
        assert!(summary.left_out.is_empty());
        let cards: Vec<(String, Vec<String>)> = summary
            .plan
            .matches
            .iter()
            .map(|m| {
                let title = m.title.clone().unwrap();
                let pieces = m
                    .pieces
                    .iter()
                    .map(|p| p.card.lines.last().unwrap().clone());
                (
                    title.title,
                    title.lines.into_iter().skip(1).chain(pieces).collect(),
                )
            })
            .collect();
        assert_eq!(
            cards,
            [
                (
                    "Arezzo".to_string(),
                    vec![
                        "Partita 1 di 2 · Azioni: 3".to_string(),
                        "1 / 2  (+2)".into()
                    ]
                ),
                (
                    "Siena".to_string(),
                    vec![
                        "Partita 2 di 2 · Azioni: 2".to_string(),
                        "2 / 2  (+1)".into()
                    ]
                ),
            ]
        );
        assert_eq!(
            left_out_said(&summary.left_out, s),
            None,
            "nothing left out, nothing said"
        );
        // the file of Siena gone, then Siena not lined up
        for (lined_up, there, why) in [
            (true, false, LeftOut::Unreadable),
            (false, true, LeftOut::NotLinedUp),
        ] {
            video(&siena, lined_up, there);
            let mut screen = summed(&core);
            let summary = screen.summary.as_ref().unwrap();
            assert_eq!(summary.actions, 3);
            assert_eq!(summary.plan.matches.len(), 1);
            assert_eq!(summary.left_out, [("Siena".to_string(), why)]);
            // cancelled: nothing made
            screen.update(Message::StopMontage);
            assert!(screen.summary.is_none() && screen.montage.is_none());
        }
        // Siena without a video: left out, said; Arezzo made, `x` again
        core.save_video(
            &core.video(&team, &siena).unwrap().0,
            &VideoProfile::default(),
        )
        .unwrap();
        let mut screen = summed(&core);
        let summary = screen.summary.as_ref().unwrap();
        assert_eq!(
            left_out_said(&summary.left_out, s).unwrap(),
            "Escluse: Siena (senza video)"
        );
        assert!(matches!(screen.key(key!(x)), Outcome::Stay));
        assert!(screen.summary.is_none() && screen.montage.is_some());
    }

    /// The seconds chosen in the settings: ← → (and the hints, the
    /// buttons) jump by them; out of clip mode the video goes that long
    /// before an event, in clip mode the margin before still counts.
    #[test]
    fn the_seconds_chosen_in_the_settings() {
        let dir = tempfile::tempdir().unwrap();
        Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into())
            .unwrap()
            .set_video_seconds(VideoSeconds {
                step_s: 12,
                lead_s: 0,
            })
            .unwrap();
        let mut screen = with_file(&dir, "20150125 000000.mp4");
        screen.status = Some(Status {
            frame: None,
            position_ms: 0,
            duration_ms: u64::MAX / 2,
            playing: false,
            speed: 1.0,
            error: None,
        });
        let i = 3;
        let at = screen.events[i].offset_ms.unwrap();
        assert_eq!(screen.event_target(i), Some(at.unsigned_abs()));
        screen.update(Message::WholeVideo);
        screen.seek(30_000);
        screen.key(key!(right));
        assert_eq!(screen.position(), 42_000);
        screen.key(key!(left));
        screen.key(key!(left));
        assert_eq!(screen.position(), 18_000);
        screen.key(key!(k));
        assert_eq!(screen.event_target(i), Some((at - 5000).unsigned_abs()));
    }

    /// Once lined up, the video before the first event (the warm-up) is
    /// left out: going back stops a little before it. "Redo the sync"
    /// shows it, to tie the events again; tying them leaves it out again.
    #[test]
    fn the_video_before_the_first_event_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "20150125 000000.mp4");
        screen.status = Some(Status {
            frame: None,
            position_ms: 0,
            duration_ms: u64::MAX / 2,
            playing: false,
            speed: 1.0,
            error: None,
        });
        let first = screen
            .events
            .iter()
            .filter_map(|e| e.offset_ms)
            .min()
            .unwrap();
        let start = (first - LEAD_MS).unsigned_abs();
        assert!(start > 0);
        assert_eq!(screen.start_ms(), start);
        screen.seek(0);
        assert_eq!(screen.position(), start);
        screen.key(key!(left));
        assert_eq!(screen.position(), start);
        // with the clips, their margin before
        screen.key(key!(k));
        assert_eq!(screen.start_ms(), (first - 5000).max(0).unsigned_abs());
        screen.key(key!(k));
        // the whole video, to tie the first event again
        screen.update(Message::WholeVideo);
        screen.seek(0);
        assert_eq!(screen.position(), 0);
        screen.select(0);
        screen.seek(1000);
        screen.update(Message::AnchorHere);
        assert!(!screen.whole);
        assert_eq!(screen.events[0].offset_ms, Some(1000));
        assert_eq!(screen.start_ms(), 0, "1 s in: from the start");
    }

    /// The actions picked made a video: `x` (or its button) asks for it,
    /// only with some picked; a piece of the video an action, from its
    /// margin before to its margin after, after a card telling it.
    #[test]
    fn the_video_of_the_actions_picked() {
        let s = crate::strings::strings(scout4all_core::shapes::enums::LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let mut whole = with_file(&dir, "20150125 000000.mp4");
        assert!(
            matches!(whole.key(key!(x)), Outcome::Stay),
            "no actions picked"
        );
        let serves: Vec<EventRef> = whole
            .events
            .iter()
            .filter(|e| e.event.event.event == scout4all_core::shapes::enums::EventTypeEnum::S)
            .map(event_ref)
            .collect();
        let mut screen = VideoScreen::with_clips(
            whole.core.clone(),
            dir.path().to_path_buf(),
            whole.team_id.clone(),
            whole.match_id.clone(),
            serves.clone(),
            "Battute".into(),
        )
        .unwrap();
        assert!(matches!(screen.key(key!(x)), Outcome::Montage));
        assert!(matches!(screen.update(Message::Montage), Outcome::Montage));
        let pieces = screen.match_plan(s).pieces;
        let spans = screen.spans();
        assert_eq!(
            pieces
                .iter()
                .map(|p| (p.from_ms, p.to_ms))
                .collect::<Vec<_>>(),
            spans
                .iter()
                .map(|sp| (sp.from_ms, sp.to_ms))
                .collect::<Vec<_>>()
        );
        let first = &pieces[0].card;
        assert!(first.title.starts_with("Battuta"), "{}", first.title);
        // the serves of the file 2 s apart: one piece, the others counted
        assert_eq!(pieces.len(), 1);
        assert_eq!(
            first.lines.last().unwrap(),
            &format!("1 / 1  (+{})", serves.len() - 1)
        );
        // who, the set and the score before it, which of how many
        assert_eq!(first.lines.len(), 3, "{:?}", first.lines);
        assert!(first.lines[1].starts_with("Set "), "{:?}", first.lines);
        // the file isn't there: why, said
        screen.start_montage(s);
        let deadline = Instant::now() + Duration::from_secs(5);
        while screen.montage.is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
            screen.tick();
        }
        assert!(screen.montage.is_none());
        assert!(screen.montage_error.is_some());
        whole.key(BACK_KEY);
    }

    /// With some actions picked, how the match stands over the video still
    /// from all its events: the sets won before, the score after the last
    /// event passed.
    #[test]
    fn how_the_match_stands_with_the_actions_picked() {
        let dir = tempfile::tempdir().unwrap();
        let whole = with_file(&dir, "20150125 000000.mp4");
        let later: Vec<EventRef> = whole
            .events
            .iter()
            .filter(|e| e.set_number > 1)
            .map(event_ref)
            .collect();
        let mut screen = VideoScreen::with_clips(
            whole.core.clone(),
            dir.path().to_path_buf(),
            whole.team_id.clone(),
            whole.match_id.clone(),
            later,
            "Set 2–3".into(),
        )
        .unwrap();
        let at = screen.events[0].offset_ms.unwrap().unsigned_abs();
        screen.status = Some(Status {
            frame: None,
            position_ms: at,
            duration_ms: u64::MAX / 2,
            playing: false,
            speed: 1.0,
            error: None,
        });
        let seen = screen.standing().unwrap();
        assert_eq!(Some(seen.clone()), standing(&whole.events, at));
        assert_ne!(
            (seen.sets_us, seen.sets_them),
            (0, 0),
            "the first set counted"
        );
    }

    /// Over the video, how the match stands at that moment: nothing before
    /// the first event; then after the last event passed: its set, the sets
    /// won before it, the score, the phase and the rotation to come. `i`
    /// hides it and shows it again.
    #[test]
    fn how_the_match_stands_over_the_video() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "20150125 000000.mp4");
        // the times of the file made up (sets starting at the same second):
        // a second after the other
        let events: Vec<VideoEvent> = screen
            .events
            .iter()
            .enumerate()
            .map(|(i, e)| VideoEvent {
                offset_ms: Some(1000 * (i as i64 + 1)),
                ..e.clone()
            })
            .collect();
        let first: i64 = 1000;
        assert_eq!(standing(&events, (first - 1).unsigned_abs()), None);
        // the last of the first set, then the first of the second
        let last = events.iter().rposition(|e| e.set_number == 1).unwrap();
        let next = &events[last + 1];
        let end = &events[last].event;
        let at = |e: &VideoEvent| e.offset_ms.unwrap().unsigned_abs();
        let seen = standing(&events, at(&events[last])).unwrap();
        assert_eq!(seen.set_number, 1);
        assert_eq!((seen.sets_us, seen.sets_them), (0, 0));
        assert_eq!(
            (seen.score_us, seen.score_them),
            (end.score_us, end.score_them)
        );
        let won = match end.score_us > end.score_them {
            true => (1, 0),
            false => (0, 1),
        };
        let seen = standing(&events, at(next)).unwrap();
        assert_eq!(seen.set_number, 2);
        assert_eq!((seen.sets_us, seen.sets_them), won);
        // a point of ours: we serve, a break to play
        let point = events
            .iter()
            .zip(events.iter().skip(1))
            .find(|(a, b)| {
                a.set_number == b.set_number
                    && b.event.score_us == a.event.score_us + 1
                    && b.offset_ms > a.offset_ms
            })
            .map(|(_, b)| b)
            .unwrap();
        let seen = standing(&events, at(point)).unwrap();
        assert_eq!(seen.phase, PhaseEnum::Break);
        assert!(seen.rotation.starts_with('P') || seen.rotation.starts_with('S'));
        assert!(screen.margins.overlay);
        screen.key(key!(i));
        assert!(!screen.margins.overlay);
        assert!(
            !Margins::read(&screen.dir).overlay,
            "kept for the next time"
        );
        screen.update(Message::Overlay);
        assert!(screen.margins.overlay);
    }

    /// The names of the teams over the video: whole when short, cut when
    /// long.
    #[test]
    fn the_names_over_the_video() {
        assert_eq!(
            overlay_name("POLISPORTIVA SAVINESE"),
            "POLISPORTIVA SAVINESE"
        );
        let long = "ASSOCIAZIONE SPORTIVA DILETTANTISTICA";
        assert_eq!(overlay_name(long), "ASSOCIAZIONE SPORTIVA…");
        assert_eq!(overlay_name(long).chars().count(), 22);
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

    /// Esc leaves the time being written, then its panel, not the screen.
    #[test]
    fn esc_leaves_the_time_written() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "20150125 000000.mp4");
        screen.key(key!(t));
        assert_eq!(screen.panel, Some(Panel::Sync), "the field is in its panel");
        assert!(matches!(screen.key(BACK_KEY), Outcome::Stay));
        assert!(screen.editing.is_none());
        assert!(matches!(screen.key(BACK_KEY), Outcome::Stay));
        assert_eq!(screen.panel, None);
        assert!(matches!(screen.key(BACK_KEY), Outcome::Back));
    }

    /// The panels over the video: one open at a time, its button again or
    /// Esc closes it; opening another leaves what was being written.
    /// The sync panel only with the events of the match.
    #[test]
    fn the_panels_over_the_video() {
        let dir = tempfile::tempdir().unwrap();
        let mut screen = with_file(&dir, "20150125 000000.mp4");
        let s = crate::strings::strings(scout4all_core::shapes::enums::LanguageEnum::It);
        let c = theme::colors(false);
        assert!(screen.panel_view(s, c).is_none());
        screen.update(Message::Panel(Panel::Sync));
        assert_eq!(screen.panel, Some(Panel::Sync));
        assert!(screen.panel_view(s, c).is_some());
        screen.update(Message::EditTime);
        screen.update(Message::Panel(Panel::Clip));
        assert_eq!(screen.panel, Some(Panel::Clip));
        assert!(screen.editing.is_none());
        assert!(!screen.clip, "opening the panel doesn't turn clips on");
        screen.update(Message::Clip);
        assert!(screen.clip);
        screen.update(Message::Panel(Panel::Clip));
        assert_eq!(screen.panel, None);
        screen.update(Message::Panel(Panel::Clip));
        assert!(matches!(screen.key(BACK_KEY), Outcome::Stay));
        assert_eq!(screen.panel, None);
        screen.events.clear();
        screen.update(Message::Panel(Panel::Sync));
        assert!(screen.panel_view(s, c).is_none(), "nothing to line up");
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
