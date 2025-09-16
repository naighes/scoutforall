use crate::errors::AppError;
use crate::localization::current_labels;
use crate::shapes::enums::{
    ErrorTypeEnum, EvalEnum, EventTypeEnum, FriendlyName, PhaseEnum, ZoneEnum,
};
use crate::shapes::player::PlayerEntry;
use crate::shapes::r#match::MatchEntry;
use crate::shapes::set::SetEntry;
use crate::shapes::snapshot::Snapshot;
use crate::shapes::stats::{Metric, Stats};
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fmt, fs};
use typst_as_library::TypstWrapperWorld;
use typst_pdf::PdfOptions;
use uuid::Uuid;

pub const TABLE_BACKGROUND_COLOR: &str = "#cce5ff";
pub const TABLE_HEADER_BACKGROUND_COLOR: &str = "#cccccc";
pub const WHITE: &str = "#ffffff";
pub const LIGHT_GRAY: &str = "#cccccc";
pub const GRAY: &str = "#666666";
pub const SCORE_BG_COLOR: &str = "#0088ff";
pub const EFFICIENCY_ALPHA: &str = "9f";
pub const COURT_ZONE_SIZE: u8 = 36;
pub const COURT_ZONE_SPACING: u8 = 6;
pub const COURT_VALUE_BOTTOM_FONT_SIZE: u8 = 9;
pub const COURT_VALUE_TOP_FONT_SIZE: u8 = 9;
pub const COURT_GUTTER: u8 = 58;
pub const COURT_TITLE_FONT_SIZE: u8 = 10;
pub const DEFAULT_TABLE_PADDING: u8 = 10;

pub fn open_match_pdf(m: &MatchEntry) -> Result<(), AppError> {
    use EvalEnum::*;
    let mut content = String::new();
    let date_str = m.date.format("%a %d %b %Y").to_string();
    content.push_str("#import table: cell, header\n");
    content.push_str(&format!(
        r#"
#import table: cell, header

#set page(
  footer: [
    #line(length: 100%)
    match played on *{date_str}*
  ]
)
"#
    ));
    let sets = m.load_sets()?;
    let mut aggregated_stats = Stats::new();
    let mut players: HashSet<Uuid> = HashSet::new();
    for set in sets {
        let (snapshot, _) = set.compute_snapshot()?;
        aggregated_stats.merge(&snapshot.stats);
        players.extend(
            snapshot
                .current_lineup
                .get_involved_players()
                .iter()
                .cloned(),
        );
        content.push_str(&header(Some(&set), Some(&snapshot), m));
        content.push_str(&resume_stats(
            &snapshot.stats,
            vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
            None,
        ));
        content.push_str(&possessions_conversion_rate_stats(
            &snapshot.stats,
            vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
        ));
        content.push_str(&phases_conversion_rate_stats(
            &snapshot.stats,
            vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
        ));
        content.push_str(&event_stats(
            &snapshot.stats,
            EventTypeEnum::S,
            vec![Perfect, Positive, Over, Error, Negative],
            vec![Some(PhaseEnum::Break)],
            None,
        ));
        content.push_str(&event_stats(
            &snapshot.stats,
            EventTypeEnum::P,
            vec![Perfect, Positive, Exclamative, Over, Error, Negative],
            vec![Some(PhaseEnum::SideOut)],
            None,
        ));
        content.push_str(&event_stats(
            &snapshot.stats,
            EventTypeEnum::D,
            vec![Perfect, Positive, Exclamative, Over, Error, Negative],
            vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
            None,
        ));
        content.push_str(&event_stats(
            &snapshot.stats,
            EventTypeEnum::B,
            vec![Perfect, Positive, Over, Error, Negative],
            vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
            None,
        ));
        content.push_str(&attack_stats(
            &snapshot.stats,
            vec![Error, Perfect, Positive, Negative, Over],
            vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
            None,
        ));
    }
    content.push_str(&header(None, None, m));
    content.push_str(&resume_stats(
        &aggregated_stats,
        vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
        None,
    ));
    content.push_str(&possessions_conversion_rate_stats(
        &aggregated_stats,
        vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
    ));
    content.push_str(&phases_conversion_rate_stats(
        &aggregated_stats,
        vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
    ));
    content.push_str(&event_stats(
        &aggregated_stats,
        EventTypeEnum::S,
        vec![Perfect, Positive, Over, Error, Negative],
        vec![Some(PhaseEnum::Break)],
        None,
    ));
    content.push_str(&event_stats(
        &aggregated_stats,
        EventTypeEnum::P,
        vec![Perfect, Positive, Exclamative, Over, Error, Negative],
        vec![Some(PhaseEnum::SideOut)],
        None,
    ));
    content.push_str(&event_stats(
        &aggregated_stats,
        EventTypeEnum::D,
        vec![Perfect, Positive, Exclamative, Over, Error, Negative],
        vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
        None,
    ));
    content.push_str(&event_stats(
        &aggregated_stats,
        EventTypeEnum::B,
        vec![Perfect, Positive, Over, Error, Negative],
        vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
        None,
    ));
    content.push_str(&attack_stats(
        &aggregated_stats,
        vec![Error, Perfect, Positive, Negative, Over],
        vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
        None,
    ));
    content.push_str(&counter_attack_stats(&aggregated_stats, None));
    content.push_str(&distribution_stats(&aggregated_stats));

    let player_map: HashMap<Uuid, &PlayerEntry> =
        m.team.players.iter().map(|p| (p.id, p)).collect();
    let involved_players: Vec<PlayerEntry> = players
        .into_iter()
        .filter_map(|id| player_map.get(&id).cloned().cloned())
        .collect();

    for player in &involved_players {
        content.push_str(&player_header(player.clone()));
        content.push_str(&resume_stats(
            &aggregated_stats,
            vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
            Some(player.id),
        ));
        content.push_str(&event_stats(
            &aggregated_stats,
            EventTypeEnum::S,
            vec![Perfect, Positive, Over, Error, Negative],
            vec![Some(PhaseEnum::Break)],
            Some(player.id),
        ));
        content.push_str(&event_stats(
            &aggregated_stats,
            EventTypeEnum::P,
            vec![Perfect, Positive, Exclamative, Over, Error, Negative],
            vec![Some(PhaseEnum::SideOut)],
            Some(player.id),
        ));
        content.push_str(&event_stats(
            &aggregated_stats,
            EventTypeEnum::D,
            vec![Perfect, Positive, Exclamative, Over, Error, Negative],
            vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
            Some(player.id),
        ));
        content.push_str(&event_stats(
            &aggregated_stats,
            EventTypeEnum::B,
            vec![Perfect, Positive, Over, Error, Negative],
            vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
            Some(player.id),
        ));
        content.push_str(&attack_stats(
            &aggregated_stats,
            vec![Error, Perfect, Positive, Negative, Over],
            vec![None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)],
            Some(player.id),
        ));
    }
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

fn header(set: Option<&SetEntry>, snapshot: Option<&Snapshot>, m: &MatchEntry) -> String {
    let set_number = set
        .map(|s| format!("set {}", s.set_number))
        .unwrap_or("".to_string());
    let score_us = snapshot.map(|s| s.score_us).unwrap_or(0);
    let score_them = snapshot.map(|s| s.score_them).unwrap_or(0);
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
    let (score_left, score_right) = match (m.get_status(), m.home, set) {
        (Err(_), _, _) => (0, 0),
        (Ok(status), true, None) => (status.us_wins, status.them_wins),
        (Ok(status), false, None) => (status.them_wins, status.us_wins),
        (Ok(_), true, Some(_)) => (score_us, score_them),
        (Ok(_), false, Some(_)) => (score_them, score_us),
    };
    let (set_left, right_set) = match (m.get_status(), m.home) {
        (Err(_), _) => (0, 0),
        (Ok(status), true) => (status.us_wins, status.them_wins),
        (Ok(status), false) => (status.them_wins, status.us_wins),
    };
    let first_row = match set {
        None => "".to_string(),
        Some(_) => format!(
            r#"
    table.cell(
      text(""),
      fill: rgb("{WHITE}"),
    ),
    table.cell(
      align: center + horizon,
      text("{set_left}", size: {MATCH_SCORE_FONT_SIZE}pt, fill: rgb("{WHITE}"), weight: "bold"),
      fill: rgb("{GRAY}"),
    ),
    table.cell(
      align: center + horizon,
      text("-", size: {MATCH_SCORE_FONT_SIZE}pt, fill: rgb("{WHITE}"), weight: "bold"),
      fill: rgb("{GRAY}"),
    ),
    table.cell(
      align: center + horizon,
      text("{right_set}", size: {MATCH_SCORE_FONT_SIZE}pt, fill: rgb("{WHITE}"), weight: "bold"),
      fill: rgb("{GRAY}"),
    ),
    table.cell(
      text(""),
      fill: rgb("{WHITE}"),
    ),
"#
        ),
    };
    const MATCH_SCORE_FONT_SIZE: u8 = 30;
    const SCORE_FONT_SIZE: u8 = 20;
    const TEAM_NAME_FONT_SIZE: u8 = 12;
    format!(
        r#"
#table(
    columns: (1fr, 32pt, 40pt, 32pt, 1fr),
    inset: {DEFAULT_TABLE_PADDING}pt,
    stroke: none,
    fill: rgb("{LIGHT_GRAY}"),
    {first_row}
    table.cell(
      align: right + horizon,
      text("{team_left}", size: {TEAM_NAME_FONT_SIZE}pt, weight: "bold"),
      fill: rgb("{LIGHT_GRAY}"),
    ),
    table.cell(
      align: center + horizon,
      text("{score_left}", size: {SCORE_FONT_SIZE}pt, fill: rgb("{WHITE}"), weight: "bold"),
      fill: rgb("{SCORE_BG_COLOR}"),
    ),
    table.cell(
      align: center + horizon,
      text("{set_number}", size: {TEAM_NAME_FONT_SIZE}pt, fill: rgb("{WHITE}"), weight: "bold"),
      fill: rgb("{GRAY}"),
    ),
    table.cell(
      align: center + horizon,
      text("{score_right}", size: {SCORE_FONT_SIZE}pt, fill: rgb("{WHITE}"), weight: "bold"),
      fill: rgb("{SCORE_BG_COLOR}"),
    ),
    table.cell(
      align: left + horizon,
      text("{team_right}", size: {TEAM_NAME_FONT_SIZE}pt, weight: "bold"),
      fill: rgb("{LIGHT_GRAY}"),
    ),
)
"#
    )
}

fn player_header(player: PlayerEntry) -> String {
    let name = player.name;
    let num = format!("{}", player.number);
    let font_size: u8 = 14;
    format!(
        r#"
#align(center, box(
  width: 100%,
  fill: rgb("{GRAY}"),
  inset: {DEFAULT_TABLE_PADDING}pt,
  stroke: none,
  text({font_size}pt, fill: rgb("{WHITE}"))[{name} [*{num}*]]
))
"#
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

fn fmt_u32(opt: Option<u32>) -> String {
    opt.map(|v| format!("{}", v))
        .unwrap_or_else(|| "-".to_string())
}

fn fmt_usize(opt: Option<usize>) -> String {
    opt.map(|v| format!("{}", v))
        .unwrap_or_else(|| "-".to_string())
}

fn fmt_rate(value: Option<f64>) -> String {
    value
        .map(|v| format!("{:.2}", v))
        .unwrap_or_else(|| "-".to_string())
}

fn escape_text(input: &str) -> String {
    input.replace('"', "\\\"")
}

fn color_interpolation(percent: f64, inverse: bool) -> String {
    // normalization
    let p = percent.clamp(0.0, 100.0) / 100.0;
    // color boundaries
    let (start, mid, end) = if inverse {
        // inverted: green -> yellow -> red
        ((0, 255, 0), (255, 255, 0), (255, 0, 0))
    } else {
        // default: red => yellow => green
        ((255, 0, 0), (255, 255, 0), (0, 255, 0))
    };
    let (r, g, b): (u8, u8, u8);
    if p <= 0.5 {
        // start => mid interpolation
        let t = p / 0.5;
        r = ((1.0 - t) * start.0 as f64 + t * mid.0 as f64) as u8;
        g = ((1.0 - t) * start.1 as f64 + t * mid.1 as f64) as u8;
        b = ((1.0 - t) * start.2 as f64 + t * mid.2 as f64) as u8;
    } else {
        // mid => end interpolation
        let t = (p - 0.5) / 0.5;
        r = ((1.0 - t) * mid.0 as f64 + t * end.0 as f64) as u8;
        g = ((1.0 - t) * mid.1 as f64 + t * end.1 as f64) as u8;
        b = ((1.0 - t) * mid.2 as f64 + t * end.2 as f64) as u8;
    }
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

#[derive(Default, Debug, Clone)]
pub enum Alignment {
    #[allow(dead_code)]
    Left,
    #[allow(dead_code)]
    Center,
    #[allow(dead_code)]
    Right,
    #[default]
    Auto,
}

impl Display for Alignment {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let label = match self {
            Alignment::Left => "left",
            Alignment::Center => "center",
            Alignment::Right => "right",
            Alignment::Auto => "auto",
        };
        write!(f, "{}", label)
    }
}

#[derive(Default, Debug, Clone)]
pub enum FontWeight {
    #[default]
    Regular,
    Bold,
}

impl Display for FontWeight {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let label = match self {
            FontWeight::Regular => "regular",
            FontWeight::Bold => "bold",
        };
        write!(f, "{}", label)
    }
}

#[derive(Default, Debug, Clone)]
pub enum FontStyle {
    #[default]
    Normal,
    #[allow(dead_code)]
    Italic,
}

impl Display for FontStyle {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let label = match self {
            FontStyle::Normal => "normal",
            FontStyle::Italic => "italic",
        };
        write!(f, "{}", label)
    }
}

#[derive(Default, Debug, Clone)]
pub struct Cell {
    pub content: String,
    pub background_color: Option<String>,
    pub alignment: Option<Alignment>,
    pub font_weight: Option<FontWeight>,
    pub font_style: Option<FontStyle>,
    pub text_color: Option<String>,
    pub font_size: Option<u8>,
}

impl Cell {
    fn render_text(&self) -> String {
        let content = escape_text(&self.content);
        let font_weight = match &self.font_weight {
            Some(weight) => weight.to_string(),
            None => FontWeight::Regular.to_string(),
        };
        let font_style = match &self.font_style {
            Some(style) => style.to_string(),
            None => FontStyle::Normal.to_string(),
        };
        let text_color = match &self.text_color {
            Some(text_color) => text_color.to_string(),
            None => GRAY.to_string(),
        };
        let font_size = match &self.font_size {
            Some(size) => format!("size: {}pt,", size),
            None => "".to_string(),
        };
        format!(
            r#"
  #text(
    "{content}",
    weight: "{font_weight}",
    style: "{font_style}",
    fill: rgb("{text_color}"),
    {font_size}
  )
"#
        )
    }

    pub fn render(&self) -> String {
        let text = self.render_text();
        let alignment = match &self.alignment {
            Some(alignment) => alignment.to_string(),
            None => Alignment::Auto.to_string(),
        };
        let background_color = match &self.background_color {
            Some(color) => format!(r#"fill: rgb("{}"),"#, color),
            None => "".to_string(),
        };
        format!(
            r#"table.cell(
  align: {alignment},
  {background_color}
)[
  {text}
]"#
        )
    }
}

#[derive(Debug, Clone)]
pub struct Row {
    pub cells: Vec<Cell>,
}

impl Row {
    fn new(cells: Vec<Cell>) -> Self {
        Row { cells }
    }
}

#[derive(Debug, Clone)]
pub struct Table {
    pub headers: Row,
    pub rows: Vec<Row>,
    pub gutter: Option<u8>,
    pub background_color: Option<String>,
    pub stroke_size: Option<u8>,
    pub stroke_color: Option<String>,
    pub padding: Option<u8>,
    pub alignment: Option<Alignment>,
    pub font_size: Option<u8>,
}

impl Table {
    pub fn new(headers: Row) -> Self {
        Self {
            headers,
            rows: vec![],
            gutter: None,
            background_color: None,
            stroke_size: None,
            stroke_color: None,
            padding: None,
            alignment: None,
            font_size: None,
        }
    }

    fn render_header(&self) -> String {
        let header = self
            .headers
            .cells
            .iter()
            .map(|c| c.render())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            r#"
table.header(
  {}
),"#,
            header
        )
    }

    pub fn add_row(mut self, row: Row) -> Self {
        self.rows.push(row);
        self
    }

    pub fn render(&self) -> String {
        let header = self.render_header();
        let mut content = String::new();
        let cells = self
            .rows
            .iter()
            .map(|r| {
                r.cells
                    .iter()
                    .map(|c| c.render())
                    .collect::<Vec<_>>()
                    .join(",\n")
            })
            .collect::<Vec<_>>()
            .join(",\n");
        content.push_str(&header);
        content.push('\n');
        content.push_str(&cells);
        let cols = std::iter::repeat_n("1fr", self.headers.cells.len())
            .collect::<Vec<_>>()
            .join(", ");
        let alignment = match &self.alignment {
            Some(alignment) => alignment.to_string(),
            None => "left".to_string(),
        };
        let font_size = match self.font_size {
            Some(size) => format!("#set text(size: {}pt)", size),
            None => "".to_string(),
        };
        let gutter = match self.gutter {
            Some(gutter) => format!("{}pt", gutter),
            None => "auto".to_string(),
        };
        let background_color = match &self.background_color {
            Some(color) => format!(r#"fill: rgb("{}"),"#, color),
            None => "".to_string(),
        };
        let stroke_color = match &self.stroke_color {
            Some(color) => color.to_string(),
            None => GRAY.to_string(),
        };
        let stroke_size = match &self.stroke_size {
            Some(size) => format!("{}pt", size),
            None => "auto".to_string(),
        };
        let padding = match &self.padding {
            Some(padding) => format!("{}pt", padding),
            None => "0pt".to_string(),
        };
        format!(
            r#"
#align({alignment}, [
{font_size}
#table(
  columns: ({cols}),
  gutter: {gutter},
  {background_color}
  stroke: (paint: rgb("{stroke_color}"), thickness: {stroke_size}, dash: auto),
  inset: {padding},
  {content}
)
])
"#
        )
    }
}

pub struct Courts {
    pub title: String,
    pub title_font_size: u8,
    pub zone_size: u8,
    pub value_bottom_font_size: u8,
    pub value_top_font_size: u8,
    pub gutter: u8,
    pub rows: Vec<CourtRow>,
    pub padding: u8,
}

impl Courts {
    pub fn new(
        title: String,
        title_font_size: u8,
        zone_size: u8,
        value_bottom_font_size: u8,
        value_top_font_size: u8,
        gutter: u8,
        padding: u8,
    ) -> Self {
        Courts {
            title,
            title_font_size,
            rows: vec![],
            zone_size,
            value_bottom_font_size,
            value_top_font_size,
            gutter,
            padding,
        }
    }

    fn add_row(&mut self, row: CourtRow) {
        self.rows.push(row);
    }

    fn render(&self) -> String {
        let title = escape_text(&self.title);
        let title_font_size = self.title_font_size;
        let value_bottom_font_size = self.value_bottom_font_size;
        let value_top_font_size = self.value_top_font_size;
        let zone_size = self.zone_size.to_string();
        let gutter = self.gutter;
        let padding = self.padding;
        let rows = self
            .rows
            .iter()
            .map(|r| r.render())
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            r#"
#let dst-perc-s(body) = text(
  size: {value_bottom_font_size}pt,
  fill: olive,
  weight: "bold",
  body
)
#let dst-perc(body) = text(
  size: {value_top_font_size}pt,
  body
)
#let dst-perc-rect(..args, body) = rect(
  height: {zone_size}pt,
  width: {zone_size}pt,
  fill: rgb("{WHITE}"),
  ..args,
  body,
)
#grid(
  rows: 3,
  columns: 1,
    fill: rgb("{TABLE_BACKGROUND_COLOR}"),
  block(
    align(
      center,
      text("{title}",
      fill: rgb("{WHITE}"),
      weight: "bold",
      size: {title_font_size}pt),
    ),
    width: 100%,
    fill: rgb("{GRAY}"),
    inset: {padding}pt,
  ),
  block(
    width: 100%,
    inset: {padding}pt,
    align(center)[
      #grid(
        columns: (auto, auto, auto),
        gutter: {gutter}pt,
        rows: 4,
        {rows}
      )
    ]
  ),
  block(
    "",
    width: 100%,
    inset: {padding}pt
  ),
)
"#
        )
    }
}

pub struct CourtRow {
    pub zones: Option<Vec<CourtCell>>,
    pub label: String,
    pub zone_size: u8,
}

impl CourtRow {
    pub fn new(
        values: Option<Vec<(Option<f64>, Option<f64>)>>,
        zone_size: u8,
        spacing: Option<u8>,
        label: String,
    ) -> Self {
        let line_size: u8 = 1;
        let configs = [
            (2, true, false, true, true),    // zone 1
            (1, false, false, true, true),   // zone 2
            (0, false, true, true, true),    // zone 3
            (-1, true, false, false, false), // zone 4
            (3, false, false, false, false), // zone 5
            (4, false, true, false, false),  // zone 6
            (-1, true, false, false, true),  // zone 7
            (-1, false, false, false, true), // zone 8
            (-1, false, true, false, true),  // zone 9
        ];
        Self {
            zones: values.map(|values| {
                configs
                    .iter()
                    .map(|&(i, left, right, top, bottom)| {
                        let (value_top, value_bottom, is_empty) = if i >= 0 {
                            let (a, b) = values[i as usize];
                            (a, b, false)
                        } else {
                            (None, None, true)
                        };
                        let mut zone = CourtCell::new(value_top, value_bottom, is_empty);
                        if left {
                            zone.stroke_left = Some(line_size);
                        }
                        if right {
                            zone.stroke_right = Some(line_size);
                        }
                        if top {
                            zone.stroke_top = Some(line_size);
                        }
                        if bottom {
                            zone.stroke_bottom = Some(line_size);
                        }
                        zone.spacing = spacing;
                        zone
                    })
                    .collect()
            }),
            zone_size,
            label,
        }
    }

    fn render_grid(&self) -> String {
        match &self.zones {
            None => "".to_string(),
            Some(zones) => {
                let size = 3;
                format!(
                    r#"
  #align(center, text(weight: "bold", "{}"))
  #grid(
    columns: {},
    rows: {},
    stroke: none,
    {}
  )
"#,
                    escape_text(&self.label),
                    size,
                    size,
                    zones
                        .iter()
                        .map(|z| z.render())
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            }
        }
    }

    fn render(&self) -> String {
        let rect_size = self.zone_size * 3;
        let grid = self.render_grid();
        format!(
            r#"
  rect(width: {}pt, height: {}pt, inset: 0%, stroke: none)[
  {}
],

"#,
            rect_size, rect_size, grid,
        )
    }
}

pub struct CourtCell {
    pub value_top: Option<f64>,
    pub value_bottom: Option<f64>,
    pub stroke_left: Option<u8>,
    pub stroke_right: Option<u8>,
    pub stroke_bottom: Option<u8>,
    pub stroke_top: Option<u8>,
    pub spacing: Option<u8>,
    pub hidden: bool,
}

impl CourtCell {
    pub fn new(value_top: Option<f64>, value_bottom: Option<f64>, hidden: bool) -> Self {
        Self {
            value_top,
            value_bottom,
            stroke_left: None,
            stroke_right: None,
            stroke_bottom: None,
            stroke_top: None,
            spacing: None,
            hidden,
        }
    }

    fn render(&self) -> String {
        let stroke = if let (None, None, None, None) = (
            self.stroke_left,
            self.stroke_right,
            self.stroke_bottom,
            self.stroke_top,
        ) {
            "stroke: none,".to_string()
        } else {
            format!(
                r#"stroke: (
  {}
  {}
  {}
  {}
)"#,
                match self.stroke_left {
                    None => "".to_string(),
                    Some(x) => format!("left: {}pt,", x),
                },
                match self.stroke_right {
                    None => "".to_string(),
                    Some(x) => format!("right: {}pt,", x),
                },
                match self.stroke_top {
                    None => "".to_string(),
                    Some(x) => format!("top: {}pt,", x),
                },
                match self.stroke_bottom {
                    None => "".to_string(),
                    Some(x) => format!("bottom: {}pt,", x),
                }
            )
        };
        format!(
            r#"
dst-perc-rect(
  {}
)[#align(center)[#stack(
  dir: ttb,
  {}
  dst-perc("{}"),
  dst-perc-s("{}"),
)]],
"#,
            stroke,
            match self.spacing {
                None => "".to_string(),
                Some(x) => format!("spacing: {}pt,", x),
            },
            if self.hidden {
                "".to_string()
            } else {
                fmt_pct(self.value_top)
            },
            if self.hidden {
                "".to_string()
            } else {
                fmt_pct(self.value_bottom)
            },
        )
    }
}

fn event_stats(
    stats: &Stats,
    event_type: EventTypeEnum,
    evals: Vec<EvalEnum>,
    phases: Vec<Option<PhaseEnum>>,
    player: Option<Uuid>,
) -> String {
    let title = event_type.friendly_name(current_labels());
    let rotations = [0, 1, 2, 3, 4, 5];
    let mut header_cells: Vec<Cell> = vec![
        Cell {
            content: title.to_string(),
            background_color: Some(GRAY.to_string()),
            text_color: Some(WHITE.to_string()),
            font_weight: Some(FontWeight::Bold),
            ..Default::default()
        },
        Cell {
            content: "eff.".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
        Cell {
            content: "pos.".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
    ];
    header_cells.extend(evals.iter().map(|eval| Cell {
        content: eval.to_string(),
        background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
        ..Default::default()
    }));
    let mut table = Table::new(Row::new(header_cells));
    table.gutter = Some(2);
    table.stroke_size = Some(0);
    table.padding = Some(6);
    table.font_size = Some(8);
    table.background_color = Some(TABLE_BACKGROUND_COLOR.to_string());
    for phase in phases {
        let efficiency = stats
            .event_positiveness(event_type, player, phase, None, None, Metric::Efficiency)
            .map(|(perc, _total, _count)| perc);
        let mut cells: Vec<Cell> = vec![
            Cell {
                content: match phase {
                    Some(phase) => phase.to_string(),
                    None => current_labels().global.to_string(),
                },
                background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
                ..Default::default()
            },
            Cell {
                content: fmt_pct(efficiency),
                font_weight: Some(FontWeight::Bold),
                background_color: efficiency
                    .map(|value| color_interpolation(value, false))
                    .map(|c| format!("{}{}", c, EFFICIENCY_ALPHA)),
                ..Default::default()
            },
            Cell {
                content: fmt_pct(
                    stats
                        .event_positiveness(event_type, player, phase, None, None, Metric::Positive)
                        .map(|(perc, _total, _count)| perc),
                ),
                ..Default::default()
            },
        ];
        cells.extend(evals.iter().map(|eval| {
            Cell {
                content: fmt_pct(
                    stats
                        .event_percentage(event_type, player, phase, None, None, *eval)
                        .map(|(perc, _total, _count)| perc),
                ),
                ..Default::default()
            }
        }));
        let row = Row::new(cells);
        table = table.add_row(row);
    }
    for rotation in rotations {
        let efficiency = stats
            .event_positiveness(
                event_type,
                player,
                None,
                Some(rotation),
                None,
                Metric::Efficiency,
            )
            .map(|(perc, _total, _count)| perc);
        let mut cells: Vec<Cell> = vec![
            Cell {
                content: format!("{}{}", current_labels().setter_prefix, rotation + 1),
                background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
                ..Default::default()
            },
            Cell {
                content: fmt_pct(efficiency),
                font_weight: Some(FontWeight::Bold),
                background_color: efficiency
                    .map(|value| color_interpolation(value, false))
                    .map(|c| format!("{}{}", c, EFFICIENCY_ALPHA)),
                ..Default::default()
            },
            Cell {
                content: fmt_pct(
                    stats
                        .event_positiveness(
                            event_type,
                            player,
                            None,
                            Some(rotation),
                            None,
                            Metric::Positive,
                        )
                        .map(|(perc, _total, _count)| perc),
                ),
                ..Default::default()
            },
        ];
        cells.extend(evals.iter().map(|eval| {
            Cell {
                content: fmt_pct(
                    stats
                        .event_percentage(event_type, player, None, Some(rotation), None, *eval)
                        .map(|(perc, _total, _count)| perc),
                ),
                ..Default::default()
            }
        }));
        let row = Row::new(cells);
        table = table.add_row(row);
    }
    let mut sub_header_cells: Vec<Cell> = vec![
        Cell {
            content: "".to_string(),
            ..Default::default()
        },
        Cell {
            content: "".to_string(),
            ..Default::default()
        },
        Cell {
            content: "".to_string(),
            ..Default::default()
        },
    ];
    sub_header_cells.extend(evals.iter().map(|eval| Cell {
        content: eval.to_string(),
        background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
        ..Default::default()
    }));
    let sub_header = Row::new(sub_header_cells);
    table = table.add_row(sub_header);
    let mut total_cells: Vec<Cell> = vec![
        Cell {
            content: "tot.".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
        Cell {
            content: "".to_string(),
            ..Default::default()
        },
        Cell {
            content: "".to_string(),
            ..Default::default()
        },
    ];
    total_cells.extend(evals.iter().map(|eval| Cell {
        content: fmt_u32(stats.event_count(event_type, player, None, None, None, Some(*eval))),
        ..Default::default()
    }));
    let total = Row::new(total_cells);
    table = table.add_row(total);
    table.render()
}

fn attack_stats(
    stats: &Stats,
    evals: Vec<EvalEnum>,
    phases: Vec<Option<PhaseEnum>>,
    player: Option<Uuid>,
) -> String {
    let event_type = EventTypeEnum::A;
    let title = event_type.friendly_name(current_labels());
    let rotations = [0, 1, 2, 3, 4, 5];
    let mut header_cells: Vec<Cell> = vec![
        Cell {
            content: title.to_string(),
            background_color: Some(GRAY.to_string()),
            text_color: Some(WHITE.to_string()),
            font_weight: Some(FontWeight::Bold),
            ..Default::default()
        },
        Cell {
            content: "eff.".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
    ];
    header_cells.extend(evals.iter().map(|eval| Cell {
        content: match eval {
            EvalEnum::Perfect | EvalEnum::Positive | EvalEnum::Negative => {
                format!("eff. {}", eval)
            }
            _ => eval.to_string(),
        },
        background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
        ..Default::default()
    }));
    header_cells.extend(vec![
        Cell {
            content: "".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
        Cell {
            content: "".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
    ]);
    let mut table = Table::new(Row::new(header_cells));
    table.gutter = Some(2);
    table.stroke_size = Some(0);
    table.padding = Some(6);
    table.font_size = Some(8);
    table.background_color = Some(TABLE_BACKGROUND_COLOR.to_string());
    for phase in phases {
        let efficiency = stats
            .event_positiveness(event_type, player, phase, None, None, Metric::Efficiency)
            .map(|(perc, _total, _count)| perc);
        let mut cells: Vec<Cell> = vec![
            Cell {
                content: match phase {
                    Some(phase) => phase.to_string(),
                    None => current_labels().global.to_string(),
                },
                background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
                ..Default::default()
            },
            Cell {
                content: fmt_pct(efficiency),
                font_weight: Some(FontWeight::Bold),
                background_color: efficiency
                    .map(|value| color_interpolation(value, false))
                    .map(|c| format!("{}{}", c, EFFICIENCY_ALPHA)),
                ..Default::default()
            },
        ];
        cells.extend(evals.iter().map(|eval| {
            match eval {
                EvalEnum::Perfect | EvalEnum::Positive | EvalEnum::Negative => {
                    let efficiency = stats
                        .attack_efficiency(player, phase, None, None, *eval)
                        .map(|(perc, _total, _count)| perc);
                    Cell {
                        content: fmt_pct(efficiency),
                        font_weight: Some(FontWeight::Bold),
                        background_color: efficiency
                            .map(|value| color_interpolation(value, false))
                            .map(|c| format!("{}{}", c, EFFICIENCY_ALPHA)),
                        ..Default::default()
                    }
                }
                _ => Cell {
                    content: fmt_pct(
                        stats
                            .event_percentage(event_type, player, phase, None, None, *eval)
                            .map(|(perc, _total, _count)| perc),
                    ),
                    ..Default::default()
                },
            }
        }));
        cells.extend(vec![
            Cell {
                content: "".to_string(),
                ..Default::default()
            },
            Cell {
                content: "".to_string(),
                ..Default::default()
            },
        ]);
        let row = Row::new(cells);
        table = table.add_row(row);
    }
    for rotation in rotations {
        let mut cells: Vec<Cell> = vec![
            Cell {
                content: format!("{}{}", current_labels().setter_prefix, rotation + 1),
                background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
                ..Default::default()
            },
            Cell {
                content: fmt_pct(
                    stats
                        .event_positiveness(
                            event_type,
                            player,
                            None,
                            Some(rotation),
                            None,
                            Metric::Efficiency,
                        )
                        .map(|(perc, _total, _count)| perc),
                ),
                ..Default::default()
            },
        ];
        cells.extend(evals.iter().map(|eval| {
            Cell {
                content: fmt_pct(
                    match eval {
                        EvalEnum::Perfect | EvalEnum::Positive | EvalEnum::Negative => {
                            stats.attack_efficiency(player, None, Some(rotation), None, *eval)
                        }
                        _ => stats.event_percentage(
                            event_type,
                            player,
                            None,
                            Some(rotation),
                            None,
                            *eval,
                        ),
                    }
                    .map(|(perc, _total, _count)| perc),
                ),
                ..Default::default()
            }
        }));
        cells.extend(vec![
            Cell {
                content: "".to_string(),
                ..Default::default()
            },
            Cell {
                content: "".to_string(),
                ..Default::default()
            },
        ]);
        let row = Row::new(cells);
        table = table.add_row(row);
    }
    let mut sub_header_cells: Vec<Cell> = vec![
        Cell {
            content: "".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
        Cell {
            content: "".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
    ];
    sub_header_cells.extend(evals.iter().map(|eval| Cell {
        content: eval.to_string(),
        background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
        ..Default::default()
    }));
    sub_header_cells.extend(vec![
        Cell {
            content: "".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
        Cell {
            content: "".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
    ]);
    let sub_header = Row::new(sub_header_cells);
    table = table.add_row(sub_header);
    let mut total_cells: Vec<Cell> = vec![
        Cell {
            content: "tot.".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
        Cell {
            content: "".to_string(),
            ..Default::default()
        },
    ];
    total_cells.extend(evals.iter().map(|eval| Cell {
        content: fmt_u32(stats.event_count(event_type, player, None, None, None, Some(*eval))),
        ..Default::default()
    }));
    total_cells.extend(vec![
        Cell {
            content: "".to_string(),
            ..Default::default()
        },
        Cell {
            content: "".to_string(),
            ..Default::default()
        },
    ]);
    let total = Row::new(total_cells);
    table = table.add_row(total);
    table.render()
}

fn conversion_rate_stats<F>(
    stats: &Stats,
    phases: Vec<Option<PhaseEnum>>,
    title: &str,
    f: F,
) -> String
where
    F: Fn(&Stats, &Option<PhaseEnum>, Option<u8>) -> Option<(f64, u32, u32)>,
{
    let rotations: [u8; 6] = [0, 1, 2, 3, 4, 5];
    let mut header_cells: Vec<Cell> = vec![Cell {
        content: title.to_string(),
        background_color: Some(GRAY.to_string()),
        text_color: Some(WHITE.to_string()),
        font_weight: Some(FontWeight::Bold),
        ..Default::default()
    }];
    header_cells.extend(rotations.iter().map(|rotation| Cell {
        content: format!("{}{}", current_labels().setter_prefix, (*rotation + 1)),
        background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
        ..Default::default()
    }));
    header_cells.push(Cell {
        content: current_labels().global.to_string(),
        background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
        ..Default::default()
    });
    let mut table = Table::new(Row::new(header_cells));
    table.gutter = Some(2);
    table.stroke_size = Some(0);
    table.padding = Some(6);
    table.font_size = Some(8);
    table.background_color = Some(TABLE_BACKGROUND_COLOR.to_string());
    for phase in phases {
        let mut cells: Vec<Cell> = vec![Cell {
            content: phase
                .as_ref()
                .map(|p| p.to_string())
                .unwrap_or_else(|| current_labels().global.to_string()),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        }];
        cells.extend(rotations.iter().map(|rotation| {
            let value = f(stats, &phase, Some(*rotation));
            let rate = value.map(|(rate, _, _)| rate);
            let tot_str = match value {
                Some((_, total, count)) => format!(" ({count}/{total})"),
                None => "".to_string(),
            };
            Cell {
                content: format!("{}{}", fmt_rate(rate), tot_str),
                ..Default::default()
            }
        }));
        let value = f(stats, &phase, None);
        let rate = value.map(|(rate, _, _)| rate);
        let tot_str = match value {
            Some((_, total, count)) => format!(" ({count}/{total})"),
            None => "".to_string(),
        };
        cells.push(Cell {
            content: format!("{}{}", fmt_rate(rate), tot_str),
            ..Default::default()
        });
        table = table.add_row(Row::new(cells));
    }
    table.render()
}

fn possessions_conversion_rate_stats(stats: &Stats, phases: Vec<Option<PhaseEnum>>) -> String {
    conversion_rate_stats(stats, phases, "pos. cnv. rt.", |s, p, r| {
        s.number_of_possessions_per_earned_point(*p, r)
    })
}

fn phases_conversion_rate_stats(stats: &Stats, phases: Vec<Option<PhaseEnum>>) -> String {
    conversion_rate_stats(stats, phases, "ph. cnv. rt.", |s, p, r| {
        s.number_of_phases_per_scored_point(*p, r)
    })
}

fn resume_stats(stats: &Stats, phases: Vec<Option<PhaseEnum>>, player: Option<Uuid>) -> String {
    let header_cells: Vec<Cell> = vec![
        Cell {
            content: "".to_string(),
            background_color: Some(WHITE.to_string()),
            ..Default::default()
        },
        Cell {
            content: "ace".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
        Cell {
            content: current_labels().faults.to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
        Cell {
            content: current_labels().errors_report_label.to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
        Cell {
            content: match player {
                Some(_) => "".to_string(),
                None => current_labels().opponent_errors_report_label.to_string(),
            },
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
        Cell {
            content: "".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
        Cell {
            content: "".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
        Cell {
            content: "".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
        Cell {
            content: "".to_string(),
            background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
            ..Default::default()
        },
    ];
    let mut table = Table::new(Row::new(header_cells));
    table.gutter = Some(2);
    table.stroke_size = Some(0);
    table.padding = Some(6);
    table.font_size = Some(8);
    table.background_color = Some(TABLE_BACKGROUND_COLOR.to_string());
    for phase in phases {
        let err_gbl_count = match stats.errors.query(phase, None, player, None).count() {
            0 => None,
            x => Some(x),
        };
        let err_gbl = fmt_usize(err_gbl_count);
        let err_unf_gbl = fmt_usize(
            match stats
                .errors
                .query(phase, None, player, Some(ErrorTypeEnum::Unforced))
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

        let opp_err_gbl_count = match stats.opponent_errors.query(phase, None).count() {
            0 => None,
            x => Some(x),
        };
        let opp_err_gbl = fmt_usize(opp_err_gbl_count);
        let opp_err_gbl_str = match opp_err_gbl_count {
            None => "-".to_string(),
            Some(_) => opp_err_gbl,
        };
        let cells: Vec<Cell> = vec![
            Cell {
                content: match phase {
                    Some(phase) => phase.to_string(),
                    None => current_labels().global.to_string(),
                },
                background_color: Some(TABLE_HEADER_BACKGROUND_COLOR.to_string()),
                ..Default::default()
            },
            Cell {
                content: fmt_u32(stats.event_count(
                    EventTypeEnum::S,
                    player,
                    phase,
                    None,
                    None,
                    Some(EvalEnum::Perfect),
                )),
                ..Default::default()
            },
            Cell {
                content: fmt_u32(stats.event_count(
                    EventTypeEnum::F,
                    player,
                    phase,
                    None,
                    None,
                    None,
                )),
                ..Default::default()
            },
            Cell {
                content: err_gbl_str,
                ..Default::default()
            },
            Cell {
                content: match player {
                    Some(_) => "".to_string(),
                    None => opp_err_gbl_str,
                },
                ..Default::default()
            },
            Cell {
                content: "".to_string(),
                ..Default::default()
            },
            Cell {
                content: "".to_string(),
                ..Default::default()
            },
            Cell {
                content: "".to_string(),
                ..Default::default()
            },
            Cell {
                content: "".to_string(),
                ..Default::default()
            },
        ];
        let row = Row::new(cells);
        table = table.add_row(row);
    }
    table.render()
}

fn counter_attack_stats(stats: &Stats, player: Option<Uuid>) -> String {
    let zones: [ZoneEnum; 5] = [
        ZoneEnum::Two,
        ZoneEnum::Three,
        ZoneEnum::Four,
        ZoneEnum::Eight,
        ZoneEnum::Nine,
    ];
    let values: Vec<(Option<f64>, Option<f64>)> = zones
        .map(|z| {
            stats.counter_attack_conversion_rate(player, Some(PhaseEnum::Break), None, Some(z))
        })
        .map(|v| (v, None))
        .to_vec();
    let mut court = Courts::new(
        current_labels().counter_attack.to_string(),
        COURT_TITLE_FONT_SIZE,
        COURT_ZONE_SIZE,
        COURT_VALUE_BOTTOM_FONT_SIZE,
        COURT_VALUE_TOP_FONT_SIZE,
        COURT_GUTTER,
        DEFAULT_TABLE_PADDING,
    );
    court.add_row(CourtRow::new(
        Some(values),
        COURT_ZONE_SIZE,
        Some(COURT_ZONE_SPACING),
        current_labels().global.to_string(),
    ));
    let rows = [0, 1, 2, 3, 4, 5].map(|rotation| {
        let values: Vec<(Option<f64>, Option<f64>)> = zones
            .map(|z| {
                stats.counter_attack_conversion_rate(
                    player,
                    Some(PhaseEnum::Break),
                    Some(rotation as u8),
                    Some(z),
                )
            })
            .map(|v| (v, None))
            .to_vec();
        CourtRow::new(
            Some(values),
            COURT_ZONE_SIZE,
            Some(COURT_ZONE_SPACING),
            format!("{}{}", current_labels().setter_prefix, rotation + 1),
        )
    });
    for row in rows {
        court.add_row(row);
    }
    court.render()
}

fn distribution_stats(stats: &Stats) -> String {
    let phases: [Option<PhaseEnum>; 3] = [None, Some(PhaseEnum::Break), Some(PhaseEnum::SideOut)];
    let zones: [ZoneEnum; 5] = [
        ZoneEnum::Two,
        ZoneEnum::Three,
        ZoneEnum::Four,
        ZoneEnum::Eight,
        ZoneEnum::Nine,
    ];
    let mut court = Courts::new(
        current_labels().distribution.to_string(),
        COURT_TITLE_FONT_SIZE,
        COURT_ZONE_SIZE,
        COURT_VALUE_BOTTOM_FONT_SIZE,
        COURT_VALUE_TOP_FONT_SIZE,
        COURT_GUTTER,
        DEFAULT_TABLE_PADDING,
    );
    let prev_evals: [Option<EvalEnum>; 5] = [
        None,
        Some(EvalEnum::Perfect),
        Some(EvalEnum::Positive),
        Some(EvalEnum::Exclamative),
        Some(EvalEnum::Negative),
    ];
    for prev_eval in prev_evals {
        for phase in phases {
            let values = zones
                .iter()
                .map(|z| stats.distribution.zone_stats(*z, phase, None, prev_eval))
                .map(|v| match v {
                    None => (None, None),
                    Some((v1, v2)) => (Some(v1), Some(v2)),
                })
                .collect::<Vec<_>>();
            court.add_row(CourtRow::new(
                Some(values),
                COURT_ZONE_SIZE,
                Some(COURT_ZONE_SPACING),
                match (phase, prev_eval) {
                    (None, None) => current_labels().global.to_string(),
                    (Some(phase), None) => phase.to_string(),
                    (None, Some(prev_eval)) => {
                        format!("{} ({})", current_labels().global, prev_eval)
                    }
                    (Some(phase), Some(prev_eval)) => {
                        format!("{} ({})", phase, prev_eval)
                    }
                },
            ));
        }
    }
    court.render()
}
