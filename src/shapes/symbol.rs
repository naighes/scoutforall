use crate::localization::current_labels;
use std::collections::HashMap;

use crokey::{KeyCombination, KeyCombinationFormat};

use std::sync::LazyLock;

static SYMBOLS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("Tab", "⇥");
    m.insert("Enter", "↵");
    m.insert("Control", "⌃");
    m.insert("Shift", "⇧");
    m.insert("Home", "↖");
    m.insert("End", "↘");
    m.insert("Alt", "⌥");
    m.insert("Left", "⬅");
    m.insert("Right", "⮕");
    m.insert("Up", "⬆");
    m.insert("Down", "⬇");
    m.insert("Backspace", "⌫");
    m.insert("Escape", "⎋");
    m.insert("Command", "⌘");
    m.insert("Hyphen", "-");
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
            .map(|token| match token {
                // a symbol for the space bar isn't understood: its name is
                "Space" => current_labels().space_key.to_string(),
                token => SYMBOLS.get(token).unwrap_or(&token).to_string(),
            })
            .collect::<Vec<String>>()
            .join(self.a.key_separator.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crokey::key;

    /// The space bar by its name, not by a symbol hard to recognize.
    #[test]
    fn the_space_bar_is_named() {
        let format = KeyCombinationFormat::default();
        let fmt = KeyCombinationFormatExt::new(&format);
        assert_eq!(fmt.to_string(key!(space)), current_labels().space_key);
        assert_eq!(fmt.to_string(key!(enter)), "↵");
    }
}
