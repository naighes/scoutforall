use crate::localization::{Describe, current_labels};
use crate::providers::settings_reader::SettingsReader;
use crate::providers::settings_writer::SettingsWriter;
use crate::screens::live_stats_screen::LiveStatsScreen;
use crate::screens::screen::{Sba, get_keybinding_actions};
use crate::screens::settings_screen::{SettingsPanel, SettingsScreen};
use crate::screens::{
    components::{
        navigation_footer::NavigationFooter, notify_banner::NotifyBanner, panel, set_board,
    },
    screen::{AppAction, Renderable, ScreenAsync},
};
use crate::shapes::actions::{ScoutingOptionEnum, ScreenActionEnum};
use crate::shapes::keybinding::navigation_keybindings;
use crate::shapes::keybinding::{ActionsKeyBindings, KeyBindings, combination_of};
use crate::shapes::settings::Settings;
use crate::shapes::symbol::KeyCombinationFormatExt;
use async_trait::async_trait;
use crokey::crossterm::event::{KeyCode, KeyEvent};
use crokey::{KeyCombination, KeyCombinationFormat};
use ratatui::text::Line;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Row, Table},
};
use scout4all_core::{
    scouting::{PlayerChoice, Progress, ScoutingError, ScoutingSession, ScoutingStep},
    shapes::{
        enums::{EventTypeEnum, FriendlyName, RoleEnum},
        snapshot::EventEntry,
        technique::{ServeTypeEnum, SetTypeEnum, TechniqueEnum},
    },
};
use std::sync::Arc;
use unicode_width::UnicodeWidthStr;

/// The scouting of a set: the keys choose the event, the player, the
/// evaluation and the set or serve of the session of the core.
#[derive(Debug)]
pub struct ScoutingScreen<
    SW: SettingsWriter + Send + Sync + 'static,
    SR: SettingsReader + Send + Sync + 'static,
> {
    settings: Settings,
    session: ScoutingSession,
    notify_message: NotifyBanner,
    back_stack_count: Option<u8>,
    back: bool,
    footer: NavigationFooter,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
    event_screen_key_bindings: ActionsKeyBindings<EventTypeEnum>,
    event_key_bindings: KeyBindings<EventTypeEnum>,
    settings_writer: Arc<SW>,
    settings_reader: Arc<SR>,
}

/// The score of the set, on top.
const SCORE_HEADER_HEIGHT: u16 = 3;
/// The point sequence, below the score: a row per team and the borders.
const POINT_SEQUENCE_HEIGHT: u16 = 4;

/// The rows the tallest panel of the left column takes: the evaluations
/// of an event with, below, the sets or serves to choose from (the attack
/// of a middle blocker), each list with its borders.
fn tallest_left_column() -> u16 {
    let most_sets = RoleEnum::ALL
        .into_iter()
        .flat_map(|role| [false, true].map(|back_row| SetTypeEnum::available(role, back_row).len()))
        .max()
        .unwrap_or(0);
    EventTypeEnum::ALL
        .into_iter()
        .map(|event| {
            let techniques = match event {
                EventTypeEnum::A => most_sets,
                EventTypeEnum::S => ServeTypeEnum::ALL.len(),
                _ => 0,
            };
            let evals = event.available_evals().len() + 2;
            match techniques {
                0 => evals,
                n => evals + n + 2,
            }
        })
        .max()
        .unwrap_or(0) as u16
}

impl<SW: SettingsWriter + Send + Sync, SR: SettingsReader + Send + Sync> Renderable
    for ScoutingScreen<SW, SR>
{
    fn footer_entries(&self) -> Vec<(String, String)> {
        get_keybinding_actions(navigation_keybindings(), &self.get_screen_actions())
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        // the point sequence only when the tallest panel of the left column
        // still fits below it
        let point_sequence_height = match body.height
            >= SCORE_HEADER_HEIGHT + POINT_SEQUENCE_HEIGHT + tallest_left_column()
        {
            true => POINT_SEQUENCE_HEIGHT,
            false => 0,
        };
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(SCORE_HEADER_HEIGHT),
                Constraint::Length(point_sequence_height),
                Constraint::Min(1),
            ])
            .split(body);
        let (header, point_sequence, body) = (rows[0], rows[1], rows[2]);
        // the set status as wide as its texts on a narrow screen, the court
        // what's left
        let center_width =
            (body.width / 5).max(set_board::set_status_width(self.session.snapshot()));
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Length(center_width),
                Constraint::Fill(1),
            ])
            .split(body);
        // the sets or serves are listed below the evaluations
        let techniques = match self.session.step() {
            ScoutingStep::Eval => self.session.available_techniques(),
            _ => vec![],
        };
        // the list to choose from takes half of the column, or all it needs
        // when it's longer, the latest events the rest
        let list_height = (columns[0].height / 2)
            .max(self.left_list_len() as u16 + 2)
            .min(columns[0].height);
        let left = Layout::default()
            .direction(Direction::Vertical)
            .constraints(match techniques.len() {
                0 => [
                    Constraint::Length(list_height),
                    Constraint::Length(0),
                    Constraint::Min(0),
                ],
                n => [
                    Constraint::Length(self.session.available_evals().len() as u16 + 2),
                    Constraint::Length(n as u16 + 2),
                    Constraint::Min(0),
                ],
            })
            .split(columns[0]);
        let (left_top, left_middle, left_bottom, center, right) =
            (left[0], left[1], left[2], columns[1], columns[2]);
        match self.session.step() {
            ScoutingStep::Event => {
                self.render_available_events(f, left_top);
            }
            ScoutingStep::Eval => {
                self.render_eval_table(f, left_top);
                self.render_technique_table(f, left_middle, &techniques);
            }
            ScoutingStep::Player => {
                self.render_lineup_choices(f, left_top);
            }
            ScoutingStep::Replacement => {
                self.render_replacement_choices(f, left_top);
            }
        }
        let screen_actions = &self.get_screen_actions();
        let kb = navigation_keybindings();
        let screen_key_bindings = kb.slice(Sba::keys(screen_actions));

        self.render_header(f, header);
        if point_sequence_height > 0 {
            set_board::render_point_sequence(
                f,
                point_sequence,
                self.session.current_match(),
                self.session.snapshot(),
            );
        }
        self.footer.render(f, footer_left, self.footer_entries());
        self.render_recent_events(f, left_bottom);
        self.render_set_status(f, center);
        self.render_court(f, right);
        self.notify_message.render(f, footer_right);
        self.screen_key_bindings = screen_key_bindings;
    }
}

#[async_trait]
impl<SW: SettingsWriter + Send + Sync, SR: SettingsReader + Send + Sync> ScreenAsync
    for ScoutingScreen<SW, SR>
{
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        use ScoutingStep::*;
        if let Some(key_combination) = self.screen_key_bindings.transform(key) {
            match (
                &self.notify_message.has_value(),
                &self.screen_key_bindings.get(key_combination),
                self.session.step(),
            ) {
                (true, _, _) => {
                    self.notify_message.reset();
                    if self.back {
                        AppAction::Back(true, self.back_stack_count)
                    } else {
                        AppAction::None
                    }
                }
                (false, Some(ScreenActionEnum::Back), _) => {
                    AppAction::Back(true, self.back_stack_count)
                }
                (false, Some(ScreenActionEnum::Settings), _) => {
                    AppAction::SwitchScreen(Box::new(SettingsScreen::new(
                        self.settings.clone(),
                        self.settings_writer.clone(),
                        self.settings_reader.clone(),
                        SettingsPanel::Shortcuts,
                    )))
                }
                (false, Some(ScreenActionEnum::LiveStats), _) => {
                    let session = &self.session;
                    let current_match = session.current_match();
                    match LiveStatsScreen::new(
                        &session.sets(),
                        current_match.max_substitutions,
                        &current_match.team,
                    ) {
                        Ok(screen) => AppAction::SwitchScreen(Box::new(screen)),
                        Err(e) => {
                            self.notify_message.set_error(e.to_string());
                            AppAction::None
                        }
                    }
                }
                (false, Some(ScreenActionEnum::Undo), _) => {
                    let undone = self.session.undo().await;
                    self.show(undone.map(|_| Progress::Choosing))
                }
                (false, _, Event) => self.handle_event_screen(key).await,
                (false, _, Player) => self.handle_player_screen(key).await,
                (false, _, Eval) => self.handle_eval_screen(key).await,
                (false, _, Replacement) => self.handle_replacement_screen(key).await,
            }
        } else {
            AppAction::None
        }
    }

    async fn refresh_data(&mut self) {
        if let Ok(settings) = &self.settings_reader.read().await {
            self.settings = settings.to_owned();
            self.event_key_bindings = settings.scouting_keybindings.clone();
            // all the events: the available ones change along the set, and the
            // others get "not available" when pressed
            self.event_screen_key_bindings =
                ActionsKeyBindings::from(self.event_key_bindings.reverse_map().iter().collect());
        }
    }
}

impl<SW: SettingsWriter + Send + Sync, SR: SettingsReader + Send + Sync> ScoutingScreen<SW, SR> {
    pub fn new(
        settings_reader: Arc<SR>,
        settings_writer: Arc<SW>,
        settings: Settings,
        session: ScoutingSession,
        back_stack_count: Option<u8>,
    ) -> Self {
        let event_key_bindings = settings.scouting_keybindings.clone();
        ScoutingScreen {
            settings_reader,
            settings_writer,
            settings,
            session,
            notify_message: NotifyBanner::new(),
            back_stack_count,
            back: false,
            footer: NavigationFooter::new(),
            screen_key_bindings: ActionsKeyBindings::empty(),
            event_key_bindings: event_key_bindings.to_owned(),
            event_screen_key_bindings: ActionsKeyBindings::from(
                event_key_bindings.reverse_map().iter().collect(),
            ),
        }
    }

    /// Tells how the choice went: the error, or that the set is over (the
    /// screen closes once the message is read).
    fn show(&mut self, progress: Result<Progress, ScoutingError>) -> AppAction {
        match progress {
            Ok(Progress::SetOver) => {
                self.notify_message
                    .set_info(current_labels().set_over.to_string());
                self.back = true;
            }
            Ok(Progress::Choosing | Progress::Saved) => {}
            Err(e) => self
                .notify_message
                .set_error(e.message(current_labels().domain)),
        }
        AppAction::None
    }

    /// The set or serve bound to the key, among the available ones.
    fn technique_for(&self, ck: &KeyCombination) -> Option<TechniqueEnum> {
        match self.session.event() {
            Some(EventTypeEnum::A) => self
                .settings
                .set_type_keybindings
                .action_for(ck)
                .copied()
                .map(TechniqueEnum::Set),
            Some(EventTypeEnum::S) => self
                .settings
                .serve_type_keybindings
                .action_for(ck)
                .copied()
                .map(TechniqueEnum::Serve),
            _ => None,
        }
        .filter(|technique| self.session.available_techniques().contains(technique))
    }

    fn technique_key(&self, technique: &TechniqueEnum) -> Option<KeyCombination> {
        match technique {
            TechniqueEnum::Set(set) => self.settings.set_type_keybindings.key_for(set),
            TechniqueEnum::Serve(serve) => self.settings.serve_type_keybindings.key_for(serve),
        }
    }

    /// The entries of the list the scout chooses from in the left column:
    /// the events, the evaluations, the players or their replacements.
    fn left_list_len(&self) -> usize {
        match self.session.step() {
            // the events, then the options
            ScoutingStep::Event => {
                self.session.available_events().len() + ScoutingOptionEnum::ALL.len()
            }
            ScoutingStep::Eval => self.session.available_evals().len(),
            ScoutingStep::Player => self.session.player_choices().len(),
            ScoutingStep::Replacement => self.session.replacement_choices().len(),
        }
    }

    /// The digit typed, e.g. the index of a player.
    fn digit(key: KeyEvent) -> Option<u8> {
        match key.code {
            KeyCode::Char(c) => c.to_digit(10).and_then(|d| u8::try_from(d).ok()),
            _ => None,
        }
    }

    /* event handling */

    // sequence is event type => player => eval
    /// An option, as the inverted P1, or the event.
    async fn handle_event_screen(&mut self, key: KeyEvent) -> AppAction {
        let option = self
            .settings
            .option_keybindings
            .action_for(&combination_of(key))
            .copied();
        let event = self
            .event_screen_key_bindings
            .transform(key)
            .and_then(|ck| self.event_screen_key_bindings.get(ck))
            .copied();
        match (option, event) {
            (Some(ScoutingOptionEnum::InvertedP1), _) => {
                let inverted = self.session.inverted_p1();
                self.session.set_inverted_p1(!inverted);
                AppAction::None
            }
            (None, Some(event)) => {
                let progress = self.session.choose_event(event).await;
                self.show(progress)
            }
            (None, None) => AppAction::None,
        }
    }

    async fn handle_replacement_screen(&mut self, key: KeyEvent) -> AppAction {
        let replacement = match key.code {
            KeyCode::Char(c) => replacement_index(c),
            _ => None,
        }
        .and_then(|i| {
            self.session
                .replacement_choices()
                .into_iter()
                .find(|p| p.index == i)
        });
        match (key.code, self.session.player(), replacement) {
            // the replaced player is required
            (KeyCode::Char(_), None, _) => self.show(Err(ScoutingError::NoPlayerSelected)),
            (_, Some(_), Some(replacement)) => {
                let progress = self.session.choose_replacement(replacement.id).await;
                self.show(progress)
            }
            _ => AppAction::None,
        }
    }

    async fn handle_player_screen(&mut self, key: KeyEvent) -> AppAction {
        let player = Self::digit(key)
            .filter(|d| (1..=7).contains(d))
            .and_then(|d| {
                self.session
                    .player_choices()
                    .into_iter()
                    .find(|p| p.index == d)
            });
        let progress = match player {
            Some(player) => self.session.choose_player(player.id).await,
            None => Err(ScoutingError::InvalidPlayerSelection),
        };
        self.show(progress)
    }

    async fn handle_eval_screen(&mut self, key: KeyEvent) -> AppAction {
        let ck = combination_of(key);
        // the evaluation bound to the key, if the event allows it
        let eval = self
            .settings
            .eval_keybindings
            .action_for(&ck)
            .copied()
            .filter(|eval| self.session.available_evals().contains(eval));
        match (eval, self.technique_for(&ck)) {
            (Some(eval), _) => {
                let progress = self.session.choose_eval(eval).await;
                self.show(progress)
            }
            // the set or serve changes, the evaluation saves the event
            (None, Some(technique)) => {
                let chosen = self.session.choose_technique(technique);
                self.show(chosen.map(|_| Progress::Choosing))
            }
            _ => AppAction::None,
        }
    }

    /* rendering */
    fn render_header(&self, f: &mut Frame, area: Rect) {
        set_board::render_score_header(
            f,
            area,
            self.session.current_match(),
            self.session.set().set_number,
            self.session.snapshot(),
        );
    }

    fn render_available_events(&mut self, f: &mut Frame, area: Rect) {
        let scf = KeyCombinationFormat::default();
        let se = self.event_key_bindings.clone();
        let rows: Vec<Row> = self
            .session
            .available_events()
            .iter()
            .map(|ev| {
                Row::new(vec![
                    Line::from(ev.friendly_name(current_labels().domain)),
                    Line::from(
                        se.key_for(ev)
                            .map(|kc| scf.to_string(kc))
                            .unwrap_or_default(),
                    )
                    .right_aligned(),
                ])
            })
            .chain(ScoutingOptionEnum::ALL.iter().map(|option| {
                let on = match option {
                    ScoutingOptionEnum::InvertedP1 => self.session.inverted_p1(),
                };
                let check = match on {
                    true => "[x]",
                    false => "[ ]",
                };
                Row::new(vec![
                    Line::from(format!("{check} {}", option.describe().1)),
                    Line::from(
                        self.settings
                            .option_keybindings
                            .key_for(option)
                            .map(|kc| scf.to_string(kc))
                            .unwrap_or_default(),
                    )
                    .right_aligned(),
                ])
                .style(Style::default().add_modifier(Modifier::DIM))
            }))
            .collect();
        let table = Table::new(rows, [Constraint::Percentage(60), Constraint::Min(2)]).block(
            panel::section(current_labels().choose_the_event)
                .style(Style::default().add_modifier(Modifier::REVERSED)),
        );
        f.render_widget(table, area);
    }

    fn render_technique_table(&self, f: &mut Frame, area: Rect, techniques: &[TechniqueEnum]) {
        let labels = current_labels();
        let rows: Vec<Row> = techniques
            .iter()
            .map(|technique| {
                let key = self
                    .technique_key(technique)
                    .map(|k| {
                        KeyCombinationFormatExt::new(&KeyCombinationFormat::default()).to_string(k)
                    })
                    .unwrap_or_else(|| labels.unassigned.to_string());
                let (check, style) = match self.session.technique() == Some(*technique) {
                    true => ("[X]", Style::default().add_modifier(Modifier::BOLD)),
                    false => ("[ ]", Style::default()),
                };
                Row::new(vec![format!(
                    "{} {} => {}",
                    check,
                    key,
                    technique.description(current_labels().domain)
                )])
                .style(style)
            })
            .collect();
        let title = match self.session.event() {
            Some(EventTypeEnum::S) => labels.serve_type,
            _ => labels.set_type,
        };
        let table = Table::new(rows, [Constraint::Percentage(100)])
            .block(panel::section(title).style(Style::default().add_modifier(Modifier::REVERSED)));
        f.render_widget(table, area);
    }

    fn render_eval_table(&mut self, f: &mut Frame, area: Rect) {
        let available_evals = self.session.available_evals();
        let rows: Vec<Row> = available_evals
            .iter()
            .map(|ev| {
                let key = self
                    .settings
                    .eval_keybindings
                    .key_for(ev)
                    .map(|k| {
                        KeyCombinationFormatExt::new(&KeyCombinationFormat::default()).to_string(k)
                    })
                    .unwrap_or_else(|| current_labels().unassigned.to_string());
                Row::new(vec![format!(
                    "{} => {}",
                    key,
                    if let Some(last_event) = self.session.event() {
                        if let Some(desc) =
                            ev.friendly_description(last_event, current_labels().domain)
                        {
                            format!(
                                "{} ({})",
                                ev.friendly_name(last_event, current_labels().domain),
                                desc
                            )
                        } else {
                            ev.friendly_name(last_event, current_labels().domain)
                        }
                    } else {
                        current_labels().unknown.to_string()
                    }
                )])
            })
            .collect();
        let table = Table::new(rows, [Constraint::Percentage(100)]).block(
            panel::section(current_labels().choose_the_evaluation)
                .style(Style::default().add_modifier(Modifier::REVERSED)),
        );
        f.render_widget(table, area);
    }

    fn render_court(&self, f: &mut Frame, area: Rect) {
        set_board::render_court(
            f,
            area,
            &self.session.current_match().team,
            self.session.snapshot(),
        );
    }

    fn recent_event_row(&'_ self, i: usize, e: &EventEntry) -> Row<'_> {
        Row::new(vec![
            format!(
                " {:<12}",
                e.event_type.friendly_name(current_labels().domain)
            ),
            format!(
                " {:<20}",
                e.player
                    .and_then(|p1| self.session.current_match().team.find_player(p1))
                    .map(|p| p.name.as_str())
                    .unwrap_or("-")
            ),
            format!(
                " {:<10}",
                [
                    e.eval
                        .map(|e1| e1.friendly_name(e.event_type, current_labels().domain)),
                    e.technique.map(|t| t.to_string()),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ")
            ),
        ])
        .style(if i == 0 {
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else if i.is_multiple_of(2) {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        })
    }

    fn render_recent_events(&self, f: &mut Frame, area: Rect) {
        let mut events: Vec<_> = self.session.set().events.iter().collect();
        events.sort_by_key(|e| e.timestamp);
        let rows = events
            .into_iter()
            .rev()
            .take(16)
            .enumerate()
            .map(|(i, e)| self.recent_event_row(i, e));
        let table = Table::new(
            rows,
            // the event wide enough for "freeball received"
            [
                Constraint::Percentage(44),
                Constraint::Percentage(24),
                Constraint::Percentage(32),
            ],
        )
        .block(panel::section(current_labels().latest_events));
        f.render_widget(table, area);
    }

    fn render_lineup_choices(&self, f: &mut Frame, area: Rect) {
        let choices = self.session.player_choices();
        let keys = choices.iter().map(|p| p.index.to_string()).collect();
        Self::render_player_choices_table(
            f,
            area,
            &choices,
            keys,
            current_labels().player_selection,
        );
    }

    fn render_replacement_choices(&mut self, f: &mut Frame, area: Rect) {
        match self.session.player() {
            None => {
                self.notify_message
                    .set_error(current_labels().domain.no_player_selected.to_string());
            }
            Some(_) => {
                let choices = self.session.replacement_choices();
                let keys = choices.iter().map(|p| replacement_key(p.index)).collect();
                Self::render_player_choices_table(
                    f,
                    area,
                    &choices,
                    keys,
                    current_labels().select_replacement,
                );
            }
        }
    }

    /// The players with the key choosing each: the key, the number ("#99")
    /// and the role sized on their content, the name in the rest.
    fn render_player_choices_table(
        f: &mut Frame,
        area: Rect,
        choices: &[PlayerChoice],
        keys: Vec<String>,
        title: &str,
    ) {
        let role_width = choices
            .iter()
            .map(|p| p.role.width())
            .max()
            .map_or(1, |w| u16::try_from(w).unwrap_or(u16::MAX));
        let rows: Vec<Row> = choices
            .iter()
            .zip(keys)
            .map(|(p, key)| {
                Row::new(vec![
                    format!(" {key}"),
                    format!("#{}", p.number),
                    p.name.clone(),
                    p.role.clone(),
                ])
            })
            .collect();
        let table = Table::new(
            rows,
            [
                Constraint::Length(3),
                Constraint::Length(4),
                Constraint::Fill(1),
                Constraint::Length(role_width),
            ],
        )
        .block(panel::section(title).style(Style::default().add_modifier(Modifier::REVERSED)));
        f.render_widget(table, area);
    }

    fn get_screen_actions(&self) -> Vec<Sba<ScreenActionEnum>> {
        match (self.session.set().events.len(), self.session.step()) {
            (0, ScoutingStep::Event) => vec![
                Sba::Simple(ScreenActionEnum::LiveStats),
                Sba::Simple(ScreenActionEnum::Settings),
                Sba::Simple(ScreenActionEnum::Back),
            ],
            _ => vec![
                Sba::Simple(ScreenActionEnum::Undo),
                Sba::Simple(ScreenActionEnum::LiveStats),
                Sba::Simple(ScreenActionEnum::Settings),
                Sba::Simple(ScreenActionEnum::Back),
            ],
        }
    }

    fn render_set_status(&self, f: &mut Frame, area: Rect) {
        let event = self
            .session
            .event()
            .map(|ev| ev.friendly_name(current_labels().domain).to_string());
        let player = self
            .session
            .player()
            .and_then(|p| self.session.current_match().team.find_player(p))
            .map(|p| p.name.clone());
        set_board::render_set_status(
            f,
            area,
            self.session.snapshot(),
            event,
            player,
            self.session.inverted_p1(),
        );
    }
}
/// The key of the replacement at `index`, from 1: the digits, then the
/// letters: 35 keys, more than any bench, also of a match without a roster.
fn replacement_key(index: u8) -> String {
    match index {
        1..=9 => index.to_string(),
        _ => char::from(b'a'.saturating_add(index.saturating_sub(10))).to_string(),
    }
}

/// The index of the replacement of the key, the other way round.
fn replacement_index(c: char) -> Option<u8> {
    match c {
        '1'..='9' => c.to_digit(10).and_then(|d| u8::try_from(d).ok()),
        'a'..='z' => u8::try_from(c).ok().map(|b| b - b'a' + 10),
        _ => None,
    }
}
