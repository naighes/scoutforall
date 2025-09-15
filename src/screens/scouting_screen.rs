use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Padding, Paragraph, Row, Table},
    Frame,
};
use uuid::Uuid;

use crate::{
    localization::current_labels,
    ops::{append_event, remove_last_event},
    screens::screen::{AppAction, Screen},
    shapes::{
        enums::{EvalEnum, EventTypeEnum, RoleEnum},
        player::PlayerEntry,
        r#match::MatchEntry,
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
    },
};

#[derive(Debug)]
pub struct ScoutingScreen {
    current_match: MatchEntry,
    set: SetEntry,
    snapshot: Snapshot,
    currently_available_options: Vec<EventTypeEnum>,
    current_event: EventTypeInput,
    player: Option<Uuid>,
    state: ScoutingScreenState,
    error: Option<String>,
    back_stack_count: Option<u8>,
}

#[derive(Debug)]
pub struct LineupChoiceEntry {
    index: u8,
    id: Uuid,
    name: String,
    number: u8,
    role: String,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum EventTypeInput {
    Some(EventTypeEnum),
    Partial,
    None,
}

impl EventTypeInput {
    pub fn is_allowed_for(&self, role: RoleEnum) -> bool {
        use EventTypeEnum::*;
        use EventTypeInput::*;
        !matches!(
            (self, role),
            (Some(A), RoleEnum::Libero) | (Some(B), RoleEnum::Libero) | (Some(S), RoleEnum::Libero)
        )
    }
}

#[derive(Debug, PartialEq)]
enum ScoutingScreenState {
    Event,
    Player,
    Eval,
    Replacement,
}

impl Screen for ScoutingScreen {
    fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        use KeyCode::*;
        use ScoutingScreenState::*;
        match (&self.error, key.code, &self.state) {
            (Some(_), _, _) => {
                self.error = None;
                AppAction::None
            }
            (None, Esc, _) => AppAction::Back(true, self.back_stack_count),
            (None, _, Event) => self.handle_event_screen(key),
            (None, _, Player) => self.handle_player_screen(key),
            (None, _, Eval) => self.handle_eval_screen(key),
            (None, _, Replacement) => self.handle_replacement_screen(key),
        }
    }

    fn on_resume(&mut self, _: bool) {}

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(body);
        let (header, body) = (rows[0], rows[1]);
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(20),
                Constraint::Percentage(40),
            ])
            .split(body);
        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(columns[0]);
        let (left_top, left_bottom, center, right) = (left[0], left[1], columns[1], columns[2]);
        match self.state {
            ScoutingScreenState::Event => {
                self.render_available_events(f, left_top);
            }
            ScoutingScreenState::Eval => {
                self.render_eval_table(f, left_top);
            }
            ScoutingScreenState::Player => {
                self.render_lineup_choices(f, left_top);
            }
            ScoutingScreenState::Replacement => {
                self.render_replacement_choices(f, left_top);
            }
        }
        self.render_header(f, header);
        self.render_footer(f, footer_left);
        self.render_recent_events(f, left_bottom);
        self.render_set_status(f, center);
        self.render_court(f, right);
        self.render_error(f, footer_right);
    }
}

impl ScoutingScreen {
    pub fn new(
        current_match: MatchEntry,
        set: SetEntry,
        snapshot: Snapshot,
        available_options: Vec<EventTypeEnum>,
        back_stack_count: Option<u8>,
    ) -> Self {
        ScoutingScreen {
            current_match,
            set,
            snapshot,
            currently_available_options: available_options,
            current_event: EventTypeInput::None,
            player: None,
            state: ScoutingScreenState::Event,
            error: None,
            back_stack_count,
        }
    }

    /// Filters and validates a potential lineup choice for a given event.
    fn filter_lineup_choices(
        &self,
        index: u8,
        (role, player_id): (String, Option<Uuid>),
    ) -> Option<LineupChoiceEntry> {
        player_id
            // ensure is within the lineup
            .and_then(|id| self.snapshot.current_lineup.find_position(&id).map(|_| id))
            // ensure this action is allowed for libero
            .take_if(|id| {
                self.snapshot.current_lineup.get_current_libero() != *id
                    || self.current_event.is_allowed_for(RoleEnum::Libero)
            })
            // on block, do not allow back players
            .take_if(|id| {
                !self.snapshot.current_lineup.is_back_row_player(id)
                    || self.current_event != EventTypeInput::Some(EventTypeEnum::B)
            })
            // check for a pending touch
            .take_if(|id| !matches!(self.snapshot.pending_touch, Some(p) if p == *id))
            .and_then(|id| self.current_match.team.find_player(id))
            .map(|player| LineupChoiceEntry {
                index,
                id: player.id,
                name: player.name.clone(),
                number: player.number,
                role,
            })
    }

    /// Filters and validates the players that can be replaced in the current lineup.
    fn filter_replaceable_choices(
        &self,
        index: u8,
        (role, player_id): (String, Option<Uuid>),
    ) -> Option<LineupChoiceEntry> {
        player_id.and_then(|id| {
            self.snapshot
                .current_lineup
                .find_position(&id)
                .and_then(|_| self.current_match.team.find_player(id))
                .map(|p| LineupChoiceEntry {
                    index,
                    id,
                    name: p.name.clone(),
                    number: p.number,
                    role,
                })
        })
    }

    /// Returns the list of lineup choices depending on the current event type.
    ///
    /// - If the current event is a substitution, this function
    ///   returns the players currently on court that can be replaced.
    /// - For all other event types, it returns the players in the current lineup
    ///   that are eligible to be associated with the event.
    fn get_lineup_choices(&self) -> Vec<LineupChoiceEntry> {
        let (choices, filter_fn): (
            Vec<(u8, (String, Option<Uuid>))>,
            Box<dyn Fn(u8, (String, Option<Uuid>)) -> Option<LineupChoiceEntry>>,
        ) = match self.current_event {
            EventTypeInput::Some(EventTypeEnum::R) => (
                self.snapshot
                    .current_lineup
                    .get_replaceable_lineup_choices(),
                Box::new(|i, data| self.filter_replaceable_choices(i, data)),
            ),
            _ => (
                self.snapshot.current_lineup.get_lineup_choices(),
                Box::new(|i, data| self.filter_lineup_choices(i, data)),
            ),
        };
        choices
            .into_iter()
            .filter_map(|(i, (role, player_id))| filter_fn(i, (role, player_id)))
            .collect()
    }

    fn undo_last_event(&mut self) -> AppAction {
        use EventTypeEnum::*;
        use ScoutingScreenState::*;
        // it's the event selection screen => remove the entry from the csv file
        let Ok(Some(removed_event)) = remove_last_event(
            &self.current_match.team,
            &self.current_match.id,
            self.set.set_number,
        ) else {
            // TODO: handle error?
            return AppAction::None;
        };
        self.set.events.pop();
        // set the previous (removed) event player
        self.player = removed_event.player;
        match (removed_event.event_type, removed_event.player) {
            (A | B | D | P | S, Some(_)) => {
                // (A)ttack, (B) block, (D)ig, (P)ass and (S)erve require evaluation
                self.current_event = EventTypeInput::Some(removed_event.event_type);
                // set the prompt eval state
                self.state = Eval;
            }
            (R, Some(_)) => {
                self.current_event = EventTypeInput::Some(removed_event.event_type);
                self.state = Replacement;
            }
            (_, Some(_)) => {
                // the removed event involved a player => set the prompt player state
                self.current_event = EventTypeInput::Some(removed_event.event_type);
                self.state = Player;
            }
            _ => {
                self.current_event = EventTypeInput::None;
                // set the prompt event selection state
                self.state = Event;
            }
        };
        // need to recompute snapshot from scratch
        match self.set.compute_snapshot() {
            // since the last event has been removed, snapshot needs to
            // be re-computed from scratch
            Ok((snapshot, available_options)) => {
                self.snapshot = snapshot;
                self.currently_available_options = available_options;
                AppAction::None
            }
            Err(_) => {
                self.error = Some(current_labels().could_not_compute_snapshot.to_string());
                AppAction::None
            }
        }
    }

    fn map_key_to_event(&self, key: KeyCode, last_event: &EventTypeInput) -> EventTypeInput {
        use EventTypeEnum::*;
        use EventTypeInput::*;
        use KeyCode::*;
        match (key, last_event) {
            (Char('s'), None) => Some(S),
            (Char('p'), None) => Some(P),
            (Char('a'), None) => Some(A),
            (Char('d'), None) => Some(D),
            (Char('b'), None) => Some(B),
            (Char('f'), None) => Some(EventTypeEnum::F),
            (Char('r'), None) => Some(R),
            (Char('o'), None) => Partial,
            (Char('e'), Partial) => Some(OE),
            (Char('s'), Partial) => Some(OS),
            _ => None,
        }
    }

    fn add_event(&mut self, event: &EventEntry) -> AppAction {
        // append event to the file
        let currently_available_options = append_event(
            &self.current_match.team,
            &self.current_match.id,
            self.set.set_number,
            event,
        )
        // update snapshot and get new available options
        .and_then(|_| {
            self.snapshot
                .add_event(event, self.currently_available_options.clone())
        });
        match currently_available_options {
            Ok(options) => {
                // append event to the set
                self.set.events.push(event.clone());
                // reset state
                self.currently_available_options = options;
                self.current_event = EventTypeInput::None;
                self.player = None;
                self.state = ScoutingScreenState::Event;
                match self.snapshot.get_set_winner(self.set.set_number) {
                    None => AppAction::None,
                    Some(_) => AppAction::Back(true, self.back_stack_count),
                }
            }
            Err(_) => {
                self.error = Some(current_labels().could_not_add_event.to_string());
                AppAction::None
            }
        }
    }

    /* event handling */

    // sequence is event type => player => eval
    fn handle_event_screen(&mut self, key: KeyEvent) -> AppAction {
        use EventTypeEnum::*;
        use KeyCode::*;
        let last_event = self.map_key_to_event(key.code, &self.current_event);
        match (key.code, last_event) {
            // undo
            (Char('u'), _) => self.undo_last_event(),
            (_, EventTypeInput::Some(event_type)) => {
                let is_option_available = self.currently_available_options.contains(&event_type);
                match (is_option_available, event_type) {
                    // player is inferred when serving
                    (true, S) => {
                        self.player = self.snapshot.current_lineup.get_serving_player();
                        self.state = ScoutingScreenState::Eval;
                        self.current_event = last_event;
                        AppAction::None
                    }
                    // these events require player selection
                    (true, e) if e.requires_player() => {
                        self.current_event = last_event;
                        self.state = ScoutingScreenState::Player;
                        AppAction::None
                    }
                    // these events do not require player nor evaluation selection
                    (true, OE | OS) => {
                        let entry = EventEntry {
                            timestamp: Utc::now(),
                            event_type,
                            eval: None,
                            player: None,
                            target_player: None,
                        };
                        self.add_event(&entry)
                    }
                    // the selected option is not available => error
                    _ => {
                        self.current_event = EventTypeInput::None;
                        let template = current_labels().event_is_not_available;
                        self.error = Some(template.replace("{}", &event_type.to_string()));
                        AppAction::None
                    }
                }
            }
            (_, EventTypeInput::Partial) => {
                self.current_event = EventTypeInput::Partial;
                AppAction::None
            }
            _ => {
                self.current_event = EventTypeInput::None;
                AppAction::None
            }
        }
    }

    fn handle_replacement_screen(&mut self, key: KeyEvent) -> AppAction {
        use KeyCode::*;
        match (key.code, &self.current_event, self.player) {
            (Char('u'), _, _) => {
                // undo
                self.player = None;
                self.state = ScoutingScreenState::Player;
                AppAction::None
            }
            (Char(_), EventTypeInput::Some(_), None) => {
                // replaced player is required
                self.error = Some(current_labels().no_player_selected.to_string());
                AppAction::None
            }
            (Char(c), EventTypeInput::Some(event_type), Some(replaced_id)) => {
                // find available replacements for the selected (replaced) player
                let available_replacements = self
                    .snapshot
                    .current_lineup
                    .get_available_replacements(&self.current_match.team, replaced_id);
                c.to_digit(10)
                    .and_then(|d| u8::try_from(d).ok())
                    .take_if(|d| (1..=available_replacements.len() as u8).contains(d))
                    .and_then(|d| available_replacements.iter().find(|(i, _)| *i == d))
                    .map(|(_, p)| EventEntry {
                        timestamp: Utc::now(),
                        event_type: *event_type,
                        eval: None,
                        player: Some(replaced_id),
                        target_player: Some(p.id),
                    })
                    .map(|entry| self.add_event(&entry))
                    .unwrap_or(AppAction::None)
            }
            _ => AppAction::None,
        }
    }

    fn handle_player_screen(&mut self, key: KeyEvent) -> AppAction {
        use KeyCode::*;
        let available_lineup_players = self.get_lineup_choices();
        let player = match key.code {
            // undo
            Char('u') => {
                self.current_event = EventTypeInput::None;
                self.state = ScoutingScreenState::Event;
                None
            }
            Char(c) => c
                .to_digit(10)
                .take_if(|d| (1..=7).contains(d))
                .map(|d| d as u8)
                .and_then(|d| available_lineup_players.iter().find(|p| p.index == d)),
            _ => None,
        };
        match (player, &self.current_event) {
            (Some(player), EventTypeInput::Some(event_type))
                if event_type.requires_evaluation() =>
            {
                self.player = Some(player.id);
                self.state = ScoutingScreenState::Eval;
                AppAction::None
            }
            (Some(player), EventTypeInput::Some(EventTypeEnum::R)) => {
                self.player = Some(player.id);
                self.state = ScoutingScreenState::Replacement;
                AppAction::None
            }
            (Some(player), EventTypeInput::Some(event_type)) => self.add_event(&EventEntry {
                timestamp: Utc::now(),
                event_type: *event_type,
                eval: None,
                player: Some(player.id),
                target_player: None,
            }),
            _ => {
                self.error = Some(current_labels().invalid_player_selection.to_string());
                AppAction::None
            }
        }
    }

    fn handle_eval_screen(&mut self, key: KeyEvent) -> AppAction {
        // undo
        if key.code == Char('u') {
            // it's an eval screen, so go to
            // * the ScoutingScreenState::Player screen if it's not serving
            // * otherwise, so it's serving, go to the ScoutingScreenState::Event
            self.player = None;
            if self.current_event == EventTypeInput::Some(S) {
                self.current_event = EventTypeInput::None;
                self.state = ScoutingScreenState::Event;
            } else {
                self.state = ScoutingScreenState::Player;
            }
            return AppAction::None;
        }
        use EventTypeEnum::*;
        use KeyCode::*;
        let eval = match (key.code, &self.current_event) {
            (Char('#'), EventTypeInput::Some(A | B | D | P | S)) => Some(EvalEnum::Perfect),
            (Char('+'), EventTypeInput::Some(A | B | D | P | S)) => Some(EvalEnum::Positive),
            (Char('!'), EventTypeInput::Some(P | D)) => Some(EvalEnum::Exclamative),
            (Char('-'), EventTypeInput::Some(A | B | D | P | S)) => Some(EvalEnum::Negative),
            (Char('/'), EventTypeInput::Some(A | B | D | P | S)) => Some(EvalEnum::Over),
            (Char('='), EventTypeInput::Some(A | B | D | P | S)) => Some(EvalEnum::Error),
            _ => None,
        };
        if let (Some(eval), EventTypeInput::Some(event_type)) = (eval, &self.current_event) {
            match event_type {
                // ensure event type allows evaluation
                A | B | P | D | S => {
                    let entry = EventEntry {
                        timestamp: Utc::now(),
                        event_type: *event_type,
                        eval: Some(eval),
                        player: self.player,
                        target_player: None,
                    };
                    return self.add_event(&entry);
                }
                _ => {
                    let template = current_labels().evaluation_not_allowed_for_event;
                    self.error = Some(template.replace("{}", &event_type.to_string()));
                }
            }
        }
        AppAction::None
    }

    /* rendering */
    fn render_header(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!("set {}", self.set.set_number))
            .border_style(Style::default().fg(Color::Yellow))
            .style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            );
        let inner_area = block.inner(area);
        f.render_widget(block, area);
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(20),
                Constraint::Percentage(40),
            ])
            .split(inner_area);
        let (us_wins, them_wins) = self
            .current_match
            .get_status()
            .map(|s| (s.us_wins.to_string(), s.them_wins.to_string()))
            .unwrap_or(("-".into(), "-".into()));
        let (left_name, left_wins, right_name, right_wins, score) = if self.current_match.home {
            (
                &self.current_match.team.name,
                &us_wins,
                &self.current_match.opponent,
                &them_wins,
                format!("{} – {}", self.snapshot.score_us, self.snapshot.score_them),
            )
        } else {
            (
                &self.current_match.opponent,
                &them_wins,
                &self.current_match.team.name,
                &us_wins,
                format!("{} – {}", self.snapshot.score_them, self.snapshot.score_us),
            )
        };
        let left = Paragraph::new(format!("[{left_wins}] {left_name}"))
            .alignment(Alignment::Left)
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
        f.render_widget(left, chunks[0]);
        let middle = Paragraph::new(score)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::White));
        f.render_widget(middle, chunks[1]);
        let right = Paragraph::new(format!("{right_name} [{right_wins}]"))
            .alignment(Alignment::Right)
            .style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
        f.render_widget(right, chunks[2]);
    }

    fn render_available_events(&mut self, f: &mut Frame, area: Rect) {
        let rows: Vec<Row> = self
            .currently_available_options
            .iter()
            .map(|ev| {
                Row::new(vec![format!(
                    "{} ({})",
                    ev,
                    ev.friendly_name(current_labels())
                )])
            })
            .collect();
        let table = Table::new(rows, [Constraint::Percentage(100)]).block(
            Block::default()
                .borders(Borders::ALL)
                .title(current_labels().choose_the_event)
                .style(Style::default().add_modifier(Modifier::REVERSED)),
        );
        f.render_widget(table, area);
    }

    fn render_eval_table(&mut self, f: &mut Frame, area: Rect) {
        let available_evals = match self.current_event {
            EventTypeInput::Some(event_type) => event_type.available_evals(),
            _ => vec![],
        };
        let rows: Vec<Row> = available_evals
            .iter()
            .map(|ev| {
                Row::new(vec![format!(
                    "{} => {}",
                    ev.to_string(),
                    if let EventTypeInput::Some(last_event) = self.current_event {
                        if let Some(desc) = ev.friendly_description(last_event, current_labels()) {
                            format!(
                                "{} ({})",
                                ev.friendly_name(last_event, current_labels()),
                                desc
                            )
                        } else {
                            ev.friendly_name(last_event, current_labels())
                        }
                    } else {
                        current_labels().unknown.to_string()
                    }
                )])
            })
            .collect();
        let table = Table::new(rows, [Constraint::Percentage(100)]).block(
            Block::default()
                .borders(Borders::ALL)
                .title(current_labels().choose_the_evaluation)
                .style(Style::default().add_modifier(Modifier::REVERSED)),
        );
        f.render_widget(table, area);
    }

    fn format_court_cell(
        &self,
        player: Option<&PlayerEntry>,
        position_index: usize,
        is_serving: bool,
        is_libero: bool,
        is_setter: bool,
    ) -> String {
        player
            .map(|player| {
                let arrow = (if is_serving {
                    "\n\n\n\n\n\n .\n / \\\n /   \\\n/_   _\\\n | |"
                } else {
                    ""
                })
                .to_string();
                if is_setter {
                    format!(
                        "{}\n{}\n({}){}",
                        player.number,
                        current_labels().setter_prefix,
                        player.name,
                        arrow
                    )
                } else if is_libero {
                    format!("{}\n{}\n(L){}", player.number, player.name, arrow)
                } else {
                    format!("{}\n{}{}", player.number, player.name, arrow)
                }
            })
            .unwrap_or_else(|| format!("pos {}", position_index + 1))
    }

    fn render_court_cell(
        &self,
        f: &mut Frame,
        area: &Rect,
        player_id: Uuid,
        position_index: usize,
    ) {
        let is_setter = self
            .snapshot
            .current_lineup
            .has_setter_at_pos(position_index);
        let is_libero = self
            .snapshot
            .current_lineup
            .has_libero_at_pos(position_index);
        let is_serving =
            self.currently_available_options.contains(&EventTypeEnum::S) && position_index == 0;
        let player = self.current_match.team.find_player(player_id);
        let content =
            self.format_court_cell(player, position_index, is_serving, is_libero, is_setter);
        let cell = Paragraph::new(content)
            .alignment(ratatui::layout::Alignment::Center)
            .block(if is_setter {
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::LightBlue))
            } else if is_libero {
                Block::default()
                    .borders(Borders::ALL)
                    .style(Style::default().add_modifier(Modifier::REVERSED))
            } else {
                Block::default().borders(Borders::ALL)
            });
        f.render_widget(cell, *area);
    }

    fn render_court(&self, f: &mut Frame, area: Rect) {
        let court_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        let position_map = [3, 2, 1, 4, 5, 0];
        for (row_index, row_area) in court_rows.iter().enumerate() {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(33),
                    Constraint::Percentage(34),
                    Constraint::Percentage(33),
                ])
                .split(*row_area);
            for (column_index, cell_area) in columns.iter().enumerate() {
                let position_index = row_index * 3 + column_index;
                if let Some(player_id) = self.snapshot.current_lineup.get(position_index) {
                    self.render_court_cell(f, cell_area, player_id, position_map[position_index]);
                }
            }
        }
    }

    fn recent_event_row(&'_ self, i: usize, e: &EventEntry) -> Row<'_> {
        Row::new(vec![
            format!(" {:<12}", e.event_type.friendly_name(current_labels())),
            format!(
                " {:<20}",
                e.player
                    .and_then(|p1| self.current_match.team.find_player(p1))
                    .map(|p| p.name.clone())
                    .unwrap_or("-".to_string())
            ),
            format!(
                " {:<10}",
                e.eval
                    .map(|e1| e1.friendly_name(e.event_type, current_labels()))
                    .unwrap_or("".to_string())
            ),
        ])
        .style(if i == 0 {
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else if i % 2 == 0 {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        })
    }

    fn render_recent_events(&self, f: &mut Frame, area: Rect) {
        let mut sorted: Vec<EventEntry> = self.set.events.clone();
        sorted.sort_by_key(|e| e.timestamp);
        let recent_events: Vec<_> = sorted.into_iter().rev().take(16).collect();
        let rows: Vec<Row> = recent_events
            .iter()
            .enumerate()
            .map(|(i, e)| self.recent_event_row(i, e))
            .collect();
        let table = Table::new(
            rows,
            [
                Constraint::Percentage(26),
                Constraint::Percentage(44),
                Constraint::Percentage(30),
            ],
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(current_labels().latest_events),
        );
        f.render_widget(table, area);
    }

    fn render_lineup_choices(&self, f: &mut Frame, area: Rect) {
        let lineup_choices = self.get_lineup_choices();
        let rows: Vec<Row> = lineup_choices
            .iter()
            .map(Self::format_player_choice_row)
            .collect();
        self.render_player_choices_table(f, area, rows, current_labels().player_selection);
    }

    fn render_replacement_choices(&mut self, f: &mut Frame, area: Rect) {
        match self.player {
            None => {
                self.error = Some(current_labels().no_player_selected.to_string());
            }
            Some(replaced_id) => {
                let rows: Vec<Row> = self
                    .snapshot
                    .current_lineup
                    .get_available_replacements(&self.current_match.team, replaced_id)
                    .iter()
                    .map(|(i, player)| LineupChoiceEntry {
                        index: *i,
                        id: player.id,
                        name: player.name.clone(),
                        number: player.number,
                        role: player.role.to_string(),
                    })
                    .map(|e| Self::format_player_choice_row(&e))
                    .collect();
                self.render_player_choices_table(
                    f,
                    area,
                    rows,
                    current_labels().select_replacement,
                );
            }
        }
    }

    fn format_player_choice_row<'a>(entry: &LineupChoiceEntry) -> Row<'a> {
        Row::new(vec![
            format!(" {:<12}", entry.index),
            format!(" #{:<12}", entry.number),
            format!(" {:<12}", entry.name),
            format!(" {:<20}", entry.role),
        ])
    }

    fn render_player_choices_table(&self, f: &mut Frame, area: Rect, rows: Vec<Row>, title: &str) {
        let table = Table::new(
            rows,
            [
                Constraint::Percentage(8),
                Constraint::Percentage(8),
                Constraint::Percentage(56),
                Constraint::Percentage(28),
            ],
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(Style::default().add_modifier(Modifier::REVERSED)),
        );
        f.render_widget(table, area);
    }

    fn render_footer(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        let paragraph = match (self.set.events.len(), &self.state) {
            (0, ScoutingScreenState::Event) => Paragraph::new(format!(
                "Esc = {} | Q = {}",
                current_labels().back,
                current_labels().quit
            )),
            _ => Paragraph::new(format!(
                "U = {} | Esc = {} | Q = {}",
                current_labels().undo,
                current_labels().back,
                current_labels().quit
            )),
        }
        .block(block);
        f.render_widget(paragraph, area);
    }

    fn render_set_status(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(area);
        let cells: Vec<(Rect, Option<String>, Color, Color)> = vec![
            (
                chunks[0],
                Some(self.snapshot.current_lineup.get_current_phase().to_string()),
                Color::Yellow,
                Color::Black,
            ),
            (
                chunks[1],
                self.snapshot
                    .current_lineup
                    .get_current_rotation()
                    .ok()
                    .map(|r| format!("{}{}", current_labels().setter_prefix, r + 1)),
                Color::Cyan,
                Color::Black,
            ),
            (
                chunks[2],
                match &self.current_event {
                    EventTypeInput::Some(ev) => Some(ev.to_string()),
                    _ => None,
                },
                Color::LightGreen,
                Color::Black,
            ),
            (
                chunks[3],
                self.player
                    .and_then(|p| self.current_match.team.find_player(p))
                    .map(|p| p.name.clone()),
                Color::Magenta,
                Color::Black,
            ),
        ];
        for cell in cells {
            if let (area, Some(text), bg, fg) = cell {
                let paragraph = Paragraph::new(format!("\n{}\n", text))
                    .style(Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD))
                    .alignment(Alignment::Center);
                f.render_widget(paragraph, area);
            }
        }
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
