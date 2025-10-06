use std::sync::Arc;

use crate::{
    localization::current_labels,
    providers::{match_writer::MatchWriter, set_writer::SetWriter},
    screens::{
        components::{
            checkbox::CheckBox, date_picker::DatePicker, navigation_footer::NavigationFooter,
            notify_banner::NotifyBanner, team_header::TeamHeader, text_box::TextBox,
        },
        screen::{AppAction, Renderable, ScreenAsync},
        start_set_screen::StartSetScreen,
    },
    shapes::team::TeamEntry,
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
    Frame,
};

#[derive(Debug)]
pub struct AddMatchScreen<MW: MatchWriter + Send + Sync, SSW: SetWriter + Send + Sync> {
    team: TeamEntry,
    opponent: TextBox, // field 0
    date: DatePicker,  // field 1
    home: CheckBox,    // field 2
    field: usize,
    notify_message: NotifyBanner,
    header: TeamHeader,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
    match_writer: Arc<MW>,
    set_writer: Arc<SSW>,
}

impl<MW: MatchWriter + Send + Sync + 'static, SSW: SetWriter + Send + Sync + 'static> Renderable
    for AddMatchScreen<MW, SSW>
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
                Constraint::Min(1),
            ])
            .split(container[1]);
        self.notify_message.render(f, footer_right);
        self.render_header(f, container[1]);
        self.opponent.render(f, area[0]);
        self.date.render(f, area[1]);
        self.home.render(f, area[2]);
        self.header.render(f, container[0], Some(&self.team));
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

#[async_trait]
impl<MW: MatchWriter + Send + Sync + 'static, SSW: SetWriter + Send + Sync + 'static> ScreenAsync
    for AddMatchScreen<MW, SSW>
{
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
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
            (KeyCode::Enter, _) => self.handle_submit().await,
            _ => AppAction::None,
        }
    }

    async fn refresh_data(&mut self) {}
}

impl<MW: MatchWriter + Send + Sync + 'static, SSW: SetWriter + Send + Sync + 'static>
    AddMatchScreen<MW, SSW>
{
    pub fn new(team: TeamEntry, match_writer: Arc<MW>, set_writer: Arc<SSW>) -> Self {
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
            footer: NavigationFooter::new(),
            footer_entries: vec![
                (
                    "Tab / Shift+Tab".to_string(),
                    current_labels().switch_field.to_string(),
                ),
                (
                    current_labels().enter.to_string(),
                    current_labels().confirm.to_string(),
                ),
                ("Esc".to_string(), current_labels().back.to_string()),
                ("Q".to_string(), current_labels().quit.to_string()),
            ],
            match_writer,
            set_writer,
        }
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(current_labels().new_match);
        f.render_widget(block, area);
    }

    async fn handle_submit(&mut self) -> AppAction {
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
                match self
                    .match_writer
                    .create(&self.team, opponent, date, self.home.get_selected_value())
                    .await
                {
                    Ok(m) => AppAction::SwitchScreen(Box::new(StartSetScreen::new(
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
