//! The desktop app of scout4all (027), replacing the TUI: the home, the
//! scouting of a set in progress, the keys of the scouting (F2). It reads
//! the data of the TUI (`~/.scout4all`, or `SCOUT4ALL_DATA_DIR`) and its
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
mod scouting;
mod settings;
mod splash;
mod start;
mod stats;
mod strings;
mod team;
mod theme;
mod transfer;
mod ui;
mod update;

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

/// What the TUI keeps in `config.json` that the prototype reads: the
/// language and the keys.
fn config(dir: &std::path::Path) -> (LanguageEnum, ScoutingKeys) {
    let json: serde_json::Value = std::fs::read_to_string(dir.join("config.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let language = json
        .get("language")
        .and_then(|l| l.as_str())
        .and_then(|l| LanguageEnum::from_str(l).ok())
        .unwrap_or(LanguageEnum::En);
    let keys = serde_json::from_value::<ScoutingKeys>(json)
        .unwrap_or_default()
        .normalized();
    (language, keys)
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
}

/// Where a key or a click leads.
enum Go {
    Stay,
    Home,
    Team(String),
    Match(String, String),
    Stats(String, String),
    /// the statistics of the team over its matches
    Analysis(String),
    /// the events of a set of the match, to correct them
    Events(String, String, u8),
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
    dark: bool,
    screen: Screen,
    error: Option<String>,
    /// what went well: a file saved, imported
    notice: Option<String>,
    /// an archive chosen, to confirm, over the screen
    importing: Option<transfer::Importing>,
    /// the terms of use, at the first start and when they change: over
    /// everything until accepted
    terms: Option<scout4all_mobile::Terms>,
    /// the text field the keys are on, last focused
    focused: Option<Id>,
    /// a newer release, offered in the bar
    update: Update,
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
    /// where the archive goes (none: not saved), and the archive
    Exported(Option<PathBuf>, Arc<Vec<u8>>),
    /// the file chosen (none: none) to import into the team, if given
    Picked(Option<String>, Option<transfer::Picked>),
    Importing(transfer::Message),
    AcceptTerms,
    /// the statistics sent or not, from the terms
    ToggleStatistics,
    /// the version of a newer release, if any
    UpdateFound(Option<String>),
    InstallUpdate,
    /// the new version in place of the app, or why not
    Installed(Result<String, String>),
}

impl App {
    /// Reads the configuration and the data: on a thread, the splash on
    /// screen meanwhile.
    fn load() -> App {
        let dir = data_dir().unwrap_or_else(|| PathBuf::from(".scout4all"));
        let (language, keys) = config(&dir);
        let strings = strings::strings(language);
        let core = Scout4All::new(dir.to_string_lossy().into_owned(), language.to_string());
        let (core, error) = match core {
            Ok(core) => (core, None),
            // an empty folder in place of the data: the error tells why
            Err(e) => match Scout4All::new(
                std::env::temp_dir().to_string_lossy().into_owned(),
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
            dark: true,
            screen: Screen::Home(Box::new(HomeScreen::new(Home {
                in_progress: vec![],
                recent: vec![],
                teams: vec![],
            }))),
            error,
            notice: None,
            importing: None,
            terms: None,
            focused: None,
            update: Update::None,
        };
        app.terms = (!app.core.terms_seen()).then(|| app.core.terms());
        // the matches queued while offline, sent now
        general::send_statistics(app.core.clone());
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
        }
    }

    fn colors(&self) -> theme::Colors {
        theme::colors(self.dark)
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

    /// The archive made, then where to save it chosen.
    fn export(&mut self, export: transfer::Export) -> Task<Message> {
        match transfer::archive(&self.core, &export) {
            Ok((name, bytes)) => {
                let bytes = Arc::new(bytes);
                Task::perform(transfer::save_to(name), move |path| {
                    Message::Exported(path, bytes.clone())
                })
            }
            Err(e) => {
                self.error = Some(e.to_string());
                Task::none()
            }
        }
    }

    /// The archive written where chosen.
    fn exported(&mut self, path: Option<PathBuf>, bytes: &[u8]) {
        let s = self.strings;
        match path.map(|path| std::fs::write(&path, bytes).map(|()| path)) {
            None => {}
            Some(Ok(path)) => {
                self.notice = Some(format!("{}: {}", s.archive_saved, path.display()));
            }
            Some(Err(e)) => self.error = Some(format!("{}: {e}", s.archive_not_saved)),
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
            Some(Ok(transfer::Imported::Team(team))) => {
                self.notice = Some(s.team_imported.to_string());
                Go::Team(team)
            }
            Some(Ok(transfer::Imported::Match(team, m))) => {
                self.notice = Some(s.match_imported.to_string());
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
                general::send_statistics(self.core.clone());
            }
            (_, true, _) => {
                let on = self.core.analytics_enabled();
                if let Err(e) = self.core.set_analytics_enabled(!on) {
                    self.error = Some(e.to_string());
                }
            }
            (_, _, Message::ToggleTheme) => self.dark = !self.dark,
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
            (_, _) if self.importing.is_some() || self.terms.is_some() => None,
            (Some(settings), _) => settings.focus_id(),
            (None, Screen::Home(screen)) => screen.focus_id(),
            (None, Screen::Team(screen)) => screen.focus_id(),
            (None, Screen::Match(screen)) => screen.focus_id(),
            (None, Screen::Dvw(screen)) => screen.focus_id(),
            (
                None,
                Screen::Start(_) | Screen::Scouting(_) | Screen::Stats(_) | Screen::Events(_),
            ) => None,
        }
    }

    /// Updates, then moves the focus where the keys went.
    fn update(&mut self, message: Message) -> Task<Message> {
        let handled = self.handle(message);
        // the event chosen kept in sight
        let task = match &self.screen {
            Screen::Events(screen) => {
                let (id, y) = screen.scroll();
                Task::batch([
                    handled,
                    iced::widget::operation::snap_to(
                        id,
                        iced::widget::operation::RelativeOffset {
                            x: None,
                            y: Some(y),
                        },
                    ),
                ])
            }
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
            Message::Exported(path, bytes) => {
                self.exported(path, &bytes);
                return Task::none();
            }
            Message::Picked(team, Some(picked)) => {
                self.picked(team, picked);
                return Task::none();
            }
            Message::Picked(_, None) => return Task::none(),
            other => other,
        };
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
                self.dark = !self.dark;
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
                stats_go(screen, outcome)
            }
            (Message::Key(ck), Screen::Stats(screen)) => {
                let outcome = screen.key(ck);
                stats_go(screen, outcome)
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
                (None, None, Screen::Dvw(screen)) => {
                    (screen.view(s, c).map(Message::Dvw), screen.hints(s), true)
                }
                (None, None, Screen::Scouting(screen)) => (
                    screen.view(s, c).map(Message::Scouting),
                    screen.hints(s),
                    true,
                ),
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
            update
                .into_iter()
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
            container(ui::pill(
                &format!("Enter  {}", s.accept_terms),
                true,
                Some(Message::AcceptTerms),
                c,
            ))
            .width(Length::Fill)
            .align_x(iced::Alignment::End),
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
        event::listen_with(|event, status, _| match (event, status) {
            (
                Event::Keyboard(keyboard::Event::KeyPressed {
                    modified_key,
                    modifiers,
                    ..
                }),
                status,
            ) if status == event::Status::Ignored
                // Esc leaves a form also from a text field, which takes it
                || modified_key == keyboard::Key::Named(keyboard::key::Named::Escape) =>
            {
                keymap::combination(&modified_key, modifiers).map(Message::Key)
            }
            _ => None,
        })
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

fn stats_go(screen: &stats::StatsScreen, outcome: stats::Outcome) -> Go {
    match outcome {
        stats::Outcome::Stay => Go::Stay,
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
    splash: Option<splash::Splash>,
    app: Option<App>,
    first_frame: OnceLock<()>,
}

impl Main {
    fn boot() -> (Main, Task<Message>) {
        let (tx, rx) = iced::futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(App::load());
        });
        let main = Main {
            splash: Some(splash::Splash::new()),
            app: None,
            first_frame: OnceLock::new(),
        };
        let loaded = Task::perform(async move { rx.await.ok() }, |app| {
            Message::Loaded(Loaded(Arc::new(Mutex::new(app))))
        });
        (main, loaded)
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
                    Some(app) => self.app = Some(app),
                    // the thread ended without the app: nothing to show
                    None => std::process::exit(1),
                }
                if let Some(started) = STARTED.get() {
                    eprintln!("data read after {} ms", started.elapsed().as_millis());
                }
                // a newer release, looked for once the app is there
                Task::perform(update::newer(), Message::UpdateFound)
            }
            // the answer can come while the splash is still on
            (message @ Message::UpdateFound(_), _, Some(app)) => app.update(message),
            (message, None, Some(app)) => app.update(message),
            _ => Task::none(),
        };
        if let (Some(splash), Some(_)) = (&self.splash, &self.app)
            && splash.done()
        {
            self.splash = None;
        }
        task
    }

    fn view(&self) -> Element<'_, Message> {
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

    fn theme(&self) -> Theme {
        theme::theme(self.app.as_ref().is_none_or(|app| app.dark))
    }

    fn subscription(&self) -> Subscription<Message> {
        match (&self.splash, &self.app) {
            (None, Some(app)) => app.subscription(),
            _ => window::frames().map(Message::Frame),
        }
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
    iced::application(Main::boot, Main::update, Main::view)
        .title("scout4all")
        .font(fonts.bytes[0])
        .font(fonts.bytes[1])
        .default_font(fonts.regular)
        .theme(Main::theme)
        .subscription(Main::subscription)
        .window(window::Settings {
            size: Size::new(1400.0, 860.0),
            min_size: Some(Size::new(1000.0, 640.0)),
            ..window::Settings::default()
        })
        .run()
}
