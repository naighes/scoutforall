use crate::screens::components::notify_banner::NotifyBanner;
use ratatui::{layout::Rect, Frame};

#[derive(Debug)]
pub struct NotifyDialogue<T> {
    pub banner: NotifyBanner,
    pub entry: Option<T>,
}

impl<T> NotifyDialogue<T> {
    pub fn new() -> Self {
        Self {
            banner: NotifyBanner::new(),
            entry: None,
        }
    }

    pub fn set(&mut self, entry: T) -> &mut Self {
        self.entry = Some(entry);
        self
    }

    pub fn has_value(&self) -> bool {
        self.entry.is_some()
    }

    pub fn reset(&mut self) {
        self.banner.reset();
        self.entry = None;
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        if !self.banner.has_value() {
            return;
        }
        self.banner.render(f, area);
    }
}
