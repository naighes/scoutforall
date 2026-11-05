//! The statistics of a team over several matches: the matches of a period
//! (the last month, the last matches, between two dates), their statistics
//! together, and each figure match by match with its moving average, to
//! see how the team goes.
use crate::errors::AppError;
use crate::shapes::r#match::MatchEntry;
use crate::shapes::set::SetEntry;
use crate::shapes::stats::{Stats, StatsFilter};
use crate::trend::Series;
use chrono::{Months, NaiveDate};

/// The matches the moving average looks at: the match and the two before.
pub const MOVING_WINDOW: usize = 3;

/// The matches to analyse: only the ones with a set scouted, up to today.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Period {
    /// from the same day of the month before, today included
    #[default]
    LastMonth,
    /// the last ones played
    LastMatches(u32),
    /// both days included
    Between { from: NaiveDate, to: NaiveDate },
}

impl Period {
    /// The days of the period, first and last: for the last matches, of
    /// the ones chosen; `None` with no matches.
    pub fn days(
        &self,
        matches: &[&MatchEntry],
        today: NaiveDate,
    ) -> Option<(NaiveDate, NaiveDate)> {
        match self {
            Period::LastMonth => Some((month_before(today), today)),
            Period::Between { from, to } => Some((*from, *to)),
            Period::LastMatches(_) => matches
                .first()
                .zip(matches.last())
                .map(|(first, last)| (day(first), day(last))),
        }
    }

    /// The matches of the period, from the first played.
    pub fn select<'a>(&self, matches: &'a [MatchEntry], today: NaiveDate) -> Vec<&'a MatchEntry> {
        let mut played: Vec<&MatchEntry> = matches
            .iter()
            .filter(|m| !m.sets.is_empty() && day(m) <= today)
            .collect();
        played.sort_by_key(|m| m.date);
        match self {
            Period::LastMonth => {
                let from = month_before(today);
                played.retain(|m| day(m) >= from);
                played
            }
            Period::Between { from, to } => {
                played.retain(|m| (*from..=*to).contains(&day(m)));
                played
            }
            Period::LastMatches(n) => {
                let skip = played.len().saturating_sub(*n as usize);
                played.split_off(skip)
            }
        }
    }
}

/// The day of the match, where it was played.
pub fn day(m: &MatchEntry) -> NaiveDate {
    m.date.date_naive()
}

/// The same day of the month before (the last day of that month when
/// it's shorter).
fn month_before(today: NaiveDate) -> NaiveDate {
    today.checked_sub_months(Months::new(1)).unwrap_or(today)
}

/// A match of the analysis, with its statistics.
#[derive(Debug, Clone)]
pub struct MatchStats<'a> {
    pub entry: &'a MatchEntry,
    pub stats: Stats,
}

/// The matches of a period with their statistics, in order of date.
#[derive(Debug, Clone, Default)]
pub struct Analysis<'a> {
    pub matches: Vec<MatchStats<'a>>,
    /// the matches whose sets don't replay, left out
    pub skipped: Vec<&'a MatchEntry>,
}

impl<'a> Analysis<'a> {
    /// The statistics of each match: a match with a set that doesn't
    /// replay is left out, not the whole analysis.
    pub fn of(matches: &[&'a MatchEntry]) -> Self {
        let (ok, skipped): (Vec<_>, Vec<_>) = matches
            .iter()
            .map(|m| (*m, match_stats(m)))
            .partition(|(_, stats)| stats.is_ok());
        Analysis {
            matches: ok
                .into_iter()
                .filter_map(|(entry, stats)| stats.ok().map(|stats| MatchStats { entry, stats }))
                .collect(),
            skipped: skipped.into_iter().map(|(m, _)| m).collect(),
        }
    }

    /// The statistics of all the matches together.
    pub fn total(&self) -> Stats {
        self.matches.iter().fold(Stats::new(), |mut all, m| {
            all.merge(&m.stats);
            all
        })
    }

    /// The figure in each match, `None` in a match with no touches.
    pub fn series(&self, series: Series, filter: &StatsFilter) -> Vec<Option<f64>> {
        self.matches
            .iter()
            .map(|m| series.value_in(&m.stats, filter))
            .collect()
    }

    /// The touches of the figure in each match.
    pub fn touches(&self, series: Series, filter: &StatsFilter) -> Vec<u32> {
        self.matches
            .iter()
            .map(|m| series.touches_in(&m.stats, filter))
            .collect()
    }

    /// The figure of each match together with the ones before it, up to
    /// `window` matches: the touches of all of them counted together (a
    /// match with few touches weighs little), `None` with none.
    pub fn moving(&self, series: Series, filter: &StatsFilter, window: usize) -> Vec<Option<f64>> {
        (0..self.matches.len())
            .map(|i| {
                let from = (i + 1).saturating_sub(window.max(1));
                let together = self.matches.get(from..=i).unwrap_or_default().iter().fold(
                    Stats::new(),
                    |mut all, m| {
                        all.merge(&m.stats);
                        all
                    },
                );
                series.value_in(&together, filter)
            })
            .collect()
    }
}

/// The statistics of a match: its sets together.
pub fn match_stats(m: &MatchEntry) -> Result<Stats, AppError> {
    let mut sets: Vec<&SetEntry> = m.sets.iter().collect();
    sets.sort_by_key(|s| s.set_number);
    sets.iter().try_fold(Stats::new(), |mut all, set| {
        all.merge(&set.compute_snapshot(m.max_substitutions)?.stats);
        Ok(all)
    })
}
