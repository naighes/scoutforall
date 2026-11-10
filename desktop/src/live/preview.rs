//! The pictures of a source as they come, shown: on the screen of the
//! source, and in a window of their own while the recording goes on
//! behind the other screens. Each new frame made a handle, given to the
//! renderer (`allocate`) before it's shown: no blank between two.

use {
    super::capture::Status,
    crate::{theme::Colors, ui, video::Picture},
    iced::{
        ContentFit, Element, Length,
        advanced::image::Allocation,
        widget::{container, image},
    },
    scout4all_keys::{KeyCombination, PREVIEW_KEY},
    std::sync::Arc,
};

/// How often the window of the recording looks for a new frame: enough to
/// see the play, little work for the screen being scouted.
pub const WINDOW_EVERY_MS: u64 = 100;

struct Shown {
    frame: Arc<Picture>,
    handle: image::Handle,
    _allocation: Option<Allocation>,
}

#[derive(Default)]
pub struct Preview {
    shown: Option<Shown>,
    /// the last frame asked for, by its serial
    requested: u64,
}

impl Preview {
    /// The frame of `status` newer than the last asked for: its handle to
    /// allocate, then `allocated`.
    pub fn wanted(&mut self, status: Option<&Status>) -> Option<(Arc<Picture>, image::Handle)> {
        match status.and_then(|s| s.frame.clone()) {
            Some(frame) if frame.serial > self.requested => {
                self.requested = frame.serial;
                let handle =
                    image::Handle::from_rgba(frame.width, frame.height, frame.rgba.clone());
                Some((frame, handle))
            }
            _ => None,
        }
    }

    /// A frame ready: shown, unless a newer one already is.
    pub fn allocated(
        &mut self,
        frame: Arc<Picture>,
        handle: image::Handle,
        allocation: Option<Allocation>,
    ) {
        let newer = self
            .shown
            .as_ref()
            .is_none_or(|shown| shown.frame.serial < frame.serial);
        if newer {
            self.shown = Some(Shown {
                frame,
                handle,
                _allocation: allocation,
            });
        }
    }

    /// Nothing shown, nothing asked for: for another source.
    pub fn clear(&mut self) {
        *self = Preview::default();
    }

    pub fn is_shown(&self) -> bool {
        self.shown.is_some()
    }

    /// The serial of the frame shown.
    #[cfg(test)]
    pub fn serial(&self) -> Option<u64> {
        self.shown.as_ref().map(|s| s.frame.serial)
    }

    /// The picture on black, as large as it fits; until there's one,
    /// what's wrong (in red) or what's awaited.
    pub fn view<'a, M: 'a>(
        &self,
        notice: Option<String>,
        waiting: &str,
        c: Colors,
    ) -> Element<'a, M> {
        let inside: Element<'a, M> = match (&self.shown, notice) {
            (Some(shown), _) => image(shown.handle.clone())
                .content_fit(ContentFit::Contain)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            (None, Some(n)) => container(ui::body(n).color(c.danger))
                .padding(24)
                .center(Length::Fill)
                .into(),
            (None, None) => container(ui::body(waiting.to_string()).color(c.muted))
                .center(Length::Fill)
                .into(),
        };
        container(inside)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(iced::Color::BLACK.into()),
                ..container::Style::default()
            })
            .into()
    }
}

/// Whether the window of the recording is open: asked for, while the
/// recording goes on (closed as it's saved).
pub fn window_wanted(asked: bool, status: Option<&Status>) -> bool {
    asked && status.is_some_and(|s| s.recording.is_some())
}

/// Whether the window of the recording is asked for after a key: the
/// fixed one (`PREVIEW_KEY`, F4) opens and closes it while recording,
/// whatever the screen; any other key, or no recording, leaves it to the
/// screen (`None`).
pub fn asked_after_key(asked: bool, ck: &KeyCombination, status: Option<&Status>) -> Option<bool> {
    let recording = status.is_some_and(|s| s.recording.is_some());
    match (*ck == PREVIEW_KEY, recording) {
        (true, true) => Some(!asked),
        _ => None,
    }
}

/// The task giving the renderer a frame, then `done` with it.
pub fn allocate<M: Send + 'static>(
    frame: Arc<Picture>,
    handle: image::Handle,
    done: impl Fn(Arc<Picture>, image::Handle, Option<Allocation>) -> M + Send + 'static,
) -> iced::Task<M> {
    iced_runtime::image::allocate(handle.clone())
        .map(move |allocated| done(frame.clone(), handle.clone(), allocated.ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(serial: u64) -> Status {
        Status {
            frame: Some(Arc::new(Picture {
                serial,
                width: 2,
                height: 2,
                rgba: bytes::Bytes::from(vec![0; 16]),
            })),
            ..Status::default()
        }
    }

    /// Each frame asked for once, the newer only; one coming late doesn't
    /// take the place of a newer one shown; cleared, from the start.
    #[test]
    fn the_frames_shown() {
        let mut preview = Preview::default();
        assert!(preview.wanted(None).is_none());
        assert!(preview.wanted(Some(&Status::default())).is_none());
        let (first, first_handle) = preview.wanted(Some(&status(1))).unwrap();
        assert!(preview.wanted(Some(&status(1))).is_none());
        let (second, second_handle) = preview.wanted(Some(&status(2))).unwrap();
        assert!(!preview.is_shown());
        preview.allocated(second, second_handle, None);
        assert_eq!(preview.serial(), Some(2));
        preview.allocated(first, first_handle, None);
        assert_eq!(preview.serial(), Some(2));
        preview.clear();
        assert!(!preview.is_shown());
        assert!(preview.wanted(Some(&status(1))).is_some());
    }

    /// The window only while recording, and only if asked for.
    #[test]
    fn the_window_while_recording() {
        let recording = Status {
            recording: Some(super::super::capture::Recording { elapsed_ms: 1 }),
            ..Status::default()
        };
        let saving = Status {
            saving: true,
            ..Status::default()
        };
        assert!(window_wanted(true, Some(&recording)));
        assert!(!window_wanted(false, Some(&recording)));
        assert!(!window_wanted(true, Some(&saving)));
        assert!(!window_wanted(true, None));
    }

    /// F4 opens and closes the window while recording; not otherwise, nor
    /// other keys: they stay with the screen.
    #[test]
    fn the_window_from_its_key() {
        use scout4all_keys::key;
        let recording = Status {
            recording: Some(super::super::capture::Recording { elapsed_ms: 1 }),
            ..Status::default()
        };
        let f4 = key!(f4);
        assert_eq!(asked_after_key(false, &f4, Some(&recording)), Some(true));
        assert_eq!(asked_after_key(true, &f4, Some(&recording)), Some(false));
        assert_eq!(asked_after_key(false, &f4, Some(&Status::default())), None);
        assert_eq!(asked_after_key(false, &f4, None), None);
        assert_eq!(asked_after_key(false, &key!(f3), Some(&recording)), None);
    }
}
