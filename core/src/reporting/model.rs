//! What the match report shows, computed from the match: the values are
//! already formatted, and the typst template (`report.typ`) only lays them
//! out, holding all the styling.
//!
//! The teams are always in the order of the scoresheet, the home team first
//! (see `sides`).
use crate::errors::AppError;
use crate::localization::Labels;
use crate::shapes::enums::{
    EvalEnum, EventTypeEnum, FriendlyName, PhaseEnum, TeamSideEnum, ZoneEnum,
};
use crate::shapes::r#match::MatchEntry;
use crate::shapes::player::PlayerEntry;
use crate::shapes::set::SetEntry;
use crate::shapes::snapshot::Snapshot;
use crate::shapes::stats::{Metric, Stats, StatsFilter};
use serde::Serialize;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct Report {
    pub labels: ReportLabels,
    pub header: Header,
    pub kpis: Vec<Kpi>,
    /// the score, partials and substitutions of each set
    pub sets: Table,
    /// the groups of columns of the statistics, shared by `players`,
    /// `rotations` and `set_stats` (see `stats_values`)
    pub stats_groups: Vec<Group>,
    pub players: StatsTable,
    pub rotations: StatsTable,
    pub set_stats: StatsTable,
    pub courts: Vec<Court>,
    pub first_rallies: Table,
    pub attack_by_phase: Table,
    pub freeballs: Table,
    pub points_won: Table,
    pub points_lost: Table,
    pub point_sequence: PointSequence,
}

/// The texts of the template that don't come with the data.
#[derive(Debug, Serialize)]
pub struct ReportLabels {
    pub created_with: &'static str,
    pub legend: &'static str,
    pub setter: &'static str,
    pub starting_position: &'static str,
    pub from_the_bench: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Header {
    pub date: String,
    pub teams: [TeamHeader; 2],
}

#[derive(Debug, Serialize)]
pub struct TeamHeader {
    pub name: String,
    pub sets: u8,
    pub us: bool,
}

/// A key figure of the match, with the numbers behind it.
#[derive(Debug, Serialize)]
pub struct Kpi {
    pub label: String,
    pub value: String,
    pub detail: String,
}

/// A table with a header row: the first column holds the labels of the
/// rows, and the last row is the total when `total` is set.
#[derive(Debug, Serialize)]
pub struct Table {
    pub title: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub total: bool,
}

/// A group of columns under a label.
#[derive(Debug, Serialize)]
pub struct Group {
    pub label: String,
    pub columns: Vec<String>,
    /// the column takes the width left by the others (e.g. the names)
    pub fill: bool,
}

/// A table of statistics: the leading groups of columns, then the
/// `Report::stats_groups`.
#[derive(Debug, Serialize)]
pub struct StatsTable {
    pub title: String,
    pub lead: Vec<Group>,
    pub rows: Vec<StatsRow>,
}

#[derive(Debug, Serialize)]
pub struct StatsRow {
    pub lead: Vec<Cell>,
    pub values: Vec<String>,
    pub total: bool,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Cell {
    Text {
        text: String,
    },
    /// a player in the starting lineup of a set (1 to 6)
    Start {
        position: usize,
        setter: bool,
    },
    /// a player entering the set from the bench
    Bench,
    Empty,
}

impl Cell {
    fn text<T: Into<String>>(text: T) -> Self {
        Cell::Text { text: text.into() }
    }
}

/// The front row (4, 3, 2) and the back row (8, 9) of the court, each
/// zone with a figure and the one behind it.
#[derive(Debug, Serialize)]
pub struct Court {
    pub title: String,
    pub zones: Vec<Zone>,
}

#[derive(Debug, Serialize)]
pub struct Zone {
    pub zone: u8,
    pub top: String,
    pub bottom: String,
}

#[derive(Debug, Serialize)]
pub struct PointSequence {
    pub title: String,
    pub teams: [String; 2],
    pub sets: Vec<SetPoints>,
}

#[derive(Debug, Serialize)]
pub struct SetPoints {
    pub label: String,
    pub points: Vec<Point>,
}

/// A point: the team scoring it (its index in `PointSequence::teams`) and
/// the score it reaches.
#[derive(Debug, PartialEq, Serialize)]
pub struct Point {
    pub team: usize,
    pub us: bool,
    pub score: u8,
}

/// The sets that replay, with their snapshot, and the statistics of the
/// whole match.
struct Replay<'a> {
    sets: Vec<(&'a SetEntry, Snapshot)>,
    stats: Stats,
}

impl<'a> Replay<'a> {
    fn new(m: &'a MatchEntry) -> Result<Self, AppError> {
        let sets = m
            .sets
            .iter()
            .map(|set| {
                set.compute_snapshot(m.max_substitutions)
                    .map(|snapshot| (set, snapshot))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let stats = sets.iter().fold(Stats::new(), |mut stats, (_, snapshot)| {
            stats.merge(&snapshot.stats);
            stats
        });
        Ok(Replay { sets, stats })
    }
}

pub fn build_report(m: &MatchEntry, labels: &Labels) -> Result<Report, AppError> {
    let replay = Replay::new(m)?;
    let status = m.get_status()?;
    let [left, right] = sides(
        m,
        (m.team.name.clone(), status.us_wins, true),
        (m.opponent.clone(), status.them_wins, false),
    )
    .map(|(name, sets, us)| TeamHeader { name, sets, us });
    Ok(Report {
        labels: ReportLabels {
            created_with: labels.created_with,
            legend: labels.report_legend,
            setter: labels.setter,
            starting_position: labels.starting_position,
            from_the_bench: labels.from_the_bench,
        },
        header: Header {
            date: m.date.format("%d/%m/%Y").to_string(),
            teams: [left, right],
        },
        kpis: kpis(labels, &replay.stats),
        sets: sets_table(labels, m, &replay),
        stats_groups: stats_groups(labels),
        players: players_table(labels, m, &replay),
        rotations: rotations_table(labels, &replay.stats),
        set_stats: set_stats_table(labels, &replay),
        courts: courts(labels, &replay.stats),
        first_rallies: first_rallies_table(labels, &replay.stats),
        attack_by_phase: attack_by_phase_table(labels, &replay.stats),
        freeballs: freeballs_table(labels, &replay.stats),
        points_won: points_won_table(labels, &replay.stats),
        points_lost: points_lost_table(labels, &replay.stats),
        point_sequence: point_sequence(labels, m, &replay),
    })
}

/// The values of us and them, in the order of the scoresheet: the home
/// team first.
fn sides<T>(m: &MatchEntry, us: T, them: T) -> [T; 2] {
    match m.home {
        true => [us, them],
        false => [them, us],
    }
}

fn kpis(labels: &Labels, stats: &Stats) -> Vec<Kpi> {
    use EventTypeEnum::*;
    let all = StatsFilter::default();
    let phase = |phase: PhaseEnum| {
        let won = stats.rallies_won(&StatsFilter::new(Some(phase), None, None));
        Kpi {
            label: phase.friendly_name(labels).to_string(),
            value: fmt_pct(won.map(|(v, _, _)| v)),
            detail: won.map_or(String::new(), |(_, won, total)| format!("{won}/{total}")),
        }
    };
    let detail = |label: &str, value: Option<f64>| format!("{label} {}", fmt_pct(value));
    vec![
        phase(PhaseEnum::SideOut),
        phase(PhaseEnum::Break),
        Kpi {
            label: labels.attack.to_string(),
            value: fmt_pct(stats.event_percentage(A, EvalEnum::Perfect, &all)),
            detail: detail(
                labels.eff_perc,
                stats.metric_percentage(A, Metric::Efficiency, &all),
            ),
        },
        Kpi {
            label: labels.reception.to_string(),
            value: fmt_pct(stats.metric_percentage(P, Metric::Positive, &all)),
            detail: detail(
                labels.prf_perc,
                stats.event_percentage(P, EvalEnum::Perfect, &all),
            ),
        },
        Kpi {
            label: labels.serve.to_string(),
            value: format!(
                "{} / {}",
                stats.event_count(S, Some(EvalEnum::Perfect), &all),
                stats.event_count(S, Some(EvalEnum::Error), &all)
            ),
            detail: format!("{} / {}", labels.ace, labels.errors),
        },
        Kpi {
            label: labels.errors.to_string(),
            value: stats.total_errors(None, &all).to_string(),
            detail: format!(
                "{} {}",
                labels.opponent_errors,
                stats.event_count(OE, None, &all)
            ),
        },
    ]
}

fn sets_table(labels: &Labels, m: &MatchEntry, replay: &Replay) -> Table {
    let score = |(us, them): (u8, u8)| {
        let [left, right] = sides(m, us, them);
        format!("{left}-{right}")
    };
    let rows = replay
        .sets
        .iter()
        .map(|(set, snapshot)| {
            let runs = Runs::new(&snapshot.points);
            let pair = |(us, them): (u8, u8)| {
                let [left, right] = sides(m, us, them);
                format!("{left} · {right}")
            };
            vec![
                set.set_number.to_string(),
                score((snapshot.score_us, snapshot.score_them)),
                snapshot
                    .partials
                    .iter()
                    .map(|p| score(*p))
                    .collect::<Vec<_>>()
                    .join("  "),
                substitutions(m, snapshot),
                pair(runs.longest),
                pair(runs.max_lead),
            ]
        })
        .collect();
    Table {
        title: String::new(),
        columns: vec![
            "set".to_string(),
            labels.set_score.to_string(),
            labels.partials.to_string(),
            labels.substitutions.to_string(),
            labels.longest_run.to_string(),
            labels.max_lead.to_string(),
        ],
        rows,
        total: false,
    }
}

/// The substitutions of a set: the number of the player leaving, then the
/// one of the player entering.
fn substitutions(m: &MatchEntry, snapshot: &Snapshot) -> String {
    let number = |id: &Uuid| m.team.find_player(*id).map(|p| p.number);
    snapshot
        .current_lineup
        .get_substitutions()
        .iter()
        .map(|s| match (number(&s.replaced), number(&s.replacement)) {
            (Some(replaced), Some(replacement)) => format!("{replaced}<{replacement}"),
            _ => "-".to_string(),
        })
        .collect::<Vec<_>>()
        .join("  ")
}

/// The longest run of points and the largest lead of each team in a set,
/// as `(us, them)`.
#[derive(Debug, PartialEq)]
struct Runs {
    longest: (u8, u8),
    max_lead: (u8, u8),
}

impl Runs {
    fn new(points: &[TeamSideEnum]) -> Self {
        let (longest, max_lead, _, _) = points.iter().fold(
            ((0, 0), (0, 0), (None, 0u8), 0i16),
            |(longest, max_lead, (last, run), diff), side| {
                let run = match last == Some(*side) {
                    true => run + 1,
                    false => 1,
                };
                let diff = match side {
                    TeamSideEnum::Us => diff + 1,
                    TeamSideEnum::Them => diff - 1,
                };
                let longest = match side {
                    TeamSideEnum::Us => (longest.0.max(run), longest.1),
                    TeamSideEnum::Them => (longest.0, longest.1.max(run)),
                };
                let max_lead = (
                    max_lead.0.max(diff.max(0) as u8),
                    max_lead.1.max((-diff).max(0) as u8),
                );
                (longest, max_lead, (Some(*side), run), diff)
            },
        );
        Runs { longest, max_lead }
    }
}

/// The groups of columns of the statistics: see `stats_values`.
fn stats_groups(labels: &Labels) -> Vec<Group> {
    let group = |label: &str, columns: &[&str]| Group {
        label: label.to_string(),
        columns: columns.iter().map(|c| c.to_string()).collect(),
        fill: false,
    };
    vec![
        group(labels.points, &[labels.tot, "bp", labels.won_lost]),
        group(
            labels.serve,
            &[labels.tot, labels.err, labels.pt, labels.eff_perc],
        ),
        group(
            labels.reception,
            &[
                labels.tot,
                labels.err,
                labels.pos_perc,
                labels.prf_perc,
                labels.eff_perc,
            ],
        ),
        group(
            labels.attack,
            &[
                labels.tot,
                labels.err,
                labels.blk,
                labels.pt,
                labels.pt_perc,
                labels.eff_perc,
            ],
        ),
        group(labels.block, &[labels.pt, labels.tch]),
        group(labels.defense, &[labels.tot, labels.pos_perc]),
        group("", &[labels.flt, labels.fb, labels.opp]),
    ]
}

/// The statistics shown for a player, a rotation, a set or the match, in
/// the columns of `stats_groups`:
/// - points: total (aces, kills, block points), break points, points minus
///   errors (faults included);
/// - serve: total, errors, aces, efficiency;
/// - reception: total, errors, positiveness, perfect, efficiency;
/// - attack: total, errors (out or net), blocked, kills, kills %,
///   efficiency;
/// - block: points, touches;
/// - defense: total, positiveness;
/// - faults, freeballs given (the ones received are in `freeballs_table`),
///   opponent errors: they have no player, a player row shows none.
fn stats_values(stats: &Stats, filter: &StatsFilter) -> Vec<String> {
    use EvalEnum::*;
    use EventTypeEnum::*;
    let count = |event_type, eval| fmt_count(stats.event_count(event_type, eval, filter));
    let pct = |event_type, eval| fmt_num(stats.event_percentage(event_type, eval, filter));
    let metric = |event_type, metric| fmt_num(stats.metric_percentage(event_type, metric, filter));
    let break_filter = StatsFilter {
        phase: Some(PhaseEnum::Break),
        ..*filter
    };
    vec![
        fmt_count(stats.points(filter)),
        fmt_count(stats.points(&break_filter)),
        stats
            .won_minus_lost(filter)
            .map_or("-".to_string(), |v| v.to_string()),
        count(S, None),
        count(S, Some(Error)),
        count(S, Some(Perfect)),
        metric(S, Metric::Efficiency),
        count(P, None),
        count(P, Some(Error)),
        metric(P, Metric::Positive),
        pct(P, Perfect),
        metric(P, Metric::Efficiency),
        count(A, None),
        count(A, Some(Error)),
        count(A, Some(Over)),
        count(A, Some(Perfect)),
        pct(A, Perfect),
        metric(A, Metric::Efficiency),
        count(B, Some(Perfect)),
        count(B, Some(Positive)),
        count(D, None),
        metric(D, Metric::Positive),
        count(F, None),
        count(FB, None),
        match filter.player {
            Some(_) => "-".to_string(),
            None => count(OE, None),
        },
    ]
}

/// The players who played, by number, with their position in the starting
/// lineup of each set, then the total of the team.
fn players_table(labels: &Labels, m: &MatchEntry, replay: &Replay) -> StatsTable {
    let mut players: Vec<&PlayerEntry> = replay
        .sets
        .iter()
        .flat_map(|(_, snapshot)| snapshot.current_lineup.get_involved_players())
        .collect::<HashSet<_>>()
        .into_iter()
        .filter_map(|id| m.team.find_player(id))
        .collect();
    players.sort_by_key(|p| p.number);
    let rows = players.iter().map(|player| {
        let mut lead = vec![
            Cell::text(player.number.to_string()),
            Cell::text(&player.name),
        ];
        lead.extend(
            replay
                .sets
                .iter()
                .map(|(set, snapshot)| set_cell(set, snapshot, &player.id)),
        );
        StatsRow {
            lead,
            values: stats_values(
                &replay.stats,
                &StatsFilter::new(None, None, Some(player.id)),
            ),
            total: false,
        }
    });
    let total = StatsRow {
        lead: [Cell::Empty, Cell::text(labels.total)]
            .into_iter()
            .chain(replay.sets.iter().map(|_| Cell::Empty))
            .collect(),
        values: stats_values(&replay.stats, &StatsFilter::default()),
        total: true,
    };
    StatsTable {
        title: labels.players.to_string(),
        lead: vec![
            Group {
                label: String::new(),
                columns: vec!["#".to_string()],
                fill: false,
            },
            Group {
                label: String::new(),
                columns: vec![labels.player.to_string()],
                fill: true,
            },
            Group {
                label: "set".to_string(),
                columns: replay
                    .sets
                    .iter()
                    .map(|(set, _)| set.set_number.to_string())
                    .collect(),
                fill: false,
            },
        ],
        rows: rows.chain(std::iter::once(total)).collect(),
    }
}

/// A player in a set: the position in the starting lineup (the setter
/// marked), entering from the bench, or not playing. The libero is never
/// counted as entering from the bench.
fn set_cell(set: &SetEntry, snapshot: &Snapshot, player: &Uuid) -> Cell {
    let lineup = &snapshot.current_lineup;
    let position = set.initial_positions.iter().position(|p| p == player);
    let from_the_bench = set.libero != *player
        && lineup.get_involved_players().contains(player)
        && lineup.was_player_already_used(player);
    match (position, from_the_bench) {
        (Some(p), _) => Cell::Start {
            position: p + 1,
            setter: set.setter == *player,
        },
        (None, true) => Cell::Bench,
        (None, false) => Cell::Empty,
    }
}

/// The leading groups of a table of the rallies won by phase, then the
/// statistics (see `phase_row`).
fn phases_lead(labels: &Labels, title: &str) -> Vec<Group> {
    vec![
        Group {
            label: String::new(),
            columns: vec![title.to_string()],
            fill: true,
        },
        Group {
            label: labels.phase.to_string(),
            columns: vec![labels.so_perc.to_string(), labels.bp_perc.to_string()],
            fill: false,
        },
    ]
}

/// A row with the rallies won in side out and in break, then the
/// statistics.
fn phase_row(label: String, stats: &Stats, filter: StatsFilter, total: bool) -> StatsRow {
    let won = |phase| {
        fmt_num(
            stats
                .rallies_won(&StatsFilter {
                    phase: Some(phase),
                    ..filter
                })
                .map(|(v, _, _)| v),
        )
    };
    StatsRow {
        lead: vec![
            Cell::text(label),
            Cell::text(won(PhaseEnum::SideOut)),
            Cell::text(won(PhaseEnum::Break)),
        ],
        values: stats_values(stats, &filter),
        total,
    }
}

fn rotations_table(labels: &Labels, stats: &Stats) -> StatsTable {
    let rows = (0..6)
        .map(|rotation| {
            phase_row(
                format!("{}{}", labels.setter_prefix, rotation + 1),
                stats,
                StatsFilter::new(None, Some(rotation), None),
                false,
            )
        })
        .chain(std::iter::once(phase_row(
            labels.total.to_string(),
            stats,
            StatsFilter::default(),
            true,
        )))
        .collect();
    StatsTable {
        title: labels.rotation.to_string(),
        lead: phases_lead(labels, labels.rotation),
        rows,
    }
}

fn set_stats_table(labels: &Labels, replay: &Replay) -> StatsTable {
    let rows = replay
        .sets
        .iter()
        .map(|(set, snapshot)| {
            phase_row(
                format!("set {}", set.set_number),
                &snapshot.stats,
                StatsFilter::default(),
                false,
            )
        })
        .chain(std::iter::once(phase_row(
            labels.total.to_string(),
            &replay.stats,
            StatsFilter::default(),
            true,
        )))
        .collect();
    StatsTable {
        title: "set".to_string(),
        lead: phases_lead(labels, "set"),
        rows,
    }
}

/// The zones of the courts, in the order the template lays them out.
const COURT_ZONES: [(ZoneEnum, u8); 5] = [
    (ZoneEnum::Four, 4),
    (ZoneEnum::Three, 3),
    (ZoneEnum::Two, 2),
    (ZoneEnum::Eight, 8),
    (ZoneEnum::Nine, 9),
];

/// The distribution of the attacks in side out and in break (the share set
/// to each zone, then its kills %), and the conversion of the
/// counter-attacks (kills %, then kills over counter-attacks).
fn courts(labels: &Labels, stats: &Stats) -> Vec<Court> {
    let court = |title: String, value: &dyn Fn(ZoneEnum) -> Option<(String, String)>| Court {
        title,
        zones: COURT_ZONES
            .iter()
            .map(|&(zone, number)| {
                let (top, bottom) = value(zone).unwrap_or(("-".to_string(), String::new()));
                Zone {
                    zone: number,
                    top,
                    bottom,
                }
            })
            .collect(),
    };
    let distribution = |phase: PhaseEnum| {
        let filter = StatsFilter::new(Some(phase), None, None);
        court(
            format!("{} · {}", labels.distribution, phase.friendly_name(labels)),
            &move |zone| {
                stats
                    .zone_distribution(zone, &filter)
                    .map(|(share, kills)| (fmt_pct(Some(share)), fmt_pct(Some(kills))))
            },
        )
    };
    vec![
        distribution(PhaseEnum::SideOut),
        distribution(PhaseEnum::Break),
        court(labels.counter_attack_conversion_rate.to_string(), &|zone| {
            stats
                .counter_attack_conversion(zone, &StatsFilter::default())
                .map(|(v, total, kills)| (fmt_pct(Some(v)), format!("{kills}/{total}")))
        }),
    ]
}

/// The first rallies of the side outs: all of them, then by reception.
fn first_rallies_table(labels: &Labels, stats: &Stats) -> Table {
    let receptions = [
        (None, labels.global),
        (Some(EvalEnum::Perfect), labels.perfect),
        (Some(EvalEnum::Positive), labels.positive),
        (Some(EvalEnum::Exclamative), labels.subpositive),
        (Some(EvalEnum::Negative), labels.negative),
    ];
    let rows = receptions
        .iter()
        .map(|(eval, label)| {
            let values = match stats.first_rallies(*eval, &StatsFilter::default()) {
                Some(s) => vec![
                    s.total.to_string(),
                    match s.unforced_errors {
                        0 => fmt_count(s.errors),
                        unforced => format!("{}({})", s.errors, unforced),
                    },
                    fmt_count(s.blocked),
                    s.won.to_string(),
                    fmt_num(Some(s.won_percentage())),
                    fmt_num(Some(s.efficiency())),
                ],
                None => vec!["-".to_string(); 6],
            };
            std::iter::once(label.to_string()).chain(values).collect()
        })
        .collect();
    Table {
        title: labels.sideout_on_first_rally.to_string(),
        columns: vec![
            String::new(),
            labels.tot.to_string(),
            format!("{}({})", labels.err, labels.unf),
            labels.blk.to_string(),
            labels.pt.to_string(),
            labels.pt_perc.to_string(),
            labels.eff_perc.to_string(),
        ],
        rows,
        total: false,
    }
}

/// The attacks built on our reception and in counter-attack.
fn attack_by_phase_table(labels: &Labels, stats: &Stats) -> Table {
    let rows = [
        (false, labels.after_reception),
        (true, labels.counter_attack),
    ]
    .iter()
    .map(|(counter_attack, label)| {
        let values = match stats.attack_summary(*counter_attack, &StatsFilter::default()) {
            Some(s) => vec![
                s.total.to_string(),
                fmt_count(s.errors),
                fmt_count(s.blocked),
                fmt_count(s.kills),
                fmt_num(Some(s.kill_percentage())),
                fmt_num(Some(s.efficiency())),
            ],
            None => vec!["-".to_string(); 6],
        };
        std::iter::once(label.to_string()).chain(values).collect()
    })
    .collect();
    Table {
        title: labels.attack_by_phase.to_string(),
        columns: vec![
            String::new(),
            labels.tot.to_string(),
            labels.err.to_string(),
            labels.blk.to_string(),
            labels.pt.to_string(),
            labels.pt_perc.to_string(),
            labels.eff_perc.to_string(),
        ],
        rows,
        total: false,
    }
}

/// The freeballs, in all the rallies and by phase: the ones we gave
/// (`FB`), the ones they gave us (`OF`), and how many of these we exploited
/// (see `Snapshot::record_freeball`).
fn freeballs_table(labels: &Labels, stats: &Stats) -> Table {
    let phases = [
        (None, labels.global),
        (
            Some(PhaseEnum::SideOut),
            PhaseEnum::SideOut.friendly_name(labels),
        ),
        (
            Some(PhaseEnum::Break),
            PhaseEnum::Break.friendly_name(labels),
        ),
    ];
    let rows = phases
        .iter()
        .map(|(phase, label)| {
            let filter = StatsFilter::new(*phase, None, None);
            let exploitation = stats.freeball_exploitation(&filter);
            vec![
                label.to_string(),
                fmt_count(stats.event_count(EventTypeEnum::FB, None, &filter)),
                fmt_count(stats.event_count(EventTypeEnum::OF, None, &filter)),
                fmt_count(exploitation.map_or(0, |(_, _, exploited)| exploited)),
                fmt_num(exploitation.map(|(perc, _, _)| perc)),
            ]
        })
        .collect();
    Table {
        title: labels.freeballs.to_string(),
        columns: vec![
            String::new(),
            labels.freeballs_given.to_string(),
            labels.freeballs_received.to_string(),
            labels.freeballs_exploited.to_string(),
            "%".to_string(),
        ],
        rows,
        total: false,
    }
}

/// A table of where the points come from: a row per source, with its
/// share, then the total.
fn origin_table(labels: &Labels, title: &str, sources: Vec<(&str, u32)>) -> Table {
    let total: u32 = sources.iter().map(|(_, n)| n).sum();
    let share = |n: u32| fmt_num((n > 0).then(|| n as f64 / total as f64 * 100.0));
    let rows = sources
        .iter()
        .map(|(label, n)| vec![label.to_string(), fmt_count(*n), share(*n)])
        .chain(std::iter::once(vec![
            labels.total.to_string(),
            fmt_count(total),
            share(total),
        ]))
        .collect();
    Table {
        title: title.to_string(),
        columns: vec![String::new(), labels.tot.to_string(), "%".to_string()],
        rows,
        total: true,
    }
}

/// Our points: aces, kills, block points, their errors.
fn points_won_table(labels: &Labels, stats: &Stats) -> Table {
    use EventTypeEnum::*;
    let all = StatsFilter::default();
    let count = |event_type, eval| stats.event_count(event_type, eval, &all);
    origin_table(
        labels,
        labels.points_won,
        vec![
            (labels.ace, count(S, Some(EvalEnum::Perfect))),
            (labels.attack, count(A, Some(EvalEnum::Perfect))),
            (labels.block, count(B, Some(EvalEnum::Perfect))),
            (labels.opponent_errors, count(OE, None)),
        ],
    )
}

/// Their points: their own ones, then our errors by fundamental and our
/// faults (see `EventTypeEnum::rally_winner`).
fn points_lost_table(labels: &Labels, stats: &Stats) -> Table {
    use EvalEnum::*;
    use EventTypeEnum::*;
    let all = StatsFilter::default();
    let count = |event_type, eval| stats.event_count(event_type, eval, &all);
    origin_table(
        labels,
        labels.points_lost,
        vec![
            (labels.opponent_score, count(OS, None)),
            (labels.serve, count(S, Some(Error))),
            (labels.reception, count(P, Some(Error))),
            (labels.attack, count(A, Some(Error))),
            (labels.attack_blocked, count(A, Some(Over))),
            (labels.block, count(B, Some(Error)) + count(B, Some(Over))),
            (labels.defense, count(D, Some(Error))),
            (labels.faults, count(F, None)),
        ],
    )
}

fn point_sequence(labels: &Labels, m: &MatchEntry, replay: &Replay) -> PointSequence {
    let [left, _] = sides(m, TeamSideEnum::Us, TeamSideEnum::Them);
    PointSequence {
        title: labels.point_sequence.to_string(),
        teams: sides(m, m.team.name.clone(), m.opponent.clone()),
        sets: replay
            .sets
            .iter()
            .map(|(set, snapshot)| SetPoints {
                label: format!("set {}", set.set_number),
                points: snapshot
                    .point_sequence()
                    .into_iter()
                    .map(|(side, score)| Point {
                        team: match side == left {
                            true => 0,
                            false => 1,
                        },
                        us: side == TeamSideEnum::Us,
                        score,
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// A percentage with its sign, rounded.
fn fmt_pct(value: Option<f64>) -> String {
    value
        .map(|v| format!("{}%", v.round() as i64))
        .unwrap_or_else(|| "-".to_string())
}

/// A percentage in a column whose header tells it is one, rounded.
fn fmt_num(value: Option<f64>) -> String {
    value
        .map(|v| (v.round() as i64).to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn fmt_count(count: u32) -> String {
    match count {
        0 => "-".to_string(),
        count => count.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{localization::labels, shapes::enums::LanguageEnum};
    use TeamSideEnum::*;

    #[test]
    fn runs_and_leads_of_each_team() {
        // 1-0 2-0 2-1 2-2 2-3 2-4 2-5 3-5
        let points = [Us, Us, Them, Them, Them, Them, Them, Us];
        assert_eq!(
            Runs::new(&points),
            Runs {
                longest: (2, 5),
                max_lead: (2, 3),
            }
        );
        assert_eq!(
            Runs::new(&[]),
            Runs {
                longest: (0, 0),
                max_lead: (0, 0),
            }
        );
    }

    /// Rounded, with no negative zero; nothing to show is a dash.
    #[test]
    fn numbers_are_rounded() {
        assert_eq!(fmt_pct(Some(54.8)), "55%");
        assert_eq!(fmt_pct(Some(-0.4)), "0%");
        assert_eq!(fmt_pct(None), "-");
        assert_eq!(fmt_num(Some(33.3)), "33");
        assert_eq!(fmt_num(Some(-50.0)), "-50");
        assert_eq!(fmt_num(Some(-0.4)), "0");
        assert_eq!(fmt_num(None), "-");
        assert_eq!(fmt_count(0), "-");
        assert_eq!(fmt_count(7), "7");
    }

    #[test]
    fn every_statistics_row_fills_the_groups() {
        let columns: usize = stats_groups(labels(LanguageEnum::En))
            .iter()
            .map(|g| g.columns.len())
            .sum();
        assert_eq!(
            stats_values(&Stats::new(), &StatsFilter::default()).len(),
            columns
        );
    }
}
