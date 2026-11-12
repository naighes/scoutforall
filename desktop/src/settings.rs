//! The settings (F2), in two pages (⇧Tab Tab): the general ones
//! (`general`) and the keys of the scouting, to change one by one: events
//! and options, evaluations and serves, sets, in three columns. The rules are the
//! TUI's (the `keys` crate): one key each, the fixed keys refused, no key
//! shared by shortcuts active at the same time. Saved in `config.json`, as
//! the TUI saves them.

use {
    crate::{general::General, keymap, strings::Strings, theme::Colors, ui},
    iced::{
        Alignment, Element, Length,
        widget::{Column, column, container, row, scrollable, space},
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
    scout4all_mobile::Scout4All,
    std::{
        path::{Path, PathBuf},
        sync::Arc,
    },
};

#[derive(Debug, Clone)]
pub enum Message {
    Page(Page),
    General(crate::general::Message),
    /// a row clicked: chosen, and waiting for its key
    Change(usize),
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Page {
    General,
    Keys,
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
    merge(dir, serde_json::to_value(keys).map_err(|e| e.to_string())?)
}

/// Writes `values` (an object) in `config.json`, leaving the rest as it
/// is: written aside, then renamed.
pub fn merge(dir: &Path, values: serde_json::Value) -> Result<(), String> {
    let path = dir.join("config.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    let groups = values;
    if let (Some(config), Some(groups)) = (json.as_object_mut(), groups.as_object()) {
        config.extend(groups.clone());
    }
    let content = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
    let aside = dir.join("config.json.tmp");
    std::fs::write(&aside, content).map_err(|e| e.to_string())?;
    std::fs::rename(&aside, &path).map_err(|e| e.to_string())
}

pub struct SettingsScreen {
    page: Page,
    pub general: General,
    dir: PathBuf,
    pub keys: ScoutingKeys,
    labels: &'static Labels,
    selected: usize,
    /// waiting for the new key of the selected row
    waiting: bool,
    error: Option<String>,
}

impl SettingsScreen {
    pub fn new(
        core: Arc<Scout4All>,
        dir: PathBuf,
        keys: ScoutingKeys,
        language: LanguageEnum,
        s: &Strings,
    ) -> Self {
        Self {
            page: Page::General,
            general: General::new(core, language, keys.player_keys, s),
            dir,
            keys,
            labels: labels(language),
            selected: 0,
            waiting: false,
            error: None,
        }
    }

    /// The language chosen now.
    pub fn language(&self) -> LanguageEnum {
        self.general.language
    }

    /// The language changed: the names in its words.
    pub fn relabel(&mut self, s: &Strings) {
        self.labels = labels(self.general.language);
        self.general.relabel(s);
    }

    pub fn focus_id(&self) -> Option<iced::widget::Id> {
        match self.page {
            Page::General => self.general.focus_id(),
            Page::Keys => None,
        }
    }

    fn turn(&mut self) {
        self.page = match self.page {
            Page::General => Page::Keys,
            Page::Keys => Page::General,
        };
        self.waiting = false;
    }

    pub fn update(&mut self, message: Message, s: &Strings) -> Outcome {
        match message {
            Message::Page(page) => {
                self.page = page;
                self.waiting = false;
            }
            Message::General(m) => {
                let stay = self.general.update(&self.dir, s, m);
                self.keys.player_keys = self.general.player_keys;
                return match stay {
                    true => Outcome::Stay,
                    false => Outcome::Close,
                };
            }
            Message::Change(i) => {
                self.selected = i;
                self.waiting = true;
                self.error = None;
            }
            // the keys only: how the digits choose a player is a general
            // setting
            Message::Reset => self.store(
                ScoutingKeys {
                    player_keys: self.keys.player_keys,
                    ..ScoutingKeys::default()
                }
                .normalized(),
                s,
            ),
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
        let turn = ck == key!(tab)
            || ck == key!(shift - tab)
            || ck == key!(shift - backtab)
            || ck == key!(backtab);
        // the terms read, or a country offered to Tab
        let reading = self.general.terms.is_some() || self.general.form.offering();
        match (self.page, turn && !self.waiting && !reading) {
            (_, true) => {
                self.turn();
                return Outcome::Stay;
            }
            (Page::General, false) => {
                let stay = self.general.key(&self.dir, s, ck);
                self.keys.player_keys = self.general.player_keys;
                return match stay {
                    true => Outcome::Stay,
                    false => Outcome::Close,
                };
            }
            (Page::Keys, false) => {}
        }
        let rows = rows();
        let shortcut = rows.get(self.selected).copied();
        match (self.waiting, shortcut) {
            (true, _) if ck == BACK_KEY => self.waiting = false,
            (true, Some(shortcut)) => {
                self.waiting = false;
                match assigned(&self.keys, shortcut, ck) {
                    Ok(keys) => self.store(keys, s),
                    Err(Conflict::Reserved) => {
                        self.error = Some(s.reserved_key.replace("{}", &keymap::label(&ck, s)))
                    }
                    Err(Conflict::Taken(other)) => {
                        self.error = Some(
                            s.taken_key
                                .replacen("{}", &keymap::label(&ck, s), 1)
                                .replacen("{}", &name(other, self.labels, s), 1),
                        )
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

    /// The keys of the screen, for the bar at the bottom.
    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        if self.page == Page::General {
            return self.general.hints(s);
        }
        vec![
            ("⇧⇥ ⇥".to_string(), s.page.to_string()),
            ("↑ ↓ ← →".to_string(), String::new()),
            (String::new(), s.change_hint.to_string()),
            ("Esc".to_string(), s.back.to_string()),
        ]
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let pages = row![
            ui::pill(
                s.general,
                self.page == Page::General,
                Some(Message::Page(Page::General)),
                c
            ),
            ui::pill(
                s.keys_page,
                self.page == Page::Keys,
                Some(Message::Page(Page::Keys)),
                c
            ),
        ]
        .spacing(8);
        let page = match (self.page, &self.general.terms) {
            // the terms on their own, as they're read
            (Page::General, Some(_)) => return self.general.view(s, c).map(Message::General),
            (Page::General, None) => self.general.view(s, c).map(Message::General),
            (Page::Keys, _) => self.keys_view(s, c),
        };
        column![pages, page].spacing(14).into()
    }

    fn keys_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
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
                for shortcut in shortcuts {
                    // the second group of the column starts with its kind
                    let second_group = |s: Option<Shortcut>| {
                        matches!(s, Some(Shortcut::Option(_) | Shortcut::Serve(_)))
                    };
                    if second_group(Some(shortcut)) && !second_group(shortcuts_before(index)) {
                        list = list
                            .push(space::vertical().height(10))
                            .push(ui::section(second, c));
                    }
                    list = list.push(self.row(index, shortcut, s, c));
                    index += 1;
                }
                container(ui::panel(
                    Some(first),
                    scrollable(list).height(Length::Fill),
                    c,
                ))
                .width(Length::FillPortion(1))
                .height(Length::Fill)
                .into()
            });
        let hint = match (self.waiting, &self.error) {
            (true, _) => ui::body(s.press_key).color(c.accent),
            (false, Some(e)) => ui::body(e.clone()).color(c.danger),
            (false, None) => ui::body(s.shortcuts).color(c.muted),
        };
        let header = row![
            hint,
            space::horizontal(),
            ui::pill(s.reset_keys, false, Some(Message::Reset), c),
        ]
        .align_y(Alignment::Center);
        column![header, row(panels).spacing(14).height(Length::Fill)]
            .spacing(14)
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
        let key: Element<'a, Message> =
            match (selected && self.waiting, key_of(&self.keys, shortcut)) {
                (true, _) => ui::keycap("…", c),
                (false, Some(ck)) => ui::keycap(&keymap::label(&ck, s), c),
                (false, None) => ui::body("–").color(c.muted).into(),
            };
        let body = row![
            ui::body(name(shortcut, self.labels, s)).width(Length::Fill),
            key,
        ]
        .align_y(Alignment::Center);
        ui::list_row(body, selected, Some(Message::Change(index)), c)
    }
}

/// The shortcut on the row before `index`, if any: a group starts where
/// the kind changes.
fn shortcuts_before(index: usize) -> Option<Shortcut> {
    index.checked_sub(1).and_then(|i| rows().get(i).copied())
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        scout4all_keys::{PlayerKeysEnum, key},
    };

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

    /// How the digits choose a player: in the general settings, saved
    /// with the keys and given to the scouting, kept when the keys go back
    /// to the defaults.
    #[test]
    fn the_player_keys_chosen_among_the_general_settings() {
        let s = crate::strings::strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let keys = ScoutingKeys::default().normalized();
        assert_eq!(keys.player_keys, PlayerKeysEnum::Number);
        let mut screen =
            SettingsScreen::new(core, dir.path().to_path_buf(), keys, LanguageEnum::It, s);
        let role = PlayerKeysEnum::ALL
            .iter()
            .position(|k| *k == PlayerKeysEnum::Role)
            .unwrap();
        let field = screen
            .general
            .form
            .fields
            .iter()
            .position(|f| f.label == s.player_keys)
            .unwrap();
        screen.update(
            Message::General(crate::general::Message::Form(crate::form::Message::Choose(
                field, role,
            ))),
            s,
        );
        assert_eq!(screen.keys.player_keys, PlayerKeysEnum::Role);
        let saved: ScoutingKeys =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join("config.json")).unwrap())
                .unwrap();
        assert_eq!(saved.player_keys, PlayerKeysEnum::Role);
        screen.update(Message::Reset, s);
        assert_eq!(screen.keys.player_keys, PlayerKeysEnum::Role);
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
