use crate::shapes::keybinding::navigation_keybindings;
use std::{fmt::Debug, sync::Arc};

use crate::{
    localization::current_labels,
    providers::{settings_reader::SettingsReader, settings_writer::SettingsWriter},
    screens::{
        components::{navigation_footer::NavigationFooter, notify_dialogue::NotifyDialogue},
        keybindings_action_screen::KeyBindingScreen,
        report_an_issue_screen::ReportAnIssueScreen,
        screen::{AppAction, Renderable, Sba, ScreenAsync, get_keybinding_actions},
    },
    shapes::{
        enums::{EvalEnum, EventTypeEnum, FriendlyName, LanguageEnum, ScreenActionEnum, WithDesc},
        keybinding::{ActionsKeyBindings, BACK_KEY, KeyBindings, SETTINGS_KEY, UNDO_KEY},
        settings::{ScoutingShortcut, Settings, set_settings},
        symbol::KeyCombinationFormatExt,
    },
};
use async_trait::async_trait;
use crokey::{
    KeyCombination, KeyCombinationFormat,
    crossterm::event::{KeyCode, KeyEvent},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Row, Table, TableState},
};

/// below this width the two panels are stacked instead of side by side
const SIDE_BY_SIDE_MIN_WIDTH: u16 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPanel {
    /// language and analytics
    General,
    /// the scouting key bindings
    Shortcuts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeneralField {
    Language,
    Analytics,
}

const GENERAL_FIELDS: [GeneralField; 2] = [GeneralField::Language, GeneralField::Analytics];

/// An editable scouting shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shortcut {
    Event(EventTypeEnum),
    Eval(EvalEnum),
}

fn shortcuts() -> Vec<Shortcut> {
    EventTypeEnum::ALL
        .into_iter()
        .map(Shortcut::Event)
        .chain(EvalEnum::ALL.into_iter().map(Shortcut::Eval))
        .collect()
}

/// The settings: general ones on a panel, the scouting key bindings on
/// the other one. Every change is saved right away.
#[derive(Debug)]
pub struct SettingsScreen<SW: SettingsWriter + Send + Sync, SR: SettingsReader + Send + Sync> {
    settings: Settings,
    settings_writer: Arc<SW>,
    settings_reader: Arc<SR>,
    panel: SettingsPanel,
    general_field: usize,
    shortcut: usize,
    shortcuts_state: TableState,
    /// holds the settings to save once the reset to defaults is confirmed
    notifier: NotifyDialogue<Settings>,
    footer: NavigationFooter,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
    format: KeyCombinationFormat,
}

impl<SW, SR> Renderable for SettingsScreen<SW, SR>
where
    SW: SettingsWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
{
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let panels = if body.width >= SIDE_BY_SIDE_MIN_WIDTH {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(body)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(GENERAL_FIELDS.len() as u16 + 2),
                    Constraint::Min(1),
                ])
                .split(body)
        };
        self.render_general(f, panels[0]);
        self.render_shortcuts(f, panels[1]);
        self.notifier.render(f, footer_right);
        self.footer.render(f, footer_left, self.footer_entries());
    }
}

#[async_trait]
impl<SW, SR> ScreenAsync for SettingsScreen<SW, SR>
where
    SW: SettingsWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
{
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        let Some(key_combination) = self.screen_key_bindings.transform(key) else {
            return AppAction::None;
        };
        // the reset confirmation comes first: "y" resets, any other key cancels
        if self.notifier.has_value() {
            let defaults = self.notifier.entry.take();
            self.notifier.reset();
            return match (key.code, defaults) {
                (KeyCode::Char(c), Some(defaults)) if c == *current_labels().y => {
                    self.save(defaults).await
                }
                _ => AppAction::None,
            };
        }
        if self.notifier.banner.has_value() {
            self.notifier.banner.reset();
            return AppAction::None;
        }
        match (key.code, self.panel) {
            (KeyCode::Tab | KeyCode::BackTab, _) => {
                self.switch_panel();
                return AppAction::None;
            }
            (KeyCode::Left | KeyCode::Right | KeyCode::Char(' '), SettingsPanel::General) => {
                return self.change_general_field(key.code == KeyCode::Left).await;
            }
            _ => {}
        }
        match (self.screen_key_bindings.get(key_combination), self.panel) {
            (Some(ScreenActionEnum::Up), _) => self.move_selection(false),
            (Some(ScreenActionEnum::Down), _) => self.move_selection(true),
            (Some(ScreenActionEnum::Confirm), SettingsPanel::General) => {
                self.change_general_field(false).await
            }
            (
                Some(ScreenActionEnum::Confirm | ScreenActionEnum::Edit),
                SettingsPanel::Shortcuts,
            ) => self.edit_shortcut(),
            (Some(ScreenActionEnum::Reset), SettingsPanel::Shortcuts) => {
                let defaults = Settings {
                    scouting_keybindings: KeyBindings::default(),
                    eval_keybindings: KeyBindings::default(),
                    ..self.settings.clone()
                };
                self.notifier
                    .set(defaults)
                    .banner
                    .set_warning(current_labels().reset_to_defaults_confirmation.to_string());
                AppAction::None
            }
            (Some(ScreenActionEnum::ReportAnIssue), _) => {
                AppAction::SwitchScreen(Box::new(ReportAnIssueScreen::new()))
            }
            (Some(ScreenActionEnum::Back), _) => AppAction::Back(true, Some(1)),
            (Some(ScreenActionEnum::Quit), _) => AppAction::Quit(Ok(())),
            _ => AppAction::None,
        }
    }

    async fn refresh_data(&mut self) {
        if let Ok(settings) = self.settings_reader.read().await {
            self.settings = settings;
        }
    }
}

impl<SW, SR> SettingsScreen<SW, SR>
where
    SW: SettingsWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
{
    pub fn new(
        settings: Settings,
        settings_writer: Arc<SW>,
        settings_reader: Arc<SR>,
        panel: SettingsPanel,
    ) -> Self {
        let screen_key_bindings = navigation_keybindings().slice(vec![
            &ScreenActionEnum::Up,
            &ScreenActionEnum::Down,
            &ScreenActionEnum::Confirm,
            &ScreenActionEnum::Edit,
            &ScreenActionEnum::Reset,
            &ScreenActionEnum::ReportAnIssue,
            &ScreenActionEnum::Back,
            &ScreenActionEnum::Quit,
        ]);
        SettingsScreen {
            settings,
            settings_writer,
            settings_reader,
            panel,
            general_field: 0,
            shortcut: 0,
            shortcuts_state: TableState::default(),
            notifier: NotifyDialogue::new(),
            footer: NavigationFooter::new(),
            screen_key_bindings,
            format: KeyCombinationFormat::default(),
        }
    }

    fn footer_entries(&self) -> Vec<(String, String)> {
        let actions = match self.panel {
            SettingsPanel::General => vec![
                Sba::Simple(ScreenActionEnum::Up),
                Sba::Simple(ScreenActionEnum::Down),
                Sba::Redacted(ScreenActionEnum::Confirm, |_| {
                    current_labels().change.to_string()
                }),
            ],
            SettingsPanel::Shortcuts => vec![
                Sba::Simple(ScreenActionEnum::Up),
                Sba::Simple(ScreenActionEnum::Down),
                Sba::Simple(ScreenActionEnum::Edit),
                Sba::Simple(ScreenActionEnum::Reset),
            ],
        };
        let mut entries = get_keybinding_actions(navigation_keybindings(), &actions);
        entries.push((
            KeyCombinationFormatExt::new(&self.format).to_string(KeyCode::Tab),
            current_labels().switch_panel.to_string(),
        ));
        entries.extend(get_keybinding_actions(
            navigation_keybindings(),
            &[
                Sba::Simple(ScreenActionEnum::ReportAnIssue),
                Sba::Simple(ScreenActionEnum::Back),
                Sba::Simple(ScreenActionEnum::Quit),
            ],
        ));
        entries
    }

    fn switch_panel(&mut self) {
        self.panel = match self.panel {
            SettingsPanel::General => SettingsPanel::Shortcuts,
            SettingsPanel::Shortcuts => SettingsPanel::General,
        };
    }

    fn move_selection(&mut self, down: bool) -> AppAction {
        let (selected, len) = match self.panel {
            SettingsPanel::General => (&mut self.general_field, GENERAL_FIELDS.len()),
            SettingsPanel::Shortcuts => (&mut self.shortcut, shortcuts().len()),
        };
        *selected = if down {
            (*selected + 1).min(len - 1)
        } else {
            selected.saturating_sub(1)
        };
        AppAction::None
    }

    /// changes the selected general setting (the next or previous language,
    /// the analytics toggled) and saves it
    async fn change_general_field(&mut self, backwards: bool) -> AppAction {
        let settings = match GENERAL_FIELDS[self.general_field] {
            GeneralField::Language => {
                let languages = LanguageEnum::ALL;
                let current = languages
                    .iter()
                    .position(|l| *l == self.settings.language)
                    .unwrap_or_default();
                let step = if backwards { languages.len() - 1 } else { 1 };
                Settings {
                    language: languages[(current + step) % languages.len()],
                    ..self.settings.clone()
                }
            }
            GeneralField::Analytics => Settings {
                analytics_enabled: !self.settings.analytics_enabled,
                ..self.settings.clone()
            },
        };
        self.save(settings).await
    }

    async fn save(&mut self, settings: Settings) -> AppAction {
        match self.settings_writer.save(settings).await {
            Ok(saved) => {
                set_settings(saved.clone());
                self.settings = saved;
            }
            Err(_) => self
                .notifier
                .banner
                .set_error(current_labels().could_not_save_settings.to_string()),
        }
        AppAction::None
    }

    fn edit_shortcut(&self) -> AppAction {
        fn screen<SW, SR, T>(s: &SettingsScreen<SW, SR>, action: T) -> AppAction
        where
            SW: SettingsWriter + Send + Sync + 'static,
            SR: SettingsReader + Send + Sync + 'static,
            T: ScoutingShortcut + Debug,
        {
            AppAction::SwitchScreen(Box::new(KeyBindingScreen::new(
                s.settings.clone(),
                action,
                s.settings_writer.clone(),
                s.settings_reader.clone(),
            )))
        }
        match shortcuts()[self.shortcut] {
            Shortcut::Event(event) => screen(self, event),
            Shortcut::Eval(eval) => screen(self, eval),
        }
    }

    fn panel_block(&self, panel: SettingsPanel, title: &str) -> Block<'static> {
        let style = if self.panel == panel {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        Block::default()
            .borders(Borders::ALL)
            .border_style(style)
            .title(Line::styled(title.to_string(), style))
    }

    fn selected_style() -> Style {
        Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD)
    }

    fn render_general(&mut self, f: &mut Frame, area: Rect) {
        let labels = current_labels();
        let rows = GENERAL_FIELDS.iter().enumerate().map(|(i, field)| {
            let text = match field {
                GeneralField::Language => format!(
                    "{}: ◀ {} ▶",
                    labels.language,
                    self.settings.language.friendly_name(labels)
                ),
                GeneralField::Analytics => format!(
                    "{} {}",
                    if self.settings.analytics_enabled {
                        "[X]"
                    } else {
                        "[ ]"
                    },
                    labels.enable_send_analytics
                ),
            };
            let row = Row::new(vec![text]);
            if self.panel == SettingsPanel::General && i == self.general_field {
                row.style(Self::selected_style())
            } else {
                row
            }
        });
        let table = Table::new(rows, [Constraint::Percentage(100)])
            .block(self.panel_block(SettingsPanel::General, labels.general_settings));
        f.render_widget(table, area);
    }

    fn keys_of<T: ScoutingShortcut>(&self, action: &T) -> String {
        let mut keys: Vec<KeyCombination> = T::bindings(&self.settings)
            .keybindings_for(action)
            .into_iter()
            .collect();
        keys.sort();
        if keys.is_empty() {
            return current_labels().unassigned.to_string();
        }
        keys.iter()
            .map(|k| KeyCombinationFormatExt::new(&self.format).to_string(*k))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn render_shortcuts(&mut self, f: &mut Frame, area: Rect) {
        let labels = current_labels();
        let fmt = KeyCombinationFormatExt::new(&self.format);
        let header = |title: &str| {
            Row::new(vec![title.to_string()])
                .style(Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED))
        };
        let mut rows = vec![header(labels.events)];
        let mut selected_row = None;
        for (i, shortcut) in shortcuts().into_iter().enumerate() {
            if shortcut == Shortcut::Eval(EvalEnum::ALL[0]) {
                rows.push(header(labels.evaluations));
            }
            if i == self.shortcut {
                selected_row = Some(rows.len());
            }
            rows.push(match shortcut {
                Shortcut::Event(event) => Row::new(vec![
                    format!("  {}", event.with_desc().1),
                    self.keys_of(&event),
                ]),
                Shortcut::Eval(eval) => Row::new(vec![
                    format!("  {}", eval.with_desc().1),
                    self.keys_of(&eval),
                ]),
            });
        }
        rows.push(header(labels.fixed_shortcuts));
        let fixed = [
            (labels.settings, fmt.to_string(SETTINGS_KEY)),
            (labels.undo, fmt.to_string(UNDO_KEY)),
            (labels.back, fmt.to_string(BACK_KEY)),
            (labels.player_number, "0-9".to_string()),
        ];
        rows.extend(fixed.into_iter().map(|(action, key)| {
            Row::new(vec![
                format!("  {}", action),
                format!("{} ({})", key, labels.fixed),
            ])
            .style(Style::default().fg(Color::DarkGray))
        }));
        self.shortcuts_state.select(match self.panel {
            SettingsPanel::Shortcuts => selected_row,
            SettingsPanel::General => None,
        });
        let table = Table::new(
            rows,
            [Constraint::Percentage(60), Constraint::Percentage(40)],
        )
        .block(self.panel_block(SettingsPanel::Shortcuts, labels.key_bindings))
        .row_highlight_style(Self::selected_style());
        f.render_stateful_widget(table, area, &mut self.shortcuts_state);
    }
}
