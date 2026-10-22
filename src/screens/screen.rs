use async_trait::async_trait;
use crokey::{
    KeyCombination, KeyCombinationFormat,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
};
use ratatui::{Frame, layout::Rect};

use crate::localization::{Describe, current_labels};
use crate::shapes::{
    actions::ScreenActionEnum, keybinding::KeyBindings, symbol::KeyCombinationFormatExt,
};

pub enum AppAction {
    None,
    SwitchScreen(Box<dyn ScreenAsync + Send + Sync>),
    Back(bool, Option<u8>), // the boolean value indicates if the previous screen needs to be refreshed
    Quit(std::io::Result<()>),
}

pub trait Renderable {
    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect);

    /// The keys the footer lists, `(key, description)`: the app makes the
    /// footer tall enough to list them all before rendering the screen.
    fn footer_entries(&self) -> Vec<(String, String)>;
}

#[async_trait]
pub trait ScreenAsync: Renderable + Send + Sync {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction;
    async fn refresh_data(&mut self);
}

pub fn get_keybinding_actions<T>(kb: &KeyBindings<T>, actions: &[Sba<T>]) -> Vec<(String, String)>
where
    T: std::hash::Hash + Eq + Clone + Describe,
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
                fn map_screen_action<T: std::hash::Hash + Eq + Clone + Describe>(
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

/// A footer entry listing every key of an action, e.g. "⬆/⇧-⇥".
fn keys_entry(keys: &[KeyCombination], description: &str) -> (String, String) {
    let format = KeyCombinationFormat::default();
    let fmt = KeyCombinationFormatExt::new(&format);
    (
        keys.iter()
            .map(|k| fmt.to_string(*k))
            .collect::<Vec<_>>()
            .join("/"),
        description.to_string(),
    )
}

fn shift_tab() -> KeyCombination {
    KeyCombination::one_key(KeyCode::Tab, KeyModifiers::SHIFT)
}

/// The footer entries moving between the fields of a form or the rows of a
/// list, both keys of each direction listed: up and shift tab to the
/// previous one, down and tab to the next one.
pub fn navigation_entries() -> Vec<(String, String)> {
    let labels = current_labels();
    vec![
        keys_entry(&[KeyCode::Up.into(), shift_tab()], labels.previous),
        keys_entry(&[KeyCode::Down.into(), KeyCode::Tab.into()], labels.next),
    ]
}

/// The footer entries moving between panels laid side by side, whose rows
/// the up and down arrows move through instead: left and shift tab to the
/// previous one, right and tab to the next one.
pub fn panel_navigation_entries() -> Vec<(String, String)> {
    let labels = current_labels();
    vec![
        keys_entry(&[KeyCode::Left.into(), shift_tab()], labels.previous),
        keys_entry(&[KeyCode::Right.into(), KeyCode::Tab.into()], labels.next),
    ]
}

/// The footer entry changing the value of a field.
pub fn change_value_entry() -> (String, String) {
    keys_entry(
        &[KeyCode::Left.into(), KeyCode::Right.into()],
        current_labels().change,
    )
}

/// The actions moving through the rows of a list: up and shift tab (left
/// too) to the previous one, down and tab (right too) to the next one. Not
/// listed one by one: `list_entries` lists them as `navigation_entries`.
pub const LIST_NAVIGATION: [Sba<ScreenActionEnum>; 4] = [
    Sba::Hidden(ScreenActionEnum::Up),
    Sba::Hidden(ScreenActionEnum::Down),
    Sba::Hidden(ScreenActionEnum::Previous),
    Sba::Hidden(ScreenActionEnum::Next),
];

/// The footer of a list: the keys moving through its rows when it has any
/// (its actions include `LIST_NAVIGATION`), then the ones of the actions.
pub fn list_entries(
    kb: &KeyBindings<ScreenActionEnum>,
    actions: &[Sba<ScreenActionEnum>],
) -> Vec<(String, String)> {
    let navigation = match actions.iter().any(|a| *a.key() == ScreenActionEnum::Up) {
        true => navigation_entries(),
        false => vec![],
    };
    navigation
        .into_iter()
        .chain(get_keybinding_actions(kb, actions))
        .collect()
}

/// The row after (`forward`) or before the selected one in a list of
/// `count` rows, stopping at the first and the last; `None` with no rows.
pub fn step(selected: usize, count: usize, forward: bool) -> Option<usize> {
    match (count, forward) {
        (0, _) => None,
        (count, true) => Some((selected + 1).min(count - 1)),
        (_, false) => Some(selected.saturating_sub(1)),
    }
}

/// The row after (`forward`) or before the selected one in a list of
/// `count` rows, going round; `None` with no rows.
pub fn cycle(selected: usize, count: usize, forward: bool) -> Option<usize> {
    match (count, forward) {
        (0, _) => None,
        (count, true) => Some((selected + 1) % count),
        (count, false) => Some((selected % count + count - 1) % count),
    }
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
fn moving_through_a_list() {
    // stopping at the ends
    assert_eq!(step(0, 3, true), Some(1));
    assert_eq!(step(2, 3, true), Some(2));
    assert_eq!(step(0, 3, false), Some(0));
    assert_eq!(step(2, 3, false), Some(1));
    // going round
    assert_eq!(cycle(2, 3, true), Some(0));
    assert_eq!(cycle(0, 3, false), Some(2));
    assert_eq!(cycle(1, 3, false), Some(0));
    // a selection past the end, the rows having gone
    assert_eq!(step(5, 3, true), Some(2));
    assert_eq!(cycle(5, 3, false), Some(1));
    // no rows: no selection, and no panic
    for forward in [true, false] {
        assert_eq!(step(0, 0, forward), None);
        assert_eq!(cycle(0, 0, forward), None);
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
