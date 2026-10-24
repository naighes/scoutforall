//! How the statistics went point by point: the sets are replayed, and after
//! every rally the statistics so far are sampled, to draw their trend.
use crate::errors::AppError;
use crate::shapes::set::SetEntry;
use crate::shapes::snapshot::Snapshot;
use crate::shapes::stats::Stats;

/// A sample taken after a rally: the set it was played in, the score it
/// reached, and what the statistics so far gave.
#[derive(Debug, Clone, PartialEq)]
pub struct TrendPoint<T> {
    pub set_number: u8,
    pub score_us: u8,
    pub score_them: u8,
    pub value: T,
}

/// Replays the sets in order and samples the statistics after every rally:
/// the statistics of the sets before and of the set so far, so the trend of
/// a match goes on across its sets. A set that doesn't replay stops the
/// trend with its error.
pub fn trend<T>(
    sets: &[&SetEntry],
    max_substitutions: u8,
    sample: impl Fn(&Stats) -> T,
) -> Result<Vec<TrendPoint<T>>, AppError> {
    let mut points = vec![];
    let mut before = Stats::new();
    for set in sets {
        let rules = set.find_lineup_rules(max_substitutions)?;
        let mut snapshot = Snapshot::with_rules(set, max_substitutions, rules)?;
        for event in &set.events {
            let rallies = snapshot.points.len();
            snapshot.add_event(event)?;
            if snapshot.points.len() > rallies {
                let mut stats = before.clone();
                stats.merge(&snapshot.stats);
                points.push(TrendPoint {
                    set_number: set.set_number,
                    score_us: snapshot.score_us,
                    score_them: snapshot.score_them,
                    value: sample(&stats),
                });
            }
        }
        before.merge(&snapshot.stats);
    }
    Ok(points)
}
