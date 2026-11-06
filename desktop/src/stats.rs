//! The statistics of a match, or of a team over a period (010), the pages
//! of the app side by side: the summary (fundamentals, phases, rotations,
//! the distribution, the errors), the trend (of every set; of the team,
//! match by match), and a fundamental in detail. ⇧Tab Tab between the
//! pages, ↑ ↓ between the filters, ← → change the filter, as in a form.
//! The filters stand on the left: the period of the team (the last month,
//! the last matches, two days typed or picked on a calendar), the player,
//! the rotation, the phase and the situation (role, our reception in the
//! rally, the touch before, the set, the reception formation, the P1).

use {
    crate::{
        calendar::{self, Calendar},
        charts::{self, MatchesTrend, Trend},
        form::Mask,
        keymap,
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL, grade_color},
        ui,
    },
    chrono::{Local, NaiveDate},
    iced::{
        Alignment, Color, Element, Length,
        widget::{Column, Row, button, column, container, row, scrollable, space, text},
    },
    scout4all_core::{
        analysis::MOVING_WINDOW,
        detail::Tone,
        shapes::court_zone::CourtZoneEnum,
        shapes::{
            enums::{EvalEnum, EventTypeEnum, PhaseEnum, RoleEnum},
            flags::ReceptionSchemeEnum,
            technique::SetTypeEnum,
        },
        video::StoredProfile,
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{
        AnalysisPeriod, ClipPick, DetailFilter, DetailStats, ErrorCount, FundamentalDetail,
        LiveStats, MobileError, PhaseStats, Scout4All, SituationChoices, StatsScope, TargetStats,
        TeamAnalysis,
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
    Period,
    Series,
    Mode,
    Event,
    Player,
    Rotation,
    Phase,
    Role,
    Reception,
    Previous,
    SetType,
    Scheme,
    InvertedP1,
    Zones,
}

/// The filters on the situation, after the player, rotation and phase.
const SITUATION: [Filter; 6] = [
    Filter::Role,
    Filter::Reception,
    Filter::Previous,
    Filter::SetType,
    Filter::Scheme,
    Filter::InvertedP1,
];

/// How the period of a team is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    LastMonth,
    LastMatches,
    Between,
}

const KINDS: [Kind; 3] = [Kind::LastMonth, Kind::LastMatches, Kind::Between];

/// The last matches offered first, as in the coach's "the last 4".
const LAST_MATCHES: u32 = 4;

/// Where the statistics come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Match(String),
    /// the matches of the team in the period
    Team,
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
    Kind(Kind),
    Last(u32),
    /// a day picked on the calendar
    Day(NaiveDate),
    /// the calendar a month back (-1) or on (1)
    Month(i32),
    /// the day to write: the last of the period (true) or the first
    Editing(bool),
    Role(Option<RoleEnum>),
    Reception(Vec<EvalEnum>),
    Previous(Vec<EvalEnum>),
    SetTypes(Vec<SetTypeEnum>),
    Scheme(Option<ReceptionSchemeEnum>),
    InvertedP1(Option<bool>),
    /// an evaluation of the detail, to watch only those (or all again)
    ClipEval(EvalEnum),
    /// a zone of the net of the attacks, to watch only those
    ClipZone(u8),
    /// a zone of the court, to watch only those
    ClipTarget(CourtZoneEnum),
    /// all the actions of the detail again
    ClearPick,
    /// the actions of the detail in the video
    Watch,
}

pub enum Outcome {
    Stay,
    Back,
    /// the actions picked, one after the other in the video
    Watch(ClipPick),
}

/// The two days of a period, as written, and the month of the calendar.
#[derive(Debug, Clone)]
struct Days {
    from: String,
    to: String,
    /// the last day is written (or picked) next, not the first
    editing_to: bool,
    /// the first day of the month shown
    month: NaiveDate,
}

impl Days {
    /// A day written whole: chrono would take "2026-10-2" too.
    fn parse(text: &str) -> Option<NaiveDate> {
        (text.len() == 10)
            .then(|| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
            .flatten()
    }

    fn period(&self) -> Option<AnalysisPeriod> {
        Self::parse(&self.from)
            .zip(Self::parse(&self.to))
            .map(|(from, to)| AnalysisPeriod::Between {
                from: from.to_string(),
                to: to.to_string(),
            })
    }

    fn editing(&mut self) -> &mut String {
        match self.editing_to {
            true => &mut self.to,
            false => &mut self.from,
        }
    }

    /// A digit written on the day being written: the first day done, the
    /// last one next.
    fn type_digit(&mut self, digit: u8) {
        let text = Mask::Date.apply(&format!("{}{digit}", self.editing()));
        let done = Self::parse(&text);
        *self.editing() = text;
        if let (Some(day), false) = (done, self.editing_to) {
            self.month = calendar::month_of(day);
            self.editing_to = true;
        }
    }

    /// The last digit written taken back, with the separator before it.
    fn erase(&mut self) {
        let mut digits: String = self
            .editing()
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        digits.pop();
        *self.editing() = Mask::Date.apply(&digits);
    }

    /// A day picked: the first, then the last (the two the right way round).
    fn pick(&mut self, day: NaiveDate) {
        let text = day.format("%Y-%m-%d").to_string();
        match (self.editing_to, Self::parse(&self.from)) {
            (true, Some(from)) if day < from => {
                self.to = self.from.clone();
                self.from = text;
            }
            (true, _) => self.to = text,
            (false, _) => self.from = text,
        }
        self.editing_to = !self.editing_to;
    }
}

pub struct StatsScreen {
    core: Arc<Scout4All>,
    pub team_id: String,
    pub source: Source,
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
    choices: SituationChoices,
    // the team
    analysis: Option<TeamAnalysis>,
    period: AnalysisPeriod,
    kind: Kind,
    last: u32,
    days: Days,
    today: NaiveDate,
    /// the days with a match, for the calendar
    played: Vec<NaiveDate>,
    /// of the detail, to watch in the video: only these evaluations (all
    /// with none), the attacks set to this zone, the ones in this zone of
    /// the court
    clip_evals: Vec<EvalEnum>,
    clip_zone: Option<u8>,
    clip_target: Option<CourtZoneEnum>,
    /// how many actions are picked: none for a team
    clips: Option<usize>,
    /// the match has files of its video
    has_video: bool,
}

/// The filters on the left.
const FILTERS_WIDTH: f32 = 500.0;

/// A day as the period shows it: `YYYY-MM-DD` as written.
fn shown_day(day: &str) -> String {
    day.to_string()
}

/// A day under the trend: day and month.
fn short_day(day: &str) -> String {
    Days::parse(day).map_or_else(|| day.to_string(), |d| d.format("%d/%m").to_string())
}

/// The same evaluations, whatever their order.
fn same_set(a: &[EvalEnum], b: &[EvalEnum]) -> bool {
    a.len() == b.len() && a.iter().all(|e| b.contains(e))
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
        // the files of its video, to watch the actions
        let has_video = core
            .video(&team_id, &match_id)
            .is_ok_and(|(_, stored)| match stored {
                StoredProfile::Profile(p) => !p.files.is_empty(),
                StoredProfile::Missing | StoredProfile::Newer(_) => false,
            });
        let mut screen = StatsScreen::with(
            core,
            team_id,
            Source::Match(match_id),
            format!("{team} – {opponent}"),
        );
        screen.live = Some(live);
        screen.has_video = has_video;
        screen.load();
        Ok(screen)
    }

    /// The analysis of the team: the last month first.
    pub fn team(
        core: Arc<Scout4All>,
        team_id: String,
        s: &Strings,
    ) -> Result<StatsScreen, MobileError> {
        let team = core.team(team_id.clone())?.name;
        let title = format!("{team} – {}", s.analysis);
        let mut screen = StatsScreen::with(core, team_id, Source::Team, title);
        screen.load();
        match &screen.error {
            Some(e) if screen.analysis.is_none() => Err(MobileError::Failed { reason: e.clone() }),
            _ => Ok(screen),
        }
    }

    fn with(core: Arc<Scout4All>, team_id: String, source: Source, title: String) -> Self {
        let today = Local::now().date_naive();
        let choices = core.situation_choices();
        StatsScreen {
            core,
            team_id,
            source,
            title,
            page: 0,
            scope: StatsScope::Match,
            live: None,
            detail: None,
            series: 0,
            recent: false,
            trend: Trend::of_stats(None, 0, false),
            event: EventTypeEnum::S,
            filter: DetailFilter::default(),
            efficiency: false,
            row: 0,
            error: None,
            choices,
            analysis: None,
            period: AnalysisPeriod::LastMonth,
            kind: Kind::LastMonth,
            last: LAST_MATCHES,
            days: Days {
                from: String::new(),
                to: String::new(),
                editing_to: false,
                month: calendar::month_of(today),
            },
            today,
            played: vec![],
            clip_evals: vec![],
            clip_zone: None,
            clip_target: None,
            clips: None,
            has_video: false,
        }
    }

    pub fn title(&self) -> String {
        self.title.clone()
    }

    /// The match the statistics are of, if of a match.
    pub fn match_id(&self) -> Option<String> {
        match &self.source {
            Source::Match(m) => Some(m.clone()),
            Source::Team => None,
        }
    }

    fn is_team(&self) -> bool {
        self.source == Source::Team
    }

    /// Reads the statistics again, for the scope (or period) and the
    /// filter chosen.
    fn load(&mut self) {
        self.error = None;
        let team = self.team_id.clone();
        match self.source.clone() {
            Source::Match(m) => {
                let live = self.core.match_stats(team.clone(), m.clone(), self.scope);
                let detail =
                    self.core
                        .match_detail(team, m, self.scope, self.event, self.filter.clone());
                match live {
                    Ok(live) => self.live = Some(live),
                    Err(e) => self.error = Some(e.to_string()),
                }
                match detail {
                    Ok(detail) => self.detail = Some(detail),
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            Source::Team => {
                let analysis = self.core.team_analysis(
                    team,
                    self.period.clone(),
                    self.event,
                    self.filter.clone(),
                );
                match analysis {
                    Ok(a) => {
                        self.live = Some(a.summary.clone());
                        self.detail = Some(a.detail.clone());
                        self.played = a
                            .played_days
                            .iter()
                            .filter_map(|d| Days::parse(d))
                            .collect();
                        self.analysis = Some(a);
                    }
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
        }
        self.recount();
        self.retrend();
    }

    /// What of the detail is picked to watch in the video.
    fn pick(&self) -> ClipPick {
        ClipPick {
            scope: self.scope,
            event: self.event,
            filter: self.filter.clone(),
            evals: self.clip_evals.clone(),
            zone: self.clip_zone,
            target: self.clip_target,
        }
    }

    /// How many actions are picked, of a match.
    fn recount(&mut self) {
        self.clips = match &self.source {
            Source::Match(m) => self
                .core
                .match_clips(&self.team_id, m, &self.pick())
                .ok()
                .map(|clips| clips.len()),
            Source::Team => None,
        };
    }

    /// Whether the actions of the detail can be watched: of a match with a
    /// video, some picked.
    fn watchable(&self) -> bool {
        self.has_video && self.current() == Page::Detail && self.clips.is_some_and(|n| n > 0)
    }

    /// What the actions picked are, as the title of the list beside the
    /// video: the fundamental, the filters chosen, the evaluations and
    /// zones picked.
    pub fn clips_title(&self, s: &Strings) -> String {
        let detail = self.detail.as_ref();
        let fundamental = detail
            .and_then(|d| d.fundamentals.iter().find(|f| f.value == self.event))
            .map(|f| ui::capitalized(&f.name));
        let scope = match self.scope {
            StatsScope::Set { number } => Some(format!("{} {number}", s.set_n)),
            StatsScope::Match => None,
        };
        let player = self.filter.player.as_ref().and_then(|id| {
            detail
                .and_then(|d| d.players.iter().find(|p| &p.id == id))
                .map(|p| format!("{} {}", p.number, p.name))
        });
        let rotation = self.filter.rotation.and_then(|r| {
            detail
                .and_then(|d| d.rotations.iter().find(|x| x.value == r))
                .map(|x| x.name.clone())
        });
        let phase = self.filter.phase.and_then(|ph| {
            detail
                .and_then(|d| d.phases.iter().find(|x| x.value == ph))
                .map(|x| ui::capitalized(&x.name))
        });
        let role = self.filter.role.and_then(|r| {
            self.choices
                .roles
                .iter()
                .find(|x| x.value == r)
                .map(|x| ui::capitalized(&x.name))
        });
        let symbols = |evals: &[EvalEnum]| -> Option<String> {
            let found: Vec<String> = self
                .choices
                .symbols
                .iter()
                .filter(|x| evals.contains(&x.value))
                .map(|x| x.symbol.clone())
                .collect();
            (!found.is_empty()).then(|| found.join(" "))
        };
        let reception =
            symbols(&self.filter.reception).map(|x| format!("{} {x}", s.rally_reception));
        let previous = symbols(&self.filter.previous).map(|x| format!("{} {x}", s.touch_before));
        let sets: Vec<String> = self
            .choices
            .set_types
            .iter()
            .filter(|t| self.filter.set_types.contains(&t.value))
            .map(|t| t.code.clone())
            .collect();
        let sets = (!sets.is_empty()).then(|| format!("{} {}", s.set_type, sets.join(" ")));
        let scheme = self
            .filter
            .scheme
            .and_then(|sc| self.choices.schemes.iter().find(|x| x.value == sc))
            .map(|x| ui::fill(s.receivers, &[x.receivers.to_string()]));
        let p1 = self.filter.inverted_p1.map(|i| match i {
            true => s.inverted.to_string(),
            false => s.canonical.to_string(),
        });
        // in the order of the detail, from the best
        let evals: Vec<String> = detail
            .and_then(|d| d.fundamental.as_ref())
            .iter()
            .flat_map(|f| f.evals.iter())
            .filter(|e| self.clip_evals.contains(&e.eval))
            .map(|e| e.symbol.clone())
            .collect();
        let evals = (!evals.is_empty()).then(|| evals.join(" "));
        let zone = self.clip_zone.map(|z| ui::fill(s.zone_n, &[z.to_string()]));
        let target = self.clip_target.and_then(|t| {
            detail
                .and_then(|d| d.fundamental.as_ref())
                .and_then(|f| f.targets.iter().flatten().find(|x| x.value == t))
                .map(|x| ui::fill(s.zone_n, std::slice::from_ref(&x.label)))
        });
        [
            fundamental,
            evals,
            player,
            zone,
            target,
            sets,
            rotation,
            phase,
            role,
            reception,
            previous,
            scheme,
            p1,
            scope,
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
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
        let narrow = [Filter::Player, Filter::Rotation, Filter::Phase];
        let first: Vec<Filter> = match (self.is_team(), self.current()) {
            (false, Page::Summary) => return vec![Filter::Scope],
            (false, Page::Trend) => return vec![Filter::Series, Filter::Mode],
            (false, Page::Detail) => vec![Filter::Scope, Filter::Event],
            (true, Page::Summary) => vec![Filter::Period],
            (true, Page::Trend) => vec![Filter::Period, Filter::Series],
            (true, Page::Detail) => vec![Filter::Period, Filter::Event],
        };
        let zones = (self.current() == Page::Detail && zones).then_some(Filter::Zones);
        first
            .into_iter()
            .chain(narrow)
            .chain(SITUATION)
            .chain(zones)
            .collect()
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

    /// The series of the trend: of the match, or of the team.
    fn series_count(&self) -> usize {
        match &self.analysis {
            Some(a) if self.is_team() => a.trend.len(),
            _ => self.live.as_ref().map_or(0, |l| l.trend.len()),
        }
    }

    /// The message ← (or →) sends on a filter.
    fn changed(&self, filter: Filter, forward: bool) -> Option<Message> {
        let detail = self.detail.as_ref();
        let evals = |groups: bool| -> Vec<Vec<EvalEnum>> {
            std::iter::once(vec![])
                .chain(
                    self.choices
                        .groups
                        .iter()
                        .filter(move |_| groups)
                        .map(|g| g.evals.clone()),
                )
                .chain(self.choices.symbols.iter().map(|s| vec![s.value]))
                .collect()
        };
        match filter {
            Filter::Scope => ui::cycle(&self.scopes(), &self.scope, forward).map(Message::Scope),
            Filter::Period => ui::cycle(&KINDS, &self.kind, forward).map(Message::Kind),
            Filter::Series => {
                let series: Vec<usize> = (0..self.series_count()).collect();
                ui::cycle(&series, &self.series, forward).map(Message::Series)
            }
            Filter::Mode => Some(Message::Recent(!self.recent)),
            Filter::Zones => Some(Message::Efficiency(!self.efficiency)),
            Filter::Event => {
                let events: Vec<EventTypeEnum> = detail
                    .map(|d| d.fundamentals.iter().map(|f| f.value).collect())
                    .unwrap_or_default();
                ui::cycle(&events, &self.event, forward).map(Message::Event)
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
                ui::cycle(&players, &self.filter.player, forward).map(Message::Player)
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
                ui::cycle(&rotations, &self.filter.rotation, forward).map(Message::Rotation)
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
                ui::cycle(&phases, &self.filter.phase, forward).map(Message::Phase)
            }
            Filter::Role => {
                let roles: Vec<Option<RoleEnum>> = [None]
                    .into_iter()
                    .chain(self.choices.roles.iter().map(|r| Some(r.value)))
                    .collect();
                ui::cycle(&roles, &self.filter.role, forward).map(Message::Role)
            }
            Filter::Reception => {
                ui::cycle(&evals(true), &self.filter.reception, forward).map(Message::Reception)
            }
            Filter::Previous => {
                ui::cycle(&evals(true), &self.filter.previous, forward).map(Message::Previous)
            }
            Filter::SetType => {
                let sets: Vec<Vec<SetTypeEnum>> = std::iter::once(vec![])
                    .chain(self.choices.set_types.iter().map(|t| vec![t.value]))
                    .collect();
                ui::cycle(&sets, &self.filter.set_types, forward).map(Message::SetTypes)
            }
            Filter::Scheme => {
                let schemes: Vec<Option<ReceptionSchemeEnum>> = [None]
                    .into_iter()
                    .chain(self.choices.schemes.iter().map(|s| Some(s.value)))
                    .collect();
                ui::cycle(&schemes, &self.filter.scheme, forward).map(Message::Scheme)
            }
            Filter::InvertedP1 => ui::cycle(
                &[None, Some(false), Some(true)],
                &self.filter.inverted_p1,
                forward,
            )
            .map(Message::InvertedP1),
        }
    }

    /// The period of the kind chosen: two days not written yet keep the
    /// period as it was.
    fn period_of(&self, kind: Kind) -> Option<AnalysisPeriod> {
        match kind {
            Kind::LastMonth => Some(AnalysisPeriod::LastMonth),
            Kind::LastMatches => Some(AnalysisPeriod::LastMatches { count: self.last }),
            Kind::Between => self.days.period(),
        }
    }

    /// A period changed: read again if it's whole and new.
    fn reperiod(&mut self) {
        if let Some(period) = self.period_of(self.kind).filter(|p| *p != self.period) {
            self.period = period;
            self.load();
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
                self.clear_pick();
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
            Message::Kind(kind) => {
                self.kind = kind;
                // the days of the period shown, to start from
                if let (Kind::Between, true, Some(a)) =
                    (kind, self.days.from.is_empty(), &self.analysis)
                {
                    self.days.from = a.from.clone().unwrap_or_default();
                    self.days.to = a.to.clone().unwrap_or_default();
                    self.days.editing_to = false;
                    if let Some(to) = Days::parse(&self.days.to) {
                        self.days.month = calendar::month_of(to);
                    }
                }
                self.reperiod();
            }
            Message::Last(last) => {
                self.last = last.clamp(1, 99);
                self.kind = Kind::LastMatches;
                self.reperiod();
            }
            Message::Day(day) => {
                self.days.pick(day);
                self.kind = Kind::Between;
                self.reperiod();
            }
            Message::Month(months) => self.days.month = calendar::shift(self.days.month, months),
            Message::Editing(to) => self.days.editing_to = to,
            Message::Role(role) => {
                self.filter.role = role;
                self.load();
            }
            Message::Reception(evals) => {
                self.filter.reception = evals;
                self.load();
            }
            Message::Previous(evals) => {
                self.filter.previous = evals;
                self.load();
            }
            Message::SetTypes(sets) => {
                self.filter.set_types = sets;
                self.load();
            }
            Message::Scheme(scheme) => {
                self.filter.scheme = scheme;
                self.load();
            }
            Message::InvertedP1(inverted) => {
                self.filter.inverted_p1 = inverted;
                self.load();
            }
            Message::ClipEval(eval) => {
                let picked = self.clip_evals.contains(&eval);
                self.clip_evals.retain(|e| *e != eval);
                if !picked {
                    self.clip_evals.push(eval);
                }
                self.recount();
            }
            Message::ClipZone(zone) => {
                self.clip_zone = (self.clip_zone != Some(zone)).then_some(zone);
                self.recount();
            }
            Message::ClipTarget(target) => {
                self.clip_target = (self.clip_target != Some(target)).then_some(target);
                self.recount();
            }
            Message::ClearPick => {
                self.clear_pick();
                self.recount();
            }
            Message::Watch => return self.watch(),
        }
        let rows = self.filters().len();
        self.row = self.row.min(rows.saturating_sub(1));
        Outcome::Stay
    }

    fn clear_pick(&mut self) {
        self.clip_evals.clear();
        self.clip_zone = None;
        self.clip_target = None;
    }

    /// The actions picked in the video, if they can be watched.
    fn watch(&self) -> Outcome {
        match self.watchable() {
            true => Outcome::Watch(self.pick()),
            false => Outcome::Stay,
        }
    }

    /// The keys of the period: + − the last matches; the digits, ⌫,
    /// space, PgUp PgDn the two days.
    fn period_key(&mut self, ck: KeyCombination) -> Option<Outcome> {
        let letter = ck.as_letter();
        match (self.kind, keymap::digit(&ck)) {
            (Kind::LastMatches, _) if matches!(letter, Some('+' | '=')) => {
                Some(self.update(Message::Last(self.last + 1)))
            }
            (Kind::LastMatches, _) if letter == Some('-') => {
                Some(self.update(Message::Last(self.last.saturating_sub(1))))
            }
            (Kind::Between, Some(digit)) => {
                self.days.type_digit(digit);
                self.reperiod();
                Some(Outcome::Stay)
            }
            (Kind::Between, None) if ck == key!(backspace) => {
                self.days.erase();
                Some(Outcome::Stay)
            }
            (Kind::Between, None) if ck == key!(space) => {
                Some(self.update(Message::Editing(!self.days.editing_to)))
            }
            (Kind::Between, None) if ck == key!(pageup) => Some(self.update(Message::Month(-1))),
            (Kind::Between, None) if ck == key!(pagedown) => Some(self.update(Message::Month(1))),
            _ => None,
        }
    }

    pub fn key(&mut self, ck: KeyCombination) -> Outcome {
        let rows = self.filters().len().max(1);
        let pages = PAGES.len();
        let previous_page =
            ck == key!(shift - tab) || ck == key!(shift - backtab) || ck == key!(backtab);
        if self.filters().get(self.row) == Some(&Filter::Period)
            && let Some(outcome) = self.period_key(ck)
        {
            return outcome;
        }
        match ck {
            k if k == BACK_KEY => Outcome::Back,
            k if k == key!(v) => self.watch(),
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
        let period = match (self.filters().get(self.row), self.kind) {
            (Some(Filter::Period), Kind::LastMatches) => {
                vec![(
                    "+ −".to_string(),
                    ui::fill(s.last_matches, &["N".to_string()]),
                )]
            }
            (Some(Filter::Period), Kind::Between) => vec![
                ("0–9 ⌫".to_string(), s.date.to_string()),
                ("␣".to_string(), s.from_to.to_string()),
                ("PgUp PgDn".to_string(), s.calendar_month.to_string()),
            ],
            _ => vec![],
        };
        [
            ("⇧⇥ ⇥".to_string(), s.page.to_string()),
            ("↑ ↓".to_string(), s.filter.to_string()),
            ("← →".to_string(), s.change.to_string()),
        ]
        .into_iter()
        .chain(period)
        .chain(
            self.watchable()
                .then(|| ("v".to_string(), s.watch_clips.to_string())),
        )
        .chain([("Esc".to_string(), s.back.to_string())])
        .collect()
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
        .spacing(8);
        let error: Element<'a, Message> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => space::vertical().height(0).into(),
        };
        let empty = self
            .analysis
            .as_ref()
            .is_some_and(|a| self.is_team() && a.matches.is_empty());
        let page: Element<'a, Message> = match (self.current(), &self.live, empty) {
            (_, _, true) => ui::panel(None, small(s.no_matches_period, c), c),
            (_, None, _) => space::vertical().into(),
            (Page::Summary, Some(live), _) => summary(live, s, c),
            (Page::Trend, Some(live), _) => match &self.analysis {
                Some(a) if self.is_team() => self.matches_page(a, s, c),
                _ => self.trend_page(live, s, c),
            },
            (Page::Detail, _, _) => self.detail_page(s, c),
        };
        let left = container(scrollable(ui::panel(None, filters, c)).height(Length::Fill))
            .width(Length::Fixed(FILTERS_WIDTH));
        let right = column![tabs, error, container(page).height(Length::Fill)]
            .spacing(12)
            .width(Length::Fill);
        row![left, right].spacing(14).into()
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
        let evals = |chosen: &Vec<EvalEnum>, message: fn(Vec<EvalEnum>) -> Message| {
            let toggled = |e: EvalEnum| -> Vec<EvalEnum> {
                let mut evals: Vec<EvalEnum> = chosen.iter().copied().filter(|c| *c != e).collect();
                if !chosen.contains(&e) {
                    evals.push(e);
                }
                evals
            };
            let names = [s.positive_group, "!", s.negative_group];
            [all(chosen.is_empty(), message(vec![]))]
                .into_iter()
                .chain(
                    self.choices
                        .groups
                        .iter()
                        .zip(names)
                        .filter(|(g, _)| g.evals.len() > 1)
                        .map(|(g, name)| {
                            let same = same_set(chosen, &g.evals);
                            pill(
                                format!("{name} {}", g.symbols),
                                same,
                                message(match same {
                                    true => vec![],
                                    false => g.evals.clone(),
                                }),
                            )
                        }),
                )
                .chain(self.choices.symbols.iter().map(|sym| {
                    pill(
                        sym.symbol.clone(),
                        chosen.contains(&sym.value),
                        message(toggled(sym.value)),
                    )
                }))
                .collect::<Vec<_>>()
        };
        let (name, pills): (&str, Vec<Element<'a, Message>>) = match filter {
            Filter::Period => return self.period_row(on, s, c),
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
            Filter::Series => {
                let names: Vec<String> = match &self.analysis {
                    Some(a) if self.is_team() => a.trend.iter().map(|l| l.name.clone()).collect(),
                    _ => self
                        .live
                        .iter()
                        .flat_map(|l| l.trend.iter().map(|t| t.name.clone()))
                        .collect(),
                };
                (
                    s.series,
                    names
                        .into_iter()
                        .enumerate()
                        .map(|(i, name)| {
                            pill(ui::capitalized(&name), i == self.series, Message::Series(i))
                        })
                        .collect(),
                )
            }
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
            Filter::Role => (
                s.role,
                [all(self.filter.role.is_none(), Message::Role(None))]
                    .into_iter()
                    .chain(self.choices.roles.iter().map(|r| {
                        pill(
                            ui::capitalized(&r.name),
                            self.filter.role == Some(r.value),
                            Message::Role(Some(r.value)),
                        )
                    }))
                    .collect(),
            ),
            Filter::Reception => (
                s.rally_reception,
                evals(&self.filter.reception, Message::Reception),
            ),
            Filter::Previous => (
                s.touch_before,
                evals(&self.filter.previous, Message::Previous),
            ),
            Filter::SetType => {
                let chosen = &self.filter.set_types;
                (
                    s.set_type,
                    [all(chosen.is_empty(), Message::SetTypes(vec![]))]
                        .into_iter()
                        .chain(self.choices.set_types.iter().map(|t| {
                            let mut toggled: Vec<SetTypeEnum> =
                                chosen.iter().copied().filter(|c| *c != t.value).collect();
                            if !chosen.contains(&t.value) {
                                toggled.push(t.value);
                            }
                            ui::tip(
                                pill(
                                    t.code.clone(),
                                    chosen.contains(&t.value),
                                    Message::SetTypes(toggled),
                                ),
                                &t.name,
                                c,
                            )
                        }))
                        .collect(),
                )
            }
            Filter::Scheme => (
                s.reception_scheme,
                [all(self.filter.scheme.is_none(), Message::Scheme(None))]
                    .into_iter()
                    .chain(self.choices.schemes.iter().map(|sc| {
                        pill(
                            ui::fill(s.receivers, &[sc.receivers.to_string()]),
                            self.filter.scheme == Some(sc.value),
                            Message::Scheme(Some(sc.value)),
                        )
                    }))
                    .collect(),
            ),
            Filter::InvertedP1 => (
                s.p1_kind,
                vec![
                    all(self.filter.inverted_p1.is_none(), Message::InvertedP1(None)),
                    pill(
                        s.canonical.to_string(),
                        self.filter.inverted_p1 == Some(false),
                        Message::InvertedP1(Some(false)),
                    ),
                    pill(
                        s.inverted.to_string(),
                        self.filter.inverted_p1 == Some(true),
                        Message::InvertedP1(Some(true)),
                    ),
                ],
            ),
        };
        ui::choice_line(name, on, pills, c)
    }

    /// The period: the last month, the last matches (+ −), two days written
    /// or picked on the calendar; then the matches it takes.
    fn period_row<'a>(&'a self, on: bool, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let pill = |label: String, chosen: bool, message: Message| {
            ui::pill(&label, chosen, Some(message), c)
        };
        let step = |label: &'static str, message: Message| {
            button(text(label).size(SIZE_SMALL).align_x(Alignment::Center))
                .padding([2, 8])
                .on_press(message)
                .style(move |_, status| button::Style {
                    background: matches!(status, button::Status::Hovered)
                        .then_some(iced::Background::Color(c.button)),
                    text_color: c.accent,
                    border: iced::border::rounded(6).color(c.border).width(1),
                    ..button::Style::default()
                })
        };
        let last = row![
            pill(
                ui::fill(s.last_matches, &[self.last.to_string()]),
                self.kind == Kind::LastMatches,
                Message::Last(self.last),
            ),
            step("−", Message::Last(self.last.saturating_sub(1))),
            step("+", Message::Last(self.last + 1)),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let pills: Vec<Element<'a, Message>> = vec![
            pill(
                s.last_month.to_string(),
                self.kind == Kind::LastMonth,
                Message::Kind(Kind::LastMonth),
            ),
            last.into(),
            pill(
                s.between.to_string(),
                self.kind == Kind::Between,
                Message::Kind(Kind::Between),
            ),
        ];
        let line = ui::choice_line(s.period, on, pills, c);
        let chosen: Element<'a, Message> = match &self.analysis {
            Some(a) => {
                let days = a
                    .from
                    .as_deref()
                    .zip(a.to.as_deref())
                    .map(|(from, to)| format!("{} → {}  ·  ", shown_day(from), shown_day(to)))
                    .unwrap_or_default();
                let skipped = match a.skipped.is_empty() {
                    true => String::new(),
                    false => format!("\n{}", ui::fill(s.skipped, &[a.skipped.join(", ")])),
                };
                small(
                    format!(
                        "{days}{}{skipped}",
                        ui::fill(s.matches_count, &[a.matches.len().to_string()])
                    ),
                    c,
                )
                .into()
            }
            None => space::vertical().height(0).into(),
        };
        let between: Element<'a, Message> = match self.kind {
            Kind::Between => self.days_view(s, c),
            _ => space::vertical().height(0).into(),
        };
        column![line, row![space::horizontal().width(148), chosen], between]
            .spacing(6)
            .into()
    }

    /// The two days, as written (the one being written lit), and the
    /// calendar.
    fn days_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let field = |label: &'static str, value: &str, editing: bool, to: bool| {
            let shown = match value.is_empty() {
                true => s.date_placeholder.to_string(),
                false => value.to_string(),
            };
            let caret = match editing {
                true => "▏",
                false => "",
            };
            button(
                column![
                    small(label, c),
                    text(format!("{shown}{caret}"))
                        .size(SIZE_BODY)
                        .font(theme::fonts().bold)
                        .color(match value.is_empty() {
                            true => c.muted,
                            false => c.text,
                        }),
                ]
                .spacing(2),
            )
            .padding([6, 10])
            .width(Length::Fixed(140.0))
            .on_press(Message::Editing(to))
            .style(move |_, _| button::Style {
                border: iced::border::rounded(8)
                    .color(match editing {
                        true => c.accent,
                        false => c.border,
                    })
                    .width(match editing {
                        true => 2,
                        false => 1,
                    }),
                ..button::Style::default()
            })
        };
        let fields = row![
            field(s.from_day, &self.days.from, !self.days.editing_to, false),
            field(s.to_day, &self.days.to, self.days.editing_to, true),
        ]
        .spacing(8);
        let calendar = Calendar {
            month: self.days.month,
            from: Days::parse(&self.days.from),
            to: Days::parse(&self.days.to),
            marked: &self.played,
            today: self.today,
        }
        .view(s, c, Message::Day, Message::Month);
        column![
            fields,
            small(s.period_hint, c),
            container(calendar).padding([4, 0])
        ]
        .spacing(6)
        .padding(iced::Padding {
            left: 148.0,
            ..iced::Padding::ZERO
        })
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

    /// The figure of the team match by match, with the moving average.
    fn matches_page<'a>(
        &'a self,
        a: &'a TeamAnalysis,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let Some(line) = a.trend.get(self.series) else {
            return space::vertical().into();
        };
        let trend = MatchesTrend::of(
            line,
            a.matches.iter().map(|m| short_day(&m.day)).collect(),
            a.matches.iter().map(|m| m.opponent.clone()).collect(),
            s.touches,
        );
        let legend = row![
            text(format!("━● {}", s.per_match))
                .size(SIZE_SMALL)
                .color(c.text),
            text(format!(
                "━━ {}",
                ui::fill(s.moving_average, &[MOVING_WINDOW.to_string()])
            ))
            .size(SIZE_SMALL)
            .color(c.accent),
            small(
                format!(
                    "- - {}: {}",
                    ui::fill(s.matches_count, &[a.matches.len().to_string()]),
                    pct(line.overall)
                ),
                c
            ),
        ]
        .spacing(14);
        let chart: Element<'a, Message> = match trend.is_empty() {
            true => small(s.no_rallies, c).into(),
            false => charts::matches_view(trend, c),
        };
        ui::panel(
            Some(&ui::capitalized(&line.name)),
            column![legend, chart].spacing(10),
            c,
        )
    }

    fn detail_page<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let page = self.detail_figures(s, c);
        match self.clips {
            Some(_) => column![self.watch_bar(s, c), page].spacing(10).into(),
            None => page,
        }
    }

    /// The actions of the detail in the video: how many, the evaluations
    /// and zones picked cleared, or why not.
    fn watch_bar<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let count = self.clips.unwrap_or(0);
        let picked =
            !self.clip_evals.is_empty() || self.clip_zone.is_some() || self.clip_target.is_some();
        let clear: Element<'a, Message> = match picked {
            true => ui::pill(
                &format!("✕ {}", s.filter_all),
                false,
                Some(Message::ClearPick),
                c,
            ),
            false => space::horizontal().width(0).into(),
        };
        let said: Element<'a, Message> = match (self.has_video, count) {
            (false, _) => small(s.clips_no_video, c).into(),
            (true, 0) => small(s.clips_none, c).into(),
            (true, _) => small(s.clips_pick_hint, c).into(),
        };
        let watch = ui::pill(
            &format!("▶ {}", s.watch_clips),
            self.watchable(),
            self.watchable().then_some(Message::Watch),
            c,
        );
        row![
            watch,
            ui::body(ui::fill(s.clips_count, &[count.to_string()])),
            clear,
            container(said).width(Length::Fill).align_x(Alignment::End),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    }

    fn detail_figures<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
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
            let body = row![
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
            .align_y(Alignment::Center);
            // picked to watch only those, of a match
            let message = self.clips.map(|_| Message::ClipEval(e.eval));
            ui::list_row(body, self.clip_evals.contains(&e.eval), message, c)
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
            column![Column::with_children(evals).spacing(2), figures].spacing(16),
            c,
        );
        let zones: Vec<Element<'a, Message>> = [
            (!f.zones.is_empty()).then(|| self.attack_zones(f, s, c)),
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
                container(column![left, middle].spacing(12)).width(Length::FillPortion(1)),
                container(right).width(Length::FillPortion(1)),
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
        let cell = container(
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
        .width(Length::Fill)
        .align_x(Alignment::Center)
        .style(move |_| container::Style {
            background: background.map(Into::into),
            border: iced::border::rounded(6).color(c.border).width(1),
            ..container::Style::default()
        });
        // picked to watch only its actions, of a match
        let message = self
            .clips
            .filter(|_| some)
            .map(|_| Message::ClipTarget(t.value));
        container(pickable(
            cell,
            self.clip_target == Some(t.value),
            message,
            c,
        ))
        .width(match t.zone {
            true => Length::FillPortion(3),
            false => Length::FillPortion(1),
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
                .width(Length::Fixed(44.0))
                .align_x(Alignment::End),
            small(ui::capitalized(s.efficiency), c)
                .width(Length::Fixed(56.0))
                .align_x(Alignment::End),
            small("+", c)
                .width(Length::Fixed(44.0))
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
                ui::name(p.player.name.clone()),
                num(p.total.to_string(), 44.0),
                text(pct(p.efficiency))
                    .size(SIZE_BODY)
                    .font(theme::fonts().bold)
                    .color(p.grade.map_or(c.text, |g| grade_color(g, c)))
                    .width(Length::Fixed(56.0))
                    .align_x(Alignment::End),
                num(pct(p.positiveness), 44.0),
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

/// A cell of the detail to click, to watch only its actions: lit when
/// picked.
fn pickable<'a>(
    content: impl Into<Element<'a, Message>>,
    picked: bool,
    message: Option<Message>,
    c: Colors,
) -> Element<'a, Message> {
    button(content)
        .padding(0)
        .on_press_maybe(message)
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let ring = match (picked, hovered) {
                (true, _) => c.accent,
                (false, true) => c.border,
                (false, false) => Color::TRANSPARENT,
            };
            button::Style {
                background: None,
                text_color: c.text,
                border: iced::border::rounded(8).color(ring).width(2),
                ..button::Style::default()
            }
        })
        .into()
}

impl StatsScreen {
    /// The attacks set to each zone of the net, and the kills.
    fn attack_zones<'a>(
        &'a self,
        f: &'a FundamentalDetail,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let cell = |zone: u8| -> Element<'a, Message> {
            let z = f.zones.iter().find(|z| z.zone == zone);
            let cell = container(
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
            .align_x(Alignment::Center);
            let message = self.clips.map(|_| Message::ClipZone(zone));
            container(pickable(cell, self.clip_zone == Some(zone), message, c))
                .width(Length::Fill)
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
            // the counts under the name: they never run into it
            Some(a) => column![
                row![
                    ui::body(name.to_string()).width(Length::Fill),
                    graded(Some(a.efficiency), a.grade),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                small(
                    format!(
                        "{} · #{} · ={} · /{}",
                        a.total, a.kills, a.errors, a.blocked
                    ),
                    c
                ),
            ]
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
#[allow(clippy::unreachable)]
mod tests {
    use super::*;

    #[test]
    fn the_arrows_go_round_the_choices() {
        let options = [None, Some(1), Some(2)];
        assert_eq!(ui::cycle(&options, &None, true), Some(Some(1)));
        assert_eq!(ui::cycle(&options, &None, false), Some(Some(2)));
        assert_eq!(ui::cycle(&options, &Some(2), true), Some(None));
        // a choice no longer there: the first
        assert_eq!(ui::cycle(&options, &Some(7), true), Some(None));
        assert_eq!(ui::cycle::<u8>(&[], &1, true), None);
    }

    #[test]
    fn the_numbers_go_where_the_language_puts_them() {
        let numbers = ["3".to_string(), "8".to_string()];
        assert_eq!(ui::fill("{1} su {2}", &numbers), "3 su 8");
        assert_eq!(ui::fill("{2} 中 {1}", &numbers), "8 中 3");
    }

    use {
        crate::strings::strings,
        scout4all_core::{
            match_form::MatchForm,
            player_form::PlayerForm,
            shapes::{
                country::CountryCode,
                enums::{GenderEnum, LanguageEnum, TeamClassificationEnum, TeamSideEnum},
            },
            team_form::TeamForm,
        },
    };

    /// A team of seven and two matches played, a set each with our serves:
    /// three aces on 24/10, an ace and an error on 31/10.
    fn team_with_two_matches(core: &Scout4All) -> String {
        let team = core
            .save_team(
                None,
                TeamForm {
                    name: "Cortona".into(),
                    classification: Some(TeamClassificationEnum::Regional),
                    gender: Some(GenderEnum::Women),
                    country: CountryCode::known("IT"),
                    year: "2026".into(),
                },
            )
            .unwrap();
        for n in 1..=7 {
            core.save_player(
                team.clone(),
                None,
                PlayerForm {
                    name: format!("P{n}"),
                    role: Some(RoleEnum::OutsideHitter),
                    number: n.to_string(),
                    birth_date: String::new(),
                    height_cm: String::new(),
                    notes: String::new(),
                },
            )
            .unwrap();
        }
        let ids: Vec<String> = core
            .team(team.clone())
            .unwrap()
            .players
            .into_iter()
            .map(|p| p.id)
            .collect();
        for (opponent, date, serves) in [
            (
                "Arezzo",
                "2026-10-24T18:00:00+02:00",
                vec![EvalEnum::Perfect; 3],
            ),
            (
                "Siena",
                "2026-10-31T18:00:00+01:00",
                vec![EvalEnum::Perfect, EvalEnum::Error],
            ),
        ] {
            let m = core
                .save_match(
                    team.clone(),
                    None,
                    MatchForm {
                        opponent: opponent.into(),
                        date: chrono::DateTime::parse_from_rfc3339(date).ok(),
                        home: true,
                        max_substitutions: "6".into(),
                        notes: String::new(),
                        roster: None,
                    },
                )
                .unwrap();
            let start = core.set_start(team.clone(), m.clone()).unwrap();
            start.choose_serving_team(TeamSideEnum::Us);
            for (position, id) in (1..=6).zip(&ids) {
                start.place(position, Some(id.clone())).unwrap();
            }
            start.choose_setter(Some(ids[0].clone())).unwrap();
            start.choose_libero(Some(ids[6].clone())).unwrap();
            core.start_set(start).unwrap();
            let scouting = core.scouting(team.clone(), m).unwrap();
            for eval in serves {
                scouting.choose_eval(eval).unwrap();
            }
        }
        team
    }

    /// The serves of the detail of a match, picked to watch in its video:
    /// how many, only the errors by clicking their row, the pick cleared
    /// with another fundamental; `v` only with a video; a team has none.
    #[test]
    fn the_actions_of_the_detail_to_watch() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = team_with_two_matches(&core);
        let siena = core
            .matches(team.clone())
            .unwrap()
            .into_iter()
            .find_map(|m| match m {
                scout4all_mobile::MatchListing::Match { id, opponent, .. }
                    if opponent == "Siena" =>
                {
                    Some(id)
                }
                _ => None,
            })
            .unwrap_or_default();
        let open = |core: &Arc<Scout4All>| {
            let mut screen = StatsScreen::new(core.clone(), team.clone(), siena.clone()).unwrap();
            screen.update(Message::Page(2));
            screen
        };
        let mut screen = open(&core);
        assert_eq!(screen.event, EventTypeEnum::S);
        assert_eq!(screen.clips, Some(2));
        // no video: nothing to watch
        assert!(!screen.has_video && !screen.watchable());
        assert!(matches!(screen.key(key!(v)), Outcome::Stay));
        screen.update(Message::ClipEval(EvalEnum::Error));
        assert_eq!(screen.clips, Some(1));
        assert_eq!(screen.clips_title(s), "Battuta · =");
        screen.update(Message::ClipEval(EvalEnum::Error));
        assert_eq!(screen.clips, Some(2));
        screen.update(Message::ClipEval(EvalEnum::Perfect));
        screen.update(Message::Event(EventTypeEnum::A));
        assert!(screen.clip_evals.is_empty());
        // a video of the match: `v` watches the serves picked
        let (the_match, _) = core.video(&team, &siena).unwrap();
        let profile = scout4all_core::video::VideoProfile {
            files: vec![scout4all_core::video::VideoFile {
                path: "match.mp4".into(),
                start: None,
            }],
            ..Default::default()
        };
        core.save_video(&the_match, &profile).unwrap();
        let mut screen = open(&core);
        assert!(screen.has_video && screen.watchable());
        screen.update(Message::ClipEval(EvalEnum::Error));
        match screen.key(key!(v)) {
            Outcome::Watch(pick) => {
                assert_eq!(pick.event, EventTypeEnum::S);
                assert_eq!(pick.evals, vec![EvalEnum::Error]);
            }
            _ => unreachable!("the serves to watch"),
        }
        assert!(screen.hints(s).iter().any(|(k, _)| k == "v"));
        // on another page, no
        screen.update(Message::Page(0));
        assert!(matches!(screen.key(key!(v)), Outcome::Stay));
        // a team: none
        let analysis = StatsScreen::team(core.clone(), team.clone(), s).unwrap();
        assert_eq!(analysis.clips, None);
    }

    fn aces(screen: &StatsScreen) -> u32 {
        screen.live.as_ref().map_or(0, |l| l.aces)
    }

    fn opponents(screen: &StatsScreen) -> Vec<String> {
        screen
            .analysis
            .iter()
            .flat_map(|a| a.matches.iter().map(|m| m.opponent.clone()))
            .collect()
    }

    /// The period by keys: the last matches (+ −), then the two days
    /// written; a day picked on the calendar; a filter on the situation;
    /// Esc back to the team.
    #[test]
    fn the_analysis_of_a_team_by_keys() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = team_with_two_matches(&core);
        // from the team, s
        let mut from = crate::team::TeamScreen::new(core.clone(), team.clone()).unwrap();
        assert!(matches!(
            from.key(s, key!(s)),
            crate::team::Outcome::Analysis
        ));
        let mut screen = StatsScreen::team(core, team, s).unwrap();
        assert_eq!(screen.filters().first(), Some(&Filter::Period));
        // the last 4: both
        screen.key(key!(right));
        assert_eq!(screen.kind, Kind::LastMatches);
        assert_eq!(opponents(&screen), ["Arezzo", "Siena"]);
        assert_eq!(aces(&screen), 4);
        for _ in 0..3 {
            screen.key(key!('-'));
        }
        assert_eq!(screen.period, AnalysisPeriod::LastMatches { count: 1 });
        assert_eq!(opponents(&screen), ["Siena"]);
        // from and to, written: the first day done, the last one next
        screen.key(key!(right));
        assert_eq!(screen.kind, Kind::Between);
        screen.days.from.clear();
        screen.days.to.clear();
        screen.days.editing_to = false;
        for d in "20261025".chars().chain("20261101".chars()) {
            let digit = KeyCombination::from(crokey::crossterm::event::KeyCode::Char(d));
            screen.key(digit);
        }
        assert_eq!(screen.days.from, "2026-10-25");
        assert_eq!(screen.days.to, "2026-11-01");
        assert_eq!(opponents(&screen), ["Siena"]);
        // ⌫ takes a digit back: the period stays the last whole one
        screen.key(key!(backspace));
        assert_eq!(screen.days.to, "2026-11-0");
        assert_eq!(opponents(&screen), ["Siena"]);
        // picked on the calendar, the last day before the first: swapped
        let day = |d| NaiveDate::from_ymd_opt(2026, 10, d).unwrap();
        screen.update(Message::Editing(false));
        screen.update(Message::Day(day(31)));
        screen.update(Message::Day(day(20)));
        assert_eq!(
            (screen.days.from.as_str(), screen.days.to.as_str()),
            ("2026-10-20", "2026-10-31")
        );
        assert_eq!(opponents(&screen), ["Arezzo", "Siena"]);
        assert_eq!(screen.played, [day(24), day(31)]);
        // the serves are in break: none with the inverted P1 asked
        screen.update(Message::InvertedP1(Some(true)));
        assert_eq!(aces(&screen), 0);
        screen.update(Message::InvertedP1(None));
        assert_eq!(aces(&screen), 4);
        // the trend: a point a match
        screen.update(Message::Page(1));
        let serve = screen
            .analysis
            .as_ref()
            .and_then(|a| {
                a.trend
                    .iter()
                    .find(|t| t.series == scout4all_core::trend::Series::Serve)
            })
            .map(|t| t.values.clone());
        assert_eq!(serve, Some(vec![Some(100.0), Some(0.0)]));
        assert!(matches!(screen.key(BACK_KEY), Outcome::Back));
        assert_eq!(screen.match_id(), None);
    }

    /// The detail of a match has the filters on the situation too.
    #[test]
    fn the_detail_of_a_match_by_situation() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = team_with_two_matches(&core);
        let first = match core
            .matches(team.clone())
            .unwrap()
            .into_iter()
            .find_map(|m| match m {
                scout4all_mobile::MatchListing::Match { id, opponent, .. }
                    if opponent == "Arezzo" =>
                {
                    Some(id)
                }
                _ => None,
            }) {
            Some(id) => id,
            None => unreachable!("the match of Arezzo"),
        };
        let mut screen = StatsScreen::new(core, team, first).unwrap();
        assert_eq!(screen.filters(), [Filter::Scope]);
        screen.update(Message::Page(2));
        assert!(screen.filters().contains(&Filter::Reception));
        let total = |screen: &StatsScreen| {
            screen
                .detail
                .as_ref()
                .and_then(|d| d.fundamental.as_ref())
                .map_or(0, |f| f.total)
        };
        assert_eq!(total(&screen), 3);
        screen.update(Message::Scheme(Some(ReceptionSchemeEnum::Four)));
        assert_eq!(total(&screen), 0);
        let _ = s;
    }
}
