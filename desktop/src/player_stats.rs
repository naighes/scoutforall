//! The statistics of a player in the profile: the filters of the period and
//! of the competition, a box a fundamental, then each fundamental match by
//! match. The figures are the ones of the analysis of the team narrowed to
//! the player, which the detail opens.

use {
    crate::{
        charts::{self, MatchesTrend},
        notice::{self, Level},
        period::{self, Kind, PeriodChoice},
        stats::{matches_legend, short_day},
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL, grade_color},
        ui,
    },
    chrono::NaiveDate,
    iced::{
        Element, Length,
        widget::{Column, Row, column, container, responsive, scrollable, space, text},
    },
    scout4all_core::shapes::enums::EventTypeEnum,
    scout4all_keys::{KeyCombination, key},
    scout4all_mobile::{
        AnalysisPeriod, PlayerFundamental, PlayerMeasure, PlayerStatistics, Scout4All,
    },
    uuid::Uuid,
};

/// The width a third column of boxes needs.
const WIDE: f32 = 700.0;
const CHART: f32 = 240.0;
const FIGURE: f32 = 34.0;

#[derive(Debug, Clone)]
pub enum Message {
    Period(period::Message),
    Competition(Option<Uuid>),
    /// the analysis of the team narrowed to the player
    Detail,
}

/// A filter the arrows change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    Period,
    Competition,
}

/// What the detail starts from, and what the profile opens again with.
#[derive(Debug, Clone)]
pub struct Filters {
    pub choice: PeriodChoice,
    pub period: AnalysisPeriod,
    pub competition: Option<Uuid>,
}

impl Filters {
    /// The whole season, `today` in it.
    pub fn season(today: NaiveDate) -> Filters {
        Filters {
            choice: PeriodChoice::new(Kind::Season, today),
            period: AnalysisPeriod::Season,
            competition: None,
        }
    }
}

pub struct PlayerStats {
    pub filters: Filters,
    read: Result<PlayerStatistics, String>,
    /// the days with a match, for the calendar
    played: Vec<NaiveDate>,
    row: usize,
}

impl PlayerStats {
    pub fn new(core: &Scout4All, team: &str, player: &str, filters: Filters) -> PlayerStats {
        let mut stats = PlayerStats {
            filters,
            read: Err(String::new()),
            played: vec![],
            row: 0,
        };
        stats.load(core, team, player);
        stats
    }

    pub fn load(&mut self, core: &Scout4All, team: &str, player: &str) {
        self.read = core
            .player_statistics(
                team.to_string(),
                player.to_string(),
                self.filters.period.clone(),
                self.filters.competition,
            )
            .map_err(|e| e.to_string());
        if let Ok(read) = &self.read {
            self.played = period::played_days(&read.played_days);
        }
    }

    /// A filter changed; the detail is opened by the page (false then).
    pub fn update(&mut self, core: &Scout4All, team: &str, player: &str, message: Message) {
        match message {
            Message::Period(message) => {
                let shown = self
                    .read
                    .as_ref()
                    .ok()
                    .and_then(|r| r.from.as_deref().zip(r.to.as_deref()));
                self.filters.choice.update(message, shown);
                if let Some(period) = self
                    .filters
                    .choice
                    .period()
                    .filter(|p| *p != self.filters.period)
                {
                    self.filters.period = period;
                    self.load(core, team, player);
                }
            }
            Message::Competition(competition) => {
                self.filters.competition = competition;
                self.load(core, team, player);
            }
            Message::Detail => {}
        }
        self.row = self.row.min(self.rows().len().saturating_sub(1));
    }

    /// Whether there's something to look at in detail.
    pub fn has_detail(&self) -> bool {
        self.read.as_ref().is_ok_and(|r| !r.fundamentals.is_empty())
    }

    /// The competitions are a filter with more than one, or one chosen.
    fn rows(&self) -> Vec<Filter> {
        let competitions = self.read.as_ref().is_ok_and(|r| r.competitions.len() > 1)
            || self.filters.competition.is_some();
        [Filter::Period]
            .into_iter()
            .chain(competitions.then_some(Filter::Competition))
            .collect()
    }

    /// ↑ ↓ between the filters, ← → change one, as in the analysis; the
    /// keys of the period on its row.
    pub fn key(&mut self, ck: KeyCombination) -> Option<Message> {
        let rows = self.rows();
        let on = rows.get(self.row).copied();
        if let (Some(Filter::Period), Some(message)) = (on, self.filters.choice.key(ck)) {
            return Some(Message::Period(message));
        }
        let count = rows.len().max(1);
        match ck {
            k if k == key!(down) => {
                self.row = (self.row + 1) % count;
                None
            }
            k if k == key!(up) => {
                self.row = (self.row + count - 1) % count;
                None
            }
            k if k == key!(left) || k == key!(right) => {
                let forward = k == key!(right);
                match on? {
                    Filter::Period => self.filters.choice.cycle(forward).map(Message::Period),
                    Filter::Competition => {
                        let ids: Vec<Option<Uuid>> = std::iter::once(None)
                            .chain(
                                self.read
                                    .iter()
                                    .flat_map(|r| r.competitions.iter().map(|c| Some(c.id))),
                            )
                            .collect();
                        ui::cycle(&ids, &self.filters.competition, forward)
                            .map(Message::Competition)
                    }
                }
            }
            k if k == key!(enter) && self.has_detail() => Some(Message::Detail),
            _ => None,
        }
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        let period = match self.rows().get(self.row) {
            Some(Filter::Period) => self.filters.choice.hints(s),
            Some(Filter::Competition) | None => vec![],
        };
        [
            ("↑ ↓".to_string(), s.filter.to_string()),
            ("← →".to_string(), s.change.to_string()),
        ]
        .into_iter()
        .chain(period)
        .chain(
            self.has_detail()
                .then(|| ("Enter".to_string(), s.detail.to_string())),
        )
        .collect()
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        responsive(move |size| {
            let columns = match size.width >= WIDE {
                true => 3,
                false => 2,
            };
            scrollable(self.page(columns, s, c))
                .height(Length::Fill)
                .into()
        })
        .into()
    }

    fn page<'a>(&'a self, columns: usize, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let rows = self.rows();
        let on = rows.get(self.row).copied();
        let small = |text_of: String| text(text_of).size(SIZE_SMALL).color(c.muted);
        let chosen: Element<'a, Message> = match &self.read {
            Ok(r) => small(period::caption(
                r.from.as_deref(),
                r.to.as_deref(),
                r.matches.len(),
                &r.skipped,
                s,
            ))
            .into(),
            Err(_) => space::vertical().height(0).into(),
        };
        let period = self.filters.choice.view(
            on == Some(Filter::Period),
            chosen,
            &self.played,
            Message::Period,
            s,
            c,
        );
        let competitions = match (&self.read, rows.contains(&Filter::Competition)) {
            (Ok(r), true) => Some(period::competition_line(
                on == Some(Filter::Competition),
                &r.competitions,
                self.filters.competition,
                Message::Competition,
                s,
                c,
            )),
            _ => None,
        };
        let content: Element<'a, Message> = match &self.read {
            Err(error) => notice::banner(Level::Error, error, c),
            Ok(r) if r.matches.is_empty() => small(s.no_matches_period.to_string()).into(),
            Ok(r) if r.fundamentals.is_empty() => small(s.no_events.to_string()).into(),
            Ok(r) => figures(r, columns, s, c),
        };
        let detail = self
            .has_detail()
            .then(|| ui::action(theme::icons::STATS, s.detail, Message::Detail, false, c));
        Column::new()
            .push(period)
            .push(competitions)
            .push(detail)
            .push(content)
            .spacing(14)
            .padding([8, 16])
            .into()
    }
}

/// The boxes, `columns` a row, then the trends.
fn figures<'a>(
    r: &'a PlayerStatistics,
    columns: usize,
    s: &'static Strings,
    c: Colors,
) -> Element<'a, Message> {
    let boxes = r.fundamentals.chunks(columns).map(|row| {
        let cells = (0..columns).map(|i| {
            let cell: Element<'a, Message> = match row.get(i) {
                Some(f) => figure(f, s, c),
                None => space::horizontal().into(),
            };
            container(cell).width(Length::FillPortion(1)).into()
        });
        Row::with_children(cells).spacing(12).into()
    });
    let days: Vec<String> = r.matches.iter().map(|m| short_day(s, &m.day)).collect();
    let opponents: Vec<String> = r.matches.iter().map(|m| m.opponent.clone()).collect();
    let trends = r.trend.iter().map(|player_trend| {
        let line = &player_trend.line;
        let trend = MatchesTrend::of(line, days.clone(), opponents.clone(), s.touches);
        let chart: Element<'a, Message> = match trend.is_empty() {
            true => text(s.no_rallies).size(SIZE_SMALL).color(c.muted).into(),
            false => charts::matches_view(trend, c),
        };
        ui::panel(
            Some(&format!(
                "{} · {}",
                ui::capitalized(&line.name),
                measure_name(player_trend.measure, s)
            )),
            column![
                matches_legend(r.matches.len(), line.overall, s, c),
                container(chart).height(Length::Fixed(CHART))
            ]
            .spacing(10),
            c,
        )
    });
    column![
        Column::with_children(boxes).spacing(12),
        Column::with_children(trends).spacing(12)
    ]
    .spacing(18)
    .into()
}

fn measure_name(measure: PlayerMeasure, s: &'static Strings) -> &'static str {
    match measure {
        PlayerMeasure::Efficiency => s.efficiency,
        PlayerMeasure::Positivity => s.positiveness,
    }
}

/// A fundamental: its figure large, the touches, the points (the aces of
/// the serve) and the errors.
fn figure<'a>(f: &PlayerFundamental, s: &'static Strings, c: Colors) -> Element<'a, Message> {
    let measure = measure_name(f.measure, s);
    let points = f.points.map(|points| {
        let name = match f.event {
            EventTypeEnum::S => s.aces,
            _ => s.points,
        };
        format!("{name} {points}")
    });
    let counts = points
        .into_iter()
        .chain([format!("{} {}", s.errors, f.errors)])
        .collect::<Vec<_>>()
        .join(" · ");
    let value = f
        .value
        .map_or_else(|| "–".to_string(), |v| format!("{v:.0}%"));
    ui::panel(
        Some(&ui::capitalized(&f.name)),
        column![
            text(value)
                .size(FIGURE)
                .font(theme::fonts().bold)
                .color(f.grade.map_or(c.text, |g| grade_color(g, c))),
            text(measure).size(SIZE_SMALL).color(c.muted),
            text(ui::fill(s.touches, &[f.total.to_string()])).size(SIZE_BODY),
            text(counts).size(SIZE_SMALL).color(c.muted),
        ]
        .spacing(4)
        .width(Length::Fill),
        c,
    )
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::stats::tests::{team_with_matches, team_with_two_matches},
        std::sync::Arc,
    };

    fn app(dir: &tempfile::TempDir) -> Arc<Scout4All> {
        Scout4All::for_tests(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap()
    }

    /// The statistics of the server of the matches (the first player).
    fn stats_of(core: &Scout4All, team: &str) -> (PlayerStats, String) {
        let player = core.team(team.into()).unwrap().players[0].id.clone();
        let today = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        let stats = PlayerStats::new(core, team, &player, Filters::season(today));
        (stats, player)
    }

    /// ↑ ↓ go round the filters, ← → on the competition go round its
    /// choices (all of them first), Enter opens the detail.
    #[test]
    fn the_arrows_go_round_the_filters_and_the_competitions() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = team_with_matches(&core, ["Coppa", "Trofeo"]);
        let (mut stats, player) = stats_of(&core, &team);
        let ids: Vec<Option<Uuid>> = std::iter::once(None)
            .chain(
                stats
                    .read
                    .iter()
                    .flat_map(|r| r.competitions.iter().map(|c| Some(c.id))),
            )
            .collect();
        assert_eq!(ids.len(), 3);
        assert_eq!(stats.rows(), [Filter::Period, Filter::Competition]);
        assert!(stats.key(key!(down)).is_none());
        assert_eq!(stats.row, 1);
        assert!(stats.key(key!(down)).is_none());
        assert_eq!(stats.row, 0);
        assert!(stats.key(key!(up)).is_none());
        assert_eq!(stats.row, 1);
        let mut chosen = vec![];
        for _ in 0..3 {
            let Some(Message::Competition(competition)) = stats.key(key!(right)) else {
                panic!("not a competition");
            };
            stats.update(&core, &team, &player, Message::Competition(competition));
            chosen.push(stats.filters.competition);
        }
        assert_eq!(chosen, [ids[1], ids[2], ids[0]]);
        assert!(matches!(
            stats.key(key!(left)),
            Some(Message::Competition(last)) if last == ids[2]
        ));
        assert!(matches!(stats.key(key!(enter)), Some(Message::Detail)));
    }

    /// One competition only: no row for it, the arrows stay on the period
    /// and change it.
    #[test]
    fn without_competitions_to_choose_the_arrows_change_the_period() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = team_with_two_matches(&core);
        let (mut stats, _) = stats_of(&core, &team);
        assert_eq!(stats.rows(), [Filter::Period]);
        assert!(stats.key(key!(down)).is_none());
        assert_eq!(stats.row, 0);
        assert!(matches!(
            stats.key(key!(right)),
            Some(Message::Period(period::Message::Kind(_)))
        ));
    }

    /// A chosen competition keeps its row even when the matches read are
    /// of it alone, so it can be taken back.
    #[test]
    fn a_competition_chosen_keeps_its_row() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = team_with_matches(&core, ["Coppa", "Trofeo"]);
        let (mut stats, player) = stats_of(&core, &team);
        let first = stats
            .read
            .as_ref()
            .ok()
            .and_then(|r| r.competitions.first())
            .map(|c| c.id);
        stats.update(&core, &team, &player, Message::Competition(first));
        assert_eq!(stats.rows(), [Filter::Period, Filter::Competition]);
        stats.row = 1;
        stats.update(&core, &team, &player, Message::Competition(None));
        assert_eq!(stats.filters.competition, None);
        assert_eq!(stats.row, 1);
    }
}
