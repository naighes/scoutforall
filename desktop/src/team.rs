//! A team: what it is, its players and its matches side by side. The
//! players are added, edited and removed here, the matches created and
//! opened. ← → (or ⇧Tab Tab) between players and matches, ↑ ↓ in the list.

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
    scout4all_mobile::{MatchListing, MobileError, Scout4All, TeamDetails},
    std::sync::Arc,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Panel {
    Players,
    Matches,
}

enum Mode {
    Browse,
    Edit(Box<Editor>),
    /// the player to remove, once confirmed: id and name
    Remove(String, String),
}

#[derive(Debug, Clone)]
pub enum Message {
    EditTeam,
    NewPlayer,
    Player(usize),
    NewMatch,
    Match(usize),
    Form(form::Message),
    Confirm,
    Cancel,
}

pub enum Outcome {
    Stay,
    Back,
    /// the match to open: saved now, or chosen
    OpenMatch(String),
}

pub struct TeamScreen {
    core: Arc<Scout4All>,
    pub team_id: String,
    details: TeamDetails,
    matches: Vec<MatchListing>,
    panel: Panel,
    player: usize,
    game: usize,
    mode: Mode,
    error: Option<String>,
}

/// The day of the date, as the home shows it.
pub fn day(date: &str) -> &str {
    date.get(..10).unwrap_or(date)
}

impl TeamScreen {
    pub fn new(core: Arc<Scout4All>, team_id: String) -> Result<TeamScreen, MobileError> {
        let details = core.team(team_id.clone())?;
        let matches = core.matches(team_id.clone())?;
        Ok(TeamScreen {
            core,
            team_id,
            details,
            matches,
            panel: Panel::Players,
            player: 0,
            game: 0,
            mode: Mode::Browse,
            error: None,
        })
    }

    pub fn title(&self) -> String {
        self.details.name.clone()
    }

    /// Read again after a change.
    fn reload(&mut self) {
        let read = self
            .core
            .team(self.team_id.clone())
            .and_then(|d| Ok((d, self.core.matches(self.team_id.clone())?)));
        match read {
            Ok((details, matches)) => {
                self.details = details;
                self.matches = matches;
                self.player = self
                    .player
                    .min(self.details.players.len().saturating_sub(1));
                self.game = self.game.min(self.matches.len().saturating_sub(1));
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn focus_id(&self) -> Option<Id> {
        match &self.mode {
            Mode::Edit(editor) => Some(editor.focus_id()),
            Mode::Browse | Mode::Remove(..) => None,
        }
    }

    fn edit(&mut self, editor: Result<Editor, MobileError>) {
        match editor {
            Ok(editor) => self.mode = Mode::Edit(Box::new(editor)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn edit_team(&mut self, s: &Strings) {
        let editor = Editor::team(
            &self.core,
            s,
            Some(self.team_id.clone()),
            self.details.form.clone(),
        );
        self.edit(Ok(editor));
    }

    fn edit_player(&mut self, s: &Strings, index: Option<usize>) {
        let player = index.and_then(|i| self.details.players.get(i));
        let editor = Editor::player(
            s,
            &self.core,
            self.team_id.clone(),
            player.map(|p| p.id.clone()),
            player.map(|p| p.form.clone()).unwrap_or_default(),
        );
        self.edit(Ok(editor));
    }

    fn new_match(&mut self, s: &Strings) {
        let editor = Editor::a_match(s, &self.core, self.team_id.clone(), None);
        self.edit(editor);
    }

    fn open_match(&self, index: usize) -> Outcome {
        match self.matches.get(index) {
            Some(MatchListing::Match { id, .. }) => Outcome::OpenMatch(id.clone()),
            Some(MatchListing::Unreadable { .. }) | None => Outcome::Stay,
        }
    }

    fn ask_removal(&mut self, index: usize) {
        if let Some(p) = self.details.players.get(index) {
            self.player = index;
            self.mode = Mode::Remove(p.id.clone(), format!("{} {}", p.number, p.name));
        }
    }

    fn remove(&mut self) {
        if let Mode::Remove(id, _) = &self.mode {
            let removed = self.core.remove_player(self.team_id.clone(), id.clone());
            self.error = removed.err().map(|e| e.to_string());
        }
        self.mode = Mode::Browse;
        self.reload();
    }

    /// After the form: saved, the team is read again; a new match opens.
    fn edited(&mut self, edited: Edited) -> Outcome {
        let new_match = matches!(&self.mode, Mode::Edit(e) if e.is_new_match());
        match edited {
            Edited::Stay => Outcome::Stay,
            Edited::Cancel => {
                self.mode = Mode::Browse;
                Outcome::Stay
            }
            Edited::Saved(id) => {
                self.mode = Mode::Browse;
                self.reload();
                match new_match {
                    true => Outcome::OpenMatch(id),
                    false => Outcome::Stay,
                }
            }
        }
    }

    pub fn update(&mut self, s: &Strings, message: Message) -> Outcome {
        match (message, &mut self.mode) {
            (Message::Form(m), Mode::Edit(editor)) => {
                let edited = editor.update(&self.core, s, m);
                self.edited(edited)
            }
            (Message::Confirm, Mode::Remove(..)) => {
                self.remove();
                Outcome::Stay
            }
            (Message::Cancel, _) => {
                self.mode = Mode::Browse;
                Outcome::Stay
            }
            (_, Mode::Edit(_) | Mode::Remove(..)) => Outcome::Stay,
            (Message::EditTeam, Mode::Browse) => {
                self.edit_team(s);
                Outcome::Stay
            }
            (Message::NewPlayer, Mode::Browse) => {
                self.edit_player(s, None);
                Outcome::Stay
            }
            (Message::Player(i), Mode::Browse) => {
                self.panel = Panel::Players;
                self.player = i;
                self.edit_player(s, Some(i));
                Outcome::Stay
            }
            (Message::NewMatch, Mode::Browse) => {
                self.new_match(s);
                Outcome::Stay
            }
            (Message::Match(i), Mode::Browse) => {
                self.panel = Panel::Matches;
                self.game = i;
                self.open_match(i)
            }
            (Message::Form(_) | Message::Confirm, Mode::Browse) => Outcome::Stay,
        }
    }

    pub fn key(&mut self, s: &Strings, ck: KeyCombination) -> Outcome {
        if let Mode::Edit(editor) = &mut self.mode {
            let edited = editor.key(&self.core, s, ck);
            return self.edited(edited);
        }
        if let Mode::Remove(..) = self.mode {
            match ck {
                k if k == key!(enter) => self.remove(),
                k if k == BACK_KEY => self.mode = Mode::Browse,
                _ => {}
            }
            return Outcome::Stay;
        }
        let count = match self.panel {
            Panel::Players => self.details.players.len(),
            Panel::Matches => self.matches.len(),
        };
        let selected = match self.panel {
            Panel::Players => &mut self.player,
            Panel::Matches => &mut self.game,
        };
        let switch = ck == key!(left)
            || ck == key!(right)
            || ck == key!(tab)
            || ck == key!(shift - tab)
            || ck == key!(shift - backtab)
            || ck == key!(backtab);
        match (self.panel, ck) {
            (_, k) if k == BACK_KEY => return Outcome::Back,
            (_, _) if switch => {
                self.panel = match self.panel {
                    Panel::Players => Panel::Matches,
                    Panel::Matches => Panel::Players,
                }
            }
            (_, k) if k == key!(down) && count > 0 => *selected = (*selected + 1) % count,
            (_, k) if k == key!(up) && count > 0 => {
                *selected = selected.checked_sub(1).unwrap_or(count - 1)
            }
            (_, k) if k == key!(e) => self.edit_team(s),
            (Panel::Players, k) if k == key!(n) => self.edit_player(s, None),
            (Panel::Matches, k) if k == key!(n) => self.new_match(s),
            (Panel::Players, k) if k == key!(enter) && count > 0 => {
                self.edit_player(s, Some(self.player))
            }
            (Panel::Matches, k) if k == key!(enter) => return self.open_match(self.game),
            (Panel::Players, k) if (k == key!(backspace) || k == key!(delete)) && count > 0 => {
                self.ask_removal(self.player)
            }
            _ => {}
        }
        Outcome::Stay
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match (&self.mode, self.panel) {
            (Mode::Edit(_), _) => form::Form::hints(s),
            (Mode::Remove(..), _) => vec![
                ("Enter".to_string(), s.remove.to_string()),
                ("Esc".to_string(), s.cancel.to_string()),
            ],
            (Mode::Browse, panel) => {
                let enter = match panel {
                    Panel::Players => s.edit,
                    Panel::Matches => s.open,
                };
                let new = match panel {
                    Panel::Players => s.new_player,
                    Panel::Matches => s.new_match,
                };
                let remove = match panel {
                    Panel::Players => vec![("⌫".to_string(), s.remove_player.to_string())],
                    Panel::Matches => vec![],
                };
                [
                    vec![
                        ("← → ⇧⇥ ⇥".to_string(), s.panel.to_string()),
                        ("↑ ↓".to_string(), String::new()),
                        ("Enter".to_string(), enter.to_string()),
                        ("n".to_string(), new.to_string()),
                        ("e".to_string(), s.edit_team.to_string()),
                    ],
                    remove,
                    vec![("Esc".to_string(), s.back.to_string())],
                ]
                .concat()
            }
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let body: Element<'a, Message> = match &self.mode {
            Mode::Edit(editor) => editor.view(s, c, Message::Form),
            Mode::Remove(_, name) => ui::confirm(
                &s.remove_confirm.replace("{}", name),
                s.remove,
                Message::Confirm,
                s.cancel,
                Message::Cancel,
                c,
            ),
            Mode::Browse => row![
                container(self.info(s, c)).width(Length::FillPortion(2)),
                container(self.players(s, c)).width(Length::FillPortion(3)),
                container(self.matches(s, c)).width(Length::FillPortion(3)),
            ]
            .spacing(14)
            .height(Length::Fill)
            .into(),
        };
        let error: Element<'a, Message> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => space::vertical().height(0).into(),
        };
        column![error, body].spacing(10).into()
    }

    fn info<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let d = &self.details;
        let line = |label: &str, value: String| {
            column![
                text(label.to_string()).size(SIZE_SMALL).color(c.muted),
                ui::body(value)
            ]
            .spacing(2)
        };
        let or_dash = |v: &Option<String>| v.clone().unwrap_or_else(|| "–".to_string());
        ui::panel(
            Some(s.details),
            column![
                text(d.name.clone()).size(22).font(theme::fonts().bold),
                line(s.year, d.year.to_string()),
                line(s.classification, or_dash(&d.classification)),
                line(s.gender, or_dash(&d.gender)),
                line(s.country, or_dash(&d.country)),
                space::vertical().height(6),
                ui::pill(s.edit_team, false, Some(Message::EditTeam), c),
            ]
            .spacing(12),
            c,
        )
    }

    fn players<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let active = self.panel == Panel::Players;
        let rows = self.details.players.iter().enumerate().map(|(i, p)| {
            ui::list_row(
                row![
                    text(p.number.to_string())
                        .size(SIZE_BODY)
                        .font(theme::fonts().bold)
                        .color(c.accent)
                        .width(Length::Fixed(34.0)),
                    ui::body(p.name.clone()).width(Length::Fill),
                    text(p.role.clone().unwrap_or_default())
                        .size(SIZE_SMALL)
                        .color(c.muted),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
                active && i == self.player,
                Some(Message::Player(i)),
                c,
            )
        });
        let list: Element<'a, Message> = match self.details.players.is_empty() {
            true => ui::body(s.no_players_yet).color(c.muted).into(),
            false => scrollable(Column::with_children(rows).spacing(3))
                .height(Length::Fill)
                .into(),
        };
        ui::panel(
            Some(&ui::capitalized(s.players)),
            column![
                ui::pill(
                    &format!("+ {}", s.new_player),
                    false,
                    Some(Message::NewPlayer),
                    c
                ),
                list
            ]
            .spacing(10)
            .height(Length::Fill),
            c,
        )
    }

    fn matches<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let active = self.panel == Panel::Matches;
        let rows = self.matches.iter().enumerate().map(|(i, m)| match m {
            MatchListing::Match {
                opponent,
                home,
                date,
                won,
                lost,
                finished,
                ..
            } => ui::list_row(
                row![
                    column![
                        ui::body(opponent.clone()).font(theme::fonts().bold),
                        text(format!(
                            "{} · {}",
                            day(date),
                            match home {
                                true => s.home_game,
                                false => s.away_game,
                            }
                        ))
                        .size(SIZE_SMALL)
                        .color(c.muted),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    text(format!("{won}–{lost}"))
                        .size(20)
                        .font(theme::fonts().bold)
                        .color(match finished {
                            true => c.text,
                            false => c.accent,
                        }),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                active && i == self.game,
                Some(Message::Match(i)),
                c,
            ),
            MatchListing::Unreadable { folder, reason } => text(format!(
                "{folder}{}",
                reason
                    .as_ref()
                    .map(|r| format!(": {r}"))
                    .unwrap_or_default()
            ))
            .size(SIZE_SMALL)
            .color(c.danger)
            .into(),
        });
        let list: Element<'a, Message> = match self.matches.is_empty() {
            true => ui::body(s.no_matches_yet).color(c.muted).into(),
            false => scrollable(Column::with_children(rows).spacing(3))
                .height(Length::Fill)
                .into(),
        };
        ui::panel(
            Some(s.matches),
            column![
                ui::pill(
                    &format!("+ {}", s.new_match),
                    false,
                    Some(Message::NewMatch),
                    c
                ),
                list
            ]
            .spacing(10)
            .height(Length::Fill),
            c,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unreachable)]
mod tests {
    use {
        super::*,
        crate::{
            game::{self, MatchScreen},
            home::{self, HomeScreen},
            strings::strings,
        },
        form::Message::{Choose, Input, Save},
        scout4all_core::{player_form::PlayerForm, shapes::enums::LanguageEnum},
    };

    fn app(dir: &tempfile::TempDir) -> Arc<Scout4All> {
        Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap()
    }

    #[test]
    fn a_team_its_players_and_a_match_from_the_home() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        // the home: n writes a new team
        let mut home = HomeScreen::new(core.home().unwrap());
        home.key(&core, s, key!(n));
        assert!(home.focus_id().is_some());
        // without a name: the error takes the keys to the name
        assert!(matches!(
            home.update(&core, s, home::Message::Form(Save)),
            home::Outcome::Stay
        ));
        for m in [
            Input(0, "Cortona".into()),
            Choose(2, 1),
            Choose(3, 1),
            Input(4, "Ital".into()),
        ] {
            home.update(&core, s, home::Message::Form(m));
        }
        let home::Outcome::Team(team_id) = home.update(&core, s, home::Message::Form(Save)) else {
            unreachable!("the team is saved and opened")
        };
        let details = core.team(team_id.clone()).unwrap();
        assert_eq!(details.name, "Cortona");
        assert_eq!(details.country.as_deref(), Some("Italia"));

        // a player added, then removed
        let mut team = TeamScreen::new(core.clone(), team_id.clone()).unwrap();
        team.update(s, Message::NewPlayer);
        for m in [
            Input(0, "Anna Rossi".into()),
            Input(1, "9".into()),
            Choose(2, 1),
        ] {
            team.update(s, Message::Form(m));
        }
        team.update(s, Message::Form(Save));
        assert_eq!(team.details.players.len(), 1);
        assert!(team.focus_id().is_none());
        team.key(s, key!(backspace));
        assert!(matches!(team.mode, Mode::Remove(..)));
        team.key(s, key!(enter));
        assert!(team.details.players.is_empty());

        // seven players, then a new match: saved, it opens
        for n in 1..=7u8 {
            let player = PlayerForm {
                name: format!("P{n}"),
                number: n.to_string(),
                role: Some(scout4all_core::shapes::enums::RoleEnum::OutsideHitter),
                ..PlayerForm::default()
            };
            core.save_player(team_id.clone(), None, player).unwrap();
        }
        team.key(s, key!(right));
        team.key(s, key!(n));
        team.update(s, Message::Form(Input(0, "Arezzo".into())));
        let Outcome::OpenMatch(match_id) = team.update(s, Message::Form(Save)) else {
            unreachable!("the match is saved and opened")
        };
        let mut m = MatchScreen::new(core, team_id, match_id).unwrap();
        assert!(matches!(m.key(s, key!(enter)), game::Outcome::Scout(true)));
        assert!(matches!(m.key(s, BACK_KEY), game::Outcome::Back));
    }
}
