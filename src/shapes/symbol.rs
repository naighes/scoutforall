use std::collections::HashMap;

use crokey::{KeyCombination, KeyCombinationFormat};

use once_cell::sync::Lazy;

static SYMBOLS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("Tab", "⇥");
    m.insert("Enter", "↵");
    m.insert("Control", "⌃");
    m.insert("Shift", "⇧");
    m.insert("Home", "↖");
    m.insert("End", "↘");
    m.insert("Alt", "⌥");
    m.insert("Space", "␣");
    m.insert("Left", "⬅");
    m.insert("Right", "⮕");
    m.insert("Up", "⬆");
    m.insert("Down", "⬇");
    m.insert("Backspace", "⌫");
    m.insert("Escape", "⎋");
    m.insert("Command", "⌘");
    m
});
pub struct KeyCombinationFormatExt<'a> {
    pub a: &'a KeyCombinationFormat,
}

impl<'a> KeyCombinationFormatExt<'a> {
    pub fn new(t: &'a KeyCombinationFormat) -> Self {
        Self { a: t }
    }

    pub fn to_string<K: Into<KeyCombination>>(&self, key: K) -> String {
        self.a
            .format(key)
            .to_string()
            .split(self.a.key_separator.as_str())
            .map(|token| SYMBOLS.get(token).unwrap_or(&token).to_string())
            .collect::<Vec<String>>()
            .join(self.a.key_separator.as_str())
    }
}
