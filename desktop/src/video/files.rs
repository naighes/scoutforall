//! The files of the video, in the order played: added (several at once,
//! put in the order of their starts), moved, removed; and the band under
//! the bar of the player with where each starts.

use {
    super::{Message, clock},
    crate::{
        strings::Strings,
        theme::{Colors, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, Row, container, row, text},
    },
    scout4all_core::video::VideoFile,
    std::path::Path,
};

/// The files with the ones added: the ones already there skipped, all in
/// the order of their starts (`video::in_order`).
pub fn added(files: &[VideoFile], new: Vec<VideoFile>) -> Vec<VideoFile> {
    let fresh: Vec<VideoFile> = new
        .into_iter()
        .filter(|f| !files.iter().any(|there| there.path == f.path))
        .collect();
    scout4all_core::video::in_order([files.to_vec(), fresh].concat())
}

/// The file at `i` one place earlier (`earlier`) or later, if it can go.
pub fn moved(files: &[VideoFile], i: usize, earlier: bool) -> Vec<VideoFile> {
    let mut files = files.to_vec();
    let other = match earlier {
        true => i.checked_sub(1),
        false => Some(i + 1).filter(|j| *j < files.len()),
    };
    if let Some(j) = other.filter(|_| i < files.len()) {
        files.swap(i, j);
    }
    files
}

fn name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// A file a line: its place, its name, when it starts, how long it is,
/// the buttons to move it or take it out.
pub fn view<'a>(
    files: &[VideoFile],
    durations_ms: &[u64],
    s: &'static Strings,
    c: Colors,
) -> Element<'a, Message> {
    let count = files.len();
    let lines = files.iter().enumerate().map(|(i, f)| {
        let start = f
            .start
            .map(|t| t.format("%H:%M:%S").to_string())
            .unwrap_or_else(|| "–".to_string());
        let length = durations_ms.get(i).map(|d| clock(*d)).unwrap_or_default();
        // a move that can't be made: no button, its place kept
        let arrow = |m: Option<Message>, label: &str, tip: &str| -> Element<'a, Message> {
            match m {
                Some(m) => ui::tip(ui::pill(label, false, Some(m), c), tip, c),
                None => iced::widget::space::horizontal()
                    .width(Length::Fixed(34.0))
                    .into(),
            }
        };
        row![
            text(format!("{}.", i + 1))
                .size(SIZE_SMALL)
                .color(c.muted)
                .width(Length::Fixed(24.0)),
            text(name(&f.path)).size(SIZE_SMALL).width(Length::Fill),
            text(format!("{start} · {length}"))
                .size(SIZE_SMALL)
                .color(c.muted),
            arrow(
                (i > 0).then_some(Message::MoveFile(i, true)),
                "↑",
                s.move_earlier
            ),
            arrow(
                (i + 1 < count).then_some(Message::MoveFile(i, false)),
                "↓",
                s.move_later
            ),
            arrow(Some(Message::RemoveFile(i)), "✕", s.remove),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    });
    Column::with_children(lines).spacing(6).into()
}

/// Under the bar, a part for each file as long as it: where one ends and
/// the next starts. Nothing for a single file.
pub fn band<'a>(files: &[VideoFile], durations_ms: &[u64], c: Colors) -> Element<'a, Message> {
    if durations_ms.len() < 2 {
        return Row::new().into();
    }
    let parts = durations_ms.iter().enumerate().map(|(i, d)| {
        let label = files.get(i).map(|f| name(&f.path)).unwrap_or_default();
        let shade = match i % 2 {
            0 => c.border,
            _ => c.button,
        };
        let portion = u16::try_from((d / 1000).clamp(1, u64::from(u16::MAX))).unwrap_or(1);
        container(text(label).size(11).color(c.muted))
            .padding([1, 6])
            .width(Length::FillPortion(portion))
            .style(move |_| container::Style {
                background: Some(shade.into()),
                ..container::Style::default()
            })
            .into()
    });
    Row::with_children(parts).spacing(2).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};

    fn file(path: &str, hour: Option<u32>) -> VideoFile {
        let offset = FixedOffset::east_opt(3600).unwrap();
        VideoFile {
            path: path.into(),
            start: hour.map(|h| offset.with_ymd_and_hms(2026, 11, 1, h, 0, 0).unwrap()),
        }
    }

    fn paths(files: &[VideoFile]) -> Vec<&str> {
        files.iter().map(|f| f.path.as_str()).collect()
    }

    #[test]
    fn the_files_added_in_the_order_of_their_starts() {
        let there = vec![file("b", Some(19))];
        let both = added(&there, vec![file("a", Some(18)), file("b", Some(19))]);
        assert_eq!(paths(&both), ["a", "b"]);
        let more = added(&both, vec![file("x", None)]);
        assert_eq!(paths(&more), ["a", "b", "x"]);
    }

    #[test]
    fn a_file_moved() {
        let files = vec![file("a", None), file("b", None), file("c", None)];
        assert_eq!(paths(&moved(&files, 1, true)), ["b", "a", "c"]);
        assert_eq!(paths(&moved(&files, 1, false)), ["a", "c", "b"]);
        // not past the ends
        assert_eq!(paths(&moved(&files, 0, true)), ["a", "b", "c"]);
        assert_eq!(paths(&moved(&files, 2, false)), ["a", "b", "c"]);
        assert_eq!(paths(&moved(&files, 7, false)), ["a", "b", "c"]);
    }
}
