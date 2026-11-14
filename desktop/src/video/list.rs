//! The events of the match beside its video: set by set, each with its
//! time in the video, the one chosen lit. Each marked by its team (a bar on
//! the left: ours light blue, theirs pink), the point it ended won or lost
//! (its score green or red) and a direct error (its symbol red, a cross).

use {
    super::{Message, clock},
    crate::{
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, Id, column, row, scrollable, text},
    },
    scout4all_core::video::EventRef,
    scout4all_mobile::{VideoEvent, video::inside},
};

/// The list, to keep the event chosen in sight.
pub const LIST: &str = "video-events";

/// The time of the event in the video, `–` when not lined up; `<` before
/// the start, `>` past the end.
fn time(offset_ms: Option<i64>, duration_ms: Option<u64>) -> String {
    match (offset_ms, duration_ms) {
        (None, _) => "–".to_string(),
        (Some(ms), _) if ms < 0 => format!("<{}", clock(ms.unsigned_abs())),
        (Some(ms), Some(d)) if ms.unsigned_abs() > d => format!(">{}", clock(d)),
        (Some(ms), _) => clock(ms.unsigned_abs()),
    }
}

/// How far an event was moved, signed: `+3.0 s`, `−9.0 s`.
pub fn seconds(ms: i64) -> String {
    let sign = match ms < 0 {
        true => "−",
        false => "+",
    };
    let tenths = (ms.unsigned_abs() + 50) / 100;
    format!("{sign}{}.{} s", tenths / 10, tenths % 10)
}

/// What an event was for the rally: whose, the point it ended, an error.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mark {
    /// of the opponent: their point, their error
    pub theirs: bool,
    /// the point it ended: won (true) or lost
    pub point: Option<bool>,
    /// a direct error: ours (=, a fault) or theirs
    pub error: bool,
}

impl Mark {
    /// The event, after the score `before` (the team's first).
    pub fn of(e: &VideoEvent, before: (u8, u8)) -> Mark {
        use scout4all_core::shapes::enums::EventTypeEnum::{F, OE, OS};
        let event = e.event.event.event;
        let point = match (e.event.score_us > before.0, e.event.score_them > before.1) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        };
        Mark {
            theirs: matches!(event, OS | OE),
            point,
            error: matches!(event, F | OE) || e.event.event.symbol.as_deref() == Some("="),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn row_of<'a>(
    e: &VideoEvent,
    i: usize,
    mark: Mark,
    chosen: bool,
    anchored: bool,
    duration_ms: Option<u64>,
    s: &'static Strings,
    c: Colors,
) -> Element<'a, Message> {
    let color = match (e.event.editable, inside(e.offset_ms, duration_ms)) {
        (true, true) => c.text,
        _ => c.muted,
    };
    let bold = theme::fonts().bold;
    let name = ui::capitalized(&e.event.event.name);
    let name = match &e.event.event.technique {
        Some(t) => format!("{name} · {t}"),
        None => name,
    };
    let player: Element<'a, Message> = match &e.event.event.player {
        Some(p) => text(format!("{} {}", p.number, p.name))
            .size(SIZE_SMALL)
            .color(c.muted)
            .into(),
        None => column![].into(),
    };
    // the event the video is tied to: its time lit, an anchor; one put at
    // its own time: a pencil, how far in the tip
    let at: Element<'a, Message> = match (anchored, e.corrected_ms) {
        (false, Some(by)) => ui::tip(
            text(format!("✎ {}", time(e.offset_ms, duration_ms)))
                .size(SIZE_SMALL)
                .font(bold)
                .color(c.text)
                .shaping(text::Shaping::Advanced),
            &ui::fill(s.corrected_by, &[seconds(by)]),
            c,
        ),
        (true, _) => ui::tip(
            text(format!("⚓ {}", time(e.offset_ms, duration_ms)))
                .size(SIZE_SMALL)
                .font(bold)
                .color(c.accent)
                .shaping(text::Shaping::Advanced),
            s.anchored_event,
            c,
        ),
        (false, None) => text(time(e.offset_ms, duration_ms))
            .size(SIZE_SMALL)
            .color(c.muted)
            .into(),
    };
    let team = match mark.theirs {
        true => theme::TEAM_THEM,
        false => theme::TEAM_US,
    };
    // a direct error: its symbol (a cross for theirs, a fault) in red
    let symbol = match (mark.error, e.event.event.symbol.clone()) {
        (true, Some(symbol)) => (symbol, theme::LOST),
        (true, None) => ("✕".to_string(), theme::LOST),
        (false, symbol) => (symbol.unwrap_or_default(), color),
    };
    // the point it ended: its score green or red, in a pill
    let score = text(format!("{}–{}", e.event.score_us, e.event.score_them))
        .size(SIZE_SMALL)
        .font(bold);
    let score: Element<'a, Message> = match mark.point {
        Some(won) => iced::widget::container(score.color(c.background))
            .padding([1, 6])
            .style(move |_| iced::widget::container::Style {
                background: Some(
                    match won {
                        true => theme::WON,
                        false => theme::LOST,
                    }
                    .into(),
                ),
                border: iced::Border {
                    radius: 4.0.into(),
                    ..iced::Border::default()
                },
                ..iced::widget::container::Style::default()
            })
            .into(),
        None => iced::widget::container(score.color(color))
            .padding([1, 6])
            .into(),
    };
    let bar = iced::widget::container(iced::widget::space::vertical())
        .width(Length::Fixed(4.0))
        .height(Length::Fixed(32.0))
        .style(move |_| iced::widget::container::Style {
            background: Some(team.into()),
            border: iced::Border {
                radius: 2.0.into(),
                ..iced::Border::default()
            },
            ..iced::widget::container::Style::default()
        });
    let body = row![
        bar,
        iced::widget::container(at).width(Length::Fixed(72.0)),
        text(symbol.0)
            .size(SIZE_BODY)
            .font(bold)
            .color(symbol.1)
            .shaping(text::Shaping::Advanced)
            .width(Length::Fixed(16.0)),
        column![text(name).size(SIZE_BODY).color(color), player].width(Length::Fill),
        score,
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    ui::list_row(body, chosen, Some(Message::Event(i)), c)
}

/// The events, under the title of their set.
pub fn view<'a>(
    events: &[VideoEvent],
    selected: usize,
    anchored: Option<EventRef>,
    duration_ms: Option<u64>,
    s: &'static Strings,
    c: Colors,
) -> Element<'a, Message> {
    let mut rows: Vec<Element<'a, Message>> = vec![];
    let mut set = None;
    // the score before each event, from 0–0 at each set
    let mut before = (0, 0);
    for (i, e) in events.iter().enumerate() {
        if set != Some(e.set_number) {
            set = Some(e.set_number);
            before = (0, 0);
            rows.push(ui::section(&format!("{} {}", s.set_n, e.set_number), c));
        }
        let is_anchored = anchored
            .is_some_and(|a| a.set_number == e.set_number && a.index == e.event.index as usize);
        let mark = Mark::of(e, before);
        before = (e.event.score_us, e.event.score_them);
        rows.push(row_of(
            e,
            i,
            mark,
            i == selected,
            is_anchored,
            duration_ms,
            s,
            c,
        ));
    }
    scrollable(Column::with_children(rows).spacing(4).padding([0, 12]))
        .id(Id::new(LIST))
        .height(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_of_an_event_in_the_video() {
        assert_eq!(time(None, Some(1000)), "–");
        assert_eq!(time(Some(378_000), Some(3_708_650)), "6:18");
        assert_eq!(time(Some(-2000), Some(3_708_650)), "<0:02");
        assert_eq!(time(Some(5_686_000), Some(3_708_650)), ">1:01:48");
        assert!(inside(Some(0), Some(10)));
        assert!(!inside(Some(11), Some(10)));
        assert_eq!(seconds(3000), "+3.0 s");
        assert_eq!(seconds(-9040), "−9.0 s");
        assert_eq!(seconds(9582), "+9.6 s");
        assert_eq!(seconds(-500), "−0.5 s");
        assert!(!inside(Some(-1), None));
        assert!(!inside(None, None));
    }
}
