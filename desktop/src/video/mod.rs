//! The video of a match (001): its file played in the app, Space plays and
//! pauses, ← → go back and forward a few seconds, `-` `+` change the speed,
//! the bar goes anywhere; `c` chooses the file. Beside it the events of the
//! match, at their time in the video when it lines up with them (`video`
//! in the core: DataVolley, or the start of the file from its name): ↑ ↓
//! choose one, Enter (or a click) goes a little before it. Only the desktop
//! app has it: the profile (`video.json`) is saved next to the match.

mod list;
mod player;
mod source;

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
    player::{Player, Status},
    scout4all_core::{
        shapes::r#match::MatchEntry,
        video::{self, AnchorSource, StoredProfile, VideoFile, VideoProfile},
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{MobileError, Scout4All, VideoEvent},
    std::{
        path::{Path, PathBuf},
        sync::Arc,
        time::{Duration, Instant},
    },
};

/// How far ← and → go.
const STEP_SECONDS: u64 = 5;

/// How long before an event the video goes to it: the serve is seen
/// coming.
const LEAD_MS: i64 = 2000;

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
    Choose,
    /// the file chosen, if any
    Chosen(Option<PathBuf>),
    /// a frame ready to be drawn at once, if it could be
    Allocated(Arc<player::Frame>, image::Handle, Option<Allocation>),
    /// an event of the list: chosen, and the video at it
    Event(usize),
}

pub enum Outcome {
    Stay,
    Back,
    /// a file to choose (`Message::Chosen`)
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
        };
        screen.load_events()?;
        screen.open_player();
        Ok(screen)
    }

    /// The events at their time in the video, as the profile lines them up.
    fn load_events(&mut self) -> Result<(), MobileError> {
        let profile = self.profile.clone().unwrap_or_default();
        self.events = self.core.video_events(&self.the_match, &profile)?;
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
        self.events
            .get(i)
            .and_then(|e| e.offset_ms)
            .map(|ms| (ms - LEAD_MS).max(0).unsigned_abs())
    }

    fn go_to_event(&mut self, i: usize) {
        self.selected = i.min(self.events.len().saturating_sub(1));
        if let Some(ms) = self.event_target(self.selected) {
            self.seek(ms);
        }
    }

    pub fn title(&self, s: &Strings) -> String {
        format!("{} – {}", s.video, self.the_match.opponent)
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

    /// The file chosen in place of the video: saved, then played.
    fn chosen(&mut self, path: &Path) {
        let Some(profile) = &self.profile else {
            return;
        };
        let offset = *self.the_match.date.offset();
        let file = VideoFile {
            path: video::stored_path(&self.dir, path),
            start: video::start_from_name(path, offset),
        };
        let profile = VideoProfile {
            files: vec![file],
            ..profile.clone()
        };
        match self.core.save_video(&self.the_match, &profile) {
            Ok(()) => {
                self.profile = Some(profile);
                if let Err(e) = self.load_events() {
                    self.error = Some(e.to_string());
                }
                self.open_player();
            }
            Err(e) => self.error = Some(e.to_string()),
        }
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
            Message::Choose => return self.choose(),
            Message::Chosen(Some(path)) => self.chosen(&path),
            Message::Chosen(None) => {}
            Message::Event(i) => self.go_to_event(i),
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
            k if k == key!(up) => self.select(self.selected.saturating_sub(1)),
            k if k == key!(down) => self.select(self.selected + 1),
            k if k == key!(enter) => self.go_to_event(self.selected),
            _ => {}
        }
        Outcome::Stay
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        let choose = match self.paths().is_empty() {
            true => s.choose_video,
            false => s.change_video,
        };
        let playing = [
            (s.space.to_string(), format!("{} / {}", s.play, s.pause)),
            (
                "← →".to_string(),
                format!(
                    "{} / {}",
                    ui::fill(s.back_seconds, &[STEP_SECONDS.to_string()]),
                    ui::fill(s.forward_seconds, &[STEP_SECONDS.to_string()])
                ),
            ),
            ("- +".to_string(), s.speed.to_string()),
        ];
        let playing = match self.player {
            Some(_) => playing.to_vec(),
            None => vec![],
        };
        let events = match self.events.is_empty() {
            true => vec![],
            false => vec![
                ("↑ ↓".to_string(), s.match_events.to_string()),
                ("Enter".to_string(), s.go_to_event.to_string()),
            ],
        };
        let playing = [playing, events].concat();
        [
            playing,
            vec![
                ("c".to_string(), choose.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ],
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
        let screen: Element<'a, Message> = match (&self.shown, &self.player) {
            (Some(shown), _) => image(shown.handle.clone())
                .content_fit(ContentFit::Contain)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            (None, Some(_)) => space::vertical().height(Length::Fill).into(),
            (None, None) => container(ui::body(s.no_video).color(c.muted))
                .center(Length::Fill)
                .into(),
        };
        let screen = container(screen)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Color::BLACK.into()),
                ..container::Style::default()
            });
        let choose = match self.paths().is_empty() {
            true => s.choose_video,
            false => s.change_video,
        };
        let file_name = self
            .paths()
            .first()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let file_line = row![
            text(file_name)
                .size(SIZE_SMALL)
                .color(c.muted)
                .width(Length::Fill),
            ui::pill(choose, self.player.is_none(), Some(Message::Choose), c),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let controls: Element<'a, Message> = match &self.status {
            Some(status) => self.controls(status, s, c),
            None => space::vertical().height(0).into(),
        };
        let notice: Element<'a, Message> = match notice {
            Some(n) => ui::body(n).color(c.danger).into(),
            None => space::vertical().height(0).into(),
        };
        let video = column![notice, screen, controls, file_line].spacing(10);
        let events: Element<'a, Message> = match self.events.is_empty() {
            true => space::horizontal().width(0).into(),
            false => {
                iced::widget::container(ui::panel(Some(s.match_events), self.events_view(s, c), c))
                    .width(Length::Fixed(400.0))
                    .into()
            }
        };
        row![video.width(Length::Fill), events].spacing(14).into()
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
        column![
            said,
            list::view(&self.events, self.selected, duration, s, c)
        ]
        .spacing(10)
        .height(Length::Fill)
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
                text(s.speed).size(SIZE_SMALL).color(c.muted),
            ]
            .extend(speeds)
            .spacing(10)
            .align_y(Alignment::Center),
        ]
        .spacing(8)
        .into()
    }
}

/// The file of the video, chosen by the user.
pub async fn pick(filter: &'static str) -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .add_filter(filter, &["mp4", "MP4", "mov", "MOV", "m4v", "M4V"])
        .pick_file()
        .await
        .map(|f| f.path().to_path_buf())
}

/// `h:mm:ss`, or `m:ss` under the hour.
fn clock(ms: u64) -> String {
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
mod screen_tests {
    use {
        super::*,
        scout4all_core::video::{offset_ms, start_from_name},
        scout4all_mobile::{DvwChoice, DvwSide},
    };

    /// The match of the tests of the TUI, three sets, imported: the file
    /// has no video column, the events are on the clock.
    fn imported(dir: &tempfile::TempDir) -> (Arc<Scout4All>, String, String) {
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
        let start = start_from_name(Path::new("20150125 000000.mp4"), *m.date.offset()).unwrap();
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
