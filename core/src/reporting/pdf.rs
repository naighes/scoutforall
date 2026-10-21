use crate::errors::{AppError, IOError};
use crate::localization::{Labels, current_labels};
use crate::reporting::align::Align;
use crate::reporting::circle::Circle;
use crate::reporting::court::render_court;
use crate::reporting::stack::{Stack, StackDirection};
use crate::reporting::table_row::{Cell, Row};
use crate::reporting::text::Text;
use crate::reporting::typst_content::TypstContent;
use crate::reporting::typst_world::ReportWorld;
use crate::reporting::util::escape_text;
use crate::shapes::enums::{EvalEnum, EventTypeEnum, FriendlyName, PhaseEnum, TeamSideEnum};
use crate::shapes::r#match::{MatchEntry, MatchStatus};
use crate::shapes::player::PlayerEntry;
use crate::shapes::set::SetEntry;
use crate::shapes::snapshot::Snapshot;
use crate::shapes::stats::{Metric, Stats, StatsFilter};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};
use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;
use uuid::Uuid;

pub const DEFAULT_FONT_SIZE: u8 = 8;
pub const WHITE: &str = "#ffffff";
pub const BLACK: &str = "#000000";
pub const LIGHT_GRAY: &str = "#cccccc";
pub const GRAY: &str = "#666666";
pub const ALTERNATE_COLOR: &str = "#efefef";

pub fn open_match_pdf(m: &MatchEntry) -> Result<(), AppError> {
    let pdf = render_match_pdf(m)?;
    let mut path: PathBuf = env::temp_dir();
    let uid = Uuid::new_v4().to_string();
    path.push(format!("{}_{}.pdf", m.id, uid));
    fs::write(&path, pdf).map_err(|e| AppError::IO(IOError::from(e)))?;
    open_with_system_viewer(&path)
}

pub fn render_match_pdf(m: &MatchEntry) -> Result<Vec<u8>, AppError> {
    let content = render_match_typst(m)?;
    let world = ReportWorld::new(content);
    let document = typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|e| typst_error("error compiling typst", &e))?;
    typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|e| typst_error("error exporting PDF", &e))
}

/// The typst source of the match report.
pub fn render_match_typst(m: &MatchEntry) -> Result<String, AppError> {
    let mut content = String::new();
    let created_with = current_labels().created_with;
    let match_status = m.get_status()?;
    let mut aggregated_stats = Stats::new();
    let mut players: HashMap<Uuid, HashMap<u8, bool>> = HashMap::new();
    let sets: Vec<(&SetEntry, Snapshot)> = m
        .sets
        .iter()
        .filter_map(|set| {
            set.compute_snapshot(m.max_substitutions)
                .ok()
                .map(|snapshot| {
                    aggregated_stats.merge(&snapshot.stats);
                    for &p in snapshot.current_lineup.get_involved_players().iter() {
                        let entered_late = if set.libero == p {
                            false
                        } else {
                            snapshot.current_lineup.was_player_already_used(&p)
                        };
                        players
                            .entry(p)
                            .or_default()
                            .insert(set.set_number, entered_late);
                    }

                    (set, snapshot)
                })
        })
        .collect();
    let mut players: Vec<_> = players
        .iter()
        .filter_map(|(&id, set_map)| m.team.find_player(id).map(|p| (p, set_map.clone())))
        .collect();
    players.sort_by_key(|(p, _)| p.number);
    content.push_str("#import table: cell, header\n");
    content.push_str(&format!(
        r#"
#set page(
  margin: (top: 1cm, right: 0.5cm, bottom: 1cm, left: 0.5cm)
)
#set text({DEFAULT_FONT_SIZE}pt)
#import table: cell, header

#set page(
  footer: [
    #line(length: 100%)
    {created_with} *scout4all* (https://naighes.github.io/scoutforall/)
  ]
)
#let dst-perc-s(body) = text(
  size: 9pt,
  fill: olive,
  weight: "bold",
  body,
)
#let dst-perc(body) = text(
  size: 12pt,
  body
)
#let dst-perc-rect(body) = rect(
  height: 36pt,
  width: 36pt,
)
#let dst-perc-rect(..args, body) = rect(
  height: 36pt,
  width: 36pt,
  ..args,
  body,
)
"#
    ));
    content.push_str(&render_header(m, &match_status));
    content.push_str(&render_match_overview(m, &sets));
    content.push_str(&render_players_stats_table(m, &players, &aggregated_stats));
    content.push_str(&render_rotations_stats_table(&aggregated_stats));
    content.push_str(&render_global_stats_table(&aggregated_stats));
    content.push_str(&render_sets_stats_table(&sets));
    content.push_str(&render_bottom_stats(&aggregated_stats));
    Ok(content)
}

fn typst_error(context: &str, diagnostics: &[typst::diag::SourceDiagnostic]) -> AppError {
    let details: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
    AppError::IO(IOError::Msg(format!("{}: {}", context, details.join("; "))))
}

fn render_bottom_stats(aggregated_stats: &Stats) -> String {
    let sideout_stats = render_phase_stats(aggregated_stats);
    let freeball_stats = render_freeball_stats(aggregated_stats);
    let counter_attack_stats = render_counter_attack(aggregated_stats);
    let distribution_sideout_stats =
        render_distribution(aggregated_stats, Some(PhaseEnum::SideOut));
    let distribution_break_stats = render_distribution(aggregated_stats, Some(PhaseEnum::Break));
    format!(
        r#"
#grid(
  columns: (auto, 4pt, auto, 4pt, auto, 4pt, auto),
  {counter_attack_stats}
  text(""),
  {distribution_sideout_stats}
  text(""),
  {distribution_break_stats}
  text(""),
  stack(
    spacing: 4pt,
    {sideout_stats}
    {freeball_stats}
  ),
)
"#
    )
}

fn render_distribution(aggregated_stats: &Stats, phase: Option<PhaseEnum>) -> String {
    let filter = StatsFilter::new(phase, None, None);
    render_court(current_labels().distribution, phase, 6, |court, zone| {
        if let Some((share, kills)) = aggregated_stats.zone_distribution(zone, &filter) {
            court.set_zone(zone, Some(fmt_pct(Some(share))), Some(fmt_pct(Some(kills))));
        }
    })
}

fn render_counter_attack(aggregated_stats: &Stats) -> String {
    render_court(
        current_labels().counter_attack_conversion_rate,
        None,
        6,
        |court, zone| {
            if let Some((v, total, converted)) =
                aggregated_stats.counter_attack_conversion(zone, &StatsFilter::default())
            {
                let top = fmt_pct(Some(v));
                let bottom = format!("{}/{}", converted, total);
                court.set_zone(zone, Some(top), Some(bottom));
            }
        },
    )
}

fn render_header(m: &MatchEntry, match_status: &MatchStatus) -> String {
    const DATE_FONT_SIZE: u8 = 14;
    const TEAM_NAME_FONT_SIZE: u8 = 12;
    const SCORE_FONT_SIZE: u8 = 30;
    const TABLE_INSET: u8 = 8;
    let date_str = m.date.format("%a %d %b %Y").to_string();
    let team_left = if m.home {
        escape_text(&m.team.name)
    } else {
        escape_text(&m.opponent)
    };
    let team_right = if m.home {
        escape_text(&m.opponent)
    } else {
        escape_text(&m.team.name)
    };
    let (set_left, set_right) = match (match_status, m.home) {
        (status, true) => (status.us_wins, status.them_wins),
        (status, false) => (status.them_wins, status.us_wins),
    };
    let rows = Row::new(vec![
        Cell::new(Text::new("")),
        Cell::new(Text::new(&date_str).fill(WHITE).size(DATE_FONT_SIZE).bold())
            .colspan(3)
            .fill(GRAY)
            .align(Align::Center),
        Cell::new(Text::new("")),
        Cell::new(
            Text::new(&team_left)
                .size(TEAM_NAME_FONT_SIZE)
                .fill(GRAY)
                .bold(),
        )
        .align(Align::Right),
        Cell::new(
            Text::new(set_left.to_string())
                .size(SCORE_FONT_SIZE)
                .fill(WHITE)
                .bold(),
        )
        .align(Align::Center)
        .fill(GRAY),
        Cell::new(Text::new("-").size(SCORE_FONT_SIZE).fill(WHITE).bold())
            .align(Align::Center)
            .fill(GRAY),
        Cell::new(
            Text::new(set_right.to_string())
                .size(SCORE_FONT_SIZE)
                .fill(WHITE)
                .bold(),
        )
        .align(Align::Center)
        .fill(GRAY),
        Cell::new(
            Text::new(&team_right)
                .size(TEAM_NAME_FONT_SIZE)
                .fill(GRAY)
                .bold(),
        )
        .align(Align::Left),
    ])
    .render();
    format!(
        r#"
    #table(
  columns: (1fr, 32pt, 50pt, 32pt, 1fr),
  inset: {TABLE_INSET}pt,
  stroke: none,
  {rows}
)
"#
    )
}

fn snapshot_partials(snapshot: &Snapshot, m: &MatchEntry) -> String {
    snapshot
        .partials
        .iter()
        .map(|(us, them)| {
            if m.home {
                format!("{}-{}", us, them)
            } else {
                format!("{}-{}", them, us)
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn snapshot_substitutions(snapshot: &Snapshot, m: &MatchEntry) -> String {
    snapshot
        .current_lineup
        .get_substitutions()
        .iter()
        .map(|s| {
            let replaced = m.team.players.iter().find(|p| p.id == s.replaced);
            let replacement = m.team.players.iter().find(|p| p.id == s.replacement);
            if let (Some(replaced), Some(replacement)) = (replaced, replacement) {
                format!("{}<{}", replaced.number, replacement.number)
            } else {
                "-".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_match_overview(m: &MatchEntry, sets: &[(&SetEntry, Snapshot)]) -> String {
    const TABLE_INSET: u8 = 4;
    const TABLE_STROKE_SIZE: u8 = 1;
    let partials_label = escape_text(current_labels().partials);
    let substitutions_label = escape_text(current_labels().substitutions);
    let finals_label = escape_text(current_labels().finals);
    let mut rows = String::new();
    let mut score_us_total: u8 = 0;
    let mut score_them_total: u8 = 0;
    for (set, snapshot) in sets {
        let winner = snapshot.get_set_winner(set.set_number);
        let partials = escape_text(&snapshot_partials(snapshot, m));
        let substitutions = escape_text(&snapshot_substitutions(snapshot, m));
        let (score_left, score_right) = match (m.home, winner) {
            (true, Some(TeamSideEnum::Us)) => (
                Text::new(snapshot.score_us.to_string()).bold(),
                Text::new(snapshot.score_them.to_string()),
            ),
            (true, Some(TeamSideEnum::Them)) => (
                Text::new(snapshot.score_us.to_string()),
                Text::new(snapshot.score_them.to_string()).bold(),
            ),
            (false, Some(TeamSideEnum::Us)) => (
                Text::new(snapshot.score_them.to_string()),
                Text::new(snapshot.score_us.to_string()).bold(),
            ),
            (false, Some(TeamSideEnum::Them)) => (
                Text::new(snapshot.score_them.to_string()).bold(),
                Text::new(snapshot.score_us.to_string()),
            ),
            _ => (Text::new(""), Text::new("")),
        };
        score_us_total += snapshot.score_us;
        score_them_total += snapshot.score_them;
        let set_number = set.set_number;
        let row = Row::new(vec![
            Cell::new(Text::new(set_number.to_string())).align(Align::Center),
            Cell::new(Text::new(partials)).align(Align::Center),
            Cell::new(Text::new(substitutions)).align(Align::Center),
            Cell::new(
                Stack::new(StackDirection::LeftToRight)
                    .push(score_left)
                    .push(Text::new("-"))
                    .push(score_right),
            )
            .align(Align::Center),
        ])
        .render();
        rows.push_str(&row);
    }
    let (score_left_total, score_right_total) = match m.home {
        true => (score_us_total, score_them_total),
        false => (score_them_total, score_us_total),
    };
    let header_row = Row::new(vec![
        Cell::new(Text::new("set").bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(partials_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(substitutions_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(finals_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
    ])
    .render();
    let total_row = Row::new(vec![
        Cell::new(Text::new("")),
        Cell::new(Text::new("")),
        Cell::new(Text::new("")),
        Cell::new(
            Stack::new(StackDirection::LeftToRight)
                .push(Text::new(score_left_total.to_string()).bold())
                .push(Text::new("-"))
                .push(Text::new(score_right_total.to_string()).bold()),
        )
        .align(Align::Center)
        .fill(LIGHT_GRAY),
    ])
    .render();
    format!(
        r#"
#table(
  columns: (1fr, 1fr, 1fr, 1fr),
  inset: {TABLE_INSET}pt,
  stroke: {TABLE_STROKE_SIZE}pt,
  // headers
  {header_row}

  // rows
  {rows}

  // totals
  {total_row}
)
"#
    )
}

fn set_player_initial_position(
    m: &MatchEntry,
    set_number: u8,
    player_id: &Uuid,
    set_substitutions: &HashMap<u8, bool>,
) -> Box<dyn TypstContent> {
    let is_setter = m
        .sets
        .iter()
        .find(|s| s.set_number == set_number)
        .map(|s| s.setter == *player_id)
        .unwrap_or(false);
    let initial_position = m
        .sets
        .iter()
        .find(|s| s.set_number == set_number)
        .and_then(|s| s.initial_positions.iter().position(|p| p == player_id));
    let is_substitute = set_substitutions.get(&set_number).unwrap_or(&false);
    match (initial_position, is_setter, is_substitute) {
        (Some(pos), false, _) => Box::new(
            Circle::new()
                .stroke("1pt + black")
                .inset(0)
                .outset(2)
                .with_content(Text::new((pos + 1).to_string()).fill(BLACK)),
        ),
        (Some(pos), true, _) => Box::new(
            Circle::new()
                .stroke("1pt + black")
                .inset(0)
                .outset(2)
                .fill(BLACK)
                .with_content(Text::new((pos + 1).to_string()).fill(WHITE)),
        ),
        (None, _, true) => Box::new(
            Circle::new()
                .stroke("1pt + gray")
                .inset(0)
                .outset(1)
                .with_content(Text::new("").fill(WHITE)),
        ),
        _ => Box::new(Text::new("")),
    }
}

/// The right border separating the groups of columns.
const GROUP_STROKE: &str = "(right: (thickness: 1pt, dash: \"dashed\"))";

/// The widths of the columns of the statistics (see `stats_cells`).
const STATS_COLUMNS: [&str; 20] = [
    "2fr", "1fr", "3fr", // points
    "2fr", "2fr", "1fr", // serve
    "2fr", "2fr", "3fr", "2fr", "3fr", // reception
    "1fr", "2fr", "2fr", "2fr", "3fr", "2fr", // attack
    "1fr", "2fr", "2fr", // block, faults, freeballs given
];

/// The statistics shown for a player, a rotation, a set or the match:
/// - points: total (aces, kills, block points), break points, points minus
///   errors (faults included);
/// - serve: total, errors, aces;
/// - reception: total, errors, positiveness, perfect, efficiency;
/// - attack: total, errors (out or net), blocked, kills, kills %, efficiency;
/// - block points, faults, freeballs given (the ones received are in
///   `render_freeball_stats`).
fn stats_cells(stats: &Stats, filter: &StatsFilter, bg_color: &'static str) -> Vec<Cell> {
    use EvalEnum::*;
    use EventTypeEnum::*;
    let count = |event_type, eval| fmt_count(stats.event_count(event_type, eval, filter));
    let break_filter = StatsFilter {
        phase: Some(PhaseEnum::Break),
        ..*filter
    };
    let data = [
        fmt_count(stats.points(filter)),
        fmt_count(stats.points(&break_filter)),
        stats
            .won_minus_lost(filter)
            .map_or("-".to_string(), |v| v.to_string()),
        count(S, None),
        count(S, Some(Error)),
        count(S, Some(Perfect)),
        count(P, None),
        count(P, Some(Error)),
        fmt_pct(stats.metric_percentage(P, Metric::Positive, filter)),
        fmt_pct(stats.event_percentage(P, Perfect, filter)),
        fmt_pct(stats.metric_percentage(P, Metric::Efficiency, filter)),
        count(A, None),
        count(A, Some(Error)),
        count(A, Some(Over)),
        count(A, Some(Perfect)),
        fmt_pct(stats.event_percentage(A, Perfect, filter)),
        fmt_pct(stats.metric_percentage(A, Metric::Efficiency, filter)),
        count(B, Some(Perfect)),
        count(F, None),
        count(FB, None),
    ];
    // the last column of each group
    let group_ends = [2, 5, 10, 16, 18, 19];
    data.into_iter()
        .enumerate()
        .map(|(i, text)| {
            let cell = Cell::new(Text::new(text))
                .align(Align::Center)
                .fill(bg_color);
            if group_ends.contains(&i) {
                cell.stroke(GROUP_STROKE)
            } else {
                cell
            }
        })
        .collect()
}

/// The header rows of a statistics table: the given leading cells, then the
/// ones of the statistics (see `stats_cells`).
fn create_stats_headers(
    labels: &Labels,
    mut header_cells: Vec<Cell>,
    mut sub_header_cells: Vec<Cell>,
) -> (String, String) {
    let header_cell = |text: &str, colspan: u8| {
        Cell::new(Text::new(escape_text(text)).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .colspan(colspan)
    };
    header_cells.extend([
        header_cell(labels.points, 3),
        header_cell(labels.serve, 3),
        header_cell(labels.reception, 5),
        header_cell(labels.attack, 6),
        header_cell(labels.blk, 1),
        header_cell(labels.flt, 1),
        header_cell(labels.fb, 1),
    ]);
    sub_header_cells.extend([
        sub_header(labels.tot, false),
        sub_header("bp", false),
        sub_header(labels.won_lost, true),
        sub_header(labels.tot, false),
        sub_header(labels.err, false),
        sub_header(labels.pt, true),
        sub_header(labels.tot, false),
        sub_header(labels.err, false),
        sub_header(labels.pos_perc, false),
        sub_header(labels.prf_perc, false),
        sub_header(labels.eff_perc, true),
        sub_header(labels.tot, false),
        sub_header(labels.err, false),
        sub_header(labels.blk, false),
        sub_header(labels.pt, false),
        sub_header(labels.pt_perc, false),
        sub_header(labels.eff_perc, true),
        sub_header(labels.pt, false),
        sub_header(labels.tot, true),
        sub_header(labels.fb_given, false),
    ]);
    (
        Row::new(header_cells).render(),
        Row::new(sub_header_cells).render(),
    )
}

fn sub_header(text: &str, group_end: bool) -> Cell {
    let cell = Cell::new(Text::new(escape_text(text)).bold())
        .align(Align::Center)
        .fill(LIGHT_GRAY);
    if group_end {
        cell.stroke(GROUP_STROKE)
    } else {
        cell
    }
}

fn empty_header(colspan: u8) -> Cell {
    Cell::new(Text::new("")).fill(LIGHT_GRAY).colspan(colspan)
}

/// A table of statistics: the leading columns, then the statistics ones.
fn render_stats_table(
    leading_columns: &[&str],
    (header_row, sub_header_row): (String, String),
    rows: String,
) -> String {
    let columns = leading_columns
        .iter()
        .chain(STATS_COLUMNS.iter())
        .map(|c| format!("      {c},\n"))
        .collect::<String>();
    format!(
        r#"
#block(
  stroke: 1pt,
  table(
    columns: (
{columns}    ),
    inset:3pt,
    stroke: none,

    // group headers
    {header_row}

    // sub-headers
    {sub_header_row}

    {rows}
  )
)
"#
    )
}

fn render_players_stats_table(
    m: &MatchEntry,
    players: &[(&PlayerEntry, HashMap<u8, bool>)],
    aggregated_stats: &Stats,
) -> String {
    let labels = current_labels();
    let rows: String = players
        .iter()
        .enumerate()
        .map(|(i, (player, set_substitutions))| {
            let bg_color = if i % 2 == 0 { WHITE } else { ALTERNATE_COLOR };
            let mut cells = vec![
                Cell::new(Text::new(player.number.to_string()))
                    .align(Align::Left)
                    .fill(bg_color),
                Cell::new(Text::new(&player.name))
                    .align(Align::Left)
                    .fill(bg_color),
            ];
            for set in 1..=5 {
                let pos = set_player_initial_position(m, set, &player.id, set_substitutions);
                let cell = Cell::new(pos).align(Align::Center).fill(bg_color);
                cells.push(if set == 5 {
                    cell.stroke(GROUP_STROKE)
                } else {
                    cell
                });
            }
            let filter = StatsFilter::new(None, None, Some(player.id));
            cells.extend(stats_cells(aggregated_stats, &filter, bg_color));
            Row::new(cells).render()
        })
        .collect();
    let headers = create_stats_headers(
        labels,
        vec![
            empty_header(1),
            Cell::new(Text::new(escape_text(labels.player)).bold())
                .align(Align::Left)
                .fill(LIGHT_GRAY),
            Cell::new(Text::new("set").bold())
                .align(Align::Center)
                .fill(LIGHT_GRAY)
                .colspan(5),
        ],
        vec![
            empty_header(1),
            empty_header(1),
            sub_header("1", false),
            sub_header("2", false),
            sub_header("3", false),
            sub_header("4", false),
            sub_header("5", true),
        ],
    );
    render_stats_table(
        &["1fr", "10fr", "1fr", "1fr", "1fr", "1fr", "1fr"],
        headers,
        rows,
    )
}

/// The headers of a table whose rows start with the rallies per break point
/// and per side out (see `phases_row`).
fn phases_headers(labels: &Labels, title: &str) -> (String, String) {
    create_stats_headers(
        labels,
        vec![
            Cell::new(Text::new(escape_text(title)).bold())
                .align(Align::Center)
                .fill(LIGHT_GRAY),
            empty_header(2),
        ],
        vec![
            empty_header(1),
            sub_header(labels.breaks_per_point, false),
            sub_header(labels.sideouts_per_point, true),
        ],
    )
}

/// A row with the rallies per break point and per side out, then the
/// statistics.
fn phases_row(
    label: String,
    stats: &Stats,
    rotation: Option<u8>,
    bg_color: &'static str,
) -> String {
    let rate = |phase| {
        fmt_rate(
            stats
                .phases_per_point(&StatsFilter::new(Some(phase), rotation, None))
                .map(|(v, _, _)| v),
        )
    };
    let centered = |text: String| {
        Cell::new(Text::new(text))
            .fill(bg_color)
            .align(Align::Center)
    };
    let mut cells = vec![
        centered(label),
        centered(rate(PhaseEnum::Break)),
        centered(rate(PhaseEnum::SideOut)).stroke(GROUP_STROKE),
    ];
    cells.extend(stats_cells(
        stats,
        &StatsFilter::new(None, rotation, None),
        bg_color,
    ));
    Row::new(cells).render()
}

const PHASES_COLUMNS: [&str; 3] = ["10fr", "4fr", "4fr"];

fn render_rotations_stats_table(aggregated_stats: &Stats) -> String {
    let labels = current_labels();
    let rows: String = (0..6)
        .map(|rotation| {
            let bg_color = if rotation % 2 == 0 {
                WHITE
            } else {
                ALTERNATE_COLOR
            };
            let label = format!("{}{}", labels.setter_prefix, rotation + 1);
            phases_row(label, aggregated_stats, Some(rotation), bg_color)
        })
        .collect();
    render_stats_table(
        &PHASES_COLUMNS,
        phases_headers(labels, labels.rotation),
        rows,
    )
}

fn render_global_stats_table(aggregated_stats: &Stats) -> String {
    let labels = current_labels();
    let row = phases_row(labels.global.to_string(), aggregated_stats, None, WHITE);
    render_stats_table(&PHASES_COLUMNS, phases_headers(labels, ""), row)
}

fn render_sets_stats_table(sets: &[(&SetEntry, Snapshot)]) -> String {
    let labels = current_labels();
    let rows: String = sets
        .iter()
        .map(|(set, snapshot)| {
            let mut cells = vec![
                Cell::new(Text::new(set.set_number.to_string()))
                    .fill(WHITE)
                    .align(Align::Center),
            ];
            cells.extend(stats_cells(&snapshot.stats, &StatsFilter::default(), WHITE));
            Row::new(cells).render()
        })
        .collect();
    let headers = create_stats_headers(
        labels,
        vec![
            Cell::new(Text::new("set").bold())
                .align(Align::Center)
                .fill(LIGHT_GRAY),
        ],
        vec![empty_header(1)],
    );
    render_stats_table(&["10fr"], headers, rows)
}

/// The first rallies of the side outs: all of them, then by reception.
fn render_phase_stats(aggregated_stats: &Stats) -> String {
    let labels = current_labels();
    let reception_types = [
        (None, labels.global),
        (Some(EvalEnum::Perfect), labels.perfect),
        (Some(EvalEnum::Positive), labels.positive),
        (Some(EvalEnum::Exclamative), labels.subpositive),
        (Some(EvalEnum::Negative), labels.negative),
    ];
    let mut rows = vec![];
    for (i, (reception_eval, label)) in reception_types.iter().enumerate() {
        let bg_color = if i % 2 == 0 { WHITE } else { ALTERNATE_COLOR };
        let summary = aggregated_stats.first_rallies(*reception_eval, &StatsFilter::default());
        let texts = match summary {
            Some(s) => [
                match s.unforced_errors {
                    0 => fmt_count(s.errors),
                    unforced => format!("{}({})", s.errors, unforced),
                },
                // as before, no blocked attack is shown as an empty cell
                match s.blocked {
                    0 => String::new(),
                    blocked => blocked.to_string(),
                },
                s.won.to_string(),
                fmt_pct(Some(s.won_percentage())),
                fmt_pct(Some(s.efficiency())),
                s.total.to_string(),
            ],
            None => [
                "-".to_string(),
                String::new(),
                "-".to_string(),
                "-".to_string(),
                "-".to_string(),
                "-".to_string(),
            ],
        };
        let mut cells = vec![
            Cell::new(Text::new(*label))
                .fill(bg_color)
                .align(Align::Left),
        ];
        cells.extend(texts.into_iter().map(|text| {
            Cell::new(Text::new(text))
                .fill(bg_color)
                .align(Align::Center)
        }));
        rows.push(Row::new(cells).render());
    }
    let table_rows = rows.join("\n");
    let err_label = escape_text(labels.err);
    let unf_label = escape_text(labels.unf);
    let eff_label = escape_text(labels.eff_perc);
    let header_row = Row::new(vec![
        Cell::new(Text::new(escape_text(labels.sideout_on_first_rally)).bold())
            .align(Align::Left)
            .fill(LIGHT_GRAY)
            .colspan(7),
        Cell::new(Text::new("")).align(Align::Left).fill(LIGHT_GRAY),
        sub_header(&format!("{}({})", err_label, unf_label), false),
        sub_header(labels.blk, false),
        sub_header(labels.pt, false),
        sub_header(labels.pt_perc, false),
        sub_header(&format!("% {}", eff_label), false),
        sub_header(labels.tot, false),
    ])
    .render();
    format!(
        r#"
block(
    stroke: 1pt,
    table(
        columns: (1fr, 1fr, 1fr, 1fr, 1fr, 1fr, 1fr),
        inset: 3pt,
        stroke: none,
        {header_row}
        {table_rows}
    )
),
"#,
    )
}

/// The freeballs, in all the rallies and by phase: the ones we gave
/// (`FB`), the ones they gave us (`OF`), and how many of these we exploited
/// (see `Snapshot::record_freeball`).
fn render_freeball_stats(aggregated_stats: &Stats) -> String {
    let labels = current_labels();
    let phases = [
        (None, labels.global),
        (
            Some(PhaseEnum::Break),
            PhaseEnum::Break.friendly_name(labels),
        ),
        (
            Some(PhaseEnum::SideOut),
            PhaseEnum::SideOut.friendly_name(labels),
        ),
    ];
    let table_rows = phases
        .iter()
        .enumerate()
        .map(|(i, (phase, label))| {
            let bg_color = if i % 2 == 0 { WHITE } else { ALTERNATE_COLOR };
            let filter = StatsFilter::new(*phase, None, None);
            let exploitation = aggregated_stats.freeball_exploitation(&filter);
            let texts = [
                fmt_count(aggregated_stats.event_count(EventTypeEnum::FB, None, &filter)),
                fmt_count(aggregated_stats.event_count(EventTypeEnum::OF, None, &filter)),
                fmt_count(exploitation.map_or(0, |(_, _, exploited)| exploited)),
                fmt_pct(exploitation.map(|(perc, _, _)| perc)),
            ];
            let mut cells = vec![
                Cell::new(Text::new(escape_text(label)))
                    .fill(bg_color)
                    .align(Align::Left),
            ];
            cells.extend(texts.into_iter().map(|text| {
                Cell::new(Text::new(text))
                    .fill(bg_color)
                    .align(Align::Center)
            }));
            Row::new(cells).render()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let header_row = Row::new(vec![
        Cell::new(Text::new(escape_text(labels.freeballs)).bold())
            .align(Align::Left)
            .fill(LIGHT_GRAY)
            .colspan(5),
        Cell::new(Text::new("")).align(Align::Left).fill(LIGHT_GRAY),
        sub_header(labels.freeballs_given, false),
        sub_header(labels.freeballs_received, false),
        sub_header(labels.freeballs_exploited, false),
        sub_header("%", false),
    ])
    .render();
    format!(
        r#"
block(
    stroke: 1pt,
    table(
        columns: (2fr, 1fr, 1fr, 1fr, 1fr),
        inset: 3pt,
        stroke: none,
        {header_row}
        {table_rows}
    )
),
"#,
    )
}

fn open_with_system_viewer(path: &Path) -> Result<(), AppError> {
    // the empty argument is the window title: without it `start` would treat
    // a quoted path (e.g. containing spaces) as the title
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]).arg(path);
        c
    };

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg(path);
        c
    };

    #[cfg(target_os = "linux")]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(path);
        c
    };

    #[allow(clippy::zombie_processes)]
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| AppError::IO(IOError::from(e)))
}

fn fmt_pct(opt: Option<f64>) -> String {
    opt.map(|v| format!("{:.1}%", v))
        .unwrap_or_else(|| "-".to_string())
}

fn fmt_count(count: u32) -> String {
    match count {
        0 => "-".to_string(),
        count => count.to_string(),
    }
}

fn fmt_rate(value: Option<f64>) -> String {
    value
        .map(|v| format!("{:.2}", v))
        .unwrap_or_else(|| "-".to_string())
}
