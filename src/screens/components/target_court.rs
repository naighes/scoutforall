//! The court of the opponent, to tell where the ball went: the nine zones
//! and the conflict zones between them (see `CourtZoneEnum`), the net at
//! the top, with the cursor and the zone chosen.
use crate::localization::current_labels;
use crate::screens::components::panel;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Paragraph},
};
use scout4all_core::shapes::court_zone::CourtZoneEnum;

/// The rows a zone takes with its borders.
const BORDERED_ROW: u16 = 3;

/// The widths of the five columns of the court (zone, conflict, zone,
/// conflict, zone): the conflict columns as wide as their label at least.
fn columns(width: u16) -> [u16; 5] {
    let conflict = (width / 11).max(3).min(width / 5);
    let zones = width.saturating_sub(2 * conflict);
    let zone = zones / 3;
    // what's left goes to the middle zone
    [zone, conflict, zones - 2 * zone, conflict, zone]
}

/// The heights of the five rows (zones, conflicts, zones, conflicts,
/// zones): the conflicts one line, more on a tall court.
fn rows(height: u16) -> [u16; 5] {
    let conflict = (height / 11).max(1).min(height / 5);
    let zones = height.saturating_sub(2 * conflict);
    let zone = zones / 3;
    [zone, conflict, zones - 2 * zone, conflict, zone]
}

/// The style of a zone: the one chosen highlighted, the cursor reversed.
fn style(zone: CourtZoneEnum, cursor: CourtZoneEnum, chosen: Option<CourtZoneEnum>) -> Style {
    let base = match (Some(zone) == chosen, zone.is_zone()) {
        (true, _) => Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD),
        (false, true) => Style::default().add_modifier(Modifier::BOLD),
        (false, false) => Style::default().fg(Color::DarkGray),
    };
    match zone == cursor {
        true => base.add_modifier(Modifier::REVERSED),
        false => base,
    }
}

/// The lines in the middle of the area, in a box if it fits: one each, or
/// all on one if there's no room.
fn render_cell(f: &mut Frame, area: Rect, lines: Vec<Line<'static>>, style: Style, boxed: bool) {
    let block = match boxed {
        true => Block::bordered(),
        false => Block::default(),
    };
    let inner = block.inner(area);
    // on one line when they don't fit one each
    let lines = match lines.len() > usize::from(inner.height) {
        true => vec![Line::from(
            lines
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
                .join(" "),
        )],
        false => lines,
    };
    let shown = lines.len().min(usize::from(inner.height));
    let top = usize::from(inner.height).saturating_sub(shown) / 2;
    let lines: Vec<Line> = std::iter::repeat_n(Line::from(""), top)
        .chain(lines.into_iter().take(shown))
        .collect();
    f.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(block)
            .style(style),
        area,
    );
}

/// The court where our serve went, to choose it: in a panel titled with the
/// zone chosen, if any.
pub fn render(
    f: &mut Frame,
    area: Rect,
    title: &str,
    cursor: CourtZoneEnum,
    chosen: Option<CourtZoneEnum>,
) {
    let labels = current_labels();
    let told = chosen.map_or_else(|| labels.no_zone.to_string(), |z| z.to_string());
    let block = panel::section(format!("{title}: {told}"))
        .style(Style::default().add_modifier(Modifier::REVERSED));
    render_court(f, area, block, true, |zone| {
        (
            vec![Line::from(zone.to_string())],
            style(zone, cursor, chosen),
        )
    });
}

/// The court in the block: each zone with its lines and style, told by
/// `cell`; the nine zones in a box when `boxes` and there's room.
pub fn render_court<F>(f: &mut Frame, area: Rect, block: Block, boxes: bool, cell: F)
where
    F: Fn(CourtZoneEnum) -> (Vec<Line<'static>>, Style),
{
    let inner = block.inner(area);
    f.render_widget(block, area);
    // an empty line on each side, if there's room
    let court = match inner.width > 20 && inner.height > 7 {
        true => Rect {
            x: inner.x + 1,
            y: inner.y + 1,
            width: inner.width - 2,
            height: inner.height - 2,
        },
        false => inner,
    };
    let [w0, w1, w2, w3, w4] = columns(court.width);
    let heights = rows(court.height);
    let boxed = boxes && heights.iter().step_by(2).all(|h| *h >= BORDERED_ROW);
    let x = court.x;
    let fives = [
        (x, w0),
        (x + w0, w1),
        (x + w0 + w1, w2),
        (x + w0 + w1 + w2, w3),
        (x + w0 + w1 + w2 + w3, w4),
    ];
    let threes = [(x, w0 + w1), (x + w0 + w1, w2), (x + w0 + w1 + w2, w3 + w4)];
    let mut y = court.y;
    for (row, height) in CourtZoneEnum::grid().iter().zip(heights) {
        // a row of five takes a column each, a row of three the columns of
        // a zone and a conflict at the sides, the middle zone in between
        let spans: &[(u16, u16)] = match row.len() {
            5 => &fives,
            _ => &threes,
        };
        for (zone, (x, width)) in row.iter().zip(spans.iter().copied()) {
            let area = Rect {
                x,
                y,
                width,
                height,
            };
            let (lines, style) = cell(*zone);
            render_cell(f, area, lines, style, boxed && zone.is_zone());
        }
        y += height;
    }
}
