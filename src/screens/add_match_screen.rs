use crate::shapes::keybinding::navigation_keybindings;
use std::sync::Arc;

use crate::localization::current_labels;
use crate::{
    providers::{settings_reader::SettingsReader, settings_writer::SettingsWriter},
    screens::{
        components::{
            checkbox::CheckBox, date_picker::DatePicker, navigation_footer::NavigationFooter,
            notify_banner::NotifyBanner, panel, team_header::TeamHeader, text_box::TextBox,
        },
        screen::{
            AppAction, FieldKey, Renderable, Sba, ScreenAsync, change_value_entry,
            get_keybinding_actions, navigation_entries,
        },
        start_set_screen::StartSetScreen,
    },
    shapes::{actions::ScreenActionEnum, keybinding::ActionsKeyBindings, settings::Settings},
};
use async_trait::async_trait;
use crokey::crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};
use scout4all_core::{
    constants::DEFAULT_MAX_SUBSTITUTIONS,
    match_form::{MatchForm, MatchInput},
    providers::{match_writer::MatchWriter, set_writer::SetWriter},
    set_start::SetStartForm,
    shapes::{r#match::MatchEntry, team::TeamEntry},
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
    editing: Option<MatchEntry>,
}

impl<
    MW: MatchWriter + Send + Sync + 'static,
    SSW: SetWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
    SW: SettingsWriter + Send + Sync + 'static,
> Renderable for AddMatchScreen<MW, SSW, SR, SW>
{
    fn footer_entries(&self) -> Vec<(String, String)> {
        self.footer_entries.clone()
    }

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
                FieldKey::of(key.code),
                &self.notify_message.has_value(),
            ) {
                (_, _, _, true) => {
                    self.notify_message.reset();
                    AppAction::None
                }
                (_, _, Some(FieldKey::Next), _) => self.handle_next(),
                (_, _, Some(FieldKey::Previous), _) => self.handle_previous(),
                (_, _, Some(FieldKey::Change { .. }), _) => {
                    // the only field with values to choose from
                    self.home.handle_char(' ');
                    AppAction::None
                }
                (None, KeyCode::Char(c), _, _) => self.handle_char(c),
                (None, KeyCode::Backspace, _, _) => self.handle_backspace(),
                (Some(ScreenActionEnum::Back), _, _, _) => AppAction::Back(true, Some(1)),
                (Some(ScreenActionEnum::Confirm), _, _, _) => self.handle_confirm().await,
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
        Self::build(
            settings_reader,
            settings_writer,
            settings,
            team,
            match_writer,
            set_writer,
            opponent,
            date,
            home,
            max_substitutions,
            None,
        )
    }

    pub fn new_edit(
        settings_reader: Arc<SR>,
        settings_writer: Arc<SW>,
        settings: Settings,
        match_writer: Arc<MW>,
        set_writer: Arc<SSW>,
        m: MatchEntry,
    ) -> Self {
        let max_sub_str = m.max_substitutions.to_string();
        let opponent = TextBox::new(
            current_labels().opponent.to_owned(),
            true,
            Some(&m.opponent),
        );
        let home = CheckBox::new(current_labels().home.to_owned(), false, m.home);
        let date = DatePicker::with_date(current_labels().date.to_owned(), false, m.date);
        let max_substitutions = TextBox::with_validator(
            current_labels().max_substitutions.to_owned(),
            false,
            Some(&max_sub_str),
            |current: &str, c: char| current.len() < 2 && c.is_ascii_digit(),
        );
        let team = m.team.clone();
        Self::build(
            settings_reader,
            settings_writer,
            settings,
            team,
            match_writer,
            set_writer,
            opponent,
            date,
            home,
            max_substitutions,
            Some(m),
        )
    }

    fn build(
        settings_reader: Arc<SR>,
        settings_writer: Arc<SW>,
        settings: Settings,
        team: TeamEntry,
        match_writer: Arc<MW>,
        set_writer: Arc<SSW>,
        opponent: TextBox,
        date: DatePicker,
        home: CheckBox,
        max_substitutions: TextBox,
        editing: Option<MatchEntry>,
    ) -> Self {
        let screen_actions = &[
            Sba::Simple(ScreenActionEnum::Confirm),
            Sba::Simple(ScreenActionEnum::Back),
        ];
        let kb = navigation_keybindings();
        let footer_entries = navigation_entries()
            .into_iter()
            .chain([change_value_entry()])
            .chain(get_keybinding_actions(kb, screen_actions))
            .collect();
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
            editing,
        }
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let title = match &self.editing {
            Some(_) => current_labels().edit_match,
            None => current_labels().new_match,
        };
        let block = panel::screen(title);
        f.render_widget(block, area);
    }

    async fn handle_confirm(&mut self) -> AppAction {
        let form = MatchForm {
            opponent: self.opponent.get_selected_value().unwrap_or_default(),
            date: self.date.get_selected_value().ok(),
            home: self.home.get_selected_value(),
            max_substitutions: self
                .max_substitutions
                .get_selected_value()
                .unwrap_or_default(),
        };
        match (form.validate(self.editing.as_ref()), self.editing.clone()) {
            (Err(e), _) => {
                self.notify_message
                    .set_error(e.message(current_labels().domain));
                AppAction::None
            }
            (Ok(input), Some(m)) => self.handle_update(m, input).await,
            (Ok(input), None) => self.handle_create(input).await,
        }
    }

    async fn handle_create(&mut self, input: MatchInput) -> AppAction {
        match self
            .match_writer
            .create(
                &self.team,
                input.opponent,
                input.date,
                input.home,
                input.max_substitutions,
            )
            .await
        {
            Ok(m) => AppAction::SwitchScreen(Box::new(StartSetScreen::new(
                self.settings_reader.clone(),
                self.settings_writer.clone(),
                self.settings.clone(),
                m,
                SetStartForm::new(1, None),
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

    async fn handle_update(&mut self, m: MatchEntry, input: MatchInput) -> AppAction {
        match self
            .match_writer
            .update(
                &m,
                input.opponent,
                input.date,
                input.home,
                input.max_substitutions,
            )
            .await
        {
            Ok(()) => AppAction::Back(true, Some(1)),
            Err(_) => {
                self.notify_message
                    .set_error(current_labels().could_not_update_match.to_string());
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
