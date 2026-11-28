//! A widget kept in sight inside its scrollable: where the keys move the
//! focus of a form taller than the window, the form scrolls just enough to
//! show the field (iced moves the focus, never the scroll).

use iced::{
    Rectangle, Task, Vector,
    advanced::widget::{
        Id, Operation,
        operation::{
            Outcome,
            scrollable::{AbsoluteOffset, Scrollable, scroll_to},
        },
    },
};

/// The space left above or below the widget brought into sight.
const MARGIN: f32 = 12.0;

/// `target` scrolled into sight in the scrollable `scroll`, if both are on
/// screen and it is out of sight; nothing otherwise.
pub fn reveal<T: Send + 'static>(scroll: Id, target: Id) -> Task<T> {
    iced_runtime::task::widget(Reveal {
        scroll,
        target,
        view: None,
        found: None,
    })
}

/// The offset that shows `target` (content coordinates, from the top of the
/// content) in a viewport `height` tall scrolled by `offset`, over a content
/// `content` tall: `None` when it is already in sight.
pub fn offset_for(target: (f32, f32), offset: f32, height: f32, content: f32) -> Option<f32> {
    let (top, size) = target;
    let bottom = top + size;
    let above = top - MARGIN < offset;
    let below = bottom + MARGIN > offset + height;
    let taller = size + 2.0 * MARGIN > height;
    let wanted = match (above || below, above || taller) {
        (false, _) => return None,
        // above, or taller than the window: its top in sight
        (true, true) => top - MARGIN,
        (true, false) => bottom + MARGIN - height,
    };
    let clamped = wanted.clamp(0.0, (content - height).max(0.0));
    match (clamped - offset).abs() < 0.5 {
        true => None,
        false => Some(clamped),
    }
}

struct Reveal {
    scroll: Id,
    target: Id,
    /// the scrollable: its bounds, the bounds of its content, how far scrolled
    view: Option<(Rectangle, Rectangle, Vector)>,
    /// the bounds of the target, unscrolled
    found: Option<Rectangle>,
}

impl<T: Send + 'static> Operation<T> for Reveal {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<T>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        content_bounds: Rectangle,
        translation: Vector,
        _state: &mut dyn Scrollable,
    ) {
        if id == Some(&self.scroll) {
            self.view = Some((bounds, content_bounds, translation));
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&self.target) {
            self.found = Some(bounds);
        }
    }

    fn finish(&self) -> Outcome<T> {
        let offset = match (self.view, self.found) {
            (Some((view, content, scrolled)), Some(target)) => offset_for(
                (target.y - content.y, target.height),
                scrolled.y,
                view.height,
                content.height,
            ),
            _ => None,
        };
        match offset {
            Some(y) => Outcome::Chain(Box::new(scroll_to(
                self.scroll.clone(),
                AbsoluteOffset {
                    x: None,
                    y: Some(y),
                },
            ))),
            None => Outcome::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_in_sight_stays_where_it_is() {
        assert_eq!(offset_for((100.0, 40.0), 0.0, 500.0, 1200.0), None);
        assert_eq!(offset_for((600.0, 40.0), 300.0, 500.0, 1200.0), None);
    }

    #[test]
    fn a_field_below_comes_up_to_the_bottom() {
        // 700..740 in a window 500 tall from 0: its bottom and the margin at
        // the bottom of the window
        assert_eq!(
            offset_for((700.0, 40.0), 0.0, 500.0, 1200.0),
            Some(740.0 + MARGIN - 500.0)
        );
    }

    #[test]
    fn a_field_above_comes_down_to_the_top() {
        assert_eq!(
            offset_for((100.0, 40.0), 400.0, 500.0, 1200.0),
            Some(100.0 - MARGIN)
        );
    }

    #[test]
    fn the_scroll_stays_inside_the_content() {
        assert_eq!(offset_for((5.0, 40.0), 300.0, 500.0, 1200.0), Some(0.0));
        assert_eq!(offset_for((1190.0, 10.0), 0.0, 500.0, 1200.0), Some(700.0));
    }

    #[test]
    fn a_field_taller_than_the_window_shows_its_top() {
        assert_eq!(
            offset_for((700.0, 600.0), 0.0, 500.0, 2000.0),
            Some(700.0 - MARGIN)
        );
    }

    /// A scrollable that only remembers where it was sent.
    #[derive(Default)]
    struct Scrolled(Option<AbsoluteOffset<Option<f32>>>);

    impl Scrollable for Scrolled {
        fn snap_to(
            &mut self,
            _: iced::advanced::widget::operation::scrollable::RelativeOffset<Option<f32>>,
        ) {
        }
        fn scroll_to(&mut self, offset: AbsoluteOffset<Option<f32>>) {
            self.0 = Some(offset);
        }
        fn scroll_by(&mut self, _: AbsoluteOffset, _: Rectangle, _: Rectangle) {}
    }

    /// The operation walked over a scrollable (window 500 tall at y 60, not
    /// scrolled) with the target at `target_y` of the screen, 40 tall.
    fn walked(scroll: &str, target: &str, target_y: f32) -> Option<AbsoluteOffset<Option<f32>>> {
        let mut op = Reveal {
            scroll: Id::from("form-scroll"),
            target: Id::from("form-row"),
            view: None,
            found: None,
        };
        let view = Rectangle::new((0.0, 60.0).into(), (800.0, 500.0).into());
        let content = Rectangle::new((0.0, 60.0).into(), (800.0, 1400.0).into());
        let mut state = Scrolled::default();
        Operation::<()>::scrollable(
            &mut op,
            Some(&Id::from(scroll.to_string())),
            view,
            content,
            Vector::new(0.0, 0.0),
            &mut state,
        );
        let row = Rectangle::new((0.0, target_y).into(), (800.0, 40.0).into());
        Operation::<()>::container(&mut op, Some(&Id::from(target.to_string())), row);
        match Operation::<()>::finish(&op) {
            Outcome::Chain(mut next) => {
                next.scrollable(
                    Some(&Id::from(scroll.to_string())),
                    view,
                    content,
                    Vector::new(0.0, 0.0),
                    &mut state,
                );
                state.0
            }
            _ => None,
        }
    }

    #[test]
    fn the_scrollable_of_the_form_is_sent_to_the_field() {
        // the row at 860 of the screen = 800 of the content
        assert_eq!(
            walked("form-scroll", "form-row", 860.0),
            Some(AbsoluteOffset {
                x: None,
                y: Some(840.0 + MARGIN - 500.0)
            })
        );
    }

    #[test]
    fn other_scrollables_and_containers_are_left_alone() {
        assert_eq!(walked("other", "form-row", 860.0), None);
        assert_eq!(walked("form-scroll", "other", 860.0), None);
        assert_eq!(walked("form-scroll", "form-row", 200.0), None);
    }
}
