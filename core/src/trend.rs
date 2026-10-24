//! How the statistics went point by point: the sets are replayed, and after
//! every rally the statistics are sampled, to draw their trend. Either all
//! of them so far, or only the last touches of a fundamental (e.g. the last
//! 10 receptions), to see a drop as it comes.
use crate::errors::AppError;
use crate::shapes::set::SetEntry;
use crate::shapes::snapshot::Snapshot;
use crate::shapes::stats::Stats;

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
