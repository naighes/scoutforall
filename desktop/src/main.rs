//! The desktop app of scout4all (027), replacing the TUI: the home, the
//! scouting of a set in progress, the keys of the scouting (F2). It reads
//! the data of the TUI (`~/.scout4all`, or `SCOUT4ALL_DATA_DIR`) and its
//! keys.

mod charts;
mod court;
mod home;
mod keymap;
mod scouting;
mod settings;
mod strings;
mod theme;
mod ui;

use {
    iced::{
        Element, Event, Length, Size, Subscription, Task, Theme, event, keyboard,
        widget::{column, container, space},
        window,
    },
    scout4all_core::{scouting::ChoiceOrder, shapes::enums::LanguageEnum},
    scout4all_keys::{BACK_KEY, KeyCombination, SETTINGS_KEY, ScoutingKeys, key},
    scout4all_mobile::{Home, Scout4All},
    scouting::{Outcome, ScoutingScreen},
    settings::SettingsScreen,
    std::{path::PathBuf, str::FromStr, sync::Arc, sync::OnceLock, time::Instant},
    strings::Strings,
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
    Home { home: Home, selected: usize },
    Scouting(Box<ScoutingScreen>),
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
    first_frame: OnceLock<()>,
}

#[derive(Debug, Clone)]
enum Message {
    Key(KeyCombination),
    Open(home::Open),
    ToggleTheme,
    /// the arrow on the bar: as Esc
    Back,
    OpenSettings,
    Scouting(scouting::Message),
    Settings(settings::Message),
}

impl App {
    fn boot() -> (App, Task<Message>) {
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
            screen: Screen::Home {
                home: Home {
                    in_progress: vec![],
                    recent: vec![],
                    teams: vec![],
                },
                selected: 0,
            },
            error,
            first_frame: OnceLock::new(),
        };
        app.go_home();
        (app, Task::none())
    }

    fn go_home(&mut self) {
        match self.core.home() {
            Ok(home) => self.screen = Screen::Home { home, selected: 0 },
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn colors(&self) -> theme::Colors {
        theme::colors(self.dark)
    }

    fn open(&mut self, open: home::Open) {
        match self
            .core
            .scouting_in_order(&open.team_id, &open.match_id, ChoiceOrder::EventFirst)
        {
            Ok(scouting) => {
                self.error = None;
                self.screen =
                    Screen::Scouting(Box::new(ScoutingScreen::new(scouting, self.keys.clone())))
            }
            Err(e) => self.error = Some(e.to_string()),
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

    fn update(&mut self, message: Message) {
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
        match (message, &mut self.screen) {
            (Message::Settings(_) | Message::OpenSettings, _) => {}
            (Message::Back, Screen::Scouting(_)) => self.go_home(),
            (Message::Back, Screen::Home { .. }) => {}
            (Message::ToggleTheme, _) => self.dark = !self.dark,
            (Message::Open(open), _) => self.open(open),
            (Message::Scouting(m), Screen::Scouting(screen)) => match screen.update(m) {
                Outcome::Stay => {}
                Outcome::Leave => self.go_home(),
            },
            (Message::Scouting(_), Screen::Home { .. }) => {}
            (Message::Key(ck), Screen::Scouting(screen)) => match screen.key(ck) {
                Outcome::Stay => {}
                Outcome::Leave => self.go_home(),
            },
            (Message::Key(ck), Screen::Home { home, selected }) => {
                let count = home.in_progress.len();
                match ck {
                    k if k == key!(down) => {
                        *selected = (*selected + 1).min(count.saturating_sub(1))
                    }
                    k if k == key!(up) => *selected = selected.saturating_sub(1),
                    k if k == key!(enter) => {
                        if let Some(open) =
                            home.in_progress.get(*selected).and_then(home::resumable)
                        {
                            self.open(open)
                        }
                    }
                    k if k == BACK_KEY => self.error = None,
                    _ => {}
                }
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        if let (Ok(()), Some(started)) = (self.first_frame.set(()), STARTED.get()) {
            eprintln!("first frame after {} ms", started.elapsed().as_millis());
        }
        let c = self.colors();
        let s = self.strings;
        let (screen, hints, back): (Element<'_, Message>, _, _) =
            match (&self.settings, &self.screen) {
                (Some(settings), _) => (
                    settings.view(s, c).map(Message::Settings),
                    settings.hints(s),
                    true,
                ),
                (None, Screen::Home { home, selected }) => (
                    home::view(home, *selected, s, Message::Open, c),
                    home::hints(s),
                    false,
                ),
                (None, Screen::Scouting(screen)) => (
                    screen.view(s, c).map(Message::Scouting),
                    screen.hints(s),
                    true,
                ),
            };
        let top = ui::top_bar(
            back.then_some(Message::Back),
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

    fn theme(&self) -> Theme {
        theme::theme(self.dark)
    }

    fn subscription(&self) -> Subscription<Message> {
        event::listen_with(|event, status, _| match (event, status) {
            (
                Event::Keyboard(keyboard::Event::KeyPressed {
                    modified_key,
                    modifiers,
                    ..
                }),
                event::Status::Ignored,
            ) => keymap::combination(&modified_key, modifiers).map(Message::Key),
            _ => None,
        })
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
    iced::application(App::boot, App::update, App::view)
        .title("scout4all")
        .font(fonts.bytes[0])
        .font(fonts.bytes[1])
        .default_font(fonts.regular)
        .theme(App::theme)
        .subscription(App::subscription)
        .window(window::Settings {
            size: Size::new(1400.0, 860.0),
            min_size: Some(Size::new(1000.0, 640.0)),
            ..window::Settings::default()
        })
        .run()
}
