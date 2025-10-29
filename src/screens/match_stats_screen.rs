use crate::{
    errors::AppError,
    localization::current_labels,
    screens::{
        components::{navigation_footer::NavigationFooter, notify_banner::NotifyBanner},
        screen::{get_keybinding_actions, AppAction, Renderable, Sba, ScreenAsync},
    },
    shapes::{
        enums::{
            ErrorTypeEnum, EvalEnum, EventTypeEnum, FriendlyName, PhaseEnum, RotationEnum,
            ScreenActionEnum, ZoneEnum,
        },
        keybinding::KeyBindings,
        player::PlayerEntry,
        r#match::MatchEntry,
        set::SetEntry,
        settings::Settings,
        snapshot::Snapshot,
        stats::{Metric, Stats},
    },
};
use async_trait::async_trait;
use crokey::{
    crossterm::event::{KeyCode, KeyEvent},
    Combiner,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{
        canvas::Canvas, Bar, BarChart, BarGroup, Block, Borders, Cell, Gauge, List, ListItem,
        ListState, Padding, Row, Table,
    },
    Frame,
};
use std::{collections::HashSet, fmt::Display, iter::once};
use uuid::Uuid;

struct Selection<T>
where
    T: Display,
{
    items: Vec<Option<T>>,
    state: ListState,
    title: String,
    writing_mode: bool,
}

impl<T> Selection<T>
where
    T: Display,
{
    pub fn new(title: String, items: Vec<Option<T>>) -> Self {
        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(0));
        }
        Self {
            items,
            state,
            title,
            writing_mode: false,
        }
    }

    pub fn selected(&self) -> Option<&T> {
        match self.state.selected() {
            None => None,
            Some(i) => self.items.get(i).and_then(|opt| opt.as_ref()),
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect) {
        let block_style = if self.writing_mode {
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let items: Vec<ListItem> = self
            .items
            .iter()
            .map(|p| {
                ListItem::new(if let Some(ref p) = p {
                    p.to_string()
                } else {
                    "-".to_string()
                })
            })
            .collect();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(self.title.as_str())
                    .style(block_style),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::Yellow)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");
        f.render_stateful_widget(list, area, &mut self.state);
    }

    fn next(&mut self) {
        if self.writing_mode {
            let i = match self.state.selected() {
                Some(i) => {
                    if i >= self.items.len() - 1 {
                        0
                    } else {
                        i + 1
                    }
                }
                None => 0,
            };
            self.state.select(Some(i));
        }
    }

    fn previous(&mut self) {
        if self.writing_mode {
            let i = match self.state.selected() {
                Some(i) => {
                    if i == 0 {
                        self.items.len() - 1
                    } else {
                        i - 1
                    }
                }
                None => 0,
            };
            self.state.select(Some(i));
        }
    }

    fn disable_writing_mode(&mut self) {
        self.writing_mode = false;
    }

    fn enable_writing_mode(&mut self) {
        self.writing_mode = true;
    }
}

pub struct EventSelection {
    event_type: EventTypeEnum,
    label: String,
}

impl Display for EventSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label)
    }
}

pub struct MatchStatsScreen {
    notify_message: NotifyBanner,
    state: ListState,
    set_filter: Selection<u8>,
    rotation_filter: Selection<RotationEnum>,
    phase_filter: Selection<PhaseEnum>,
    event_filter: Selection<EventSelection>,
    player_filter: Selection<PlayerEntry>,
    sets: Vec<(SetEntry, Snapshot)>,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
    combiner: Combiner,
    screen_key_bindings: KeyBindings,
}

#[async_trait]
impl ScreenAsync for MatchStatsScreen {
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
                (Some(ScreenActionEnum::ScrollDown), _, _) => {
                    self.set_filter.previous();
                    self.rotation_filter.previous();
                    self.phase_filter.previous();
                    self.event_filter.previous();
                    self.player_filter.previous();
                    AppAction::None
                }
                (Some(ScreenActionEnum::ScrollUp), _, _) => {
                    self.set_filter.next();
                    self.rotation_filter.next();
                    self.phase_filter.next();
                    self.event_filter.next();
                    self.player_filter.next();
                    AppAction::None
                }
                (Some(ScreenActionEnum::Back), KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
                (Some(ScreenActionEnum::Next), _, _) => {
                    match self.state.selected() {
                        Some(i) => {
                            let next_index = if i >= 4 { 0 } else { i + 1 };
                            self.state.select(Some(next_index));
                            self.set_filter.disable_writing_mode();
                            self.rotation_filter.disable_writing_mode();
                            self.phase_filter.disable_writing_mode();
                            self.event_filter.disable_writing_mode();
                            self.player_filter.disable_writing_mode();
                            match next_index {
                                0 => self.set_filter.enable_writing_mode(),
                                1 => self.rotation_filter.enable_writing_mode(),
                                2 => self.phase_filter.enable_writing_mode(),
                                3 => self.event_filter.enable_writing_mode(),
                                4 => self.player_filter.enable_writing_mode(),
                                _ => {}
                            }
                        }
                        None => {
                            self.state.select(Some(0));
                        }
                    }
                    AppAction::None
                }
                (Some(ScreenActionEnum::Previous), _, _) => {
                    match self.state.selected() {
                        Some(i) => {
                            let prev_index = if i == 0 { 4 } else { i - 1 };
                            self.state.select(Some(prev_index));
                            self.set_filter.disable_writing_mode();
                            self.rotation_filter.disable_writing_mode();
                            self.phase_filter.disable_writing_mode();
                            self.event_filter.disable_writing_mode();
                            self.player_filter.disable_writing_mode();
                            match prev_index {
                                0 => self.set_filter.enable_writing_mode(),
                                1 => self.rotation_filter.enable_writing_mode(),
                                2 => self.phase_filter.enable_writing_mode(),
                                3 => self.event_filter.enable_writing_mode(),
                                4 => self.player_filter.enable_writing_mode(),
                                _ => {}
                            }
                        }
                        None => {
                            self.state.select(Some(0));
                        }
                    }
                    AppAction::None
                }
                _ => AppAction::None,
            }
        } else {
            AppAction::None
        }
    }

    async fn refresh_data(&mut self) {}
}

impl Renderable for MatchStatsScreen {
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, _: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(body);
        let left_col = chunks[0];
        let center_col = chunks[1];
        let right_col = chunks[2];
        self.render_left(f, left_col);
        self.render_center(f, center_col);
        self.render_right(f, right_col);
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

impl MatchStatsScreen {
    pub fn new(settings: Settings, current_match: MatchEntry) -> Result<Self, AppError> {
        use EventTypeEnum::*;
        let mut state = ListState::default();
        state.select(Some(0));
        let mut sets: Vec<(SetEntry, Snapshot)> = Vec::new();
        for set in &current_match.sets {
            let (snapshot, _) = set.compute_snapshot()?;
            sets.push((set.clone(), snapshot));
        }
        let set_numbers = sets.iter().map(|(set, _)| set.set_number);
        let mut set_filter = Selection::new(
            "set".to_string(),
            once(None).chain(set_numbers.map(Some)).collect(),
        );
        set_filter.enable_writing_mode();
        let rotation_filter = Selection::new(
            current_labels().rotation.into(),
            once(None)
                .chain(RotationEnum::ALL.iter().copied().map(Some))
                .collect(),
        );
        let phase_filter = Selection::new(
            current_labels().phase.into(),
            once(None)
                .chain(PhaseEnum::ALL.iter().copied().map(Some))
                .collect(),
        );
        let event_filter = Selection::new(
            "stat".to_string(),
            vec![
                EventSelection {
                    event_type: S,
                    label: S.friendly_name(current_labels()).to_string(),
                },
                EventSelection {
                    event_type: P,
                    label: P.friendly_name(current_labels()).to_string(),
                },
                EventSelection {
                    event_type: D,
                    label: D.friendly_name(current_labels()).to_string(),
                },
                EventSelection {
                    event_type: B,
                    label: B.friendly_name(current_labels()).to_string(),
                },
                EventSelection {
                    event_type: A,
                    label: A.friendly_name(current_labels()).to_string(),
                },
            ]
            .into_iter()
            .map(Some)
            .collect(),
        );
        let mut players: HashSet<Uuid> = HashSet::new();
        for (_, snapshot) in &sets {
            players.extend(
                snapshot
                    .current_lineup
                    .get_involved_players()
                    .iter()
                    .cloned(),
            );
        }
        let players = once(None).chain(
            players
                .iter()
                .map(|p| current_match.team.find_player(*p).cloned()),
        );
        let screen_actions = &[
            Sba::Simple(ScreenActionEnum::Next),
            Sba::Simple(ScreenActionEnum::Previous),
            Sba::Simple(ScreenActionEnum::ScrollUp),
            Sba::Simple(ScreenActionEnum::ScrollDown),
            Sba::Simple(ScreenActionEnum::Back),
            Sba::Simple(ScreenActionEnum::Quit),
        ];
        let kb = &settings.keybindings;
        let player_filter = Selection::new(current_labels().player.to_string(), players.collect());
        let footer_entries = get_keybinding_actions(kb, screen_actions);
        let screen_key_bindings = kb.slice(Sba::keys(screen_actions));

        Ok(Self {
            notify_message: NotifyBanner::new(),
            set_filter,
            rotation_filter,
            phase_filter,
            event_filter,
            player_filter,
            state,
            sets,
            footer: NavigationFooter::new(),
            footer_entries,
            combiner: Combiner::default(),
            screen_key_bindings,
        })
    }

    fn get_current_stats(&self, set_number: Option<u8>) -> Option<Stats> {
        match set_number {
            None => {
                let mut aggregated_stats = Stats::new();
                for (_, snapshot) in &self.sets {
                    aggregated_stats.merge(&snapshot.stats);
                }
                Some(aggregated_stats)
            }
            Some(num) => self
                .sets
                .iter()
                .find(|(set, _)| set.set_number == num)
                .map(|(_, snapshot)| snapshot.stats.clone()),
        }
    }

    fn render_event_stats(&self, f: &mut Frame, selection: &EventSelection, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(20),
                Constraint::Length(3),
                Constraint::Length(3),
            ])
            .split(area);
        self.render_evals_bars(f, selection, chunks[0]);
        self.render_summary_table(f, selection, chunks[1]);
        self.render_efficiency_bars(f, selection, chunks[2]);
        self.render_positiveness_bars(f, selection, chunks[3]);
    }

    fn render_right(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);
        if let Some(EventTypeEnum::A) = self
            .event_filter
            .selected()
            .map(|selection| selection.event_type)
        {
            self.render_court_canvas(f, chunks[0], current_labels().distribution, |(p, _)| p);
            self.render_court_canvas(f, chunks[1], current_labels().conversion_rate, |(_, s)| s);
        }
    }

    pub fn render_court_canvas<F>(&self, f: &mut Frame, area: Rect, label: &str, select_value: F)
    where
        F: Fn((f64, f64)) -> f64,
    {
        let set = self.set_filter.selected().copied();
        let rotation = self.rotation_filter.selected().cloned().map(|r| r as u8);
        let phase = self.phase_filter.selected().cloned();
        let player = self.player_filter.selected().map(|p| p.id);
        if let Some(stats) = self.get_current_stats(set) {
            let zones = [
                (ZoneEnum::Four, 1.0, 1.0),
                (ZoneEnum::Three, 4.0, 1.0),
                (ZoneEnum::Two, 7.0, 1.0),
                (ZoneEnum::Eight, 4.0, 4.5),
                (ZoneEnum::Nine, 7.0, 4.5),
            ];
            let char_ratio = 2.0;
            let side: u16 = (area.width as f64 * 0.8) as u16;
            let visual_height = (side as f64 / char_ratio).floor() as u16;
            let x_offset = (area.width as i32 - side as i32) / 2;
            let y_offset = (area.height as i32 - visual_height as i32) / 2;
            let field_rect = Rect {
                x: area.x + x_offset.max(0) as u16,
                y: area.y + y_offset.max(0) as u16,
                width: side.min(area.width),
                height: visual_height.min(area.height),
            };
            let field_side: f64 = 9.0;
            let canvas = Canvas::default()
                .block(Block::default().borders(Borders::ALL).title(label))
                .x_bounds([0.0, field_side])
                .y_bounds([0.0, field_side])
                .paint(|ctx| {
                    let y_inverted = |y: f64| field_side - y;
                    ctx.draw(&ratatui::widgets::canvas::Line {
                        x1: 0.0,
                        y1: y_inverted(3.0),
                        x2: field_side,
                        y2: y_inverted(3.0),
                        color: Color::Blue,
                    });
                    for (zone, x, y) in zones.iter() {
                        let text = stats
                            .distribution
                            .zone_stats(*zone, phase, rotation, player, None)
                            .map(|(p, s)| format!("{:.1}%", select_value((p, s))))
                            .unwrap_or("-".to_string());
                        ctx.print(*x, y_inverted(*y), text);
                    }
                });
            f.render_widget(canvas, field_rect);
        }
    }

    fn render_evals_bars(&self, f: &mut Frame, selection: &EventSelection, area: Rect) {
        let event_type = selection.event_type;
        let set = self.set_filter.selected().copied();
        let rotation = self.rotation_filter.selected().cloned().map(|r| r as u8);
        let phase = self.phase_filter.selected().cloned();
        let player = self.player_filter.selected().map(|p| p.id);
        let stats = self.get_current_stats(set);
        let evals_with_colors = match event_type {
            EventTypeEnum::S => vec![
                (EvalEnum::Perfect, Color::Green),
                (EvalEnum::Over, Color::Green),
                (EvalEnum::Positive, Color::LightGreen),
                (EvalEnum::Negative, Color::Yellow),
                (EvalEnum::Error, Color::Red),
            ],
            EventTypeEnum::P => vec![
                (EvalEnum::Perfect, Color::Green),
                (EvalEnum::Positive, Color::LightGreen),
                (EvalEnum::Exclamative, Color::LightYellow),
                (EvalEnum::Negative, Color::Yellow),
                (EvalEnum::Over, Color::Yellow),
                (EvalEnum::Error, Color::Red),
            ],
            EventTypeEnum::B => vec![
                (EvalEnum::Perfect, Color::Green),
                (EvalEnum::Positive, Color::LightGreen),
                (EvalEnum::Negative, Color::Yellow),
                (EvalEnum::Error, Color::Red),
                (EvalEnum::Over, Color::Red),
            ],
            EventTypeEnum::D => vec![
                (EvalEnum::Perfect, Color::Green),
                (EvalEnum::Positive, Color::LightGreen),
                (EvalEnum::Exclamative, Color::LightYellow),
                (EvalEnum::Negative, Color::Yellow),
                (EvalEnum::Over, Color::Yellow),
                (EvalEnum::Error, Color::Red),
            ],
            EventTypeEnum::A => vec![
                (EvalEnum::Perfect, Color::Green),
                (EvalEnum::Positive, Color::LightGreen),
                (EvalEnum::Negative, Color::Yellow),
                (EvalEnum::Over, Color::Red),
                (EvalEnum::Error, Color::Red),
            ],
            _ => vec![],
        };
        let mut bars: Vec<Bar> = Vec::new();
        if let Some(stats) = &stats {
            for (eval, color) in evals_with_colors {
                if let Some((percent, _total, _count)) =
                    stats.event_percentage(event_type, player, phase, rotation, None, eval)
                {
                    bars.push(
                        Bar::default()
                            .label(eval.to_string().into())
                            .value((percent.round() as u64).min(100))
                            .text_value(format!("{}%", percent.round()))
                            .style(Style::default().fg(color))
                            .value_style(Style::default().fg(Color::Black).bg(color)),
                    );
                }
            }
        }
        let group = BarGroup::default().bars(&bars);
        let evals_barchart = BarChart::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(current_labels().evaluations)
                    .padding(Padding::new(2, 2, 1, 1)),
            )
            .data(group)
            .bar_width(5)
            .bar_gap(1)
            .max(100);
        f.render_widget(evals_barchart, area);
    }

    fn render_efficiency_bars(&self, f: &mut Frame, selection: &EventSelection, area: Rect) {
        let event_type = selection.event_type;
        let set = self.set_filter.selected().copied();
        let rotation = self.rotation_filter.selected().cloned().map(|r| r as u8);
        let phase = self.phase_filter.selected().cloned();
        let player = self.player_filter.selected().map(|p| p.id);
        let stats = self.get_current_stats(set);
        if let Some(stats) = &stats {
            if let Some((eff, _tot, _count)) = stats.event_positiveness(
                event_type,
                player,
                phase,
                rotation,
                None,
                Metric::Efficiency,
            ) {
                let color = if eff < 10.0 {
                    Color::Red
                } else if eff < 30.0 {
                    Color::Yellow
                } else if eff < 40.0 {
                    Color::LightGreen
                } else {
                    Color::Green
                };
                let normalized = (((eff + 100.0) / 2.0).clamp(0.0, 100.0)).round() as u16;
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(0)])
                    .split(area);
                let gauge = Gauge::default()
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(current_labels().efficiency),
                    )
                    .gauge_style(Style::default().fg(color).bg(Color::Black))
                    .percent(normalized)
                    .label(Span::styled(
                        format!("{:.0}%", eff),
                        Style::default().fg(Color::White),
                    ));
                f.render_widget(gauge, chunks[0]);
            }
        }
    }

    fn render_positiveness_bars(&self, f: &mut Frame, selection: &EventSelection, area: Rect) {
        let event_type = selection.event_type;
        let set = self.set_filter.selected().copied();
        let rotation = self.rotation_filter.selected().cloned().map(|r| r as u8);
        let phase = self.phase_filter.selected().cloned();
        let player = self.player_filter.selected().map(|p| p.id);
        let stats = self.get_current_stats(set);
        if let Some(stats) = &stats {
            if let Some((eff, _tot, _count)) = stats.event_positiveness(
                event_type,
                player,
                phase,
                rotation,
                None,
                Metric::Positive,
            ) {
                let color = if eff < 10.0 {
                    Color::Red
                } else if eff < 20.0 {
                    Color::Yellow
                } else if eff < 30.0 {
                    Color::LightGreen
                } else {
                    Color::Green
                };
                let normalized = eff.round() as u16;
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(3), Constraint::Min(0)])
                    .split(area);
                let gauge = Gauge::default()
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(current_labels().positiveness),
                    )
                    .gauge_style(Style::default().fg(color).bg(Color::Black))
                    .percent(normalized)
                    .label(Span::styled(
                        format!("{:.0}%", eff),
                        Style::default().fg(Color::White),
                    ));
                f.render_widget(gauge, chunks[0]);
            }
        }
    }

    fn render_summary_table(&self, f: &mut Frame, selection: &EventSelection, area: Rect) {
        let event_type = selection.event_type;
        let set = self.set_filter.selected().copied();
        let rotation = self.rotation_filter.selected().cloned().map(|r| r as u8);
        let phase = self.phase_filter.selected().cloned();
        let player = self.player_filter.selected().map(|p| p.id);
        let stats = self.get_current_stats(set);
        if let Some(stats) = &stats {
            let mut rows: Vec<Row> = vec![];
            let total = stats
                .event_count(event_type, player, phase, rotation, None, None)
                .map(|x| x.to_string())
                .unwrap_or("-".to_string());
            rows.push(Row::new(vec![
                Cell::from(current_labels().total),
                Cell::from(total),
            ]));
            let errors = stats.errors(event_type, None, player, phase, rotation, None);
            let unforced_errors = stats.errors(
                event_type,
                Some(ErrorTypeEnum::Unforced),
                player,
                phase,
                rotation,
                None,
            );
            rows.push(Row::new(vec![
                Cell::from(format!(
                    "{}({})",
                    current_labels().errors,
                    current_labels().unforced
                )),
                Cell::from(match (errors, unforced_errors) {
                    (Some(e), Some(ue)) => format!("{}({})", e, ue),
                    (Some(e), None) => format!("{}(-)", e),
                    _ => "-".to_string(),
                }),
            ]));
            let phases_convertion_rate = stats.number_of_phases_per_scored_point(phase, rotation);
            if player.is_none() {
                rows.push(Row::new(vec![
                    Cell::from(current_labels().phase_efficiency),
                    Cell::from(match phases_convertion_rate {
                        Some((p, t, c)) => format!("{:.2}({}/{})", p, c, t),
                        _ => "-".to_string(),
                    }),
                ]));
            }
            let faults = stats
                .event_count(EventTypeEnum::F, player, phase, rotation, None, None)
                .map(|x| x.to_string())
                .unwrap_or("-".to_string());
            rows.push(Row::new(vec![
                Cell::from(current_labels().faults),
                Cell::from(faults),
            ]));
            if event_type.provides_direct_points() {
                let points = stats
                    .scored_points(event_type, player, phase, rotation, None)
                    .map(|x| x.to_string())
                    .unwrap_or("-".to_string());
                rows.push(Row::new(vec![
                    Cell::from(current_labels().points),
                    Cell::from(points),
                ]));
            }
            let summary_box = Table::new(rows, [Constraint::Length(16), Constraint::Length(5)])
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(current_labels().summary),
                )
                .widths([Constraint::Percentage(70), Constraint::Percentage(30)]);
            f.render_widget(summary_box, area);
        }
    }

    fn render_center(&mut self, f: &mut Frame, area: Rect) {
        if let Some(selection) = self.event_filter.selected() {
            self.render_event_stats(f, selection, area)
        }
    }

    fn render_left(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),  // set
                Constraint::Length(9),  // rotation
                Constraint::Length(5),  // phase
                Constraint::Length(9),  // event
                Constraint::Length(14), // player
            ])
            .split(area);

        self.set_filter.render(f, chunks[0]);
        self.rotation_filter.render(f, chunks[1]);
        self.phase_filter.render(f, chunks[2]);
        self.event_filter.render(f, chunks[3]);
        self.player_filter.render(f, chunks[4]);
    }
}
