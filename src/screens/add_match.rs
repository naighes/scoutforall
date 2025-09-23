use crate::{
    localization::current_labels,
    ops::create_match,
    screens::{
        components::{
            checkbox::CheckBox, date_picker::DatePicker, notify_banner::NotifyBanner,
            team_header::TeamHeader, text_box::TextBox,
        },
        screen::{AppAction, Screen},
        start_set_screen::StartSetScreen,
    },
    shapes::team::TeamEntry,
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
    Frame,
};

#[derive(Debug)]
pub struct AddMatchScreen {
    team: TeamEntry,
    opponent: TextBox, // field 0
    date: DatePicker,  // field 1
    home: CheckBox,    // field 2
    field: usize,
    notify_message: NotifyBanner,
    header: TeamHeader,
}

impl Screen for AddMatchScreen {
    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        match (key.code, &self.notify_message.has_value()) {
            (_, true) => {
                self.notify_message.reset();
                AppAction::None
            }
            (KeyCode::Char(c), _) => self.handle_char(c),
            (KeyCode::Backspace, _) => self.handle_backspace(),
            (KeyCode::Tab, _) => self.handle_tab(),
            (KeyCode::BackTab, _) => self.handle_backtab(),
            (KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
            (KeyCode::Enter, _) => self.handle_submit(),
            _ => AppAction::None,
        }
    }

    fn on_resume(&mut self, _: bool) {}

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
                Constraint::Min(1),
            ])
            .split(container[1]);
        self.notify_message.render(f, footer_right);
        self.render_header(f, container[1]);
        self.opponent.render(f, area[0]);
        self.date.render(f, area[1]);
        self.home.render(f, area[2]);
        self.header.render(f, container[0], Some(&self.team));
        self.render_footer(f, footer_left);
    }
}

impl AddMatchScreen {
    pub fn new(team: TeamEntry) -> Self {
        let opponent = TextBox::new(current_labels().opponent.to_owned(), true, None);
        let home = CheckBox::new(current_labels().home.to_owned(), false);
        let date = DatePicker::new(current_labels().date.to_owned(), false);
        AddMatchScreen {
            team,
            opponent,
            date,
            home,
            field: 0,
            notify_message: NotifyBanner::new(),
            header: TeamHeader::default(),
        }
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(current_labels().new_match);
        f.render_widget(block, area);
    }

    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        let paragraph = Paragraph::new(format!(
            "Tab / Shift+Tab = {} | Enter = {} | Esc = {} | Q = {}",
            current_labels().navigate,
            current_labels().confirm,
            current_labels().back,
            current_labels().quit,
        ))
        .block(block)
        .wrap(Wrap { trim: true });
        f.render_widget(paragraph, area);
    }

    fn handle_submit(&mut self) -> AppAction {
        match (
            self.date.get_selected_value(),
            self.opponent.get_selected_value(),
        ) {
            (_, None) => {
                self.notify_message
                    .set_error(current_labels().opponent_cannot_be_empty.to_string());
                AppAction::None
            }
            (Ok(date), Some(opponent)) => {
                match create_match(&self.team, opponent, date, self.home.get_selected_value()) {
                    Ok(m) => {
                        AppAction::SwitchScreen(Box::new(StartSetScreen::new(m, 1, None, Some(2))))
                    }
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
        AppAction::None
    }

    fn handle_char(&mut self, c: char) -> AppAction {
        self.opponent.handle_char(c);
        self.home.handle_char(c);
        self.date.handle_char(c);
        AppAction::None
    }

    fn handle_tab(&mut self) -> AppAction {
        self.date.handle_tab();
        self.field = (self.field + 1) % 3;
        self.update_writing_modes();
        AppAction::None
    }

    fn handle_backtab(&mut self) -> AppAction {
        self.date.handle_tab();
        self.field = (self.field + 2) % 3;
        self.update_writing_modes();
        AppAction::None
    }

    fn update_writing_modes(&mut self) {
        self.opponent.writing_mode = self.field == 0;
        self.date.writing_mode = self.field == 1;
        self.home.writing_mode = self.field == 2;
    }
}
