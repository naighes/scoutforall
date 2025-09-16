use crate::{
    localization::current_labels,
    ops::{load_settings, load_teams},
    screens::{
        edit_team::EditTeamScreen,
        screen::{AppAction, Screen},
        settings::SettingsScreen,
        team_details::TeamDetailsScreen,
    },
    shapes::{enums::FriendlyName, team::TeamEntry},
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph, Wrap},
    Frame,
};

#[derive(Debug)]
pub struct TeamListScreen {
    list_state: ListState,
    teams: Vec<TeamEntry>,
    refresh: bool,
    error: Option<String>,
}

impl Screen for TeamListScreen {
    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        match (key.code, &self.error) {
            (_, Some(_)) => {
                self.error = None;
                AppAction::None
            }
            (KeyCode::Down, _) => {
                self.next_team();
                AppAction::None
            }
            (KeyCode::Up, _) => {
                self.previous_team();
                AppAction::None
            }
            (KeyCode::Enter, _) => match self.list_state.selected().and_then(|x| self.teams.get(x))
            {
                None => AppAction::None,
                Some(team) => AppAction::SwitchScreen(Box::new(TeamDetailsScreen::new(
                    self.teams.clone(),
                    team.id,
                ))),
            },
            (KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
            (KeyCode::Char('n'), _) => AppAction::SwitchScreen(Box::new(EditTeamScreen::new())),
            (KeyCode::Char('s'), _) => match load_settings() {
                Ok(settings) => AppAction::SwitchScreen(Box::new(SettingsScreen::new(settings))),
                Err(_) => {
                    self.error = Some(current_labels().could_not_load_settings.to_string());
                    AppAction::None
                }
            },
            _ => AppAction::None,
        }
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        if self.refresh {
            self.refresh = false;
            self.teams = match load_teams() {
                Ok(teams) => teams,
                Err(_) => {
                    self.error = Some(current_labels().could_not_load_teams.to_string());
                    vec![]
                }
            }
        }
        self.render_error(f, footer_right);
        let items: Vec<ListItem> = self
            .teams
            .iter()
            .map(|t| {
                ListItem::new(format!(
                    "{} ({}/{}, {})",
                    t.name,
                    t.classification
                        .map(|c| c.friendly_name(current_labels()))
                        .unwrap_or_default(),
                    t.gender
                        .map(|g| g.friendly_name(current_labels()))
                        .unwrap_or_default(),
                    t.year,
                ))
            })
            .collect();
        if items.is_empty() {
            self.render_no_teams_yet(f, body);
        } else {
            self.render_list(f, body, items);
        }
        self.render_footer(f, footer_left);
    }

    fn on_resume(&mut self, refresh: bool) {
        if refresh {
            self.refresh = true;
        }
    }
}

impl TeamListScreen {
    pub fn new() -> Self {
        TeamListScreen {
            teams: vec![],
            refresh: true,
            list_state: ListState::default(),
            error: None,
        }
    }

    fn next_team(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let new_selected = (selected + 1).min(self.teams.len() - 1);
            self.list_state.select(Some(new_selected));
        }
    }

    fn previous_team(&mut self) {
        if let Some(selected) = self.list_state.selected() {
            let new_selected = if selected == 0 { 0 } else { selected - 1 };
            self.list_state.select(Some(new_selected));
        }
    }

    fn render_footer(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        let paragraph = (match self.teams.len() {
            0 => Paragraph::new(format!(
                "N = {} | S = {} | Q = {}",
                current_labels().new_team,
                current_labels().settings,
                current_labels().quit
            )),
            _ => Paragraph::new(format!(
                "↑↓ = {} | Enter = {} | S = {} | N = {} | Q = {}",
                current_labels().navigate,
                current_labels().select,
                current_labels().settings,
                current_labels().new_team,
                current_labels().quit
            )),
        })
        .block(block)
        .wrap(Wrap { trim: true });
        f.render_widget(paragraph, area);
    }

    fn render_no_teams_yet(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Length(3),
                Constraint::Percentage(40),
            ])
            .split(area);
        let paragraph = Paragraph::new(current_labels().no_teams_yet)
            .block(Block::default().borders(Borders::NONE))
            .alignment(Alignment::Center);
        f.render_widget(paragraph, chunks[1]);
    }

    fn render_list(&mut self, f: &mut Frame, area: Rect, items: Vec<ListItem>) {
        if self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(current_labels().teams),
            )
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .add_modifier(Modifier::REVERSED),
            )
            .highlight_symbol(">> ");

        f.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_error(&self, f: &mut Frame, area: Rect) {
        if let Some(err) = &self.error {
            let error_widget = Paragraph::new(err.clone())
                .style(
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                )
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(current_labels().error),
                );
            f.render_widget(error_widget, area);
        }
    }
}
