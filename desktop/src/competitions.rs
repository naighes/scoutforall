//! The competitions (D12), from the home: each with its season, format and
//! matches. ↑ ↓ in the list, Enter (or a click) edits the one chosen, ⌫
//! (or its bin) deletes it, its matches staying without it. They're
//! created in the form of a match, not here.

use {
    crate::{
        editors::{Edited, Editor},
        form,
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, Id, column, container, row, scrollable, space, text},
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{CompetitionListing, MobileError, Scout4All},
    std::sync::Arc,
};

enum Mode {
    Browse,
    Edit(Box<Editor>),
    /// the competition of the list to delete, once confirmed
    Delete(usize),
}

#[derive(Debug, Clone)]
pub enum Message {
    Edit(usize),
    Delete(usize),
    Form(form::Message),
    Confirm,
    Cancel,
}

pub enum Outcome {
    Stay,
    Back,
}

pub struct CompetitionsScreen {
    core: Arc<Scout4All>,
    listed: Vec<CompetitionListing>,
    selected: usize,
    mode: Mode,
    error: Option<String>,
}

impl CompetitionsScreen {
    pub fn new(core: Arc<Scout4All>) -> Result<CompetitionsScreen, MobileError> {
        let listed = core.competition_listings()?;
        Ok(CompetitionsScreen {
            core,
            listed,
            selected: 0,
            mode: Mode::Browse,
            error: None,
        })
    }

    /// Read again after a change, the same row chosen (or the last).
    fn reload(&mut self) {
        match self.core.competition_listings() {
            Ok(listed) => {
                self.listed = listed;
                self.selected = self.selected.min(self.listed.len().saturating_sub(1));
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn focus_id(&self) -> Option<Id> {
        match &self.mode {
            Mode::Edit(editor) => Some(editor.focus_id()),
            Mode::Browse | Mode::Delete(_) => None,
        }
    }

    fn edit(&mut self, s: &Strings, index: usize) {
        let Some(c) = self.listed.get(index) else {
            return;
        };
        self.selected = index;
        match Editor::competition(&self.core, s, c.id) {
            Ok(editor) => self.mode = Mode::Edit(Box::new(editor)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn ask_deletion(&mut self, index: usize) {
        if index < self.listed.len() {
            self.selected = index;
            self.mode = Mode::Delete(index);
        }
    }

    fn delete(&mut self) {
        let mode = std::mem::replace(&mut self.mode, Mode::Browse);
        if let Some(c) = match mode {
            Mode::Delete(index) => self.listed.get(index),
            Mode::Browse | Mode::Edit(_) => None,
        } {
            self.error = self
                .core
                .delete_competition(c.id)
                .err()
                .map(|e| e.to_string());
        }
        self.reload();
    }

    fn edited(&mut self, edited: Edited) {
        match edited {
            Edited::Stay => {}
            Edited::Cancel => self.mode = Mode::Browse,
            Edited::Saved(_) => {
                self.mode = Mode::Browse;
                self.error = None;
                self.reload();
            }
        }
    }

    pub fn update(&mut self, s: &Strings, message: Message) -> Outcome {
        match (message, &mut self.mode) {
            (Message::Form(m), Mode::Edit(editor)) => {
                let edited = editor.update(&self.core, s, m);
                self.edited(edited);
            }
            (Message::Confirm, Mode::Delete(_)) => self.delete(),
            (Message::Cancel, _) => self.mode = Mode::Browse,
            (Message::Edit(i), Mode::Browse) => self.edit(s, i),
            (Message::Delete(i), Mode::Browse) => self.ask_deletion(i),
            (_, _) => {}
        }
        Outcome::Stay
    }

    pub fn key(&mut self, s: &Strings, ck: KeyCombination) -> Outcome {
        if let Mode::Edit(editor) = &mut self.mode {
            let edited = editor.key(&self.core, s, ck);
            self.edited(edited);
            return Outcome::Stay;
        }
        if let Mode::Delete(_) = self.mode {
            match ck {
                k if k == key!(enter) => self.delete(),
                k if k == BACK_KEY => self.mode = Mode::Browse,
                _ => {}
            }
            return Outcome::Stay;
        }
        let count = self.listed.len();
        match ck {
            k if k == BACK_KEY => return Outcome::Back,
            k if k == key!(down) && count > 0 => self.selected = (self.selected + 1) % count,
            k if k == key!(up) && count > 0 => {
                self.selected = self.selected.checked_sub(1).unwrap_or(count - 1)
            }
            k if k == key!(enter) => self.edit(s, self.selected),
            k if k == key!(backspace) || k == key!(delete) => self.ask_deletion(self.selected),
            _ => {}
        }
        Outcome::Stay
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match &self.mode {
            Mode::Edit(editor) => editor.hints(s),
            Mode::Delete(_) => vec![
                ("Enter".to_string(), s.delete_competition.to_string()),
                ("Esc".to_string(), s.cancel.to_string()),
            ],
            Mode::Browse => vec![
                ("↑ ↓".to_string(), String::new()),
                ("Enter".to_string(), s.edit.to_string()),
                ("⌫".to_string(), s.delete_competition.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ],
        }
    }

    /// What the question of the deletion says: the competition and its
    /// matches, which stay.
    fn deletion_text(&self, s: &Strings, index: usize) -> String {
        self.listed.get(index).map_or_else(String::new, |c| {
            let text = match c.matches {
                0 => s.delete_competition_unused,
                _ => s.delete_competition_text,
            };
            ui::fill(text, &[c.name.clone(), c.matches.to_string()])
        })
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let body: Element<'a, Message> = match &self.mode {
            Mode::Edit(editor) => editor.view(s, c, Message::Form),
            Mode::Delete(index) => ui::confirm_deletion_with(
                s,
                &self.deletion_text(s, *index),
                vec![],
                s.delete_competition,
                Message::Confirm,
                Message::Cancel,
                c,
            ),
            Mode::Browse => self.list(s, c),
        };
        let error: Element<'a, Message> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => space::vertical().height(0).into(),
        };
        column![error, body].spacing(10).into()
    }

    /// How wide the list is at most: a name, its season and its matches
    /// read together.
    const WIDTH: f32 = 900.0;

    fn list<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let rows = self.listed.iter().enumerate().map(|(i, competition)| {
            let season = competition
                .season
                .clone()
                .map_or_else(String::new, |season| format!("{season} · "));
            let line = row![
                ui::list_row(
                    row![
                        column![
                            text(competition.name.clone())
                                .size(SIZE_BODY)
                                .font(theme::fonts().bold),
                            text(format!("{season}{}", competition.format))
                                .size(SIZE_SMALL)
                                .color(c.muted),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                        text(ui::fill(
                            s.matches_count,
                            &[competition.matches.to_string()]
                        ))
                        .size(SIZE_SMALL)
                        .color(c.muted),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                    i == self.selected,
                    Some(Message::Edit(i)),
                    c,
                ),
                ui::icon_button(
                    theme::icons::DELETE,
                    s.delete_competition,
                    Message::Delete(i),
                    false,
                    c,
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center);
            line.into()
        });
        let list: Element<'a, Message> = match self.listed.is_empty() {
            true => ui::body(s.no_competitions).color(c.muted).into(),
            false => scrollable(Column::with_children(rows).spacing(3))
                .height(Length::Fill)
                .into(),
        };
        container(
            container(ui::panel(
                Some(s.competitions),
                container(list).height(Length::Fill),
                c,
            ))
            .max_width(Self::WIDTH),
        )
        .center_x(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}

#[cfg(test)]
#[allow(clippy::unreachable)]
mod tests {
    use {
        super::*,
        crate::{
            home::{self, HomeScreen},
            strings::strings,
        },
        chrono::DateTime,
        form::Message::{Choose, Focus, Input, Save},
        scout4all_core::{
            domain::CompetitionFormat,
            match_form::MatchForm,
            player_form::PlayerForm,
            shapes::{
                country::CountryCode,
                enums::{GenderEnum, LanguageEnum, RoleEnum, TeamClassificationEnum},
            },
            team_form::TeamForm,
        },
    };

    /// Cortona with its players, a match in "Serie C" and two in "Coppa".
    fn competitions_played(dir: &tempfile::TempDir) -> (Arc<Scout4All>, String, Vec<String>) {
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = core
            .save_team(
                None,
                TeamForm {
                    name: "Cortona".into(),
                    classification: Some(TeamClassificationEnum::Regional),
                    gender: Some(GenderEnum::Men),
                    country: CountryCode::known("IT"),
                    year: "2026".into(),
                },
            )
            .unwrap();
        for n in 1..=7u8 {
            let player = PlayerForm {
                name: format!("P{n}"),
                number: n.to_string(),
                role: Some(RoleEnum::OutsideHitter),
                ..PlayerForm::default()
            };
            core.save_player(team.clone(), None, player, false).unwrap();
        }
        let matches = [("Arezzo", "Serie C"), ("Pisa", "Coppa"), ("Siena", "Coppa")]
            .into_iter()
            .map(|(opponent, competition)| {
                let form = MatchForm {
                    opponent: opponent.into(),
                    date: DateTime::parse_from_rfc3339("2026-10-18T18:00:00+02:00").ok(),
                    home: true,
                    max_substitutions: "8".into(),
                    competition: competition.into(),
                    ..MatchForm::default()
                };
                core.save_match(team.clone(), None, form).unwrap()
            })
            .collect();
        (core, team, matches)
    }

    /// From the home with "c" (or its pill): the competitions with their
    /// season, format and matches; Esc back.
    #[test]
    fn the_competitions_are_listed_from_the_home() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let (core, _, _) = competitions_played(&dir);
        let mut home = HomeScreen::new(core.home().unwrap());
        assert!(matches!(
            home.key(&core, s, key!(c)),
            home::Outcome::Competitions
        ));
        assert!(matches!(
            home.update(&core, s, home::Message::Competitions),
            home::Outcome::Competitions
        ));
        assert!(
            home.hints(s)
                .contains(&("c".to_string(), s.competitions.to_string()))
        );

        let mut screen = CompetitionsScreen::new(core.clone()).unwrap();
        assert_eq!(
            screen
                .listed
                .iter()
                .map(|c| (
                    c.name.as_str(),
                    c.season.as_deref(),
                    c.format.as_str(),
                    c.matches
                ))
                .collect::<Vec<_>>(),
            vec![
                ("Coppa", Some("2026/2027"), "Campionato", 2),
                ("Serie C", Some("2026/2027"), "Campionato", 1),
            ]
        );
        screen.key(s, key!(down));
        assert_eq!(screen.selected, 1);
        screen.key(s, key!(down));
        assert_eq!(screen.selected, 0);
        screen.key(s, key!(up));
        assert_eq!(screen.selected, 1);
        assert!(matches!(screen.key(s, BACK_KEY), Outcome::Back));

        // none yet: they're made in the form of a match
        let empty = tempfile::tempdir().unwrap();
        let none =
            Scout4All::new(empty.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        assert!(CompetitionsScreen::new(none).unwrap().listed.is_empty());
    }

    /// Enter edits the one chosen: a name taken refused on the name, a new
    /// one saved with its format, read in the form of its matches; Esc
    /// leaves the form unsaved.
    #[test]
    fn a_competition_is_edited_in_its_form() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let (core, team, matches) = competitions_played(&dir);
        let mut screen = CompetitionsScreen::new(core.clone()).unwrap();
        screen.key(s, key!(enter));
        let Mode::Edit(editor) = &screen.mode else {
            unreachable!("the form of Coppa")
        };
        assert_eq!(editor.form.text(0), "Coppa");
        assert_eq!(editor.form.text(1), "2026/2027");

        // the name of the other one in the same season
        screen.update(s, Message::Form(Input(0, "serie c".into())));
        screen.update(s, Message::Form(Focus(3)));
        screen.update(s, Message::Form(Save));
        let Mode::Edit(editor) = &screen.mode else {
            unreachable!("the form, with its error")
        };
        assert_eq!(editor.form.focused, 0);
        assert_eq!(
            editor.form.error.as_deref(),
            Some("esiste già una competizione con questo nome e questa stagione")
        );
        // a country not found: on the country
        screen.update(s, Message::Form(Input(0, "Coppa Toscana".into())));
        screen.update(s, Message::Form(Input(4, "Narnia".into())));
        screen.update(s, Message::Form(Save));
        let Mode::Edit(editor) = &screen.mode else {
            unreachable!("the form, with its error")
        };
        assert_eq!(editor.form.focused, 4);

        screen.update(s, Message::Form(Input(4, String::new())));
        let cup = core
            .competition_formats()
            .iter()
            .position(|f| f.value == CompetitionFormat::Cup)
            .unwrap();
        screen.update(s, Message::Form(Choose(2, cup)));
        screen.update(s, Message::Form(Save));
        assert!(matches!(screen.mode, Mode::Browse));
        assert_eq!(
            screen
                .listed
                .iter()
                .map(|c| (c.name.as_str(), c.format.as_str()))
                .collect::<Vec<_>>(),
            vec![("Coppa Toscana", "Coppa"), ("Serie C", "Campionato")]
        );
        let read = core.match_form(team, matches[1].clone()).unwrap();
        assert_eq!(read.competition, "Coppa Toscana");

        // Esc: nothing saved
        screen.key(s, key!(enter));
        screen.update(s, Message::Form(Input(0, "Altro".into())));
        screen.key(s, BACK_KEY);
        assert!(matches!(screen.mode, Mode::Browse));
        assert_eq!(
            screen.listed.first().map(|c| c.name.as_str()),
            Some("Coppa Toscana")
        );
    }

    /// ⌫ (or the bin) asks first, telling the matches stay; Esc keeps it,
    /// Enter deletes it and its matches stay without it.
    #[test]
    fn a_competition_is_deleted_its_matches_staying() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let (core, team, matches) = competitions_played(&dir);
        let mut screen = CompetitionsScreen::new(core.clone()).unwrap();
        screen.key(s, key!(backspace));
        let Mode::Delete(index) = screen.mode else {
            unreachable!("the deletion to confirm")
        };
        assert_eq!(
            screen.deletion_text(s, index),
            "La competizione Coppa sarà eliminata. Le sue partite (2) restano, senza competizione."
        );
        screen.key(s, key!(x));
        assert!(matches!(screen.mode, Mode::Delete(_)));
        screen.key(s, BACK_KEY);
        assert!(matches!(screen.mode, Mode::Browse));
        assert_eq!(screen.listed.len(), 2);

        screen.update(s, Message::Delete(1));
        assert!(matches!(screen.mode, Mode::Delete(1)));
        assert_eq!(
            screen.deletion_text(s, 1),
            "La competizione Serie C sarà eliminata. Le sue partite (1) restano, senza competizione."
        );
        screen.update(s, Message::Cancel);
        screen.update(s, Message::Delete(0));
        screen.key(s, key!(enter));
        assert!(screen.error.is_none());
        assert_eq!(
            screen
                .listed
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Serie C"]
        );
        assert_eq!(screen.selected, 0);
        for m in &matches[1..] {
            let read = core.match_form(team.clone(), m.clone()).unwrap();
            assert_eq!((read.competition.as_str(), read.competition_id), ("", None));
        }
        assert_eq!(core.matches(team).unwrap().len(), 3);
    }
}
