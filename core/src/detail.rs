//! A fundamental in detail, narrowed to a phase, a rotation and a player:
//! how its evaluations went, its efficiency and positiveness, its errors and
//! points, where the attacks were set; the freeballs given and received;
//! the players compared in a fundamental; the phases won in each rotation.
use crate::overview::{DISTRIBUTION_ZONES, RalliesWon};
use crate::shapes::court_zone::{CourtSide, CourtZoneEnum, ServeStartEnum, TargetKind};
use crate::shapes::enums::{ErrorTypeEnum, EvalEnum, EventTypeEnum, PhaseEnum, ZoneEnum};
use crate::shapes::flags::BlockersEnum;
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
        // `!` as their reception `!`, between the two
        S => &[
            (Perfect, Excellent),
            (Over, Excellent),
            (Positive, Good),
            (Exclamative, Fair),
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

/// The serves (or receptions) needed for the efficiency of a zone: on
/// fewer, it would mislead.
pub const MIN_SERVES_FOR_EFFICIENCY: u32 = 3;

/// Our serves or attacks sent to a zone of their court, or our receptions
/// or digs in a zone of ours, or our blocks in a zone at the net (see
/// `CourtZoneEnum`), among the ones whose zone was told; or our receptions
/// or digs on a conflict zone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetDetail {
    pub zone: CourtZoneEnum,
    /// in a zone, two cells of different zones count half in each
    pub count: f64,
    /// of the ones whose zone was told, 0 to 100
    pub share: f64,
    /// as `Metric::Efficiency`, -100 to 100: `None` on fewer than
    /// `MIN_SERVES_FOR_EFFICIENCY`
    pub efficiency: Option<f64>,
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
    /// for a serve, an attack, a reception, a dig or a block, how many had
    /// their zone told, and each of the zones (`TargetKind::zones`: the nine,
    /// the three at the net for the block; the ones never told too); none
    /// for the other fundamentals. For a serve, only the ones from the zone
    /// asked (see `Breakdown::of`)
    pub targets_told: u32,
    pub targets: Vec<TargetDetail>,
    /// for a reception or a dig (on our court), each conflict zone
    /// (`CourtZoneEnum::CONFLICTS`)
    /// and all of them together; none for the other fundamentals, or with
    /// no zone told
    pub conflicts: Vec<TargetDetail>,
    pub on_conflicts: Option<OnConflicts>,
    /// for an attack or a block, how many had how many blocked told, and
    /// each of the ones it can tell (`BlockersEnum::of`), the ones never
    /// told too; none for the other fundamentals, or with none told
    pub blockers_told: u32,
    pub blockers: Vec<BlockersDetail>,
    /// for a serve, how many had where it started from told, and each zone
    /// it starts from (`ServeStartEnum::ALL`), the ones never told too;
    /// none for the other fundamentals, or with none told
    pub serve_starts_told: u32,
    pub serve_starts: Vec<ServeStartDetail>,
    /// for a serve, where they went from where they started (both told),
    /// by start and zone of their court, the most first: none with none
    /// told. Only from the zone asked, as `targets`
    pub serve_paths: Vec<ServePath>,
}

/// Our serves from a zone of our end line to a zone of their court, among
/// the ones with both told.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServePath {
    pub start: ServeStartEnum,
    pub zone: CourtZoneEnum,
    /// two cells of different zones count half in each
    pub count: f64,
    /// of the ones with both told, 0 to 100
    pub share: f64,
    /// as `Metric::Efficiency`, -100 to 100: `None` on fewer than
    /// `MIN_SERVES_FOR_EFFICIENCY`
    pub efficiency: Option<f64>,
}

/// Our serves started from a zone, among the ones with where it started
/// from told.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServeStartDetail {
    pub start: ServeStartEnum,
    pub count: u32,
    /// of the ones told, 0 to 100
    pub share: f64,
    /// as `Metric::Efficiency`, -100 to 100: `None` on fewer than
    /// `MIN_SERVES_FOR_EFFICIENCY`
    pub efficiency: Option<f64>,
    /// the errors among them: they went nowhere on their court
    pub errors: u32,
}

/// Our attacks against so many of theirs at the block, or our blocks with
/// so many of ours, among the ones with how many told.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockersDetail {
    pub blockers: BlockersEnum,
    pub count: u32,
    /// of the ones told, 0 to 100
    pub share: f64,
    /// as `Metric::Efficiency`, -100 to 100: `None` on fewer than
    /// `MIN_SERVES_FOR_EFFICIENCY`
    pub efficiency: Option<f64>,
}

/// Our receptions (or digs) on any conflict zone, among the ones whose zone
/// was told.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnConflicts {
    pub count: u32,
    /// of the ones whose zone was told, 0 to 100
    pub share: f64,
    /// as `Metric::Efficiency`: `None` on fewer than
    /// `MIN_SERVES_FOR_EFFICIENCY`
    pub efficiency: Option<f64>,
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

/// A fundamental in detail, or the freeballs. One at a time, for a page:
/// the larger variant isn't worth a box.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Breakdown {
    Fundamental(Detail),
    Freeballs(Freeballs),
}

impl Breakdown {
    /// One of `FUNDAMENTALS` in the statistics, narrowed by the filter.
    pub fn of(stats: &Stats, event_type: EventTypeEnum, filter: &StatsFilter) -> Self {
        Self::from_start(stats, event_type, filter, None)
    }

    /// As `of`, where our serves went only from `start`, when given: the
    /// rest of the detail as for all the serves.
    pub fn from_start(
        stats: &Stats,
        event_type: EventTypeEnum,
        filter: &StatsFilter,
        start: Option<ServeStartEnum>,
    ) -> Self {
        match event_type {
            EventTypeEnum::FB => Breakdown::Freeballs(Freeballs::of(stats, filter)),
            _ => Breakdown::Fundamental(Detail::of(stats, event_type, filter, start)),
        }
    }
}

impl Detail {
    fn of(
        stats: &Stats,
        event_type: EventTypeEnum,
        filter: &StatsFilter,
        start: Option<ServeStartEnum>,
    ) -> Self {
        // where the serve started from: only for a serve
        let start = start.filter(|_| event_type == EventTypeEnum::S);
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
        let side = CourtSide::of(event_type);
        let targets_told = match side {
            Some(_) => stats.events_with_target(event_type, start, filter),
            None => 0,
        };
        let detail = |zone: CourtZoneEnum, (count, efficiency): (f64, f64)| TargetDetail {
            zone,
            count,
            share: count / f64::from(targets_told) * 100.0,
            efficiency: (count >= f64::from(MIN_SERVES_FOR_EFFICIENCY)).then_some(efficiency),
        };
        let kind = TargetKind::of(event_type);
        let targets = match (kind, targets_told) {
            (Some(kind), 1..) => kind
                .zones()
                .iter()
                .map(|zone| detail(*zone, stats.events_to(event_type, *zone, start, filter)))
                .collect(),
            _ => Vec::new(),
        };
        // where two of ours could take their serve or their attack: only
        // on our court, in reception and in defense (not at the net)
        let (conflicts, on_conflicts) = match (side, kind, targets_told) {
            (Some(CourtSide::Ours), Some(TargetKind::Cells), 1..) => {
                let conflicts = CourtZoneEnum::CONFLICTS
                    .iter()
                    .map(|zone| {
                        let (count, efficiency) =
                            stats.events_on_conflict(event_type, Some(*zone), filter);
                        detail(*zone, (f64::from(count), efficiency))
                    })
                    .collect();
                let (count, efficiency) = stats.events_on_conflict(event_type, None, filter);
                let on_conflicts = OnConflicts {
                    count,
                    share: f64::from(count) / f64::from(targets_told) * 100.0,
                    efficiency: (count >= MIN_SERVES_FOR_EFFICIENCY).then_some(efficiency),
                };
                (conflicts, Some(on_conflicts))
            }
            _ => (Vec::new(), None),
        };
        let (blockers_told, _) = stats.events_with_blockers(event_type, None, filter);
        let blockers = match blockers_told {
            0 => Vec::new(),
            told => BlockersEnum::of(event_type)
                .iter()
                .map(|b| {
                    let (count, efficiency) =
                        stats.events_with_blockers(event_type, Some(*b), filter);
                    BlockersDetail {
                        blockers: *b,
                        count,
                        share: f64::from(count) / f64::from(told) * 100.0,
                        efficiency: (count >= MIN_SERVES_FOR_EFFICIENCY).then_some(efficiency),
                    }
                })
                .collect(),
        };
        let (serve_starts_told, _) = match event_type {
            EventTypeEnum::S => stats.serves_from(None, filter),
            _ => (0, 0.0),
        };
        let serve_starts = match serve_starts_told {
            0 => Vec::new(),
            told => ServeStartEnum::ALL
                .iter()
                .map(|z| {
                    let (count, efficiency) = stats.serves_from(Some(*z), filter);
                    ServeStartDetail {
                        start: *z,
                        count,
                        share: f64::from(count) / f64::from(told) * 100.0,
                        efficiency: (count >= MIN_SERVES_FOR_EFFICIENCY).then_some(efficiency),
                        errors: stats.serve_errors_from(*z, filter),
                    }
                })
                .collect(),
        };
        let paths = match event_type {
            EventTypeEnum::S => stats.serve_paths(start, filter),
            _ => Vec::new(),
        };
        let both: f64 = paths.iter().map(|(_, _, count, _)| count).sum();
        let mut serve_paths: Vec<ServePath> = paths
            .into_iter()
            .map(|(start, zone, count, efficiency)| ServePath {
                start,
                zone,
                count,
                share: count / both * 100.0,
                efficiency: (count >= f64::from(MIN_SERVES_FOR_EFFICIENCY)).then_some(efficiency),
            })
            .collect();
        // the most first; in halves: an integer to sort by
        serve_paths.sort_by_key(|p| std::cmp::Reverse((p.count * 2.0).round() as i64));
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
            targets_told,
            targets,
            conflicts,
            on_conflicts,
            blockers_told,
            blockers,
            serve_starts_told,
            serve_starts,
            serve_paths,
        }
    }
}

impl Freeballs {
    /// Team events, ours and theirs: with a player, none given and the
    /// others not told.
    fn of(stats: &Stats, filter: &StatsFilter) -> Self {
        let team = filter.player.is_none();
        Freeballs {
            given: stats.event_count(EventTypeEnum::FB, None, filter),
            received: team.then(|| stats.event_count(EventTypeEnum::OF, None, filter)),
            exploited: match team {
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
            // the situation too: only the player is theirs
            let only = StatsFilter {
                player: Some(player),
                ..*filter
            };
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
    by_rotation_in(stats, &StatsFilter::default())
}

/// Every rotation narrowed by the filter (its rotation aside): a phase
/// asked leaves the other one empty.
pub fn by_rotation_in(stats: &Stats, filter: &StatsFilter) -> Vec<RotationLine> {
    (0..6)
        .map(|rotation| {
            let in_rotation = StatsFilter {
                rotation: Some(rotation),
                ..*filter
            };
            let phase =
                |p| crate::overview::in_phase(&in_rotation, p).and_then(|f| stats.rallies_won(&f));
            RotationLine {
                rotation,
                side_out: phase(PhaseEnum::SideOut),
                break_points: phase(PhaseEnum::Break),
            }
        })
        .collect()
}
