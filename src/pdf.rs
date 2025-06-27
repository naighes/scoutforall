use crate::errors::AppError;
use crate::ops::get_match_status;
use crate::ops::{compute_snapshot, get_sets};
use crate::shapes::enums::{ErrorTypeEnum, EvalEnum, EventTypeEnum, PhaseEnum, ZoneEnum};
use crate::shapes::player::PlayerEntry;
use crate::shapes::r#match::MatchEntry;
use crate::shapes::set::SetEntry;
use crate::shapes::snapshot::Snapshot;
use crate::shapes::stats::Stats;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;
use std::{env, fs};
use typst_as_library::TypstWrapperWorld;
use typst_pdf::PdfOptions;
use uuid::Uuid;

pub fn open_match_pdf(m: &MatchEntry) -> Result<(), AppError> {
    let mut content = String::new();
    content.push_str("#import table: cell, header\n");
    let sets = get_sets(m)?;
    let mut aggregated_stats = Stats::new();
    let mut players: HashSet<Uuid> = HashSet::new();
    // print match header
    content.push_str(header(m).as_str());
    for set in sets {
        let (snapshot, _) = compute_snapshot(&set)?;
        aggregated_stats.merge(&snapshot.stats);
        players.extend(
            snapshot
                .current_lineup
                .get_involved_players()
                .iter()
                .cloned(),
        );
        content.push_str(&set_header(&set, &snapshot, m));
        content.push_str(&generic_stats(&snapshot.stats, None));
        content.push_str(&conversion_rate_stats(&snapshot.stats));
        content.push_str(&serve_stats(&snapshot.stats, None));
        content.push_str(&pass_stats(&snapshot.stats, None));
        content.push_str(&dig_stats(&snapshot.stats, None));
        content.push_str(&block_stats(&snapshot.stats, None));
        content.push_str(&attack_stats(&snapshot.stats, None));
        content.push_str(&counter_attack_stats(&snapshot.stats, None));
        content.push_str(&distribution_stats(&snapshot.stats));
    }
    content.push_str(&match_stats_header());
    content.push_str(&generic_stats(&aggregated_stats, None));
    content.push_str(&conversion_rate_stats(&aggregated_stats));
    content.push_str(&serve_stats(&aggregated_stats, None));
    content.push_str(&pass_stats(&aggregated_stats, None));
    content.push_str(&dig_stats(&aggregated_stats, None));
    content.push_str(&block_stats(&aggregated_stats, None));
    content.push_str(&attack_stats(&aggregated_stats, None));
    content.push_str(&counter_attack_stats(&aggregated_stats, None));
    content.push_str(&distribution_stats(&aggregated_stats));

    // TODO: add stats for each player
    let player_map: HashMap<Uuid, &PlayerEntry> =
        m.team.players.iter().map(|p| (p.id, p)).collect();
    let involved_players: Vec<PlayerEntry> = players
        .into_iter()
        .filter_map(|id| player_map.get(&id).cloned().cloned())
        .collect();

    for player in &involved_players {
        content.push_str(&player_header(player.clone()));
        content.push_str(&generic_stats(&aggregated_stats, Some(player.id)));
        content.push_str(&serve_stats(&aggregated_stats, Some(player.id)));
        content.push_str(&pass_stats(&aggregated_stats, Some(player.id)));
        content.push_str(&dig_stats(&aggregated_stats, Some(player.id)));
        content.push_str(&block_stats(&aggregated_stats, Some(player.id)));
        content.push_str(&attack_stats(&aggregated_stats, Some(player.id)));
    }

    // tmp file
    let mut path: PathBuf = env::temp_dir();
    let uid = Uuid::new_v4().to_string();
    path.push(format!("{}_{}.pdf", m.id, uid));
    // compile
    let world = TypstWrapperWorld::new("../".to_owned(), content);
    let document = typst::compile(&world)
        .output
        .expect("error compiling typst");
    // save
    let pdf = typst_pdf::pdf(&document, &PdfOptions::default()).expect("Error exporting PDF");
    fs::write(&path, pdf).expect("Error writing PDF");
    // open
    open_with_system_viewer(&path);
    Ok(())
}

fn set_header(set: &SetEntry, snapshot: &Snapshot, m: &MatchEntry) -> String {
    let set_number = format!("{}", set.set_number);
    let score_left = if m.home {
        format!("{}", snapshot.score_us)
    } else {
        format!("{}", snapshot.score_them)
    };
    let score_right = if m.home {
        format!("{}", snapshot.score_them)
    } else {
        format!("{}", snapshot.score_us)
    };
    format!(
        r#"
#pagebreak()

#align(center,
  box(
    width: 100%,
    fill: blue.lighten(80%),
    inset: 10pt,
    stroke: blue,
    text(16pt)[set *{set_number}*]
  )
)

#table(
    columns: (1fr, auto, auto, auto, 1fr),
    inset: 5pt,
    stroke: none,
    [],
    [
        #align(center, box(
          fill: blue,
          inset: 10pt,
          stroke: blue.lighten(80%),
          text(16pt, fill: white)[*{score_left}*]
        ))
    ],
    [
        #align(center)[]
    ],
    [
        #align(center, box(
          fill: blue,
          inset: 10pt,
          stroke: blue.lighten(80%),
          text(16pt, fill: white)[*{score_right}*]
        ))
    ],
    [],
)
"#
    )
}

fn player_header(player: PlayerEntry) -> String {
    let name = player.name;
    let num = format!("{}", player.number);
    format!(
        r#"
#pagebreak()

#align(center, box(
  width: 100%,
  fill: blue,
  inset: 20pt,
  stroke: blue.lighten(80%),
  text(16pt, fill: white)[{name} [*{num}*]]
))
"#
    )
}

fn match_stats_header() -> String {
    format!(
        r#"
#pagebreak()

#align(center,
  box(
    width: 100%,
    fill: blue.lighten(80%),
    inset: 10pt,
    stroke: blue,
    text(16pt)[*match stats*]
  )
)
"#
    )
}

fn header(m: &MatchEntry) -> String {
    let match_status = get_match_status(m).expect("could not get match status");
    let date_str = m.date.format("%a %b %d, %Y").to_string();
    let (left_team_name, right_team_name, left_score, right_score) = if m.home {
        (
            &m.team.name,
            &m.opponent,
            format!("{}", match_status.us_wins),
            format!("{}", match_status.them_wins),
        )
    } else {
        (
            &m.opponent,
            &m.team.name,
            format!("{}", match_status.them_wins),
            format!("{}", match_status.us_wins),
        )
    };
    format!(
        r#"
#align(center, text(12pt)[
    {date_str}
])

#table(
    columns: (1fr, auto, auto, auto, 1fr),
    inset: 5pt,
    stroke: none,
    [
      #align(right, box(
        fill: blue,
        inset: 20pt,
        stroke: blue.lighten(80%),
        text(16pt, fill: white)[{left_team_name}]
      ))
    ],
    [
        #align(right, box(
          fill: blue.lighten(80%),
          inset: 18pt,
          stroke: blue,
          text(20pt)[*{left_score}*]
        ))
    ],
    [
        #align(center)[]
    ],
    [
        #align(left, box(
          fill: blue.lighten(80%),
          inset: 18pt,
          stroke: blue,
          text(20pt)[*{right_score}*]
        ))
    ],
    [
      #align(left, box(
        fill: blue,
        inset: 20pt,
        stroke: blue.lighten(80%),
        text(16pt, fill: white)[{right_team_name}]
      ))
    ],
)
"#
    )
}

fn open_with_system_viewer(file: &PathBuf) {
    let path = file.as_path();

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(&["/C", "start", path.to_str().unwrap()])
            .spawn()
            .expect("failed to open PDF");
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(path)
            .spawn()
            .expect("failed to open PDF");
    }

    #[cfg(target_os = "linux")]
    {
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

fn fmt_u32(opt: Option<u32>) -> String {
    opt.map(|v| format!("{}", v))
        .unwrap_or_else(|| "-".to_string())
}

fn fmt_usize(opt: Option<usize>) -> String {
    opt.map(|v| format!("{}", v))
        .unwrap_or_else(|| "-".to_string())
}

struct BasicRow {
    eff: String,
    pos: String,
    perfect: String,
    positive: String,
    exclamative: String,
    negative: String,
    over: String,
    error: String,
}

fn format_basic_row(label: &str, row: &BasicRow) -> String {
    let values = [
        &row.eff,
        &row.pos,
        &row.perfect,
        &row.positive,
        &row.negative,
        &row.over,
        &row.error,
        &row.exclamative,
    ];
    let mut parts = vec![format!("[#align(right)[{label}]]")];
    parts.extend(values.iter().map(|v| format!("[#align(right)[{v}]]")));
    parts.join(", ")
}

fn pass_row(stats: &Stats, rotation: Option<u8>, player: Option<Uuid>) -> BasicRow {
    BasicRow {
        eff: fmt_pct(stats.pass_efficiency_percentage(player, None, rotation)),
        pos: fmt_pct(stats.positive_pass_percentage(player, None, rotation)),
        perfect: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::P,
            player,
            Some(PhaseEnum::SideOut),
            rotation,
            None,
            EvalEnum::Perfect,
        )),
        positive: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::P,
            player,
            Some(PhaseEnum::SideOut),
            rotation,
            None,
            EvalEnum::Positive,
        )),
        exclamative: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::P,
            player,
            Some(PhaseEnum::SideOut),
            rotation,
            None,
            EvalEnum::Exclamative,
        )),
        negative: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::P,
            player,
            Some(PhaseEnum::SideOut),
            rotation,
            None,
            EvalEnum::Negative,
        )),
        over: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::P,
            player,
            Some(PhaseEnum::SideOut),
            rotation,
            None,
            EvalEnum::Over,
        )),
        error: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::P,
            player,
            Some(PhaseEnum::SideOut),
            rotation,
            None,
            EvalEnum::Error,
        )),
    }
}

fn pass_count_row(stats: &Stats, rotation: Option<u8>, player: Option<Uuid>) -> BasicRow {
    BasicRow {
        eff: "".to_string(),
        pos: "".to_string(),
        perfect: fmt_u32(stats.count_events(
            EventTypeEnum::P,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Perfect),
        )),
        positive: fmt_u32(stats.count_events(
            EventTypeEnum::P,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Positive),
        )),
        exclamative: fmt_u32(stats.count_events(
            EventTypeEnum::P,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Exclamative),
        )),
        negative: fmt_u32(stats.count_events(
            EventTypeEnum::P,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Negative),
        )),
        over: fmt_u32(stats.count_events(
            EventTypeEnum::P,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Over),
        )),
        error: fmt_u32(stats.count_events(
            EventTypeEnum::P,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Error),
        )),
    }
}

fn pass_stats(stats: &Stats, player: Option<Uuid>) -> String {
    let global = pass_row(stats, None, player);
    let tot = pass_count_row(stats, None, player);
    let s1 = pass_row(stats, Some(0), player);
    let s2 = pass_row(stats, Some(1), player);
    let s3 = pass_row(stats, Some(2), player);
    let s4 = pass_row(stats, Some(3), player);
    let s5 = pass_row(stats, Some(4), player);
    let s6 = pass_row(stats, Some(5), player);

    let global_row = format_basic_row("gbl", &global);
    let tot_row = format_basic_row("tot.", &tot);
    let s1_row = format_basic_row("S1", &s1);
    let s2_row = format_basic_row("S2", &s2);
    let s3_row = format_basic_row("S3", &s3);
    let s4_row = format_basic_row("S4", &s4);
    let s5_row = format_basic_row("S5", &s5);
    let s6_row = format_basic_row("S6", &s6);

    format!(
        r#"
#align(center, [
#set text(size: 9pt)
#table(
  stroke: none,
  gutter: 0.2em,
  fill: (x, y) => if calc.odd(y) and x != 0 {{ rgb("EAF2F5") }} else if y == 0 or x == 0 {{ rgb("cccccc") }},
  inset: (right: 1.5em),
  align: center + horizon,
  columns: (50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt),
  // header
  [#align(right)[*pass*]], [eff.], [pos.], [\#], [\+], [\-], [/], [\=], [\!],
  // rows
  {global_row},
  {s1_row},
  {s2_row},
  {s3_row},
  {s4_row},
  {s5_row},
  {s6_row},
  cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [\#],
  ), cell(
    fill: rgb("cccccc"),
    [\+],
  ), cell(
    fill: rgb("cccccc"),
    [\-],
  ), cell(
    fill: rgb("cccccc"),
    [\/],
  ), cell(
    fill: rgb("cccccc"),
    [\=],
  ), cell(
    fill: rgb("cccccc"),
    [\!],
  ),
  {tot_row},
)])"#
    )
}

fn dig_row(stats: &Stats, rotation: Option<u8>, player: Option<Uuid>) -> BasicRow {
    BasicRow {
        eff: fmt_pct(stats.dig_efficiency_percentage(player, None, rotation)),
        pos: fmt_pct(stats.positive_dig_percentage(player, None, rotation)),
        perfect: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::D,
            player,
            None,
            rotation,
            None,
            EvalEnum::Perfect,
        )),
        positive: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::D,
            player,
            None,
            rotation,
            None,
            EvalEnum::Positive,
        )),
        exclamative: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::D,
            player,
            None,
            rotation,
            None,
            EvalEnum::Exclamative,
        )),
        negative: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::D,
            player,
            None,
            rotation,
            None,
            EvalEnum::Negative,
        )),
        over: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::D,
            player,
            None,
            rotation,
            None,
            EvalEnum::Over,
        )),
        error: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::D,
            player,
            None,
            rotation,
            None,
            EvalEnum::Error,
        )),
    }
}

fn dig_count_row(stats: &Stats, rotation: Option<u8>, player: Option<Uuid>) -> BasicRow {
    BasicRow {
        eff: "".to_string(),
        pos: "".to_string(),
        perfect: fmt_u32(stats.count_events(
            EventTypeEnum::D,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Perfect),
        )),
        positive: fmt_u32(stats.count_events(
            EventTypeEnum::D,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Positive),
        )),
        exclamative: fmt_u32(stats.count_events(
            EventTypeEnum::P,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Exclamative),
        )),
        negative: fmt_u32(stats.count_events(
            EventTypeEnum::D,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Negative),
        )),
        over: fmt_u32(stats.count_events(
            EventTypeEnum::D,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Over),
        )),
        error: fmt_u32(stats.count_events(
            EventTypeEnum::D,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Error),
        )),
    }
}

fn dig_stats(stats: &Stats, player: Option<Uuid>) -> String {
    let global = dig_row(stats, None, player);
    let tot = dig_count_row(stats, None, player);
    let s1 = dig_row(stats, Some(0), player);
    let s2 = dig_row(stats, Some(1), player);
    let s3 = dig_row(stats, Some(2), player);
    let s4 = dig_row(stats, Some(3), player);
    let s5 = dig_row(stats, Some(4), player);
    let s6 = dig_row(stats, Some(5), player);

    let global_row = format_basic_row("gbl", &global);
    let tot_row = format_basic_row("tot.", &tot);
    let s1_row = format_basic_row("S1", &s1);
    let s2_row = format_basic_row("S2", &s2);
    let s3_row = format_basic_row("S3", &s3);
    let s4_row = format_basic_row("S4", &s4);
    let s5_row = format_basic_row("S5", &s5);
    let s6_row = format_basic_row("S6", &s6);

    format!(
        r#"
#align(center, [
#set text(size: 9pt)
#table(
  stroke: none,
  gutter: 0.2em,
  fill: (x, y) => if calc.odd(y) and x != 0 {{ rgb("EAF2F5") }} else if y == 0 or x == 0 {{ rgb("cccccc") }},
  inset: (right: 1.5em),
  align: center + horizon,
  columns: (50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt),
  // header
  [#align(right)[*dig*]], [eff.], [pos.], [\#], [\+], [\-], [/], [\=], [\!],
  // rows
  {global_row},
  {s1_row},
  {s2_row},
  {s3_row},
  {s4_row},
  {s5_row},
  {s6_row},
  cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [\#],
  ), cell(
    fill: rgb("cccccc"),
    [\+],
  ), cell(
    fill: rgb("cccccc"),
    [\-],
  ), cell(
    fill: rgb("cccccc"),
    [\/],
  ), cell(
    fill: rgb("cccccc"),
    [\=],
  ), cell(
    fill: rgb("cccccc"),
    [\!],
  ),
  {tot_row},
)])"#
    )
}

fn serve_row(stats: &Stats, rotation: Option<u8>, player: Option<Uuid>) -> BasicRow {
    BasicRow {
        eff: fmt_pct(stats.serve_efficiency_percentage(player, None, rotation)),
        pos: fmt_pct(stats.positive_serve_percentage(player, None, rotation)),
        perfect: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::S,
            player,
            Some(PhaseEnum::Break),
            rotation,
            None,
            EvalEnum::Perfect,
        )),
        positive: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::S,
            player,
            Some(PhaseEnum::Break),
            rotation,
            None,
            EvalEnum::Positive,
        )),
        exclamative: "".to_string(),
        negative: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::S,
            player,
            Some(PhaseEnum::Break),
            rotation,
            None,
            EvalEnum::Negative,
        )),
        over: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::S,
            player,
            Some(PhaseEnum::Break),
            rotation,
            None,
            EvalEnum::Over,
        )),
        error: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::S,
            player,
            Some(PhaseEnum::Break),
            rotation,
            None,
            EvalEnum::Error,
        )),
    }
}

fn serve_count_row(stats: &Stats, rotation: Option<u8>, player: Option<Uuid>) -> BasicRow {
    BasicRow {
        eff: "".to_string(),
        pos: "".to_string(),
        perfect: fmt_u32(stats.count_events(
            EventTypeEnum::S,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Perfect),
        )),
        positive: fmt_u32(stats.count_events(
            EventTypeEnum::S,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Positive),
        )),
        exclamative: "".to_string(),
        negative: fmt_u32(stats.count_events(
            EventTypeEnum::S,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Negative),
        )),
        over: fmt_u32(stats.count_events(
            EventTypeEnum::S,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Over),
        )),
        error: fmt_u32(stats.count_events(
            EventTypeEnum::S,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Error),
        )),
    }
}

fn serve_stats(stats: &Stats, player: Option<Uuid>) -> String {
    let global = serve_row(stats, None, player);
    let tot = serve_count_row(stats, None, player);
    let s1 = serve_row(stats, Some(0), player);
    let s2 = serve_row(stats, Some(1), player);
    let s3 = serve_row(stats, Some(2), player);
    let s4 = serve_row(stats, Some(3), player);
    let s5 = serve_row(stats, Some(4), player);
    let s6 = serve_row(stats, Some(5), player);

    let global_row = format_basic_row("gbl", &global);
    let tot_row = format_basic_row("tot.", &tot);
    let s1_row = format_basic_row("S1", &s1);
    let s2_row = format_basic_row("S2", &s2);
    let s3_row = format_basic_row("S3", &s3);
    let s4_row = format_basic_row("S4", &s4);
    let s5_row = format_basic_row("S5", &s5);
    let s6_row = format_basic_row("S6", &s6);

    format!(
        r#"
#align(center, [
#set text(size: 9pt)
#table(
  stroke: none,
  gutter: 0.2em,
  fill: (x, y) => if calc.odd(y) and x != 0 {{ rgb("EAF2F5") }} else if y == 0 or x == 0 {{ rgb("cccccc") }},
  inset: (right: 1.5em),
  align: center + horizon,
  columns: (50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt),
  // header
  [#align(right)[*serve*]], [eff.], [pos.], [\#], [\+], [\-], [/], [\=], [],
  // rows
  {global_row},
  {s1_row},
  {s2_row},
  {s3_row},
  {s4_row},
  {s5_row},
  {s6_row},
  cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [\#],
  ), cell(
    fill: rgb("cccccc"),
    [\+],
  ), cell(
    fill: rgb("cccccc"),
    [\-],
  ), cell(
    fill: rgb("cccccc"),
    [\/],
  ), cell(
    fill: rgb("cccccc"),
    [\=],
  ), cell(
    fill: rgb("cccccc"),
    [],
  ),
  {tot_row},
)])"#
    )
}

fn block_row(stats: &Stats, rotation: Option<u8>, player: Option<Uuid>) -> BasicRow {
    BasicRow {
        eff: fmt_pct(stats.block_efficiency_percentage(player, None, rotation)),
        pos: fmt_pct(stats.positive_block_percentage(player, None, rotation)),
        perfect: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::B,
            player,
            None,
            rotation,
            None,
            EvalEnum::Perfect,
        )),
        positive: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::B,
            player,
            None,
            rotation,
            None,
            EvalEnum::Positive,
        )),
        exclamative: "".to_string(),
        negative: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::B,
            player,
            None,
            rotation,
            None,
            EvalEnum::Negative,
        )),
        over: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::B,
            player,
            None,
            rotation,
            None,
            EvalEnum::Over,
        )),
        error: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::B,
            player,
            None,
            rotation,
            None,
            EvalEnum::Error,
        )),
    }
}

fn block_count_row(stats: &Stats, rotation: Option<u8>, player: Option<Uuid>) -> BasicRow {
    BasicRow {
        eff: "".to_string(),
        pos: "".to_string(),
        perfect: fmt_u32(stats.count_events(
            EventTypeEnum::B,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Perfect),
        )),
        positive: fmt_u32(stats.count_events(
            EventTypeEnum::B,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Positive),
        )),
        exclamative: "".to_string(),
        negative: fmt_u32(stats.count_events(
            EventTypeEnum::B,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Negative),
        )),
        over: fmt_u32(stats.count_events(
            EventTypeEnum::B,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Over),
        )),
        error: fmt_u32(stats.count_events(
            EventTypeEnum::B,
            player,
            None,
            rotation,
            None,
            Some(EvalEnum::Error),
        )),
    }
}

fn block_stats(stats: &Stats, player: Option<Uuid>) -> String {
    let global = block_row(stats, None, player);
    let tot = block_count_row(stats, None, player);
    let s1 = block_row(stats, Some(0), player);
    let s2 = block_row(stats, Some(1), player);
    let s3 = block_row(stats, Some(2), player);
    let s4 = block_row(stats, Some(3), player);
    let s5 = block_row(stats, Some(4), player);
    let s6 = block_row(stats, Some(5), player);

    let global_row = format_basic_row("gbl", &global);
    let tot_row = format_basic_row("tot.", &tot);
    let s1_row = format_basic_row("S1", &s1);
    let s2_row = format_basic_row("S2", &s2);
    let s3_row = format_basic_row("S3", &s3);
    let s4_row = format_basic_row("S4", &s4);
    let s5_row = format_basic_row("S5", &s5);
    let s6_row = format_basic_row("S6", &s6);

    format!(
        r#"
#align(center, [
#set text(size: 9pt)
#table(
  stroke: none,
  gutter: 0.2em,
  fill: (x, y) => if calc.odd(y) and x != 0 {{ rgb("EAF2F5") }} else if y == 0 or x == 0 {{ rgb("cccccc") }},
  inset: (right: 1.5em),
  align: center + horizon,
  columns: (50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt),
  // header
  [#align(right)[*block*]], [eff.], [pos.], [\#], [\+], [\-], [/], [\=], [],
  // rows
  {global_row},
  {s1_row},
  {s2_row},
  {s3_row},
  {s4_row},
  {s5_row},
  {s6_row},
  cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [\#],
  ), cell(
    fill: rgb("cccccc"),
    [\+],
  ), cell(
    fill: rgb("cccccc"),
    [\-],
  ), cell(
    fill: rgb("cccccc"),
    [\/],
  ), cell(
    fill: rgb("cccccc"),
    [\=],
  ), cell(
    fill: rgb("cccccc"),
    [],
  ),
  {tot_row},
)])"#
    )
}

struct AttackRow {
    eff: String,
    eff_perfect: String,
    eff_positive: String,
    eff_exclamative: String,
    eff_negative: String,
    over: String,
    error: String,
}

fn format_attack_row(label: &str, row: &AttackRow) -> String {
    let values = [
        &row.eff,
        &row.error,
        &row.eff_perfect,
        &row.eff_positive,
        &row.eff_negative,
        &row.over,
        &row.eff_exclamative,
        "",
    ];
    let mut parts = vec![format!("[#align(right)[{label}]]")];
    parts.extend(values.iter().map(|v| format!("[#align(right)[{v}]]")));
    parts.join(", ")
}

fn attack_row(
    stats: &Stats,
    rotation: Option<u8>,
    phase: Option<PhaseEnum>,
    player: Option<Uuid>,
) -> AttackRow {
    AttackRow {
        eff: fmt_pct(stats.attack_efficiency_percentage(player, phase, rotation, None)),
        eff_perfect: fmt_pct(stats.attack_efficiency(
            player,
            phase,
            rotation,
            None,
            EvalEnum::Perfect,
        )),
        eff_positive: fmt_pct(stats.attack_efficiency(
            player,
            phase,
            rotation,
            None,
            EvalEnum::Positive,
        )),
        eff_negative: fmt_pct(stats.attack_efficiency(
            player,
            phase,
            rotation,
            None,
            EvalEnum::Negative,
        )),
        over: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::A,
            player,
            phase,
            rotation,
            None,
            EvalEnum::Over,
        )),
        error: fmt_pct(stats.event_type_percentage(
            EventTypeEnum::A,
            player,
            phase,
            rotation,
            None,
            EvalEnum::Error,
        )),
        eff_exclamative: fmt_pct(stats.attack_efficiency(
            player,
            phase,
            rotation,
            None,
            EvalEnum::Exclamative,
        )),
    }
}

fn attack_count_row(
    stats: &Stats,
    rotation: Option<u8>,
    phase: Option<PhaseEnum>,
    player: Option<Uuid>,
) -> AttackRow {
    AttackRow {
        eff: "".to_string(),
        eff_perfect: fmt_u32(stats.count_events(
            EventTypeEnum::A,
            player,
            phase,
            rotation,
            None,
            Some(EvalEnum::Perfect),
        )),
        eff_positive: fmt_u32(stats.count_events(
            EventTypeEnum::A,
            player,
            phase,
            rotation,
            None,
            Some(EvalEnum::Positive),
        )),
        eff_exclamative: "".to_string(),
        eff_negative: fmt_u32(stats.count_events(
            EventTypeEnum::A,
            player,
            phase,
            rotation,
            None,
            Some(EvalEnum::Negative),
        )),
        over: fmt_u32(stats.count_events(
            EventTypeEnum::A,
            player,
            phase,
            rotation,
            None,
            Some(EvalEnum::Over),
        )),
        error: fmt_u32(stats.count_events(
            EventTypeEnum::A,
            player,
            phase,
            rotation,
            None,
            Some(EvalEnum::Error),
        )),
    }
}

fn attack_stats(stats: &Stats, player: Option<Uuid>) -> String {
    let global = attack_row(stats, None, None, player);
    let brk = attack_row(stats, None, Some(PhaseEnum::Break), player);
    let so = attack_row(stats, None, Some(PhaseEnum::SideOut), player);
    let tot = attack_count_row(stats, None, Some(PhaseEnum::SideOut), player);
    let s1 = attack_row(stats, Some(0), None, player);
    let s2 = attack_row(stats, Some(1), None, player);
    let s3 = attack_row(stats, Some(2), None, player);
    let s4 = attack_row(stats, Some(3), None, player);
    let s5 = attack_row(stats, Some(4), None, player);
    let s6 = attack_row(stats, Some(5), None, player);

    let global_row = format_attack_row("gbl", &global);
    let brk_row = format_attack_row("brk", &brk);
    let so_row = format_attack_row("so", &so);
    let tot_row = format_attack_row("tot.", &tot);
    let s1_row = format_attack_row("S1", &s1);
    let s2_row = format_attack_row("S2", &s2);
    let s3_row = format_attack_row("S3", &s3);
    let s4_row = format_attack_row("S4", &s4);
    let s5_row = format_attack_row("S5", &s5);
    let s6_row = format_attack_row("S6", &s6);

    format!(
        r#"
#align(center, [
#set text(size: 9pt)
#table(
  stroke: none,
  gutter: 0.2em,
  fill: (x, y) => if calc.odd(y) and x != 0 {{ rgb("EAF2F5") }} else if y == 0 or x == 0 {{ rgb("cccccc") }},
  inset: (right: 1.5em),
  align: center + horizon,
  columns: (50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt),
  // header
  [#align(right)[*att*]], [eff.], [err], [eff. \#], [eff. \+], [eff. \-], [blk], [eff. \!], [],
  // rows
  {global_row},
  {brk_row},
  {so_row},
  {s1_row},
  {s2_row},
  {s3_row},
  {s4_row},
  {s5_row},
  {s6_row},
  cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [\=],
  ), cell(
    fill: rgb("cccccc"),
    [\#],
  ), cell(
    fill: rgb("cccccc"),
    [\+],
  ), cell(
    fill: rgb("cccccc"),
    [\-],
  ), cell(
    fill: rgb("cccccc"),
    [\/],
  ), cell(
    fill: rgb("cccccc"),
    [],
  ), cell(
    fill: rgb("cccccc"),
    [],
  ),
  {tot_row},
)])"#
    )
}

fn generic_stats(stats: &Stats, player: Option<Uuid>) -> String {
    let ace_gbl = fmt_u32(stats.count_events(
        EventTypeEnum::S,
        player,
        None,
        None,
        None,
        Some(EvalEnum::Perfect),
    ));
    let ace_brk = fmt_u32(stats.count_events(
        EventTypeEnum::S,
        player,
        Some(PhaseEnum::Break),
        None,
        None,
        Some(EvalEnum::Perfect),
    ));

    let flt_gbl = fmt_u32(stats.count_events(EventTypeEnum::F, player, None, None, None, None));
    let flt_brk = fmt_u32(stats.count_events(
        EventTypeEnum::F,
        player,
        Some(PhaseEnum::Break),
        None,
        None,
        None,
    ));
    let flt_so = fmt_u32(stats.count_events(
        EventTypeEnum::F,
        player,
        Some(PhaseEnum::SideOut),
        None,
        None,
        None,
    ));

    let err_gbl_count = match stats.errors.query(None, None, player, None).count() {
        0 => None,
        x => Some(x),
    };
    let err_gbl = fmt_usize(err_gbl_count);
    let err_unf_gbl = fmt_usize(
        match stats
            .errors
            .query(None, None, player, Some(ErrorTypeEnum::Unforced))
            .count()
        {
            0 => None,
            x => Some(x),
        },
    );
    let err_gbl_str = match err_gbl_count {
        None => "-".to_string(),
        Some(_) => format!("{} ({})", err_gbl, err_unf_gbl),
    };
    let err_brk_count = match stats
        .errors
        .query(Some(PhaseEnum::Break), None, player, None)
        .count()
    {
        0 => None,
        x => Some(x),
    };
    let err_brk = fmt_usize(err_brk_count);
    let err_unf_brk = fmt_usize(
        match stats
            .errors
            .query(
                Some(PhaseEnum::Break),
                None,
                player,
                Some(ErrorTypeEnum::Unforced),
            )
            .count()
        {
            0 => None,
            x => Some(x),
        },
    );
    let err_brk_str = match err_brk_count {
        None => "-".to_string(),
        Some(_) => format!("{} ({})", err_brk, err_unf_brk),
    };
    let err_so_count = match stats
        .errors
        .query(Some(PhaseEnum::SideOut), None, player, None)
        .count()
    {
        0 => None,
        x => Some(x),
    };
    let err_so = fmt_usize(err_so_count);
    let err_unf_so = fmt_usize(
        match stats
            .errors
            .query(
                Some(PhaseEnum::SideOut),
                None,
                player,
                Some(ErrorTypeEnum::Unforced),
            )
            .count()
        {
            0 => None,
            x => Some(x),
        },
    );
    let err_so_str = match err_so_count {
        None => "-".to_string(),
        Some(_) => format!("{} ({})", err_so, err_unf_so),
    };

    let opp_err_gbl_count = match stats.opponent_errors.query(None, None).count() {
        0 => None,
        x => Some(x),
    };
    let opp_err_gbl = fmt_usize(opp_err_gbl_count);
    let opp_err_gbl_str = match opp_err_gbl_count {
        None => "-".to_string(),
        Some(_) => format!("{}", opp_err_gbl),
    };

    let opp_err_brk_count = match stats
        .opponent_errors
        .query(Some(PhaseEnum::Break), None)
        .count()
    {
        0 => None,
        x => Some(x),
    };
    let opp_err_brk = fmt_usize(opp_err_brk_count);
    let opp_err_brk_str = match opp_err_brk_count {
        None => "-".to_string(),
        Some(_) => format!("{}", opp_err_brk),
    };

    let opp_err_so_count = match stats
        .opponent_errors
        .query(Some(PhaseEnum::SideOut), None)
        .count()
    {
        0 => None,
        x => Some(x),
    };
    let opp_err_so = fmt_usize(opp_err_so_count);
    let opp_err_so_str = match opp_err_so_count {
        None => "-".to_string(),
        Some(_) => format!("{}", opp_err_so),
    };

    match player {
        Some(_) => format!(
            r#"
#align(center, [
#set text(size: 9pt)
#table(
  stroke: none,
  gutter: 0.2em,
  fill: (x, y) => if calc.odd(y) and x != 0 {{ rgb("EAF2F5") }} else if y == 0 or x == 0 {{ rgb("cccccc") }},
  inset: (right: 1.5em),
  columns: (50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt),
  [], [ace], [flt], [err. (unf.)], [], [], [], [], [],

  [#align(right)[gbl]], [{ace_gbl}], [{flt_gbl}], [{err_gbl_str}], [], [], [], [], [],
  
  [#align(right)[brk]], [{ace_brk}], [{flt_brk}], [{err_brk_str}], [], [], [], [], [],

  [#align(right)[so]], [], [{flt_so}], [{err_so_str}], [], [], [], [], [],
)])"#
        ),
        None => format!(
            r#"
#align(center, [
#set text(size: 9pt)
#table(
  stroke: none,
  gutter: 0.2em,
  fill: (x, y) => if calc.odd(y) and x != 0 {{ rgb("EAF2F5") }} else if y == 0 or x == 0 {{ rgb("cccccc") }},
  inset: (right: 1.5em),
  columns: (50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt, 50pt),
  [], [ace], [flt], [err. (unf.)], [opp. err], [], [], [], [],

  [#align(right)[gbl]], [{ace_gbl}], [{flt_gbl}], [{err_gbl_str}], [{opp_err_gbl_str}], [], [], [], [],
  
  [#align(right)[brk]], [{ace_brk}], [{flt_brk}], [{err_brk_str}], [{opp_err_brk_str}], [], [], [], [],

  [#align(right)[so]], [], [{flt_so}], [{err_so_str}], [{opp_err_so_str}], [], [], [], [],
)])"#
        ),
    }
}

fn conversion_rate_stats(stats: &Stats) -> String {
    fn fmt_rate(stats: &Stats, phase: Option<PhaseEnum>, rotation: Option<u8>) -> String {
        stats
            .number_of_possessions_per_point(phase, rotation)
            .map(|v| format!("{:.2}", v))
            .unwrap_or_else(|| "-".to_string())
    }
    let phases = [
        ("gbl", None),
        ("brk", Some(PhaseEnum::Break)),
        ("so", Some(PhaseEnum::SideOut)),
    ];
    let rotations: Vec<u8> = (0..=5).collect();

    // costruzione righe
    let mut rows = String::new();
    for (label, phase) in phases {
        let mut row = format!("[#align(right)[{}]]", label);
        for &rot in &rotations {
            row.push_str(&format!(", [{}]", fmt_rate(stats, phase, Some(rot))));
        }
        row.push_str(&format!(", [{}]", fmt_rate(stats, phase, None)));
        rows.push_str(&format!("  {},\n", row));
    }
    format!(
        r#"
#align(center, [
#set text(size: 9pt)
#table(
  stroke: none,
  gutter: 0.2em,
  fill: (x, y) => if calc.odd(y) and x != 0 {{ rgb("EAF2F5") }} else if y == 0 or x == 0 {{ rgb("cccccc") }},
  inset: (right: 1.5em),
  columns: (56pt, 56pt, 56pt, 56pt, 56pt, 56pt, 56pt, 56pt),
  [#align(right)[*cnv. rt.*]], [S1], [S2], [S3], [S4], [S5], [S6], [gbl],

{rows}
)])"#
    )
}

fn distribution_stats(stats: &Stats) -> String {
    let zones = [
        ZoneEnum::Two,
        ZoneEnum::Three,
        ZoneEnum::Four,
        ZoneEnum::Eight,
        ZoneEnum::Nine,
    ];
    fn build_for_phase(
        stats: &Stats,
        zones: &[ZoneEnum],
        phase: Option<PhaseEnum>,
        prev_eval: Option<EvalEnum>,
        label: &str,
    ) -> String {
        let pairs: Vec<(String, String)> = zones
            .iter()
            .map(|z| {
                let (d, ds) =
                    stats
                        .distribution
                        .zone_stats(z.clone(), phase, None, prev_eval.clone());
                (fmt_pct(Some(d)), fmt_pct(Some(ds)))
            })
            .collect();
        let pairs_arr: [(String, String); 5] =
            pairs.try_into().expect("expected exactly 5 zone pairs");
        distribution_cell_template(pairs_arr, label.to_string())
    }
    let t_gbl = build_for_phase(stats, &zones, None, None, "gbl. dist.");
    let t_brk = build_for_phase(stats, &zones, Some(PhaseEnum::Break), None, "brk. dist.");
    let t_so = build_for_phase(stats, &zones, Some(PhaseEnum::SideOut), None, "so. dist.");

    let t_gbl_perfect = build_for_phase(
        stats,
        &zones,
        None,
        Some(EvalEnum::Perfect),
        "gbl. dist. (#)",
    );
    let t_brk_perfect = build_for_phase(
        stats,
        &zones,
        Some(PhaseEnum::Break),
        Some(EvalEnum::Perfect),
        "brk. dist. (#)",
    );
    let t_so_perfect = build_for_phase(
        stats,
        &zones,
        Some(PhaseEnum::SideOut),
        Some(EvalEnum::Perfect),
        "so. dist. (#)",
    );

    let t_gbl_pos = build_for_phase(
        stats,
        &zones,
        None,
        Some(EvalEnum::Positive),
        "gbl. dist. (+)",
    );
    let t_brk_pos = build_for_phase(
        stats,
        &zones,
        Some(PhaseEnum::Break),
        Some(EvalEnum::Positive),
        "brk. dist. (+)",
    );
    let t_so_pos = build_for_phase(
        stats,
        &zones,
        Some(PhaseEnum::SideOut),
        Some(EvalEnum::Positive),
        "so. dist. (+)",
    );

    let t_gbl_neg = build_for_phase(
        stats,
        &zones,
        None,
        Some(EvalEnum::Negative),
        "gbl. dist. (-)",
    );
    let t_brk_neg = build_for_phase(
        stats,
        &zones,
        Some(PhaseEnum::Break),
        Some(EvalEnum::Negative),
        "brk. dist. (-)",
    );
    let t_so_neg = build_for_phase(
        stats,
        &zones,
        Some(PhaseEnum::SideOut),
        Some(EvalEnum::Negative),
        "so. dist. (-)",
    );

    let t_gbl_exc = build_for_phase(
        stats,
        &zones,
        None,
        Some(EvalEnum::Exclamative),
        "gbl. dist. (!)",
    );
    let t_brk_exc = build_for_phase(
        stats,
        &zones,
        Some(PhaseEnum::Break),
        Some(EvalEnum::Exclamative),
        "brk. dist. (!)",
    );
    let t_so_exc = build_for_phase(
        stats,
        &zones,
        Some(PhaseEnum::SideOut),
        Some(EvalEnum::Exclamative),
        "so. dist. (!)",
    );

    format!(
        r#"
#align(center, text("DISTRIBUTION", size: 14pt, weight: "bold"))

#line(length: 100%)
#let dst-perc-s(body) = text(
  size: 9pt,
  fill: olive,
  weight: "bold",
  body
)

#let dst-perc(body) = text(
  size: 12pt,
  body
)

#let dst-perc-rect(body) = rect(
  height: 36pt,
  width: 36pt
)

#let dst-perc-rect(..args, body) = rect(
  height: 36pt,
  width: 36pt,
  ..args,
  body,
)

#grid(
  columns: (auto, auto, auto),
  gutter: 58pt,
  rows: 4,
  {t_gbl}
  {t_brk}
  {t_so}
  {t_gbl_perfect}
  {t_brk_perfect}
  {t_so_perfect}
  {t_gbl_pos}
  {t_brk_pos}
  {t_so_pos}
  {t_gbl_neg}
  {t_brk_neg}
  {t_so_neg}
  {t_gbl_exc}
  {t_brk_exc}
  {t_so_exc}
)
#text("")
"#
    )
}
fn counter_attack_stats(stats: &Stats, player: Option<Uuid>) -> String {
    let zones = [
        ZoneEnum::Two,
        ZoneEnum::Three,
        ZoneEnum::Four,
        ZoneEnum::Eight,
        ZoneEnum::Nine,
    ];
    fn build_for_phase(
        stats: &Stats,
        player: Option<Uuid>,
        zones: &[ZoneEnum],
        rotation: Option<u8>,
        label: &str,
    ) -> String {
        let pairs: Vec<String> = zones
            .iter()
            .map(|z| {
                let d = stats.counter_attack_conversion_rate(
                    player,
                    Some(PhaseEnum::Break),
                    rotation,
                    Some(z.clone()),
                );
                match d {
                    Some(v) => format!("{:.1}%", v),
                    None => "-".to_string(),
                }
            })
            .collect();
        let arr: [String; 5] = pairs.try_into().expect("expected exactly 5 zone pairs");
        counter_attack_cell_template(arr, label.to_string())
    }
    let t_gbl = build_for_phase(stats, player, &zones, None, "brk");
    let rotations: Vec<u8> = (0..=5).collect();
    let mut rows = String::new();
    for &rot in &rotations {
        rows.push_str(&build_for_phase(
            stats,
            player,
            &zones,
            Some(rot),
            format!("brk (S{})", rot + 1).as_ref(),
        ));
    }

    format!(
        r#"
#align(center, text("COUNTER ATTACK", size: 14pt, weight: "bold"))

#line(length: 100%)
#let dst-perc-s(body) = text(
  size: 9pt,
  fill: olive,
  weight: "bold",
  body
)

#let dst-perc(body) = text(
  size: 12pt,
  body
)

#let dst-perc-rect(body) = rect(
  height: 36pt,
  width: 36pt
)

#let dst-perc-rect(..args, body) = rect(
  height: 36pt,
  width: 36pt,
  ..args,
  body,
)

#grid(
  columns: (auto, auto, auto),
  gutter: 58pt,
  rect(width: 108pt, height: 108pt, inset: 0%, stroke: none)[],
  {t_gbl}
  rect(width: 108pt, height: 108pt, inset: 0%, stroke: none)[],
  {rows}
)
#text("")
"#
    )
}

fn distribution_cell_template(values: [(String, String); 5], label: String) -> String {
    let [(d_2, s_2), (d_3, s_3), (d_4, s_4), (d_8, s_8), (d_9, s_9)] = values;
    format!(
        r#"
rect(width: 108pt, height: 108pt, inset: 0%, stroke: none)[
  #align(center, text(weight: "bold", "{label}"))
  #grid(
    columns: 3,
    rows: 3,
    stroke: none,
    dst-perc-rect(
      stroke: (left: 1pt, top: 1pt, bottom: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc("{d_4}"),
      dst-perc-s("{s_4}"),
    )]],
    dst-perc-rect(
      stroke: (top: 1pt, bottom: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc("{d_3}"),
      dst-perc-s("{s_3}"),
    )]],
    dst-perc-rect(
      stroke: (top: 1pt, right: 1pt, bottom: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc("{d_2}"),
      dst-perc-s("{s_2}"),
    )]],

    dst-perc-rect(
      stroke: (left: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc(""),
      dst-perc-s(""),
    )]],
    dst-perc-rect(
      stroke: none,
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc("{d_8}"),
      dst-perc-s("{s_8}"),
    )]],
    dst-perc-rect(
      stroke: (right: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc("{d_9}"),
      dst-perc-s("{s_9}"),
    )]],

    dst-perc-rect(
      stroke: (left: 1pt, bottom: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc(""),
      dst-perc-s(""),
    )]],
    dst-perc-rect(
      stroke: (bottom: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc(""),
      dst-perc-s(""),
    )]],
    dst-perc-rect(
      stroke: (right: 1pt, bottom: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc(""),
      dst-perc-s(""),
    )]],
  )
],"#
    )
}

fn counter_attack_cell_template(values: [String; 5], label: String) -> String {
    let [d_2, d_3, d_4, d_8, d_9] = values;
    format!(
        r#"
rect(width: 108pt, height: 108pt, inset: 0%, stroke: none)[
  #align(center, text(weight: "bold", "{label}"))
  #grid(
    columns: 3,
    rows: 3,
    stroke: none,
    dst-perc-rect(
      stroke: (left: 1pt, top: 1pt, bottom: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc("{d_4}"),
    )]],
    dst-perc-rect(
      stroke: (top: 1pt, bottom: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc("{d_3}"),
    )]],
    dst-perc-rect(
      stroke: (top: 1pt, right: 1pt, bottom: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc("{d_2}"),
    )]],

    dst-perc-rect(
      stroke: (left: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc(""),
      dst-perc-s(""),
    )]],
    dst-perc-rect(
      stroke: none,
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc("{d_8}"),
    )]],
    dst-perc-rect(
      stroke: (right: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc("{d_9}"),
    )]],

    dst-perc-rect(
      stroke: (left: 1pt, bottom: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc(""),
      dst-perc-s(""),
    )]],
    dst-perc-rect(
      stroke: (bottom: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc(""),
      dst-perc-s(""),
    )]],
    dst-perc-rect(
      stroke: (right: 1pt, bottom: 1pt),
    )[#align(center)[#stack(
      dir: ttb,
      spacing: 6pt,
      dst-perc(""),
      dst-perc-s(""),
    )]],
  )
],
"#
    )
}

// TODO: sideout (in percentuale) => totale punti | totale punti al primo scambio | totale fasi perse
// TODO: break => tirare fuori delle statistiche sullo scambio prolungato
