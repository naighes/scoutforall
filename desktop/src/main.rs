//! The desktop app of scout4all (027), replacing the TUI: the home, the
//! scouting of a set in progress, the keys of the scouting (F2). It reads
//! the data of the TUI (`~/.scout4all`, or `SCOUT4ALL_DATA_DIR`) and its
//! keys.

mod charts;
mod court;
mod editors;
mod form;
mod game;
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
mod ui;

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
}

/// Where a key or a click leads.
enum Go {
    Stay,
    Home,
    Team(String),
    Match(String, String),
    Stats(String, String),
    /// a set of the match: to start, or in progress
    Scout(home::Open),
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
    /// the text field the keys are on, last focused
    focused: Option<Id>,
}

#[derive(Debug, Clone)]
enum Message {
    Key(KeyCombination),
    ToggleTheme,
    /// the arrow on the bar: as Esc
    Back,
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
            focused: None,
        };
        app.go(Go::Home);
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
        }
    }

    fn colors(&self) -> theme::Colors {
        theme::colors(self.dark)
    }

    /// Opens the screen `go` leads to; an error stays on the one shown.
    fn go(&mut self, go: Go) {
        let core = self.core.clone();
        let opened = match go {
            Go::Stay => return,
            Go::Home => core
                .home()
                .map(|home| Screen::Home(Box::new(HomeScreen::new(home)))),
            Go::Team(team) => TeamScreen::new(core, team).map(|t| Screen::Team(Box::new(t))),
            Go::Match(team, m) => {
                MatchScreen::new(core, team, m).map(|m| Screen::Match(Box::new(m)))
            }
            Go::Stats(team, m) => {
                stats::StatsScreen::new(core, team, m).map(|screen| Screen::Stats(Box::new(screen)))
            }
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
            self.dir.clone(),
            self.keys.clone(),
            self.language,
        ));
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
            (Some(_), _) => None,
            (None, Screen::Home(screen)) => screen.focus_id(),
            (None, Screen::Team(screen)) => screen.focus_id(),
            (None, Screen::Match(screen)) => screen.focus_id(),
            (None, Screen::Start(_) | Screen::Scouting(_) | Screen::Stats(_)) => None,
        }
    }

    /// Updates, then moves the focus where the keys went.
    fn update(&mut self, message: Message) -> Task<Message> {
        self.handle(message);
        let focus = self.focus_id();
        match focus == self.focused {
            true => Task::none(),
            false => {
                self.focused = focus.clone();
                focus.map_or_else(Task::none, iced::widget::operation::focus)
            }
        }
    }

    fn handle(&mut self, message: Message) {
        let s = self.strings;
        // the settings, while open, take the keys
        let closed = match (&message, &mut self.settings) {
            (Message::Key(ck), Some(settings)) => Some(settings.key(*ck, s)),
            (Message::Settings(m), Some(settings)) => Some(settings.update(m.clone(), s)),
            (Message::Back, Some(_)) => Some(settings::Outcome::Close),
            (Message::Key(ck), None) if *ck == SETTINGS_KEY => return self.open_settings(),
            (Message::OpenSettings, None) => return self.open_settings(),
            _ => None,
        };
        match closed {
            Some(settings::Outcome::Close) => return self.close_settings(),
            Some(settings::Outcome::Stay) => return,
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
        self.go(go);
    }

    fn view(&self) -> Element<'_, Message> {
        let c = self.colors();
        let s = self.strings;
        let (screen, hints, back): (Element<'_, Message>, _, _) =
            match (&self.settings, &self.screen) {
                (Some(settings), _) => (
                    settings.view(s, c).map(Message::Settings),
                    settings.hints(s),
                    true,
                ),
                (None, Screen::Home(screen)) => {
                    (screen.view(s, c).map(Message::Home), screen.hints(s), false)
                }
                (None, Screen::Team(screen)) => {
                    (screen.view(s, c).map(Message::Team), screen.hints(s), true)
                }
                (None, Screen::Match(screen)) => {
                    (screen.view(s, c).map(Message::Match), screen.hints(s), true)
                }
                (None, Screen::Start(screen)) => {
                    (screen.view(s, c).map(Message::Start), screen.hints(s), true)
                }
                (None, Screen::Stats(screen)) => {
                    (screen.view(s, c).map(Message::Stats), screen.hints(s), true)
                }
                (None, Screen::Scouting(screen)) => (
                    screen.view(s, c).map(Message::Scouting),
                    screen.hints(s),
                    true,
                ),
            };
        let top = ui::top_bar(
            back.then_some(Message::Back),
            self.title(),
            vec![
                ui::bar_button("◐", Message::ToggleTheme, c),
                ui::bar_button("⚙", Message::OpenSettings, c),
            ],
            c,
        );
        let error: Element<'_, Message> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => space::vertical().height(0).into(),
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
    }
}

fn stats_go(screen: &stats::StatsScreen, outcome: stats::Outcome) -> Go {
    match outcome {
        stats::Outcome::Stay => Go::Stay,
        stats::Outcome::Back => Go::Match(screen.team_id.clone(), screen.match_id.clone()),
    }
}

fn match_go(screen: &MatchScreen, outcome: game::Outcome) -> Go {
    match outcome {
        game::Outcome::Stay => Go::Stay,
        game::Outcome::Back => Go::Team(screen.team_id.clone()),
        game::Outcome::Stats => Go::Stats(screen.team_id.clone(), screen.match_id.clone()),
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
                Task::none()
            }
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
        println!("scout4all {}", scout4all_mobile::version());
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
