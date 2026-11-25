//! The statistics of a team over several matches (see `analysis` in the
//! core): the matches of a period, their statistics together as for a
//! match, each figure match by match with its moving average, and the
//! choices of the filters on the situation.
use super::{
    ClipPick, CompetitionChoice, DetailFilter, DetailStats, GradeBand, LiveStats, MobileError,
    RoleChoice, Scout4All, ScoutingPlayer, StatsScope, choosable, clips_of, detail_of, narrowed,
    series_name, summary, touched,
};
use chrono::{Local, NaiveDate};
use scout4all_core::player_form::name_the_removed;
use scout4all_core::providers::match_reader::MatchReader;
use scout4all_core::{
    analysis::{Analysis, MOVING_WINDOW, Period, Played, competitions_of, day},
    benchmark::{band, grade},
    shapes::{
        enums::{EvalEnum, EventTypeEnum, FriendlyName, RoleEnum},
        flags::ReceptionSchemeEnum,
        r#match::{MatchEntry, MatchSummary},
        stats::{EvalSet, Stats, StatsFilter},
        team::TeamEntry,
        technique::{SetTypeEnum, TechniqueEnum},
    },
    trend::Series,
    video::{EventRef, StoredProfile, VideoProfiles},
};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// The matches to analyse: only the ones with a set scouted, up to today.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum AnalysisPeriod {
    /// from the same day of the month before, today included
    LastMonth,
    /// every one: a team lasts one season
    Season,
    /// the last ones played
    LastMatches { count: u32 },
    /// both days included, as `YYYY-MM-DD`
    Between { from: String, to: String },
}

/// A match of the analysis.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AnalysedMatch {
    pub id: String,
    pub opponent: String,
    /// RFC 3339
    pub date: String,
    /// `YYYY-MM-DD`, where it was played
    pub day: String,
    pub home: bool,
    pub won: u8,
    pub lost: u8,
}

/// A figure match by match.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MatchTrend {
    pub series: Series,
    pub name: String,
    pub band: Option<GradeBand>,
    /// in each match, `None` with no touches
    pub values: Vec<Option<f64>>,
    /// of each match with the ones before (`MOVING_WINDOW` together)
    pub moving: Vec<Option<f64>>,
    pub touches: Vec<u32>,
    /// in all the matches, and its grade for the level of the team
    pub overall: Option<f64>,
    pub overall_grade: Option<f64>,
}

/// Evaluations chosen together: the good ones (`#+`), `!`, the bad ones
/// keeping the ball or giving it to them (`-/`).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct EvalGroup {
    pub evals: Vec<EvalEnum>,
    /// the symbols, e.g. `#+`
    pub symbols: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SymbolChoice {
    pub value: EvalEnum,
    pub symbol: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SetTypeChoice {
    pub value: SetTypeEnum,
    /// as scouted, e.g. `7`, `C`, `F`
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SchemeChoice {
    pub value: ReceptionSchemeEnum,
    pub receivers: u8,
}

/// What the filters on the situation offer.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SituationChoices {
    /// the attackers first
    pub roles: Vec<RoleChoice>,
    pub groups: Vec<EvalGroup>,
    /// the reception and the touch before: `#`, `+`, `!`, `-`, `/`
    pub symbols: Vec<SymbolChoice>,
    pub set_types: Vec<SetTypeChoice>,
    pub schemes: Vec<SchemeChoice>,
}

/// The statistics of the team in the period.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TeamAnalysis {
    pub period: AnalysisPeriod,
    /// the competition the matches are narrowed to, `None` for every one
    pub competition: Option<Uuid>,
    /// the competitions of the matches scouted, to choose from
    pub competitions: Vec<CompetitionChoice>,
    /// the days of the period (`YYYY-MM-DD`), `None` for the last matches
    /// with none
    pub from: Option<String>,
    pub to: Option<String>,
    /// in order of date
    pub matches: Vec<AnalysedMatch>,
    /// the opponents of the matches left out: a set doesn't replay
    pub skipped: Vec<String>,
    /// every day with a match played by the team, to mark on a calendar
    pub played_days: Vec<String>,
    /// as for a match, narrowed by the filter
    pub summary: LiveStats,
    pub trend: Vec<MatchTrend>,
    pub detail: DetailStats,
    /// the players with a touch of any fundamental in the matches (a
    /// libero only receiving and digging too), and the ones chosen even
    /// with none, by their number: the filter of every page but the
    /// detail, which offers the players of its fundamental
    pub players: Vec<ScoutingPlayer>,
    pub situation: SituationChoices,
}

/// An opponent scouted on both sides, to analyse from its side.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ScoutedOpponent {
    pub id: Uuid,
    pub name: String,
    /// the matches against it scouted on both sides
    pub matches: u32,
}

/// The figure a fundamental of a player is told by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PlayerMeasure {
    Efficiency,
    Positivity,
}

/// A fundamental the player touched, as the detail of the analysis tells
/// it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlayerFundamental {
    pub event: EventTypeEnum,
    pub name: String,
    pub measure: PlayerMeasure,
    /// in percent
    pub value: Option<f64>,
    /// of an efficiency, for the level of the team
    pub grade: Option<f64>,
    pub total: u32,
    /// the points scored (the aces of the serve), only for an attack, a
    /// serve or a block
    pub points: Option<u32>,
    pub errors: u32,
}

/// The statistics of a player of a team over the matches of a period: the
/// analysis of the team narrowed to the player.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlayerStatistics {
    pub period: AnalysisPeriod,
    pub competition: Option<Uuid>,
    /// the competitions of the matches scouted, to choose from
    pub competitions: Vec<CompetitionChoice>,
    /// the days of the period (`YYYY-MM-DD`), `None` with no matches
    pub from: Option<String>,
    pub to: Option<String>,
    /// in order of date
    pub matches: Vec<AnalysedMatch>,
    /// the opponents of the matches left out: a set doesn't replay
    pub skipped: Vec<String>,
    /// every day with a match played by the team, to mark on a calendar
    pub played_days: Vec<String>,
    /// attack, reception, serve, block, dig: only the ones touched
    pub fundamentals: Vec<PlayerFundamental>,
    /// match by match, of the fundamentals touched that have a trend:
    /// attack, reception, serve
    pub trend: Vec<MatchTrend>,
}

/// The fundamentals of a player, in the order they're shown, with the
/// figure of each and its trend, when the analysis has one.
const PLAYER_FUNDAMENTALS: [(EventTypeEnum, PlayerMeasure, Option<Series>); 5] = [
    (
        EventTypeEnum::A,
        PlayerMeasure::Efficiency,
        Some(Series::Attack),
    ),
    (
        EventTypeEnum::P,
        PlayerMeasure::Positivity,
        Some(Series::Reception),
    ),
    (
        EventTypeEnum::S,
        PlayerMeasure::Efficiency,
        Some(Series::Serve),
    ),
    (EventTypeEnum::B, PlayerMeasure::Efficiency, None),
    (EventTypeEnum::D, PlayerMeasure::Positivity, None),
];

const DAY: &str = "%Y-%m-%d";

fn parse_day(text: &str) -> Result<NaiveDate, MobileError> {
    NaiveDate::parse_from_str(text.trim(), DAY).map_err(|_| MobileError::Failed {
        reason: format!("not a day: {text}"),
    })
}

impl AnalysisPeriod {
    fn period(&self) -> Result<Period, MobileError> {
        Ok(match self {
            AnalysisPeriod::LastMonth => Period::LastMonth,
            AnalysisPeriod::Season => Period::Season,
            AnalysisPeriod::LastMatches { count } => Period::LastMatches(*count),
            AnalysisPeriod::Between { from, to } => {
                let (from, to) = (parse_day(from)?, parse_day(to)?);
                // the days the other way round: the same period
                Period::Between {
                    from: from.min(to),
                    to: from.max(to),
                }
            }
        })
    }
}

#[uniffi::export]
impl Scout4All {
    /// The statistics of the team in the period, narrowed by the filter;
    /// the detail of the fundamental.
    pub fn team_analysis(
        &self,
        team_id: String,
        period: AnalysisPeriod,
        competition: Option<Uuid>,
        event: EventTypeEnum,
        filter: DetailFilter,
    ) -> Result<TeamAnalysis, MobileError> {
        let team = self.read_team(&team_id)?;
        let today = Local::now().date_naive();
        let (all, chosen) = self.chosen_matches(&team, &period, competition, today)?;
        let competitions = self.competitions_played(&all, today)?;
        team_analysis(
            &team,
            &all,
            &chosen,
            period,
            competition,
            competitions,
            today,
            event,
            filter,
            self.language.labels(),
        )
    }

    /// The statistics of the player in the matches of the team in the
    /// period, from either side.
    pub fn player_statistics(
        &self,
        team_id: String,
        player_id: String,
        period: AnalysisPeriod,
        competition: Option<Uuid>,
    ) -> Result<PlayerStatistics, MobileError> {
        let team = self.read_team(&team_id)?;
        let today = Local::now().date_naive();
        let (all, chosen) = self.chosen_matches(&team, &period, competition, today)?;
        let competitions = self.competitions_played(&all, today)?;
        let labels = self.language.labels();
        let filter = DetailFilter {
            players: vec![player_id],
            ..DetailFilter::default()
        };
        let narrowed = narrowed(&filter)?;
        let matches: Vec<&MatchEntry> = chosen.iter().collect();
        let days = period.period()?.days(&matches, today);
        let analysis = Analysis::of(&matches);
        let total = analysis.total();
        let teams: Vec<&TeamEntry> = std::iter::once(&team)
            .chain(analysis.matches.iter().map(|m| &m.entry.team))
            .collect();
        let mut fundamentals = vec![];
        let mut trend = vec![];
        for (event, measure, series) in PLAYER_FUNDAMENTALS {
            let detail = detail_of(&total, &total, &teams, labels, event, filter.clone())?;
            let Some(d) = detail.fundamental.filter(|d| d.total > 0) else {
                continue;
            };
            let (value, grade) = match measure {
                PlayerMeasure::Efficiency => (d.efficiency, d.grade),
                PlayerMeasure::Positivity => (d.positiveness, None),
            };
            fundamentals.push(PlayerFundamental {
                event,
                name: event.friendly_name(labels).to_string(),
                measure,
                value,
                grade,
                total: d.total,
                points: d.points,
                errors: d.errors,
            });
            trend.extend(
                series.map(|s| match_trend(s, &analysis, &total, &narrowed, &team, labels)),
            );
        }
        Ok(PlayerStatistics {
            period,
            competition,
            competitions,
            from: days.map(|(from, _)| from.format(DAY).to_string()),
            to: days.map(|(_, to)| to.format(DAY).to_string()),
            matches: analysed_matches(&analysis, &all),
            skipped: skipped_opponents(&analysis),
            played_days: played_days(&all),
            fundamentals,
            trend,
        })
    }

    /// The teams the team has played scouted on both sides, to analyse
    /// from their side (`opponent_analysis`): by name, then by id.
    pub fn analysis_opponents(&self, team_id: String) -> Result<Vec<ScoutedOpponent>, MobileError> {
        let team = self.read_team(&team_id)?;
        let mut found: Vec<ScoutedOpponent> = vec![];
        for m in self.match_reader.both_matches(&team)? {
            match found.iter_mut().find(|o| o.id == m.team_b) {
                Some(o) => o.matches += 1,
                None => found.push(ScoutedOpponent {
                    id: m.team_b,
                    name: m.name_b,
                    matches: 1,
                }),
            }
        }
        found.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
        Ok(found)
    }

    /// The statistics of team B `opponent` in the matches of the team
    /// scouted on both sides, as `team_analysis` tells those of the team:
    /// the matches seen from B (its opponent is the team, the sets won the
    /// other way).
    pub fn opponent_analysis(
        &self,
        team_id: String,
        opponent: Uuid,
        period: AnalysisPeriod,
        competition: Option<Uuid>,
        event: EventTypeEnum,
        filter: DetailFilter,
    ) -> Result<TeamAnalysis, MobileError> {
        let team = self.read_team(&team_id)?;
        let team_b = self.read_team(&opponent.to_string())?;
        let today = Local::now().date_naive();
        let (all, chosen) =
            self.chosen_matches_of_b(&team, &opponent, &period, competition, today)?;
        let competitions = self.competitions_played(&all, today)?;
        team_analysis(
            &team_b,
            &all,
            &chosen,
            period,
            competition,
            competitions,
            today,
            event,
            filter,
            self.language.labels(),
        )
    }

    /// The statistics of the team in the matches scouted on both sides
    /// against `opponent` (the same ones `opponent_analysis` takes, seen
    /// from the team), to compare with those of the opponent.
    pub fn analysis_against(
        &self,
        team_id: String,
        opponent: Uuid,
        period: AnalysisPeriod,
        competition: Option<Uuid>,
        event: EventTypeEnum,
        filter: DetailFilter,
    ) -> Result<TeamAnalysis, MobileError> {
        let team = self.read_team(&team_id)?;
        let today = Local::now().date_naive();
        let against = self.both_against(&team, &opponent)?;
        let (all, chosen) = self.chosen_matches_among(&team, &period, competition, today, |m| {
            against.contains_key(&m.id)
        })?;
        let competitions = self.competitions_played(&all, today)?;
        team_analysis(
            &team,
            &all,
            &chosen,
            period,
            competition,
            competitions,
            today,
            event,
            filter,
            self.language.labels(),
        )
    }

    /// What the filters on the situation offer, also for a match.
    pub fn situation_choices(&self) -> SituationChoices {
        situation_choices(self.language.labels())
    }
}

/// The actions of a match of the period picked in the statistics of the
/// team, to watch in its video (desktop only).
#[derive(Debug, Clone, PartialEq)]
pub struct MatchClips {
    pub match_id: String,
    pub opponent: String,
    /// `YYYY-MM-DD`, where it was played
    pub day: String,
    pub events: Vec<EventRef>,
    /// the match has files of its video
    pub has_video: bool,
}

/// For the apps written in Rust (the desktop app), not exported to the
/// bindings.
impl Scout4All {
    /// The actions picked of each match of the period with some, in the
    /// order of the analysis (the scope of the pick left aside: every set).
    pub fn team_clips(
        &self,
        team_id: &str,
        period: &AnalysisPeriod,
        competition: Option<Uuid>,
        pick: &ClipPick,
    ) -> Result<Vec<MatchClips>, MobileError> {
        let team = self.read_team(team_id)?;
        let (_, chosen) =
            self.chosen_matches(&team, period, competition, Local::now().date_naive())?;
        // the actions of a match are watched from the page of its team A
        let own: HashSet<String> = self
            .runtime
            .block_on(self.match_reader.read_summaries(&team))?
            .into_iter()
            .filter_map(Result::ok)
            .map(|m| m.id)
            .collect();
        let matches: Vec<&MatchEntry> = chosen.iter().filter(|m| own.contains(&m.id)).collect();
        let analysis = Analysis::of(&matches);
        let pick = ClipPick {
            scope: StatsScope::Match,
            ..pick.clone()
        };
        let mut found = vec![];
        for m in analysis.matches.iter().map(|m| m.entry) {
            let events = clips_of(m, &pick)?;
            if events.is_empty() {
                continue;
            }
            let profile = self.runtime.block_on(self.videos.read(&m.team.id, &m.id));
            found.push(MatchClips {
                match_id: m.id.clone(),
                opponent: m.opponent.clone(),
                day: day(m).format(DAY).to_string(),
                events,
                has_video: matches!(profile, Ok(StoredProfile::Profile(p)) if !p.files.is_empty()),
            });
        }
        Ok(found)
    }

    /// The competitions of the matches scouted up to today, by name (the
    /// most recent season first): the ones the filter offers.
    fn competitions_played(
        &self,
        all: &[MatchSummary],
        today: NaiveDate,
    ) -> Result<Vec<CompetitionChoice>, MobileError> {
        let played = competitions_of(all, today);
        Ok(self
            .competitions()?
            .into_iter()
            .filter(|c| played.contains(&c.id))
            .collect())
    }

    /// The matches of `team` scouted on both sides against `opponent`: by
    /// id, whether B played them at home.
    fn both_against(
        &self,
        team: &TeamEntry,
        opponent: &Uuid,
    ) -> Result<HashMap<String, bool>, MobileError> {
        Ok(self
            .match_reader
            .both_matches(team)?
            .into_iter()
            .filter(|m| m.team_b == *opponent)
            .map(|m| (m.id, m.home_b))
            .collect())
    }

    /// As `chosen_matches`, for team B `opponent` in the matches of `team`
    /// scouted on both sides: the summaries as B sees them, the matches
    /// read from B's side.
    fn chosen_matches_of_b(
        &self,
        team: &TeamEntry,
        opponent: &Uuid,
        period: &AnalysisPeriod,
        competition: Option<Uuid>,
        today: NaiveDate,
    ) -> Result<(Vec<MatchSummary>, Vec<MatchEntry>), MobileError> {
        let against = self.both_against(team, opponent)?;
        let all: Vec<MatchSummary> = self
            .runtime
            .block_on(self.match_reader.read_summaries(team))?
            .into_iter()
            .filter_map(Result::ok)
            .filter_map(|m| {
                against
                    .get(&m.id)
                    .map(|home_b| m.seen_by_b(team.name.clone(), *home_b))
            })
            .collect();
        let chosen = period
            .period()?
            .select_in(&all, today, competition)
            .into_iter()
            .filter_map(|m| self.match_reader.read_of_b(team, &m.id).ok().flatten())
            .map(|mut m: MatchEntry| {
                name_the_removed(&mut m.team, self.language.labels());
                m
            })
            .collect();
        Ok((all, chosen))
    }

    /// The matches of the team as the index lists them (the readable
    /// ones), and the ones of the period read in full, in its order: only
    /// those are read and replayed. One changed meanwhile so that it can't
    /// be read is left out.
    fn chosen_matches(
        &self,
        team: &TeamEntry,
        period: &AnalysisPeriod,
        competition: Option<Uuid>,
        today: NaiveDate,
    ) -> Result<(Vec<MatchSummary>, Vec<MatchEntry>), MobileError> {
        self.chosen_matches_among(team, period, competition, today, |_| true)
    }

    /// As `chosen_matches`, among the matches `keep` keeps only.
    fn chosen_matches_among(
        &self,
        team: &TeamEntry,
        period: &AnalysisPeriod,
        competition: Option<Uuid>,
        today: NaiveDate,
        keep: impl Fn(&MatchSummary) -> bool,
    ) -> Result<(Vec<MatchSummary>, Vec<MatchEntry>), MobileError> {
        let (all, team_a): (Vec<MatchSummary>, Vec<Option<TeamEntry>>) = self
            .runtime
            .block_on(self.match_reader.scouted_summaries(team))?
            .into_iter()
            .filter(|m| keep(&m.summary))
            .map(|m| (m.summary, m.team_a))
            .unzip();
        // the matches the team is B of, with their team A
        let as_b: HashMap<&str, &TeamEntry> = all
            .iter()
            .zip(&team_a)
            .filter_map(|(m, a)| a.as_ref().map(|a| (m.id.as_str(), a)))
            .collect();
        let chosen = period
            .period()?
            .select_in(&all, today, competition)
            .into_iter()
            .filter_map(|m| match as_b.get(m.id.as_str()) {
                Some(a) => self.match_reader.read_of_b(a, &m.id).ok().flatten(),
                None => self
                    .runtime
                    .block_on(
                        self.match_reader
                            .read_some(team, std::slice::from_ref(&m.id)),
                    )
                    .into_iter()
                    .find_map(Result::ok),
            })
            .map(|mut m: MatchEntry| {
                name_the_removed(&mut m.team, self.language.labels());
                m
            })
            .collect();
        Ok((all, chosen))
    }
}

/// The analysis of the matches of the team (see `Scout4All::team_analysis`).
pub(crate) fn team_analysis(
    team: &TeamEntry,
    all: &[MatchSummary],
    chosen: &[MatchEntry],
    period: AnalysisPeriod,
    competition: Option<Uuid>,
    competitions: Vec<CompetitionChoice>,
    today: NaiveDate,
    event: EventTypeEnum,
    filter: DetailFilter,
    labels: &scout4all_core::localization::Labels,
) -> Result<TeamAnalysis, MobileError> {
    let matches: Vec<&MatchEntry> = chosen.iter().collect();
    let days = period.period()?.days(&matches, today);
    let analysis = Analysis::of(&matches);
    let narrowed = narrowed(&filter)?;
    let total = analysis.total();
    // the players as the team is now, then as each match saw them (the
    // ones removed since)
    let teams: Vec<&TeamEntry> = std::iter::once(team)
        .chain(analysis.matches.iter().map(|m| &m.entry.team))
        .collect();
    let trend = Series::ALL
        .iter()
        .map(|series| match_trend(*series, &analysis, &total, &narrowed, team, labels))
        .collect();
    let played_days = played_days(all);
    Ok(TeamAnalysis {
        period,
        competition,
        competitions,
        from: days.map(|(from, _)| from.format(DAY).to_string()),
        to: days.map(|(_, to)| to.format(DAY).to_string()),
        matches: analysed_matches(&analysis, all),
        skipped: skipped_opponents(&analysis),
        played_days,
        summary: summary(&total, &narrowed, team, labels),
        trend,
        players: choosable(touched(&total), &narrowed, &teams, labels),
        detail: detail_of(&total, &total, &teams, labels, event, filter)?,
        situation: situation_choices(labels),
    })
}

/// The matches of the analysis, the score of each as the index tells it.
pub(crate) fn analysed_matches(analysis: &Analysis, all: &[MatchSummary]) -> Vec<AnalysedMatch> {
    let scores: HashMap<&str, (u8, u8)> = all
        .iter()
        .map(|m| (m.id.as_str(), (m.us_wins, m.them_wins)))
        .collect();
    analysis
        .matches
        .iter()
        .map(|m| {
            let (won, lost) = scores.get(m.entry.id.as_str()).copied().unwrap_or_default();
            AnalysedMatch {
                id: m.entry.id.clone(),
                opponent: m.entry.opponent.clone(),
                date: m.entry.date.to_rfc3339(),
                day: day(m.entry).format(DAY).to_string(),
                home: m.entry.home,
                won,
                lost,
            }
        })
        .collect()
}

/// The opponents of the matches left out: a set doesn't replay.
pub(crate) fn skipped_opponents(analysis: &Analysis) -> Vec<String> {
    analysis
        .skipped
        .iter()
        .map(|m| m.opponent.clone())
        .collect()
}

/// Every day with a match played, once each.
pub(crate) fn played_days(all: &[MatchSummary]) -> Vec<String> {
    let mut days: Vec<String> = all
        .iter()
        .filter(|m| m.scouted())
        .map(|m| day(m).format(DAY).to_string())
        .collect();
    days.sort();
    days.dedup();
    days
}

/// A figure match by match, narrowed by the filter, graded for the level
/// of the team.
pub(crate) fn match_trend(
    series: Series,
    analysis: &Analysis,
    total: &Stats,
    narrowed: &StatsFilter,
    team: &TeamEntry,
    labels: &scout4all_core::localization::Labels,
) -> MatchTrend {
    let overall = series.value_in(total, narrowed);
    MatchTrend {
        series,
        name: series_name(series, labels),
        band: team
            .classification
            .zip(team.gender)
            .map(|(level, gender)| band(level, gender, series.measure()))
            .map(|b| GradeBand {
                low: b.low,
                high: b.high,
            }),
        values: analysis.series(series, narrowed),
        moving: analysis.moving(series, narrowed, MOVING_WINDOW),
        touches: analysis.touches(series, narrowed),
        overall,
        overall_grade: overall
            .and_then(|v| grade(team.classification, team.gender, series.measure(), v)),
    }
}

/// The choices of the filters.
fn situation_choices(labels: &scout4all_core::localization::Labels) -> SituationChoices {
    use EvalEnum::*;
    let group = |set: EvalSet| {
        let evals: Vec<EvalEnum> = [Perfect, Positive, Exclamative, Negative, Over]
            .into_iter()
            .filter(|e| set.contains(*e))
            .collect();
        EvalGroup {
            symbols: evals.iter().map(|e| e.to_string()).collect(),
            evals,
        }
    };
    SituationChoices {
        roles: [
            RoleEnum::OutsideHitter,
            RoleEnum::OppositeHitter,
            RoleEnum::MiddleBlocker,
            RoleEnum::Setter,
            RoleEnum::Libero,
        ]
        .into_iter()
        .map(|r| RoleChoice {
            value: r,
            name: r.friendly_name(labels).to_string(),
        })
        .collect(),
        groups: [EvalSet::POSITIVE, EvalSet::NEGATIVE]
            .into_iter()
            .map(group)
            .collect(),
        symbols: [Perfect, Positive, Exclamative, Negative, Over]
            .into_iter()
            .map(|e| SymbolChoice {
                value: e,
                symbol: e.to_string(),
            })
            .collect(),
        set_types: SetTypeEnum::ALL
            .into_iter()
            .map(|t| SetTypeChoice {
                value: t,
                code: TechniqueEnum::Set(t).to_string(),
                name: TechniqueEnum::Set(t).description(labels),
            })
            .collect(),
        schemes: ReceptionSchemeEnum::ALL
            .into_iter()
            .map(|s| SchemeChoice {
                value: s,
                receivers: s.players(),
            })
            .collect(),
    }
}
