use async_trait::async_trait;
use crokey::{
    KeyCombination, KeyCombinationFormat,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
};
use ratatui::{Frame, layout::Rect};

use crate::shapes::{keybinding::KeyBindings, symbol::KeyCombinationFormatExt};
use scout4all_core::{localization::current_labels, shapes::enums::WithDesc};

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
        .filter(|action| !matches!(action, Sba::Hidden(_)))
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
            Sba::Hidden(_) => None,
        })
        .map_while(|f| f)
        .collect()
}

/// How a key moves between the fields of a form: up and shift tab to the
/// previous one, down and tab to the next one. Left and right change the
/// value of the field instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKey {
    Previous,
    Next,
    Change { back: bool },
}

impl FieldKey {
    pub fn of(code: KeyCode) -> Option<FieldKey> {
        match code {
            KeyCode::Up | KeyCode::BackTab => Some(FieldKey::Previous),
            KeyCode::Down | KeyCode::Tab => Some(FieldKey::Next),
            KeyCode::Left => Some(FieldKey::Change { back: true }),
            KeyCode::Right => Some(FieldKey::Change { back: false }),
            _ => None,
        }
    }
}

/// The footer entries moving between the fields of a form, both keys of
/// each direction listed.
pub fn field_navigation_entries() -> Vec<(String, String)> {
    let format = KeyCombinationFormat::default();
    let fmt = KeyCombinationFormatExt::new(&format);
    let shift_tab = KeyCombination::one_key(KeyCode::Tab, KeyModifiers::SHIFT);
    vec![
        (
            format!(
                "{}/{}",
                fmt.to_string(KeyCode::Up),
                fmt.to_string(shift_tab)
            ),
            current_labels().previous.to_string(),
        ),
        (
            format!(
                "{}/{}",
                fmt.to_string(KeyCode::Down),
                fmt.to_string(KeyCode::Tab)
            ),
            current_labels().next.to_string(),
        ),
    ]
}

/// The footer entry changing the value of a field.
pub fn change_value_entry() -> (String, String) {
    let format = KeyCombinationFormat::default();
    let fmt = KeyCombinationFormatExt::new(&format);
    (
        format!(
            "{}/{}",
            fmt.to_string(KeyCode::Left),
            fmt.to_string(KeyCode::Right)
        ),
        current_labels().change.to_string(),
    )
}

#[derive(Clone)]
pub enum Sba<T> {
    Simple(T),
    Redacted(T, fn(String) -> String),
    /// bound to its key combinations, but not listed in the footer
    Hidden(T),
}
impl<T> Sba<T> {
    pub fn key(&self) -> &T {
        match self {
            Sba::Simple(ae) => ae,
            Sba::Redacted(ae, _) => ae,
            Sba::Hidden(ae) => ae,
        }
    }
    pub fn keys(slice: &[Sba<T>]) -> Vec<&T> {
        slice.iter().map(|f| f.key()).collect()
    }
}

#[test]
fn test_get_keybinding_actions() {
    use crate::shapes::actions::ScreenActionEnum;
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

#[test]
fn test_hidden_actions_are_bound_but_not_listed() {
    use crate::shapes::actions::ScreenActionEnum;
    use crokey::key;
    let kb = KeyBindings::<ScreenActionEnum>::default();
    let actions = &[
        Sba::Simple(ScreenActionEnum::Up),
        Sba::Hidden(ScreenActionEnum::Next),
        Sba::Simple(ScreenActionEnum::Quit),
    ];

    let footer = get_keybinding_actions(&kb, actions);
    assert_eq!(footer.len(), 2);
    assert_eq!(footer[0].1, "previous");
    assert_eq!(footer[1].1, "quit");

    let bindings = kb.slice(Sba::keys(actions));
    assert_eq!(bindings.get(key!(tab)), Some(&ScreenActionEnum::Next));
    assert_eq!(bindings.get(key!(up)), Some(&ScreenActionEnum::Up));
}
