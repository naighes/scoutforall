//! A team: what it is, its players and its matches side by side. The
//! players are added, edited and removed here, the matches created, opened
//! and deleted, the team deleted. ← → (or ⇧Tab Tab) between players and
//! matches, ↑ ↓ in the list, ⌫ deletes the one chosen, ⇧⌫ the team.

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
    /// what to delete, once confirmed
    Delete(Deletion),
}

/// What goes, for good, with what the question says of it.
enum Deletion {
    /// id and name
    Player(String, String),
    /// id (or the folder of an unreadable one) and what it is
    Match(String, String),
    Team,
}

#[derive(Debug, Clone)]
pub enum Message {
    EditTeam,
    NewPlayer,
    Player(usize),
    NewMatch,
    Match(usize),
    Import,
    Export,
    Analysis,
    DeleteTeam,
    Form(form::Message),
    Confirm,
    Cancel,
}

pub enum Outcome {
    Stay,
    Back,
    /// the match to open: saved now, or chosen
    OpenMatch(String),
    /// an archive of a match to choose, into the team
    Import,
    /// the archive of the team, to save
    Export,
    /// the statistics of the team over its matches
    Analysis,
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
            Mode::Browse | Mode::Delete(_) => None,
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

    /// The deletion of the player or the match chosen, to confirm.
    fn ask_deletion(&mut self) {
        let deletion = match self.panel {
            Panel::Players => self
                .details
                .players
                .get(self.player)
                .map(|p| Deletion::Player(p.id.clone(), format!("{} {}", p.number, p.name))),
            Panel::Matches => self.matches.get(self.game).map(|m| match m {
                MatchListing::Match {
                    id, opponent, date, ..
                } => Deletion::Match(id.clone(), format!("{opponent} · {}", day(date))),
                MatchListing::Unreadable { folder, .. } => {
                    Deletion::Match(folder.clone(), folder.clone())
                }
            }),
        };
        if let Some(deletion) = deletion {
            self.mode = Mode::Delete(deletion);
        }
    }

    /// Deleted as confirmed: the team gone, back to the home.
    fn delete(&mut self) -> Outcome {
        let mode = std::mem::replace(&mut self.mode, Mode::Browse);
        let team = self.team_id.clone();
        let deleted = match &mode {
            Mode::Delete(Deletion::Player(id, _)) => self.core.remove_player(team, id.clone()),
            Mode::Delete(Deletion::Match(id, _)) => self.core.delete_match(team, id.clone()),
            Mode::Delete(Deletion::Team) => self.core.delete_team(team),
            Mode::Browse | Mode::Edit(_) => Ok(()),
        };
        match (deleted, mode) {
            (Ok(()), Mode::Delete(Deletion::Team)) => Outcome::Back,
            (deleted, _) => {
                self.error = deleted.err().map(|e| e.to_string());
                self.reload();
                Outcome::Stay
            }
        }
    }

    /// What the question of the deletion says.
    fn deletion_text(&self, s: &Strings, deletion: &Deletion) -> String {
        match deletion {
            Deletion::Player(_, name) => ui::fill(s.delete_player_text, std::slice::from_ref(name)),
            Deletion::Match(_, name) => ui::fill(s.delete_match_text, std::slice::from_ref(name)),
            Deletion::Team => ui::fill(
                s.delete_team_text,
                &[self.details.name.clone(), self.matches.len().to_string()],
            ),
        }
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
            (Message::Confirm, Mode::Delete(_)) => self.delete(),
            (Message::Cancel, _) => {
                self.mode = Mode::Browse;
                Outcome::Stay
            }
            (_, Mode::Edit(_) | Mode::Delete(_)) => Outcome::Stay,
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
            (Message::Import, Mode::Browse) => Outcome::Import,
            (Message::Export, Mode::Browse) => Outcome::Export,
            (Message::Analysis, Mode::Browse) => Outcome::Analysis,
            (Message::DeleteTeam, Mode::Browse) => {
                self.mode = Mode::Delete(Deletion::Team);
                Outcome::Stay
            }
            (Message::Form(_) | Message::Confirm, Mode::Browse) => Outcome::Stay,
        }
    }

    pub fn key(&mut self, s: &Strings, ck: KeyCombination) -> Outcome {
        if let Mode::Edit(editor) = &mut self.mode {
            let edited = editor.key(&self.core, s, ck);
            return self.edited(edited);
        }
        if let Mode::Delete(_) = self.mode {
            return match ck {
                k if k == key!(enter) => self.delete(),
                k if k == BACK_KEY => {
                    self.mode = Mode::Browse;
                    Outcome::Stay
                }
                _ => Outcome::Stay,
            };
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
            (_, k) if k == key!(i) => return Outcome::Import,
            (_, k) if k == key!(x) => return Outcome::Export,
            (_, k) if k == key!(s) => return Outcome::Analysis,
            (Panel::Players, k) if k == key!(n) => self.edit_player(s, None),
            (Panel::Matches, k) if k == key!(n) => self.new_match(s),
            (Panel::Players, k) if k == key!(enter) && count > 0 => {
                self.edit_player(s, Some(self.player))
            }
            (Panel::Matches, k) if k == key!(enter) => return self.open_match(self.game),
            (_, k) if k == key!(shift - backspace) || k == key!(shift - delete) => {
                self.mode = Mode::Delete(Deletion::Team)
            }
            (_, k) if (k == key!(backspace) || k == key!(delete)) && count > 0 => {
                self.ask_deletion()
            }
            _ => {}
        }
        Outcome::Stay
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match (&self.mode, self.panel) {
            (Mode::Edit(editor), _) => editor.hints(s),
            (Mode::Delete(_), _) => vec![
                ("Enter".to_string(), s.delete_forever.to_string()),
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
                    Panel::Players => s.remove_player,
                    Panel::Matches => s.delete_match,
                };
                [
                    vec![
                        ("← → ⇧⇥ ⇥".to_string(), s.panel.to_string()),
                        ("↑ ↓".to_string(), String::new()),
                        ("Enter".to_string(), enter.to_string()),
                        ("n".to_string(), new.to_string()),
                        ("e".to_string(), s.edit_team.to_string()),
                        ("i".to_string(), s.import_archive.to_string()),
                        ("x".to_string(), s.export_team.to_string()),
                        ("s".to_string(), s.analysis.to_string()),
                    ],
                    vec![
                        ("⌫".to_string(), remove.to_string()),
                        ("⇧⌫".to_string(), s.delete_team.to_string()),
                        ("Esc".to_string(), s.back.to_string()),
                    ],
                ]
                .concat()
            }
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let body: Element<'a, Message> = match &self.mode {
            Mode::Edit(editor) => editor.view(s, c, Message::Form),
            Mode::Delete(deletion) => ui::confirm_deletion(
                s,
                &self.deletion_text(s, deletion),
                Message::Confirm,
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
                space::vertical(),
                ui::action(
                    theme::icons::ANALYSIS,
                    s.analysis,
                    Message::Analysis,
                    false,
                    c
                )
                .width(Length::Fill),
                ui::action(theme::icons::EDIT, s.edit_team, Message::EditTeam, false, c)
                    .width(Length::Fill),
                ui::action(
                    theme::icons::SHARE,
                    s.export_team,
                    Message::Export,
                    false,
                    c
                )
                .width(Length::Fill),
                ui::action(
                    theme::icons::IMPORT,
                    s.import_match,
                    Message::Import,
                    false,
                    c
                )
                .width(Length::Fill),
                ui::action(
                    theme::icons::DELETE,
                    s.delete_team,
                    Message::DeleteTeam,
                    false,
                    c
                )
                .width(Length::Fill),
            ]
            .spacing(12)
            .width(Length::Fill)
            .height(Length::Fill),
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
                    ui::name(p.name.clone()),
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
        ui::panel_with(
            &ui::capitalized(s.players),
            vec![ui::add_button(s.new_player, Message::NewPlayer, c)],
            container(list).height(Length::Fill),
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
                    // home or away at a glance, as on Android
                    ui::tip(
                        ui::icon(
                            match home {
                                true => theme::icons::HOME,
                                false => theme::icons::AWAY,
                            },
                            22.0,
                            c.muted,
                        ),
                        match home {
                            true => s.home_game,
                            false => s.away_game,
                        },
                        c,
                    ),
                    column![
                        ui::body(opponent.clone()).font(theme::fonts().bold),
                        row![
                            text(day(date)).size(SIZE_SMALL).color(c.muted),
                            text(match finished {
                                true => String::new(),
                                false => format!("· {}", s.in_progress),
                            })
                            .size(SIZE_SMALL)
                            .color(c.accent),
                        ]
                        .spacing(6),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    text(format!("{won}–{lost}"))
                        .size(20)
                        .font(theme::fonts().bold)
                        .color(ui::score_color(*won, *lost, *finished, c)),
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
        ui::panel_with(
            s.matches,
            vec![ui::add_button(s.new_match, Message::NewMatch, c)],
            container(list).width(Length::Fill).height(Length::Fill),
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
        form::Message::{Choose, Input, Save, Take},
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
        assert!(matches!(home.key(&core, s, key!(i)), home::Outcome::Import));
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
            // offered, then taken
            Take(4, 0),
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
        assert!(matches!(team.mode, Mode::Delete(Deletion::Player(..))));
        team.key(s, key!(enter));
        assert!(team.details.players.is_empty());
        // its archive to save, a match to import into it
        assert!(matches!(team.key(s, key!(x)), Outcome::Export));
        assert!(matches!(team.key(s, key!(i)), Outcome::Import));

        // seven players, then a new match: saved, it opens
        for n in 1..=7u8 {
            let player = PlayerForm {
                name: format!("P{n}"),
                number: n.to_string(),
                role: Some(scout4all_core::shapes::enums::RoleEnum::OutsideHitter),
                ..PlayerForm::default()
            };
            core.save_player(team_id.clone(), None, player, false)
                .unwrap();
        }
        team.key(s, key!(right));
        team.key(s, key!(n));
        team.update(s, Message::Form(Input(0, "Arezzo".into())));
        team.update(s, Message::Form(Input(1, "2026-11-05".into())));
        team.update(s, Message::Form(Input(2, "2030".into())));
        // the details, then the score sheet
        assert!(matches!(team.update(s, Message::Form(Save)), Outcome::Stay));
        let Outcome::OpenMatch(match_id) = team.update(s, Message::Form(Save)) else {
            unreachable!("the match is saved and opened")
        };
        let mut m = MatchScreen::new(core.clone(), team_id.clone(), match_id.clone()).unwrap();
        assert!(matches!(m.key(s, key!(enter)), game::Outcome::Scout(true)));
        assert!(matches!(m.key(s, BACK_KEY), game::Outcome::Back));
        assert!(matches!(m.key(s, key!(x)), game::Outcome::Export));
        // its statistics, with no rally yet: every page and filter goes
        assert!(matches!(m.key(s, key!(s)), game::Outcome::Stats));
        let mut stats = crate::stats::StatsScreen::new(core, team_id, match_id).unwrap();
        for k in [
            key!(tab),
            key!(right),
            key!(down),
            key!(left),
            key!(tab),
            key!(down),
            key!(right),
            key!(down),
            key!(right),
            key!(up),
            key!(left),
            key!(shift - tab),
        ] {
            assert!(matches!(stats.key(k), crate::stats::Outcome::Stay));
        }
        assert!(matches!(stats.key(BACK_KEY), crate::stats::Outcome::Back));
    }

    /// ⌫ asks to delete the match chosen, Esc doesn't, Enter does; ⇧⌫
    /// asks for the team, and the team deleted goes back to the home.
    #[test]
    fn a_match_and_the_team_are_deleted_once_confirmed() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        use scout4all_core::shapes::{
            country::CountryCode,
            enums::{GenderEnum, TeamClassificationEnum},
        };
        let form = scout4all_core::team_form::TeamForm {
            name: "Cortona".into(),
            classification: Some(TeamClassificationEnum::Regional),
            gender: Some(GenderEnum::Women),
            country: CountryCode::known("IT"),
            year: "2026".into(),
        };
        let team_id = core.save_team(None, form).unwrap();
        std::fs::create_dir(dir.path().join(&team_id).join("not-a-match")).unwrap();
        let mut team = TeamScreen::new(core.clone(), team_id.clone()).unwrap();
        assert_eq!(team.matches.len(), 1);
        team.key(s, key!(right));
        team.key(s, key!(backspace));
        assert!(matches!(team.mode, Mode::Delete(Deletion::Match(..))));
        assert!(
            team.hints(s)
                .iter()
                .any(|(_, what)| what == s.delete_forever)
        );
        team.key(s, BACK_KEY);
        assert!(matches!(team.mode, Mode::Browse));
        assert_eq!(team.matches.len(), 1);
        team.key(s, key!(delete));
        assert!(matches!(team.key(s, key!(enter)), Outcome::Stay));
        assert!(team.matches.is_empty());
        assert!(team.error.is_none());

        team.key(s, key!(shift - backspace));
        let Mode::Delete(deletion) = &team.mode else {
            unreachable!("the team to delete")
        };
        assert!(team.deletion_text(s, deletion).contains("Cortona"));
        team.update(s, Message::Cancel);
        assert!(matches!(team.mode, Mode::Browse));
        team.update(s, Message::DeleteTeam);
        assert!(matches!(team.update(s, Message::Confirm), Outcome::Back));
        assert!(!dir.path().join(&team_id).exists());
        assert!(core.home().unwrap().teams.is_empty());
    }

    /// A player with the name of another one is asked about first: Esc
    /// goes back to the form, Enter saves all the same, as the button does.
    #[test]
    fn a_namesake_is_saved_once_confirmed() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        use scout4all_core::shapes::{
            country::CountryCode,
            enums::{GenderEnum, RoleEnum, TeamClassificationEnum},
        };
        let form = scout4all_core::team_form::TeamForm {
            name: "Cortona".into(),
            classification: Some(TeamClassificationEnum::Regional),
            gender: Some(GenderEnum::Women),
            country: CountryCode::known("IT"),
            year: "2026".into(),
        };
        let team_id = core.save_team(None, form).unwrap();
        let anna = PlayerForm {
            name: "Anna Rossi".into(),
            number: "9".into(),
            role: Some(RoleEnum::Setter),
            ..PlayerForm::default()
        };
        core.save_player(team_id.clone(), None, anna, false)
            .unwrap();
        let mut team = TeamScreen::new(core.clone(), team_id).unwrap();
        let asking =
            |team: &TeamScreen| team.hints(s).iter().any(|(_, what)| what == s.save_anyway);
        team.update(s, Message::NewPlayer);
        for m in [
            Input(0, "anna rossi".into()),
            Input(1, "10".into()),
            Choose(2, 1),
        ] {
            team.update(s, Message::Form(m));
        }
        assert!(matches!(team.key(s, key!(enter)), Outcome::Stay));
        assert!(asking(&team));
        assert_eq!(team.details.players.len(), 1);
        // the other keys stay on the question
        team.key(s, key!(a));
        assert!(asking(&team));
        team.key(s, BACK_KEY);
        assert!(!asking(&team));
        assert!(matches!(team.mode, Mode::Edit(_)));
        team.update(s, Message::Form(Save));
        assert!(asking(&team));
        team.update(s, Message::Form(form::Message::Cancel));
        assert!(!asking(&team));
        team.key(s, key!(enter));
        team.key(s, key!(enter));
        assert!(matches!(team.mode, Mode::Browse));
        assert_eq!(team.details.players.len(), 2);
    }
}
