use crate::shapes::keybinding::navigation_keybindings;
use std::sync::Arc;

use crate::localization::current_labels;
use crate::{
    screens::{
        components::{
            form_fields::field_areas, navigation_footer::NavigationFooter,
            notify_banner::NotifyBanner, panel, select::Select, team_header::TeamHeader,
            text_box::TextBox,
        },
        screen::{
            AppAction, FieldKey, Renderable, Sba, ScreenAsync, change_value_entry,
            get_keybinding_actions, navigation_entries,
        },
    },
    shapes::{actions::ScreenActionEnum, keybinding::ActionsKeyBindings},
};
use async_trait::async_trait;
use chrono::Local;
use crokey::crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Margin, Rect},
};
use scout4all_core::{
    player_form::PlayerForm,
    providers::team_writer::TeamWriter,
    shapes::{enums::RoleEnum, player::PlayerEntry, team::TeamEntry},
};

fn validate_player_number(current: &str, c: char) -> bool {
    c.is_ascii_digit() && current.len() < 2 && !(current.is_empty() && c == '0')
}

/// YYYY-MM-DD: the core checks the date.
fn validate_birth_date(current: &str, c: char) -> bool {
    (c.is_ascii_digit() || c == '-') && current.len() < 10
}

fn validate_height(current: &str, c: char) -> bool {
    c.is_ascii_digit() && current.len() < 3
}

/// The fields, in the order of Tab.
const FIELDS: usize = 6;

/// What a text box starts with: nothing for an empty text.
fn text(value: &str) -> Option<&str> {
    Some(value).filter(|v| !v.is_empty())
}

pub struct EditPlayerScreen<TW: TeamWriter + Send + Sync> {
    team: TeamEntry,
    name: TextBox,
    number: TextBox,
    role: Select<RoleEnum>,
    birth_date: TextBox,
    height: TextBox,
    notes: TextBox,
    field: usize,
    notify_message: NotifyBanner,
    existing_player: Option<PlayerEntry>,
    back: bool,
    header: TeamHeader,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
    team_writer: Arc<TW>,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
}

impl<TW: TeamWriter + Send + Sync> Renderable for EditPlayerScreen<TW> {
    fn footer_entries(&self) -> Vec<(String, String)> {
        self.footer_entries.clone()
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let container = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(5), Constraint::Min(1)])
            .split(body);
        let area = field_areas(container[1].inner(Margin::new(1, 1)), FIELDS);
        self.notify_message.render(f, footer_right);
        self.render_header(f, container[1]);
        self.name.render(f, area[0]);
        self.role.render(f, area[1]);
        self.number.render(f, area[2]);
        self.birth_date.render(f, area[3]);
        self.height.render(f, area[4]);
        self.notes.render(f, area[5]);
        self.header.render(f, container[0], Some(&self.team));
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

#[async_trait]
impl<TW: TeamWriter + Send + Sync> ScreenAsync for EditPlayerScreen<TW> {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if let Some(key_combination) = self.screen_key_bindings.transform(key) {
            match (
                self.screen_key_bindings.get(key_combination),
                key.code,
                FieldKey::of(key.code),
                &self.notify_message.has_value(),
            ) {
                (_, _, _, true) => self.handle_error_reset(),
                (_, _, Some(FieldKey::Next), _) => self.handle_next(),
                (_, _, Some(FieldKey::Previous), _) => self.handle_previous(),
                (_, _, Some(FieldKey::Change { back: true }), _) => self.handle_up(),
                (_, _, Some(FieldKey::Change { back: false }), _) => self.handle_down(),
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

impl<TW: TeamWriter + Send + Sync> EditPlayerScreen<TW> {
    pub fn new(team: TeamEntry, team_writer: Arc<TW>) -> Self {
        Self::with(team, None, team_writer)
    }

    pub fn edit(team: TeamEntry, player: PlayerEntry, team_writer: Arc<TW>) -> Self {
        Self::with(team, Some(player), team_writer)
    }

    fn with(team: TeamEntry, existing_player: Option<PlayerEntry>, team_writer: Arc<TW>) -> Self {
        let labels = current_labels();
        let form = existing_player
            .as_ref()
            .map(PlayerForm::of)
            .unwrap_or_default();
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

        EditPlayerScreen {
            team,
            name: TextBox::new(labels.name.to_owned(), true, text(&form.name)),
            number: TextBox::with_validator(
                labels.number.to_owned(),
                false,
                text(&form.number),
                validate_player_number,
            ),
            role: Select::new(
                labels.role.to_owned(),
                RoleEnum::ALL.to_vec(),
                form.role,
                false,
            ),
            birth_date: TextBox::with_validator(
                labels.birth_date.to_owned(),
                false,
                text(&form.birth_date),
                validate_birth_date,
            ),
            height: TextBox::with_validator(
                labels.height.to_owned(),
                false,
                text(&form.height_cm),
                validate_height,
            ),
            notes: TextBox::new(labels.notes.to_owned(), false, text(&form.notes)).wrapping(),
            field: 0,
            notify_message: NotifyBanner::new(),
            existing_player,
            back: false,
            header: TeamHeader::default(),
            footer: NavigationFooter::new(),
            footer_entries,
            team_writer,
            screen_key_bindings,
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

    fn text_boxes(&mut self) -> [&mut TextBox; 5] {
        [
            &mut self.name,
            &mut self.number,
            &mut self.birth_date,
            &mut self.height,
            &mut self.notes,
        ]
    }

    fn handle_char(&mut self, c: char) -> AppAction {
        self.text_boxes().into_iter().for_each(|t| t.handle_char(c));
        AppAction::None
    }

    async fn handle_confirm(&mut self) -> AppAction {
        let form = PlayerForm {
            name: self.name.get_selected_value().unwrap_or_default(),
            role: self.role.get_selected_value(),
            number: self.number.get_selected_value().unwrap_or_default(),
            birth_date: self.birth_date.get_selected_value().unwrap_or_default(),
            height_cm: self.height.get_selected_value().unwrap_or_default(),
            notes: self.notes.get_selected_value().unwrap_or_default(),
        };
        match form.validate(
            &self.team,
            self.existing_player.as_ref(),
            Local::now().date_naive(),
        ) {
            Err(e) => {
                self.notify_message
                    .set_error(e.message(current_labels().domain));
                AppAction::None
            }
            Ok(input) => match self.team_writer.save_player(input, &mut self.team).await {
                Ok(_) => {
                    self.back = true;
                    self.notify_message
                        .set_info(current_labels().operation_successful.to_string());
                    AppAction::None
                }
                Err(_) => {
                    self.notify_message
                        .set_error(current_labels().could_not_create_player.to_string());
                    AppAction::None
                }
            },
        }
    }

    fn handle_next(&mut self) -> AppAction {
        self.field = (self.field + 1) % FIELDS;
        self.update_writing_modes();
        AppAction::None
    }

    fn handle_previous(&mut self) -> AppAction {
        self.field = (self.field + FIELDS - 1) % FIELDS;
        self.update_writing_modes();
        AppAction::None
    }

    fn update_writing_modes(&mut self) {
        self.name.writing_mode = self.field == 0;
        self.role.writing_mode = self.field == 1;
        self.number.writing_mode = self.field == 2;
        self.birth_date.writing_mode = self.field == 3;
        self.height.writing_mode = self.field == 4;
        self.notes.writing_mode = self.field == 5;
    }

    fn handle_up(&mut self) -> AppAction {
        self.role.handle_up();
        AppAction::None
    }

    fn handle_down(&mut self) -> AppAction {
        self.role.handle_down();
        AppAction::None
    }

    fn handle_backspace(&mut self) -> AppAction {
        self.text_boxes()
            .into_iter()
            .for_each(TextBox::handle_backspace);
        AppAction::None
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let block = panel::screen(match self.existing_player {
            Some(_) => current_labels().edit_player,
            None => current_labels().new_player,
        });
        f.render_widget(block, area);
    }
}
