//! What the coach wants to know while the match is played, from the
//! statistics of a set or of the match: the efficiency of each fundamental,
//! the phases won, the errors (unforced first), the aces and the errors of
//! the opponent, and the distribution of the setter with the efficiency of
//! each target.
use crate::errors::AppError;
use crate::shapes::enums::{ErrorTypeEnum, EvalEnum, EventTypeEnum, PhaseEnum, ZoneEnum};
use crate::shapes::set::SetEntry;
use crate::shapes::stats::{AttackSummary, Metric, Stats, StatsFilter};

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
    fn of(stats: &Stats, event_type: EventTypeEnum) -> Self {
        let all = StatsFilter::default();
        Fundamental {
            total: stats.event_count(event_type, None, &all),
            efficiency: stats.metric_percentage(event_type, Metric::Efficiency, &all),
            perfect: stats.event_count(event_type, Some(EvalEnum::Perfect), &all),
            errors: stats.errors(event_type, None, &all),
        }
    }
}

/// The rallies won in a phase: `(percentage, won, rallies)`, as
/// `Stats::rallies_won`.
pub type RalliesWon = Option<(f64, u32, u32)>;

/// The errors of a kind, by the fundamental they were made in, always all
/// of them in the same order (some may be 0).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Errors {
    pub total: u32,
    pub by_event: Vec<(EventTypeEnum, u32)>,
}

impl Errors {
    fn of(stats: &Stats, error_type: ErrorTypeEnum) -> Self {
        let all = StatsFilter::default();
        let by_event: Vec<(EventTypeEnum, u32)> = Self::events(error_type)
            .iter()
            .map(|et| (*et, stats.errors(*et, Some(error_type), &all)))
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
        use EventTypeEnum::*;
        let all = StatsFilter::default();
        let phase = |p| stats.rallies_won(&StatsFilter::new(Some(p), None, None));
        Overview {
            reception: Fundamental::of(stats, P),
            attack: Fundamental::of(stats, A),
            serve: Fundamental::of(stats, S),
            block: Fundamental::of(stats, B),
            dig: Fundamental::of(stats, D),
            side_out: phase(PhaseEnum::SideOut),
            break_points: phase(PhaseEnum::Break),
            first_attack: stats.attack_summary(false, &all),
            counter_attack: stats.attack_summary(true, &all),
            unforced_errors: Errors::of(stats, ErrorTypeEnum::Unforced),
            forced_errors: Errors::of(stats, ErrorTypeEnum::Forced),
            aces: stats.event_count(S, Some(EvalEnum::Perfect), &all),
            aces_against: stats.event_count(P, Some(EvalEnum::Error), &all)
                + stats.untouched_aces(&all),
            opponent_errors: stats.event_count(OE, None, &all),
            opponent_points: stats.event_count(OS, None, &all) - stats.untouched_aces(&all),
        }
    }
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
    let all = StatsFilter::default();
    let zones: Vec<(ZoneEnum, AttackSummary)> = DISTRIBUTION_ZONES
        .iter()
        .filter_map(|z| {
            stats
                .zone_attacks(*z, counter_attack, &all)
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
