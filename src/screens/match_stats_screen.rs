use crate::localization::current_labels;
use crate::shapes::keybinding::navigation_keybindings;
use crate::{
    screens::{
        components::{
            navigation_footer::NavigationFooter, notify_banner::NotifyBanner, panel, target_court,
        },
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
    text::{Line, Span},
    widgets::{
        Bar, BarChart, BarGroup, Cell, Gauge, List, ListItem, ListState, Padding, Row, Table,
        canvas::Canvas,
    },
};
use scout4all_core::{
    detail::{Breakdown, Detail, TargetDetail, Tone},
    errors::AppError,
    overview::side_out_possessions,
    shapes::{
        court_zone::{CourtSide, CourtZoneEnum, TargetKind},
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
        let Some(Breakdown::Fundamental(detail)) = self.breakdown(selection.event_type) else {
            return;
        };
        // how many blocked, under the rest, when told
        let blockers = match detail.blockers.len() {
            0 => 0,
            n => u16::try_from(n).unwrap_or(0) + 3,
        };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(20),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(blockers),
            ])
            .split(area);
        let with_player = self.player_filter.selected().is_some();
        Self::render_evals_bars(f, &detail, chunks[0]);
        Self::render_summary_table(f, &detail, with_player, chunks[1]);
        Self::render_efficiency_bars(f, &detail, chunks[2]);
        Self::render_positiveness_bars(f, &detail, chunks[3]);
        Self::render_blockers(f, &detail, selection.event_type, chunks[4]);
    }

    /// Our attacks by how many of theirs blocked, or our blocks by how many
    /// of ours, of the ones told: how many, their share and, from a few on,
    /// their efficiency.
    fn render_blockers(f: &mut Frame, detail: &Detail, event_type: EventTypeEnum, area: Rect) {
        if detail.blockers.is_empty() {
            return;
        }
        let labels = current_labels();
        let domain = labels.domain;
        let title = match event_type {
            EventTypeEnum::B => domain.blocks_by_blockers,
            _ => domain.attacks_by_block,
        };
        let told = domain
            .blockers_told
            .replace("{told}", &detail.blockers_told.to_string())
            .replace("{total}", &detail.total.to_string());
        let rows = detail.blockers.iter().map(|b| {
            let shown = b.count > 0;
            Row::new(vec![
                Cell::from(b.blockers.name(domain)),
                Cell::from(match shown {
                    true => b.count.to_string(),
                    false => "-".to_string(),
                }),
                Cell::from(match shown {
                    true => format!("{:.0}%", b.share),
                    false => String::new(),
                }),
                Cell::from(b.efficiency.map_or(String::new(), |e| format!("{e:+.0}%"))),
            ])
            .style(match shown {
                true => Style::default(),
                false => Style::default().add_modifier(Modifier::DIM),
            })
        });
        let table = Table::new(
            rows,
            [
                Constraint::Fill(1),
                Constraint::Length(4),
                Constraint::Length(5),
                Constraint::Length(6),
            ],
        )
        .block(panel::section(title).title_bottom(told));
        f.render_widget(table, area);
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
            // with the zones of their court told, the distribution on the
            // left and the zones on the right
            let told = matches!(
                self.breakdown(EventTypeEnum::A),
                Some(Breakdown::Fundamental(Detail {
                    targets_told: 1..,
                    ..
                }))
            );
            let halves = |area: Rect| {
                Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(area)
            };
            let (top, bottom) = match told {
                true => {
                    let (top, bottom) = (halves(chunks[0]), halves(chunks[1]));
                    self.render_targets(EventTypeEnum::A, f, top[1], bottom[1]);
                    (top[0], bottom[0])
                }
                false => (chunks[0], chunks[1]),
            };
            self.render_court_canvas(f, top, current_labels().domain.distribution, |(p, _)| p);
            self.render_court_canvas(f, bottom, current_labels().conversion_rate, |(_, s)| s);
        }
        match (
            self.event_filter.selected().map(|s| s.event_type),
            self.player_filter.selected(),
        ) {
            (Some(event_type @ (EventTypeEnum::S | EventTypeEnum::D | EventTypeEnum::B)), _) => {
                self.render_targets(event_type, f, chunks[0], chunks[1]);
            }
            // the side outs have no player: with one, the zones take the
            // room
            (Some(EventTypeEnum::P), None) => {
                // the side outs a few lines, the zones under them
                let parts = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(6),
                        Constraint::Fill(1),
                        Constraint::Fill(1),
                    ])
                    .split(area);
                self.render_side_out_possessions(f, parts[0]);
                self.render_targets(EventTypeEnum::P, f, parts[1], parts[2]);
            }
            (Some(EventTypeEnum::P), Some(_)) => {
                self.render_targets(EventTypeEnum::P, f, chunks[0], chunks[1]);
            }
            _ => {}
        }
    }

    /// After the reception, the side outs by possession: the first, the
    /// second and the later ones, narrowed by the set, the rotation and the
    /// phase. They have no player: not with a player selected.
    fn render_side_out_possessions(&self, f: &mut Frame, area: Rect) {
        let Some(stats) = self.get_current_stats(self.set_filter.selected().copied()) else {
            return;
        };
        let labels = current_labels();
        let domain = labels.domain;
        let filter = self.current_filter();
        let rows: Vec<Row> = side_out_possessions(&stats, &filter)
            .iter()
            .zip(domain.possessions)
            .map(|((_, summary), name)| {
                Row::new(vec![
                    Cell::from(name),
                    Cell::from(summary.map_or("-".to_string(), |s| s.total.to_string())),
                    Cell::from(summary.map_or("-".to_string(), |s| s.won.to_string())),
                    Cell::from(
                        summary.map_or("-".to_string(), |s| format!("{:.0}%", s.finalization())),
                    ),
                ])
            })
            .collect();
        let header = Row::new(vec!["", domain.tot, domain.pt, domain.pt_perc])
            .style(Style::new().add_modifier(Modifier::DIM));
        let area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(rows.len() as u16 + 3),
                Constraint::Min(0),
            ])
            .split(area)[0];
        let table = Table::new(
            rows,
            [
                Constraint::Fill(1),
                Constraint::Length(5),
                Constraint::Length(5),
                Constraint::Length(5),
            ],
        )
        .header(header)
        .block(panel::section(domain.sideout_by_possession));
        f.render_widget(table, area);
    }

    /// Where our serves or attacks went (on their court), or where we
    /// received, dug or blocked theirs (on ours), of the ones whose zone was told: how many
    /// in each zone, in a shade of one color, the more the brighter; and
    /// their efficiency there, red to green, only from a few on. For the
    /// reception, the conflict zones between the zones, and all of them in
    /// the title.
    fn render_targets(
        &self,
        event_type: EventTypeEnum,
        f: &mut Frame,
        density: Rect,
        efficiency: Rect,
    ) {
        let (Some(Breakdown::Fundamental(detail)), Some(side)) =
            (self.breakdown(event_type), CourtSide::of(event_type))
        else {
            return;
        };
        let domain = current_labels().domain;
        let by_zone = match event_type {
            EventTypeEnum::P => domain.receptions_by_zone,
            EventTypeEnum::A => domain.attacks_by_zone,
            EventTypeEnum::D => domain.digs_by_zone,
            EventTypeEnum::B => domain.blocks_by_zone,
            _ => domain.serves_by_zone,
        };
        let told = domain
            .zones_told
            .replace("{told}", &detail.targets_told.to_string())
            .replace("{total}", &detail.total.to_string());
        // how many told, and all the conflict zones together, at the
        // bottom of the panels
        let (on_density, on_efficiency) = match detail.on_conflicts {
            Some(on) => {
                let conflicts = domain
                    .on_conflicts
                    .replace("{count}", &on.count.to_string())
                    .replace("{share}", &format!("{:.0}", on.share));
                let efficiency = match on.efficiency {
                    Some(e) => format!("{conflicts} {e:+.0}%"),
                    None => conflicts.clone(),
                };
                (format!("{told} · {conflicts}"), efficiency)
            }
            None => (told, String::new()),
        };
        // the zones at the net for the block, the conflict zones between
        // the zones in reception and in defense
        let rows = match (TargetKind::of(event_type), detail.conflicts.is_empty()) {
            (Some(TargetKind::Net), _) => vec![CourtZoneEnum::net_shown(side)],
            (_, false) => CourtZoneEnum::shown(side),
            (_, true) => CourtZoneEnum::zones_shown(side),
        };
        let target = |zone: CourtZoneEnum| {
            detail
                .targets
                .iter()
                .chain(detail.conflicts.iter())
                .find(|t| t.zone == zone)
                .copied()
        };
        let most = detail
            .targets
            .iter()
            .chain(detail.conflicts.iter())
            .map(|t| t.count)
            .fold(0.0, f64::max);
        target_court::render_court(
            f,
            density,
            panel::section(by_zone).title_bottom(on_density),
            &rows,
            false,
            |zone| {
                let shown = target(zone).filter(|t| t.count > 0.0);
                let lines = match shown {
                    Some(t) => vec![
                        Line::from(zone.to_string()),
                        Line::from(format!("{:.0}%", t.share)),
                        Line::from(format!("({})", count(t.count))),
                    ],
                    None => vec![Line::from(zone.to_string())],
                };
                (lines, density_style(zone, shown, most))
            },
        );
        target_court::render_court(
            f,
            efficiency,
            panel::section(domain.efficiency_by_zone).title_bottom(on_efficiency),
            &rows,
            false,
            |zone| {
                let lines = match target(zone).filter(|t| t.count > 0.0) {
                    Some(TargetDetail {
                        efficiency: Some(e),
                        count: n,
                        ..
                    }) => vec![
                        Line::from(zone.to_string()),
                        Line::from(format!("{e:+.0}%")),
                        Line::from(format!("n={}", count(n))),
                    ],
                    // too few to tell
                    Some(t) => vec![
                        Line::from(zone.to_string()),
                        Line::from(format!("({})", count(t.count))),
                    ],
                    None => vec![Line::from(zone.to_string())],
                };
                let style = efficiency_style(zone, target(zone).and_then(|t| t.efficiency));
                (lines, style)
            },
        );
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
    /// many of these we exploited. Team events: with a player selected,
    /// none given and the others not shown.
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
        // team events
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

/// The shades of the serves to a zone, from few to the most: of amber,
/// darker to brighter (the colors of 256, as terminals show them all).
const DENSITY_SHADES: [u8; 4] = [94, 136, 172, 214];

/// A zone of the serves: none, or a shade by how many went there, the most
/// the brightest.
fn density_style(zone: CourtZoneEnum, target: Option<TargetDetail>, most: f64) -> Style {
    let shade = target
        .filter(|_| most > 0.0)
        .map(|t| (t.count * DENSITY_SHADES.len() as f64 / most).ceil() as usize)
        .and_then(|step| DENSITY_SHADES.get(step.saturating_sub(1)));
    match shade {
        Some(&color) => Style::default()
            .bg(Color::Indexed(color))
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
        None => empty_zone(zone),
    }
}

/// How many in a zone: whole, or with the half of two cells of different
/// zones ("2.5").
fn count(count: f64) -> String {
    match count.fract() == 0.0 {
        true => format!("{count:.0}"),
        false => format!("{count:.1}"),
    }
}

/// The efficiency of a zone, red when negative, green when positive, the
/// more the brighter; gray about zero; none without it.
fn efficiency_style(zone: CourtZoneEnum, efficiency: Option<f64>) -> Style {
    let color = efficiency.map(|e| match e {
        e if e <= -50.0 => 160,
        e if e <= -10.0 => 131,
        e if e < 10.0 => 245,
        e if e < 50.0 => 71,
        _ => 34,
    });
    match color {
        Some(color) => Style::default()
            .bg(Color::Indexed(color))
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
        None => empty_zone(zone),
    }
}

/// A zone with nothing to show: gray, to see the court, the conflict zones
/// darker.
fn empty_zone(zone: CourtZoneEnum) -> Style {
    let background = match zone.is_zone() {
        true => 238,
        false => 236,
    };
    Style::default()
        .bg(Color::Indexed(background))
        .fg(Color::Gray)
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
