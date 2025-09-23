use crate::{
    localization::current_labels,
    ops::{save_team, TeamInput},
    screens::{
        components::{notify_banner::NotifyBanner, select::Select, text_box::TextBox},
        screen::{AppAction, Screen},
    },
    shapes::{
        enums::{FriendlyName, GenderEnum, TeamClassificationEnum},
        team::TeamEntry,
    },
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
    Frame,
};

#[derive(Debug)]
pub struct EditTeamScreen {
    name: TextBox,
    gender: Select<GenderEnum>,
    classification: Select<TeamClassificationEnum>,
    year: TextBox,
    field: usize,
    notify_message: NotifyBanner,
    existing_team: Option<TeamEntry>,
    back: bool,
}

impl Screen for EditTeamScreen {
    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        match (key.code, &self.notify_message.has_value()) {
            (_, true) => self.handle_error_reset(),
            (KeyCode::Char(c), _) => self.handle_char(c),
            (KeyCode::Backspace, _) => self.handle_backspace(),
            (KeyCode::Up, _) => self.handle_up(),
            (KeyCode::Down, _) => self.handle_down(),
            (KeyCode::Tab, _) => self.handle_tab(),
            (KeyCode::BackTab, _) => self.handle_backtab(),
            (KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
            (KeyCode::Enter, _) => self.handle_enter(),
            _ => AppAction::None,
        }
    }

    fn on_resume(&mut self, _: bool) {}

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let area = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // 0: name
                Constraint::Length(3), // 1: classification
                Constraint::Length(3), // 2: classification info
                Constraint::Length(3), // 3: gender
                Constraint::Length(3), // 4: year
                Constraint::Min(1),
            ])
            .split(body);
        self.notify_message.render(f, footer_right);
        self.render_header(f, body);
        self.name.render(f, area[0]);
        self.classification.render(f, area[1]);
        self.render_classification_description(f, area[2]);
        self.gender.render(f, area[3]);
        self.year.render(f, area[4]);
        self.render_footer(f, footer_left);
    }
}

impl EditTeamScreen {
    pub fn new() -> Self {
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
        let year = TextBox::with_validator(
            current_labels().year.to_owned(),
            false,
            None,
            |current: &str, c: char| current.len() < 4 && c.is_ascii_digit(),
        );
        EditTeamScreen {
            name,
            gender,
            classification,
            year,
            field: 0,
            notify_message: NotifyBanner::new(),
            existing_team: None,
            back: false,
        }
    }

    pub fn edit(team: &TeamEntry) -> Self {
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
        let year = TextBox::with_validator(
            current_labels().year.to_owned(),
            false,
            Some(&team.year.to_string()),
            |current: &str, c: char| current.len() < 4 && c.is_ascii_digit(),
        );
        EditTeamScreen {
            name,
            gender,
            classification,
            year,
            field: 0,
            notify_message: NotifyBanner::new(),
            existing_team: Some(team.clone()),
            back: false,
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
        self.field = (self.field + 1) % 4;
        self.update_writing_modes();
        AppAction::None
    }

    fn handle_backtab(&mut self) -> AppAction {
        self.field = (self.field + 3) % 4;
        self.update_writing_modes();
        AppAction::None
    }

    fn update_writing_modes(&mut self) {
        self.name.writing_mode = self.field == 0;
        self.classification.writing_mode = self.field == 1;
        self.gender.writing_mode = self.field == 2;
        self.year.writing_mode = self.field == 3;
    }

    fn handle_up(&mut self) -> AppAction {
        self.gender.handle_down();
        self.classification.handle_up();
        AppAction::None
    }

    fn handle_down(&mut self) -> AppAction {
        self.gender.handle_down();
        self.classification.handle_up();
        AppAction::None
    }

    fn handle_backspace(&mut self) -> AppAction {
        self.name.handle_backspace();
        self.year.handle_backspace();
        AppAction::None
    }

    fn handle_enter(&mut self) -> AppAction {
        match (
            self.name.get_selected_value(),
            self.classification.get_selected_value(),
            self.gender.get_selected_value(),
            self.year.get_selected_value().map(|y| y.parse::<u16>()),
        ) {
            (None, _, _, _) => {
                self.notify_message
                    .set_error(current_labels().name_cannot_be_empty.to_string());
                AppAction::None
            }
            (_, None, _, _) => {
                self.notify_message
                    .set_error(current_labels().classification_is_required.to_string());
                AppAction::None
            }
            (_, _, None, _) => {
                self.notify_message
                    .set_error(current_labels().gender_is_required.to_string());
                AppAction::None
            }
            (Some(name), Some(classification), Some(gender), Some(Ok(year))) => {
                let input = match &self.existing_team {
                    Some(team) => {
                        let mut updated = team.clone();
                        updated.name = name;
                        updated.year = year;
                        updated.classification = Some(classification);
                        updated.gender = Some(gender);
                        TeamInput::Existing(updated)
                    }
                    None => TeamInput::New {
                        name,
                        year,
                        classification: Some(classification),
                        gender: Some(gender),
                    },
                };
                match save_team(input) {
                    Ok(_) => {
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
                }
            }
            (_, _, _, None | Some(Err(_))) => {
                self.notify_message.set_error(
                    current_labels()
                        .year_must_be_a_four_digit_number
                        .to_string(),
                );
                AppAction::None
            }
        }
    }

    fn handle_char(&mut self, c: char) -> AppAction {
        self.name.handle_char(c);
        self.year.handle_char(c);
        AppAction::None
    }

    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        let paragraph = Paragraph::new(format!(
            "Tab / Shift+Tab | Enter = {} | Esc = {} | Q = {}",
            current_labels().confirm,
            current_labels().back,
            current_labels().quit
        ))
        .block(block)
        .wrap(Wrap { trim: true });
        f.render_widget(paragraph, area);
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(match self.existing_team {
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
            let paragraph = Paragraph::new(classification.friendly_description(current_labels()))
                .style(Style::default().fg(Color::Cyan))
                .block(
                    Block::default().borders(Borders::ALL).title(Span::styled(
                        classification.friendly_name(current_labels()),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )),
                )
                .alignment(Alignment::Left);
            f.render_widget(paragraph, area);
        }
    }
}
