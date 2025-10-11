use crate::{
    localization::current_labels,
    reporting::typst_content::TypstContent,
    shapes::enums::{FriendlyName, PhaseEnum, ZoneEnum},
};
use std::collections::HashMap;

pub struct CourtCell {
    stroke: Option<&'static str>,
    spacing: Option<u8>,
    text_top: Option<String>,
    text_bottom: Option<String>,
}

impl CourtCell {
    pub fn new() -> Self {
        Self {
            stroke: None,
            spacing: None,
            text_top: None,
            text_bottom: None,
        }
    }

    pub fn stroke(mut self, s: &'static str) -> Self {
        self.stroke = Some(s);
        self
    }

    pub fn spacing(mut self, s: u8) -> Self {
        self.spacing = Some(s);
        self
    }

    pub fn text_top<T: Into<String>>(mut self, t: T) -> Self {
        self.text_top = Some(t.into());
        self
    }

    pub fn text_bottom<T: Into<String>>(mut self, t: T) -> Self {
        self.text_bottom = Some(t.into());
        self
    }
}

impl TypstContent for CourtCell {
    fn render(&self) -> String {
        let stroke = self
            .stroke
            .map_or(String::new(), |s| format!("stroke: {},", s));
        let spacing = self
            .spacing
            .map_or(String::new(), |s| format!("spacing: {}pt,", s));
        let text_top = self
            .text_top
            .as_ref()
            .map_or("-".to_string(), |s| s.to_string());
        let text_bottom = self
            .text_bottom
            .as_ref()
            .map_or("-".to_string(), |s| s.to_string());
        format!(
            r#"
dst-perc-rect(
  {stroke}
)[#align(center)[#stack(
  dir: ttb,
  {spacing}
  dst-perc("{text_top}"),
  dst-perc-s("{text_bottom}"),
)]],
"#
        )
    }
}

#[derive(Debug)]
pub struct ZoneValue {
    top: Option<String>,
    bottom: Option<String>,
}

pub struct Court {
    zones: HashMap<ZoneEnum, ZoneValue>,
    spacing: u8,
    label: String,
}

impl Court {
    fn new(spacing: u8, label: &str) -> Self {
        Self {
            zones: HashMap::new(),
            spacing,
            label: label.to_string(),
        }
    }

    pub fn set_zone(&mut self, zone: ZoneEnum, top: Option<String>, bottom: Option<String>) {
        self.zones.insert(zone, ZoneValue { top, bottom });
    }

    fn render_cell(&self, zone: ZoneEnum, stroke: &'static str) -> String {
        let (top_text, bottom_text) = self
            .zones
            .get(&zone)
            .map(|v| {
                (
                    v.top.clone().unwrap_or_else(|| "-".to_string()),
                    v.bottom.clone().unwrap_or_else(|| "-".to_string()),
                )
            })
            .unwrap_or_else(|| ("-".to_string(), "-".to_string()));
        CourtCell::new()
            .stroke(stroke)
            .spacing(self.spacing)
            .text_top(top_text)
            .text_bottom(bottom_text)
            .render()
    }

    fn render(&self) -> String {
        let z4 = self.render_cell(ZoneEnum::Four, "(left: 1pt, top: 1pt, bottom: 1pt)");
        let z3 = self.render_cell(ZoneEnum::Three, "(top: 1pt, bottom: 1pt)");
        let z2 = self.render_cell(ZoneEnum::Two, "(right: 1pt, top: 1pt, bottom: 1pt)");
        let z7 = CourtCell::new()
            .stroke("(left: 1pt)")
            .spacing(self.spacing)
            .render();
        let z8 = self.render_cell(ZoneEnum::Eight, "none");
        let z9 = self.render_cell(ZoneEnum::Nine, "(right: 1pt)");
        let z5 = CourtCell::new()
            .stroke("(left: 1pt, bottom: 1pt)")
            .spacing(self.spacing)
            .render();
        let z6 = CourtCell::new()
            .stroke("(bottom: 1pt)")
            .spacing(self.spacing)
            .render();
        let z1 = CourtCell::new()
            .stroke("(right: 1pt, bottom: 1pt)")
            .spacing(self.spacing)
            .render();
        let label = &self.label;
        format!(
            r#"
grid(
  columns: (auto, auto, auto),
  rows: 4,
  rect(inset: 0%, stroke: none)[
    #align(center, text(weight: "bold", "{label}"))
    #grid(
      columns: 3,
      rows: 3,
      stroke: none,
      {z4}
      {z3}
      {z2}
      {z7}
      {z8}
      {z9}
      {z5}
      {z6}
      {z1}
    )
  ],
),
"#
        )
    }
}

pub fn render_court<F>(label: &str, phase: Option<PhaseEnum>, spacing: u8, formatter: F) -> String
where
    F: Fn(&mut Court, ZoneEnum),
{
    let title = match phase {
        Some(p) => format!("{} ({})", label, p.friendly_name(current_labels())),
        None => label.to_string(),
    };
    let mut court = Court::new(spacing, &title);
    for &zone in &[
        ZoneEnum::Four,
        ZoneEnum::Three,
        ZoneEnum::Two,
        ZoneEnum::Eight,
        ZoneEnum::Nine,
    ] {
        formatter(&mut court, zone);
    }
    court.render()
}
