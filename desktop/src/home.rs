//! The home: the matches in progress, the recent ones and the teams side by
//! side. ← → (or ⇧Tab Tab) between them, ↑ ↓ in the list, Enter opens: a
//! match in progress straight to its set, a recent one, a team; `n` a new
//! team, `c` the competitions.

use scout4all_mobile::clock::ClockExt;
use {
    crate::{
        agenda::{self, Agenda, Pick},
        editors::{Edited, Editor},
        form,
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, Id, column, container, row, scrollable, text},
    },
    scout4all_keys::{KeyCombination, key},
    scout4all_mobile::{Home, HomeMatch, NextSetKind, Scout4All, TeamListing},
};

#[derive(Debug, Clone)]
pub struct Open {
    pub team_id: String,
    pub match_id: String,
    /// a set to start, not one in progress
    pub start: bool,
}

/// What the match goes on with: a set to start or to resume; nothing once
/// it's over.
pub fn openable(m: &HomeMatch) -> Option<Open> {
    let open = |start| {
        Some(Open {
            team_id: m.team_id.clone(),
            match_id: m.id.clone(),
            start,
        })
    };
    match m.next {
        NextSetKind::Resume { .. } => open(false),
        NextSetKind::Start { .. } => open(true),
        NextSetKind::Over => None,
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    /// a match in progress: scouted
    Scout(Open),
    /// a recent match: its page
    Match(String, String),
    Team(String),
    NewTeam,
    Import,
    Competitions,
    Venues,
    Form(form::Message),
    /// the agenda of the week
    Week(agenda::Message),
}

pub enum Outcome {
    Stay,
    Scout(Open),
    Match(String, String),
    Team(String),
    /// the training of a team: a session or a new one
    Training(String, crate::training::screen::Open),
    /// an archive or a DataVolley file to choose
    Import,
    Competitions,
    Venues,
}

const WEEK: usize = 0;
const IN_PROGRESS: usize = 1;
const RECENT: usize = 2;
const TEAMS: usize = 3;
const PANELS: usize = 4;

pub struct HomeScreen {
    pub home: Home,
    week: Agenda,
    panel: usize,
    selected: [usize; PANELS],
    /// a new team, being written
    editor: Option<Box<Editor>>,
    /// the logos of the teams of the list, in its order
    logos: Vec<Option<ui::TeamLogo>>,
}

fn team_id(t: &TeamListing) -> Option<String> {
    match t {
        TeamListing::Team { id, .. } => Some(id.clone()),
        TeamListing::Unreadable { .. } => None,
    }
}

impl HomeScreen {
    pub fn new(core: &Scout4All, home: Home) -> HomeScreen {
        let logos = home
            .teams
            .iter()
            .map(|t| match t {
                TeamListing::Team { id, .. } => core
                    .team_logo(id.clone())
                    .ok()
                    .flatten()
                    .and_then(|png| ui::TeamLogo::from_png(&png)),
                TeamListing::Unreadable { .. } => None,
            })
            .collect();
        HomeScreen {
            logos,
            home,
            week: Agenda::week(core, None, core.clock().today()),
            panel: IN_PROGRESS,
            selected: [0; PANELS],
            editor: None,
        }
    }

    pub fn focus_id(&self) -> Option<Id> {
        self.editor.as_ref().map(|e| e.focus_id())
    }

    fn count(&self, panel: usize) -> usize {
        match panel {
            IN_PROGRESS => self.home.in_progress.len(),
            RECENT => self.home.recent.len(),
            TEAMS => self.home.teams.len(),
            _ => 0,
        }
    }

    /// The panel after (or before) the one chosen.
    fn step(&self, forward: bool) -> usize {
        let one = match forward {
            true => 1,
            false => PANELS - 1,
        };
        (self.panel + one) % PANELS
    }

    /// What the row picked in an agenda opens: a match or a session.
    fn picked(pick: Option<Pick>) -> Outcome {
        match pick {
            Some(Pick::Match { team_id, id }) => Outcome::Match(team_id, id),
            Some(Pick::Session { team_id, id }) => {
                Outcome::Training(team_id, crate::training::screen::Open::Session(id))
            }
            None => Outcome::Stay,
        }
    }

    fn new_team(&mut self, core: &Scout4All, s: &Strings) {
        let year = chrono::Datelike::year(&core.clock().today());
        let form = core.new_team_form(u16::try_from(year).unwrap_or(2026));
        self.editor = Some(Box::new(Editor::team(core, s, None, form)));
    }

    fn edited(&mut self, edited: Edited) -> Outcome {
        match edited {
            Edited::Stay | Edited::Crop(_) => Outcome::Stay,
            Edited::Cancel => {
                self.editor = None;
                Outcome::Stay
            }
            Edited::Saved(id) => {
                self.editor = None;
                Outcome::Team(id)
            }
        }
    }

    /// What the row of `panel` at `index` opens.
    fn open(&self, panel: usize, index: usize) -> Outcome {
        match panel {
            WEEK => Outcome::Stay,
            IN_PROGRESS => self
                .home
                .in_progress
                .get(index)
                .and_then(openable)
                .map_or(Outcome::Stay, Outcome::Scout),
            RECENT => self.home.recent.get(index).map_or(Outcome::Stay, |m| {
                Outcome::Match(m.team_id.clone(), m.id.clone())
            }),
            _ => self
                .home
                .teams
                .get(index)
                .and_then(team_id)
                .map_or(Outcome::Stay, Outcome::Team),
        }
    }

    pub fn update(&mut self, core: &Scout4All, s: &Strings, message: Message) -> Outcome {
        match (message, &mut self.editor) {
            (Message::Form(m), Some(editor)) => {
                let edited = editor.update(core, s, m);
                self.edited(edited)
            }
            (_, Some(_)) | (Message::Form(_), None) => Outcome::Stay,
            (Message::Week(m), None) => HomeScreen::picked(self.week.update(core, m)),
            (Message::Scout(open), None) => Outcome::Scout(open),
            (Message::Match(team, m), None) => Outcome::Match(team, m),
            (Message::Team(team), None) => Outcome::Team(team),
            (Message::Import, None) => Outcome::Import,
            (Message::Competitions, None) => Outcome::Competitions,
            (Message::Venues, None) => Outcome::Venues,
            (Message::NewTeam, None) => {
                self.new_team(core, s);
                Outcome::Stay
            }
        }
    }

    pub fn key(&mut self, core: &Scout4All, s: &Strings, ck: KeyCombination) -> Outcome {
        if let Some(editor) = &mut self.editor {
            let edited = editor.key(core, s, ck);
            return self.edited(edited);
        }
        let count = self.count(self.panel);
        let next = ck == key!(right) || ck == key!(tab);
        let previous = ck == key!(left)
            || ck == key!(shift - tab)
            || ck == key!(shift - backtab)
            || ck == key!(backtab);
        let selected = self.selected.get(self.panel).copied().unwrap_or(0);
        if self.panel == WEEK
            && !(next || previous || ck == key!(n) || ck == key!(i) || ck == key!(c))
        {
            return HomeScreen::picked(self.week.key(core, ck));
        }
        let moved = match ck {
            _ if next => {
                self.panel = self.step(true);
                None
            }
            _ if previous => {
                self.panel = self.step(false);
                None
            }
            k if k == key!(down) && count > 0 => Some((selected + 1) % count),
            k if k == key!(up) && count > 0 => Some(selected.checked_sub(1).unwrap_or(count - 1)),
            k if k == key!(enter) => return self.open(self.panel, selected),
            k if k == key!(n) => {
                self.new_team(core, s);
                None
            }
            k if k == key!(i) => return Outcome::Import,
            k if k == key!(c) => return Outcome::Competitions,
            k if k == key!(g) => return Outcome::Venues,
            _ => None,
        };
        if let (Some(index), Some(slot)) = (moved, self.selected.get_mut(self.panel)) {
            *slot = index;
        }
        Outcome::Stay
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match &self.editor {
            Some(editor) => editor.hints(s),
            None if self.panel == WEEK => [
                vec![("← → ⇧⇥ ⇥".to_string(), s.panel.to_string())],
                self.week.hints(s),
                vec![
                    ("Enter".to_string(), s.open.to_string()),
                    ("n".to_string(), s.new_team.to_string()),
                    ("i".to_string(), s.import_archive.to_string()),
                    ("c".to_string(), s.competitions.to_string()),
                    ("g".to_string(), s.venues.to_string()),
                    ("F2".to_string(), s.settings.to_string()),
                ],
            ]
            .concat(),
            None => vec![
                ("← → ⇧⇥ ⇥".to_string(), s.panel.to_string()),
                ("↑ ↓".to_string(), String::new()),
                ("Enter".to_string(), s.open.to_string()),
                ("n".to_string(), s.new_team.to_string()),
                ("i".to_string(), s.import_archive.to_string()),
                ("c".to_string(), s.competitions.to_string()),
                ("g".to_string(), s.venues.to_string()),
                ("F2".to_string(), s.settings.to_string()),
            ],
        }
    }

    #[cfg(test)]
    pub fn logos_shown(&self) -> Vec<bool> {
        self.logos.iter().map(Option::is_some).collect()
    }

    #[cfg(test)]
    pub fn week_shown(&self) -> Vec<(String, bool)> {
        self.week.shown_days()
    }

    #[cfg(test)]
    pub fn panel_name(&self) -> &'static str {
        match self.panel {
            WEEK => "week",
            IN_PROGRESS => "in progress",
            RECENT => "recent",
            _ => "teams",
        }
    }

    fn is_selected(&self, panel: usize, index: usize) -> bool {
        self.panel == panel && self.selected.get(panel) == Some(&index)
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        if let Some(editor) = &self.editor {
            return editor.view(s, c, Message::Form);
        }
        let in_progress: Vec<Element<'a, Message>> = self
            .home
            .in_progress
            .iter()
            .enumerate()
            .map(|(i, m)| {
                match_row(
                    m,
                    s,
                    self.is_selected(IN_PROGRESS, i),
                    openable(m).map(Message::Scout),
                    c,
                )
            })
            .collect();
        let recent = self
            .home
            .recent
            .iter()
            .enumerate()
            .map(|(i, m)| {
                match_row(
                    m,
                    s,
                    self.is_selected(RECENT, i),
                    Some(Message::Match(m.team_id.clone(), m.id.clone())),
                    c,
                )
            })
            .collect();
        let teams = self
            .home
            .teams
            .iter()
            .enumerate()
            .map(|(i, t)| match t {
                TeamListing::Team {
                    id,
                    name,
                    year,
                    players,
                    pinned,
                } => ui::list_row(
                    row![
                        text(match pinned {
                            true => "★",
                            false => "☆",
                        })
                        .size(SIZE_BODY)
                        .color(match pinned {
                            true => c.accent,
                            false => c.muted,
                        })
                        .width(Length::Fixed(22.0)),
                        ui::team_logo_slot(self.logos.get(i).and_then(Option::as_ref), 28.0, c),
                        text(name.clone())
                            .size(SIZE_BODY)
                            .font(theme::fonts().bold)
                            .width(Length::Fill),
                        text(format!("{year} · {players} {}", s.players))
                            .size(SIZE_SMALL)
                            .color(c.muted),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                    self.is_selected(TEAMS, i),
                    Some(Message::Team(id.clone())),
                    c,
                ),
                TeamListing::Unreadable { folder } => {
                    text(folder.clone()).size(SIZE_SMALL).color(c.muted).into()
                }
            })
            .collect();
        let actions = vec![
            ui::pill(
                &format!("↓ {}", s.import_archive),
                false,
                Some(Message::Import),
                c,
            ),
            ui::add_button(s.new_team, Message::NewTeam, c),
        ];
        // the first column: the week (today lit in it), the matches in
        // progress; these, none, are a line
        let week = container(
            self.week
                .view(s, c, self.panel == WEEK, true)
                .map(Message::Week),
        )
        .height(Length::FillPortion(2));
        let in_progress: Element<'a, Message> = match in_progress.is_empty() {
            true => ui::panel_with(
                s.in_progress,
                vec![],
                text(s.no_matches).size(SIZE_BODY).color(c.muted),
                c,
            ),
            false => container(list(s.in_progress, vec![], in_progress, s.no_matches, c))
                .height(Length::FillPortion(1))
                .into(),
        };
        let first = column![week, in_progress]
            .spacing(14)
            .width(Length::FillPortion(1))
            .height(Length::Fill);
        row![
            first,
            // the competitions their matches are played in: the teams
            // have no room left for a third action in a small window
            list(
                s.recent,
                vec![
                    ui::pill(s.competitions, false, Some(Message::Competitions), c),
                    ui::icon_pill(theme::icons::GYM, s.venues, Message::Venues, c),
                ],
                recent,
                s.no_matches,
                c
            ),
            list(s.teams, actions, teams, "", c),
        ]
        .spacing(14)
        .height(Length::Fill)
        .into()
    }
}

fn match_row<'a>(
    m: &HomeMatch,
    s: &'static Strings,
    selected: bool,
    message: Option<Message>,
    c: Colors,
) -> Element<'a, Message> {
    let next = match m.next {
        NextSetKind::Resume { set_number } => format!("{} · {} {set_number}", s.resume, s.set_n),
        NextSetKind::Start { set_number } => format!("{} · {} {set_number}", s.start, s.set_n),
        NextSetKind::Over => s.match_over.to_string(),
    };
    let bold = theme::fonts().bold;
    let body = row![
        column![
            row![
                text(m.team_name.clone()).size(SIZE_BODY).font(bold),
                text(format!("– {}", m.opponent))
                    .size(SIZE_BODY)
                    .color(c.muted),
            ]
            .spacing(6)
            .wrap(),
            text(format!("{} · {next}", s.day(&m.date)))
                .size(SIZE_SMALL)
                .color(c.muted),
        ]
        .spacing(2)
        .width(Length::Fill),
        text(format!("{}–{}", m.won, m.lost))
            .size(22)
            .font(bold)
            .color(ui::score_color(
                m.won,
                m.lost,
                matches!(m.next, NextSetKind::Over),
                c,
            )),
    ]
    .align_y(Alignment::Center)
    .spacing(10);
    ui::list_row(body, selected, message, c)
}

fn list<'a>(
    title: &str,
    actions: Vec<Element<'a, Message>>,
    items: Vec<Element<'a, Message>>,
    empty: &str,
    c: Colors,
) -> Element<'a, Message> {
    let body: Element<'a, Message> = match items.is_empty() {
        true => text(empty.to_string())
            .size(SIZE_BODY)
            .color(c.muted)
            .into(),
        false => Column::with_children(items).spacing(4).into(),
    };
    let content = scrollable(body).height(Length::Fill);
    container(ui::panel_with(title, actions, content, c))
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .into()
}
