use crate::errors::AppError;
use crate::localization::{current_labels, Labels};
use crate::reporting::align::Align;
use crate::reporting::circle::Circle;
use crate::reporting::court::render_court;
use crate::reporting::stack::{Stack, StackDirection};
use crate::reporting::table_row::{Cell, Row};
use crate::reporting::text::Text;
use crate::reporting::typst_content::TypstContent;
use crate::reporting::util::escape_text;
use crate::shapes::enums::{ErrorTypeEnum, EvalEnum, EventTypeEnum, PhaseEnum, TeamSideEnum};
use crate::shapes::player::PlayerEntry;
use crate::shapes::r#match::{MatchEntry, MatchStatus};
use crate::shapes::set::SetEntry;
use crate::shapes::snapshot::Snapshot;
use crate::shapes::stats::{Metric, Stats};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};
use typst_as_library::TypstWrapperWorld;
use typst_pdf::PdfOptions;
use uuid::Uuid;

pub const DEFAULT_FONT_SIZE: u8 = 8;
pub const WHITE: &str = "#ffffff";
pub const BLACK: &str = "#000000";
pub const LIGHT_GRAY: &str = "#cccccc";
pub const GRAY: &str = "#666666";
pub const ALTERNATE_COLOR: &str = "#efefef";

pub fn open_match_pdf(m: &MatchEntry) -> Result<(), AppError> {
    let mut content = String::new();
    let created_with = current_labels().created_with;
    let match_status = m.get_status()?;
    let mut aggregated_stats = Stats::new();
    let mut players: HashMap<Uuid, HashMap<u8, bool>> = HashMap::new();
    let sets: Vec<(&SetEntry, Snapshot)> = m
        .sets
        .iter()
        .filter_map(|set| {
            set.compute_snapshot().ok().map(|(snapshot, _)| {
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
    let mut path: PathBuf = env::temp_dir();
    let uid = Uuid::new_v4().to_string();
    path.push(format!("{}_{}.pdf", m.id, uid));
    let world = TypstWrapperWorld::new("../".to_owned(), content);
    let document = typst::compile(&world)
        .output
        .expect("error compiling typst");
    let pdf = typst_pdf::pdf(&document, &PdfOptions::default()).expect("error exporting PDF");
    fs::write(&path, pdf).expect("error writing PDF");
    open_with_system_viewer(&path);
    Ok(())
}

fn render_bottom_stats(aggregated_stats: &Stats) -> String {
    let sideout_stats = render_phase_stats(aggregated_stats);
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
  {sideout_stats}
)
"#
    )
}

fn render_distribution(aggregated_stats: &Stats, phase: Option<PhaseEnum>) -> String {
    render_court(current_labels().distribution, phase, 6, |court, zone| {
        if let Some((top, bottom)) = aggregated_stats
            .distribution
            .zone_stats(zone, phase, None, None, None)
        {
            court.set_zone(zone, Some(fmt_pct(Some(top))), Some(fmt_pct(Some(bottom))));
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
                aggregated_stats.counter_attack_conversion_rate(None, None, None, Some(zone))
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

fn render_match_overview(m: &MatchEntry, sets: &Vec<(&SetEntry, Snapshot)>) -> String {
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

struct StatsRow {
    total_points: String,
    break_points: String,
    won_minus_lost: String,
    serve_total: String,
    serve_errors: String,
    serve_points: String,
    reception_total: String,
    reception_errors: String,
    reception_positiveness: String,
    reception_efficiency: String,
    reception_perfect: String,
    attack_total: String,
    attack_errors: String,
    attack_blocked: String,
    attack_points: String,
    attack_perfect: String,
    attack_efficiency: String,
    blocks_total: String,
    faults_total: String,
}

impl StatsRow {
    fn calculate(aggregated_stats: &Stats, player_id: Option<Uuid>, rotation: Option<u8>) -> Self {
        let event_count = |event_type, eval| {
            aggregated_stats
                .event_count(event_type, player_id, None, rotation, None, eval)
                .map_or("-".to_string(), |v| v.to_string())
        };
        let total_points = aggregated_stats.total_scored_points(player_id, None, rotation, None);
        let total_points_str = total_points.map_or("-".to_string(), |v| v.to_string());
        let break_points = aggregated_stats
            .total_scored_points(player_id, Some(PhaseEnum::Break), rotation, None)
            .map_or("-".to_string(), |v| v.to_string());
        let won_minus_lost = match (
            total_points,
            aggregated_stats.total_errors(None, player_id, None, rotation, None),
        ) {
            (Some(pts), Some(errs)) => (pts as i32 - errs as i32).to_string(),
            _ => "-".to_string(),
        };
        StatsRow {
            total_points: total_points_str,
            break_points,
            won_minus_lost,
            serve_total: event_count(EventTypeEnum::S, None),
            serve_errors: event_count(EventTypeEnum::S, Some(EvalEnum::Error)),
            serve_points: event_count(EventTypeEnum::S, Some(EvalEnum::Perfect)),
            reception_total: event_count(EventTypeEnum::P, None),
            reception_errors: event_count(EventTypeEnum::P, Some(EvalEnum::Error)),
            reception_positiveness: fmt_pct(
                aggregated_stats
                    .event_positiveness(
                        EventTypeEnum::P,
                        player_id,
                        None,
                        rotation,
                        None,
                        Metric::Positive,
                    )
                    .map(|(v, _, _)| v),
            ),
            reception_efficiency: fmt_pct(
                aggregated_stats
                    .event_positiveness(
                        EventTypeEnum::P,
                        player_id,
                        None,
                        rotation,
                        None,
                        Metric::Efficiency,
                    )
                    .map(|(v, _, _)| v),
            ),
            reception_perfect: fmt_pct(
                aggregated_stats
                    .event_percentage(
                        EventTypeEnum::P,
                        player_id,
                        None,
                        rotation,
                        None,
                        EvalEnum::Perfect,
                    )
                    .map(|(v, _, _)| v),
            ),
            attack_total: event_count(EventTypeEnum::A, None),
            attack_errors: event_count(EventTypeEnum::A, Some(EvalEnum::Error)),
            attack_blocked: event_count(EventTypeEnum::A, Some(EvalEnum::Over)),
            attack_points: event_count(EventTypeEnum::A, Some(EvalEnum::Perfect)),
            attack_perfect: fmt_pct(
                aggregated_stats
                    .event_percentage(
                        EventTypeEnum::A,
                        player_id,
                        None,
                        rotation,
                        None,
                        EvalEnum::Perfect,
                    )
                    .map(|(v, _, _)| v),
            ),
            attack_efficiency: fmt_pct(
                aggregated_stats
                    .event_positiveness(
                        EventTypeEnum::A,
                        player_id,
                        None,
                        rotation,
                        None,
                        Metric::Efficiency,
                    )
                    .map(|(v, _, _)| v),
            ),
            blocks_total: event_count(EventTypeEnum::B, Some(EvalEnum::Perfect)),
            faults_total: event_count(EventTypeEnum::F, None),
        }
    }

    fn to_cells(&self, bg_color: &'static str, stroke_positions: &[usize]) -> Vec<Cell> {
        let data = [
            &self.total_points,
            &self.break_points,
            &self.won_minus_lost,
            &self.serve_total,
            &self.serve_errors,
            &self.serve_points,
            &self.reception_total,
            &self.reception_errors,
            &self.reception_positiveness,
            &self.reception_perfect,
            &self.reception_efficiency,
            &self.attack_total,
            &self.attack_errors,
            &self.attack_blocked,
            &self.attack_points,
            &self.attack_perfect,
            &self.attack_efficiency,
            &self.blocks_total,
            &self.faults_total,
        ];
        data.iter()
            .enumerate()
            .map(|(i, text)| {
                let mut cell = Cell::new(Text::new(*text))
                    .align(Align::Center)
                    .fill(bg_color);
                if stroke_positions.contains(&i) {
                    cell = cell.stroke("(right: (thickness: 1pt, dash: \"dashed\"))");
                }
                cell
            })
            .collect()
    }
}

fn create_stats_headers(labels: &Labels, include_rotation_headers: bool) -> (String, String) {
    let header_cell = |text: &str, colspan: u8| {
        Cell::new(Text::new(escape_text(text)).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .colspan(colspan)
    };
    let sub_header = |text: &str, stroke: bool, bold: bool| {
        let text_content = if bold {
            Text::new(escape_text(text)).bold()
        } else {
            Text::new(text)
        };
        let mut cell = Cell::new(text_content)
            .align(Align::Center)
            .fill(LIGHT_GRAY);
        if stroke {
            cell = cell.stroke("(right: (thickness: 1pt, dash: \"dashed\"))");
        }
        cell
    };
    let mut header_cells = vec![];
    let mut sub_header_cells = vec![];
    if include_rotation_headers {
        header_cells.push(
            Cell::new(Text::new(escape_text(labels.rotation)).bold())
                .align(Align::Center)
                .fill(LIGHT_GRAY),
        );
        header_cells.push(Cell::new(Text::new("")).fill(LIGHT_GRAY).colspan(2));

        sub_header_cells.push(Cell::new(Text::new("")).fill(LIGHT_GRAY));
        sub_header_cells.push(sub_header(labels.breaks_per_point, false, true));
        sub_header_cells.push(sub_header(labels.sideouts_per_point, true, true));
    } else {
        header_cells.push(Cell::new(Text::new("")).fill(LIGHT_GRAY));
        header_cells.push(
            Cell::new(Text::new(escape_text(labels.player)).bold())
                .align(Align::Left)
                .fill(LIGHT_GRAY),
        );
        header_cells.push(header_cell("set", 5));
        sub_header_cells.extend([
            Cell::new(Text::new("")).fill(LIGHT_GRAY),
            Cell::new(Text::new("")).fill(LIGHT_GRAY),
            sub_header("1", false, true),
            sub_header("2", false, true),
            sub_header("3", false, true),
            sub_header("4", false, true),
            sub_header("5", true, true),
        ]);
    }
    header_cells.extend([
        header_cell(labels.points, 3),
        header_cell(labels.serve, 3),
        header_cell(labels.reception, 5),
        header_cell(labels.attack, 6),
        header_cell(labels.blk, 1),
        header_cell(labels.flt, 1),
    ]);
    sub_header_cells.extend([
        sub_header(labels.tot, false, true),
        sub_header("bp", false, false),
        sub_header(labels.won_lost, true, true),
        sub_header(labels.tot, false, true),
        sub_header(labels.err, false, true),
        sub_header(labels.pt, true, true),
        sub_header(labels.tot, false, true),
        sub_header(labels.err, false, true),
        sub_header(labels.pos_perc, false, true),
        sub_header(labels.prf_perc, false, true),
        sub_header(labels.eff_perc, true, true),
        sub_header(labels.tot, false, true),
        sub_header(labels.err, false, true),
        sub_header(labels.blk, false, true),
        sub_header(labels.pt, false, true),
        sub_header(labels.pt_perc, false, true),
        sub_header(labels.eff_perc, true, true),
        sub_header(labels.pt, false, true),
        sub_header(labels.tot, false, true),
    ]);
    (
        Row::new(header_cells).render(),
        Row::new(sub_header_cells).render(),
    )
}

fn render_players_stats_table(
    m: &MatchEntry,
    players: &[(&PlayerEntry, HashMap<u8, bool>)],
    aggregated_stats: &Stats,
) -> String {
    let labels = current_labels();
    let stroke_positions = [2, 5, 10, 16, 18];
    let rows: String = players
        .iter()
        .enumerate()
        .map(|(i, (player, set_substitutions))| {
            let bg_color = if i % 2 == 0 { WHITE } else { ALTERNATE_COLOR };
            let player_id = player.id;
            let positions: Vec<_> = (1..=5)
                .map(|set| set_player_initial_position(m, set, &player_id, set_substitutions))
                .collect();
            let stats = StatsRow::calculate(aggregated_stats, Some(player_id), None);
            let mut cells = vec![
                Cell::new(Text::new(player.number.to_string()))
                    .align(Align::Left)
                    .fill(bg_color),
                Cell::new(Text::new(&player.name))
                    .align(Align::Left)
                    .fill(bg_color),
            ];
            for (idx, pos) in positions.into_iter().enumerate() {
                let mut cell = Cell::new(pos).align(Align::Center).fill(bg_color);
                if idx == 4 {
                    cell = cell.stroke("(right: (thickness: 1pt, dash: \"dashed\"))");
                }
                cells.push(cell);
            }
            cells.extend(stats.to_cells(bg_color, &stroke_positions));
            Row::new(cells).render()
        })
        .collect();
    let (header_row, sub_header_row) = create_stats_headers(labels, false);
    format!(
        r#"
#block(
  stroke: 1pt,
  table(
    columns: (
      1fr,
      10fr,
      1fr,
      1fr,
      1fr,
      1fr,
      1fr,
      2fr,
      1fr,
      3fr,
      2fr,
      2fr,
      1fr,
      2fr,
      2fr,
      3fr,
      2fr,
      3fr,
      1fr,
      2fr,
      2fr,
      2fr,
      3fr,
      2fr,
      1fr,
      2fr,
    ),
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

fn render_rotations_stats_table(aggregated_stats: &Stats) -> String {
    let labels = current_labels();
    let stroke_positions = [2, 5, 10, 16, 18];
    let rows: String = (0..6)
        .map(|rotation| {
            let rotation_name = format!("{}{}", labels.setter_prefix, rotation + 1);
            let bg_color = if rotation % 2 == 0 {
                WHITE
            } else {
                ALTERNATE_COLOR
            };
            let stats = StatsRow::calculate(aggregated_stats, None, Some(rotation));
            let breaks_per_point = fmt_rate(
                aggregated_stats
                    .number_of_phases_per_scored_point(Some(PhaseEnum::Break), Some(rotation))
                    .map(|(v, _, _)| v),
            );
            let sideouts_per_point = fmt_rate(
                aggregated_stats
                    .number_of_phases_per_scored_point(Some(PhaseEnum::SideOut), Some(rotation))
                    .map(|(v, _, _)| v),
            );
            let centered = |text: String, stroke: bool| {
                let mut cell = Cell::new(Text::new(text))
                    .fill(bg_color)
                    .align(Align::Center);
                if stroke {
                    cell = cell.stroke("(right: (thickness: 1pt, dash: \"dashed\"))");
                }
                cell
            };
            let mut cells = vec![
                centered(rotation_name, false),
                centered(breaks_per_point, false),
                centered(sideouts_per_point, true),
            ];
            cells.extend(stats.to_cells(bg_color, &stroke_positions));
            Row::new(cells).render()
        })
        .collect();
    let (header_row, sub_header_row) = create_stats_headers(labels, true);
    format!(
        r#"
#block(
  stroke: 1pt,
  table(
    columns: (
      10fr,
      4fr,
      4fr,
      2fr,
      1fr,
      3fr,
      2fr,
      2fr,
      1fr,
      2fr,
      2fr,
      3fr,
      2fr,
      3fr,
      1fr,
      2fr,
      2fr,
      2fr,
      3fr,
      2fr,
      1fr,
      2fr,
    ),
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

fn render_global_stats_table(aggregated_stats: &Stats) -> String {
    let points_label = escape_text(current_labels().points);
    let serve_label = escape_text(current_labels().serve);
    let reception_label = escape_text(current_labels().reception);
    let attack_label = escape_text(current_labels().attack);
    let pt_label = escape_text(current_labels().pt);
    let tot_label = escape_text(current_labels().tot);
    let won_lost_label = escape_text(current_labels().won_lost);
    let err_label = escape_text(current_labels().err);
    let pos_perc_label = escape_text(current_labels().pos_perc);
    let prf_perc_label = escape_text(current_labels().prf_perc);
    let eff_perc_label = escape_text(current_labels().eff_perc);
    let pt_perc_label = escape_text(current_labels().pt_perc);
    let blk_label = escape_text(current_labels().blk);
    let flt_label = escape_text(current_labels().flt);
    let breaks_per_point_label = escape_text(current_labels().breaks_per_point);
    let sideouts_per_point_label = escape_text(current_labels().sideouts_per_point);
    let bg_color = WHITE;
    let total_point_opt = aggregated_stats.total_scored_points(None, None, None, None);
    let total_points = match total_point_opt {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let total_break_points =
        match aggregated_stats.total_scored_points(None, Some(PhaseEnum::Break), None, None) {
            Some(v) => v.to_string(),
            None => "-".to_string(),
        };
    let total_errors_opt = aggregated_stats.total_errors(None, None, None, None, None);
    let won_minus_lost = match total_errors_opt {
        Some(total_errors) => match total_point_opt {
            Some(total_points) => (total_points as i32 - total_errors as i32).to_string(),
            None => "-".to_string(),
        },
        None => "-".to_string(),
    };
    let serve_total =
        match aggregated_stats.event_count(EventTypeEnum::S, None, None, None, None, None) {
            Some(v) => v.to_string(),
            None => "-".to_string(),
        };
    let serve_errors = match aggregated_stats.event_count(
        EventTypeEnum::S,
        None,
        None,
        None,
        None,
        Some(EvalEnum::Error),
    ) {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let serve_points = match aggregated_stats.event_count(
        EventTypeEnum::S,
        None,
        None,
        None,
        None,
        Some(EvalEnum::Perfect),
    ) {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let reception_total =
        match aggregated_stats.event_count(EventTypeEnum::P, None, None, None, None, None) {
            Some(v) => v.to_string(),
            None => "-".to_string(),
        };
    let reception_errors = match aggregated_stats.event_count(
        EventTypeEnum::P,
        None,
        None,
        None,
        None,
        Some(EvalEnum::Error),
    ) {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let reception_positiveness = fmt_pct(
        aggregated_stats
            .event_positiveness(EventTypeEnum::P, None, None, None, None, Metric::Positive)
            .map(|(v, _, _)| v),
    );
    let reception_efficiency = fmt_pct(
        aggregated_stats
            .event_positiveness(EventTypeEnum::P, None, None, None, None, Metric::Efficiency)
            .map(|(v, _, _)| v),
    );
    let reception_perfect = fmt_pct(
        aggregated_stats
            .event_percentage(EventTypeEnum::P, None, None, None, None, EvalEnum::Perfect)
            .map(|(v, _, _)| v),
    );
    let attack_total =
        match aggregated_stats.event_count(EventTypeEnum::A, None, None, None, None, None) {
            Some(v) => v.to_string(),
            None => "-".to_string(),
        };
    let attack_errors = match aggregated_stats.event_count(
        EventTypeEnum::A,
        None,
        None,
        None,
        None,
        Some(EvalEnum::Error),
    ) {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let attack_blocked = match aggregated_stats.event_count(
        EventTypeEnum::A,
        None,
        None,
        None,
        None,
        Some(EvalEnum::Over),
    ) {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let attack_points = match aggregated_stats.event_count(
        EventTypeEnum::A,
        None,
        None,
        None,
        None,
        Some(EvalEnum::Perfect),
    ) {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let attack_perfect = fmt_pct(
        aggregated_stats
            .event_percentage(EventTypeEnum::A, None, None, None, None, EvalEnum::Perfect)
            .map(|(v, _, _)| v),
    );
    let attack_efficiency = fmt_pct(
        aggregated_stats
            .event_positiveness(EventTypeEnum::A, None, None, None, None, Metric::Efficiency)
            .map(|(v, _, _)| v),
    );
    let blocks_total = match aggregated_stats.event_count(
        EventTypeEnum::B,
        None,
        None,
        None,
        None,
        Some(EvalEnum::Perfect),
    ) {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let faults_total =
        match aggregated_stats.event_count(EventTypeEnum::F, None, None, None, None, None) {
            Some(v) => v.to_string(),
            None => "-".to_string(),
        };
    let number_of_breaks_per_scored_point = fmt_rate(
        aggregated_stats
            .number_of_phases_per_scored_point(Some(PhaseEnum::Break), None)
            .map(|(v, _, _)| v),
    );
    let number_of_sideouts_per_scored_point = fmt_rate(
        aggregated_stats
            .number_of_phases_per_scored_point(Some(PhaseEnum::SideOut), None)
            .map(|(v, _, _)| v),
    );
    let row = Row::new(vec![
        Cell::new(Text::new(current_labels().global))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(number_of_breaks_per_scored_point))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(number_of_sideouts_per_scored_point))
            .fill(bg_color)
            .align(Align::Center)
            .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
        Cell::new(Text::new(total_points))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(total_break_points))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(won_minus_lost))
            .fill(bg_color)
            .align(Align::Center)
            .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
        Cell::new(Text::new(serve_total))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(serve_errors))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(serve_points))
            .fill(bg_color)
            .align(Align::Center)
            .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
        Cell::new(Text::new(reception_total))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(reception_errors))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(reception_positiveness))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(reception_perfect))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(reception_efficiency))
            .fill(bg_color)
            .align(Align::Center)
            .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
        Cell::new(Text::new(attack_total))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(attack_errors))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(attack_blocked))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(attack_points))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(attack_perfect))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(attack_efficiency))
            .fill(bg_color)
            .align(Align::Center)
            .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
        Cell::new(Text::new(blocks_total))
            .fill(bg_color)
            .align(Align::Center),
        Cell::new(Text::new(faults_total))
            .fill(bg_color)
            .align(Align::Center),
    ])
    .render();
    let header_row = Row::new(vec![
        Cell::new(Text::new(""))
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new("")).fill(LIGHT_GRAY).colspan(2),
        Cell::new(Text::new(&points_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .colspan(3),
        Cell::new(Text::new(&serve_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .colspan(3),
        Cell::new(Text::new(&reception_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .colspan(5),
        Cell::new(Text::new(&attack_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .colspan(6),
        Cell::new(Text::new(&blk_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&flt_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
    ])
    .render();
    let sub_header_row = Row::new(vec![
        Cell::new(Text::new("")).fill(LIGHT_GRAY),
        Cell::new(Text::new(&breaks_per_point_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&sideouts_per_point_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
        Cell::new(Text::new(&tot_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new("bp"))
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&won_lost_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
        Cell::new(Text::new(&tot_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&err_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&pt_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
        Cell::new(Text::new(&tot_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&err_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&pos_perc_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&prf_perc_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&eff_perc_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
        Cell::new(Text::new(&tot_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&err_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&blk_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&pt_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&pt_perc_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&eff_perc_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
        Cell::new(Text::new(&pt_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&tot_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
    ])
    .render();

    format!(
        r#"
#block(
  stroke: 1pt,
  table(
    columns: (
      10fr,
      4fr,
      4fr,
      2fr,
      1fr,
      3fr,
      2fr,
      2fr,
      1fr,
      2fr,
      2fr,
      3fr,
      2fr,
      3fr,
      1fr,
      2fr,
      2fr,
      2fr,
      3fr,
      2fr,
      1fr,
      2fr,
    ),
    inset:3pt,
    stroke: none,
    
    // group headers
    {header_row}

    // sub-headers
    {sub_header_row}

    {row}
  )
)
"#
    )
}

fn render_sets_stats_table(sets: &Vec<(&SetEntry, Snapshot)>) -> String {
    let points_label = escape_text(current_labels().points);
    let serve_label = escape_text(current_labels().serve);
    let reception_label = escape_text(current_labels().reception);
    let attack_label = escape_text(current_labels().attack);
    let pt_label = escape_text(current_labels().pt);
    let tot_label = escape_text(current_labels().tot);
    let won_lost_label = escape_text(current_labels().won_lost);
    let err_label = escape_text(current_labels().err);
    let pos_perc_label = escape_text(current_labels().pos_perc);
    let prf_perc_label = escape_text(current_labels().prf_perc);
    let eff_perc_label = escape_text(current_labels().eff_perc);
    let pt_perc_label = escape_text(current_labels().pt_perc);
    let blk_label = escape_text(current_labels().blk);
    let flt_label = escape_text(current_labels().flt);
    let mut rows = String::new();
    for (set, snapshot) in sets {
        let set_number = set.set_number;
        let bg_color = WHITE;
        let total_point_opt = snapshot.stats.total_scored_points(None, None, None, None);
        let total_points = match total_point_opt {
            Some(v) => v.to_string(),
            None => "-".to_string(),
        };
        let total_break_points =
            match snapshot
                .stats
                .total_scored_points(None, Some(PhaseEnum::Break), None, None)
            {
                Some(v) => v.to_string(),
                None => "-".to_string(),
            };
        let total_errors_opt = snapshot.stats.total_errors(None, None, None, None, None);
        let won_minus_lost = match total_errors_opt {
            Some(total_errors) => match total_point_opt {
                Some(total_points) => (total_points as i32 - total_errors as i32).to_string(),
                None => "-".to_string(),
            },
            None => "-".to_string(),
        };
        let serve_total =
            match snapshot
                .stats
                .event_count(EventTypeEnum::S, None, None, None, None, None)
            {
                Some(v) => v.to_string(),
                None => "-".to_string(),
            };
        let serve_errors = match snapshot.stats.event_count(
            EventTypeEnum::S,
            None,
            None,
            None,
            None,
            Some(EvalEnum::Error),
        ) {
            Some(v) => v.to_string(),
            None => "-".to_string(),
        };
        let serve_points = match snapshot.stats.event_count(
            EventTypeEnum::S,
            None,
            None,
            None,
            None,
            Some(EvalEnum::Perfect),
        ) {
            Some(v) => v.to_string(),
            None => "-".to_string(),
        };
        let reception_total =
            match snapshot
                .stats
                .event_count(EventTypeEnum::P, None, None, None, None, None)
            {
                Some(v) => v.to_string(),
                None => "-".to_string(),
            };
        let reception_errors = match snapshot.stats.event_count(
            EventTypeEnum::P,
            None,
            None,
            None,
            None,
            Some(EvalEnum::Error),
        ) {
            Some(v) => v.to_string(),
            None => "-".to_string(),
        };
        let reception_positiveness = fmt_pct(
            snapshot
                .stats
                .event_positiveness(EventTypeEnum::P, None, None, None, None, Metric::Positive)
                .map(|(v, _, _)| v),
        );
        let reception_efficiency = fmt_pct(
            snapshot
                .stats
                .event_positiveness(EventTypeEnum::P, None, None, None, None, Metric::Efficiency)
                .map(|(v, _, _)| v),
        );
        let reception_perfect = fmt_pct(
            snapshot
                .stats
                .event_percentage(EventTypeEnum::P, None, None, None, None, EvalEnum::Perfect)
                .map(|(v, _, _)| v),
        );
        let attack_total =
            match snapshot
                .stats
                .event_count(EventTypeEnum::A, None, None, None, None, None)
            {
                Some(v) => v.to_string(),
                None => "-".to_string(),
            };
        let attack_errors = match snapshot.stats.event_count(
            EventTypeEnum::A,
            None,
            None,
            None,
            None,
            Some(EvalEnum::Error),
        ) {
            Some(v) => v.to_string(),
            None => "-".to_string(),
        };
        let attack_blocked = match snapshot.stats.event_count(
            EventTypeEnum::A,
            None,
            None,
            None,
            None,
            Some(EvalEnum::Over),
        ) {
            Some(v) => v.to_string(),
            None => "-".to_string(),
        };
        let attack_points = match snapshot.stats.event_count(
            EventTypeEnum::A,
            None,
            None,
            None,
            None,
            Some(EvalEnum::Perfect),
        ) {
            Some(v) => v.to_string(),
            None => "-".to_string(),
        };
        let attack_perfect = fmt_pct(
            snapshot
                .stats
                .event_percentage(EventTypeEnum::A, None, None, None, None, EvalEnum::Perfect)
                .map(|(v, _, _)| v),
        );
        let attack_efficiency = fmt_pct(
            snapshot
                .stats
                .event_positiveness(EventTypeEnum::A, None, None, None, None, Metric::Efficiency)
                .map(|(v, _, _)| v),
        );
        let blocks_total = match snapshot.stats.event_count(
            EventTypeEnum::B,
            None,
            None,
            None,
            None,
            Some(EvalEnum::Perfect),
        ) {
            Some(v) => v.to_string(),
            None => "-".to_string(),
        };
        let faults_total =
            match snapshot
                .stats
                .event_count(EventTypeEnum::F, None, None, None, None, None)
            {
                Some(v) => v.to_string(),
                None => "-".to_string(),
            };
        let row = Row::new(vec![
            Cell::new(Text::new(set_number.to_string()))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(total_points))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(total_break_points))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(won_minus_lost))
                .fill(bg_color)
                .align(Align::Center)
                .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
            Cell::new(Text::new(serve_total))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(serve_errors))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(serve_points))
                .fill(bg_color)
                .align(Align::Center)
                .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
            Cell::new(Text::new(reception_total))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(reception_errors))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(reception_positiveness))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(reception_perfect))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(reception_efficiency))
                .fill(bg_color)
                .align(Align::Center)
                .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
            Cell::new(Text::new(attack_total))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(attack_errors))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(attack_blocked))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(attack_points))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(attack_perfect))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(attack_efficiency))
                .fill(bg_color)
                .align(Align::Center)
                .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
            Cell::new(Text::new(blocks_total))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(faults_total))
                .fill(bg_color)
                .align(Align::Center),
        ])
        .render();
        rows.push_str(&row);
    }
    let header_row = Row::new(vec![
        Cell::new(Text::new("set").bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&points_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .colspan(3),
        Cell::new(Text::new(&serve_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .colspan(3),
        Cell::new(Text::new(&reception_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .colspan(5),
        Cell::new(Text::new(&attack_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .colspan(6),
        Cell::new(Text::new(&blk_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&flt_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
    ])
    .render();
    let sub_header_row = Row::new(vec![
        Cell::new(Text::new("")).fill(LIGHT_GRAY),
        Cell::new(Text::new(&tot_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new("bp").bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&won_lost_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
        Cell::new(Text::new(&tot_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&err_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&pt_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
        Cell::new(Text::new(&tot_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&err_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&pos_perc_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&prf_perc_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&eff_perc_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
        Cell::new(Text::new(&tot_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&err_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&blk_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&pt_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&pt_perc_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&eff_perc_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY)
            .stroke("(right: (thickness: 1pt, dash: \"dashed\"))"),
        Cell::new(Text::new(&pt_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&tot_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
    ])
    .render();

    format!(
        r#"
#block(
  stroke: 1pt,
  table(
    columns: (
      10fr,
      2fr,
      1fr,
      3fr,
      2fr,
      2fr,
      1fr,
      2fr,
      2fr,
      3fr,
      2fr,
      3fr,
      1fr,
      2fr,
      2fr,
      2fr,
      3fr,
      2fr,
      1fr,
      2fr,
    ),
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

fn render_phase_stats(aggregated_stats: &Stats) -> String {
    let reception_types = [
        (None, current_labels().global),
        (Some(EvalEnum::Perfect), current_labels().perfect),
        (Some(EvalEnum::Positive), current_labels().positive),
        (Some(EvalEnum::Exclamative), current_labels().subpositive),
        (Some(EvalEnum::Negative), current_labels().negative),
    ];
    let mut rows = vec![];
    for (i, (re, label)) in reception_types.iter().enumerate() {
        let bg_color = if i % 2 == 0 { WHITE } else { ALTERNATE_COLOR };
        let (eff, total) = aggregated_stats
            .sideout_first_rally_positiveness(None, *re, None, Metric::Efficiency)
            .map(|(v, t, _)| (fmt_pct(Some(v)), t.to_string()))
            .unwrap_or(("-".to_string(), "-".to_string()));
        let (scored_perc, scored) = aggregated_stats
            .sideout_first_rally_positiveness(None, *re, None, Metric::Positive)
            .map(|(v, _, c)| (fmt_pct(Some(v)), c.to_string()))
            .unwrap_or(("-".to_string(), "-".to_string()));
        let blocked = aggregated_stats
            .sideout_first_rally_count(None, *re, Some(EventTypeEnum::A), Some(EvalEnum::Over))
            .map(|v| v.to_string())
            .unwrap_or("".to_string());
        let errors_total = aggregated_stats
            .sideout_first_rally_errors(None, *re, None, None, None)
            .map(|v| v.to_string());
        let errors_unforced = aggregated_stats
            .sideout_first_rally_errors(None, *re, None, None, Some(ErrorTypeEnum::Unforced))
            .map(|v| v.to_string());
        let error_str = match (errors_total, errors_unforced) {
            (Some(total), Some(unforced)) => format!("{}({})", total, unforced),
            (Some(total), None) => total,
            _ => "-".to_string(),
        };
        let row = Row::new(vec![
            Cell::new(Text::new(*label))
                .fill(bg_color)
                .align(Align::Left),
            Cell::new(Text::new(error_str.clone()))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(blocked))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(scored))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(scored_perc))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(eff))
                .fill(bg_color)
                .align(Align::Center),
            Cell::new(Text::new(total))
                .fill(bg_color)
                .align(Align::Center),
        ])
        .render();
        rows.push(row);
    }
    let table_rows = rows.join("\n");
    let err_label = escape_text(current_labels().err);
    let unf_label = escape_text(current_labels().unf);
    let blk_label = escape_text(current_labels().blk);
    let pt_label = escape_text(current_labels().pt);
    let pt_perc_label = escape_text(current_labels().pt_perc);
    let eff_label = escape_text(current_labels().eff_perc);
    let tot_label = escape_text(current_labels().tot);
    let sideout_on_first_rally_label = escape_text(current_labels().sideout_on_first_rally);
    let header_row = Row::new(vec![
        Cell::new(Text::new(&sideout_on_first_rally_label).bold())
            .align(Align::Left)
            .fill(LIGHT_GRAY)
            .colspan(7),
        Cell::new(Text::new("")).align(Align::Left).fill(LIGHT_GRAY),
        Cell::new(Text::new(format!("{}({})", err_label, unf_label)).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&blk_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&pt_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&pt_perc_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(format!("% {}", eff_label)).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
        Cell::new(Text::new(&tot_label).bold())
            .align(Align::Center)
            .fill(LIGHT_GRAY),
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

fn open_with_system_viewer(file: &Path) {
    let path = file;

    #[cfg(target_os = "windows")]
    {
        #[allow(clippy::zombie_processes)]
        Command::new("cmd")
            .args(&["/C", "start", path.to_str().unwrap()])
            .spawn()
            .expect("failed to open PDF");
    }

    #[cfg(target_os = "macos")]
    {
        #[allow(clippy::zombie_processes)]
        Command::new("open")
            .arg(path)
            .spawn()
            .expect("failed to open PDF");
    }

    #[cfg(target_os = "linux")]
    {
        #[allow(clippy::zombie_processes)]
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .expect("failed to open PDF");
    }
}

fn fmt_pct(opt: Option<f64>) -> String {
    opt.map(|v| format!("{:.1}%", v))
        .unwrap_or_else(|| "-".to_string())
}

fn fmt_rate(value: Option<f64>) -> String {
    value
        .map(|v| format!("{:.2}", v))
        .unwrap_or_else(|| "-".to_string())
}
