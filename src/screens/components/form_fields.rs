//! Where the fields of a form go: one under the other, with room around
//! them when it's there, closer when it isn't, in two columns (the first half
//! on the left) when not even a line each fits. A low terminal shows them
//! all.

use ratatui::layout::{Constraint, Layout, Rect};

/// The heights of a field, from the roomiest: its text and the room around
/// it, down to its text alone.
const FIELD_HEIGHTS: [u16; 3] = [3, 2, 1];

/// The areas of `count` fields in `area`, in their order.
pub fn field_areas(area: Rect, count: usize) -> Vec<Rect> {
    let rows = |n: usize, height: u16, column: Rect| -> Vec<Rect> {
        Layout::vertical(
            (0..n)
                .map(|_| Constraint::Length(height))
                .chain([Constraint::Min(0)]),
        )
        .split(column)
        .iter()
        .take(n)
        .copied()
        .collect()
    };
    let needed = |height: u16| {
        u16::try_from(count)
            .unwrap_or(u16::MAX)
            .saturating_mul(height)
    };
    match FIELD_HEIGHTS
        .into_iter()
        .find(|h| area.height >= needed(*h))
    {
        Some(height) => rows(count, height, area),
        None => Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)])
            .spacing(1)
            .split(area)
            .iter()
            .flat_map(|column| rows(count.div_ceil(2), 1, *column))
            .take(count)
            .collect(),
    }
}
