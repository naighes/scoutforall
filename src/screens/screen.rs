use async_trait::async_trait;
use crokey::crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

use crate::shapes::{enums::WithDesc, keybinding::KeyBindings, symbol::KeyCombinationFormatExt};

pub enum AppAction {
    None,
    SwitchScreen(Box<dyn ScreenAsync + Send + Sync>),
    Back(bool, Option<u8>), // the boolean value indicates if the previous screen needs to be refreshed
    Quit(std::io::Result<()>),
}

pub trait Renderable {
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect);
}

#[async_trait]
pub trait ScreenAsync: Renderable + Send + Sync {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction;
    async fn refresh_data(&mut self);
}

pub fn get_keybinding_actions<T>(kb: &KeyBindings<T>, actions: &[Sba<T>]) -> Vec<(String, String)>
where
    T: std::hash::Hash + Eq + Clone + WithDesc<T>,
{
    use crokey::KeyCombinationFormat;
    let fmt = &KeyCombinationFormat::default();
    actions
        .iter()
        .map(|action| match action {
            Sba::Simple(ae) => {
                if let Some((key_combination, description)) = kb.shortest_key_for(ae) {
                    Some((
                        KeyCombinationFormatExt::new(fmt).to_string(key_combination),
                        description,
                    ))
                } else {
                    None
                }
            }
            Sba::Redacted(ae, description_fn) => {
                fn map_screen_action<T: std::hash::Hash + Eq + Clone + WithDesc<T>>(
                    kb: &KeyBindings<T>,
                    action: &T,
                    description_fn: fn(String) -> String,
                    fmt: &KeyCombinationFormat,
                ) -> Option<(String, String)> {
                    if let Some((key_combination, description)) = kb.shortest_key_for(action) {
                        Some((
                            KeyCombinationFormatExt::new(fmt).to_string(key_combination),
                            description_fn(description),
                        ))
                    } else {
                        None
                    }
                }
                map_screen_action(kb, ae, *description_fn, fmt)
            }
        })
        .take_while(|f| f.is_some())
        .map(|f| f.unwrap())
        .collect()
}

#[derive(Clone)]
pub enum Sba<T> {
    Simple(T),
    Redacted(T, fn(String) -> String),
}
impl<T> Sba<T> {
    pub fn key(&self) -> &T {
        match self {
            Sba::Simple(ae) => ae,
            Sba::Redacted(ae, _) => ae,
        }
    }
    pub fn keys(slice: &[Sba<T>]) -> Vec<&T> {
        slice.iter().map(|f| f.key()).collect()
    }
}

#[test]
fn test_get_keybinding_actions() {
    use crate::shapes::enums::ScreenActionEnum;
    // Setup test data
    let kb = KeyBindings::<ScreenActionEnum>::default();
    let actions = &[
        Sba::Simple(ScreenActionEnum::Next),
        Sba::Simple(ScreenActionEnum::Previous),
        Sba::Redacted(ScreenActionEnum::Import, |lbl| -> String {
            lbl.replace("{}", "pizza")
        }),
    ];

    // Call the function under test
    let result = get_keybinding_actions(&kb, actions);

    // Assert the expected outcome
    assert_eq!(result.len(), 3);
    assert_eq!(result[0], ("⇥".to_string(), "next".to_string()));
    assert_eq!(result[1], ("⬅".to_string(), "previous".to_string()));
    assert_eq!(result[2], ("i".to_string(), "import pizza".to_string()));
}
