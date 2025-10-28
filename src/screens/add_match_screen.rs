use std::sync::Arc;

use crate::{
    localization::current_labels,
    providers::{match_writer::MatchWriter, set_writer::SetWriter},
    screens::{
        components::{
            checkbox::CheckBox, date_picker::DatePicker, navigation_footer::NavigationFooter,
            notify_banner::NotifyBanner, team_header::TeamHeader, text_box::TextBox,
        },
        screen::{get_keybinding_actions, AppAction, Renderable, Sba, ScreenAsync},
        start_set_screen::StartSetScreen,
    },
    shapes::{
        enums::ScreenActionEnum, keybinding::KeyBindings, settings::Settings, team::TeamEntry,
    },
};
use async_trait::async_trait;
use crokey::{
    crossterm::event::{KeyCode, KeyEvent},
    Combiner,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{Block, Borders},
    Frame,
};

#[derive(Debug)]
pub struct AddMatchScreen<MW: MatchWriter + Send + Sync, SSW: SetWriter + Send + Sync> {
    settings: Settings,
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
    combiner: Combiner,
    screen_key_bindings: KeyBindings,
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
        if let Some(key_combination) = self.combiner.transform(key) {
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

impl<MW: MatchWriter + Send + Sync + 'static, SSW: SetWriter + Send + Sync + 'static>
    AddMatchScreen<MW, SSW>
{
    pub fn new(
        settings: Settings,
        team: TeamEntry,
        match_writer: Arc<MW>,
        set_writer: Arc<SSW>,
    ) -> Self {
        let opponent = TextBox::new(current_labels().opponent.to_owned(), true, None);
        let home = CheckBox::new(current_labels().home.to_owned(), false, false);
        let date = DatePicker::new(current_labels().date.to_owned(), false);

        let screen_actions = &[
            &ScreenActionEnum::Next,
            &ScreenActionEnum::Previous,
            &ScreenActionEnum::Confirm,
            &ScreenActionEnum::Back,
        ];
        let kb = &settings.keybindings.clone();
        let footer_entries = get_keybinding_actions(kb, Sba::Simple(&screen_actions.to_vec()));
        let screen_key_bindings = kb.slice(screen_actions.to_vec());
        AddMatchScreen {
            settings,
            team,
            opponent,
            date,
            home,
            field: 0,
            notify_message: NotifyBanner::new(),
            header: TeamHeader::default(),
            footer: NavigationFooter::new(),
            footer_entries,
            match_writer,
            set_writer,
            combiner: Combiner::default(),
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
        AppAction::None
    }

    fn handle_char(&mut self, c: char) -> AppAction {
        self.opponent.handle_char(c);
        self.home.handle_char(c);
        self.date.handle_char(c);
        AppAction::None
    }

    fn handle_next(&mut self) -> AppAction {
        self.date.handle_tab();
        self.field = (self.field + 1) % 3;
        self.update_writing_modes();
        AppAction::None
    }

    fn handle_previous(&mut self) -> AppAction {
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
