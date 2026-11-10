//! The strip of the clips of the actions picked, under the bar of the
//! video: each clip where it is in the video, red, the one playing in the
//! accent and taller. A click on a clip goes to its start (the margin
//! before it) and makes it the one playing; a click between the clips goes
//! to that point of the video. Drawn with the canvas of iced, to know where
//! the click is across it.

use {
    crate::theme::Colors,
    iced::{
        Color, Point, Rectangle, Renderer, Size, Theme, mouse,
        widget::canvas::{self, Action, Event, Frame, Geometry, Path},
    },
};

/// The narrowest a clip is drawn: a short one, on a narrow strip, still
/// seen.
const MIN_WIDTH: f32 = 3.0;

/// How far from a clip a click still takes it: a short one is easy to hit.
const REACH: f32 = 4.0;

/// How tall the strip is (the room for the click), a clip and the one
/// playing.
pub const HEIGHT: f32 = 14.0;
const CLIP_HEIGHT: f32 = 6.0;
const PLAYING_HEIGHT: f32 = 10.0;

/// A clip of the actions picked, in the video: the ones overlapping as
/// one, from the first action in it (`event`, in the list).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub from: u64,
    pub to: u64,
    pub playing: bool,
    pub event: usize,
}

/// What a click on the strip asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// the clip of this action (in the list), from its start
    Clip(usize),
    /// the video at this point
    At(u64),
}

/// Where each clip is drawn across a strip `width` wide, from its start
/// to its end: at least `MIN_WIDTH`, inside the strip even at the end of
/// the video (or past it).
pub fn bars(spans: &[Span], duration_ms: u64, width: f32) -> Vec<(f32, f32)> {
    let x = |ms: u64| {
        (ms.min(duration_ms) as f64 / duration_ms.max(1) as f64 * f64::from(width)) as f32
    };
    let last = (width - MIN_WIDTH).max(0.0);
    spans
        .iter()
        .map(|sp| {
            let from = x(sp.from).min(last);
            (from, x(sp.to).max(from + MIN_WIDTH))
        })
        .collect()
}

/// What the click at `x`, across a strip `width` wide, asks for: the clip
/// nearest within `REACH`, otherwise the video at that point.
pub fn clicked(spans: &[Span], duration_ms: u64, width: f32, x: f32) -> Click {
    let distance = |(from, to): (f32, f32)| (from - x).max(x - to).max(0.0);
    let near = bars(spans, duration_ms, width)
        .into_iter()
        .zip(spans)
        .map(|(bar, sp)| (distance(bar), sp.event))
        .filter(|(d, _)| *d <= REACH)
        .min_by(|a, b| a.0.total_cmp(&b.0));
    match near {
        Some((_, event)) => Click::Clip(event),
        None => {
            let share = f64::from((x / width.max(1.0)).clamp(0.0, 1.0));
            Click::At((share * duration_ms as f64).round() as u64)
        }
    }
}

/// The strip, ready to draw and to be clicked.
pub struct Strip {
    pub spans: Vec<Span>,
    pub duration_ms: u64,
    pub colors: Colors,
}

impl canvas::Program<Click> for Strip {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Click>> {
        match (event, cursor.position_in(bounds)) {
            (Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), Some(at)) => Some(
                Action::publish(clicked(&self.spans, self.duration_ms, bounds.width, at.x))
                    .and_capture(),
            ),
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        _state: &(),
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match cursor.position_in(bounds) {
            Some(_) => mouse::Interaction::Pointer,
            None => mouse::Interaction::default(),
        }
    }

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let c = self.colors;
        // the line of the video, faint, for the clicks between the clips
        frame.fill_rectangle(
            Point::new(0.0, (bounds.height - 2.0) / 2.0),
            Size::new(bounds.width, 2.0),
            Color { a: 0.25, ..c.muted },
        );
        let bars = bars(&self.spans, self.duration_ms, bounds.width);
        // the one playing drawn last, over the others
        let mut drawn: Vec<_> = bars.into_iter().zip(&self.spans).collect();
        drawn.sort_by_key(|(_, sp)| sp.playing);
        for ((from, to), sp) in drawn {
            let (height, color) = match sp.playing {
                true => (PLAYING_HEIGHT, c.accent),
                false => (CLIP_HEIGHT, c.danger),
            };
            frame.fill(
                &Path::rounded_rectangle(
                    Point::new(from, (bounds.height - height) / 2.0),
                    Size::new(to - from, height),
                    2.0.into(),
                ),
                color,
            );
        }
        vec![frame.into_geometry()]
    }
}
