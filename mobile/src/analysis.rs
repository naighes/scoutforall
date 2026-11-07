//! The statistics of a team over several matches (see `analysis` in the
//! core): the matches of a period, their statistics together as for a
//! match, each figure match by match with its moving average, and the
//! choices of the filters on the situation.
use super::{
    ClipPick, DetailFilter, DetailStats, GradeBand, LiveStats, MobileError, RoleChoice, Scout4All,
    StatsScope, clips_of, detail_of, narrowed, series_name, summary,
};
use chrono::{Local, NaiveDate};
use scout4all_core::providers::match_reader::MatchReader;
use scout4all_core::{
    analysis::{Analysis, MOVING_WINDOW, Period, Played, day},
    benchmark::{band, grade},
    shapes::{
        enums::{EvalEnum, EventTypeEnum, FriendlyName, RoleEnum},
        flags::ReceptionSchemeEnum,
        r#match::{MatchEntry, MatchSummary},
        stats::EvalSet,
        team::TeamEntry,
        technique::{SetTypeEnum, TechniqueEnum},
    },
    trend::Series,
    video::{EventRef, StoredProfile, read_profile},
};
use std::collections::HashMap;

/// The matches to analyse: only the ones with a set scouted, up to today.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum AnalysisPeriod {
    /// from the same day of the month before, today included
    LastMonth,
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
    pub situation: SituationChoices,
}

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
        event: EventTypeEnum,
        filter: DetailFilter,
    ) -> Result<TeamAnalysis, MobileError> {
        let team = self.read_team(&team_id)?;
        let today = Local::now().date_naive();
        let (all, chosen) = self.chosen_matches(&team, &period, today)?;
        team_analysis(
            &team,
            &all,
            &chosen,
            period,
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
        pick: &ClipPick,
    ) -> Result<Vec<MatchClips>, MobileError> {
        let team = self.read_team(team_id)?;
        let (_, chosen) = self.chosen_matches(&team, period, Local::now().date_naive())?;
        let matches: Vec<&MatchEntry> = chosen.iter().collect();
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
            let profile = self
                .runtime
                .block_on(read_profile(&*self.store, &m.team.id, &m.id));
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

    /// The matches of the team as the index lists them (the readable
    /// ones), and the ones of the period read in full, in its order: only
    /// those are read and replayed. One changed meanwhile so that it can't
    /// be read is left out.
    fn chosen_matches(
        &self,
        team: &TeamEntry,
        period: &AnalysisPeriod,
        today: NaiveDate,
    ) -> Result<(Vec<MatchSummary>, Vec<MatchEntry>), MobileError> {
        let all: Vec<MatchSummary> = self
            .runtime
            .block_on(self.match_reader.read_summaries(team))?
            .into_iter()
            .filter_map(Result::ok)
            .collect();
        let ids: Vec<String> = period
            .period()?
            .select(&all, today)
            .into_iter()
            .map(|m| m.id.clone())
            .collect();
        let chosen = self
            .runtime
            .block_on(self.match_reader.read_some(team, &ids))
            .into_iter()
            .filter_map(Result::ok)
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
    today: NaiveDate,
    event: EventTypeEnum,
    filter: DetailFilter,
    labels: &scout4all_core::localization::Labels,
) -> Result<TeamAnalysis, MobileError> {
    let matches: Vec<&MatchEntry> = chosen.iter().collect();
    let days = period.period()?.days(&matches, today);
    // the score of each match, as the index tells it
    let scores: HashMap<&str, (u8, u8)> = all
        .iter()
        .map(|m| (m.id.as_str(), (m.us_wins, m.them_wins)))
        .collect();
    let analysis = Analysis::of(&matches);
    let narrowed = narrowed(&filter)?;
    let total = analysis.total();
    // the players as the team is now, then as each match saw them (the
    // ones removed since)
    let teams: Vec<&TeamEntry> = std::iter::once(team)
        .chain(analysis.matches.iter().map(|m| &m.entry.team))
        .collect();
    let level = |series: Series, value: Option<f64>| {
        value.and_then(|v| grade(team.classification, team.gender, series.measure(), v))
    };
    let trend = Series::ALL
        .iter()
        .map(|series| {
            let overall = series.value_in(&total, &narrowed);
            MatchTrend {
                series: *series,
                name: series_name(*series, labels),
                band: team
                    .classification
                    .zip(team.gender)
                    .map(|(level, gender)| band(level, gender, series.measure()))
                    .map(|b| GradeBand {
                        low: b.low,
                        high: b.high,
                    }),
                values: analysis.series(*series, &narrowed),
                moving: analysis.moving(*series, &narrowed, MOVING_WINDOW),
                touches: analysis.touches(*series, &narrowed),
                overall,
                overall_grade: level(*series, overall),
            }
        })
        .collect();
    let mut played_days: Vec<String> = all
        .iter()
        .filter(|m| m.scouted())
        .map(|m| day(m).format(DAY).to_string())
        .collect();
    played_days.sort();
    played_days.dedup();
    Ok(TeamAnalysis {
        period,
        from: days.map(|(from, _)| from.format(DAY).to_string()),
        to: days.map(|(_, to)| to.format(DAY).to_string()),
        matches: analysis
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
            .collect(),
        skipped: analysis
            .skipped
            .iter()
            .map(|m| m.opponent.clone())
            .collect(),
        played_days,
        summary: summary(&total, &narrowed, team, labels),
        trend,
        detail: detail_of(&total, &total, &teams, labels, event, filter)?,
        situation: situation_choices(labels),
    })
}

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
        groups: [EvalSet::POSITIVE, EvalSet::EXCLAMATIVE, EvalSet::NEGATIVE]
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
