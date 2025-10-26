use async_trait::async_trait;
use crokey::crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

use crate::shapes::{
    enums::ScreenActionEnum, keybinding::KeyBindings, symbol::KeyCombinationFormatExt,
};

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

pub fn get_keybinding_actions(kb: &KeyBindings, actions: Sba) -> Vec<(String, String)> {
    use crate::shapes::enums::ScreenActionEnum;
    use crokey::KeyCombinationFormat;
    let fmt = &KeyCombinationFormat::default();
    match actions {
        Sba::ScreenActions(actions) => actions
            .iter()
            .flat_map(|action| kb.shortest_key_for(action))
            .map(|x| (KeyCombinationFormatExt::new(fmt).to_string(x.0), x.1))
            .collect(),
        Sba::MappedAction(action) => {
            fn map_screen_action(
                kb: &KeyBindings,
                action: &(&ScreenActionEnum, Option<fn(String) -> String>),
                fmt: KeyCombinationFormat,
            ) -> Option<(String, String)> {
                let screen_action = kb.shortest_key_for(action.0);
                let description_fn = action.1;
                if let Some(found) = screen_action {
                    if let Some(description_fn) = description_fn {
                        Some((
                            KeyCombinationFormatExt::new(&fmt).to_string(found.0),
                            description_fn(found.1),
                        ))
                    } else {
                        Some((
                            KeyCombinationFormatExt::new(&fmt).to_string(found.0),
                            found.1,
                        ))
                    }
                } else {
                    None
                }
            }

            action
                .iter()
                .map_while(|action| map_screen_action(kb, action, fmt.clone()))
                .collect()
        }
    }
}

pub enum Sba<'a> {
    ScreenActions(&'a Vec<&'a ScreenActionEnum>),
    MappedAction(&'a Vec<(&'a ScreenActionEnum, Option<fn(String) -> String>)>),
}

#[test]
fn test_get_keybinding_actions() {
    // Setup test data
    let kb = KeyBindings::default();
    let actions = vec![&ScreenActionEnum::Next, &ScreenActionEnum::Previous];
    let sba = Sba::ScreenActions(&actions);

    // Call the function under test
    let result = get_keybinding_actions(&kb, sba);

    // Assert the expected outcome
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], ("⇥".to_string(), "next".to_string()));
    assert_eq!(result[1], ("⬅".to_string(), "previous".to_string()));
}
