//! The keys of the scouting, to change one by one: events and options,
//! evaluations and serves, sets, in three columns. The rules are the
//! TUI's (the `keys` crate): one key each, the fixed keys refused, no key
//! shared by shortcuts active at the same time. Saved in `config.json`, as
//! the TUI saves them.

use {
    crate::{keymap, strings::Strings, theme::Colors},
    iced::{
        Alignment, Color, Element, Length, Padding,
        widget::{Column, button, column, container, row, scrollable, space, text},
    },
    scout4all_core::{
        localization::{Labels, labels},
        shapes::{
            enums::{EvalEnum, EventTypeEnum, LanguageEnum, WithDesc},
            technique::{ServeTypeEnum, SetTypeEnum},
        },
    },
    scout4all_keys::{
        BACK_KEY, Conflict, KeyCombination, ScoutingKeys, ScoutingOptionEnum, Shortcut, key,
    },
    std::path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub enum Message {
    /// a row clicked: chosen, and waiting for its key
    Change(usize),
    Reset,
}

/// What the screen does after a key or a message.
pub enum Outcome {
    Stay,
    /// back where it was opened, with the keys as saved
    Close,
}

/// The rows, in the order the arrows go through them, by column.
fn columns() -> [Vec<Shortcut>; 3] {
    [
        EventTypeEnum::ALL
            .into_iter()
            .map(Shortcut::Event)
            .chain(ScoutingOptionEnum::ALL.into_iter().map(Shortcut::Option))
            .collect(),
        EvalEnum::ALL
            .into_iter()
            .map(Shortcut::Eval)
            .chain(ServeTypeEnum::ALL.into_iter().map(Shortcut::Serve))
            .collect(),
        SetTypeEnum::ALL.into_iter().map(Shortcut::Set).collect(),
    ]
}

fn rows() -> Vec<Shortcut> {
    columns().into_iter().flatten().collect()
}

/// The name of the shortcut, as the scout reads it.
pub fn name(shortcut: Shortcut, labels: &Labels, s: &Strings) -> String {
    match shortcut {
        Shortcut::Event(e) => e.with_desc(labels).1,
        Shortcut::Eval(e) => e.with_desc(labels).1,
        Shortcut::Set(t) => t.with_desc(labels).1,
        Shortcut::Serve(t) => t.with_desc(labels).1,
        Shortcut::Option(ScoutingOptionEnum::InvertedP1) => s.inverted_p1.to_string(),
        Shortcut::Option(ScoutingOptionEnum::Reception) => s.reception.to_string(),
    }
}

/// The key of the shortcut now.
pub fn key_of(keys: &ScoutingKeys, shortcut: Shortcut) -> Option<KeyCombination> {
    match shortcut {
        Shortcut::Event(e) => keys.scouting_keybindings.key_for(&e),
        Shortcut::Eval(e) => keys.eval_keybindings.key_for(&e),
        Shortcut::Set(t) => keys.set_type_keybindings.key_for(&t),
        Shortcut::Serve(t) => keys.serve_type_keybindings.key_for(&t),
        Shortcut::Option(o) => keys.option_keybindings.key_for(&o),
    }
}

/// The keys with `ck` bound to the shortcut, or why it can't be.
pub fn assigned(
    keys: &ScoutingKeys,
    shortcut: Shortcut,
    ck: KeyCombination,
) -> Result<ScoutingKeys, Conflict> {
    match shortcut {
        Shortcut::Event(e) => keys.assigned(e, ck),
        Shortcut::Eval(e) => keys.assigned(e, ck),
        Shortcut::Set(t) => keys.assigned(t, ck),
        Shortcut::Serve(t) => keys.assigned(t, ck),
        Shortcut::Option(o) => keys.assigned(o, ck),
    }
}

/// Writes the keys in `config.json`, leaving the rest as it is (the other
/// apps keep their settings there too): written aside, then renamed.
pub fn save(dir: &Path, keys: &ScoutingKeys) -> Result<(), String> {
    let path = dir.join("config.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    let groups = serde_json::to_value(keys).map_err(|e| e.to_string())?;
    if let (Some(config), Some(groups)) = (json.as_object_mut(), groups.as_object()) {
        config.extend(groups.clone());
    }
    let content = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    let aside = dir.join("config.json.tmp");
    std::fs::write(&aside, content).map_err(|e| e.to_string())?;
    std::fs::rename(&aside, &path).map_err(|e| e.to_string())
}

pub struct SettingsScreen {
    dir: PathBuf,
    pub keys: ScoutingKeys,
    labels: &'static Labels,
    selected: usize,
    /// waiting for the new key of the selected row
    waiting: bool,
    error: Option<String>,
}

impl SettingsScreen {
    pub fn new(dir: PathBuf, keys: ScoutingKeys, language: LanguageEnum) -> Self {
        Self {
            dir,
            keys,
            labels: labels(language),
            selected: 0,
            waiting: false,
            error: None,
        }
    }

    pub fn update(&mut self, message: Message, s: &Strings) -> Outcome {
        match message {
            Message::Change(i) => {
                self.selected = i;
                self.waiting = true;
                self.error = None;
            }
            Message::Reset => self.store(ScoutingKeys::default().normalized(), s),
        }
        Outcome::Stay
    }

    fn store(&mut self, keys: ScoutingKeys, s: &Strings) {
        match save(&self.dir, &keys) {
            Ok(()) => {
                self.keys = keys;
                self.error = None;
            }
            Err(e) => self.error = Some(s.not_saved.replace("{}", &e)),
        }
    }

    pub fn key(&mut self, ck: KeyCombination, s: &Strings) -> Outcome {
        let rows = rows();
        let shortcut = rows.get(self.selected).copied();
        match (self.waiting, shortcut) {
            (true, _) if ck == BACK_KEY => self.waiting = false,
            (true, Some(shortcut)) => {
                self.waiting = false;
                match assigned(&self.keys, shortcut, ck) {
                    Ok(keys) => self.store(keys, s),
                    Err(Conflict::Reserved) => {
                        self.error = Some(s.reserved_key.replace("{}", &keymap::label(&ck)))
                    }
                    Err(Conflict::Taken(other)) => {
                        self.error =
                            Some(s.taken_key.replacen("{}", &keymap::label(&ck), 1).replacen(
                                "{}",
                                &name(other, self.labels, s),
                                1,
                            ))
                    }
                }
            }
            (true, None) => self.waiting = false,
            (false, _) => return self.navigate(ck, rows.len()),
        }
        Outcome::Stay
    }

    /// ↑ ↓ along the rows, ← → to the next column, Enter to change, Esc
    /// back.
    fn navigate(&mut self, ck: KeyCombination, count: usize) -> Outcome {
        let starts: Vec<usize> = columns()
            .iter()
            .scan(0, |start, c| {
                let this = *start;
                *start += c.len();
                Some(this)
            })
            .collect();
        let column = starts
            .iter()
            .rposition(|s| *s <= self.selected)
            .unwrap_or(0);
        let offset = self
            .selected
            .saturating_sub(starts.get(column).copied().unwrap_or(0));
        let column_at = |c: usize| {
            let lens = columns();
            let start = starts.get(c).copied().unwrap_or(0);
            let len = lens.get(c).map_or(1, Vec::len);
            start + offset.min(len.saturating_sub(1))
        };
        match ck {
            k if k == BACK_KEY => return Outcome::Close,
            k if k == key!(down) => {
                self.selected = (self.selected + 1).min(count.saturating_sub(1))
            }
            k if k == key!(up) => self.selected = self.selected.saturating_sub(1),
            k if k == key!(right) => {
                self.selected = column_at((column + 1).min(starts.len().saturating_sub(1)))
            }
            k if k == key!(left) => self.selected = column_at(column.saturating_sub(1)),
            k if k == key!(enter) => {
                self.waiting = true;
                self.error = None;
            }
            _ => {}
        }
        Outcome::Stay
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let titles = [
            [s.events, s.options],
            [s.evals_title, s.serves_title],
            [s.sets_title, ""],
        ];
        let mut index = 0;
        let panels = columns()
            .into_iter()
            .zip(titles)
            .map(|(shortcuts, [first, second])| {
                let mut list = Column::new().spacing(2);
                list = list.push(text(first.to_uppercase()).size(12).color(c.muted));
                for shortcut in shortcuts {
                    // the second group of the column starts with its kind
                    let second_group = |s: Option<Shortcut>| {
                        matches!(s, Some(Shortcut::Option(_) | Shortcut::Serve(_)))
                    };
                    if second_group(Some(shortcut)) && !second_group(shortcuts_before(index)) {
                        list = list
                            .push(space::vertical().height(10))
                            .push(text(second.to_uppercase()).size(12).color(c.muted));
                    }
                    list = list.push(self.row(index, shortcut, s, c));
                    index += 1;
                }
                container(scrollable(list).height(Length::Fill))
                    .padding(12)
                    .width(Length::FillPortion(1))
                    .height(Length::Fill)
                    .style(move |_| panel_style(c))
                    .into()
            });
        let hint = match (self.waiting, &self.error) {
            (true, _) => text(s.press_key).color(c.accent),
            (false, Some(e)) => text(e.clone()).color(crate::theme::LOST),
            (false, None) => text(s.change_hint).color(c.muted),
        };
        let header = row![
            column![
                text(s.settings).size(24).color(c.accent),
                text(s.shortcuts).size(14).color(c.muted)
            ],
            space::horizontal(),
            button(text(s.reset_keys).size(13))
                .on_press(Message::Reset)
                .style(button::secondary),
        ]
        .align_y(Alignment::Center);
        let footer = text(format!("↑ ↓ ← →  Enter  ·  Esc {}", s.back))
            .size(12)
            .color(c.muted);
        column![
            header,
            hint.size(15),
            row(panels).spacing(12).height(Length::Fill),
            footer
        ]
        .spacing(12)
        .padding(16)
        .into()
    }

    fn row<'a>(
        &self,
        index: usize,
        shortcut: Shortcut,
        s: &Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let selected = index == self.selected;
        let key = match (selected && self.waiting, key_of(&self.keys, shortcut)) {
            (true, _) => "…".to_string(),
            (false, Some(ck)) => keymap::label(&ck),
            (false, None) => "–".to_string(),
        };
        let body = row![
            text(name(shortcut, self.labels, s))
                .size(15)
                .width(Length::Fill),
            container(text(key).size(14).color(c.accent))
                .width(Length::Fixed(70.0))
                .align_x(Alignment::End),
        ]
        .align_y(Alignment::Center);
        button(body)
            .width(Length::Fill)
            .padding(Padding::from([5, 8]))
            .on_press(Message::Change(index))
            .style(move |_, status| button::Style {
                background: match (selected, status) {
                    (true, _) => Some(
                        Color {
                            a: 0.25,
                            ..c.accent
                        }
                        .into(),
                    ),
                    (false, button::Status::Hovered) => Some(c.raised.into()),
                    _ => None,
                },
                text_color: c.text,
                border: iced::Border {
                    radius: 6.0.into(),
                    ..iced::Border::default()
                },
                ..button::Style::default()
            })
            .into()
    }
}

/// The shortcut on the row before `index`, if any: a group starts where
/// the kind changes.
fn shortcuts_before(index: usize) -> Option<Shortcut> {
    index.checked_sub(1).and_then(|i| rows().get(i).copied())
}

fn panel_style(c: Colors) -> container::Style {
    container::Style {
        background: Some(c.surface.into()),
        border: iced::Border {
            radius: 8.0.into(),
            width: 1.0,
            color: c.raised,
        },
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use {super::*, scout4all_keys::key};

    #[test]
    fn keys_refused_and_assigned() {
        let keys = ScoutingKeys::default().normalized();
        // F2 is the settings', a digit chooses the players
        assert_eq!(
            assigned(&keys, Shortcut::Event(EventTypeEnum::A), key!(f2)).err(),
            Some(Conflict::Reserved)
        );
        assert_eq!(
            assigned(&keys, Shortcut::Event(EventTypeEnum::A), key!('3')).err(),
            Some(Conflict::Reserved)
        );
        // a set can have a digit: chosen with the evaluation
        assert!(assigned(&keys, Shortcut::Set(SetTypeEnum::Five), key!('5')).is_ok());
        // the key of another event
        let serve = key_of(&keys, Shortcut::Event(EventTypeEnum::S));
        let taken = serve.map(|ck| assigned(&keys, Shortcut::Event(EventTypeEnum::A), ck).err());
        assert_eq!(
            taken,
            Some(Some(Conflict::Taken(Shortcut::Event(EventTypeEnum::S))))
        );
        // a free key: the event has it, and only it
        let changed = assigned(&keys, Shortcut::Event(EventTypeEnum::A), key!(z));
        assert_eq!(
            changed
                .ok()
                .and_then(|k| key_of(&k, Shortcut::Event(EventTypeEnum::A))),
            Some(key!(z))
        );
    }

    #[test]
    fn saved_with_the_rest_of_the_config() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.json"),
            r##"{ "language": "it", "pinned_teams": ["x"], "eval_keybindings": { "#": ["h"] } }"##,
        )
        .unwrap();
        let keys = ScoutingKeys::default()
            .normalized()
            .assigned(EventTypeEnum::A, key!(z))
            .unwrap();
        save(dir.path(), &keys).unwrap();
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join("config.json")).unwrap())
                .unwrap();
        assert_eq!(saved["language"], "it");
        assert_eq!(saved["pinned_teams"][0], "x");
        let read: ScoutingKeys = serde_json::from_value(saved).unwrap();
        assert_eq!(
            read.scouting_keybindings.key_for(&EventTypeEnum::A),
            Some(key!(z))
        );
        // the evaluations as saved now: the defaults
        assert_eq!(
            read.eval_keybindings.key_for(&EvalEnum::Perfect),
            keys.eval_keybindings.key_for(&EvalEnum::Perfect)
        );
    }
}
