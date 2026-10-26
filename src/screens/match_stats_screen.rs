use crate::localization::current_labels;
use crate::shapes::keybinding::navigation_keybindings;
use crate::{
    screens::{
        components::{navigation_footer::NavigationFooter, notify_banner::NotifyBanner, panel},
        screen::{
            AppAction, Renderable, Sba, ScreenAsync, cycle, get_keybinding_actions,
            panel_navigation_entries,
        },
    },
    shapes::{actions::ScreenActionEnum, keybinding::ActionsKeyBindings},
};
use async_trait::async_trait;
use crokey::crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{
        Bar, BarChart, BarGroup, Cell, Gauge, List, ListItem, ListState, Padding, Row, Table,
        canvas::Canvas,
    },
};
use scout4all_core::{
    detail::{Breakdown, Detail, Tone},
    errors::AppError,
    shapes::{
        enums::{EventTypeEnum, FriendlyName, PhaseEnum, RotationEnum, ZoneEnum},
        r#match::MatchEntry,
        player::PlayerEntry,
        set::SetEntry,
        snapshot::Snapshot,
        stats::{Stats, StatsFilter},
    },
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
                ListItem::new(if let Some(p) = p {
                    p.to_string()
                } else {
                    "-".to_string()
                })
            })
            .collect();
        let list = List::new(items)
            .block(panel::section(self.title.as_str()).style(block_style))
            .highlight_style(
                Style::default()
                    .bg(Color::Yellow)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");
        f.render_stateful_widget(list, area, &mut self.state);
    }

    /// Selects the item after (`forward`) or before the selected one,
    /// going round, when the filter has the focus.
    fn step(&mut self, forward: bool) {
        let target = match self.state.selected() {
            Some(i) => cycle(i, self.items.len(), forward),
            None => Some(0),
        };
        if self.writing_mode && !self.items.is_empty() {
            self.state.select(target);
        }
    }

    fn enable_writing_mode(&mut self) {
        self.writing_mode = true;
    }
}

/// A filter of the statistics: the keys move the focus through the filters,
/// and the selection in the one having it.
trait Filter {
    fn step(&mut self, forward: bool);
    fn focus(&mut self, focused: bool);
}

impl<T: Display> Filter for Selection<T> {
    fn step(&mut self, forward: bool) {
        Selection::step(self, forward)
    }

    fn focus(&mut self, focused: bool) {
        self.writing_mode = focused;
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
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
}

#[async_trait]
impl ScreenAsync for MatchStatsScreen {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if let Some(key_combination) = self.screen_key_bindings.transform(key) {
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
                    self.filters().into_iter().for_each(|f| f.step(true));
                    AppAction::None
                }
                (Some(ScreenActionEnum::ScrollUp), _, _) => {
                    self.filters().into_iter().for_each(|f| f.step(false));
                    AppAction::None
                }
                (Some(ScreenActionEnum::Back), KeyCode::Esc, _) => AppAction::Back(true, Some(1)),
                (Some(ScreenActionEnum::Quit), _, _) => AppAction::Quit(Ok(())),
                (Some(ScreenActionEnum::Next), _, _) => {
                    self.move_focus(true);
                    AppAction::None
                }
                (Some(ScreenActionEnum::Previous), _, _) => {
                    self.move_focus(false);
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
    fn footer_entries(&self) -> Vec<(String, String)> {
        self.footer_entries.clone()
    }

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
    /// The filters, in the order the focus moves through them.
    fn filters(&mut self) -> [&mut dyn Filter; 5] {
        [
            &mut self.set_filter,
            &mut self.rotation_filter,
            &mut self.phase_filter,
            &mut self.event_filter,
            &mut self.player_filter,
        ]
    }

    /// Moves the focus to the filter after (`forward`) or before the one
    /// having it, going round.
    fn move_focus(&mut self, forward: bool) {
        let count = self.filters().len();
        let target = cycle(self.state.selected().unwrap_or(0), count, forward).unwrap_or(0);
        self.state.select(Some(target));
        self.filters()
            .into_iter()
            .enumerate()
            .for_each(|(i, f)| f.focus(i == target));
    }

    pub fn new(current_match: MatchEntry) -> Result<Self, AppError> {
        use EventTypeEnum::*;
        let mut state = ListState::default();
        state.select(Some(0));
        let mut sets: Vec<(SetEntry, Snapshot)> = Vec::new();
        for set in &current_match.sets {
            let snapshot = set.compute_snapshot(current_match.max_substitutions)?;
            sets.push((set.clone(), snapshot));
        }
        let set_numbers = sets.iter().map(|(set, _)| set.set_number);
        let mut set_filter = Selection::new(
            "set".to_string(),
            once(None).chain(set_numbers.map(Some)).collect(),
        );
        set_filter.enable_writing_mode();
        let rotation_filter = Selection::new(
            current_labels().domain.rotation.into(),
            once(None)
                .chain(RotationEnum::ALL.iter().copied().map(Some))
                .collect(),
        );
        let phase_filter = Selection::new(
            current_labels().domain.phase.into(),
            once(None)
                .chain(PhaseEnum::ALL.iter().copied().map(Some))
                .collect(),
        );
        let event_filter = Selection::new(
            "stat".to_string(),
            vec![
                EventSelection {
                    event_type: S,
                    label: S.friendly_name(current_labels().domain).to_string(),
                },
                EventSelection {
                    event_type: P,
                    label: P.friendly_name(current_labels().domain).to_string(),
                },
                EventSelection {
                    event_type: D,
                    label: D.friendly_name(current_labels().domain).to_string(),
                },
                EventSelection {
                    event_type: B,
                    label: B.friendly_name(current_labels().domain).to_string(),
                },
                EventSelection {
                    event_type: A,
                    label: A.friendly_name(current_labels().domain).to_string(),
                },
                // the freeballs, given and received
                EventSelection {
                    event_type: FB,
                    label: current_labels().domain.freeballs.to_string(),
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
        // by their number, to find them
        let mut players: Vec<Option<PlayerEntry>> = players
            .iter()
            .map(|p| current_match.team.find_player(*p).cloned())
            .collect();
        players.sort_by_key(|p| p.as_ref().map(|p| p.number));
        let players = once(None).chain(players);
        let screen_actions = &[
            Sba::Hidden(ScreenActionEnum::Previous),
            Sba::Hidden(ScreenActionEnum::Next),
            Sba::Simple(ScreenActionEnum::ScrollUp),
            Sba::Simple(ScreenActionEnum::ScrollDown),
            Sba::Simple(ScreenActionEnum::Back),
            Sba::Simple(ScreenActionEnum::Quit),
        ];
        let kb = navigation_keybindings();
        let player_filter = Selection::new(
            current_labels().domain.player.to_string(),
            players.collect(),
        );
        let footer_entries = panel_navigation_entries()
            .into_iter()
            .chain(get_keybinding_actions(kb, screen_actions))
            .collect();
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

    /// The statistics filtered by the selected phase, rotation and player.
    fn current_filter(&self) -> StatsFilter {
        StatsFilter::new(
            self.phase_filter.selected().cloned(),
            // the rotations are listed in order, from the setter in position 1
            self.rotation_filter.selected().map(|r| *r as u8),
            self.player_filter.selected().map(|p| p.id),
        )
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
        let Some(Breakdown::Fundamental(detail)) = self.breakdown(selection.event_type) else {
            return;
        };
        let with_player = self.player_filter.selected().is_some();
        Self::render_evals_bars(f, &detail, chunks[0]);
        Self::render_summary_table(f, &detail, with_player, chunks[1]);
        Self::render_efficiency_bars(f, &detail, chunks[2]);
        Self::render_positiveness_bars(f, &detail, chunks[3]);
    }

    /// The fundamental in the set (or the match) selected, narrowed by the
    /// filters.
    fn breakdown(&self, event_type: EventTypeEnum) -> Option<Breakdown> {
        self.get_current_stats(self.set_filter.selected().copied())
            .map(|stats| Breakdown::of(&stats, event_type, &self.current_filter()))
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
            self.render_court_canvas(
                f,
                chunks[0],
                current_labels().domain.distribution,
                |(p, _)| p,
            );
            self.render_court_canvas(f, chunks[1], current_labels().conversion_rate, |(_, s)| s);
        }
    }

    pub fn render_court_canvas<F>(&self, f: &mut Frame, area: Rect, label: &str, select_value: F)
    where
        F: Fn((f64, f64)) -> f64,
    {
        if let Some(Breakdown::Fundamental(detail)) = self.breakdown(EventTypeEnum::A) {
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
                .block(panel::section(label))
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
                        let text = detail
                            .zones
                            .iter()
                            .find(|z| z.zone == *zone)
                            .map(|z| format!("{:.1}%", select_value((z.share, z.conversion))))
                            .unwrap_or("-".to_string());
                        ctx.print(*x, y_inverted(*y), text);
                    }
                });
            f.render_widget(canvas, field_rect);
        }
    }

    fn render_evals_bars(f: &mut Frame, detail: &Detail, area: Rect) {
        // only the evaluations given at least once
        let bars: Vec<Bar> = detail
            .evals
            .iter()
            .map(|e| {
                let color = tone_color(e.tone);
                Bar::default()
                    .label(e.eval.to_string())
                    .value((e.share.round() as u64).min(100))
                    .text_value(format!("{}%", e.share.round()))
                    .style(Style::default().fg(color))
                    .value_style(Style::default().fg(Color::Black).bg(color))
            })
            .collect();
        let group = BarGroup::default().bars(&bars);
        let evals_barchart = BarChart::default()
            .block(panel::section(current_labels().evaluations).padding(Padding::new(2, 2, 1, 1)))
            .data(group)
            .bar_width(5)
            .bar_gap(1)
            .max(100);
        f.render_widget(evals_barchart, area);
    }

    fn render_efficiency_bars(f: &mut Frame, detail: &Detail, area: Rect) {
        if let Some(eff) = detail.efficiency {
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
                .block(panel::section(current_labels().efficiency))
                .gauge_style(Style::default().fg(color).bg(Color::Black))
                .percent(normalized)
                .label(Span::styled(
                    format!("{:.0}%", eff),
                    Style::default().fg(Color::White),
                ));
            f.render_widget(gauge, chunks[0]);
        }
    }

    fn render_positiveness_bars(f: &mut Frame, detail: &Detail, area: Rect) {
        if let Some(eff) = detail.positiveness {
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
                .block(panel::section(current_labels().positiveness))
                .gauge_style(Style::default().fg(color).bg(Color::Black))
                .percent(normalized)
                .label(Span::styled(
                    format!("{:.0}%", eff),
                    Style::default().fg(Color::White),
                ));
            f.render_widget(gauge, chunks[0]);
        }
    }

    fn render_summary_table(f: &mut Frame, detail: &Detail, with_player: bool, area: Rect) {
        let count = |n: u32| match n {
            0 => "-".to_string(),
            n => n.to_string(),
        };
        let mut rows: Vec<Row> = vec![Row::new(vec![
            Cell::from(current_labels().domain.total),
            Cell::from(count(detail.total)),
        ])];
        rows.push(Row::new(vec![
            Cell::from(format!(
                "{}({})",
                current_labels().domain.errors,
                current_labels().unforced
            )),
            Cell::from(match (detail.errors, detail.unforced_errors) {
                (0, _) => "-".to_string(),
                (e, 0) => format!("{}(-)", e),
                (e, ue) => format!("{}({})", e, ue),
            }),
        ]));
        // the rallies have no player
        if !with_player {
            rows.push(Row::new(vec![
                Cell::from(current_labels().phase_efficiency),
                Cell::from(match detail.phases_per_point {
                    Some((p, won, rallies)) => format!("{:.2}({}/{})", p, won, rallies),
                    _ => "-".to_string(),
                }),
            ]));
        }
        rows.push(Row::new(vec![
            Cell::from(current_labels().domain.faults),
            Cell::from(count(detail.faults)),
        ]));
        if let Some(points) = detail.points {
            rows.push(Row::new(vec![
                Cell::from(current_labels().domain.points),
                Cell::from(count(points)),
            ]));
        }
        let summary_box = Table::new(rows, [Constraint::Length(16), Constraint::Length(5)])
            .block(panel::section(current_labels().summary))
            .widths([Constraint::Percentage(70), Constraint::Percentage(30)]);
        f.render_widget(summary_box, area);
    }

    fn render_center(&mut self, f: &mut Frame, area: Rect) {
        match self.event_filter.selected() {
            Some(EventSelection {
                event_type: EventTypeEnum::FB,
                ..
            }) => self.render_freeball_stats(f, area),
            Some(selection) => self.render_event_stats(f, selection, area),
            None => {}
        }
    }

    /// The freeballs we gave (`FB`), the ones they gave us (`OF`) and how
    /// many of these we exploited. Theirs have no player: with a player
    /// selected, only the ones given.
    fn render_freeball_stats(&self, f: &mut Frame, area: Rect) {
        let Some(Breakdown::Freeballs(freeballs)) = self.breakdown(EventTypeEnum::FB) else {
            return;
        };
        let labels = current_labels();
        let count = |n: u32| match n {
            0 => "-".to_string(),
            n => n.to_string(),
        };
        let given = Row::new(vec![
            Cell::from(labels.domain.freeballs_given),
            Cell::from(count(freeballs.given)),
        ]);
        // theirs have no player
        let received = freeballs.received.map(|received| {
            [
                Row::new(vec![
                    Cell::from(labels.domain.freeballs_received),
                    Cell::from(count(received)),
                ]),
                Row::new(vec![
                    Cell::from(labels.domain.freeballs_exploited),
                    Cell::from(match freeballs.exploited {
                        Some((p, total, exploited)) => {
                            format!("{:.0}%({}/{})", p, exploited, total)
                        }
                        None => "-".to_string(),
                    }),
                ]),
            ]
        });
        let rows: Vec<Row> = once(given).chain(received.into_iter().flatten()).collect();
        let area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(rows.len() as u16 + 2),
                Constraint::Min(0),
            ])
            .split(area)[0];
        let table = Table::new(
            rows,
            [Constraint::Percentage(60), Constraint::Percentage(40)],
        )
        .block(panel::section(labels.domain.freeballs));
        f.render_widget(table, area);
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

/// The color of an evaluation, by how good it is for the fundamental.
fn tone_color(tone: Tone) -> Color {
    match tone {
        Tone::Excellent => Color::Green,
        Tone::Good => Color::LightGreen,
        Tone::Fair => Color::LightYellow,
        Tone::Poor => Color::Yellow,
        Tone::Bad => Color::Red,
    }
}
