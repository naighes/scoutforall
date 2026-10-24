//! How the statistics went point by point: the sets are replayed, and after
//! every rally the statistics are sampled, to draw their trend. Either all
//! of them so far, or only the last touches of a fundamental (e.g. the last
//! 10 receptions), to see a drop as it comes.
use crate::benchmark::Measure;
use crate::errors::AppError;
use crate::shapes::enums::{EventTypeEnum, PhaseEnum};
use crate::shapes::set::SetEntry;
use crate::shapes::snapshot::Snapshot;
use crate::shapes::stats::{Metric, Stats, StatsFilter};

/// The touches the recent trend looks at: the last 10 of each fundamental.
pub const RECENT_TOUCHES: u32 = 10;

/// A figure the coach follows along the match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Series {
    /// the efficiencies
    Reception,
    Attack,
    CounterAttack,
    Serve,
    /// the share of the rallies won
    SideOut,
    BreakPoint,
}

impl Series {
    pub const ALL: [Series; 6] = [
        Series::Reception,
        Series::Attack,
        Series::CounterAttack,
        Series::Serve,
        Series::SideOut,
        Series::BreakPoint,
    ];

    /// The figure, `None` with no touches.
    pub fn value(&self, stats: &Stats) -> Option<f64> {
        let all = StatsFilter::default();
        let efficiency = |e| stats.metric_percentage(e, Metric::Efficiency, &all);
        match self {
            Series::Reception => efficiency(EventTypeEnum::P),
            Series::Attack => efficiency(EventTypeEnum::A),
            Series::CounterAttack => stats.attack_summary(true, &all).map(|a| a.efficiency()),
            Series::Serve => efficiency(EventTypeEnum::S),
            Series::SideOut | Series::BreakPoint => {
                stats.rallies_won(&self.phase_filter()).map(|(v, _, _)| v)
            }
        }
    }

    /// The touches of the figure: the receptions, the attacks, the
    /// counter-attacks, the serves, the rallies of the phase.
    pub fn touches(&self, stats: &Stats) -> u32 {
        let all = StatsFilter::default();
        match self {
            Series::Reception => stats.event_count(EventTypeEnum::P, None, &all),
            Series::Attack => stats.event_count(EventTypeEnum::A, None, &all),
            Series::CounterAttack => stats.attack_summary(true, &all).map_or(0, |a| a.total),
            Series::Serve => stats.event_count(EventTypeEnum::S, None, &all),
            Series::SideOut | Series::BreakPoint => stats
                .rallies_won(&self.phase_filter())
                .map_or(0, |(_, _, rallies)| rallies),
        }
    }

    /// The band its color comes from (see `benchmark`).
    pub fn measure(&self) -> Measure {
        match self {
            Series::Reception => Measure::Reception,
            Series::Attack | Series::CounterAttack => Measure::Attack,
            Series::Serve => Measure::Serve,
            Series::SideOut => Measure::SideOut,
            Series::BreakPoint => Measure::BreakPoint,
        }
    }

    fn phase_filter(&self) -> StatsFilter {
        let phase = match self {
            Series::BreakPoint => PhaseEnum::Break,
            _ => PhaseEnum::SideOut,
        };
        StatsFilter::new(Some(phase), None, None)
    }
}

/// A sample taken after a rally: the set it was played in, the score it
/// reached, and what the statistics gave.
#[derive(Debug, Clone, PartialEq)]
pub struct TrendPoint<T> {
    pub set_number: u8,
    pub score_us: u8,
    pub score_them: u8,
    pub value: T,
}

/// The end of a rally: the event ending it, and the score it reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RallyEnd {
    event: usize,
    set_number: u8,
    score_us: u8,
    score_them: u8,
}

/// The sets replayed in order, with the statistics after every event: of
/// the sets before and of the set so far, so the trend of a match goes on
/// across its sets.
#[derive(Debug, Clone)]
pub struct Replay {
    /// after each event
    stats: Vec<Stats>,
    rallies: Vec<RallyEnd>,
}

impl Replay {
    /// A set that doesn't replay stops the replay with its error.
    pub fn new(sets: &[&SetEntry], max_substitutions: u8) -> Result<Self, AppError> {
        let mut replay = Replay {
            stats: vec![],
            rallies: vec![],
        };
        let mut before = Stats::new();
        for set in sets {
            let rules = set.find_lineup_rules(max_substitutions)?;
            let mut snapshot = Snapshot::with_rules(set, max_substitutions, rules)?;
            for event in &set.events {
                let rallies = snapshot.points.len();
                snapshot.add_event(event)?;
                let mut stats = before.clone();
                stats.merge(&snapshot.stats);
                replay.stats.push(stats);
                if snapshot.points.len() > rallies {
                    replay.rallies.push(RallyEnd {
                        event: replay.stats.len() - 1,
                        set_number: set.set_number,
                        score_us: snapshot.score_us,
                        score_them: snapshot.score_them,
                    });
                }
            }
            before.merge(&snapshot.stats);
        }
        Ok(replay)
    }

    /// The statistics so far, after every rally.
    pub fn cumulative<T>(&self, sample: impl Fn(&Stats) -> T) -> Vec<TrendPoint<T>> {
        self.points(|end| self.stats.get(end.event).map(&sample))
    }

    /// The statistics of the last `size` touches, after every rally: the
    /// touches are what `count` counts (e.g. the receptions), and each
    /// event adds one at most. Before `size` touches, all of them.
    pub fn window<T>(
        &self,
        size: u32,
        count: impl Fn(&Stats) -> u32,
        sample: impl Fn(&Stats) -> T,
    ) -> Vec<TrendPoint<T>> {
        let counts: Vec<u32> = self.stats.iter().map(&count).collect();
        self.points(|end| {
            let now = self.stats.get(end.event)?;
            let touches = counts.get(end.event).copied().unwrap_or(0);
            // the statistics after the touch before the window
            let before = touches
                .checked_sub(size)
                .filter(|t| *t > 0)
                .and_then(|t| counts.iter().position(|c| *c == t))
                .and_then(|i| self.stats.get(i));
            Some(match before {
                Some(before) => sample(&now.since(before)),
                None => sample(now),
            })
        })
    }

    /// The figure after every rally: so far, or of its last
    /// `RECENT_TOUCHES` touches.
    pub fn series(&self, series: Series, recent: bool) -> Vec<TrendPoint<Option<f64>>> {
        match recent {
            true => self.window(RECENT_TOUCHES, |s| series.touches(s), |s| series.value(s)),
            false => self.cumulative(|s| series.value(s)),
        }
    }

    fn points<T>(&self, value: impl Fn(&RallyEnd) -> Option<T>) -> Vec<TrendPoint<T>> {
        self.rallies
            .iter()
            .filter_map(|end| {
                value(end).map(|value| TrendPoint {
                    set_number: end.set_number,
                    score_us: end.score_us,
                    score_them: end.score_them,
                    value,
                })
            })
            .collect()
    }
}

/// The statistics so far after every rally (see `Replay::cumulative`).
pub fn trend<T>(
    sets: &[&SetEntry],
    max_substitutions: u8,
    sample: impl Fn(&Stats) -> T,
) -> Result<Vec<TrendPoint<T>>, AppError> {
    Ok(Replay::new(sets, max_substitutions)?.cumulative(sample))
}
