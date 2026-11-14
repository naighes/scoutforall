//! A team: what it is, its players and its matches side by side. The
//! players are added (new, or from the registry), edited and removed here
//! (from the team, or from the registry), the matches created, opened and
//! deleted, the team deleted or taken to a new season. ← → (or ⇧Tab Tab)
//! between players and matches, ↑ ↓ in the list, ⌫ deletes the one
//! chosen, ⇧⌫ the team, t starts the new season.

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
    scout4all_mobile::{MatchListing, MobileError, Scout4All, TeamDetails, TeamRemovalCounts},
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
    /// a player, from the team or from the registry
    Player(PlayerDeletion),
    /// id (or the folder of an unreadable one) and what it is
    Match(String, String),
    /// what happens to its matches
    Team(TeamRemovalCounts),
}

/// A player to remove: from the team, or from the registry (so from every
/// team: D16).
struct PlayerDeletion {
    id: String,
    /// number and name
    name: String,
    number: u8,
    /// the other teams they play in
    others: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    EditTeam,
    NewSeason,
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
    /// the player of the deletion asked removed from the registry
    DeletePerson,
    Cancel,
}

pub enum Outcome {
    Stay,
    Back,
    /// the match of the team to open, chosen
    OpenMatch(String),
    /// the match saved now, of its team A (maybe another one)
    OpenNewMatch {
        team: String,
        id: String,
    },
    /// an archive of a match to choose, into the team
    Import,
    /// the archive of the team, to save
    Export,
    /// the statistics of the team over its matches
    Analysis,
    /// a team saved now (the new season), to open
    OpenTeam(String),
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
            // a new one: the first number free
            player.map(|p| p.form.clone()).unwrap_or_else(|| {
                self.core
                    .new_player_form(self.team_id.clone())
                    .unwrap_or_default()
            }),
        );
        self.edit(Ok(editor));
    }

    /// The new season of the team (D15), a step at a time.
    fn new_season(&mut self, s: &Strings) {
        let editor = Editor::season(&self.core, s, self.team_id.clone());
        self.edit(editor);
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
    fn ask_deletion(&mut self, s: &Strings) {
        let deletion = match self.panel {
            Panel::Players => self.details.players.get(self.player).map(|p| {
                Deletion::Player(PlayerDeletion {
                    id: p.id.clone(),
                    name: format!("{} {}", p.number, p.name),
                    number: p.number,
                    others: self
                        .core
                        .other_teams(self.team_id.clone(), p.id.clone())
                        .unwrap_or_default(),
                })
            }),
            Panel::Matches => self.matches.get(self.game).map(|m| match m {
                MatchListing::Match {
                    id, opponent, date, ..
                } => Deletion::Match(id.clone(), format!("{opponent} · {}", s.day(date))),
                MatchListing::Unreadable { folder, .. } => {
                    Deletion::Match(folder.clone(), folder.clone())
                }
            }),
        };
        if let Some(deletion) = deletion {
            self.mode = Mode::Delete(deletion);
        }
    }

    /// The deletion of the team, to confirm with what happens to its
    /// matches.
    fn ask_team_deletion(&mut self) {
        match self.core.team_removal(self.team_id.clone()) {
            Ok(counts) => self.mode = Mode::Delete(Deletion::Team(counts)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Deleted as confirmed: the team gone, back to the home.
    fn delete(&mut self) -> Outcome {
        let mode = std::mem::replace(&mut self.mode, Mode::Browse);
        let team = self.team_id.clone();
        let deleted = match &mode {
            Mode::Delete(Deletion::Player(p)) => self.core.remove_player(team, p.id.clone()),
            Mode::Delete(Deletion::Match(id, _)) => self.core.delete_match(team, id.clone()),
            Mode::Delete(Deletion::Team(_)) => self.core.delete_team(team),
            Mode::Browse | Mode::Edit(_) => Ok(()),
        };
        match (deleted, mode) {
            (Ok(()), Mode::Delete(Deletion::Team(_))) => Outcome::Back,
            (deleted, _) => {
                self.error = deleted.err().map(|e| e.to_string());
                self.reload();
                Outcome::Stay
            }
        }
    }

    /// The player of the deletion asked removed from the registry, so from
    /// every team, as confirmed.
    fn delete_person(&mut self) -> Outcome {
        let mode = std::mem::replace(&mut self.mode, Mode::Browse);
        if let Mode::Delete(Deletion::Player(p)) = &mode {
            self.error = self
                .core
                .delete_person(p.id.clone())
                .err()
                .map(|e| e.to_string());
            self.reload();
        }
        Outcome::Stay
    }

    /// What the question of the deletion says.
    fn deletion_text(&self, s: &Strings, deletion: &Deletion) -> String {
        match deletion {
            Deletion::Player(p) => {
                let text = ui::fill(s.delete_player_text, std::slice::from_ref(&p.name));
                let others = match p.others.is_empty() {
                    true => String::new(),
                    false => format!("\n{}", ui::fill(s.plays_also_in, &[p.others.join(", ")])),
                };
                let registry = ui::fill(s.delete_person_text, &[p.number.to_string()]);
                format!("{text}{others}\n\n{registry}")
            }
            Deletion::Match(_, name) => ui::fill(s.delete_match_text, std::slice::from_ref(name)),
            Deletion::Team(counts) => {
                let name = self.details.name.clone();
                let text = ui::fill(
                    s.delete_team_text,
                    &[name.clone(), counts.removed.to_string()],
                );
                match counts.kept {
                    0 => text,
                    kept => format!(
                        "{text} {}",
                        ui::fill(s.delete_team_kept, &[kept.to_string(), name])
                    ),
                }
            }
        }
    }

    /// After the form: saved, the team is read again; a new match opens.
    fn edited(&mut self, edited: Edited) -> Outcome {
        let new_match = match &self.mode {
            Mode::Edit(e) if e.is_new_match() => e.match_team().map(str::to_string),
            _ => None,
        };
        let season = matches!(&self.mode, Mode::Edit(e) if e.is_season());
        match edited {
            Edited::Stay => Outcome::Stay,
            Edited::Cancel => {
                self.mode = Mode::Browse;
                Outcome::Stay
            }
            Edited::Saved(id) => {
                self.mode = Mode::Browse;
                self.reload();
                match (new_match, season) {
                    (Some(team), _) => Outcome::OpenNewMatch { team, id },
                    (None, true) => Outcome::OpenTeam(id),
                    (None, false) => Outcome::Stay,
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
            (Message::DeletePerson, Mode::Delete(Deletion::Player(_))) => self.delete_person(),
            (Message::Cancel, _) => {
                self.mode = Mode::Browse;
                Outcome::Stay
            }
            (_, Mode::Edit(_) | Mode::Delete(_)) => Outcome::Stay,
            (Message::EditTeam, Mode::Browse) => {
                self.edit_team(s);
                Outcome::Stay
            }
            (Message::NewSeason, Mode::Browse) => {
                self.new_season(s);
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
                self.ask_team_deletion();
                Outcome::Stay
            }
            (Message::Form(_) | Message::Confirm | Message::DeletePerson, Mode::Browse) => {
                Outcome::Stay
            }
        }
    }

    pub fn key(&mut self, s: &Strings, ck: KeyCombination) -> Outcome {
        if let Mode::Edit(editor) = &mut self.mode {
            let edited = editor.key(&self.core, s, ck);
            return self.edited(edited);
        }
        if let Mode::Delete(deletion) = &self.mode {
            let player = matches!(deletion, Deletion::Player(_));
            return match ck {
                k if k == key!(enter) => self.delete(),
                k if k == key!(a) && player => self.delete_person(),
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
            (_, k) if k == key!(t) => self.new_season(s),
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
                self.ask_team_deletion()
            }
            (_, k) if (k == key!(backspace) || k == key!(delete)) && count > 0 => {
                self.ask_deletion(s)
            }
            _ => {}
        }
        Outcome::Stay
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match (&self.mode, self.panel) {
            (Mode::Edit(editor), _) => editor.hints(s),
            (Mode::Delete(Deletion::Player(_)), _) => vec![
                ("Enter".to_string(), s.remove_from_team.to_string()),
                ("a".to_string(), s.delete_person.to_string()),
                ("Esc".to_string(), s.cancel.to_string()),
            ],
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
                        ("t".to_string(), s.new_season.to_string()),
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
            Mode::Delete(deletion @ Deletion::Player(_)) => ui::confirm_deletion_with(
                s,
                &self.deletion_text(s, deletion),
                vec![(theme::icons::DELETE, s.delete_person, Message::DeletePerson)],
                s.remove_from_team,
                Message::Confirm,
                Message::Cancel,
                c,
            ),
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
            // the actions scroll when the window is low: none cut off
            scrollable(
                column![
                    text(d.name.clone()).size(22).font(theme::fonts().bold),
                    line(s.year, d.year.to_string()),
                    line(s.classification, or_dash(&d.classification)),
                    line(s.gender, or_dash(&d.gender)),
                    line(s.country, or_dash(&d.country)),
                    space::vertical().height(6),
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
                        theme::icons::FORWARD_ARROW,
                        s.new_season,
                        Message::NewSeason,
                        false,
                        c
                    )
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
                .width(Length::Fill),
            )
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
        let rows =
            self.matches.iter().enumerate().map(|(i, m)| match m {
                MatchListing::Match {
                    opponent,
                    home,
                    date,
                    won,
                    lost,
                    finished,
                    scouted_by,
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
                                text(s.day(date)).size(SIZE_SMALL).color(c.muted),
                                text(match finished {
                                    true => String::new(),
                                    false => format!("· {}", s.in_progress),
                                })
                                .size(SIZE_SMALL)
                                .color(c.accent),
                            ]
                            .spacing(6),
                        ]
                        .push(scouted_by.as_ref().map(|_| {
                            text(s.scouted_by_opponent).size(SIZE_SMALL).color(c.muted)
                        }))
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
        chrono::DateTime,
        form::Message::{Choose, Input, Save, Take},
        scout4all_core::{
            match_form::MatchForm,
            player_form::PlayerForm,
            shapes::{
                country::CountryCode,
                enums::{GenderEnum, LanguageEnum, TeamClassificationEnum},
            },
            team_form::TeamForm,
        },
        scout4all_mobile::Player,
        uuid::Uuid,
    };

    fn app(dir: &tempfile::TempDir) -> Arc<Scout4All> {
        Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap()
    }

    fn team_form(name: &str) -> TeamForm {
        TeamForm {
            name: name.into(),
            classification: Some(TeamClassificationEnum::Regional),
            gender: Some(GenderEnum::Women),
            country: CountryCode::known("IT"),
            year: "2026".into(),
        }
    }

    /// A match between two teams (TODO/001, step 21): in the list of B,
    /// marked as scouted by the opponent; opened from B, only its result,
    /// Enter opens it as A; removing a team tells which matches stay.
    #[test]
    fn a_match_of_b_is_listed_and_opens_as_a() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let a = core.save_team(None, team_form("Cortona")).unwrap();
        let b = core.save_team(None, team_form("Cascina")).unwrap();
        for n in 1..=7u8 {
            let player = PlayerForm {
                name: format!("P{n}"),
                number: n.to_string(),
                role: Some(scout4all_core::shapes::enums::RoleEnum::OutsideHitter),
                ..PlayerForm::default()
            };
            core.save_player(a.clone(), None, player, false).unwrap();
        }
        let form = |opponent: &str, team: Option<Uuid>| MatchForm {
            opponent: opponent.into(),
            opponent_team: team,
            date: DateTime::parse_from_rfc3339("2026-10-18T18:00:00+02:00").ok(),
            home: true,
            max_substitutions: "8".into(),
            ..MatchForm::default()
        };
        let m = core
            .save_match(a.clone(), None, form("Cascina", Uuid::parse_str(&b).ok()))
            .unwrap();
        core.save_match(a.clone(), None, form("Arezzo", None))
            .unwrap();

        let mut team = TeamScreen::new(core.clone(), b.clone()).unwrap();
        assert!(matches!(
            team.matches.as_slice(),
            [MatchListing::Match { opponent, scouted_by: Some(of), .. }]
                if opponent == "Cortona" && of == &a
        ));
        team.key(s, key!(right));
        assert!(matches!(team.key(s, key!(enter)), Outcome::OpenMatch(id) if id == m));

        let mut seen = MatchScreen::new(core.clone(), b.clone(), m.clone()).unwrap();
        assert_eq!(seen.title(), "Cascina – Cortona");
        // nothing to scout nor to look into from B's side
        for k in [
            key!(s),
            key!(e),
            key!(x),
            key!(p),
            key!(v),
            key!(r),
            key!(1),
        ] {
            assert!(matches!(seen.key(s, k), game::Outcome::Stay));
        }
        for message in [
            game::Message::Stats,
            game::Message::Next,
            game::Message::Set(1),
        ] {
            assert!(matches!(seen.update(s, message), game::Outcome::Stay));
        }
        assert_eq!(
            seen.hints(s),
            vec![
                ("Enter".to_string(), "Apri come Cortona".to_string()),
                ("Esc".to_string(), s.back.to_string())
            ]
        );
        assert!(matches!(seen.key(s, key!(enter)), game::Outcome::OpenAs(of) if of == a));
        assert!(matches!(seen.key(s, BACK_KEY), game::Outcome::Back));
        // from A: its own match
        let mut of_a = MatchScreen::new(core.clone(), a.clone(), m).unwrap();
        assert!(matches!(of_a.key(s, key!(s)), game::Outcome::Stats));

        // removing B: its match stays to A; removing A: one stays, one goes
        team.key(s, key!(shift - backspace));
        let Mode::Delete(deletion) = &team.mode else {
            unreachable!("the deletion of the team, to confirm")
        };
        assert_eq!(
            team.deletion_text(s, deletion),
            "La squadra Cascina sarà eliminata con le sue partite (0): scouting, report e \
             statistiche. I suoi giocatori restano nell'anagrafica. Le partite contro altre \
             squadre (1) restano a loro, senza lo scouting di Cascina."
        );
        let mut team = TeamScreen::new(core.clone(), a).unwrap();
        team.key(s, key!(shift - delete));
        let Mode::Delete(deletion) = &team.mode else {
            unreachable!("the deletion of the team, to confirm")
        };
        assert!(
            team.deletion_text(s, deletion)
                .contains("le sue partite (1)")
        );
        team.key(s, key!(enter));
        let listed = TeamScreen::new(core, b).unwrap().matches;
        assert!(matches!(
            listed.as_slice(),
            [MatchListing::Match { opponent, scouted_by: None, .. }] if opponent == "Cortona"
        ));
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
        // the first number free, how to write it and the height said in
        // the empty fields
        let Mode::Edit(editor) = &team.mode else {
            unreachable!("the form of the player is open")
        };
        assert_eq!(editor.form.text(1), "1");
        assert_eq!(editor.form.fields[1].placeholder, s.number_placeholder);
        assert_eq!(editor.form.fields[4].placeholder, s.height_placeholder);
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
        team.update(s, Message::Form(Input(1, "Arezzo".into())));
        team.update(s, Message::Form(Input(3, "2026-11-05".into())));
        team.update(s, Message::Form(Input(4, "2030".into())));
        // the details, the competition, then the score sheet
        assert!(matches!(team.update(s, Message::Form(Save)), Outcome::Stay));
        assert!(matches!(team.update(s, Message::Form(Save)), Outcome::Stay));
        let Outcome::OpenNewMatch {
            team: of,
            id: match_id,
        } = team.update(s, Message::Form(Save))
        else {
            unreachable!("the match is saved and opened")
        };
        assert_eq!(of, team_id);
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
        // the example team, with players enough for a match
        let team_id = core.tutorial_team().unwrap();
        let date = chrono::DateTime::parse_from_rfc3339("2026-10-24T18:00:00+02:00").unwrap();
        let m = core.tutorial_match_form(team_id.clone(), date).unwrap();
        core.save_match(team_id.clone(), None, m).unwrap();
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
        assert!(team.deletion_text(s, deletion).contains("Italia"));
        team.update(s, Message::Cancel);
        assert!(matches!(team.mode, Mode::Browse));
        team.update(s, Message::DeleteTeam);
        assert!(matches!(team.update(s, Message::Confirm), Outcome::Back));
        assert!(core.home().unwrap().teams.is_empty());
    }

    /// A team of 2025 with Anna Rossi (9, setter, with her details) and
    /// six more players; its id and Anna's.
    fn senior(core: &Scout4All) -> (String, String) {
        let team = core
            .save_team(
                None,
                TeamForm {
                    year: "2025".into(),
                    ..team_form("Cortona")
                },
            )
            .unwrap();
        let anna = PlayerForm {
            name: "Anna Rossi".into(),
            number: "9".into(),
            role: Some(scout4all_core::shapes::enums::RoleEnum::Setter),
            birth_date: "2008-03-14".into(),
            height_cm: "181".into(),
            notes: "mancina".into(),
            ..PlayerForm::default()
        };
        let anna = core.save_player(team.clone(), None, anna, false).unwrap();
        for n in 1..=6u8 {
            let player = PlayerForm {
                name: format!("P{n}"),
                number: n.to_string(),
                role: Some(scout4all_core::shapes::enums::RoleEnum::OutsideHitter),
                ..PlayerForm::default()
            };
            core.save_player(team.clone(), None, player, false).unwrap();
        }
        (team, anna)
    }

    /// The name of a new player offers the persons of the registry with
    /// their teams; Enter takes one: their details, number and role, and
    /// saved, the same person in one more team. A name written over lets
    /// the person go: a new one.
    #[test]
    fn a_player_of_the_registry_is_added_by_name() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let (senior, anna) = senior(&core);
        let under = core.save_team(None, team_form("Cortona U19")).unwrap();
        let mut team = TeamScreen::new(core.clone(), under.clone()).unwrap();
        team.key(s, key!(n));
        let Mode::Edit(editor) = &team.mode else {
            unreachable!("the form of the new player")
        };
        assert_eq!(
            editor.form.fields[0].hint.as_deref(),
            Some(s.name_or_existing)
        );
        team.update(s, Message::Form(Input(0, "ross".into())));
        let Mode::Edit(editor) = &team.mode else {
            unreachable!("the form of the new player")
        };
        let form::Value::Search { suggestions, .. } = &editor.form.fields[0].value else {
            unreachable!("a search")
        };
        assert_eq!(
            suggestions,
            &[form::Suggestion {
                label: "Anna Rossi (Cortona 2025)".into(),
                value: "Anna Rossi".into(),
            }]
        );
        // "ross" isn't the beginning: ↓ lights it, Enter takes it
        team.key(s, key!(down));
        team.key(s, key!(enter));
        let Mode::Edit(editor) = &team.mode else {
            unreachable!("still the form")
        };
        let f = &editor.form;
        assert_eq!(
            (f.text(0), f.text(1), f.text(3), f.text(4), f.text(5)),
            (
                "Anna Rossi".to_string(),
                "9".to_string(),
                "2008-03-14".to_string(),
                "181".to_string(),
                "mancina".to_string()
            )
        );
        assert!(f.index(2) > 0);
        assert_eq!(
            f.fields[0].hint.as_deref(),
            Some(
                "Già nell'anagrafica: la stessa persona, con gli stessi dati · Gioca anche in: \
                 Cortona 2025"
            )
        );
        team.update(s, Message::Form(Input(1, "10".into())));
        team.update(s, Message::Form(Save));
        assert!(matches!(team.mode, Mode::Browse));
        let added = &core.team(under.clone()).unwrap().players;
        assert_eq!(
            added
                .iter()
                .map(|p| (p.id.as_str(), p.number))
                .collect::<Vec<_>>(),
            [(anna.as_str(), 10)]
        );
        assert_eq!(core.team(senior.clone()).unwrap().players[6].number, 9);
        // once added, the other team is told in the form; not offered again
        team.key(s, key!(enter));
        let Mode::Edit(editor) = &team.mode else {
            unreachable!("the form of Anna")
        };
        assert_eq!(
            editor.form.fields[0].hint.as_deref(),
            Some("Gioca anche in: Cortona (2025)")
        );
        team.key(s, BACK_KEY);
        assert!(
            core.existing_players(under.clone())
                .unwrap()
                .iter()
                .all(|p| p.id.to_string() != anna)
        );

        // taken with the mouse, then written over: a new person
        team.update(s, Message::NewPlayer);
        team.update(s, Message::Form(Input(0, "P".into())));
        team.update(s, Message::Form(Take(0, 0)));
        let Mode::Edit(editor) = &team.mode else {
            unreachable!("the form")
        };
        assert_eq!(editor.form.text(0), "P1");
        team.update(s, Message::Form(Input(0, "P1 bis".into())));
        team.update(s, Message::Form(Input(1, "11".into())));
        team.update(s, Message::Form(Save));
        let players = core.team(under).unwrap().players;
        let ids: Vec<String> = core
            .team(senior)
            .unwrap()
            .players
            .iter()
            .map(|p| p.id.clone())
            .collect();
        assert!(matches!(
            players.as_slice(),
            [_, Player { name, number: 11, id, .. }] if name == "P1 bis" && !ids.contains(id)
        ));
    }

    /// ⌫ on a player: Enter removes them from the team only (still in the
    /// other one, offered again); "a" from the registry, so from every
    /// team.
    #[test]
    fn a_player_is_removed_from_the_team_or_from_the_registry() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let (senior, anna) = senior(&core);
        let under = core.save_team(None, team_form("Cortona U19")).unwrap();
        let form = PlayerForm {
            number: "10".into(),
            person: Uuid::parse_str(&anna).ok(),
            ..core.team(senior.clone()).unwrap().players[6].form.clone()
        };
        core.save_player(under.clone(), None, form, false).unwrap();

        let mut team = TeamScreen::new(core.clone(), under.clone()).unwrap();
        team.key(s, key!(backspace));
        let Mode::Delete(deletion) = &team.mode else {
            unreachable!("the deletion to confirm")
        };
        assert_eq!(
            team.deletion_text(s, deletion),
            "Il giocatore 10 Anna Rossi sarà tolto dalla squadra. Resta nelle partite già \
             giocate e nell'anagrafica, da cui si può aggiungere di nuovo.\nGioca anche in: \
             Cortona (2025)\n\nEliminato dall'anagrafica, sparisce da tutte le squadre e i \
             suoi dati si cancellano; nelle partite giocate resta come «#10, giocatore \
             rimosso»."
        );
        assert_eq!(
            team.hints(s),
            vec![
                ("Enter".to_string(), s.remove_from_team.to_string()),
                ("a".to_string(), s.delete_person.to_string()),
                ("Esc".to_string(), s.cancel.to_string()),
            ]
        );
        // the other keys stay on the question
        team.key(s, key!(x));
        assert!(matches!(team.mode, Mode::Delete(_)));
        team.key(s, key!(enter));
        assert!(team.details.players.is_empty());
        assert_eq!(core.team(senior.clone()).unwrap().players.len(), 7);
        let offered = core.existing_players(under.clone()).unwrap();
        assert!(offered.iter().any(|p| p.id.to_string() == anna));

        // from the registry: gone from the senior team too
        let mut team = TeamScreen::new(core.clone(), senior.clone()).unwrap();
        team.player = 6;
        team.key(s, key!(delete));
        assert!(matches!(
            team.update(s, Message::DeletePerson),
            Outcome::Stay
        ));
        assert!(team.error.is_none());
        assert_eq!(team.details.players.len(), 6);
        assert!(team.details.players.iter().all(|p| p.id != anna));
        assert!(
            core.existing_players(under)
                .unwrap()
                .iter()
                .all(|p| p.id.to_string() != anna)
        );
        // "a" is the deletion from the registry only for a player
        team.key(s, key!(right));
        team.key(s, key!(shift - backspace));
        team.key(s, key!(a));
        assert!(matches!(team.mode, Mode::Delete(Deletion::Team(_))));
    }

    /// "t" starts the new season: the team the year after, then its
    /// players, all taken; space leaves one, "e" corrects a number; a
    /// number taken twice refused there. Saved, the new team opens with
    /// the same persons; the old one is as it was.
    #[test]
    fn a_new_season_takes_the_players_chosen() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let (senior, anna) = senior(&core);
        let before = core.team(senior.clone()).unwrap();
        let mut team = TeamScreen::new(core.clone(), senior.clone()).unwrap();
        assert!(
            team.hints(s)
                .contains(&("t".to_string(), s.new_season.to_string()))
        );
        team.key(s, key!(t));
        let Mode::Edit(editor) = &team.mode else {
            unreachable!("the new season")
        };
        assert_eq!(editor.form.title, s.new_season);
        assert_eq!(editor.form.visible(), vec![0, 1, 2, 3, 4]);
        assert_eq!(
            (editor.form.text(0), editor.form.text(1)),
            ("Cortona".to_string(), "2026".to_string())
        );
        assert!(
            editor
                .hints(s)
                .contains(&("Enter".to_string(), s.go_on.to_string()))
        );
        team.key(s, key!(enter));
        let Mode::Edit(editor) = &team.mode else {
            unreachable!("the players of the new season")
        };
        // seven tiles, by number: Anna is the last
        assert_eq!(editor.form.visible(), (5..12).collect::<Vec<_>>());
        assert_eq!(editor.form.focused, 5);
        assert!(
            editor
                .hints(s)
                .contains(&(s.space.to_string(), s.season_toggle.to_string()))
        );
        // P1 left, P2 given number 9... taken by Anna: refused
        team.key(s, key!(space));
        team.key(s, key!(right));
        team.key(s, key!(e));
        team.update(s, Message::Form(Input(0, "9".into())));
        team.key(s, key!(enter));
        let Mode::Edit(editor) = &team.mode else {
            unreachable!("still the correction")
        };
        assert!(
            editor
                .hints(s)
                .contains(&("Enter".to_string(), s.ok.to_string()))
        );
        // 1, P1's, is free: P1 isn't taken
        team.update(s, Message::Form(Input(0, "1".into())));
        team.key(s, key!(enter));
        // P1 taken back: two with the 1, refused on saving
        team.update(s, Message::Form(Choose(5, 1)));
        assert!(matches!(team.key(s, key!(enter)), Outcome::Stay));
        let Mode::Edit(editor) = &team.mode else {
            unreachable!("still the players")
        };
        assert_eq!(editor.form.focused, 5);
        assert!(editor.form.error.is_some());
        team.update(s, Message::Form(Choose(5, 0)));
        // Esc goes back to the team, Enter on again
        team.key(s, BACK_KEY);
        team.update(s, Message::Form(Input(1, "2027".into())));
        team.key(s, key!(enter));
        let Outcome::OpenTeam(new) = team.update(s, Message::Form(Save)) else {
            unreachable!("the new team opens")
        };
        let new = core.team(new).unwrap();
        assert_eq!((new.name.as_str(), new.year), ("Cortona", 2027));
        assert_eq!(new.classification, before.classification);
        let players: Vec<(u8, &str)> = new
            .players
            .iter()
            .map(|p| (p.number, p.name.as_str()))
            .collect();
        assert_eq!(
            players,
            [
                (1, "P2"),
                (3, "P3"),
                (4, "P4"),
                (5, "P5"),
                (6, "P6"),
                (9, "Anna Rossi")
            ]
        );
        assert_eq!(new.players[5].id, anna);
        assert_eq!(new.players[5].form.height_cm, "181");
        assert_eq!(core.team(senior.clone()).unwrap(), before);
        assert_eq!(
            core.other_teams(senior, anna).unwrap(),
            ["Cortona (2027)".to_string()]
        );
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
