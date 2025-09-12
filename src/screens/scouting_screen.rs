use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Padding, Paragraph, Row, Table, Wrap},
    Frame,
};
use uuid::Uuid;

use crate::{
    ops::{append_event, remove_last_event},
    screens::screen::{AppAction, Screen},
    shapes::{
        enums::{EvalEnum, EventTypeEnum},
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

#[derive(Debug, PartialEq, Eq)]
enum EventTypeInput {
    Some(EventTypeEnum),
    Partial,
    None,
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
        if self.error.is_some() {
            self.error = None;
            return AppAction::None;
        }
        if let KeyCode::Esc = key.code {
            return AppAction::Back(true, self.back_stack_count);
        };
        match self.state {
            ScoutingScreenState::Event => self.handle_event_screen(key),
            ScoutingScreenState::Player => self.handle_player_screen(key),
            ScoutingScreenState::Eval => self.handle_eval_screen(key),
            ScoutingScreenState::Replacement => self.handle_replacement_screen(key),
        }
    }

    fn on_resume(&mut self, _: bool) {}

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        self.render_footer(f, footer_left);
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)])
            .split(body);
        self.render_header(f, rows[0]);
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(20),
                Constraint::Percentage(40),
            ])
            .split(rows[1]);
        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(columns[0]);
        match self.state {
            ScoutingScreenState::Event => {
                self.render_available_events(f, left_chunks[0]);
            }
            ScoutingScreenState::Eval => {
                self.render_eval_table(f, left_chunks[0]);
            }
            ScoutingScreenState::Player => {
                self.render_lineup_choices(f, left_chunks[0]);
            }
            ScoutingScreenState::Replacement => {
                self.render_replacement_choices(f, left_chunks[0]);
            }
        }
        self.render_recent_events(f, left_chunks[1]);
        self.render_set_status(f, columns[1]);
        self.render_court(f, columns[2]);
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

    fn filter_lineup_choices(
        &self,
        i: u8,
        (role, player_id): (String, Option<Uuid>),
    ) -> Option<LineupChoiceEntry> {
        let id = player_id?;
        // ensure is within the lineup
        if self.snapshot.current_lineup.find_position(&id).is_none() {
            return None;
        }
        // ensure this action is allowed for liibero
        let is_libero_allowed_event = match self.current_event {
            EventTypeInput::Some(EventTypeEnum::A)
            | EventTypeInput::Some(EventTypeEnum::B)
            | EventTypeInput::Some(EventTypeEnum::S) => false,
            _ => true,
        };
        if self.snapshot.current_lineup.get_current_libero() == id && !is_libero_allowed_event {
            return None;
        }
        // on block, do not allow back players
        if self.snapshot.current_lineup.is_back_row_player(&id)
            && self.current_event == EventTypeInput::Some(EventTypeEnum::B)
        {
            return None;
        }
        if let Some(pending_touch) = self.snapshot.pending_touch {
            if pending_touch == id {
                return None;
            }
        }
        // search for the player
        let player = self
            .current_match
            .team
            .players
            .iter()
            .find(|p| p.id == id)?;
        Some(LineupChoiceEntry {
            index: i as u8,
            id,
            name: player.name.clone(),
            number: player.number,
            role,
        })
    }

    fn filter_replaceable_choices(
        &self,
        i: u8,
        (role, player_id): (String, Option<Uuid>),
    ) -> Option<LineupChoiceEntry> {
        let id = player_id?;
        // ensure is within the lineup
        if self.snapshot.current_lineup.find_position(&id).is_none() {
            return None;
        }
        // search for the player
        let player = self
            .current_match
            .team
            .players
            .iter()
            .find(|p| p.id == id)?;
        Some(LineupChoiceEntry {
            index: i as u8,
            id,
            name: player.name.clone(),
            number: player.number,
            role,
        })
    }

    fn get_lineup_choices(&self) -> Vec<LineupChoiceEntry> {
        match self.current_event {
            EventTypeInput::Some(EventTypeEnum::R) => self
                .snapshot
                .current_lineup
                .get_replaceable_lineup_choices()
                .into_iter()
                .filter_map(|(i, (role, player_id))| {
                    self.filter_replaceable_choices(i, (role.clone(), player_id))
                })
                .collect(),
            _ => self
                .snapshot
                .current_lineup
                .get_lineup_choices()
                .into_iter()
                .filter_map(|(i, (role, player_id))| {
                    self.filter_lineup_choices(i, (role.clone(), player_id))
                })
                .collect(),
        }
    }

    fn undo_last_event(&mut self) -> AppAction {
        use EventTypeEnum::*;
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
                self.state = ScoutingScreenState::Eval;
            }
            (R, Some(_)) => {
                self.current_event = EventTypeInput::Some(removed_event.event_type);
                self.state = ScoutingScreenState::Replacement;
            }
            (_, Some(_)) => {
                // the removed event involved a player => set the prompt player state
                self.current_event = EventTypeInput::Some(removed_event.event_type);
                self.state = ScoutingScreenState::Player;
            }
            _ => {
                self.current_event = EventTypeInput::None;
                // set the prompt event selection state
                self.state = ScoutingScreenState::Event;
            }
        };
        // need to recompute snapshot from scratch
        match self.set.compute_snapshot() {
            // since the last event has been removed, snapshot needs to
            // be re-computed from scratch
            Ok((snapshot, available_options)) => {
                self.snapshot = snapshot;
                self.currently_available_options = available_options;
            }
            Err(_) => {
                self.error = Some("could not re-compute snapshot".to_string());
            }
        }
        AppAction::None
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
                self.error = Some("could not add event".to_string());
                AppAction::None
            }
        }
    }

    /* event handling */

    // sequence is event type => player => eval
    fn handle_event_screen(&mut self, key: KeyEvent) -> AppAction {
        use EventTypeEnum::*;
        use KeyCode::*;
        if key.code == Char('u') {
            // undo operation
            self.undo_last_event();
            return AppAction::None;
        }
        // parse and set last event
        let last_event = self.map_key_to_event(key.code, &self.current_event);
        match last_event {
            EventTypeInput::Some(event_type) => {
                let is_option_available = self
                    .currently_available_options
                    .iter()
                    .any(|o| *o == event_type);
                match (is_option_available, event_type) {
                    (false, _) => {
                        // the selected option is not available => error
                        self.current_event = EventTypeInput::None;
                        self.error = Some(format!("event {} is not available", event_type));
                    }
                    // these events require player selection
                    (true, A | B | P | EventTypeEnum::F | D | R) => {
                        self.current_event = last_event;
                        self.state = ScoutingScreenState::Player;
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
                        return self.add_event(&entry);
                    }
                    // player is inferred when serving
                    (true, S) => {
                        self.player = self.snapshot.current_lineup.get_serving_player();
                        self.state = ScoutingScreenState::Eval;
                        self.current_event = last_event;
                    }
                }
            }
            EventTypeInput::Partial => {
                self.current_event = EventTypeInput::Partial;
            }
            _ => {
                self.current_event = EventTypeInput::None;
            }
        };
        AppAction::None
    }

    fn handle_replacement_screen(&mut self, key: KeyEvent) -> AppAction {
        use KeyCode::*;
        if key.code == Char('u') {
            // undo
            self.player = None;
            self.state = ScoutingScreenState::Player;
            return AppAction::None;
        }
        // valid char and ongoing event
        let (Char(c), EventTypeInput::Some(event_type)) = (key.code, &self.current_event) else {
            return AppAction::None;
        };
        match self.player {
            None => {
                self.error = Some("no player selected".to_string());
                return AppAction::None;
            }
            Some(replaced_id) => {
                // find available replacements for the selected player
                let available_replacements = self
                    .snapshot
                    .current_lineup
                    .get_available_replacements(&self.current_match.team, replaced_id);
                // expected 1..=available_replacements.len() digits
                let Some(digit) = c.to_digit(10) else {
                    return AppAction::None;
                };
                let digit_usize = digit as usize;
                if !(1..=available_replacements.len()).contains(&digit_usize) {
                    return AppAction::None;
                }
                // lookup selected index in available replacements
                if let Some((_, player)) = available_replacements
                    .iter()
                    .find(|(i, p)| *i == digit as u8)
                {
                    // attempt of collecting the event
                    let entry = EventEntry {
                        timestamp: Utc::now(),
                        event_type: *event_type,
                        eval: None,
                        player: Some(replaced_id),
                        target_player: Some(player.id),
                    };
                    return self.add_event(&entry);
                }
            }
        };
        AppAction::None
    }

    fn handle_player_screen(&mut self, key: KeyEvent) -> AppAction {
        use KeyCode::*;
        if key.code == Char('u') {
            // undo
            self.current_event = EventTypeInput::None;
            self.state = ScoutingScreenState::Event;
            return AppAction::None;
        }
        // valid char and ongoing event
        let (Char(c), EventTypeInput::Some(event_type)) = (key.code, &self.current_event) else {
            return AppAction::None;
        };
        // expected 1..=7 digits
        let Some(digit) = c.to_digit(10) else {
            return AppAction::None;
        };
        if !(1..=7).contains(&digit) {
            return AppAction::None;
        }
        // lookup player in lineup
        let available_lineup_players = self.get_lineup_choices();
        let Some(player) = available_lineup_players
            .iter()
            .find(|p| p.index == digit as u8)
        else {
            return AppAction::None;
        };
        // attempt of collecting the event
        match event_type {
            EventTypeEnum::A
            | EventTypeEnum::B
            | EventTypeEnum::P
            | EventTypeEnum::D
            | EventTypeEnum::S => {
                self.player = Some(player.id);
                self.state = ScoutingScreenState::Eval;
            }
            EventTypeEnum::R => {
                self.player = Some(player.id);
                self.state = ScoutingScreenState::Replacement;
            }
            _ => {
                let entry = EventEntry {
                    timestamp: Utc::now(),
                    event_type: *event_type,
                    eval: None,
                    player: Some(player.id),
                    target_player: None,
                };
                return self.add_event(&entry);
            }
        }
        AppAction::None
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
        match (eval, &self.current_event) {
            (Some(eval), EventTypeInput::Some(event_type)) => {
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
                        self.error =
                            Some(format!("evaluation not allowed for event {}", event_type));
                    }
                }
            }
            _ => {}
        };
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
        let team_name = &self.current_match.team.name;
        let opponent_name = &self.current_match.opponent;
        let (left_name, left_wins, right_name, right_wins, score) = if self.current_match.home {
            (
                team_name,
                &us_wins,
                opponent_name,
                &them_wins,
                format!("{} – {}", self.snapshot.score_us, self.snapshot.score_them),
            )
        } else {
            (
                opponent_name,
                &them_wins,
                team_name,
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
            .map(|ev| Row::new(vec![format!("{} ({})", ev, ev.friendly_name())]))
            .collect();
        let table = Table::new(rows, [Constraint::Percentage(100)]).block(
            Block::default()
                .borders(Borders::ALL)
                .title("choose the event")
                .style(Style::default().add_modifier(Modifier::REVERSED)),
        );
        f.render_widget(table, area);
    }

    fn render_eval_table(&mut self, f: &mut Frame, area: Rect) {
        use EvalEnum::*;
        use EventTypeEnum::*;
        let available_evals = match self.current_event {
            EventTypeInput::Some(S | A | B) => {
                vec![Perfect, Positive, Over, Negative, Error]
            }
            EventTypeInput::Some(D | P) => {
                vec![Perfect, Positive, Exclamative, Over, Negative, Error]
            }
            _ => vec![],
        };
        let rows: Vec<Row> = available_evals
            .iter()
            .map(|ev| {
                Row::new(vec![format!(
                    "{} => {}",
                    ev.to_string(),
                    if let EventTypeInput::Some(last_event) = self.current_event {
                        if let Some(desc) = ev.friendly_description(last_event) {
                            format!("{} ({})", ev.friendly_name(last_event), desc)
                        } else {
                            format!("{}", ev.friendly_name(last_event))
                        }
                    } else {
                        "unknown".to_string()
                    }
                )])
            })
            .collect();
        let table = Table::new(rows, [Constraint::Percentage(100)]).block(
            Block::default()
                .borders(Borders::ALL)
                .title("choose the evaluation")
                .style(Style::default().add_modifier(Modifier::REVERSED)),
        );
        f.render_widget(table, area);
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
                let cell_index = row_index * 3 + column_index;
                let position_index = position_map[cell_index];
                let is_setter = match (
                    self.snapshot.current_lineup.get(position_index),
                    self.snapshot.current_lineup.get_setter(),
                ) {
                    (Some(player_id), Some(setter_id)) => player_id == setter_id,
                    _ => false,
                };
                let is_libero = match (
                    self.snapshot.current_lineup.get(position_index),
                    self.snapshot.current_lineup.get_current_libero(),
                ) {
                    (Some(player_id), setter_id) => player_id == setter_id,
                    _ => false,
                };
                let is_serving = self.currently_available_options.contains(&EventTypeEnum::S)
                    && column_index == 2
                    && row_index == 1;
                if let Some(player_id) = self.snapshot.current_lineup.get(position_index) {
                    let content = if let Some(player) = self
                        .current_match
                        .team
                        .players
                        .iter()
                        .find(|p| p.id == player_id)
                    {
                        let arrow = (if is_serving {
                            "\n\n\n\n\n\n .\n / \\\n /   \\\n/_   _\\\n | |"
                        } else {
                            ""
                        })
                        .to_string();
                        if is_setter {
                            format!("{}\n{}\n(S){}", player.number, player.name, arrow)
                        } else if is_libero {
                            format!("{}\n{}\n(L){}", player.number, player.name, arrow)
                        } else {
                            format!("{}\n{}{}", player.number, player.name, arrow)
                        }
                    } else {
                        format!("pos {}", position_index + 1)
                    };
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
                    f.render_widget(cell, *cell_area);
                }
            }
        }
    }

    fn render_recent_events(&self, f: &mut Frame, area: Rect) {
        let mut sorted = self.set.events.clone();
        sorted.sort_by_key(|e| e.timestamp);
        let recent_events: Vec<_> = sorted.into_iter().rev().take(16).collect();
        let rows: Vec<Row> = recent_events
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let style = if i == 0 {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else if i % 2 == 0 {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };
                Row::new(vec![
                    format!(" {:<12}", e.event_type.friendly_name()),
                    format!(
                        " {:<20}",
                        match e.player.and_then(|p1| self
                            .current_match
                            .team
                            .players
                            .iter()
                            .find(|p2| p1 == p2.id))
                        {
                            Some(p) => p.name.clone(),
                            None => "-".to_string(),
                        }
                    ),
                    format!(
                        " {:<10}",
                        match e.eval.map(|e1| e1.friendly_name(e.event_type)) {
                            Some(e) => e,
                            None => "".to_string(),
                        }
                    ),
                ])
                .style(style)
            })
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
                .title("latest events"),
        );

        f.render_widget(table, area);
    }

    fn render_lineup_choices(&self, f: &mut Frame, area: Rect) {
        let rows: Vec<Row> = self
            .get_lineup_choices()
            .iter()
            .map(|e| {
                Row::new(vec![
                    format!(" {:<12}", e.index),
                    format!(" #{:<12}", e.number),
                    format!(" {:<12}", e.name),
                    format!(" {:<20}", e.role),
                ])
            })
            .collect();
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
                .title("player selection")
                .style(Style::default().add_modifier(Modifier::REVERSED)),
        );
        f.render_widget(table, area);
    }

    fn render_replacement_choices(&mut self, f: &mut Frame, area: Rect) {
        match self.player {
            None => {
                self.error = Some("no player selected".to_string());
            }
            Some(replaced_id) => {
                let rows: Vec<Row> = self
                    .snapshot
                    .current_lineup
                    .get_available_replacements(&self.current_match.team, replaced_id)
                    .iter()
                    .map(|(i, e)| {
                        Row::new(vec![
                            format!(" {:<12}", i),
                            format!(" #{:<12}", e.number),
                            format!(" {:<12}", e.name),
                            format!(" {:<20}", e.role),
                        ])
                    })
                    .collect();
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
                        .title("select replacement")
                        .style(Style::default().add_modifier(Modifier::REVERSED)),
                );
                f.render_widget(table, area);
            }
        }
    }

    fn render_footer(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::NONE)
            .padding(Padding::new(1, 0, 0, 0));
        if self.set.events.len() == 0 && self.state == ScoutingScreenState::Event {
            f.render_widget(Paragraph::new("Esc = back | Q = quit").block(block), area);
        } else {
            f.render_widget(
                Paragraph::new(format!(
                    "U = undo | Esc = back | Q = quit CE:{:?}",
                    self.current_event
                ))
                .block(block),
                area,
            );
        }
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
        let phase_container = Paragraph::new(format!(
            "\n{}\n",
            self.snapshot.current_lineup.get_current_phase().to_string()
        ))
        .style(
            Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
        f.render_widget(phase_container, chunks[0]);
        if let Ok(rotation) = self.snapshot.current_lineup.get_current_rotation() {
            let rotation_container = Paragraph::new(format!("\nS{}\n", rotation + 1))
                .style(
                    Style::default()
                        .bg(Color::Cyan)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            f.render_widget(rotation_container, chunks[1]);
        }
        if let EventTypeInput::Some(ev) = &self.current_event {
            let event_container = Paragraph::new(format!("\n{}\n", ev.to_string()))
                .style(
                    Style::default()
                        .bg(Color::LightGreen)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            f.render_widget(event_container, chunks[2]);
        }
        if let Some(player) = &self
            .player
            .and_then(|p| self.current_match.team.players.iter().find(|p1| p1.id == p))
        {
            let player_container = Paragraph::new(format!("\n{}\n", player.name))
                .style(
                    Style::default()
                        .bg(Color::Magenta)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });
            f.render_widget(player_container, chunks[3]);
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
                .block(Block::default().borders(Borders::ALL).title("error"));
            f.render_widget(error_widget, area);
        }
    }
}
