//! The home: the matches in progress, the recent ones and the teams side by
//! side. ← → (or ⇧Tab Tab) between them, ↑ ↓ in the list, Enter opens: a
//! match in progress straight to its set, a recent one, a team; `n` a new
//! team.

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
    Form(form::Message),
}

pub enum Outcome {
    Stay,
    Scout(Open),
    Match(String, String),
    Team(String),
}

const IN_PROGRESS: usize = 0;
const RECENT: usize = 1;
const TEAMS: usize = 2;

pub struct HomeScreen {
    pub home: Home,
    panel: usize,
    selected: [usize; 3],
    /// a new team, being written
    editor: Option<Box<Editor>>,
}

fn date(m: &HomeMatch) -> String {
    m.date.get(..10).unwrap_or(&m.date).to_string()
}

fn team_id(t: &TeamListing) -> Option<String> {
    match t {
        TeamListing::Team { id, .. } => Some(id.clone()),
        TeamListing::Unreadable { .. } => None,
    }
}

impl HomeScreen {
    pub fn new(home: Home) -> HomeScreen {
        HomeScreen {
            home,
            panel: IN_PROGRESS,
            selected: [0; 3],
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
            _ => self.home.teams.len(),
        }
    }

    fn new_team(&mut self, core: &Scout4All, s: &Strings) {
        let year = chrono::Datelike::year(&chrono::Local::now());
        let form = core.new_team_form(u16::try_from(year).unwrap_or(2026));
        self.editor = Some(Box::new(Editor::team(core, s, None, form)));
    }

    fn edited(&mut self, edited: Edited) -> Outcome {
        match edited {
            Edited::Stay => Outcome::Stay,
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
            (Message::Scout(open), None) => Outcome::Scout(open),
            (Message::Match(team, m), None) => Outcome::Match(team, m),
            (Message::Team(team), None) => Outcome::Team(team),
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
        let moved = match ck {
            _ if next => {
                self.panel = (self.panel + 1) % 3;
                None
            }
            _ if previous => {
                self.panel = (self.panel + 2) % 3;
                None
            }
            k if k == key!(down) && count > 0 => Some((selected + 1) % count),
            k if k == key!(up) && count > 0 => Some(selected.checked_sub(1).unwrap_or(count - 1)),
            k if k == key!(enter) => return self.open(self.panel, selected),
            k if k == key!(n) => {
                self.new_team(core, s);
                None
            }
            _ => None,
        };
        if let (Some(index), Some(slot)) = (moved, self.selected.get_mut(self.panel)) {
            *slot = index;
        }
        Outcome::Stay
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match &self.editor {
            Some(_) => form::Form::hints(s),
            None => vec![
                ("← → ⇧⇥ ⇥".to_string(), s.panel.to_string()),
                ("↑ ↓".to_string(), String::new()),
                ("Enter".to_string(), s.open.to_string()),
                ("n".to_string(), s.new_team.to_string()),
                ("F2".to_string(), s.settings.to_string()),
            ],
        }
    }

    fn is_selected(&self, panel: usize, index: usize) -> bool {
        self.panel == panel && self.selected.get(panel) == Some(&index)
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        if let Some(editor) = &self.editor {
            return editor.view(s, c, Message::Form);
        }
        let in_progress = self
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
                        text(name.clone())
                            .size(SIZE_BODY)
                            .font(theme::fonts().bold)
                            .width(Length::Fill),
                        text(format!("{year} · {players} {}", s.players))
                            .size(SIZE_SMALL)
                            .color(c.muted),
                    ]
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
        let new_team = ui::pill(
            &format!("+ {}", s.new_team),
            false,
            Some(Message::NewTeam),
            c,
        );
        row![
            list(s.in_progress, None, in_progress, s.no_matches, c),
            list(s.recent, None, recent, s.no_matches, c),
            list(s.teams, Some(new_team), teams, "", c),
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
            text(format!("{} · {next}", date(m)))
                .size(SIZE_SMALL)
                .color(c.muted),
        ]
        .spacing(2)
        .width(Length::Fill),
        text(format!("{}–{}", m.won, m.lost))
            .size(22)
            .font(bold)
            .color(c.accent),
    ]
    .align_y(Alignment::Center)
    .spacing(10);
    ui::list_row(body, selected, message, c)
}

fn list<'a>(
    title: &str,
    action: Option<Element<'a, Message>>,
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
    let content = match action {
        Some(action) => column![action, scrollable(body).height(Length::Fill)].spacing(10),
        None => column![scrollable(body).height(Length::Fill)],
    };
    container(ui::panel(Some(title), content, c))
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .into()
}
