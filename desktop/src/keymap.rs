//! The keys of iced as the TUI reads them: the same combinations as in
//! `config.json` (see the `keys` crate), so the scout keeps their keys.

use {
    crokey::crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    iced::keyboard::{Key, Modifiers, key::Named},
    scout4all_keys::{KeyCombination, combination_of},
};

use crate::strings::Strings;

/// The combination of a key pressed, none for the keys scouting doesn't
/// use (e.g. shift alone). `key` has shift applied (iced's
/// `modified_key`): "L" for shift-l, "+" for the symbol, as a terminal
/// reports them.
pub fn combination(key: &Key, modifiers: Modifiers) -> Option<KeyCombination> {
    // ⌘ on macOS is ⌃ elsewhere, to undo and do again in a form; the other
    // shortcuts of ⌘ are the system's
    match (modifiers.macos_command(), key) {
        (true, Key::Character(c)) if ["z", "Z", "y"].contains(&c.as_str()) => {}
        (true, _) => return None,
        (false, _) => {}
    }
    let code = match key {
        Key::Named(Named::Escape) => Some(KeyCode::Esc),
        Key::Named(Named::Tab) => Some(KeyCode::Tab),
        Key::Named(Named::Space) => Some(KeyCode::Char(' ')),
        Key::Named(Named::Enter) => Some(KeyCode::Enter),
        Key::Named(Named::Backspace) => Some(KeyCode::Backspace),
        Key::Named(Named::Delete) => Some(KeyCode::Delete),
        Key::Named(Named::ArrowLeft) => Some(KeyCode::Left),
        Key::Named(Named::ArrowRight) => Some(KeyCode::Right),
        Key::Named(Named::ArrowUp) => Some(KeyCode::Up),
        Key::Named(Named::ArrowDown) => Some(KeyCode::Down),
        Key::Named(Named::PageUp) => Some(KeyCode::PageUp),
        Key::Named(Named::PageDown) => Some(KeyCode::PageDown),
        Key::Named(Named::F1) => Some(KeyCode::F(1)),
        Key::Named(Named::F2) => Some(KeyCode::F(2)),
        Key::Named(Named::F3) => Some(KeyCode::F(3)),
        Key::Named(Named::F4) => Some(KeyCode::F(4)),
        Key::Character(s) => s.chars().next().map(KeyCode::Char),
        _ => None,
    }?;
    let mut mods = KeyModifiers::NONE;
    mods.set(KeyModifiers::SHIFT, modifiers.shift());
    mods.set(
        KeyModifiers::CONTROL,
        modifiers.control() || modifiers.macos_command(),
    );
    mods.set(KeyModifiers::ALT, modifiers.alt());
    Some(combination_of(KeyEvent::new(code, mods)))
}

/// The digit of the key, alone: the players are chosen by it.
pub fn digit(ck: &KeyCombination) -> Option<u8> {
    ck.as_letter()
        .and_then(|c| c.to_digit(10))
        .and_then(|d| u8::try_from(d).ok())
}

/// How the scout reads the key, on a keycap: the modifiers as symbols
/// ("⇧E" for shift-e, the letter in capitals as on the keyboard), the
/// named keys as the hints write them (← ⌫ ⇥ Enter Esc), the space in
/// the language.
pub fn label(ck: &KeyCombination, s: &Strings) -> String {
    let mods = [
        (KeyModifiers::CONTROL, "⌃"),
        (KeyModifiers::ALT, "⌥"),
        (KeyModifiers::SHIFT, "⇧"),
    ];
    // a lone shift-tab (crossterm's BackTab carries the shift): "⇧⇥"
    let modified = ck.modifiers != KeyModifiers::NONE;
    let mut label: String = mods
        .iter()
        .filter(|(m, _)| ck.modifiers.contains(*m))
        .map(|(_, symbol)| *symbol)
        .collect();
    let keys: Vec<String> = ck
        .codes
        .iter()
        .map(|code| match code {
            KeyCode::Char(' ') => s.space.to_string(),
            KeyCode::Char(c) if modified => c.to_uppercase().to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Tab => "⇥".to_string(),
            KeyCode::BackTab if !ck.modifiers.contains(KeyModifiers::SHIFT) => "⇧⇥".to_string(),
            KeyCode::BackTab => "⇥".to_string(),
            KeyCode::Backspace => "⌫".to_string(),
            KeyCode::Delete => "⌦".to_string(),
            KeyCode::Left => "←".to_string(),
            KeyCode::Right => "→".to_string(),
            KeyCode::Up => "↑".to_string(),
            KeyCode::Down => "↓".to_string(),
            KeyCode::PageUp => "PgUp".to_string(),
            KeyCode::PageDown => "PgDn".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::Insert => "Ins".to_string(),
            KeyCode::F(n) => format!("F{n}"),
            other => format!("{other:?}"),
        })
        .collect();
    label.push_str(&keys.join("+"));
    label
}

#[cfg(test)]
mod tests {
    use {super::*, scout4all_keys::key};

    fn ch(s: &str) -> Key {
        Key::Character(s.into())
    }

    #[test]
    fn letters_and_symbols_as_the_tui() {
        assert_eq!(combination(&ch("a"), Modifiers::empty()), Some(key!(a)));
        // shift-l: the TUI binds "Shift-l"
        assert_eq!(
            combination(&ch("L"), Modifiers::SHIFT),
            Some(key!(shift - l))
        );
        assert_eq!(
            combination(&ch("U"), Modifiers::SHIFT),
            Some(scout4all_keys::UNDO_KEY)
        );
        // "+" with shift on many layouts: the symbol alone
        assert_eq!(combination(&ch("+"), Modifiers::SHIFT), Some(key!('+')));
        assert_eq!(combination(&ch("-"), Modifiers::empty()), Some(key!('-')));
        assert_eq!(
            combination(&Key::Named(Named::Escape), Modifiers::empty()),
            Some(scout4all_keys::BACK_KEY)
        );
        assert_eq!(
            combination(&Key::Named(Named::Space), Modifiers::empty()),
            Some(scout4all_keys::ZONE_KEY)
        );
        assert_eq!(digit(&key!('3')), Some(3));
        assert_eq!(digit(&key!(a)), None);
    }

    #[test]
    fn labels_as_on_the_keycaps() {
        use scout4all_core::shapes::enums::LanguageEnum::{En, It, Ja};
        let it = crate::strings::strings(It);
        assert_eq!(label(&key!('-'), it), "-");
        assert_eq!(label(&key!(shift - e), it), "⇧E");
        assert_eq!(label(&key!(ctrl - shift - z), it), "⌃⇧Z");
        assert_eq!(label(&key!(a), it), "a");
        assert_eq!(label(&key!('.'), it), ".");
        assert_eq!(label(&key!(space), it), "spazio");
        assert_eq!(label(&key!(space), crate::strings::strings(En)), "space");
        assert_eq!(label(&key!(space), crate::strings::strings(Ja)), "スペース");
        assert_eq!(label(&key!(tab), it), "⇥");
        assert_eq!(label(&key!(shift - backtab), it), "⇧⇥");
        assert_eq!(label(&key!(enter), it), "Enter");
        assert_eq!(label(&key!(esc), it), "Esc");
        assert_eq!(label(&key!(backspace), it), "⌫");
        assert_eq!(label(&key!(left), it), "←");
        assert_eq!(label(&key!(f2), it), "F2");
    }

    #[test]
    fn undo_and_redo_with_the_command_of_the_system() {
        let command = match cfg!(target_os = "macos") {
            true => Modifiers::LOGO,
            false => Modifiers::CTRL,
        };
        assert_eq!(combination(&ch("z"), command), Some(key!(ctrl - z)));
        let redo = combination(&ch("Z"), command | Modifiers::SHIFT);
        assert!(redo.is_some_and(crate::form::is_redo));
        assert_eq!(combination(&ch("y"), command), Some(key!(ctrl - y)));
        // ⌘Q, ⌘W: the system's
        assert_eq!(
            combination(&ch("q"), Modifiers::LOGO).is_none(),
            cfg!(target_os = "macos")
        );
    }
}
