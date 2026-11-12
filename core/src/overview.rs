//! What the coach wants to know while the match is played, from the
//! statistics of a set or of the match: the efficiency of each fundamental,
//! the phases won, the errors (unforced first), the aces and the errors of
//! the opponent, and the distribution of the setter with the efficiency of
//! each target.
use crate::errors::AppError;
use crate::shapes::enums::{ErrorTypeEnum, EvalEnum, EventTypeEnum, PhaseEnum, ZoneEnum};
use crate::shapes::set::SetEntry;
use crate::shapes::stats::{
    AttackSummary, LAST_POSSESSION, Metric, PossessionSummary, Stats, StatsFilter,
};

/// The zones the setter sets to: the front row (4, 3, 2), the pipe (8) and
/// the opposite from the back row (9), in the order they're drawn.
pub const DISTRIBUTION_ZONES: [ZoneEnum; 5] = [
    ZoneEnum::Four,
    ZoneEnum::Three,
    ZoneEnum::Two,
    ZoneEnum::Eight,
    ZoneEnum::Nine,
];

/// The events scored in a fundamental.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Fundamental {
    pub total: u32,
    /// as `Metric::Efficiency`, `None` with no events
    pub efficiency: Option<f64>,
    /// the perfect ones (`#`): aces, kills, block points, perfect
    /// receptions and digs
    pub perfect: u32,
    /// the errors, forced or not (see `EventTypeEnum::error_type`)
    pub errors: u32,
}

impl Fundamental {
    fn of(stats: &Stats, event_type: EventTypeEnum, filter: &StatsFilter) -> Self {
        Fundamental {
            total: stats.event_count(event_type, None, filter),
            efficiency: stats.metric_percentage(event_type, Metric::Efficiency, filter),
            perfect: stats.event_count(event_type, Some(EvalEnum::Perfect), filter),
            errors: stats.errors(event_type, None, filter),
        }
    }
}

/// The rallies won in a phase: `(percentage, won, rallies)`, as
/// `Stats::rallies_won`.
pub type RalliesWon = Option<(f64, u32, u32)>;

/// The points won minus the ones lost in the rallies, none when no rally
/// was played.
pub fn balance(rallies: RalliesWon) -> Option<i64> {
    rallies.map(|(_, won, total)| 2 * i64::from(won) - i64::from(total))
}

/// The errors of a kind, by the fundamental they were made in, always all
/// of them in the same order (some may be 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Errors {
    pub total: u32,
    pub by_event: Vec<(EventTypeEnum, u32)>,
}

impl Errors {
    fn of(stats: &Stats, error_type: ErrorTypeEnum, filter: &StatsFilter) -> Self {
        let by_event: Vec<(EventTypeEnum, u32)> = Self::events(error_type)
            .iter()
            .map(|et| (*et, stats.errors(*et, Some(error_type), filter)))
            .collect();
        Errors {
            total: by_event.iter().map(|(_, n)| n).sum(),
            by_event,
        }
    }

    /// The fundamentals an error of the kind can be made in (see
    /// `EventTypeEnum::error_type`): unforced, a serve or attack out or in
    /// the net, a block invasion, a fault; forced, an attack blocked, a
    /// block out, an ace received, a dig lost.
    pub fn events(error_type: ErrorTypeEnum) -> [EventTypeEnum; 4] {
        use EventTypeEnum::*;
        match error_type {
            ErrorTypeEnum::Unforced => [S, A, B, F],
            ErrorTypeEnum::Forced => [A, B, P, D],
        }
    }
}

/// The share of the attacks set to a zone, and how they went.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoneShare {
    pub zone: ZoneEnum,
    /// of the attacks set to the zones of the distribution, 0 to 100
    pub share: f64,
    pub attacks: AttackSummary,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Overview {
    pub reception: Fundamental,
    pub attack: Fundamental,
    pub serve: Fundamental,
    pub block: Fundamental,
    pub dig: Fundamental,
    pub side_out: RalliesWon,
    /// as `side_out_possessions`
    pub side_out_possessions: [(u8, Option<PossessionSummary>); LAST_POSSESSION as usize],
    pub break_points: RalliesWon,
    /// the attacks built on our reception and in counter-attack (see
    /// `Stats::attack_summary`)
    pub first_attack: Option<AttackSummary>,
    pub counter_attack: Option<AttackSummary>,
    pub unforced_errors: Errors,
    pub forced_errors: Errors,
    /// our serves `#`
    pub aces: u32,
    /// their aces: our receptions `=`, and their points right after their
    /// serve, untouched (see `Stats::untouched_aces`)
    pub aces_against: u32,
    /// their errors (`OE`): serves, attacks, faults, they aren't told apart
    pub opponent_errors: u32,
    /// their points the scouting doesn't give to one of ours (`OS`), but
    /// their untouched aces, among `aces_against`
    pub opponent_points: u32,
}

impl Overview {
    pub fn of(stats: &Stats) -> Self {
        Self::narrowed(stats, &StatsFilter::default())
    }

    /// The overview narrowed by the filter: a phase asked leaves the other
    /// one empty.
    pub fn narrowed(stats: &Stats, filter: &StatsFilter) -> Self {
        use EventTypeEnum::*;
        let phase = |p| in_phase(filter, p).and_then(|f| stats.rallies_won(&f));
        Overview {
            reception: Fundamental::of(stats, P, filter),
            attack: Fundamental::of(stats, A, filter),
            serve: Fundamental::of(stats, S, filter),
            block: Fundamental::of(stats, B, filter),
            dig: Fundamental::of(stats, D, filter),
            side_out: phase(PhaseEnum::SideOut),
            side_out_possessions: side_out_possessions(stats, filter),
            break_points: phase(PhaseEnum::Break),
            first_attack: stats.attack_summary(false, filter),
            counter_attack: stats.attack_summary(true, filter),
            unforced_errors: Errors::of(stats, ErrorTypeEnum::Unforced, filter),
            forced_errors: Errors::of(stats, ErrorTypeEnum::Forced, filter),
            aces: stats.event_count(S, Some(EvalEnum::Perfect), filter),
            aces_against: stats.event_count(P, Some(EvalEnum::Error), filter)
                + stats.untouched_aces(filter),
            opponent_errors: stats.event_count(OE, None, filter),
            opponent_points: stats
                .event_count(OS, None, filter)
                .saturating_sub(stats.untouched_aces(filter)),
        }
    }
}

/// The filter on a phase, `None` when it asks the other one.
pub fn in_phase(filter: &StatsFilter, phase: PhaseEnum) -> Option<StatsFilter> {
    match filter.phase {
        Some(asked) if asked != phase => None,
        _ => Some(StatsFilter {
            phase: Some(phase),
            ..*filter
        }),
    }
}

/// The side outs by possession: the first, the second, and the third with
/// the later ones (see `Stats::side_out_possessions`), `None` for the ones
/// never played.
pub fn side_out_possessions(
    stats: &Stats,
    filter: &StatsFilter,
) -> [(u8, Option<PossessionSummary>); LAST_POSSESSION as usize] {
    [1, 2, LAST_POSSESSION].map(|n| (n, stats.side_out_possessions(Some(n), filter)))
}

/// The statistics of the sets together: a set, or the match so far.
pub fn stats_of(sets: &[&SetEntry], max_substitutions: u8) -> Result<Stats, AppError> {
    sets.iter().try_fold(Stats::new(), |mut stats, set| {
        set.compute_snapshot(max_substitutions).map(|s| {
            stats.merge(&s.stats);
            stats
        })
    })
}

/// The distribution of the setter: the attacks set to each zone of
/// `DISTRIBUTION_ZONES` (all of them, or only the ones on our reception or
/// in counter-attack), the zones never set to left out. The shares add up
/// to 100; empty with no attacks. The attacks with no zone (a back row
/// setter or middle blocker, the libero) aren't in it.
pub fn distribution(stats: &Stats, counter_attack: Option<bool>) -> Vec<ZoneShare> {
    distribution_in(stats, counter_attack, &StatsFilter::default())
}

/// The distribution narrowed by the filter (see `distribution`).
pub fn distribution_in(
    stats: &Stats,
    counter_attack: Option<bool>,
    filter: &StatsFilter,
) -> Vec<ZoneShare> {
    let zones: Vec<(ZoneEnum, AttackSummary)> = DISTRIBUTION_ZONES
        .iter()
        .filter_map(|z| {
            stats
                .zone_attacks(*z, counter_attack, filter)
                .map(|a| (*z, a))
        })
        .collect();
    let total: u32 = zones.iter().map(|(_, a)| a.total).sum();
    zones
        .into_iter()
        .map(|(zone, attacks)| ZoneShare {
            zone,
            share: attacks.total as f64 / total as f64 * 100.0,
            attacks,
        })
        .collect()
}
