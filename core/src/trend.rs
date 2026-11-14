//! How the statistics went point by point: the sets are replayed, and after
//! every rally the statistics are sampled, to draw their trend. Either all
//! of them so far, or only the last touches of a fundamental (e.g. the last
//! 10 receptions), to see a drop as it comes.
use crate::benchmark::Measure;
use crate::errors::AppError;
use crate::shapes::enums::{ErrorTypeEnum, EventTypeEnum, PhaseEnum, TeamSideEnum};
use crate::shapes::set::SetEntry;
use crate::shapes::snapshot::Snapshot;
use crate::shapes::stats::{Metric, Stats, StatsFilter, percentage};

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

    /// Its place in `ALL`.
    fn index(&self) -> usize {
        match self {
            Series::Reception => 0,
            Series::Attack => 1,
            Series::CounterAttack => 2,
            Series::Serve => 3,
            Series::SideOut => 4,
            Series::BreakPoint => 5,
        }
    }

    /// The figure, `None` with no touches.
    pub fn value(&self, stats: &Stats) -> Option<f64> {
        self.value_in(stats, &StatsFilter::default())
    }

    /// The touches of the figure: the receptions, the attacks, the
    /// counter-attacks, the serves, the rallies of the phase.
    pub fn touches(&self, stats: &Stats) -> u32 {
        self.touches_in(stats, &StatsFilter::default())
    }

    /// The figure narrowed by the filter, `None` with no touches (also
    /// with the filter on the other phase, for side out and break point).
    pub fn value_in(&self, stats: &Stats, filter: &StatsFilter) -> Option<f64> {
        self.tally_in(stats, filter).value()
    }

    /// The touches of the figure narrowed by the filter.
    pub fn touches_in(&self, stats: &Stats, filter: &StatsFilter) -> u32 {
        self.tally_in(stats, filter).touches
    }

    /// What the figure narrowed by the filter is made of.
    pub fn tally_in(&self, stats: &Stats, filter: &StatsFilter) -> Tally {
        let efficiency = |e| {
            let (score, total) = stats.metric_score(e, Metric::Efficiency, filter);
            Tally {
                score,
                total,
                touches: stats.event_count(e, None, filter),
                errors: stats.errors(e, None, filter),
                unforced: stats.errors(e, Some(ErrorTypeEnum::Unforced), filter),
            }
        };
        match self {
            Series::Reception => efficiency(EventTypeEnum::P),
            Series::Attack => efficiency(EventTypeEnum::A),
            Series::CounterAttack => {
                stats
                    .attack_summary(true, filter)
                    .map_or(Tally::default(), |a| Tally {
                        score: a.kills as i32 - a.errors as i32 - a.blocked as i32,
                        total: a.total,
                        touches: a.total,
                        // blocked: forced
                        errors: a.errors + a.blocked,
                        unforced: a.errors,
                    })
            }
            Series::Serve => efficiency(EventTypeEnum::S),
            Series::SideOut | Series::BreakPoint => self
                .phase_filter(filter)
                .and_then(|f| stats.rallies_won(&f).map(|r| (f, r)))
                .map_or(Tally::default(), |(f, (_, won, rallies))| Tally {
                    score: won as i32,
                    total: rallies,
                    touches: rallies,
                    // every error of the phase, a fault too
                    errors: stats.total_errors(None, &f),
                    unforced: stats.total_errors(Some(ErrorTypeEnum::Unforced), &f),
                }),
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

    /// The filter on the phase of the figure, `None` when it asks the
    /// other phase.
    fn phase_filter(&self, filter: &StatsFilter) -> Option<StatsFilter> {
        let phase = match self {
            Series::BreakPoint => PhaseEnum::Break,
            _ => PhaseEnum::SideOut,
        };
        crate::overview::in_phase(filter, phase)
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

/// What a figure is made of: the figure and its touches come from it, and
/// it adds up, so the figure of the last touches is a difference (the
/// counters only grow while the sets are replayed).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    /// the score of the efficiency, or the rallies won
    pub score: i32,
    /// what the score is over: the touches evaluated, or the rallies
    pub total: u32,
    pub touches: u32,
    /// the errors of the figure: of its fundamental (a reception aced, an
    /// attack out or blocked), of every fundamental in its phase
    pub errors: u32,
    /// the ones unforced (`EventTypeEnum::error_type`): a serve or an
    /// attack out, a block invasion, a fault
    pub unforced: u32,
}

impl Tally {
    /// The figure, `None` with nothing to score.
    pub fn value(&self) -> Option<f64> {
        (self.total > 0).then(|| percentage(self.score, self.total))
    }

    fn plus(self, other: Tally) -> Tally {
        Tally {
            score: self.score + other.score,
            total: self.total + other.total,
            touches: self.touches + other.touches,
            errors: self.errors + other.errors,
            unforced: self.unforced + other.unforced,
        }
    }

    /// Since `earlier`, a tally of the same sets taken before.
    fn minus(self, earlier: Tally) -> Tally {
        Tally {
            score: self.score - earlier.score,
            total: self.total.saturating_sub(earlier.total),
            touches: self.touches.saturating_sub(earlier.touches),
            errors: self.errors.saturating_sub(earlier.errors),
            unforced: self.unforced.saturating_sub(earlier.unforced),
        }
    }
}

/// The tally of each figure, in the order of `Series::ALL`.
type Tallies = [Tally; 6];

fn tallies(stats: &Stats) -> Tallies {
    Series::ALL.map(|s| s.tally_in(stats, &StatsFilter::default()))
}

fn plus(a: &Tallies, b: &Tallies) -> Tallies {
    std::array::from_fn(|i| match (a.get(i), b.get(i)) {
        (Some(a), Some(b)) => a.plus(*b),
        _ => Tally::default(),
    })
}

fn minus(a: &Tallies, b: &Tallies) -> Tallies {
    std::array::from_fn(|i| match (a.get(i), b.get(i)) {
        (Some(a), Some(b)) => a.minus(*b),
        _ => Tally::default(),
    })
}

/// The end of a rally: the event ending it, and the score it reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RallyEnd {
    event: usize,
    set_number: u8,
    score_us: u8,
    score_them: u8,
}

/// Replays the sets in order, calling `each` after every event with the set
/// number, the snapshot of the set so far, the statistics of the sets
/// before, and whether the event ended a rally.
fn replay(
    sets: &[&SetEntry],
    max_substitutions: u8,
    mut each: impl FnMut(u8, &Snapshot, &Stats, bool),
) -> Result<(), AppError> {
    let mut before = Stats::new();
    for set in sets {
        let rules = set.find_lineup_rules(max_substitutions)?;
        let mut snapshot = Snapshot::with_rules(set, max_substitutions, rules)?;
        for event in &set.events {
            let rallies = snapshot.points.len();
            snapshot.add_event(event)?;
            each(
                set.set_number,
                &snapshot,
                &before,
                snapshot.points.len() > rallies,
            );
        }
        before.merge(&snapshot.stats);
    }
    Ok(())
}

/// The sets replayed in order, with the figures after every event: of the
/// sets before and of the set so far, so the trend of a match goes on
/// across its sets. It keeps the tallies, not the statistics: the figure of
/// the last touches is then a difference, not a count of every key.
#[derive(Debug, Clone)]
pub struct Replay {
    /// after each event: its set and the tallies
    events: Vec<(u8, Tallies)>,
    rallies: Vec<RallyEnd>,
}

impl Replay {
    /// A set that doesn't replay stops the replay with its error.
    pub fn new(sets: &[&SetEntry], max_substitutions: u8) -> Result<Self, AppError> {
        let mut events: Vec<(u8, Tallies)> = vec![];
        let mut rallies = vec![];
        // the tallies of the sets before, as they were when the set began
        let mut before = (0, [Tally::default(); 6]);
        replay(sets, max_substitutions, |set_number, snapshot, _, ended| {
            if before.0 != set_number {
                before = (
                    set_number,
                    events.last().map_or([Tally::default(); 6], |(_, t)| *t),
                );
            }
            events.push((set_number, plus(&before.1, &tallies(&snapshot.stats))));
            if ended {
                rallies.push(RallyEnd {
                    event: events.len() - 1,
                    set_number,
                    score_us: snapshot.score_us,
                    score_them: snapshot.score_them,
                });
            }
        })?;
        Ok(Replay { events, rallies })
    }

    /// One set of the replay, as if replayed alone: its figures from 0.
    pub fn set(&self, set_number: u8) -> Replay {
        let first = self.events.iter().position(|(n, _)| *n == set_number);
        let start = first
            .and_then(|f| f.checked_sub(1))
            .and_then(|i| self.events.get(i))
            .map_or([Tally::default(); 6], |(_, t)| *t);
        let offset = first.unwrap_or(0);
        Replay {
            events: self
                .events
                .iter()
                .filter(|(n, _)| *n == set_number)
                .map(|(n, t)| (*n, minus(t, &start)))
                .collect(),
            rallies: self
                .rallies
                .iter()
                .filter(|end| end.set_number == set_number)
                .filter_map(|end| {
                    end.event
                        .checked_sub(offset)
                        .map(|event| RallyEnd { event, ..*end })
                })
                .collect(),
        }
    }

    /// The tally of the figure so far, after every rally.
    pub fn cumulative(&self, series: Series) -> Vec<TrendPoint<Tally>> {
        self.points(|end| self.tally(end.event, series))
    }

    /// The tally of the last `size` touches of the figure, after every
    /// rally: each event adds one touch at most. Before `size` touches,
    /// all of them.
    pub fn window(&self, series: Series, size: u32) -> Vec<TrendPoint<Tally>> {
        self.points(|end| {
            let now = self.tally(end.event, series)?;
            // the tally after the touch before the window: the touches
            // only grow, the first event reaching them
            let before = now
                .touches
                .checked_sub(size)
                .filter(|t| *t > 0)
                .and_then(|t| {
                    let i = self.events.partition_point(|(_, tallies)| {
                        tallies
                            .get(series.index())
                            .is_some_and(|tally| tally.touches < t)
                    });
                    self.tally(i, series).filter(|tally| tally.touches == t)
                });
            Some(before.map_or(now, |before| now.minus(before)))
        })
    }

    /// The figure after every rally: so far, or of its last
    /// `RECENT_TOUCHES` touches.
    pub fn series(&self, series: Series, recent: bool) -> Vec<TrendPoint<Option<f64>>> {
        let points = match recent {
            true => self.window(series, RECENT_TOUCHES),
            false => self.cumulative(series),
        };
        points
            .into_iter()
            .map(|p| TrendPoint {
                set_number: p.set_number,
                score_us: p.score_us,
                score_them: p.score_them,
                value: p.value.value(),
            })
            .collect()
    }

    /// How far the set had gone after every rally, to draw the sets side
    /// by side: the points of the team ahead, the rallies ending at the
    /// same points (the team behind scoring) spread evenly up to them. From
    /// 0 to 25 in a set without deuce, e.g. 1 after the first rally, 1.5
    /// and 2 after 1-1 and 2-1.
    pub fn progress(&self) -> Vec<f64> {
        let leading: Vec<(u8, u8)> = self
            .rallies
            .iter()
            .map(|end| (end.set_number, end.score_us.max(end.score_them)))
            .collect();
        leading
            .iter()
            .enumerate()
            .map(|(i, at)| {
                let same = leading.iter().filter(|l| *l == at).count();
                let nth = leading.iter().take(i + 1).filter(|l| *l == at).count();
                f64::from(at.1) - 1.0 + nth as f64 / same as f64
            })
            .collect()
    }

    /// The errors of the figure made in each rally (only the unforced
    /// ones, `unforced`): where they pile up along the set.
    pub fn errors(&self, series: Series, unforced: bool) -> Vec<u32> {
        let count = |t: Tally| match unforced {
            true => t.unforced,
            false => t.errors,
        };
        let totals: Vec<u32> = self
            .rallies
            .iter()
            .map(|end| self.tally(end.event, series).map_or(0, count))
            .collect();
        // the first rally of the replay from 0, each set from the last
        // rally of the set before it in a whole match
        totals
            .iter()
            .enumerate()
            .map(|(i, now)| {
                let before = i
                    .checked_sub(1)
                    .and_then(|b| totals.get(b))
                    .copied()
                    .unwrap_or(0);
                now.saturating_sub(before)
            })
            .collect()
    }

    /// The points in order, as `Snapshot::point_sequence`: the team scoring
    /// each rally, with the score it reaches.
    pub fn point_sequence(&self) -> Vec<(TeamSideEnum, u8)> {
        self.rallies
            .iter()
            .scan((0u8, 0u8, 0u8), |(set, us, them), end| {
                // a new set from 0-0
                if *set != end.set_number {
                    *set = end.set_number;
                    *us = 0;
                    *them = 0;
                }
                let point = match end.score_us > *us {
                    true => (TeamSideEnum::Us, end.score_us),
                    false => (TeamSideEnum::Them, end.score_them),
                };
                *us = end.score_us;
                *them = end.score_them;
                Some(point)
            })
            .collect()
    }

    fn tally(&self, event: usize, series: Series) -> Option<Tally> {
        self.events
            .get(event)
            .and_then(|(_, tallies)| tallies.get(series.index()))
            .copied()
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

/// Any sample of the statistics so far, after every rally.
pub fn trend<T>(
    sets: &[&SetEntry],
    max_substitutions: u8,
    sample: impl Fn(&Stats) -> T,
) -> Result<Vec<TrendPoint<T>>, AppError> {
    let mut points = vec![];
    replay(
        sets,
        max_substitutions,
        |set_number, snapshot, before, ended| {
            if ended {
                let mut stats = before.clone();
                stats.merge(&snapshot.stats);
                points.push(TrendPoint {
                    set_number,
                    score_us: snapshot.score_us,
                    score_them: snapshot.score_them,
                    value: sample(&stats),
                });
            }
        },
    )?;
    Ok(points)
}
