//! The statistics while scouting, opened and closed with F3: what the coach
//! asks on the bench, at a glance. The figures of the set being played, or
//! of another set or of the match so far (left and right): the efficiency
//! of each fundamental, the points and the errors (unforced first), the
//! distribution of the setter; below, the trend of the figures set by set.
//! The figures come from the core (`overview`, `trend`), as in the app, and
//! are colored by the band of the level of the team (`benchmark`): red
//! below it, yellow in it, green above it.

use crate::localization::current_labels;
use crate::shapes::keybinding::navigation_keybindings;
use crate::{
    screens::{
        components::{navigation_footer::NavigationFooter, panel},
        screen::{
            AppAction, Renderable, Sba, ScreenAsync, cycle, get_keybinding_actions,
            panel_navigation_entries,
        },
    },
    shapes::{actions::ScreenActionEnum, keybinding::ActionsKeyBindings},
};
use async_trait::async_trait;
use crokey::crossterm::event::KeyEvent;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Cell, Row, Table},
};
use scout4all_core::{
    benchmark::{Measure, grade},
    errors::AppError,
    overview::{
        DISTRIBUTION_ZONES, Errors, Fundamental, Overview, ZoneShare, distribution, stats_of,
    },
    shapes::{
        enums::{EventTypeEnum, FriendlyName, GenderEnum, PhaseEnum, TeamClassificationEnum},
        set::SetEntry,
        stats::{AttackSummary, Stats},
        team::TeamEntry,
    },
    trend::{Replay, Series},
};
use unicode_width::UnicodeWidthStr;

/// The width from which the fundamentals, the points and the distribution
/// stand side by side, the trend below them.
const WIDE: u16 = 100;
/// The rows of the panels on top: the fundamentals (6 and their header) or
/// the points and errors (7), with the borders.
const TOP_HEIGHT: u16 = 9;
/// The rows of the trend: a figure each and their header, with the borders.
const TREND_HEIGHT: u16 = Series::ALL.len() as u16 + 3;
/// The width of a column of the trend.
const TREND_COLUMN: u16 = 7;

/// A set, or the match so far.
struct Scope {
    /// `None` for the match
    set_number: Option<u8>,
    stats: Stats,
    overview: Overview,
}

/// A figure of the trend: at the end of each set, in the match so far and
/// in its last 10 touches.
struct TrendRow {
    series: Series,
    sets: Vec<Option<f64>>,
    overall: Option<f64>,
    recent: Option<f64>,
}

pub struct LiveStatsScreen {
    /// the sets, then the match
    scopes: Vec<Scope>,
    selected: usize,
    trend: Vec<TrendRow>,
    level: Option<TeamClassificationEnum>,
    gender: Option<GenderEnum>,
    footer: NavigationFooter,
    footer_entries: Vec<(String, String)>,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
}

#[async_trait]
impl ScreenAsync for LiveStatsScreen {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        let action = self
            .screen_key_bindings
            .transform(key)
            .and_then(|ck| self.screen_key_bindings.get(ck));
        match action {
            Some(ScreenActionEnum::Back | ScreenActionEnum::LiveStats) => {
                AppAction::Back(false, Some(1))
            }
            Some(ScreenActionEnum::Next) => self.select(true),
            Some(ScreenActionEnum::Previous) => self.select(false),
            _ => AppAction::None,
        }
    }

    async fn refresh_data(&mut self) {}
}

impl Renderable for LiveStatsScreen {
    fn footer_entries(&self) -> Vec<(String, String)> {
        self.footer_entries.clone()
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, _: Rect) {
        match (body.width >= WIDE, self.scopes.get(self.selected)) {
            (_, None) => {}
            (true, Some(scope)) => {
                let [top, bottom, _] = Layout::vertical([
                    Constraint::Length(TOP_HEIGHT),
                    Constraint::Length(TREND_HEIGHT),
                    Constraint::Min(0),
                ])
                .areas(body);
                let [left, middle, right] = Layout::horizontal([
                    Constraint::Percentage(34),
                    Constraint::Percentage(31),
                    Constraint::Fill(1),
                ])
                .areas(top);
                self.render_fundamentals(f, left, scope);
                self.render_points(f, middle, scope);
                self.render_distribution(f, right, scope);
                self.render_trend(f, bottom);
            }
            (false, Some(scope)) => {
                let [top, bottom] =
                    Layout::vertical([Constraint::Length(TOP_HEIGHT), Constraint::Min(0)])
                        .areas(body);
                let halves =
                    || Layout::horizontal([Constraint::Percentage(50), Constraint::Fill(1)]);
                let [left, right] = halves().areas(top);
                let [bottom_left, bottom_right] = halves().areas(bottom);
                self.render_fundamentals(f, left, scope);
                self.render_points(f, right, scope);
                self.render_distribution(f, bottom_left, scope);
                self.render_trend(f, bottom_right);
            }
        }
        self.footer
            .render(f, footer_left, self.footer_entries.clone());
    }
}

impl LiveStatsScreen {
    /// The statistics of the sets played so far, the last one (being
    /// played) shown first.
    pub fn new(
        sets: &[&SetEntry],
        max_substitutions: u8,
        team: &TeamEntry,
    ) -> Result<Self, AppError> {
        let scope = |set_number, sets: &[&SetEntry]| {
            stats_of(sets, max_substitutions).map(|stats| Scope {
                set_number,
                overview: Overview::of(&stats),
                stats,
            })
        };
        let scopes = sets
            .iter()
            .map(|set| scope(Some(set.set_number), &[set]))
            .chain(std::iter::once(scope(None, sets)))
            .collect::<Result<Vec<_>, _>>()?;
        let replay = Replay::new(sets, max_substitutions)?;
        let last = |series, recent| replay.series(series, recent).last().and_then(|p| p.value);
        let trend = Series::ALL
            .iter()
            .map(|series| TrendRow {
                series: *series,
                sets: scopes
                    .iter()
                    .filter(|s| s.set_number.is_some())
                    .map(|s| series.value(&s.stats))
                    .collect(),
                overall: last(*series, false),
                recent: last(*series, true),
            })
            .collect();
        let screen_actions = &[
            Sba::Hidden(ScreenActionEnum::Previous),
            Sba::Hidden(ScreenActionEnum::Next),
            Sba::Hidden(ScreenActionEnum::LiveStats),
            Sba::Simple(ScreenActionEnum::Back),
        ];
        let kb = navigation_keybindings();
        Ok(LiveStatsScreen {
            selected: sets.len().saturating_sub(1),
            scopes,
            trend,
            level: team.classification,
            gender: team.gender,
            footer: NavigationFooter::new(),
            footer_entries: panel_navigation_entries()
                .into_iter()
                .chain(get_keybinding_actions(kb, screen_actions))
                .collect(),
            screen_key_bindings: kb.slice(Sba::keys(screen_actions)),
        })
    }

    fn select(&mut self, forward: bool) -> AppAction {
        self.selected = cycle(self.selected, self.scopes.len(), forward).unwrap_or(0);
        AppAction::None
    }

    /// The name of the screen, with the scope shown between the arrows
    /// choosing another one.
    fn title(&self) -> Line<'static> {
        let labels = current_labels();
        let scope = match self.scopes.get(self.selected).and_then(|s| s.set_number) {
            Some(n) => format!("set {n}"),
            None => labels.match_word.to_string(),
        };
        Line::from(vec![
            Span::raw(format!("{} ◂ ", labels.live_stats)),
            Span::styled(scope, Style::new().add_modifier(Modifier::REVERSED)),
            Span::raw(" ▸"),
        ])
    }

    /// How good the value is for the team, its color: none without the
    /// level and the gender of the team.
    fn graded(&self, measure: Option<Measure>, value: Option<f64>) -> Style {
        let grade = measure
            .zip(value)
            .and_then(|(m, v)| grade(self.level, self.gender, m, v));
        match grade {
            None => Style::new(),
            Some(g) if g < 0.25 => Style::new().fg(Color::Red),
            Some(g) if g > 0.75 => Style::new().fg(Color::Green),
            Some(_) => Style::new().fg(Color::Yellow),
        }
    }

    fn percent_cell(&self, measure: Option<Measure>, value: Option<f64>) -> Cell<'static> {
        Cell::from(percent(value)).style(self.graded(measure, value))
    }

    fn render_fundamentals(&self, f: &mut Frame, area: Rect, scope: &Scope) {
        let labels = current_labels();
        let domain = labels.domain;
        let o = &scope.overview;
        let fundamental = |event: EventTypeEnum, x: &Fundamental| {
            self.row(
                event.friendly_name(domain).to_string(),
                x.total,
                x.perfect,
                x.errors,
                Measure::of(event),
                x.efficiency,
            )
        };
        let attacks = |name: &str, a: &Option<AttackSummary>| {
            let a = a.unwrap_or_default();
            self.row(
                format!("  {name}"),
                a.total,
                a.kills,
                a.errors + a.blocked,
                Some(Measure::Attack),
                (a.total > 0).then(|| a.efficiency()),
            )
        };
        let rows = vec![
            fundamental(EventTypeEnum::P, &o.reception),
            fundamental(EventTypeEnum::A, &o.attack),
            attacks(domain.after_reception, &o.first_attack),
            attacks(domain.counter_attack, &o.counter_attack),
            fundamental(EventTypeEnum::S, &o.serve),
            fundamental(EventTypeEnum::B, &o.block),
        ];
        let header = Row::new(["", domain.tot, "#", domain.err, domain.eff_perc])
            .style(Style::new().add_modifier(Modifier::DIM));
        let table = Table::new(
            rows,
            [
                Constraint::Fill(1),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(5),
            ],
        )
        .header(header)
        .block(panel::screen(self.title()));
        f.render_widget(table, area);
    }

    fn row(
        &self,
        name: String,
        total: u32,
        perfect: u32,
        errors: u32,
        measure: Option<Measure>,
        efficiency: Option<f64>,
    ) -> Row<'static> {
        Row::new([
            Cell::from(name),
            Cell::from(total.to_string()),
            Cell::from(perfect.to_string()),
            Cell::from(errors.to_string()),
            self.percent_cell(measure, efficiency),
        ])
    }

    fn render_points(&self, f: &mut Frame, area: Rect, scope: &Scope) {
        let labels = current_labels();
        let domain = labels.domain;
        let o = &scope.overview;
        let phase = |name: &str, won: Option<(f64, u32, u32)>, measure| {
            let (value, detail) = match won {
                Some((p, won, rallies)) => (Some(p), format!("{won}/{rallies}")),
                None => (None, String::new()),
            };
            Row::new([
                Cell::from(name.to_string()),
                self.percent_cell(Some(measure), value),
                Cell::from(detail),
            ])
        };
        let count = |name: &str, n: u32, detail: String| {
            Row::new([name.to_string(), n.to_string(), detail])
        };
        let errors = |e: &Errors| {
            e.by_event
                .iter()
                .filter(|(_, n)| *n > 0)
                .map(|(event, n)| format!("{} {n}", event.friendly_name(domain)))
                .collect::<Vec<_>>()
                .join(" · ")
        };
        let rows = vec![
            phase(
                PhaseEnum::SideOut.friendly_name(domain),
                o.side_out,
                Measure::SideOut,
            ),
            phase(
                PhaseEnum::Break.friendly_name(domain),
                o.break_points,
                Measure::BreakPoint,
            ),
            count(labels.our_aces, o.aces, String::new()),
            count(labels.their_aces, o.aces_against, String::new()),
            // the errors the coach can do something about, first
            count(
                labels.unforced_errors,
                o.unforced_errors.total,
                errors(&o.unforced_errors),
            )
            .style(Style::new().add_modifier(Modifier::BOLD)),
            count(
                labels.forced_errors,
                o.forced_errors.total,
                errors(&o.forced_errors),
            ),
            count(domain.opponent_errors, o.opponent_errors, String::new()),
        ];
        let name_width = [
            labels.our_aces,
            labels.their_aces,
            labels.unforced_errors,
            labels.forced_errors,
            domain.opponent_errors,
        ]
        .iter()
        .map(|n| n.width())
        .max()
        .unwrap_or(0) as u16;
        let table = Table::new(
            rows,
            [
                Constraint::Length(name_width),
                Constraint::Length(5),
                Constraint::Fill(1),
            ],
        )
        .block(panel::section(labels.points_and_errors));
        f.render_widget(table, area);
    }

    fn render_distribution(&self, f: &mut Frame, area: Rect, scope: &Scope) {
        let labels = current_labels();
        let domain = labels.domain;
        let all = distribution(&scope.stats, None);
        let on_reception = distribution(&scope.stats, Some(false));
        let counter_attack = distribution(&scope.stats, Some(true));
        let share = |shares: &[ZoneShare], zone| {
            shares
                .iter()
                .find(|z| z.zone == zone)
                .map(|z| percent(Some(z.share)))
                .unwrap_or_else(|| "-".to_string())
        };
        let rows = DISTRIBUTION_ZONES.iter().map(|zone| {
            let attacks = all.iter().find(|z| z.zone == *zone).map(|z| z.attacks);
            Row::new([
                Cell::from(zone.to_string()),
                Cell::from(share(&all, *zone)),
                Cell::from(attacks.map_or(0, |a| a.total).to_string()),
                self.percent_cell(Some(Measure::Attack), attacks.map(|a| a.efficiency())),
                Cell::from(share(&on_reception, *zone)),
                Cell::from(share(&counter_attack, *zone)),
            ])
        });
        let titles = [
            labels.zone,
            "%",
            domain.tot,
            domain.eff_perc,
            labels.reception_share,
            labels.counter_attack_share,
        ];
        // the values take up to 5 columns ("-100%"), the titles can take
        // more: "ゾーン" takes 6
        let widths = titles
            .iter()
            .zip([4, 5, 4, 5, 4, 4])
            .map(|(title, values)| Constraint::Length((title.width() as u16).max(values)));
        let header = Row::new(titles).style(Style::new().add_modifier(Modifier::DIM));
        let table = Table::new(rows, widths)
            .header(header)
            .block(panel::section(domain.distribution));
        f.render_widget(table, area);
    }

    fn render_trend(&self, f: &mut Frame, area: Rect) {
        let labels = current_labels();
        let all: Vec<u8> = self.scopes.iter().filter_map(|s| s.set_number).collect();
        let name_width = Series::ALL
            .iter()
            .map(|s| series_name(*s).width())
            .max()
            .unwrap_or(0) as u16;
        let skipped = all.len() - trend_sets_shown(area.width, name_width, all.len());
        let sets: Vec<&u8> = all.iter().skip(skipped).collect();
        let header = std::iter::once(String::new())
            .chain(sets.iter().map(|n| format!("set {n}")))
            .chain([
                labels.match_word.to_string(),
                labels.last_touches.to_string(),
            ]);
        let rows = self.trend.iter().map(|row| {
            let measure = Some(row.series.measure());
            Row::new(
                std::iter::once(Cell::from(series_name(row.series)))
                    .chain(
                        row.sets
                            .iter()
                            .skip(skipped)
                            .map(|v| self.percent_cell(measure, *v)),
                    )
                    .chain([
                        Cell::from(percent(row.overall)).style(
                            self.graded(measure, row.overall)
                                .add_modifier(Modifier::BOLD),
                        ),
                        self.percent_cell(measure, row.recent),
                    ]),
            )
        });
        let widths = std::iter::once(Constraint::Length(name_width))
            .chain(sets.iter().map(|_| Constraint::Length(TREND_COLUMN)))
            .chain([
                Constraint::Length(TREND_COLUMN + 2),
                Constraint::Length(TREND_COLUMN + 2),
            ]);
        let block = panel::section(labels.trend);
        let block = match self.level.zip(self.gender) {
            Some(_) => block,
            None => block.title_bottom(
                Line::from(labels.set_team_level).style(Style::new().add_modifier(Modifier::DIM)),
            ),
        };
        let table = Table::new(rows, widths)
            .header(Row::new(header).style(Style::new().add_modifier(Modifier::DIM)))
            .block(block);
        f.render_widget(table, area);
    }
}

/// How many sets the trend has room for, the latest ones: the match and
/// the last touches are always shown, after the names of the figures.
fn trend_sets_shown(width: u16, name_width: u16, sets: usize) -> usize {
    // the borders, and each column with the space after it
    let room = width.saturating_sub(2 + name_width + 1 + 2 * (TREND_COLUMN + 3));
    usize::from(room / (TREND_COLUMN + 1)).min(sets)
}

fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_string(), |v| format!("{v:.0}%"))
}

fn series_name(series: Series) -> String {
    let domain = current_labels().domain;
    match series {
        Series::Reception => EventTypeEnum::P.friendly_name(domain),
        Series::Attack => EventTypeEnum::A.friendly_name(domain),
        Series::CounterAttack => domain.counter_attack,
        Series::Serve => EventTypeEnum::S.friendly_name(domain),
        Series::SideOut => PhaseEnum::SideOut.friendly_name(domain),
        Series::BreakPoint => PhaseEnum::Break.friendly_name(domain),
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The latest sets the trend has room for: none on a narrow screen,
    /// all of them on a wide one.
    #[test]
    fn the_sets_the_trend_has_room_for() {
        // names 14 wide: 2 + 15 + 2 * 10 = 37 before the sets, 8 a set
        assert_eq!(trend_sets_shown(37, 14, 5), 0);
        assert_eq!(trend_sets_shown(44, 14, 5), 0);
        assert_eq!(trend_sets_shown(45, 14, 5), 1);
        assert_eq!(trend_sets_shown(77, 14, 5), 5);
        assert_eq!(trend_sets_shown(200, 14, 3), 3);
        assert_eq!(trend_sets_shown(10, 14, 3), 0);
    }
}
