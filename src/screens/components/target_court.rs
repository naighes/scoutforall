//! The court of the opponent as our server sees it, their end line at the
//! top: to tell where the ball went, a cell of a zone or two side by side
//! (see `CourtTarget`), with the cursor and the cells chosen; and, for the
//! statistics, the nine zones, with the conflict zones between them for
//! the reception.
use crate::localization::current_labels;
use crate::screens::components::panel;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Paragraph},
};
use scout4all_core::shapes::court_zone::{CourtCell, CourtSide, CourtTarget, CourtZoneEnum};

/// The rows a zone takes with its borders.
const BORDERED_ROW: u16 = 3;

/// The widths of the five columns of the court (zone, conflict, zone,
/// conflict, zone): the conflict columns as wide as their label at least.
/// Without the conflict zones, three as wide.
fn columns(width: u16, conflicts: bool) -> Vec<u16> {
    let conflict = match conflicts {
        true => (width / 11).max(4).min(width / 5),
        false => return split(width, 3),
    };
    let zones = width.saturating_sub(2 * conflict);
    let zone = zones / 3;
    // what's left goes to the middle zone
    vec![zone, conflict, zones - 2 * zone, conflict, zone]
}

/// The style of a cell: the ones chosen highlighted, the cursor reversed;
/// on `shade` (a zone told apart from the next ones without its box).
fn style(cell: CourtCell, cursor: CourtCell, chosen: Option<CourtTarget>, shade: Style) -> Style {
    let base = match chosen.is_some_and(|t| t.has(&cell)) {
        true => Style::default()
            .fg(Color::Black)
            .bg(Color::Green)
            .add_modifier(Modifier::BOLD),
        false => shade,
    };
    match cell == cursor {
        true => base.add_modifier(Modifier::REVERSED),
        false => base,
    }
}

/// `total` in `parts` sizes, what's left to the middle ones.
fn split(total: u16, parts: usize) -> Vec<u16> {
    let parts16 = u16::try_from(parts).unwrap_or(1).max(1);
    let size = total / parts16;
    let left = total - size * parts16;
    (0..parts16)
        .map(|i| size + u16::from(i >= (parts16 - left) / 2 && i < (parts16 - left) / 2 + left))
        .collect()
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

/// The court where the ball went, as shown on `side` (theirs for our
/// serve, ours for our reception), to choose it: the nine zones in a box
/// each when there's room, their four cells inside; in a panel titled with
/// what's told, if any.
pub fn render(
    f: &mut Frame,
    area: Rect,
    title: &str,
    side: CourtSide,
    cursor: CourtCell,
    chosen: Option<CourtTarget>,
) {
    let labels = current_labels();
    let told = chosen.map_or_else(|| labels.no_zone.to_string(), |t| t.described());
    let block = panel::section(format!("{title}: {told}"))
        .style(Style::default().add_modifier(Modifier::REVERSED));
    let inner = block.inner(area);
    f.render_widget(block, area);
    let court = match inner.width > 20 && inner.height > 7 {
        true => Rect {
            x: inner.x + 1,
            y: inner.y + 1,
            width: inner.width - 2,
            height: inner.height - 2,
        },
        false => inner,
    };
    let (widths, heights) = (split(court.width, 3), split(court.height, 3));
    // a box for each zone when its two rows still have a line each
    let boxed = heights.iter().all(|h| *h >= 4) && widths.iter().all(|w| *w >= 6);
    let grid = CourtCell::shown(side);
    let mut y = court.y;
    for (zone_row, height) in heights.iter().enumerate() {
        let mut x = court.x;
        for (zone_column, width) in widths.iter().enumerate() {
            let zone = Rect {
                x,
                y,
                width: *width,
                height: *height,
            };
            // without boxes, the zones as on a chessboard
            let shade = match (boxed, (zone_row + zone_column) % 2) {
                (false, 1) => Style::default().bg(Color::Indexed(238)),
                _ => Style::default(),
            };
            let block = match boxed {
                true => Block::bordered(),
                false => Block::default().style(shade),
            };
            let cells = block.inner(zone);
            f.render_widget(block, zone);
            let (cell_widths, cell_heights) = (split(cells.width, 2), split(cells.height, 2));
            let mut cy = cells.y;
            for (r, ch) in cell_heights.iter().enumerate() {
                let mut cx = cells.x;
                for (c, cw) in cell_widths.iter().enumerate() {
                    let cell = grid
                        .get(zone_row * 2 + r)
                        .and_then(|row| row.get(zone_column * 2 + c))
                        .copied();
                    if let Some(cell) = cell {
                        let area = Rect {
                            x: cx,
                            y: cy,
                            width: *cw,
                            height: *ch,
                        };
                        render_cell(
                            f,
                            area,
                            vec![Line::from(cell.to_string())],
                            style(cell, cursor, chosen, shade),
                            false,
                        );
                    }
                    cx += cw;
                }
                cy += ch;
            }
            x += width;
        }
        y += height;
    }
}

/// The court in the block, as shown on `side`: each zone with its lines
/// and style, told by `cell`, with the conflict zones between them when
/// `conflicts`; the nine zones in a box when `boxes` and there's room.
pub fn render_court<F>(
    f: &mut Frame,
    area: Rect,
    block: Block,
    side: CourtSide,
    (conflicts, boxes): (bool, bool),
    cell: F,
) where
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
    let widths = columns(court.width, conflicts);
    let heights = split(court.height, 3);
    let boxed = boxes && heights.iter().all(|h| *h >= BORDERED_ROW);
    let spans: Vec<(u16, u16)> = widths
        .iter()
        .scan(court.x, |x, width| {
            let span = (*x, *width);
            *x += width;
            Some(span)
        })
        .collect();
    let rows = match conflicts {
        true => CourtZoneEnum::shown(side),
        false => CourtZoneEnum::zones_shown(side),
    };
    let mut y = court.y;
    for (row, height) in rows.iter().zip(heights) {
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
