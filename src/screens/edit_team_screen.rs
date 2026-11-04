use crate::shapes::keybinding::navigation_keybindings;
use std::sync::Arc;

use crate::localization::current_labels;
use crate::{
    providers::team_pins::TeamPins,
    screens::{
        components::{
            country_picker::CountryPicker, navigation_footer::NavigationFooter,
            notify_banner::NotifyBanner, panel, select::Select, text_box::TextBox,
        },
        screen::{
            AppAction, FieldKey, Renderable, Sba, ScreenAsync, change_value_entry,
            get_keybinding_actions, navigation_entries,
        },
    },
    shapes::{actions::ScreenActionEnum, keybinding::ActionsKeyBindings},
};
use async_trait::async_trait;
use crokey::crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::Paragraph,
};
use scout4all_core::{
    logging::logger::log_error,
    providers::team_writer::TeamWriter,
    shapes::{
        country::CountryCode,
        enums::{FriendlyName, GenderEnum, TeamClassificationEnum},
        team::TeamEntry,
    },
    team_form::TeamForm,
};

/// name, classification, gender, country, year
const FIELDS: usize = 5;

#[derive(Debug)]
pub struct EditTeamScreen<TW: TeamWriter + Send + Sync> {
    name: TextBox,
    gender: Select<GenderEnum>,
    classification: Select<TeamClassificationEnum>,
    country: CountryPicker,
    year: TextBox,
    field: usize,
    notify_message: NotifyBanner,
    existing_team: Option<TeamEntry>,
    back: bool,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
    team_writer: Arc<TW>,
    /// where a new team is pinned, once saved: the teams created by hand
    /// start at the top of the list
    pins: Option<Arc<dyn TeamPins>>,
}

impl<TW: TeamWriter + Send + Sync> Renderable for EditTeamScreen<TW> {
    fn footer_entries(&self) -> Vec<(String, String)> {
        self.footer_entries.clone()
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        // a line for each field, three for the one being changed (in a
        // frame) and for the description of the classification chosen: all
        // of them fit a short terminal
        let rows = |shown: bool| Constraint::Length(if shown { 3 } else { 1 });
        let info = Constraint::Length(if self.classification.writing_mode {
            3
        } else {
            0
        });
        let area = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(1),                  // 0: name
                rows(self.classification.writing_mode), // 1: classification
                info,                                   // 2: classification info
                rows(self.gender.writing_mode),         // 3: gender
                rows(self.country.writing_mode),        // 4: country
                Constraint::Length(1),                  // 5: year
                Constraint::Min(0),
            ])
            .split(body);
        self.notify_message.render(f, footer_right);
        self.render_header(f, body);
        self.name.render(f, area[0]);
        self.classification.render(f, area[1]);
        self.render_classification_description(f, area[2]);
        self.gender.render(f, area[3]);
        self.country.render(f, area[4]);
        self.year.render(f, area[5]);
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

#[async_trait]
impl<TW: TeamWriter + Send + Sync> ScreenAsync for EditTeamScreen<TW> {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if let Some(key_combination) = self.screen_key_bindings.transform(key) {
            match (
                self.screen_key_bindings.get(key_combination),
                key.code,
                FieldKey::of(key.code),
                &self.notify_message.has_value(),
            ) {
                (_, _, _, true) => self.handle_error_reset(),
                (_, _, Some(FieldKey::Next), _) => self.handle_tab(),
                (_, _, Some(FieldKey::Previous), _) => self.handle_backtab(),
                (_, _, Some(FieldKey::Change { back: true }), _) => self.handle_up(),
                (_, _, Some(FieldKey::Change { back: false }), _) => self.handle_down(),
                (_, KeyCode::Char(c), _, _) => self.handle_char(c),
                (_, KeyCode::Backspace, _, _) => self.handle_backspace(),
                (Some(ScreenActionEnum::Back), _, _, _) => AppAction::Back(true, Some(1)),
                (Some(ScreenActionEnum::Confirm), _, _, _) => self.handle_enter().await,
                _ => AppAction::None,
            }
        } else {
            AppAction::None
        }
    }

    async fn refresh_data(&mut self) {}
}

impl<TW: TeamWriter + Send + Sync> EditTeamScreen<TW> {
    /// A new team, of `country` (the one of the settings) unless changed,
    /// pinned in `pins` once saved.
    pub fn new(
        team_writer: Arc<TW>,
        country: Option<CountryCode>,
        pins: Arc<dyn TeamPins>,
    ) -> Self {
        let classification = Select::new(
            current_labels().team_classification.to_owned(),
            TeamClassificationEnum::ALL.to_vec(),
            None,
            false,
        );
        let gender = Select::new(
            current_labels().gender.to_owned(),
            GenderEnum::ALL.to_vec(),
            None,
            false,
        );
        let name = TextBox::new(current_labels().name.to_owned(), true, None);
        let country = CountryPicker::new(current_labels().country.to_owned(), country, false);
        let year = TextBox::with_validator(
            current_labels().year.to_owned(),
            false,
            None,
            |current: &str, c: char| current.len() < 4 && c.is_ascii_digit(),
        );

        let actions = &[
            Sba::Simple(ScreenActionEnum::Confirm),
            Sba::Simple(ScreenActionEnum::Back),
        ];

        let kb = navigation_keybindings();
        let footer_entries = navigation_entries()
            .into_iter()
            .chain([change_value_entry()])
            .chain(get_keybinding_actions(kb, actions))
            .collect();
        let screen_key_bindings = kb.slice(Sba::keys(actions));

        EditTeamScreen {
            name,
            gender,
            classification,
            country,
            year,
            field: 0,
            notify_message: NotifyBanner::new(),
            existing_team: None,
            back: false,
            footer: NavigationFooter::new(),
            footer_entries,
            team_writer,
            screen_key_bindings,

            pins: Some(pins),
        }
    }

    /// The team, of `country` (the one of the settings) when it has none,
    /// as the teams saved before v0.12.
    pub fn edit(team: &TeamEntry, team_writer: Arc<TW>, country: Option<CountryCode>) -> Self {
        let classification = Select::new(
            current_labels().team_classification.to_owned(),
            TeamClassificationEnum::ALL.to_vec(),
            team.classification,
            false,
        );
        let gender = Select::new(
            current_labels().gender.to_owned(),
            GenderEnum::ALL.to_vec(),
            team.gender,
            false,
        );
        let name = TextBox::new(current_labels().name.to_owned(), true, Some(&team.name));
        let country = CountryPicker::new(
            current_labels().country.to_owned(),
            team.country.or(country),
            false,
        );
        let year = TextBox::with_validator(
            current_labels().year.to_owned(),
            false,
            Some(&team.year.to_string()),
            |current: &str, c: char| current.len() < 4 && c.is_ascii_digit(),
        );

        let actions = &[
            Sba::Simple(ScreenActionEnum::Confirm),
            Sba::Simple(ScreenActionEnum::Back),
        ];

        let kb = navigation_keybindings();
        let footer_entries = navigation_entries()
            .into_iter()
            .chain([change_value_entry()])
            .chain(get_keybinding_actions(kb, actions))
            .collect();
        let screen_key_bindings = kb.slice(Sba::keys(actions));

        EditTeamScreen {
            name,
            gender,
            classification,
            country,
            year,
            field: 0,
            notify_message: NotifyBanner::new(),
            existing_team: Some(team.clone()),
            back: false,
            footer: NavigationFooter::new(),
            footer_entries,
            team_writer,
            screen_key_bindings,

            pins: None,
        }
    }

    fn handle_error_reset(&mut self) -> AppAction {
        self.notify_message.reset();
        if self.back {
            AppAction::Back(true, Some(1))
        } else {
            AppAction::None
        }
    }

    fn handle_tab(&mut self) -> AppAction {
        self.field = (self.field + 1) % FIELDS;
        self.update_writing_modes();
        AppAction::None
    }

    fn handle_backtab(&mut self) -> AppAction {
        self.field = (self.field + FIELDS - 1) % FIELDS;
        self.update_writing_modes();
        AppAction::None
    }

    fn update_writing_modes(&mut self) {
        self.name.writing_mode = self.field == 0;
        self.classification.writing_mode = self.field == 1;
        self.gender.writing_mode = self.field == 2;
        self.country.writing_mode = self.field == 3;
        self.year.writing_mode = self.field == 4;
    }

    fn handle_up(&mut self) -> AppAction {
        self.gender.handle_up();
        self.classification.handle_up();
        self.country.handle_up();
        AppAction::None
    }

    fn handle_down(&mut self) -> AppAction {
        self.gender.handle_down();
        self.classification.handle_down();
        self.country.handle_down();
        AppAction::None
    }

    fn handle_backspace(&mut self) -> AppAction {
        self.name.handle_backspace();
        self.country.handle_backspace();
        self.year.handle_backspace();
        AppAction::None
    }

    async fn handle_enter(&mut self) -> AppAction {
        let form = TeamForm {
            name: self.name.get_selected_value().unwrap_or_default(),
            classification: self.classification.get_selected_value(),
            gender: self.gender.get_selected_value(),
            country: self.country.get_selected_value(),
            year: self.year.get_selected_value().unwrap_or_default(),
        };
        match form.validate(self.existing_team.as_ref()) {
            Err(e) => {
                self.notify_message
                    .set_error(e.message(current_labels().domain));
                AppAction::None
            }
            Ok(input) => match self.team_writer.save(input).await {
                Ok(team) => {
                    if let Some(pins) = &self.pins
                        && let Err(e) = pins.pin(team.id).await
                    {
                        // the team is there all the same, just not pinned
                        log_error(&format!("could not pin the new team: {e}"));
                    }
                    self.notify_message
                        .set_info(current_labels().operation_successful.to_string());
                    self.back = true;
                    AppAction::None
                }
                Err(_) => {
                    self.notify_message
                        .set_error(current_labels().could_not_create_team.to_string());
                    AppAction::None
                }
            },
        }
    }

    fn handle_char(&mut self, c: char) -> AppAction {
        self.name.handle_char(c);
        self.country.handle_char(c);
        self.year.handle_char(c);
        AppAction::None
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let block = panel::screen(match self.existing_team {
            Some(_) => current_labels().edit_team,
            None => current_labels().new_team,
        });
        f.render_widget(block, area);
    }

    fn render_classification_description(&self, f: &mut Frame, area: Rect) {
        if let (Some(classification), true) = (
            self.classification.get_selected_value(),
            self.classification.writing_mode,
        ) {
            let paragraph =
                Paragraph::new(classification.friendly_description(current_labels().domain))
                    .style(Style::default().fg(Color::Cyan))
                    .block(panel::section(
                        classification.friendly_name(current_labels().domain),
                    ))
                    .alignment(Alignment::Left);
            f.render_widget(paragraph, area);
        }
    }
}
