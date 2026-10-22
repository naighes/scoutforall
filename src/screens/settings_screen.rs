use crate::shapes::keybinding::navigation_keybindings;
use std::{fmt::Debug, sync::Arc};

use crate::localization::current_labels;
use crate::{
    providers::{settings_reader::SettingsReader, settings_writer::SettingsWriter},
    screens::{
        components::{navigation_footer::NavigationFooter, notify_dialogue::NotifyDialogue, panel},
        keybindings_change_screen::ChangeKeyBindingScreen,
        report_an_issue_screen::ReportAnIssueScreen,
        screen::{AppAction, Renderable, Sba, ScreenAsync, get_keybinding_actions},
    },
    shapes::{
        actions::ScreenActionEnum,
        keybinding::{ActionsKeyBindings, BACK_KEY, KeyBindings, SETTINGS_KEY, UNDO_KEY},
        settings::{ScoutingShortcut, Settings, set_settings},
        symbol::KeyCombinationFormatExt,
    },
};
use async_trait::async_trait;
use crokey::{
    KeyCombinationFormat,
    crossterm::event::{KeyCode, KeyEvent},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Row, Table, TableState},
};
use scout4all_core::shapes::{
    enums::{EvalEnum, EventTypeEnum, FriendlyName, LanguageEnum},
    technique::{ServeTypeEnum, SetTypeEnum},
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
    SetType(SetTypeEnum),
    ServeType(ServeTypeEnum),
}

fn shortcuts() -> Vec<Shortcut> {
    EventTypeEnum::ALL
        .into_iter()
        .map(Shortcut::Event)
        .chain(EvalEnum::ALL.into_iter().map(Shortcut::Eval))
        .chain(SetTypeEnum::ALL.into_iter().map(Shortcut::SetType))
        .chain(ServeTypeEnum::ALL.into_iter().map(Shortcut::ServeType))
        .collect()
}

/// The title of the group the shortcut opens, if it's the first of it.
fn group_title(shortcut: Shortcut) -> Option<&'static str> {
    let labels = current_labels();
    match shortcut {
        Shortcut::Event(event) if event == EventTypeEnum::ALL[0] => Some(labels.events),
        Shortcut::Eval(eval) if eval == EvalEnum::ALL[0] => Some(labels.evaluations),
        Shortcut::SetType(set) if set == SetTypeEnum::ALL[0] => Some(labels.set_types),
        Shortcut::ServeType(serve) if serve == ServeTypeEnum::ALL[0] => Some(labels.serve_types),
        _ => None,
    }
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
                Sba::Redacted(ScreenActionEnum::Edit, |_| {
                    current_labels().change.to_string()
                }),
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

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        // the fixed keys below the general settings: at the end of the
        // shortcuts they would scroll out of sight
        let general_and_fixed = [
            Constraint::Length(GENERAL_FIELDS.len() as u16 + 2),
            Constraint::Length(self.fixed_keys().len() as u16 + 2),
        ];
        let (general, fixed, shortcuts) = if body.width >= SIDE_BY_SIDE_MIN_WIDTH {
            let panels = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
                .split(body);
            let left = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    general_and_fixed[0],
                    general_and_fixed[1],
                    Constraint::Min(0),
                ])
                .split(panels[0]);
            (left[0], left[1], panels[1])
        } else {
            let panels = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    general_and_fixed[0],
                    general_and_fixed[1],
                    Constraint::Min(1),
                ])
                .split(body);
            (panels[0], panels[1], panels[2])
        };
        self.render_general(f, general);
        self.render_fixed_keys(f, fixed);
        self.render_shortcuts(f, shortcuts);
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
                    set_type_keybindings: KeyBindings::default(),
                    serve_type_keybindings: KeyBindings::default(),
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
            AppAction::SwitchScreen(Box::new(ChangeKeyBindingScreen::new(
                action,
                s.settings.clone(),
                s.settings_writer.clone(),
            )))
        }
        match shortcuts()[self.shortcut] {
            Shortcut::Event(event) => screen(self, event),
            Shortcut::Eval(eval) => screen(self, eval),
            Shortcut::SetType(set) => screen(self, set),
            Shortcut::ServeType(serve) => screen(self, serve),
        }
    }

    fn panel_block(&self, shown: SettingsPanel, title: &str) -> Block<'static> {
        let block = panel::section(title.to_string());
        match self.panel == shown {
            true => block
                .border_style(panel::SCREEN_TITLE)
                .title_style(panel::SCREEN_TITLE),
            false => block,
        }
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
                    self.settings.language.friendly_name(labels.domain)
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
        T::bindings(&self.settings)
            .key_for(action)
            .map(|k| KeyCombinationFormatExt::new(&self.format).to_string(k))
            .unwrap_or_else(|| current_labels().unassigned.to_string())
    }

    fn shortcut_row<T: ScoutingShortcut>(&self, action: T) -> Row<'static> {
        Row::new(vec![
            format!("  {}", action.describe().1),
            self.keys_of(&action),
        ])
    }

    /// The keys scouting keeps for itself, with what they do.
    fn fixed_keys(&self) -> Vec<(&'static str, String)> {
        let labels = current_labels();
        let fmt = KeyCombinationFormatExt::new(&self.format);
        vec![
            (labels.settings, fmt.to_string(SETTINGS_KEY)),
            (labels.undo, fmt.to_string(UNDO_KEY)),
            (labels.back, fmt.to_string(BACK_KEY)),
            (labels.player_number, "0-9".to_string()),
        ]
    }

    fn render_fixed_keys(&self, f: &mut Frame, area: Rect) {
        let labels = current_labels();
        let rows = self.fixed_keys().into_iter().map(|(action, key)| {
            Row::new(vec![
                format!("  {}", action),
                format!("{} ({})", key, labels.fixed),
            ])
            .style(Style::default().fg(Color::DarkGray))
        });
        let table = Table::new(
            rows,
            [Constraint::Percentage(60), Constraint::Percentage(40)],
        )
        .block(panel::section(format!(
            "{} – {}",
            labels.key_bindings, labels.fixed_shortcuts
        )));
        f.render_widget(table, area);
    }

    fn render_shortcuts(&mut self, f: &mut Frame, area: Rect) {
        let labels = current_labels();
        let header = |title: &str| {
            Row::new(vec![title.to_string()])
                .style(Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED))
        };
        let mut rows = vec![];
        let mut selected_row = None;
        for (i, shortcut) in shortcuts().into_iter().enumerate() {
            rows.extend(group_title(shortcut).map(header));
            if i == self.shortcut {
                selected_row = Some(rows.len());
            }
            rows.push(match shortcut {
                Shortcut::Event(event) => self.shortcut_row(event),
                Shortcut::Eval(eval) => self.shortcut_row(eval),
                Shortcut::SetType(set) => self.shortcut_row(set),
                Shortcut::ServeType(serve) => self.shortcut_row(serve),
            });
        }
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
