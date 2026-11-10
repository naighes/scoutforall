//! The strip of the clips of the actions picked, under the bar of the
//! video: each clip where it is in the video, red, the one playing in the
//! accent and taller. A click on a clip goes to its start (the margin
//! before it) and makes it the one playing; a click between the clips goes
//! to that point of the video. Drawn with the canvas of iced, to know where
//! the click is across it.

//! Where the clips are drawn and what a click asks for: in the mobile crate
//! (`scout4all_mobile::strip`), the same on Android.

use {
    crate::theme::Colors,
    iced::{
        Color, Point, Rectangle, Renderer, Size, Theme, mouse,
        widget::canvas::{self, Action, Event, Frame, Geometry, Path},
    },
    scout4all_mobile::strip::{
        ClipSpan, MOUSE_REACH, StripBar, StripClick, strip_bars, strip_clicked,
    },
};

/// How tall the strip is (the room for the click), a clip and the one
/// playing.
pub const HEIGHT: f32 = 14.0;
const CLIP_HEIGHT: f32 = 6.0;
const PLAYING_HEIGHT: f32 = 10.0;

/// The strip, ready to draw and to be clicked.
pub struct Strip {
    pub spans: Vec<ClipSpan>,
    pub duration_ms: u64,
    pub colors: Colors,
}

impl canvas::Program<StripClick> for Strip {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<StripClick>> {
        match (event, cursor.position_in(bounds)) {
            (Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)), Some(at)) => Some(
                Action::publish(strip_clicked(
                    &self.spans,
                    self.duration_ms,
                    bounds.width,
                    at.x,
                    MOUSE_REACH,
                ))
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
        let bars = strip_bars(&self.spans, self.duration_ms, bounds.width);
        // the one playing drawn last, over the others
        let mut drawn: Vec<_> = bars.into_iter().zip(&self.spans).collect();
        drawn.sort_by_key(|(_, sp)| sp.playing);
        for (StripBar { from, to }, sp) in drawn {
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
