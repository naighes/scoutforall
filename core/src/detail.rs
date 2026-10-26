//! A fundamental in detail, narrowed to a phase, a rotation and a player:
//! how its evaluations went, its efficiency and positiveness, its errors and
//! points, where the attacks were set; the freeballs given and received;
//! the players compared in a fundamental; the phases won in each rotation.
use crate::overview::{DISTRIBUTION_ZONES, RalliesWon};
use crate::shapes::enums::{ErrorTypeEnum, EvalEnum, EventTypeEnum, PhaseEnum, ZoneEnum};
use crate::shapes::stats::{Metric, Stats, StatsFilter};
use std::cmp::Reverse;
use uuid::Uuid;

/// The fundamentals to look at in detail, in the order they're listed: the
/// freeballs (`FB`) stand for the ones given and received.
pub const FUNDAMENTALS: [EventTypeEnum; 6] = [
    EventTypeEnum::S,
    EventTypeEnum::P,
    EventTypeEnum::D,
    EventTypeEnum::B,
    EventTypeEnum::A,
    EventTypeEnum::FB,
];

/// How good an evaluation is for the fundamental, from the best to the
/// worst: to color it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Excellent,
    Good,
    Fair,
    Poor,
    Bad,
}

/// The evaluations of a fundamental from the best to the worst, with how
/// good each is; none for an event with no evaluation.
pub fn evaluations(event_type: EventTypeEnum) -> &'static [(EvalEnum, Tone)] {
    use EvalEnum::*;
    use EventTypeEnum::*;
    use Tone::*;
    match event_type {
        // their reception coming straight back (`/`) is as good as an ace
        S => &[
            (Perfect, Excellent),
            (Over, Excellent),
            (Positive, Good),
            (Negative, Poor),
            (Error, Bad),
        ],
        // an overpass (`/`) gives them the ball, but isn't a point
        P | D => &[
            (Perfect, Excellent),
            (Positive, Good),
            (Exclamative, Fair),
            (Negative, Poor),
            (Over, Poor),
            (Error, Bad),
        ],
        // an invasion (`/`) is a point of theirs
        B => &[
            (Perfect, Excellent),
            (Positive, Good),
            (Negative, Poor),
            (Error, Bad),
            (Over, Bad),
        ],
        // blocked but covered (`!`), we play the ball again; blocked (`/`)
        A => &[
            (Perfect, Excellent),
            (Positive, Good),
            (Exclamative, Good),
            (Negative, Poor),
            (Over, Bad),
            (Error, Bad),
        ],
        _ => &[],
    }
}

/// The events with an evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvalShare {
    pub eval: EvalEnum,
    pub tone: Tone,
    pub count: u32,
    /// of the events of the fundamental, 0 to 100
    pub share: f64,
}

/// The attacks set to a zone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoneDetail {
    pub zone: ZoneEnum,
    /// of the attacks, 0 to 100
    pub share: f64,
    /// the kills among them, 0 to 100
    pub conversion: f64,
}

/// A fundamental in detail.
#[derive(Debug, Clone, PartialEq)]
pub struct Detail {
    pub total: u32,
    /// the evaluations given at least once, from the best to the worst
    pub evals: Vec<EvalShare>,
    /// as `Metric`, `None` with no events
    pub efficiency: Option<f64>,
    pub positiveness: Option<f64>,
    /// forced or not, and the unforced among them
    pub errors: u32,
    pub unforced_errors: u32,
    /// the rallies it takes to win a point, as `Stats::phases_per_point`:
    /// `None` with a player, the rallies have none
    pub phases_per_point: Option<(f64, u32, u32)>,
    /// every fault, whatever the fundamental
    pub faults: u32,
    /// the points scored (`#`), only for a serve, an attack or a block
    pub points: Option<u32>,
    /// for an attack, the zones of `DISTRIBUTION_ZONES` set to
    pub zones: Vec<ZoneDetail>,
}

/// The freeballs: the ones we gave (`FB`), the ones they gave us (`OF`)
/// and how many of these we exploited. Theirs have no player: with a player,
/// only the ones given.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Freeballs {
    pub given: u32,
    pub received: Option<u32>,
    /// `(percentage, freeballs, exploited)`, as
    /// `Stats::freeball_exploitation`
    pub exploited: Option<(f64, u32, u32)>,
}

/// A fundamental in detail, or the freeballs.
#[derive(Debug, Clone, PartialEq)]
pub enum Breakdown {
    Fundamental(Detail),
    Freeballs(Freeballs),
}

impl Breakdown {
    /// One of `FUNDAMENTALS` in the statistics, narrowed by the filter.
    pub fn of(stats: &Stats, event_type: EventTypeEnum, filter: &StatsFilter) -> Self {
        match event_type {
            EventTypeEnum::FB => Breakdown::Freeballs(Freeballs::of(stats, filter)),
            _ => Breakdown::Fundamental(Detail::of(stats, event_type, filter)),
        }
    }
}

impl Detail {
    fn of(stats: &Stats, event_type: EventTypeEnum, filter: &StatsFilter) -> Self {
        let total = stats.event_count(event_type, None, filter);
        let evals = evaluations(event_type)
            .iter()
            .filter_map(|(eval, tone)| {
                let count = stats.event_count(event_type, Some(*eval), filter);
                stats
                    .event_percentage(event_type, *eval, filter)
                    .filter(|_| count > 0)
                    .map(|share| EvalShare {
                        eval: *eval,
                        tone: *tone,
                        count,
                        share,
                    })
            })
            .collect();
        let zones = match event_type {
            EventTypeEnum::A => DISTRIBUTION_ZONES
                .iter()
                .filter_map(|zone| {
                    stats
                        .zone_distribution(*zone, filter)
                        .map(|(share, conversion)| ZoneDetail {
                            zone: *zone,
                            share,
                            conversion,
                        })
                })
                .collect(),
            _ => Vec::new(),
        };
        Detail {
            total,
            evals,
            efficiency: stats.metric_percentage(event_type, Metric::Efficiency, filter),
            positiveness: stats.metric_percentage(event_type, Metric::Positive, filter),
            errors: stats.errors(event_type, None, filter),
            unforced_errors: stats.errors(event_type, Some(ErrorTypeEnum::Unforced), filter),
            phases_per_point: stats.phases_per_point(filter),
            faults: stats.event_count(EventTypeEnum::F, None, filter),
            points: event_type
                .provides_direct_points()
                .then(|| stats.event_count(event_type, Some(EvalEnum::Perfect), filter)),
            zones,
        }
    }
}

impl Freeballs {
    fn of(stats: &Stats, filter: &StatsFilter) -> Self {
        let theirs = filter.player.is_none();
        Freeballs {
            given: stats.event_count(EventTypeEnum::FB, None, filter),
            received: theirs.then(|| stats.event_count(EventTypeEnum::OF, None, filter)),
            exploited: match theirs {
                true => stats.freeball_exploitation(filter),
                false => None,
            },
        }
    }
}

/// A player in a fundamental.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerLine {
    pub player: Uuid,
    pub total: u32,
    pub efficiency: Option<f64>,
    pub positiveness: Option<f64>,
    /// as `Detail::points`
    pub points: Option<u32>,
    pub errors: u32,
}

/// The players who played the fundamental (narrowed by the filter, but
/// its player), the most involved first, then by efficiency: to compare
/// them. None for the freeballs.
pub fn by_player(
    stats: &Stats,
    event_type: EventTypeEnum,
    filter: &StatsFilter,
) -> Vec<PlayerLine> {
    let mut players: Vec<Uuid> = stats
        .events
        .iter(|k| k.event_type == event_type && event_type != EventTypeEnum::FB)
        .filter_map(|(k, _)| k.player)
        .collect();
    players.sort_unstable();
    players.dedup();
    let mut lines: Vec<PlayerLine> = players
        .into_iter()
        .map(|player| {
            let only = StatsFilter::new(filter.phase, filter.rotation, Some(player));
            PlayerLine {
                player,
                total: stats.event_count(event_type, None, &only),
                efficiency: stats.metric_percentage(event_type, Metric::Efficiency, &only),
                positiveness: stats.metric_percentage(event_type, Metric::Positive, &only),
                points: event_type
                    .provides_direct_points()
                    .then(|| stats.event_count(event_type, Some(EvalEnum::Perfect), &only)),
                errors: stats.errors(event_type, None, &only),
            }
        })
        .filter(|line| line.total > 0)
        .collect();
    // efficiencies are within -100..100: in tenths, an integer to sort by
    lines.sort_by_key(|line| {
        (
            Reverse(line.total),
            Reverse(line.efficiency.map(|e| (e * 10.0).round() as i64)),
            line.player,
        )
    });
    lines
}

/// The phases won in a rotation (0 to 5, the setter in position 1 to 6
/// and so on, as `StatsFilter::rotation`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotationLine {
    pub rotation: u8,
    pub side_out: RalliesWon,
    pub break_points: RalliesWon,
}

/// Every rotation, in order, played or not.
pub fn by_rotation(stats: &Stats) -> Vec<RotationLine> {
    (0..6)
        .map(|rotation| {
            let phase = |p| stats.rallies_won(&StatsFilter::new(Some(p), Some(rotation), None));
            RotationLine {
                rotation,
                side_out: phase(PhaseEnum::SideOut),
                break_points: phase(PhaseEnum::Break),
            }
        })
        .collect()
}
