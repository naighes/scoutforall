//! The statistics of a match, the pages of the app side by side: the
//! summary (fundamentals, phases, rotations, the distribution, the errors),
//! the trend of every set, and a fundamental in detail. ⇧Tab Tab between
//! the pages, ↑ ↓ between the filters, ← → change the filter, as in a form.

use {
    crate::{
        charts::{self, Trend},
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL, grade_color},
        ui,
    },
    iced::{
        Alignment, Color, Element, Length,
        widget::{Column, Row, column, container, row, scrollable, space, text},
    },
    scout4all_core::{
        detail::Tone,
        shapes::enums::{EventTypeEnum, PhaseEnum},
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{
        DetailFilter, DetailStats, ErrorCount, FundamentalDetail, LiveStats, MobileError,
        PhaseStats, Scout4All, StatsScope, TargetStats,
    },
    std::sync::Arc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Summary,
    Trend,
    Detail,
}

const PAGES: [Page; 3] = [Page::Summary, Page::Trend, Page::Detail];

/// What ← → change on a page, a row each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    Scope,
    Series,
    Mode,
    Event,
    Player,
    Rotation,
    Phase,
    Zones,
}

#[derive(Debug, Clone)]
pub enum Message {
    Page(usize),
    Scope(StatsScope),
    Series(usize),
    Recent(bool),
    Event(EventTypeEnum),
    Player(Option<String>),
    Rotation(Option<u8>),
    Phase(Option<PhaseEnum>),
    Efficiency(bool),
}

pub enum Outcome {
    Stay,
    Back,
}

pub struct StatsScreen {
    core: Arc<Scout4All>,
    pub team_id: String,
    pub match_id: String,
    title: String,
    page: usize,
    scope: StatsScope,
    live: Option<LiveStats>,
    detail: Option<DetailStats>,
    series: usize,
    recent: bool,
    trend: Trend,
    event: EventTypeEnum,
    filter: DetailFilter,
    /// the zones of the detail by efficiency, not by how many
    efficiency: bool,
    /// the filter the arrows change, on the page
    row: usize,
    error: Option<String>,
}

/// The option after (or before) `current`, round.
fn cycle<T: PartialEq + Clone>(options: &[T], current: &T, forward: bool) -> Option<T> {
    let count = options.len();
    let at = options.iter().position(|o| o == current);
    let next = match (at, forward) {
        (_, _) if count == 0 => return None,
        (Some(i), true) => (i + 1) % count,
        (Some(i), false) => (i + count - 1) % count,
        (None, _) => 0,
    };
    options.get(next).cloned()
}

fn pct(v: Option<f64>) -> String {
    v.map_or_else(|| "–".to_string(), |v| format!("{v:.0}%"))
}

fn small<'a>(s: impl ToString, c: Colors) -> iced::widget::Text<'a> {
    text(s.to_string()).size(SIZE_SMALL).color(c.muted)
}

fn big<'a>(s: impl ToString, color: Color) -> iced::widget::Text<'a> {
    text(s.to_string())
        .size(22)
        .font(theme::fonts().bold)
        .color(color)
}

fn tone_grade(tone: &Tone) -> f64 {
    match tone {
        Tone::Excellent => 1.0,
        Tone::Good => 0.75,
        Tone::Fair => 0.5,
        Tone::Poor => 0.25,
        Tone::Bad => 0.0,
    }
}

/// A bar `share` of `most` long.
fn bar<'a, M: 'a>(share: f64, most: f64, color: Color) -> Element<'a, M> {
    let filled = ((share / most.max(f64::EPSILON)).clamp(0.0, 1.0) * 100.0).round() as u16;
    let full = container(space::horizontal())
        .width(Length::FillPortion(filled.max(1)))
        .height(10)
        .style(move |_| container::Style {
            background: Some(color.into()),
            border: iced::border::rounded(5),
            ..container::Style::default()
        });
    row![
        full,
        space::horizontal().width(Length::FillPortion(100_u16.saturating_sub(filled).max(1)))
    ]
    .width(Length::Fill)
    .into()
}

/// A figure in a box: its name, its value, a line under it.
fn tile<'a, M: 'a>(
    label: &str,
    value: String,
    color: Color,
    detail: Option<String>,
    c: Colors,
) -> Element<'a, M> {
    let detail: Element<'a, M> = match detail {
        Some(d) => small(d, c).into(),
        None => space::vertical().height(0).into(),
    };
    container(column![small(label, c), big(value, color), detail].spacing(2))
        .padding(10)
        .width(Length::Fill)
        .style(move |_| container::Style {
            border: iced::border::rounded(8).color(c.border).width(1),
            ..container::Style::default()
        })
        .into()
}

impl StatsScreen {
    pub fn new(
        core: Arc<Scout4All>,
        team_id: String,
        match_id: String,
    ) -> Result<StatsScreen, MobileError> {
        let team = core.team(team_id.clone())?.name;
        let opponent = core
            .match_details(team_id.clone(), match_id.clone())?
            .opponent;
        let live = core.match_stats(team_id.clone(), match_id.clone(), StatsScope::Match)?;
        let mut screen = StatsScreen {
            core,
            team_id,
            match_id,
            title: format!("{team} – {opponent}"),
            page: 0,
            scope: StatsScope::Match,
            live: Some(live),
            detail: None,
            series: 0,
            recent: false,
            trend: Trend::of_stats(None, 0, false),
            event: EventTypeEnum::S,
            filter: DetailFilter::default(),
            efficiency: false,
            row: 0,
            error: None,
        };
        screen.load();
        Ok(screen)
    }

    pub fn title(&self) -> String {
        self.title.clone()
    }

    /// Reads the statistics again, for the scope and the filter chosen.
    fn load(&mut self) {
        let (team, m) = (self.team_id.clone(), self.match_id.clone());
        let live = self.core.match_stats(team.clone(), m.clone(), self.scope);
        let detail = self
            .core
            .match_detail(team, m, self.scope, self.event, self.filter.clone());
        self.error = None;
        match live {
            Ok(live) => self.live = Some(live),
            Err(e) => self.error = Some(e.to_string()),
        }
        match detail {
            Ok(detail) => self.detail = Some(detail),
            Err(e) => self.error = Some(e.to_string()),
        }
        self.retrend();
    }

    fn retrend(&mut self) {
        self.trend = Trend::of_stats(self.live.as_ref(), self.series, self.recent);
    }

    fn current(&self) -> Page {
        PAGES.get(self.page).copied().unwrap_or(Page::Summary)
    }

    fn filters(&self) -> Vec<Filter> {
        let zones = self
            .detail
            .as_ref()
            .and_then(|d| d.fundamental.as_ref())
            .is_some_and(|f| !f.targets.is_empty());
        match (self.current(), zones) {
            (Page::Summary, _) => vec![Filter::Scope],
            (Page::Trend, _) => vec![Filter::Series, Filter::Mode],
            (Page::Detail, false) => vec![
                Filter::Scope,
                Filter::Event,
                Filter::Player,
                Filter::Rotation,
                Filter::Phase,
            ],
            (Page::Detail, true) => vec![
                Filter::Scope,
                Filter::Event,
                Filter::Player,
                Filter::Rotation,
                Filter::Phase,
                Filter::Zones,
            ],
        }
    }

    fn scopes(&self) -> Vec<StatsScope> {
        self.live
            .as_ref()
            .map(|l| l.sets.clone())
            .unwrap_or_default()
            .into_iter()
            .map(|number| StatsScope::Set { number })
            .chain([StatsScope::Match])
            .collect()
    }

    /// The message ← (or →) sends on a filter.
    fn changed(&self, filter: Filter, forward: bool) -> Option<Message> {
        let detail = self.detail.as_ref();
        match filter {
            Filter::Scope => cycle(&self.scopes(), &self.scope, forward).map(Message::Scope),
            Filter::Series => {
                let count = self.live.as_ref().map_or(0, |l| l.trend.len());
                let series: Vec<usize> = (0..count).collect();
                cycle(&series, &self.series, forward).map(Message::Series)
            }
            Filter::Mode => Some(Message::Recent(!self.recent)),
            Filter::Zones => Some(Message::Efficiency(!self.efficiency)),
            Filter::Event => {
                let events: Vec<EventTypeEnum> = detail
                    .map(|d| d.fundamentals.iter().map(|f| f.value).collect())
                    .unwrap_or_default();
                cycle(&events, &self.event, forward).map(Message::Event)
            }
            Filter::Player => {
                let players: Vec<Option<String>> = [None]
                    .into_iter()
                    .chain(
                        detail
                            .into_iter()
                            .flat_map(|d| d.players.iter().map(|p| Some(p.id.clone()))),
                    )
                    .collect();
                cycle(&players, &self.filter.player, forward).map(Message::Player)
            }
            Filter::Rotation => {
                let rotations: Vec<Option<u8>> = [None]
                    .into_iter()
                    .chain(
                        detail
                            .into_iter()
                            .flat_map(|d| d.rotations.iter().map(|r| Some(r.value))),
                    )
                    .collect();
                cycle(&rotations, &self.filter.rotation, forward).map(Message::Rotation)
            }
            Filter::Phase => {
                let phases: Vec<Option<PhaseEnum>> = [None]
                    .into_iter()
                    .chain(
                        detail
                            .into_iter()
                            .flat_map(|d| d.phases.iter().map(|p| Some(p.value))),
                    )
                    .collect();
                cycle(&phases, &self.filter.phase, forward).map(Message::Phase)
            }
        }
    }

    pub fn update(&mut self, message: Message) -> Outcome {
        match message {
            Message::Page(page) => {
                self.page = page.min(PAGES.len() - 1);
                self.row = 0;
            }
            Message::Scope(scope) => {
                self.scope = scope;
                self.load();
            }
            Message::Series(series) => {
                self.series = series;
                self.retrend();
            }
            Message::Recent(recent) => {
                self.recent = recent;
                self.retrend();
            }
            Message::Event(event) => {
                self.event = event;
                self.load();
            }
            Message::Player(player) => {
                self.filter.player = player;
                self.load();
            }
            Message::Rotation(rotation) => {
                self.filter.rotation = rotation;
                self.load();
            }
            Message::Phase(phase) => {
                self.filter.phase = phase;
                self.load();
            }
            Message::Efficiency(efficiency) => self.efficiency = efficiency,
        }
        let rows = self.filters().len();
        self.row = self.row.min(rows.saturating_sub(1));
        Outcome::Stay
    }

    pub fn key(&mut self, ck: KeyCombination) -> Outcome {
        let rows = self.filters().len().max(1);
        let pages = PAGES.len();
        let previous_page =
            ck == key!(shift - tab) || ck == key!(shift - backtab) || ck == key!(backtab);
        match ck {
            k if k == BACK_KEY => Outcome::Back,
            k if k == key!(tab) => self.update(Message::Page((self.page + 1) % pages)),
            _ if previous_page => self.update(Message::Page((self.page + pages - 1) % pages)),
            k if k == key!(down) => {
                self.row = (self.row + 1) % rows;
                Outcome::Stay
            }
            k if k == key!(up) => {
                self.row = (self.row + rows - 1) % rows;
                Outcome::Stay
            }
            k if k == key!(left) || k == key!(right) => {
                let changed = self
                    .filters()
                    .get(self.row)
                    .and_then(|f| self.changed(*f, k == key!(right)));
                match changed {
                    Some(message) => self.update(message),
                    None => Outcome::Stay,
                }
            }
            _ => Outcome::Stay,
        }
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        vec![
            ("⇧⇥ ⇥".to_string(), s.page.to_string()),
            ("↑ ↓".to_string(), s.filter.to_string()),
            ("← →".to_string(), s.change.to_string()),
            ("Esc".to_string(), s.back.to_string()),
        ]
    }

    // --- view ------------------------------------------------------------

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let names = [s.summary, s.trend, s.detail];
        let tabs = row(names
            .iter()
            .enumerate()
            .map(|(i, name)| ui::pill(name, i == self.page, Some(Message::Page(i)), c)))
        .spacing(6);
        let filters = Column::with_children(
            self.filters()
                .into_iter()
                .enumerate()
                .map(|(i, f)| self.filter_row(f, i == self.row, s, c)),
        )
        .spacing(6);
        let error: Element<'a, Message> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => space::vertical().height(0).into(),
        };
        let page: Element<'a, Message> = match (self.current(), &self.live) {
            (_, None) => space::vertical().into(),
            (Page::Summary, Some(live)) => summary(live, s, c),
            (Page::Trend, Some(live)) => self.trend_page(live, s, c),
            (Page::Detail, _) => self.detail_page(s, c),
        };
        column![
            tabs,
            ui::panel(None, filters, c),
            error,
            container(page).height(Length::Fill)
        ]
        .spacing(12)
        .into()
    }

    /// A filter: its name, then a pill each choice; the row the arrows are
    /// on lit.
    fn filter_row<'a>(
        &'a self,
        filter: Filter,
        on: bool,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let pill = |label: String, chosen: bool, message: Message| {
            ui::pill(&label, chosen, Some(message), c)
        };
        let all = |chosen: bool, message: Message| pill(s.filter_all.to_string(), chosen, message);
        let detail = self.detail.as_ref();
        let (name, pills): (&str, Vec<Element<'a, Message>>) = match filter {
            Filter::Scope => (
                s.sets,
                self.scopes()
                    .into_iter()
                    .map(|scope| {
                        let label = match scope {
                            StatsScope::Set { number } => format!("{} {number}", s.set_n),
                            StatsScope::Match => s.whole_match.to_string(),
                        };
                        pill(label, scope == self.scope, Message::Scope(scope))
                    })
                    .collect(),
            ),
            Filter::Series => (
                s.series,
                self.live
                    .iter()
                    .flat_map(|l| l.trend.iter().enumerate())
                    .map(|(i, line)| {
                        pill(
                            ui::capitalized(&line.name),
                            i == self.series,
                            Message::Series(i),
                        )
                    })
                    .collect(),
            ),
            Filter::Mode => (
                s.mode,
                vec![
                    pill(s.so_far.to_string(), !self.recent, Message::Recent(false)),
                    pill(
                        s.last_touches.to_string(),
                        self.recent,
                        Message::Recent(true),
                    ),
                ],
            ),
            Filter::Zones => (
                s.zones_view,
                vec![
                    pill(
                        s.zone_density.to_string(),
                        !self.efficiency,
                        Message::Efficiency(false),
                    ),
                    pill(
                        ui::capitalized(s.efficiency),
                        self.efficiency,
                        Message::Efficiency(true),
                    ),
                ],
            ),
            Filter::Event => (
                s.fundamental,
                detail
                    .iter()
                    .flat_map(|d| d.fundamentals.iter())
                    .map(|f| {
                        pill(
                            ui::capitalized(&f.name),
                            f.value == self.event,
                            Message::Event(f.value),
                        )
                    })
                    .collect(),
            ),
            Filter::Player => (
                s.player,
                [all(self.filter.player.is_none(), Message::Player(None))]
                    .into_iter()
                    .chain(detail.iter().flat_map(|d| d.players.iter()).map(|p| {
                        pill(
                            format!("{} {}", p.number, p.name),
                            self.filter.player.as_ref() == Some(&p.id),
                            Message::Player(Some(p.id.clone())),
                        )
                    }))
                    .collect(),
            ),
            Filter::Rotation => (
                s.rotation,
                [all(self.filter.rotation.is_none(), Message::Rotation(None))]
                    .into_iter()
                    .chain(detail.iter().flat_map(|d| d.rotations.iter()).map(|r| {
                        pill(
                            r.name.clone(),
                            self.filter.rotation == Some(r.value),
                            Message::Rotation(Some(r.value)),
                        )
                    }))
                    .collect(),
            ),
            Filter::Phase => (
                s.phase,
                [all(self.filter.phase.is_none(), Message::Phase(None))]
                    .into_iter()
                    .chain(detail.iter().flat_map(|d| d.phases.iter()).map(|p| {
                        pill(
                            ui::capitalized(&p.name),
                            self.filter.phase == Some(p.value),
                            Message::Phase(Some(p.value)),
                        )
                    }))
                    .collect(),
            ),
        };
        let label = text(name.to_string())
            .size(SIZE_SMALL)
            .font(theme::fonts().bold)
            .color(match on {
                true => c.accent,
                false => c.muted,
            })
            .width(Length::Fixed(130.0));
        row![
            text(match on {
                true => "▸",
                false => " ",
            })
            .size(SIZE_SMALL)
            .color(c.accent)
            .width(Length::Fixed(12.0)),
            label,
            Row::with_children(pills)
                .spacing(4)
                .wrap()
                .vertical_spacing(4)
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    }

    fn trend_page<'a>(
        &'a self,
        live: &'a LiveStats,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let legend = row(live
            .trend
            .first()
            .map(|l| l.sets.iter().map(|s| s.set_number).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .map(|n| {
                text(format!("━ {} {n}", s.set_n))
                    .size(SIZE_SMALL)
                    .color(theme::set_color(n))
                    .into()
            }))
        .push(small(format!("- - {}", s.whole_match), c))
        .spacing(14);
        let chart: Element<'a, Message> = match self.trend.is_empty() {
            true => small(s.no_rallies, c).into(),
            false => charts::canvas_view(self.trend.clone(), c, Length::Fill),
        };
        let name = live
            .trend
            .get(self.series)
            .map_or(s.trend.to_string(), |l| ui::capitalized(&l.name));
        ui::panel(Some(&name), column![legend, chart].spacing(10), c)
    }

    fn detail_page<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let Some(detail) = &self.detail else {
            return space::vertical().into();
        };
        match (&detail.fundamental, &detail.freeballs) {
            (Some(f), _) if f.total == 0 => ui::panel(None, small(s.no_events, c), c),
            (Some(f), _) => self.fundamental(f, s, c),
            (None, Some(fb)) => {
                let exploited = fb.exploited.as_ref();
                row![
                    tile(s.freeballs_given, fb.given.to_string(), c.text, None, c),
                    tile(
                        s.freeballs_received,
                        fb.received.map_or("–".to_string(), |r| r.to_string()),
                        c.text,
                        None,
                        c
                    ),
                    tile(
                        s.freeballs_exploited,
                        pct(exploited.map(|e| e.percentage)),
                        c.text,
                        exploited.map(|e| {
                            ui::fill(s.of_total, &[e.exploited.to_string(), e.total.to_string()])
                        }),
                        c
                    ),
                ]
                .spacing(10)
                .into()
            }
            (None, None) => space::vertical().into(),
        }
    }

    /// How the evaluations went and the figures; the zones; the players.
    fn fundamental<'a>(
        &'a self,
        f: &'a FundamentalDetail,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let most = f.evals.iter().map(|e| e.share).fold(1.0, f64::max);
        let evals = f.evals.iter().map(|e| {
            let color = grade_color(tone_grade(&e.tone), c);
            row![
                text(e.symbol.clone())
                    .size(SIZE_BODY)
                    .font(theme::fonts().bold)
                    .color(color)
                    .width(Length::Fixed(20.0)),
                ui::body(ui::capitalized(&e.name)).width(Length::Fixed(150.0)),
                bar(e.share, most, color),
                ui::body(e.count)
                    .width(Length::Fixed(36.0))
                    .align_x(Alignment::End),
                small(format!("{:.0}%", e.share), c)
                    .width(Length::Fixed(44.0))
                    .align_x(Alignment::End),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        });
        let grade = f.grade.map_or(c.text, |g| grade_color(g, c));
        let figures = column![
            row![
                tile(
                    &ui::capitalized(s.efficiency),
                    pct(f.efficiency),
                    grade,
                    None,
                    c
                ),
                tile(s.positiveness, pct(f.positiveness), c.text, None, c),
                tile(s.total, f.total.to_string(), c.text, None, c),
            ]
            .spacing(8),
            row![
                tile(
                    s.errors,
                    f.errors.to_string(),
                    match f.unforced_errors > 0 {
                        true => c.danger,
                        false => c.text,
                    },
                    Some(ui::fill(
                        s.unforced_detail,
                        &[f.unforced_errors.to_string()]
                    )),
                    c
                ),
                tile(
                    s.points,
                    f.points.map_or("–".to_string(), |p| p.to_string()),
                    c.text,
                    None,
                    c
                ),
                tile(s.faults, f.faults.to_string(), c.text, None, c),
                tile(
                    s.rallies_per_point,
                    f.phases_per_point
                        .as_ref()
                        .map_or("–".to_string(), |r| format!("{:.2}", r.per_point)),
                    c.text,
                    f.phases_per_point
                        .as_ref()
                        .map(|r| ui::fill(s.of_total, &[r.won.to_string(), r.rallies.to_string()])),
                    c
                ),
            ]
            .spacing(8),
        ]
        .spacing(8);
        let left = ui::panel(
            None,
            column![Column::with_children(evals).spacing(6), figures].spacing(16),
            c,
        );
        let zones: Vec<Element<'a, Message>> = [
            (!f.zones.is_empty()).then(|| attack_zones(f, s, c)),
            (!f.targets.is_empty()).then(|| self.targets(f, s, c)),
            (!f.blockers.is_empty()).then(|| self.blockers(f, s, c)),
        ]
        .into_iter()
        .flatten()
        .collect();
        let middle = Column::with_children(zones).spacing(12);
        let right = self.players(f, s, c);
        scrollable(
            row![
                container(column![left, middle].spacing(12)).width(Length::FillPortion(3)),
                container(right).width(Length::FillPortion(2)),
            ]
            .spacing(14),
        )
        .into()
    }

    fn targets<'a>(
        &'a self,
        f: &'a FundamentalDetail,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let (title, told) = match self.event {
            EventTypeEnum::P => (s.receptions_by_zone, s.reception_zones_told),
            EventTypeEnum::A => (s.attacks_by_zone, s.attack_zones_told),
            EventTypeEnum::D => (s.digs_by_zone, s.dig_zones_told),
            EventTypeEnum::B => (s.blocks_by_zone, s.block_zones_told),
            _ => (s.serves_by_zone, s.serve_zones_told),
        };
        let most = f
            .targets
            .iter()
            .flatten()
            .map(|t| t.count)
            .fold(1.0, f64::max);
        let grid = Column::with_children(f.targets.iter().map(|cells| {
            Row::with_children(cells.iter().map(|t| self.target(t, most, c)))
                .spacing(4)
                .into()
        }))
        .spacing(4);
        let conflicts: Element<'a, Message> = match &f.on_conflicts {
            Some(on) => {
                let all = ui::fill(
                    s.on_conflicts,
                    &[on.count.to_string(), format!("{:.0}", on.share)],
                );
                let efficiency = match (on.efficiency, self.efficiency) {
                    (Some(e), true) => format!(" · {e:+.0}%"),
                    _ => String::new(),
                };
                ui::body(format!("{all}{efficiency}")).into()
            }
            None => space::vertical().height(0).into(),
        };
        ui::panel(
            Some(title),
            column![
                small(
                    ui::fill(told, &[f.targets_told.to_string(), f.total.to_string()]),
                    c
                ),
                grid,
                conflicts
            ]
            .spacing(8),
            c,
        )
    }

    /// A zone of the court: how many went there, or how well.
    fn target<'a>(&self, t: &TargetStats, most: f64, c: Colors) -> Element<'a, Message> {
        let some = t.count > 0.0;
        let count = match t.count.fract() == 0.0 {
            true => format!("{:.0}", t.count),
            false => format!("{:.1}", t.count),
        };
        let (background, value, detail) = match (self.efficiency, t.efficiency) {
            (false, _) => (
                some.then(|| theme::mix(c.panel, c.accent, (t.count / most) as f32)),
                match some {
                    true => format!("{:.0}%", t.share),
                    false => String::new(),
                },
                match some {
                    true => format!("({count})"),
                    false => String::new(),
                },
            ),
            (true, Some(e)) => (
                Some(grade_color((e + 100.0) / 200.0, c)),
                format!("{e:+.0}%"),
                format!("n={count}"),
            ),
            // too few to tell
            (true, None) => (
                None,
                match some {
                    true => format!("({count})"),
                    false => String::new(),
                },
                String::new(),
            ),
        };
        let ink = match background {
            Some(b) if b.r * 0.299 + b.g * 0.587 + b.b * 0.114 > 0.55 => theme::DARK.background,
            _ => c.text,
        };
        container(
            column![
                text(t.label.clone()).size(SIZE_SMALL).color(ink),
                text(value)
                    .size(SIZE_BODY)
                    .font(theme::fonts().bold)
                    .color(ink),
                text(detail).size(SIZE_SMALL).color(ink),
            ]
            .align_x(Alignment::Center)
            .spacing(1),
        )
        .padding(6)
        .width(match t.zone {
            true => Length::FillPortion(3),
            false => Length::FillPortion(1),
        })
        .align_x(Alignment::Center)
        .style(move |_| container::Style {
            background: background.map(Into::into),
            border: iced::border::rounded(6).color(c.border).width(1),
            ..container::Style::default()
        })
        .into()
    }

    fn blockers<'a>(
        &'a self,
        f: &'a FundamentalDetail,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let title = match self.event {
            EventTypeEnum::B => s.blocks_by_blockers,
            _ => s.attacks_by_block,
        };
        let rows = f.blockers.iter().map(|b| {
            let shown = b.count > 0;
            let color = match shown {
                true => c.text,
                false => c.muted,
            };
            row![
                ui::body(b.name.clone()).color(color).width(Length::Fill),
                ui::body(match shown {
                    true => format!("{:.0}% ({})", b.share, b.count),
                    false => "–".to_string(),
                })
                .color(color),
                text(b.efficiency.map_or(String::new(), |e| format!("{e:+.0}%")))
                    .size(SIZE_BODY)
                    .font(theme::fonts().bold)
                    .color(
                        b.efficiency
                            .map_or(c.muted, |e| grade_color((e + 100.0) / 200.0, c))
                    )
                    .width(Length::Fixed(64.0))
                    .align_x(Alignment::End),
            ]
            .spacing(10)
            .into()
        });
        ui::panel(
            Some(title),
            column![
                small(
                    ui::fill(
                        s.blockers_told,
                        &[f.blockers_told.to_string(), f.total.to_string()]
                    ),
                    c
                ),
                Column::with_children(rows).spacing(4)
            ]
            .spacing(8),
            c,
        )
    }

    /// The players compared: a click narrows to one, again to all.
    fn players<'a>(
        &'a self,
        f: &'a FundamentalDetail,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let num = |v: String, w: f32| ui::body(v).width(Length::Fixed(w)).align_x(Alignment::End);
        let head = row![
            space::horizontal(),
            small(s.total, c)
                .width(Length::Fixed(52.0))
                .align_x(Alignment::End),
            small(ui::capitalized(s.efficiency), c)
                .width(Length::Fixed(56.0))
                .align_x(Alignment::End),
            small("+", c)
                .width(Length::Fixed(52.0))
                .align_x(Alignment::End),
            small(s.points, c)
                .width(Length::Fixed(52.0))
                .align_x(Alignment::End),
            small("=", c)
                .width(Length::Fixed(36.0))
                .align_x(Alignment::End),
        ]
        .spacing(4)
        .padding([0, 10]);
        let rows = f.players.iter().map(|p| {
            let chosen = self.filter.player.as_ref() == Some(&p.player.id);
            let body = row![
                text(p.player.number.to_string())
                    .size(SIZE_BODY)
                    .font(theme::fonts().bold)
                    .color(c.accent)
                    .width(Length::Fixed(30.0)),
                ui::body(p.player.name.clone()).width(Length::Fill),
                num(p.total.to_string(), 52.0),
                text(pct(p.efficiency))
                    .size(SIZE_BODY)
                    .font(theme::fonts().bold)
                    .color(p.grade.map_or(c.text, |g| grade_color(g, c)))
                    .width(Length::Fixed(56.0))
                    .align_x(Alignment::End),
                num(pct(p.positiveness), 52.0),
                num(p.points.map_or("–".to_string(), |p| p.to_string()), 52.0),
                num(p.errors.to_string(), 36.0),
            ]
            .spacing(4)
            .align_y(Alignment::Center);
            let message = match chosen {
                true => Message::Player(None),
                false => Message::Player(Some(p.player.id.clone())),
            };
            ui::list_row(body, chosen, Some(message), c)
        });
        ui::panel(
            Some(&ui::capitalized(s.players)),
            column![head, Column::with_children(rows).spacing(2)].spacing(6),
            c,
        )
    }
}

/// The attacks set to each zone of the net, and the kills.
fn attack_zones<'a>(
    f: &'a FundamentalDetail,
    s: &'static Strings,
    c: Colors,
) -> Element<'a, Message> {
    let cell = |zone: u8| -> Element<'a, Message> {
        let z = f.zones.iter().find(|z| z.zone == zone);
        container(
            column![
                small(zone, c),
                big(
                    pct(z.map(|z| z.share)),
                    match z {
                        Some(_) => c.text,
                        None => c.muted,
                    }
                ),
                small(
                    z.map_or(String::new(), |z| ui::fill(
                        s.kill_rate,
                        &[pct(Some(z.conversion))]
                    )),
                    c
                ),
            ]
            .align_x(Alignment::Center),
        )
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .into()
    };
    ui::panel(
        Some(s.distribution),
        column![
            row![cell(4), cell(3), cell(2)],
            row![space::horizontal(), cell(8), cell(9)],
        ]
        .spacing(10),
        c,
    )
}

/// The summary: the fundamentals and the phases; the rotations and the
/// distribution; the errors.
fn summary<'a>(live: &'a LiveStats, s: &'static Strings, c: Colors) -> Element<'a, Message> {
    let graded = |v: Option<f64>, g: Option<f64>| {
        text(pct(v))
            .size(SIZE_BODY)
            .font(theme::fonts().bold)
            .color(g.map_or(c.text, |g| grade_color(g, c)))
            .width(Length::Fixed(56.0))
            .align_x(Alignment::End)
    };
    let num = |n: String, w: f32| ui::body(n).width(Length::Fixed(w)).align_x(Alignment::End);
    let head = row![
        space::horizontal(),
        small(s.total, c)
            .width(Length::Fixed(44.0))
            .align_x(Alignment::End),
        small("#", c)
            .width(Length::Fixed(36.0))
            .align_x(Alignment::End),
        small("=", c)
            .width(Length::Fixed(36.0))
            .align_x(Alignment::End),
        small(s.efficiency, c)
            .width(Length::Fixed(56.0))
            .align_x(Alignment::End),
    ];
    let fundamentals = live.fundamentals.iter().map(|f| {
        row![
            ui::body(ui::capitalized(&f.name)).width(Length::Fill),
            num(f.total.to_string(), 44.0),
            num(f.perfect.to_string(), 36.0),
            num(f.errors.to_string(), 36.0),
            graded(f.efficiency, f.grade),
        ]
        .into()
    });
    let level: Element<'a, Message> = match live
        .fundamentals
        .iter()
        .filter(|f| f.total > 0)
        .all(|f| f.grade.is_none())
        && live.fundamentals.iter().any(|f| f.total > 0)
    {
        true => small(s.set_team_level, c).into(),
        false => space::vertical().height(0).into(),
    };
    let phase = |name: &str, p: &Option<PhaseStats>| -> Element<'a, Message> {
        row![
            ui::body(name.to_string()).width(Length::Fill),
            small(
                p.as_ref()
                    .map(|p| ui::fill(s.of_total, &[p.won.to_string(), p.rallies.to_string()]))
                    .unwrap_or_default(),
                c
            ),
            graded(
                p.as_ref().map(|p| p.percentage),
                p.as_ref().and_then(|p| p.grade)
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    };
    let attack = |name: &str, a: &Option<scout4all_mobile::AttackStats>| -> Element<'a, Message> {
        match a {
            Some(a) => row![
                ui::body(name.to_string()).width(Length::Fill),
                small(
                    format!(
                        "{} · #{} · ={} · /{}",
                        a.total, a.kills, a.errors, a.blocked
                    ),
                    c
                ),
                graded(Some(a.efficiency), a.grade),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
            None => space::vertical().height(0).into(),
        }
    };
    let possessions = live.side_out_possessions.iter().map(|p| {
        row![
            ui::body(p.name.clone()).width(Length::Fill),
            small(
                ui::fill(s.of_total, &[p.won.to_string(), p.total.to_string()]),
                c
            ),
            graded(p.percentage, None),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    });
    let first = ui::panel(
        Some(s.summary),
        column![
            head,
            Column::with_children(fundamentals).spacing(4),
            level,
            space::vertical().height(4),
            phase(s.side_out, &live.side_out),
            phase(s.break_point, &live.break_points),
            attack(s.first_attack, &live.first_attack),
            attack(s.counter_attack, &live.counter_attack),
            space::vertical().height(4),
            ui::section(s.side_out_by_possession, c),
            Column::with_children(possessions).spacing(4),
        ]
        .spacing(6),
        c,
    );
    let rotations = live.rotations.iter().map(|r| {
        let side = |p: &Option<PhaseStats>| -> Element<'a, Message> {
            column![
                graded(
                    p.as_ref().map(|p| p.percentage),
                    p.as_ref().and_then(|p| p.grade)
                ),
                small(
                    p.as_ref()
                        .map(|p| ui::fill(s.of_total, &[p.won.to_string(), p.rallies.to_string()]))
                        .unwrap_or_default(),
                    c
                ),
            ]
            .align_x(Alignment::End)
            .width(Length::Fixed(90.0))
            .into()
        };
        row![
            ui::body(r.name.clone()).width(Length::Fill),
            side(&r.side_out),
            side(&r.break_points),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    });
    let second = ui::panel(
        Some(s.rotations),
        column![
            row![
                space::horizontal(),
                small(s.side_out, c)
                    .width(Length::Fixed(90.0))
                    .align_x(Alignment::End),
                small(s.break_point, c)
                    .width(Length::Fixed(90.0))
                    .align_x(Alignment::End),
            ]
            .spacing(8),
            Column::with_children(rotations).spacing(6),
        ]
        .spacing(6),
        c,
    );
    let zone = |zone: u8| -> Element<'a, Message> {
        let z = live.distribution.iter().find(|z| z.zone == zone);
        column![
            small(zone, c),
            big(
                z.map_or("–".to_string(), |z| format!("{:.0}%", z.share)),
                match z {
                    Some(_) => c.text,
                    None => c.muted,
                }
            ),
            text(z.map_or(String::new(), |z| {
                format!(
                    "{} · {}",
                    ui::fill(s.zone_attacks, &[z.attacks.total.to_string()]),
                    pct(Some(z.attacks.efficiency))
                )
            }))
            .size(SIZE_SMALL)
            .color(
                z.and_then(|z| z.attacks.grade)
                    .map_or(c.muted, |g| grade_color(g, c))
            ),
        ]
        .align_x(Alignment::Center)
        .width(Length::Fill)
        .into()
    };
    let distribution: Element<'a, Message> = match live.distribution.is_empty() {
        true => small(s.no_attacks, c).into(),
        false => column![
            row![zone(4), zone(3), zone(2)],
            row![space::horizontal(), zone(8), zone(9)],
        ]
        .spacing(10)
        .into(),
    };
    let third = ui::panel(Some(s.distribution), distribution, c);
    let most = live
        .unforced_by_event
        .iter()
        .chain(live.forced_by_event.iter())
        .map(|e| f64::from(e.count))
        .fold(1.0, f64::max);
    let group = |title: &str, total: u32, errors: &'a [ErrorCount], forced: bool| {
        let color = match forced {
            true => c.muted,
            false => c.danger,
        };
        let rows = errors.iter().map(move |e| {
            row![
                ui::body(error_name(e.event, forced, s)).width(Length::Fixed(170.0)),
                bar(f64::from(e.count), most, color),
                ui::body(e.count)
                    .width(Length::Fixed(32.0))
                    .align_x(Alignment::End),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        });
        column![
            row![
                ui::body(title.to_string())
                    .font(theme::fonts().bold)
                    .width(Length::Fill),
                big(total, color),
            ]
            .align_y(Alignment::Center),
            Column::with_children(rows).spacing(4),
        ]
        .spacing(6)
    };
    let fourth = ui::panel(
        Some(s.errors),
        column![
            group(
                s.unforced_errors,
                live.unforced_errors,
                &live.unforced_by_event,
                false
            ),
            group(
                s.forced_errors,
                live.forced_errors,
                &live.forced_by_event,
                true
            ),
            row![
                tile(
                    s.opponent_errors,
                    live.opponent_errors.to_string(),
                    c.text,
                    Some(format!("{} {}", s.opponent_points, live.opponent_points)),
                    c
                ),
                tile(
                    s.aces,
                    format!("{} – {}", live.aces, live.aces_against),
                    c.text,
                    Some(s.ours_theirs.to_string()),
                    c
                ),
            ]
            .spacing(8),
        ]
        .spacing(14),
        c,
    );
    scrollable(
        row![
            container(first).width(Length::FillPortion(1)),
            container(column![second, third].spacing(14)).width(Length::FillPortion(1)),
            container(fourth).width(Length::FillPortion(1)),
        ]
        .spacing(14),
    )
    .into()
}

fn error_name(event: EventTypeEnum, forced: bool, s: &Strings) -> String {
    match (forced, event) {
        (false, EventTypeEnum::S) => s.error_serve,
        (false, EventTypeEnum::A) => s.error_attack,
        (false, EventTypeEnum::B) => s.error_block_invasion,
        (false, EventTypeEnum::F) => s.faults,
        (true, EventTypeEnum::A) => s.error_blocked,
        (true, EventTypeEnum::B) => s.error_block,
        (true, EventTypeEnum::P) => s.error_reception,
        (true, EventTypeEnum::D) => s.error_dig,
        _ => "",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arrows_go_round_the_choices() {
        let options = [None, Some(1), Some(2)];
        assert_eq!(cycle(&options, &None, true), Some(Some(1)));
        assert_eq!(cycle(&options, &None, false), Some(Some(2)));
        assert_eq!(cycle(&options, &Some(2), true), Some(None));
        // a choice no longer there: the first
        assert_eq!(cycle(&options, &Some(7), true), Some(None));
        assert_eq!(cycle::<u8>(&[], &1, true), None);
    }

    #[test]
    fn the_numbers_go_where_the_language_puts_them() {
        let numbers = ["3".to_string(), "8".to_string()];
        assert_eq!(ui::fill("{1} su {2}", &numbers), "3 su 8");
        assert_eq!(ui::fill("{2} 中 {1}", &numbers), "8 中 3");
    }
}
