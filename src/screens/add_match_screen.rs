use crate::shapes::keybinding::navigation_keybindings;
use std::sync::Arc;

use crate::{
    constants::DEFAULT_MAX_SUBSTITUTIONS,
    localization::current_labels,
    providers::{
        match_writer::MatchWriter, set_writer::SetWriter, settings_reader::SettingsReader,
        settings_writer::SettingsWriter,
    },
    screens::{
        components::{
            checkbox::CheckBox, date_picker::DatePicker, navigation_footer::NavigationFooter,
            notify_banner::NotifyBanner, team_header::TeamHeader, text_box::TextBox,
        },
        screen::{AppAction, Renderable, Sba, ScreenAsync, get_keybinding_actions},
        start_set_screen::StartSetScreen,
    },
    shapes::{
        enums::ScreenActionEnum, keybinding::ActionsKeyBindings, settings::Settings,
        team::TeamEntry,
    },
};
use async_trait::async_trait;
use crokey::crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
};

#[derive(Debug)]
pub struct AddMatchScreen<
    MW: MatchWriter + Send + Sync,
    SSW: SetWriter + Send + Sync,
    SR: SettingsReader + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
> {
    settings_reader: Arc<SR>,
    settings_writer: Arc<SW>,
    match_writer: Arc<MW>,
    set_writer: Arc<SSW>,
    settings: Settings,
    team: TeamEntry,
    opponent: TextBox,          // field 0
    date: DatePicker,           // field 1
    home: CheckBox,             // field 2
    max_substitutions: TextBox, // field 3
    field: usize,
    notify_message: NotifyBanner,
    header: TeamHeader,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
}

impl<
    MW: MatchWriter + Send + Sync + 'static,
    SSW: SetWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
> Renderable for AddMatchScreen<MW, SSW, SR, SW>
{
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let container = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(1)])
            .split(body);

        let area = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // opponent
                Constraint::Length(3), // date
                Constraint::Length(3), // home
                Constraint::Length(3), // max substitutions
                Constraint::Min(1),
            ])
            .split(container[1]);
        self.notify_message.render(f, footer_right);
        self.render_header(f, container[1]);
        self.opponent.render(f, area[0]);
        self.date.render(f, area[1]);
        self.home.render(f, area[2]);
        self.max_substitutions.render(f, area[3]);
        self.header.render(f, container[0], Some(&self.team));
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

#[async_trait]
impl<
    MW: MatchWriter + Send + Sync + 'static,
    SSW: SetWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
> ScreenAsync for AddMatchScreen<MW, SSW, SR, SW>
{
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if let Some(key_combination) = self.screen_key_bindings.transform(key) {
            match (
                self.screen_key_bindings.get(key_combination),
                key.code,
                &self.notify_message.has_value(),
            ) {
                (_, _, true) => {
                    self.notify_message.reset();
                    AppAction::None
                }
                (None, KeyCode::Char(c), _) => self.handle_char(c),
                (None, KeyCode::Backspace, _) => self.handle_backspace(),
                (Some(ScreenActionEnum::Next), _, _) => self.handle_next(),
                (Some(ScreenActionEnum::Previous), _, _) => self.handle_previous(),
                (Some(ScreenActionEnum::Back), _, _) => AppAction::Back(true, Some(1)),
                (Some(ScreenActionEnum::Confirm), _, _) => self.handle_confirm().await,
                _ => AppAction::None,
            }
        } else {
            AppAction::None
        }
    }

    async fn refresh_data(&mut self) {}
}

impl<
    MW: MatchWriter + Send + Sync + 'static,
    SSW: SetWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
> AddMatchScreen<MW, SSW, SR, SW>
{
    pub fn new(
        settings_reader: Arc<SR>,
        settings_writer: Arc<SW>,
        settings: Settings,
        team: TeamEntry,
        match_writer: Arc<MW>,
        set_writer: Arc<SSW>,
    ) -> Self {
        let opponent = TextBox::new(current_labels().opponent.to_owned(), true, None);
        let home = CheckBox::new(current_labels().home.to_owned(), false, false);
        let date = DatePicker::new(current_labels().date.to_owned(), false);
        let max_substitutions = TextBox::with_validator(
            current_labels().max_substitutions.to_owned(),
            false,
            Some(&DEFAULT_MAX_SUBSTITUTIONS.to_string()),
            |current: &str, c: char| current.len() < 2 && c.is_ascii_digit(),
        );

        let screen_actions = &[
            Sba::Simple(ScreenActionEnum::Next),
            Sba::Simple(ScreenActionEnum::Previous),
            Sba::Simple(ScreenActionEnum::Confirm),
            Sba::Simple(ScreenActionEnum::Back),
        ];
        let kb = navigation_keybindings();
        let footer_entries = get_keybinding_actions(kb, screen_actions);
        let screen_key_bindings = kb.slice(Sba::keys(screen_actions));
        AddMatchScreen {
            settings_reader,
            settings_writer,
            settings,
            team,
            opponent,
            date,
            home,
            max_substitutions,
            field: 0,
            notify_message: NotifyBanner::new(),
            header: TeamHeader::default(),
            footer: NavigationFooter::new(),
            footer_entries,
            match_writer,
            set_writer,
            screen_key_bindings,
        }
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(current_labels().new_match);
        f.render_widget(block, area);
    }

    async fn handle_confirm(&mut self) -> AppAction {
        let max_substitutions = self
            .max_substitutions
            .get_selected_value()
            .and_then(|v| v.parse::<u8>().ok())
            .filter(|v| *v > 0);
        match (
            self.date.get_selected_value(),
            self.opponent
                .get_selected_value()
                .filter(|o| !o.trim().is_empty()),
            max_substitutions,
        ) {
            (_, None, _) => {
                self.notify_message
                    .set_error(current_labels().opponent_cannot_be_empty.to_string());
                AppAction::None
            }
            (_, _, None) => {
                self.notify_message
                    .set_error(current_labels().invalid_max_substitutions.to_string());
                AppAction::None
            }
            (Ok(date), Some(opponent), Some(max_substitutions)) => {
                match self
                    .match_writer
                    .create(
                        &self.team,
                        opponent,
                        date,
                        self.home.get_selected_value(),
                        max_substitutions,
                    )
                    .await
                {
                    Ok(m) => AppAction::SwitchScreen(Box::new(StartSetScreen::new(
                        self.settings_reader.clone(),
                        self.settings_writer.clone(),
                        self.settings.clone(),
                        m,
                        1,
                        None,
                        Some(2),
                        self.set_writer.clone(),
                    ))),
                    Err(_) => {
                        self.notify_message
                            .set_error(current_labels().could_not_create_match.to_string());
                        AppAction::None
                    }
                }
            }
            _ => {
                self.notify_message
                    .set_error(current_labels().invalid_date.to_string());
                AppAction::None
            }
        }
    }

    fn handle_backspace(&mut self) -> AppAction {
        self.opponent.handle_backspace();
        self.date.handle_backspace();
        self.max_substitutions.handle_backspace();
        AppAction::None
    }

    fn handle_char(&mut self, c: char) -> AppAction {
        self.opponent.handle_char(c);
        self.home.handle_char(c);
        self.date.handle_char(c);
        self.max_substitutions.handle_char(c);
        AppAction::None
    }

    fn handle_next(&mut self) -> AppAction {
        self.date.handle_tab();
        self.field = (self.field + 1) % 4;
        self.update_writing_modes();
        AppAction::None
    }

    fn handle_previous(&mut self) -> AppAction {
        self.date.handle_tab();
        self.field = (self.field + 3) % 4;
        self.update_writing_modes();
        AppAction::None
    }

    fn update_writing_modes(&mut self) {
        self.opponent.writing_mode = self.field == 0;
        self.date.writing_mode = self.field == 1;
        self.home.writing_mode = self.field == 2;
        self.max_substitutions.writing_mode = self.field == 3;
    }
}
