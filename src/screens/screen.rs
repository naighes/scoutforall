use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

pub enum AppAction {
    None,
    SwitchScreen(Box<dyn ScreenAsync + Send + Sync>),
    Back(bool, Option<u8>), // the boolean value indicates if the previous screen needs to be refreshed
}

pub trait Renderable {
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect);
}

#[async_trait]
pub trait ScreenAsync: Renderable + Send + Sync {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction;
    async fn refresh_data(&mut self);
}
