//! The keys of iced as the TUI reads them: the same combinations as in
//! `config.json` (see the `keys` crate), so the scout keeps their keys.

use {
    crokey::crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    iced::keyboard::{Key, Modifiers, key::Named},
    scout4all_keys::{KeyCombination, combination_of},
};

/// The combination of a key pressed, none for the keys scouting doesn't
/// use (e.g. shift alone). `key` has shift applied (iced's
/// `modified_key`): "L" for shift-l, "+" for the symbol, as a terminal
/// reports them.
pub fn combination(key: &Key, modifiers: Modifiers) -> Option<KeyCombination> {
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
        Key::Named(Named::F1) => Some(KeyCode::F(1)),
        Key::Named(Named::F2) => Some(KeyCode::F(2)),
        Key::Named(Named::F3) => Some(KeyCode::F(3)),
        Key::Named(Named::F4) => Some(KeyCode::F(4)),
        Key::Character(s) => s.chars().next().map(KeyCode::Char),
        _ => None,
    }?;
    let mut mods = KeyModifiers::NONE;
    mods.set(KeyModifiers::SHIFT, modifiers.shift());
    mods.set(KeyModifiers::CONTROL, modifiers.control());
    mods.set(KeyModifiers::ALT, modifiers.alt());
    Some(combination_of(KeyEvent::new(code, mods)))
}

/// The digit of the key, alone: the players are chosen by it.
pub fn digit(ck: &KeyCombination) -> Option<u8> {
    ck.as_letter()
        .and_then(|c| c.to_digit(10))
        .and_then(|d| u8::try_from(d).ok())
}

/// How the scout reads the key, as the TUI writes it.
pub fn label(ck: &KeyCombination) -> String {
    let label = scout4all_keys::KeyCombinationFormat::default()
        .format(*ck)
        .to_string();
    // crokey names the minus: the scout reads the symbol, as in the TUI;
    // the modifiers as symbols, to fit a keycap: "⇧E" for shift-e
    match label.as_str() {
        "Hyphen" => "-".to_string(),
        _ => {
            let short = label
                .replace("Shift-", "⇧")
                .replace("Ctrl-", "⌃")
                .replace("Alt-", "⌥");
            // the letter after a modifier in capitals, as on the keyboard
            match short.rfind(['⇧', '⌃', '⌥']) {
                Some(at) => {
                    let end = at
                        + short
                            .get(at..)
                            .and_then(|s| s.chars().next())
                            .map_or(0, char::len_utf8);
                    match (short.get(..end), short.get(end..)) {
                        (Some(mods), Some(key)) => format!("{mods}{}", key.to_uppercase()),
                        _ => short,
                    }
                }
                None => short,
            }
        }
    }
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
        assert_eq!(label(&key!('-')), "-");
        assert_eq!(label(&key!(shift - e)), "⇧E");
        assert_eq!(label(&key!(a)), "a");
        assert_eq!(digit(&key!(a)), None);
    }
}
