use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

pub enum AppAction {
    None,
    SwitchScreen(Box<dyn Screen>),
    Back(bool, Option<u8>), // the boolean value indicates if the previous screen needs to be refreshed
}

pub trait Screen {
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect);
    fn handle_key(&mut self, key: KeyEvent) -> AppAction;
    fn on_resume(&mut self, refresh: bool);
}
