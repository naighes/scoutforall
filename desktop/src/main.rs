//! The desktop app of scout4all (027), which took the place of the TUI: the
//! home, the scouting of a set in progress, the keys of the scouting (F2).
//! It reads the data of the TUI (`~/.scout4all`, or `SCOUT4ALL_DATA_DIR`) and its
//! keys.

// no console window on windows for the builds of the releases: `--version`
// still answers whoever reads its output (the self update, the workflow)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod calendar;
mod charts;
mod countries;
mod court;
mod dvw;
mod editors;
mod events;
mod field;
mod form;
mod game;
mod general;
mod home;
mod keymap;
mod live;
mod paths;
mod scouting;
mod settings;
mod share;
mod shots;
mod splash;
mod start;
mod stats;
mod strings;
mod team;
mod theme;
mod transfer;
mod ui;
mod update;
mod video;

use {
    game::MatchScreen,
    home::HomeScreen,
    iced::{
        Element, Event, Length, Size, Subscription, Task, Theme, event, keyboard,
        widget::{Id, column, container, space},
        window,
    },
    scout4all_core::{scouting::ChoiceOrder, shapes::enums::LanguageEnum},
    scout4all_keys::{BACK_KEY, KeyCombination, SETTINGS_KEY, ScoutingKeys},
    scout4all_mobile::{Home, Scout4All},
    scouting::{Outcome, ScoutingScreen},
    settings::SettingsScreen,
    std::{
        path::PathBuf,
        str::FromStr,
        sync::{Arc, Mutex, OnceLock},
        time::Instant,
    },
    strings::Strings,
    team::TeamScreen,
};

const DATA_DIR_VAR: &str = "SCOUT4ALL_DATA_DIR";

/// When the process started, to measure the time to the first frame.
static STARTED: OnceLock<Instant> = OnceLock::new();

fn data_dir() -> Option<PathBuf> {
    std::env::var_os(DATA_DIR_VAR)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".scout4all")))
}

/// A new empty folder for the data when theirs can't be opened, gone at
/// the next start of the system.
fn empty_data_dir() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("scout4all-{}-{stamp}", std::process::id()))
}

/// What was done, then what to tell of it, if anything, below.
fn told(done: &str, summary: Option<String>) -> String {
    match summary {
        Some(summary) => format!("{done}\n{summary}"),
        None => done.to_string(),
    }
}

/// What the TUI keeps in `config.json` that the prototype reads: the
/// language and the keys.
/// The language, the keys and the theme chosen (dark or light; none:
/// the system's).
fn config(dir: &std::path::Path) -> (LanguageEnum, ScoutingKeys, Option<bool>) {
    let json: serde_json::Value = std::fs::read_to_string(dir.join("config.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let language = json
        .get("language")
        .and_then(|l| l.as_str())
        .and_then(|l| LanguageEnum::from_str(l).ok())
        .unwrap_or(LanguageEnum::En);
    let theme = match json.get("theme").and_then(|t| t.as_str()) {
        Some("dark") => Some(true),
        Some("light") => Some(false),
        _ => None,
    };
    let keys = serde_json::from_value::<ScoutingKeys>(json)
        .unwrap_or_default()
        .normalized();
    (language, keys, theme)
}

enum Screen {
    Home(Box<HomeScreen>),
    Team(Box<TeamScreen>),
    Match(Box<MatchScreen>),
    Start(Box<start::StartScreen>),
    Scouting(Box<ScoutingScreen>),
    Stats(Box<stats::StatsScreen>),
    Dvw(Box<dvw::DvwScreen>),
    Events(Box<events::EventsScreen>),
    Video(Box<video::VideoScreen>),
    Live(Box<live::screen::LiveScreen>),
}

/// Where a key or a click leads.
enum Go {
    Stay,
    /// the video of the actions picked to make
    Montage,
    /// a file made, to share or save: what it shows, the file
    Share(String, PathBuf),
    Home,
    Team(String),
    Match(String, String),
    Stats(String, String),
    /// the statistics of the team over its matches
    Analysis(String),
    /// the events of a set of the match, to correct them
    Events(String, String, u8),
    /// the video of the match
    Video(String, String),
    /// the actions of the match picked in its statistics, in the video
    Clips(String, String, scout4all_mobile::ClipPick, String),
    /// the actions of the team picked in its statistics, match after match
    TeamClips(String, Vec<scout4all_mobile::MatchClips>, String),
    /// back to the statistics the actions were picked in
    Unpark,
    /// the file of the video to choose
    PickVideo,
    /// a frame of the video to allocate before it's shown
    VideoFrame(Box<video::Frame>),
    /// the video source of the match, to record it
    Live(String, String),
    /// the system asked whether the cameras may be used
    AskCameras,
    /// the folder of the recordings to choose
    PickRecordings,
    /// a picture of the camera to allocate before it's shown
    LiveFrame(Box<(Arc<video::Picture>, iced::widget::image::Handle)>),
    /// the camera of the source screen recording, the app keeping it
    Record,
    /// a set of the match: to start, or in progress
    Scout(home::Open),
    /// the zip of a team or of a match, to save
    Export(transfer::Export),
    /// a file to choose to import: a match into the team, when given
    Import(Option<String>),
}

struct App {
    core: Arc<Scout4All>,
    dir: PathBuf,
    language: LanguageEnum,
    keys: ScoutingKeys,
    /// the settings, over the screen they were opened from (F2)
    settings: Option<SettingsScreen>,
    strings: &'static Strings,
    /// the theme chosen, saved (dark or light); none: the system's
    theme: Option<bool>,
    /// the system's theme is dark (until it's known, and with none)
    system_dark: bool,
    screen: Screen,
    error: Option<String>,
    /// what went well: a file saved, imported
    notice: Option<String>,
    /// an archive chosen, to confirm, over the screen
    importing: Option<transfer::Importing>,
    /// a file made, to share or save
    sharing: Option<transfer::Sharing>,
    /// the terms of use, at the first start and when they change: over
    /// everything until accepted
    terms: Option<scout4all_mobile::Terms>,
    /// the text field the keys are on, last focused
    focused: Option<Id>,
    /// a newer release, offered in the bar
    update: Update,
    /// the own window of the video, while open
    video_window: Option<window::Id>,
    /// the camera recording a match, whatever the screen; kept until its
    /// MP4 is saved
    recorder: Option<live::capture::Capture>,
    recorder_status: Option<live::capture::Status>,
    /// the recording's own window, asked for (the button in the bar):
    /// open while it records
    preview_asked: bool,
    preview_window: Option<window::Id>,
    /// the pictures of the recording in its window
    recording_preview: live::preview::Preview,
    /// the statistics the actions were picked in, while they're watched
    /// in the video: back to them as they were
    parked: Option<Box<stats::StatsScreen>>,
}

/// The self update (`update`), as the bar shows it.
#[derive(Debug, Clone, PartialEq)]
enum Update {
    None,
    /// a newer version: asked before it's installed
    Available(String),
    Installing(String),
}

#[derive(Debug, Clone)]
enum Message {
    Key(KeyCombination),
    ToggleTheme,
    /// the theme of the system, at the start and as it changes
    SystemTheme(iced::theme::Mode),
    /// the arrow on the bar: as Esc
    Back,
    /// the house on the bar: the home, from wherever
    GoHome,
    /// the door on the bar: the app closed (all is saved as it's made)
    Quit,
    OpenSettings,
    Home(home::Message),
    Team(team::Message),
    Match(game::Message),
    Scouting(scouting::Message),
    Start(start::Message),
    Stats(stats::Message),
    /// a frame of the splash
    Frame(Instant),
    Loaded(Loaded),
    Settings(settings::Message),
    Dvw(dvw::Message),
    Events(events::Message),
    Video(video::Message),
    /// where the archive goes (none: not saved), and the archive
    Exported(Option<PathBuf>, transfer::Content),
    /// the file chosen (none: none) to import into the team, if given
    Picked(Option<String>, Option<transfer::Picked>),
    Importing(transfer::Message),
    Sharing(transfer::ShareMessage),
    /// the sheet of the system shown, or why not
    Shared(Result<(), String>),
    AcceptTerms,
    /// the statistics sent or not, from the terms
    ToggleStatistics,
    /// the version of a newer release, if any
    UpdateFound(Option<String>),
    InstallUpdate,
    /// the new version in place of the app, or why not
    Installed(Result<String, String>),
    /// a window other than the app's closed (the app's ends it)
    WindowClosed(window::Id),
    Live(live::screen::Message),
    /// the recording's own window opened or closed
    TogglePreview,
    /// a new picture of the recording looked for, for its window
    PreviewTick,
    PreviewAllocated(
        Arc<video::Picture>,
        iced::widget::image::Handle,
        Option<iced::advanced::image::Allocation>,
    ),
    /// the recording looked at: how long, saved
    LiveTick,
    StopRecording,
    /// the screenshots of a plan (`shots`)
    Shot(ShotMessage),
}

#[derive(Debug, Clone)]
enum ShotMessage {
    /// the steps of the plan up to a wait or a capture
    Next,
    /// the window to capture, saved as the name
    Take(String),
    Taken(String, window::Screenshot),
    /// a frame of the recording to take, taken
    Frame,
    Framed(window::Screenshot),
}

impl shots::Open {
    fn go(self) -> Go {
        match self {
            shots::Open::Home => Go::Home,
            shots::Open::Team(team) => Go::Team(team),
            shots::Open::Match(team, m) => Go::Match(team, m),
            shots::Open::Scouting(team_id, match_id) => Go::Scout(home::Open {
                team_id,
                match_id,
                start: false,
            }),
            shots::Open::Stats(team, m) => Go::Stats(team, m),
            shots::Open::Analysis(team) => Go::Analysis(team),
            shots::Open::Video(team, m) => Go::Video(team, m),
        }
    }
}

impl App {
    /// Reads the configuration and the data: on a thread, the splash on
    /// screen meanwhile. `quiet` for the screenshots: nothing sent, no
    /// terms over the screens.
    fn load(quiet: bool) -> App {
        let dir = data_dir().unwrap_or_else(|| PathBuf::from(".scout4all"));
        let (language, keys, theme) = config(&dir);
        let strings = strings::strings(language);
        let core = Scout4All::new(dir.to_string_lossy().into_owned(), language.to_string());
        let (core, error) = match core {
            // what the start has to tell: the data folder of the versions
            // before not all moved into the database
            Ok(core) => {
                let notice = core.startup_notice();
                (core, notice)
            }
            // a new empty folder in place of the data (never one with
            // something in it: the start moves it into a database): the
            // error tells why
            Err(e) => match Scout4All::new(
                empty_data_dir().to_string_lossy().into_owned(),
                language.to_string(),
            ) {
                Ok(core) => (core, Some(e.to_string())),
                Err(_) => std::process::exit(1),
            },
        };
        let mut app = App {
            core,
            dir,
            language,
            keys,
            settings: None,
            strings,
            // the screenshots always dark, whatever was chosen
            theme: if quiet { None } else { theme },
            system_dark: true,
            screen: Screen::Home(Box::new(HomeScreen::new(Home {
                in_progress: vec![],
                recent: vec![],
                teams: vec![],
            }))),
            error,
            notice: None,
            importing: None,
            sharing: None,
            terms: None,
            focused: None,
            update: Update::None,
            video_window: None,
            preview_asked: false,
            preview_window: None,
            recording_preview: live::preview::Preview::default(),
            recorder: None,
            recorder_status: None,
            parked: None,
        };
        // the recordings cut short last time (the app closed while
        // recording): made videos of their matches
        let folder = live::LiveConfig::read(&app.dir).folder(&app.dir);
        for recovered in live::recover(&app.core, &app.dir, &folder) {
            match recovered {
                Ok(name) => app.notice = Some(ui::fill(strings.recording_recovered, &[name])),
                Err(e) => app.error = Some(ui::fill(strings.recording_failed, &[e])),
            }
        }
        app.terms = (!quiet && !app.core.terms_seen()).then(|| app.core.terms());
        // the matches queued while offline sent now, the next ones as they
        // are queued, again with a backoff while some are left
        if !quiet {
            app.core.send_in_background();
        }
        let _ = app.go(Go::Home);
        app
    }

    /// The title of the screen, in the middle of the bar.
    fn title(&self) -> String {
        match (&self.settings, &self.screen) {
            (Some(_), _) => self.strings.settings.to_string(),
            (None, Screen::Home(_)) => String::new(),
            (None, Screen::Team(screen)) => screen.title(),
            (None, Screen::Match(screen)) => screen.title(),
            (None, Screen::Start(screen)) => screen.title(),
            (None, Screen::Scouting(screen)) => screen.title(),
            (None, Screen::Stats(screen)) => screen.title(),
            (None, Screen::Dvw(_)) => self.strings.import_dvw.to_string(),
            (None, Screen::Events(screen)) => screen.title(self.strings),
            (None, Screen::Video(screen)) => screen.title(self.strings),
            (None, Screen::Live(screen)) => screen.title(self.strings),
        }
    }

    fn colors(&self) -> theme::Colors {
        theme::colors(self.dark())
    }

    /// Dark: chosen, or else as the system.
    fn dark(&self) -> bool {
        self.theme.unwrap_or(self.system_dark)
    }

    /// The other theme, saved; back as the system's, the system followed
    /// again (no choice saved).
    fn toggle_theme(&mut self) {
        let dark = !self.dark();
        self.theme = (dark != self.system_dark).then_some(dark);
        let saved = self.theme.map(|dark| if dark { "dark" } else { "light" });
        let theme = serde_json::json!({ "theme": saved });
        if let Err(e) = settings::merge(&self.dir, theme, self.language) {
            self.error = Some(e);
        }
    }

    /// Opens the screen `go` leads to; an error stays on the one shown.
    fn go(&mut self, go: Go) -> Task<Message> {
        let core = self.core.clone();
        let opened = match go {
            Go::Stay => return Task::none(),
            Go::Export(export) => return self.export(export),
            Go::Import(team) => {
                let s = self.strings;
                return Task::perform(transfer::pick(s.archives), move |f| {
                    Message::Picked(team.clone(), f)
                });
            }
            Go::Home => core
                .home()
                .map(|home| Screen::Home(Box::new(HomeScreen::new(home)))),
            Go::Team(team) => TeamScreen::new(core, team).map(|t| Screen::Team(Box::new(t))),
            Go::Match(team, m) => {
                MatchScreen::new(core, team, m).map(|m| Screen::Match(Box::new(m)))
            }
            Go::Events(team, m, set) => events::EventsScreen::new(core, team, m, set)
                .map(|screen| Screen::Events(Box::new(screen))),
            Go::Video(team, m) => video::VideoScreen::new(core, self.dir.clone(), team, m)
                .map(|screen| Screen::Video(Box::new(screen))),
            Go::Clips(team, m, pick, title) => {
                let opened = core.match_clips(&team, &m, &pick).and_then(|events| {
                    video::VideoScreen::with_clips(core, self.dir.clone(), team, m, events, title)
                });
                self.watch(opened);
                return Task::none();
            }
            Go::TeamClips(team, matches, title) => {
                let opened =
                    video::VideoScreen::with_tour(core, self.dir.clone(), team, matches, title);
                self.watch(opened);
                return Task::none();
            }
            Go::Unpark => match self.parked.take() {
                Some(stats) => Ok(Screen::Stats(stats)),
                None => return Task::none(),
            },
            Go::Live(team, m) => match &self.recorder_status {
                // the camera taken: `r` stops the recording
                Some(status) if status.recording.is_some() => {
                    self.stop_recording();
                    return Task::none();
                }
                _ => core.video(&team, &m).map(|(the_match, _)| {
                    Screen::Live(Box::new(live::screen::LiveScreen::new(
                        team,
                        m,
                        the_match.opponent,
                        self.dir.clone(),
                    )))
                }),
            },
            Go::AskCameras => return ask_cameras(),
            Go::PickRecordings => {
                return Task::perform(pick_folder(), |folder| {
                    Message::Live(live::screen::Message::Folder(folder))
                });
            }
            Go::LiveFrame(frame) => {
                return live::screen::allocate(frame.0, frame.1).map(Message::Live);
            }
            Go::Record => return self.record(),
            Go::VideoFrame(frame) => {
                return video::allocate(frame.0, frame.1).map(Message::Video);
            }
            Go::Montage => {
                if let Screen::Video(screen) = &mut self.screen {
                    screen.start_montage(self.strings);
                }
                return Task::none();
            }
            Go::Share(heading, path) => {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                return self.offer(transfer::Sharing {
                    heading,
                    name,
                    content: transfer::Content::File(path),
                });
            }
            Go::PickVideo => {
                return Task::perform(video::pick(self.strings.video_files), |path| {
                    Message::Video(video::Message::Chosen(path))
                });
            }
            Go::Stats(team, m) => {
                stats::StatsScreen::new(core, team, m).map(|screen| Screen::Stats(Box::new(screen)))
            }
            Go::Analysis(team) => stats::StatsScreen::team(core, team, self.strings)
                .map(|screen| Screen::Stats(Box::new(screen))),
            Go::Scout(open) if open.start => core
                .set_start(open.team_id.clone(), open.match_id.clone())
                .map(|start| {
                    Screen::Start(Box::new(start::StartScreen::new(
                        start,
                        open.team_id,
                        open.match_id,
                    )))
                }),
            Go::Scout(open) => core
                .scouting_in_order(&open.team_id, &open.match_id, ChoiceOrder::EventFirst)
                .map(|scouting| {
                    Screen::Scouting(Box::new(ScoutingScreen::new(
                        scouting,
                        self.keys.clone(),
                        open.team_id,
                        open.match_id,
                    )))
                }),
        };
        match opened {
            Ok(screen) => {
                self.error = None;
                self.screen = screen;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
        Task::none()
    }

    /// The actions picked in the statistics, in the video: the statistics
    /// parked, to go back to them as they were.
    fn watch(&mut self, opened: Result<video::VideoScreen, scout4all_mobile::MobileError>) {
        match opened {
            Ok(screen) => {
                self.error = None;
                let left = std::mem::replace(&mut self.screen, Screen::Video(Box::new(screen)));
                if let Screen::Stats(stats) = left {
                    self.parked = Some(stats);
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// The archive made, then shared or saved where chosen (only saved
    /// where the system has no sheet to share with).
    fn export(&mut self, export: transfer::Export) -> Task<Message> {
        let s = self.strings;
        let heading = match export {
            transfer::Export::Team(_) => s.export_team,
            transfer::Export::Match(..) => s.export_match,
            transfer::Export::Report(..) => s.match_report,
        };
        match transfer::archive(&self.core, &export) {
            Ok((name, bytes)) => self.offer(transfer::Sharing {
                heading: heading.to_string(),
                name,
                content: transfer::Content::Bytes(Arc::new(bytes)),
            }),
            Err(e) => {
                self.error = Some(e.to_string());
                Task::none()
            }
        }
    }

    /// A file made, to share or save: asked in a dialog, or saved where
    /// the system has no sheet to share with.
    fn offer(&mut self, sharing: transfer::Sharing) -> Task<Message> {
        match share::NATIVE {
            true => {
                self.sharing = Some(sharing);
                Task::none()
            }
            false => save(sharing),
        }
    }

    /// The file shared with the sheet of the system, or saved: chosen in
    /// the dialog.
    fn sharing(&mut self, message: transfer::ShareMessage) -> Task<Message> {
        let Some(sharing) = self.sharing.take() else {
            return Task::none();
        };
        match message {
            transfer::ShareMessage::Cancel => Task::none(),
            transfer::ShareMessage::Save => save(sharing),
            transfer::ShareMessage::Share => match match &sharing.content {
                transfer::Content::Bytes(bytes) => {
                    share::staged(&sharing.name, bytes).map_err(|e| ui::io_error(&e, self.language))
                }
                transfer::Content::File(path) => Ok(path.clone()),
            } {
                Ok(path) => share::share(path, sharing.heading).map(Message::Shared),
                Err(e) => {
                    self.error = Some(e);
                    Task::none()
                }
            },
        }
    }

    /// The file written where chosen.
    fn exported(&mut self, path: Option<PathBuf>, content: &transfer::Content) {
        let s = self.strings;
        match path.map(|path| content.write(&path).map(|()| path)) {
            None => {}
            Some(Ok(path)) => {
                self.notice = Some(format!("{}: {}", s.archive_saved, path.display()));
            }
            Some(Err(e)) => {
                let why = ui::io_error(&e, self.language);
                self.error = Some(format!("{}: {why}", s.archive_not_saved));
            }
        }
    }

    /// The file chosen: an archive shown before importing it, a
    /// DataVolley file to its screen.
    fn picked(&mut self, team: Option<String>, picked: transfer::Picked) {
        let core = self.core.clone();
        match transfer::is_dvw(&picked.name) {
            true => {
                match dvw::DvwScreen::new(core, self.strings, picked.name, picked.bytes, team) {
                    Ok(screen) => {
                        self.error = None;
                        self.screen = Screen::Dvw(Box::new(screen));
                    }
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            false => match transfer::Importing::new(&core, picked, team) {
                Ok(importing) => self.importing = Some(importing),
                Err(e) => self.error = Some(e.to_string()),
            },
        }
    }

    /// After a key or a click on the archive to import.
    fn importing(&mut self, outcome: transfer::Outcome) -> Go {
        let s = self.strings;
        let imported = match (outcome, &self.importing) {
            (transfer::Outcome::Stay, _) | (_, None) => return Go::Stay,
            (transfer::Outcome::Cancel, Some(_)) => None,
            (transfer::Outcome::Import, Some(importing)) => Some(importing.import(&self.core)),
        };
        self.importing = None;
        match imported {
            None => Go::Stay,
            Some(Ok(transfer::Imported::Team(team, summary))) => {
                self.notice = Some(told(s.team_imported, summary));
                Go::Team(team)
            }
            Some(Ok(transfer::Imported::Match(team, m, summary))) => {
                self.notice = Some(told(s.match_imported, summary));
                Go::Match(team, m)
            }
            Some(Err(e)) => {
                self.error = Some(e.to_string());
                Go::Stay
            }
        }
    }

    /// After a key or a click on the start of a set: back to the match, or
    /// the set saved and scouted.
    fn started(&mut self, outcome: start::Outcome) -> Go {
        let Screen::Start(screen) = &mut self.screen else {
            return Go::Stay;
        };
        let (team_id, match_id) = (screen.team_id.clone(), screen.match_id.clone());
        match outcome {
            start::Outcome::Stay => Go::Stay,
            start::Outcome::Back => Go::Match(team_id, match_id),
            start::Outcome::Start => match self.core.start_set(screen.set_start()) {
                Ok(()) => Go::Scout(home::Open {
                    team_id,
                    match_id,
                    start: false,
                }),
                Err(e) => {
                    screen.failed(e.to_string());
                    Go::Stay
                }
            },
        }
    }

    fn open_settings(&mut self) {
        self.settings = Some(SettingsScreen::new(
            self.core.clone(),
            self.dir.clone(),
            self.keys.clone(),
            self.language,
            self.strings,
        ));
    }

    /// The screen shown, to open again: in another language.
    fn here(&self) -> Go {
        match &self.screen {
            Screen::Home(_) => Go::Home,
            Screen::Team(t) => Go::Team(t.team_id.clone()),
            Screen::Match(m) => Go::Match(m.team_id.clone(), m.match_id.clone()),
            Screen::Stats(st) => match st.match_id() {
                Some(m) => Go::Stats(st.team_id.clone(), m),
                None => Go::Analysis(st.team_id.clone()),
            },
            Screen::Events(e) => Go::Events(e.team_id.clone(), e.match_id.clone(), e.set_number()),
            Screen::Video(v) => Go::Video(v.team_id.clone(), v.match_id.clone()),
            Screen::Live(l) => Go::Live(l.team_id.clone(), l.match_id.clone()),
            Screen::Start(st) => Go::Scout(home::Open {
                team_id: st.team_id.clone(),
                match_id: st.match_id.clone(),
                start: true,
            }),
            Screen::Scouting(sc) => Go::Scout(home::Open {
                team_id: sc.team_id.clone(),
                match_id: sc.match_id.clone(),
                start: false,
            }),
            // the import goes on as it was
            Screen::Dvw(_) => Go::Stay,
        }
    }

    /// The language chosen in the settings: the words of the app, and the
    /// screen behind read again in it.
    fn follow_language(&mut self) -> Task<Message> {
        let chosen = self.settings.as_ref().map(SettingsScreen::language);
        match chosen {
            Some(language) if language != self.language => {
                self.language = language;
                self.strings = strings::strings(language);
                if let Some(settings) = &mut self.settings {
                    settings.relabel(self.strings);
                }
                let here = self.here();
                self.go(here)
            }
            _ => Task::none(),
        }
    }

    /// While the terms are shown: Enter accepts them, space turns the
    /// statistics on or off.
    fn on_terms(&mut self, message: &Message) -> bool {
        if self.terms.is_none() {
            return false;
        }
        let accept = matches!(message, Message::AcceptTerms)
            || matches!(message, Message::Key(ck) if *ck == scout4all_keys::key!(enter));
        let toggle = matches!(message, Message::ToggleStatistics)
            || matches!(message, Message::Key(ck) if *ck == scout4all_keys::key!(space));
        match (accept, toggle, message) {
            (true, _, _) => {
                // not saved: shown again at the next start, as the TUI
                let _ = self.core.accept_terms();
                self.terms = None;
                self.core.send_uploads();
            }
            (_, true, _) => {
                let on = self.core.analytics_enabled();
                if let Err(e) = self.core.set_analytics_enabled(!on) {
                    self.error = Some(e.to_string());
                }
            }
            (_, _, Message::ToggleTheme) => self.toggle_theme(),
            _ => {}
        }
        true
    }

    fn close_settings(&mut self) {
        if let Some(settings) = self.settings.take() {
            self.keys = settings.keys;
            if let Screen::Scouting(screen) = &mut self.screen {
                screen.set_keys(self.keys.clone());
            }
        }
    }

    /// The text field of the screen the keys are on, if any.
    fn focus_id(&self) -> Option<Id> {
        match (&self.settings, &self.screen) {
            (_, _)
                if self.importing.is_some() || self.sharing.is_some() || self.terms.is_some() =>
            {
                None
            }
            (Some(settings), _) => settings.focus_id(),
            (None, Screen::Home(screen)) => screen.focus_id(),
            (None, Screen::Team(screen)) => screen.focus_id(),
            (None, Screen::Match(screen)) => screen.focus_id(),
            (None, Screen::Dvw(screen)) => screen.focus_id(),
            (None, Screen::Video(screen)) => screen.focus_id(),
            (None, Screen::Live(screen)) => screen.focus_id(),
            (
                None,
                Screen::Start(_) | Screen::Scouting(_) | Screen::Stats(_) | Screen::Events(_),
            ) => None,
        }
    }

    /// Updates, then moves the focus where the keys went.
    fn update(&mut self, message: Message) -> Task<Message> {
        let handled = match message {
            Message::WindowClosed(id) => self.window_closed(id),
            Message::LiveTick => self.live_tick(),
            Message::StopRecording => {
                self.stop_recording();
                Task::none()
            }
            Message::TogglePreview => {
                self.preview_asked = !self.preview_asked;
                Task::none()
            }
            Message::Key(ck)
                if live::preview::asked_after_key(
                    self.preview_asked,
                    &ck,
                    self.recorder_status.as_ref(),
                )
                .is_some() =>
            {
                self.preview_asked = !self.preview_asked;
                Task::none()
            }
            Message::PreviewTick => self.preview_tick(),
            Message::PreviewAllocated(frame, handle, allocation) => {
                self.recording_preview.allocated(frame, handle, allocation);
                Task::none()
            }
            message => self.handle(message),
        };
        let handled = Task::batch([handled, self.video_window(), self.preview_window()]);
        // the event chosen kept in sight
        let scroll = match &mut self.screen {
            Screen::Events(screen) => Some(screen.scroll()),
            // only once the keys move the event chosen: the frames of the
            // video don't take the list back while scrolled
            Screen::Video(screen) => screen.take_scroll(),
            Screen::Stats(screen) => screen.take_scroll(),
            _ => None,
        };
        let task = match scroll {
            Some((id, y)) => Task::batch([
                handled,
                iced::widget::operation::snap_to(
                    id,
                    iced::widget::operation::RelativeOffset {
                        x: None,
                        y: Some(y),
                    },
                ),
            ]),
            _ => handled,
        };
        let focus = self.focus_id();
        match focus == self.focused {
            true => task,
            false => {
                self.focused = focus.clone();
                Task::batch([
                    task,
                    focus.map_or_else(Task::none, iced::widget::operation::focus),
                ])
            }
        }
    }

    /// The camera of the source screen recording into the folder chosen,
    /// the app keeping it; back to the match.
    fn record(&mut self) -> Task<Message> {
        let Screen::Live(screen) = &mut self.screen else {
            return Task::none();
        };
        let Some((capture, quality, folder)) = screen.take_capture() else {
            return Task::none();
        };
        let (team, m) = (screen.team_id.clone(), screen.match_id.clone());
        let started = chrono::Local::now();
        let base = live::capture::base_name(&folder, started);
        let sidecar = live::Sidecar {
            team_id: team.clone(),
            match_id: m.clone(),
            started: started.fixed_offset(),
        };
        let written = std::fs::create_dir_all(&folder)
            .map_err(|e| format!("{}: {e}", folder.display()))
            .and_then(|()| live::write_sidecar(&base, &sidecar));
        match written {
            Ok(()) => {
                capture.record(base, started, quality);
                // nothing shows it while scouting: the recording keeps up
                capture.preview(false);
                self.recorder = Some(capture);
                self.go(Go::Match(team, m))
            }
            Err(e) => {
                self.error = Some(e);
                Task::none()
            }
        }
    }

    fn stop_recording(&mut self) {
        if let Some(recorder) = &self.recorder {
            recorder.stop();
        }
    }

    /// The recording looked at: the ones saved given to their matches; the
    /// camera let go once nothing is left to save.
    fn live_tick(&mut self) -> Task<Message> {
        let s = self.strings;
        let Some(recorder) = &self.recorder else {
            return Task::none();
        };
        let status = recorder.status();
        for done in recorder.take_done() {
            let attached = done.result.and_then(|_| {
                live::attach(&self.core, &self.dir, &done.base, &done.mp4, done.started)
            });
            let path = done.mp4.display().to_string();
            match (attached, done.out_of_space) {
                (Ok(()), false) => self.notice = Some(ui::fill(s.recording_saved, &[path])),
                // stopped before the end of the match: in red
                (Ok(()), true) => {
                    self.error = Some(ui::fill(s.recording_out_of_space, &[path]));
                }
                (Err(e), _) => self.error = Some(ui::fill(s.recording_failed, &[e])),
            }
        }
        let idle = status
            .as_ref()
            .is_none_or(|st| st.recording.is_none() && !st.saving);
        match idle {
            true => {
                self.recorder = None;
                self.recorder_status = None;
            }
            false => self.recorder_status = status,
        }
        Task::none()
    }

    /// The own window of the video, open while the screen wants it: closed
    /// once the screen is left or the video brought back.
    fn video_window(&mut self) -> Task<Message> {
        let wanted = matches!(&self.screen, Screen::Video(screen) if screen.detached());
        match (wanted, self.video_window) {
            (true, None) => {
                let (id, opened) = window::open(window::Settings {
                    size: Size::new(1280.0, 720.0),
                    min_size: Some(Size::new(320.0, 180.0)),
                    ..window::Settings::default()
                });
                self.video_window = Some(id);
                opened.discard()
            }
            (false, Some(id)) => {
                self.video_window = None;
                window::close(id)
            }
            _ => Task::none(),
        }
    }

    /// The recording's own window, open while asked for and recording:
    /// the pictures made only then, for the recording to keep up.
    fn preview_window(&mut self) -> Task<Message> {
        let wanted =
            live::preview::window_wanted(self.preview_asked, self.recorder_status.as_ref());
        match (wanted, self.preview_window) {
            (true, None) => {
                let (id, opened) = window::open(window::Settings {
                    size: Size::new(960.0, 580.0),
                    min_size: Some(Size::new(320.0, 200.0)),
                    ..window::Settings::default()
                });
                self.preview_window = Some(id);
                self.show_preview(true);
                // the keys stay on the screen being scouted
                opened.then(|_| window::oldest().and_then(window::gain_focus))
            }
            (false, Some(id)) => {
                self.preview_window = None;
                self.show_preview(false);
                window::close(id)
            }
            _ => Task::none(),
        }
    }

    /// The pictures of the recording made as often as the window looks
    /// for them, or not; the last one gone.
    fn show_preview(&mut self, on: bool) {
        match (&self.recorder, on) {
            (Some(recorder), true) => recorder.preview_every(std::time::Duration::from_millis(
                live::preview::WINDOW_EVERY_MS,
            )),
            (Some(recorder), false) => recorder.preview(false),
            (None, _) => {}
        }
        self.recording_preview.clear();
    }

    /// A newer picture of the recording, for its window.
    fn preview_tick(&mut self) -> Task<Message> {
        let status = self
            .recorder
            .as_ref()
            .and_then(live::capture::Capture::status);
        match self.recording_preview.wanted(status.as_ref()) {
            Some((frame, handle)) => {
                live::preview::allocate(frame, handle, Message::PreviewAllocated)
            }
            None => Task::none(),
        }
    }

    /// What the recording's own window shows: its pictures, how long.
    fn preview_window_view(&self) -> Element<'_, Message> {
        let s = self.strings;
        let c = self.colors();
        let status = self.recorder_status.as_ref();
        let notice = status.and_then(|st| st.error.clone());
        let elapsed = status
            .and_then(|st| st.recording.as_ref())
            .map(|r| format!("● REC {}", video::clock(r.elapsed_ms())))
            .unwrap_or_default();
        let below = container(ui::body(elapsed).color(c.danger))
            .padding(iced::Padding::from([6, 12]))
            .width(Length::Fill);
        container(column![
            self.recording_preview.view(notice, s.preview_waiting, c),
            below
        ])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(c.background.into()),
            text_color: Some(c.text),
            ..container::Style::default()
        })
        .into()
    }

    /// The video's window closed by the user: the video back in the
    /// screen. The recording's: not asked for any more. Closed by the
    /// app (`video_window`, `preview_window`), already forgotten.
    fn window_closed(&mut self, id: window::Id) -> Task<Message> {
        if self.preview_window == Some(id) {
            self.preview_window = None;
            self.preview_asked = false;
            self.show_preview(false);
        }
        if let (true, Screen::Video(screen)) = (self.video_window == Some(id), &mut self.screen) {
            screen.attach();
        }
        if self.video_window == Some(id) {
            self.video_window = None;
        }
        Task::none()
    }

    /// What the own window of the video shows.
    fn video_window_view(&self) -> Element<'_, Message> {
        match &self.screen {
            Screen::Video(screen) => screen
                .window_view(self.strings, self.colors())
                .map(Message::Video),
            _ => space::vertical().into(),
        }
    }

    /// The self update: offered, installed, then the app started again.
    fn on_update(&mut self, message: &Message) -> Option<Task<Message>> {
        let s = self.strings;
        let install = matches!(message, Message::InstallUpdate)
            || matches!(
                (message, &self.screen, &self.settings, &self.importing, &self.terms),
                (Message::Key(ck), Screen::Home(_), None, None, None)
                    if *ck == scout4all_keys::key!(u)
            );
        match (message, &self.update) {
            (Message::UpdateFound(found), _) => {
                self.update = found.clone().map_or(Update::None, Update::Available);
                Some(Task::none())
            }
            (_, Update::Available(version)) if install => {
                self.update = Update::Installing(version.clone());
                Some(Task::perform(update::install(), Message::Installed))
            }
            (Message::Installed(Ok(_)), _) => {
                // returns only when the new version can't start
                let why = update::restart();
                self.update = Update::None;
                self.error = Some(ui::fill(s.update_failed, &[why]));
                Some(Task::none())
            }
            (Message::Installed(Err(why)), _) => {
                self.update = Update::None;
                self.error = Some(ui::fill(s.update_failed, std::slice::from_ref(why)));
                Some(Task::none())
            }
            _ => None,
        }
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        let s = self.strings;
        if let Some(task) = self.on_update(&message) {
            return task;
        }
        // what went well is said until the next key or click
        if !matches!(
            message,
            Message::Frame(_) | Message::Loaded(_) | Message::Exported(..) | Message::Picked(..)
        ) {
            self.notice = None;
        }
        // the files: chosen or saved, whatever the screen
        let message = match message {
            Message::Exported(path, content) => {
                self.exported(path, &content);
                return Task::none();
            }
            Message::Picked(team, Some(picked)) => {
                self.picked(team, picked);
                return Task::none();
            }
            Message::Picked(_, None) => return Task::none(),
            Message::Shared(Err(e)) => {
                self.error = Some(e);
                return Task::none();
            }
            Message::Shared(Ok(())) => return Task::none(),
            other => other,
        };
        // the file to share, while asked, takes the keys
        let chosen = match (&message, &self.sharing) {
            (Message::Key(ck), Some(sharing)) => match sharing.key(*ck) {
                Some(chosen) => Some(chosen),
                None => return Task::none(),
            },
            (Message::Sharing(chosen), Some(_)) => Some(chosen.clone()),
            (Message::Back | Message::GoHome, Some(_)) => Some(transfer::ShareMessage::Cancel),
            _ => None,
        };
        if let Some(chosen) = chosen {
            return self.sharing(chosen);
        }
        if let Message::Quit = message {
            return iced::exit();
        }
        if self.on_terms(&message) {
            return Task::none();
        }
        if let Message::GoHome = message {
            self.importing = None;
            self.close_settings();
            return self.go(Go::Home);
        }
        // the archive to import, while shown, takes the keys
        let imported = match (&message, &mut self.importing) {
            (Message::Key(ck), Some(importing)) => Some(importing.key(*ck)),
            (Message::Importing(m), Some(importing)) => Some(importing.update(m.clone())),
            (Message::Back, Some(_)) => Some(transfer::Outcome::Cancel),
            (Message::ToggleTheme, _) => None,
            (_, Some(_)) => return Task::none(),
            _ => None,
        };
        if let Some(outcome) = imported {
            let go = self.importing(outcome);
            return self.go(go);
        }
        // the settings, while open, take the keys
        let closed = match (&message, &mut self.settings) {
            (Message::Key(ck), Some(settings)) => Some(settings.key(*ck, s)),
            (Message::Settings(m), Some(settings)) => Some(settings.update(m.clone(), s)),
            (Message::Back, Some(settings)) => Some(settings.key(BACK_KEY, s)),
            (Message::Key(ck), None) if *ck == SETTINGS_KEY => {
                self.open_settings();
                return Task::none();
            }
            (Message::OpenSettings, None) => {
                self.open_settings();
                return Task::none();
            }
            _ => None,
        };
        match closed {
            Some(settings::Outcome::Close) => {
                self.close_settings();
                return Task::none();
            }
            Some(settings::Outcome::Stay) => return self.follow_language(),
            None => {}
        }
        let core = self.core.clone();
        let back = |message: Message| match message {
            Message::Back => Message::Key(BACK_KEY),
            other => other,
        };
        let go = match (back(message), &mut self.screen) {
            (Message::ToggleTheme, _) => {
                self.toggle_theme();
                Go::Stay
            }
            (Message::Home(m), Screen::Home(screen)) => home_go(screen.update(&core, s, m)),
            (Message::Key(ck), Screen::Home(screen)) => {
                self.error = None;
                home_go(screen.key(&core, s, ck))
            }
            (Message::Team(m), Screen::Team(screen)) => {
                team_go(&screen.team_id.clone(), screen.update(s, m))
            }
            (Message::Key(ck), Screen::Team(screen)) => {
                team_go(&screen.team_id.clone(), screen.key(s, ck))
            }
            (Message::Match(m), Screen::Match(screen)) => {
                let outcome = screen.update(s, m);
                match_go(screen, outcome)
            }
            (Message::Key(ck), Screen::Match(screen)) => {
                let outcome = screen.key(s, ck);
                match_go(screen, outcome)
            }
            (Message::Stats(m), Screen::Stats(screen)) => {
                let outcome = screen.update(m);
                stats_go(screen, outcome, s)
            }
            (Message::Key(ck), Screen::Stats(screen)) => {
                let outcome = screen.key(ck);
                stats_go(screen, outcome, s)
            }
            (Message::Start(m), Screen::Start(screen)) => {
                let outcome = screen.update(m);
                self.started(outcome)
            }
            (Message::Key(ck), Screen::Start(screen)) => {
                let outcome = screen.key(ck);
                self.started(outcome)
            }
            (Message::Dvw(m), Screen::Dvw(screen)) => {
                let outcome = screen.update(s, m);
                dvw_go(screen, outcome)
            }
            (Message::Events(m), Screen::Events(screen)) => {
                let outcome = screen.update(m);
                events_go(screen, outcome)
            }
            (Message::Key(ck), Screen::Events(screen)) => {
                let outcome = screen.key(ck);
                events_go(screen, outcome)
            }
            (Message::Video(m), Screen::Video(screen)) => {
                let outcome = screen.update(m);
                video_go(screen, outcome)
            }
            (Message::Key(ck), Screen::Video(screen)) => {
                let outcome = screen.key(ck);
                video_go(screen, outcome)
            }
            (Message::Live(m), Screen::Live(screen)) => {
                let outcome = screen.update(m, s);
                live_go(screen, outcome)
            }
            (Message::Key(ck), Screen::Live(screen)) => {
                let outcome = screen.key(ck, s);
                live_go(screen, outcome)
            }
            (Message::Key(ck), Screen::Dvw(screen)) => {
                let outcome = screen.key(s, ck);
                dvw_go(screen, outcome)
            }
            (Message::Scouting(m), Screen::Scouting(screen)) => {
                let outcome = screen.update(m);
                scouting_go(screen, outcome)
            }
            (Message::Key(ck), Screen::Scouting(screen)) => {
                let outcome = screen.key(ck);
                scouting_go(screen, outcome)
            }
            _ => Go::Stay,
        };
        self.go(go)
    }

    fn view(&self) -> Element<'_, Message> {
        let c = self.colors();
        let s = self.strings;
        let (screen, hints, back): (Element<'_, Message>, _, _) =
            match (&self.settings, &self.importing, &self.screen) {
                // the terms, over everything until accepted
                _ if self.terms.is_some() => match &self.terms {
                    Some(terms) => (self.terms_view(terms), self.terms_hints(), false),
                    None => (space::vertical().into(), vec![], false),
                },
                // the file to share, over the screen
                _ if self.sharing.is_some() => match &self.sharing {
                    Some(sharing) => (
                        sharing.view(s, c).map(Message::Sharing),
                        sharing.hints(s),
                        true,
                    ),
                    None => (space::vertical().into(), vec![], false),
                },
                // the archive to import, over the screen
                (None, Some(importing), _) => (
                    importing.view(s, c).map(Message::Importing),
                    importing.hints(s),
                    true,
                ),
                (Some(settings), _, _) => (
                    settings.view(s, c).map(Message::Settings),
                    settings.hints(s),
                    true,
                ),
                (None, None, Screen::Home(screen)) => {
                    (screen.view(s, c).map(Message::Home), screen.hints(s), false)
                }
                (None, None, Screen::Team(screen)) => {
                    (screen.view(s, c).map(Message::Team), screen.hints(s), true)
                }
                (None, None, Screen::Match(screen)) => {
                    (screen.view(s, c).map(Message::Match), screen.hints(s), true)
                }
                (None, None, Screen::Start(screen)) => {
                    (screen.view(s, c).map(Message::Start), screen.hints(s), true)
                }
                (None, None, Screen::Stats(screen)) => {
                    (screen.view(s, c).map(Message::Stats), screen.hints(s), true)
                }
                (None, None, Screen::Events(screen)) => (
                    screen.view(s, c).map(Message::Events),
                    screen.hints(s),
                    true,
                ),
                (None, None, Screen::Video(screen)) => {
                    (screen.view(s, c).map(Message::Video), screen.hints(s), true)
                }
                (None, None, Screen::Live(screen)) => {
                    (screen.view(s, c).map(Message::Live), screen.hints(s), true)
                }
                (None, None, Screen::Dvw(screen)) => {
                    (screen.view(s, c).map(Message::Dvw), screen.hints(s), true)
                }
                (None, None, Screen::Scouting(screen)) => (
                    screen.view(s, c).map(Message::Scouting),
                    screen.hints(s),
                    true,
                ),
            };
        // the recording, on every screen: how long, a click stops it
        let recording: Vec<Element<'_, Message>> = match &self.recorder_status {
            Some(live::capture::Status {
                recording: Some(r), ..
            }) => vec![
                ui::tip(
                    ui::pill(
                        s.preview,
                        self.preview_window.is_some(),
                        Some(Message::TogglePreview),
                        c,
                    ),
                    s.preview_tip,
                    c,
                ),
                ui::tip(
                    ui::pill(
                        &format!("● REC {}", video::clock(r.elapsed_ms())),
                        true,
                        Some(Message::StopRecording),
                        c,
                    ),
                    s.stop_recording,
                    c,
                ),
            ],
            Some(live::capture::Status { saving: true, .. }) => {
                vec![ui::pill(s.saving_recording, false, None, c)]
            }
            _ => vec![],
        };
        let update: Vec<Element<'_, Message>> = match &self.update {
            Update::None => vec![],
            Update::Available(version) => vec![ui::pill(
                &format!("↑ {}", ui::fill(s.update_to, std::slice::from_ref(version))),
                true,
                Some(Message::InstallUpdate),
                c,
            )],
            Update::Installing(version) => vec![ui::pill(
                &ui::fill(s.updating, std::slice::from_ref(version)),
                false,
                None,
                c,
            )],
        };
        // the home from any screen but itself
        let at_home = matches!(self.screen, Screen::Home(_))
            && self.settings.is_none()
            && self.importing.is_none();
        let home = (!at_home && self.terms.is_none()).then_some(Message::GoHome);
        let top = ui::top_bar(
            back.then_some(Message::Back),
            home,
            self.title(),
            recording
                .into_iter()
                .chain(update)
                .chain([
                    ui::bar_button(theme::icons::THEME, s.switch_theme, Message::ToggleTheme, c),
                    ui::bar_button(theme::icons::SETTINGS, s.settings, Message::OpenSettings, c),
                    ui::bar_button(theme::icons::QUIT, s.quit, Message::Quit, c),
                ])
                .collect(),
            s,
            c,
        );
        // `u` updates from the home
        let hints = match (&self.update, &self.screen, &self.settings, &self.importing) {
            (Update::Available(version), Screen::Home(_), None, None) if self.terms.is_none() => [
                hints,
                vec![(
                    "u".to_string(),
                    ui::fill(s.update_to, std::slice::from_ref(version)),
                )],
            ]
            .concat(),
            _ => hints,
        };
        let error: Element<'_, Message> = match (&self.error, &self.notice) {
            (Some(e), _) => ui::body(e.clone()).color(c.danger).into(),
            (None, Some(notice)) => ui::body(notice.clone()).color(c.accent).into(),
            (None, None) => space::vertical().height(0).into(),
        };
        let content = container(column![error, screen].spacing(10))
            .padding(16)
            .width(Length::Fill)
            .height(Length::Fill);
        container(column![top, content, ui::bottom_bar(hints, c)])
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(c.background.into()),
                text_color: Some(c.text),
                ..container::Style::default()
            })
            .into()
    }

    fn terms_view<'a>(&'a self, terms: &'a scout4all_mobile::Terms) -> Element<'a, Message> {
        let s = self.strings;
        let c = self.colors();
        let on = self.core.analytics_enabled();
        let below = column![
            ui::list_row(
                iced::widget::row![
                    ui::body(match on {
                        true => "■",
                        false => "□",
                    })
                    .color(c.accent),
                    column![
                        ui::body(s.send_statistics),
                        iced::widget::text(s.send_statistics_detail)
                            .size(theme::SIZE_SMALL)
                            .color(c.muted),
                    ]
                    .spacing(2),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center),
                false,
                Some(Message::ToggleStatistics),
                c,
            ),
            ui::answers(
                None,
                theme::icons::CHECK,
                s.accept_terms,
                Some(Message::AcceptTerms),
                c,
            ),
        ]
        .spacing(14);
        general::terms_view(terms, Some(below.into()), c)
    }

    fn terms_hints(&self) -> Vec<(String, String)> {
        let s = self.strings;
        vec![
            (s.space.to_string(), s.send_statistics.to_string()),
            ("Enter".to_string(), s.accept_terms.to_string()),
        ]
    }

    fn subscription(&self) -> Subscription<Message> {
        // the frames of the video, while it plays or a frame is on its way
        let video = match (&self.screen, &self.settings) {
            (Screen::Video(screen), None) if screen.awake() => {
                window::frames().map(|_| Message::Video(video::Message::Tick))
            }
            (Screen::Live(screen), None) if screen.awake() => {
                window::frames().map(|_| Message::Live(live::screen::Message::Tick))
            }
            _ => Subscription::none(),
        };
        // the recording, twice a second, whatever the screen; its
        // pictures, while its window is open
        let recording = match self.recorder {
            Some(_) => Subscription::run(live_ticks),
            None => Subscription::none(),
        };
        let preview = match self.preview_window {
            Some(_) => Subscription::run(preview_ticks),
            None => Subscription::none(),
        };
        let keys = event::listen_with(|event, status, _| match (event, status) {
            (
                Event::Keyboard(keyboard::Event::KeyPressed {
                    modified_key,
                    modifiers,
                    ..
                }),
                status,
            ) if status == event::Status::Ignored
                // Esc leaves a form also from a text field, which takes it;
                // F4 opens the preview of the recording from anywhere
                || modified_key == keyboard::Key::Named(keyboard::key::Named::Escape)
                || modified_key == keyboard::Key::Named(keyboard::key::Named::F4) =>
            {
                keymap::combination(&modified_key, modifiers).map(Message::Key)
            }
            _ => None,
        });
        Subscription::batch([keys, video, recording, preview])
    }
}

fn home_go(outcome: home::Outcome) -> Go {
    match outcome {
        home::Outcome::Stay => Go::Stay,
        home::Outcome::Import => Go::Import(None),
        home::Outcome::Scout(open) => Go::Scout(open),
        home::Outcome::Match(team, m) => Go::Match(team, m),
        home::Outcome::Team(team) => Go::Team(team),
    }
}

fn team_go(team_id: &str, outcome: team::Outcome) -> Go {
    match outcome {
        team::Outcome::Stay => Go::Stay,
        team::Outcome::Back => Go::Home,
        team::Outcome::OpenMatch(m) => Go::Match(team_id.to_string(), m),
        team::Outcome::Import => Go::Import(Some(team_id.to_string())),
        team::Outcome::Export => Go::Export(transfer::Export::Team(team_id.to_string())),
        team::Outcome::Analysis => Go::Analysis(team_id.to_string()),
    }
}

fn live_go(screen: &live::screen::LiveScreen, outcome: live::screen::Outcome) -> Go {
    match outcome {
        live::screen::Outcome::Stay => Go::Stay,
        live::screen::Outcome::Back => Go::Match(screen.team_id.clone(), screen.match_id.clone()),
        live::screen::Outcome::AskPermission => Go::AskCameras,
        live::screen::Outcome::PickFolder => Go::PickRecordings,
        live::screen::Outcome::Allocate(frame, handle) => Go::LiveFrame(Box::new((frame, handle))),
        live::screen::Outcome::Record => Go::Record,
    }
}

/// Whether the cameras may be used: the Mac asks the user the first time.
fn ask_cameras() -> Task<Message> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    let tx = Mutex::new(Some(tx));
    live::capture::ask_permission(move |allowed| {
        if let Some(tx) = tx.lock().ok().and_then(|mut tx| tx.take()) {
            let _ = tx.send(allowed);
        }
    });
    Task::perform(async move { rx.await.unwrap_or(false) }, |allowed| {
        Message::Live(live::screen::Message::Allowed(allowed))
    })
}

async fn pick_folder() -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .pick_folder()
        .await
        .map(|f| f.path().to_path_buf())
}

/// A tick every half a second, from a thread: no timer in the executor.
fn live_ticks() -> impl iced::futures::Stream<Item = Message> {
    every(500, Message::LiveTick)
}

fn preview_ticks() -> impl iced::futures::Stream<Item = Message> {
    every(live::preview::WINDOW_EVERY_MS, Message::PreviewTick)
}

/// `message` every `ms` milliseconds.
fn every(ms: u64, message: Message) -> impl iced::futures::Stream<Item = Message> {
    iced::futures::stream::unfold(message, move |message| async move {
        let (tx, rx) = iced::futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(ms));
            let _ = tx.send(());
        });
        let _ = rx.await;
        Some((message.clone(), message))
    })
}

fn video_go(screen: &video::VideoScreen, outcome: video::Outcome) -> Go {
    match outcome {
        video::Outcome::Stay => Go::Stay,
        video::Outcome::Back if screen.is_clips() => Go::Unpark,
        video::Outcome::Back => Go::Match(screen.team_id.clone(), screen.match_id.clone()),
        video::Outcome::Choose => Go::PickVideo,
        video::Outcome::Allocate(frame, handle) => Go::VideoFrame(Box::new((frame, handle))),
        video::Outcome::Montage => Go::Montage,
        video::Outcome::Share(path, heading) => Go::Share(heading, path),
    }
}

fn events_go(screen: &events::EventsScreen, outcome: events::Outcome) -> Go {
    match outcome {
        events::Outcome::Stay => Go::Stay,
        events::Outcome::Back => Go::Match(screen.team_id.clone(), screen.match_id.clone()),
    }
}

/// Back where the import was asked from: the team, or the home.
fn dvw_go(screen: &dvw::DvwScreen, outcome: dvw::Outcome) -> Go {
    match outcome {
        dvw::Outcome::Stay => Go::Stay,
        dvw::Outcome::Back => screen.from.clone().map_or(Go::Home, Go::Team),
        dvw::Outcome::Team(team) => Go::Team(team),
    }
}

fn stats_go(screen: &stats::StatsScreen, outcome: stats::Outcome, s: &Strings) -> Go {
    match outcome {
        stats::Outcome::Stay => Go::Stay,
        stats::Outcome::Watch(pick) => match screen.match_id() {
            Some(m) => Go::Clips(screen.team_id.clone(), m, pick, screen.clips_title(s)),
            None => Go::Stay,
        },
        stats::Outcome::WatchTour(matches) => {
            Go::TeamClips(screen.team_id.clone(), matches, screen.clips_title(s))
        }
        stats::Outcome::Back => match screen.match_id() {
            Some(m) => Go::Match(screen.team_id.clone(), m),
            None => Go::Team(screen.team_id.clone()),
        },
    }
}

fn match_go(screen: &MatchScreen, outcome: game::Outcome) -> Go {
    match outcome {
        game::Outcome::Stay => Go::Stay,
        game::Outcome::Back => Go::Team(screen.team_id.clone()),
        game::Outcome::Stats => Go::Stats(screen.team_id.clone(), screen.match_id.clone()),
        game::Outcome::Events(set) => {
            Go::Events(screen.team_id.clone(), screen.match_id.clone(), set)
        }
        game::Outcome::Video => Go::Video(screen.team_id.clone(), screen.match_id.clone()),
        game::Outcome::Record => Go::Live(screen.team_id.clone(), screen.match_id.clone()),
        game::Outcome::Report => Go::Export(transfer::Export::Report(
            screen.team_id.clone(),
            screen.match_id.clone(),
        )),
        game::Outcome::Export => Go::Export(transfer::Export::Match(
            screen.team_id.clone(),
            screen.match_id.clone(),
        )),
        game::Outcome::Scout(start) => Go::Scout(home::Open {
            team_id: screen.team_id.clone(),
            match_id: screen.match_id.clone(),
            start,
        }),
    }
}

/// Leaving the scouting: back to its match.
fn scouting_go(screen: &ScoutingScreen, outcome: Outcome) -> Go {
    match outcome {
        Outcome::Stay => Go::Stay,
        Outcome::Leave => Go::Match(screen.team_id.clone(), screen.match_id.clone()),
    }
}

/// The app once read, from the thread that read it.
#[derive(Clone)]
struct Loaded(Arc<Mutex<Option<App>>>);

impl std::fmt::Debug for Loaded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Loaded")
    }
}

/// The splash until the app is read and the ball has come in, then the
/// app.
struct Main {
    /// the app's window: closed, the app ends
    window: window::Id,
    splash: Option<splash::Splash>,
    app: Option<App>,
    /// the system's theme is dark, given to the app once it's read
    system_dark: bool,
    first_frame: OnceLock<()>,
    /// the screenshots asked with `--shots`
    shots: Option<shots::Shots>,
}

impl Main {
    fn boot() -> (Main, Task<Message>) {
        let shots = match shots::Shots::from_args() {
            Some(Ok(shots)) => Some(shots),
            Some(Err(e)) => {
                eprintln!("{e}");
                std::process::exit(2)
            }
            None => None,
        };
        let quiet = shots.is_some();
        let (tx, rx) = iced::futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(App::load(quiet));
        });
        // the app's window: the others (the video's) open from it; for the
        // screenshots, the size of the plan
        let settings = match &shots {
            Some(shots) => window::Settings {
                size: Size::new(shots.plan_window.0, shots.plan_window.1),
                resizable: false,
                ..window::Settings::default()
            },
            None => window::Settings {
                size: Size::new(1400.0, 860.0),
                min_size: Some(Size::new(1000.0, 640.0)),
                ..window::Settings::default()
            },
        };
        let (window, opened) = window::open(settings);
        let main = Main {
            window,
            splash: Some(splash::Splash::new()),
            app: None,
            system_dark: true,
            first_frame: OnceLock::new(),
            shots,
        };
        let loaded = Task::perform(async move { rx.await.ok() }, |app| {
            Message::Loaded(Loaded(Arc::new(Mutex::new(app))))
        });
        // the system's theme: not for the screenshots, always dark
        let theme = match quiet {
            true => Task::none(),
            false => iced::system::theme().map(Message::SystemTheme),
        };
        (main, Task::batch([opened.discard(), loaded, theme]))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let task = match (message, &mut self.splash, &mut self.app) {
            (Message::Frame(now), Some(splash), _) => {
                splash.tick(now);
                Task::none()
            }
            (Message::Loaded(Loaded(app)), _, _) => {
                let app = app.lock().ok().and_then(|mut app| app.take());
                match app {
                    Some(mut app) => {
                        app.system_dark = self.system_dark;
                        self.app = Some(app);
                    }
                    // the thread ended without the app: nothing to show
                    None => std::process::exit(1),
                }
                if let Some(started) = STARTED.get() {
                    eprintln!("data read after {} ms", started.elapsed().as_millis());
                }
                // a newer release, looked for once the app is there (not
                // for the screenshots)
                match self.shots {
                    Some(_) => Task::none(),
                    None => Task::perform(update::newer(), Message::UpdateFound),
                }
            }
            (Message::SystemTheme(mode), _, app) => {
                // no tone known: as it was
                self.system_dark = match mode {
                    iced::theme::Mode::Light => false,
                    iced::theme::Mode::Dark => true,
                    iced::theme::Mode::None => self.system_dark,
                };
                if let Some(app) = app {
                    app.system_dark = self.system_dark;
                }
                Task::none()
            }
            (Message::Shot(message), _, Some(_)) => return self.shot(message),
            // the answer can come while the splash is still on
            (message @ Message::UpdateFound(_), _, Some(app)) => app.update(message),
            (Message::WindowClosed(id), _, _) if id == self.window => iced::exit(),
            (message @ Message::WindowClosed(_), _, Some(app)) => app.update(message),
            (message, None, Some(app)) => app.update(message),
            _ => Task::none(),
        };
        if let (Some(splash), Some(_)) = (&self.splash, &self.app)
            && splash.done()
        {
            self.splash = None;
        }
        // the app on screen: the screenshots start
        match (&mut self.shots, &self.splash, &self.app) {
            (Some(shots), None, Some(_)) if !shots.started => {
                shots.started = true;
                Task::batch([task, Task::done(Message::Shot(ShotMessage::Next))])
            }
            _ => task,
        }
    }

    /// A step of the screenshots (`shots`).
    fn shot(&mut self, message: ShotMessage) -> Task<Message> {
        let (Some(shots), Some(app)) = (&mut self.shots, &mut self.app) else {
            return Task::none();
        };
        match message {
            ShotMessage::Next => {
                let mut tasks = vec![];
                loop {
                    match shots.next() {
                        shots::Next::Do(shots::Step::Go(open)) => tasks.push(app.go(open.go())),
                        shots::Next::Do(shots::Step::Key(key)) => {
                            tasks.push(app.update(Message::Key(key)));
                        }
                        shots::Next::Do(shots::Step::Keys(keys)) => {
                            tasks.extend(keys.into_iter().map(|key| app.update(Message::Key(key))));
                        }
                        // waits, captures and recordings come as such
                        shots::Next::Do(
                            shots::Step::Wait(_)
                            | shots::Step::Shot(_)
                            | shots::Step::Record(_)
                            | shots::Step::Stop,
                        ) => {}
                        shots::Next::Recording => {
                            tasks.push(Task::done(Message::Shot(ShotMessage::Frame)));
                        }
                        shots::Next::Stopped(done) => match done {
                            Ok(said) => eprintln!("shots: recorded, {said}"),
                            Err(e) => eprintln!("shots: recording: {e}"),
                        },
                        shots::Next::Wait(time) => {
                            tasks.push(Task::perform(shots::after(time), |()| {
                                Message::Shot(ShotMessage::Next)
                            }));
                            break;
                        }
                        shots::Next::Shot(name) => {
                            tasks.push(Task::perform(shots::after(shots::settle()), move |()| {
                                Message::Shot(ShotMessage::Take(name.clone()))
                            }));
                            break;
                        }
                        shots::Next::Done => {
                            eprintln!("shots: done");
                            tasks.push(iced::exit());
                            break;
                        }
                    }
                }
                Task::batch(tasks)
            }
            ShotMessage::Take(name) => window::screenshot(self.window)
                .map(move |shot| Message::Shot(ShotMessage::Taken(name.clone(), shot))),
            ShotMessage::Frame => match shots.recording() {
                true => window::screenshot(self.window)
                    .map(|shot| Message::Shot(ShotMessage::Framed(shot))),
                false => Task::none(),
            },
            ShotMessage::Framed(shot) => {
                shots.frame(&shot);
                Task::done(Message::Shot(ShotMessage::Frame))
            }
            ShotMessage::Taken(name, shot) => {
                match shots.save(&name, &shot) {
                    Ok(path) => eprintln!("shots: {}", path.display()),
                    Err(e) => eprintln!("shots: {e}"),
                }
                Task::done(Message::Shot(ShotMessage::Next))
            }
        }
    }

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        if let Some(app) = self.app.as_ref().filter(|app| app.video_window == Some(id)) {
            return app.video_window_view();
        }
        if let Some(app) = self
            .app
            .as_ref()
            .filter(|app| app.preview_window == Some(id))
        {
            return app.preview_window_view();
        }
        if let (Ok(()), Some(started)) = (self.first_frame.set(()), STARTED.get()) {
            eprintln!("first frame after {} ms", started.elapsed().as_millis());
        }
        match (&self.splash, &self.app) {
            (None, Some(app)) => app.view(),
            _ => self
                .splash
                .as_ref()
                .map_or_else(|| splash::Splash::new().view(), |splash| splash.view()),
        }
    }

    fn theme(&self, _: window::Id) -> Theme {
        theme::theme(self.app.as_ref().is_none_or(App::dark))
    }

    fn title(&self, id: window::Id) -> String {
        match &self.app {
            Some(app) if app.video_window == Some(id) => app.title(),
            Some(app) if app.preview_window == Some(id) => {
                app.strings.recording_preview.to_string()
            }
            _ => "scout4all".to_string(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let closed = window::close_events().map(Message::WindowClosed);
        let app = match (&self.splash, &self.app) {
            (None, Some(app)) => app.subscription(),
            _ => window::frames().map(Message::Frame),
        };
        let theme = match self.shots {
            Some(_) => Subscription::none(),
            None => iced::system::theme_changes().map(Message::SystemTheme),
        };
        Subscription::batch([closed, app, theme])
    }
}

fn main() -> iced::Result {
    let _ = STARTED.set(Instant::now());
    // as the CLI: the self-update of the old clients checks it
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("{}", update::version_line());
        return Ok(());
    }
    let fonts = theme::fonts();
    iced::daemon(Main::boot, Main::update, Main::view)
        .title(Main::title)
        .font(fonts.bytes[0])
        .font(fonts.bytes[1])
        .default_font(fonts.regular)
        .theme(Main::theme)
        .subscription(Main::subscription)
        .run()
}

/// Where to save the file chosen, then it written there.
fn save(sharing: transfer::Sharing) -> Task<Message> {
    let content = sharing.content;
    Task::perform(transfer::save_to(sharing.name), move |path| {
        Message::Exported(path, content.clone())
    })
}
