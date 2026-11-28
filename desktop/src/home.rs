//! The home: three blocks side by side, the week, the matches (the ones to
//! resume or play, then the last results) and the teams. ← → (or ⇧Tab Tab)
//! between the lists, ↑ ↓ in one, Enter opens: a match in progress straight
//! to its set, a recent one, a team; ␣ closes or opens the block of the
//! list; `n` a new team, `c` the competitions. After the teams, the ones
//! hidden from the home, to show them again.

use scout4all_mobile::clock::ClockExt;
use {
    crate::{
        agenda::{self, Agenda, Pick},
        editors::{Edited, Editor},
        form,
        head::Head,
        notice::{self, Level},
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL, SIZE_TITLE},
        ui,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, Id, button, column, container, row, scrollable, text},
    },
    scout4all_keys::{KeyCombination, key},
    scout4all_mobile::{
        Home, HomeBlock, HomeMatch, MatchStanding, NextSetKind, Scout4All, TeamListing,
    },
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
    /// a block closed, or opened again
    Toggle(HomeBlock),
    /// the teams hidden listed, or not
    ToggleHidden,
    /// a team hidden, back on the home
    Restore(String),
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

/// The block a list is in.
fn block_of(panel: usize) -> HomeBlock {
    match panel {
        WEEK => HomeBlock::Week,
        IN_PROGRESS | RECENT => HomeBlock::Matches,
        _ => HomeBlock::Teams,
    }
}

/// A row of the teams: a team, the line that lists the hidden ones, a
/// hidden one.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TeamRow {
    Team(usize),
    Hidden,
    Restore(usize),
}

pub struct HomeScreen {
    pub home: Home,
    week: Agenda,
    panel: usize,
    selected: [usize; PANELS],
    /// a new team, being written
    editor: Option<Box<Editor>>,
    /// the logos of the teams of the list, in its order
    logos: Vec<Option<ui::TeamLogo>>,
    hidden_logos: Vec<Option<ui::TeamLogo>>,
    /// the teams hidden listed
    show_hidden: bool,
    error: Option<String>,
}

fn team_id(t: &TeamListing) -> Option<String> {
    match t {
        TeamListing::Team { id, .. } => Some(id.clone()),
        TeamListing::Unreadable { .. } => None,
    }
}

fn logos_of(core: &Scout4All, teams: &[TeamListing]) -> Vec<Option<ui::TeamLogo>> {
    teams
        .iter()
        .map(|t| team_id(t).and_then(|id| ui::TeamLogo::of(core, &id)))
        .collect()
}

impl HomeScreen {
    pub fn new(core: &Scout4All, home: Home) -> HomeScreen {
        let mut screen = HomeScreen {
            logos: logos_of(core, &home.teams),
            hidden_logos: logos_of(core, &home.hidden),
            home,
            week: Agenda::home_week(core, core.clock().today()),
            panel: IN_PROGRESS,
            selected: [0; PANELS],
            editor: None,
            show_hidden: false,
            error: None,
        };
        screen.settle();
        screen
    }

    fn closed(&self, block: HomeBlock) -> bool {
        match block {
            HomeBlock::Week => self.home.week_closed,
            HomeBlock::Matches => self.home.matches_closed,
            HomeBlock::Teams => self.home.teams_closed,
        }
    }

    /// Whether ← → stop on `panel`: a list of the matches with nothing is
    /// not shown, the matches closed or without any are one stop.
    fn reachable(&self, panel: usize) -> bool {
        let closed = self.home.matches_closed;
        match panel {
            IN_PROGRESS => {
                closed || !self.home.in_progress.is_empty() || self.home.recent.is_empty()
            }
            RECENT => !closed && !self.home.recent.is_empty(),
            _ => true,
        }
    }

    /// The other list of the matches, when the one chosen is not there.
    fn settle(&mut self) {
        self.panel = match (self.panel, self.reachable(self.panel)) {
            (IN_PROGRESS, false) => RECENT,
            (RECENT, false) => IN_PROGRESS,
            (panel, _) => panel,
        };
    }

    fn toggle(&mut self, core: &Scout4All, block: HomeBlock) {
        let closed = !self.closed(block);
        match core.set_home_block_closed(block, closed) {
            Ok(()) => {
                match block {
                    HomeBlock::Week => self.home.week_closed = closed,
                    HomeBlock::Matches => self.home.matches_closed = closed,
                    HomeBlock::Teams => self.home.teams_closed = closed,
                }
                self.error = None;
                self.settle();
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Shows again on the home a team hidden from it.
    fn restore(&mut self, core: &Scout4All, id: String) {
        match core.set_hidden(id, false).and_then(|()| core.home()) {
            Ok(home) => {
                self.logos = logos_of(core, &home.teams);
                self.hidden_logos = logos_of(core, &home.hidden);
                self.show_hidden = self.show_hidden && !home.hidden.is_empty();
                self.home = home;
                self.week.reload(core);
                self.error = None;
                let last = self.count(TEAMS).saturating_sub(1);
                if let Some(selected) = self.selected.get_mut(TEAMS) {
                    *selected = (*selected).min(last);
                }
                self.settle();
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn team_row(&self, index: usize) -> Option<TeamRow> {
        let teams = self.home.teams.len();
        let hidden = self.home.hidden.len();
        match index.checked_sub(teams) {
            None => Some(TeamRow::Team(index)),
            Some(0) if hidden > 0 => Some(TeamRow::Hidden),
            Some(after) if self.show_hidden && after <= hidden => Some(TeamRow::Restore(after - 1)),
            Some(_) => None,
        }
    }

    /// The logo of a team of the list.
    fn logo_of(&self, id: &str) -> Option<&ui::TeamLogo> {
        self.home
            .teams
            .iter()
            .zip(&self.logos)
            .find(|(t, _)| matches!(t, TeamListing::Team { id: team, .. } if team == id))
            .and_then(|(_, logo)| logo.as_ref())
    }

    pub fn focus_id(&self) -> Option<Id> {
        self.editor.as_ref().map(|e| e.focus_id())
    }

    /// The rows of a list: none in a block closed.
    fn count(&self, panel: usize) -> usize {
        let hidden = self.home.hidden.len();
        match panel {
            _ if self.closed(block_of(panel)) => 0,
            IN_PROGRESS => self.home.in_progress.len(),
            RECENT => self.home.recent.len(),
            TEAMS => {
                self.home.teams.len()
                    + usize::from(hidden > 0)
                    + if self.show_hidden { hidden } else { 0 }
            }
            _ => 0,
        }
    }

    /// The panel after (or before) the one chosen.
    fn step(&self, forward: bool) -> usize {
        let one = match forward {
            true => 1,
            false => PANELS - 1,
        };
        (1..=PANELS)
            .map(|n| (self.panel + n * one) % PANELS)
            .find(|&p| self.reachable(p))
            .unwrap_or(self.panel)
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

    /// What the row of `panel` at `index` opens; the lines of the hidden
    /// teams act on the home itself.
    fn open(&mut self, core: &Scout4All, panel: usize, index: usize) -> Outcome {
        if index >= self.count(panel) {
            return Outcome::Stay;
        }
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
            _ => match self.team_row(index) {
                Some(TeamRow::Team(i)) => self
                    .home
                    .teams
                    .get(i)
                    .and_then(team_id)
                    .map_or(Outcome::Stay, Outcome::Team),
                Some(TeamRow::Hidden) => {
                    self.show_hidden = !self.show_hidden;
                    Outcome::Stay
                }
                Some(TeamRow::Restore(i)) => {
                    if let Some(id) = self.home.hidden.get(i).and_then(team_id) {
                        self.restore(core, id);
                    }
                    Outcome::Stay
                }
                None => Outcome::Stay,
            },
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
            (Message::Toggle(block), None) => {
                self.toggle(core, block);
                Outcome::Stay
            }
            (Message::ToggleHidden, None) => {
                self.show_hidden = !self.show_hidden;
                Outcome::Stay
            }
            (Message::Restore(id), None) => {
                self.restore(core, id);
                Outcome::Stay
            }
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
        let own = [key!(n), key!(i), key!(c), key!(g), key!(space)].contains(&ck);
        if self.panel == WEEK && !self.home.week_closed && !(next || previous || own) {
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
            k if k == key!(enter) => return self.open(core, self.panel, selected),
            k if k == key!(space) => {
                self.toggle(core, block_of(self.panel));
                None
            }
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
            None => {
                let moves = match self.panel {
                    WEEK if !self.home.week_closed => self.week.hints(s),
                    _ => vec![("↑ ↓".to_string(), String::new())],
                };
                [
                    vec![("← → ⇧⇥ ⇥".to_string(), s.panel.to_string())],
                    moves,
                    vec![
                        ("Enter".to_string(), s.open.to_string()),
                        ("␣".to_string(), s.open_close.to_string()),
                        ("n".to_string(), s.new_team.to_string()),
                        ("i".to_string(), s.import_archive.to_string()),
                        ("c".to_string(), s.competitions.to_string()),
                        ("g".to_string(), s.venues.to_string()),
                        ("F2".to_string(), s.settings.to_string()),
                    ],
                ]
                .concat()
            }
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
                    self.logo_of(&m.team_id),
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
                    self.logo_of(&m.team_id),
                    s,
                    self.is_selected(RECENT, i),
                    Some(Message::Match(m.team_id.clone(), m.id.clone())),
                    c,
                )
            })
            .collect();
        let mut teams: Vec<Element<'a, Message>> = self
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
                    initials,
                } => ui::box_row(
                    logo_head(self.logos.get(i).and_then(Option::as_ref), initials, c),
                    row![
                        column![
                            ui::body(name.clone()).font(theme::fonts().bold),
                            text(format!("{year} · {players} {}", s.players))
                                .size(SIZE_SMALL)
                                .color(c.muted),
                        ]
                        .spacing(2)
                        .width(Length::Fill),
                        text(match pinned {
                            true => "★",
                            false => "☆",
                        })
                        .size(20)
                        .color(match pinned {
                            true => c.accent,
                            false => c.muted,
                        }),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                    self.is_selected(TEAMS, i),
                    Message::Team(id.clone()),
                    c,
                ),
                TeamListing::Unreadable { folder } => ui::box_row(
                    ui::icon(theme::icons::ERROR_OUTLINE, 22.0, c.danger),
                    ui::body(folder.clone()).font(theme::fonts().bold),
                    false,
                    None,
                    c,
                ),
            })
            .collect();
        teams.extend(self.hidden_rows(s, c));
        let actions = vec![
            ui::pill(
                &format!("↓ {}", s.import_archive),
                false,
                Some(Message::Import),
                c,
            ),
            ui::add_button(s.new_team, Message::NewTeam, c),
        ];
        // the week (today lit in it)
        let (week_actions, week) = self.week.parts(s, c, self.panel == WEEK, true);
        let week = self.block(
            HomeBlock::Week,
            week_actions
                .into_iter()
                .map(|a| a.map(Message::Week))
                .collect(),
            week.map(Message::Week),
            s,
            c,
        );
        // a list of the matches only with something in it
        let lists: Vec<Element<'a, Message>> =
            [(s.to_resume, in_progress), (s.last_results, recent)]
                .into_iter()
                .filter(|(_, rows)| !rows.is_empty())
                .map(|(title, rows)| {
                    column![
                        ui::section(title, c),
                        Column::with_children(rows).spacing(4)
                    ]
                    .spacing(6)
                    .into()
                })
                .collect();
        // the competitions their matches are played in: the teams have no
        // room left for a third action in a small window
        let matches = self.block(
            HomeBlock::Matches,
            vec![
                ui::pill(s.competitions, false, Some(Message::Competitions), c),
                ui::icon_pill(theme::icons::GYM, s.venues, Message::Venues, c),
            ],
            list(lists, s.no_matches, c),
            s,
            c,
        );
        let teams = self.block(HomeBlock::Teams, actions, list(teams, "", c), s, c);
        let blocks = row![week, matches, teams].spacing(14).height(Length::Fill);
        column![]
            .push(
                self.error
                    .as_ref()
                    .map(|e| notice::banner(Level::Error, e, c)),
            )
            .push(blocks)
            .spacing(10)
            .into()
    }

    /// The line that lists the teams hidden, and them once listed.
    fn hidden_rows<'a>(&'a self, s: &'static Strings, c: Colors) -> Vec<Element<'a, Message>> {
        let count = self.home.hidden.len();
        if count == 0 {
            return vec![];
        }
        let teams = self.home.teams.len();
        let label = match self.show_hidden {
            true => s.hide_hidden.to_string(),
            false => s.show_hidden.replace("{1}", &count.to_string()),
        };
        let toggle = ui::pill(
            &label,
            self.is_selected(TEAMS, teams),
            Some(Message::ToggleHidden),
            c,
        );
        let listed = self
            .home
            .hidden
            .iter()
            .zip(&self.hidden_logos)
            .enumerate()
            .filter(|_| self.show_hidden)
            .filter_map(|(i, (team, logo))| match team {
                TeamListing::Team {
                    id, name, initials, ..
                } => Some(ui::box_row(
                    logo_head(logo.as_ref(), initials, c),
                    column![
                        ui::body(name.clone()).color(c.muted),
                        text(s.show_in_home).size(SIZE_SMALL).color(c.accent),
                    ]
                    .spacing(2),
                    self.is_selected(TEAMS, teams + 1 + i),
                    Message::Restore(id.clone()),
                    c,
                )),
                TeamListing::Unreadable { .. } => None,
            });
        std::iter::once(toggle).chain(listed).collect()
    }

    /// A block of the home: its icon and title, which close and open it,
    /// then its actions and content while open.
    fn block<'a>(
        &'a self,
        block: HomeBlock,
        actions: Vec<Element<'a, Message>>,
        content: Element<'a, Message>,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let (icon, title) = match block {
            HomeBlock::Week => (theme::icons::CALENDAR, s.home_calendar),
            HomeBlock::Matches => (theme::icons::VOLLEYBALL, s.matches),
            HomeBlock::Teams => (theme::icons::ALL_TEAMS, s.teams),
        };
        let closed = self.closed(block);
        let chevron = match closed {
            true => theme::icons::CHEVRON_RIGHT,
            false => theme::icons::EXPAND_MORE,
        };
        // closed, no row shows where the keys are: its title does
        let color = match closed && block_of(self.panel) == block {
            true => c.accent,
            false => c.text,
        };
        let title = button(
            row![
                ui::icon(chevron, 18.0, c.muted),
                ui::icon(icon, 20.0, c.accent),
                text(title)
                    .size(SIZE_TITLE)
                    .font(theme::fonts().bold)
                    .color(color),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(0)
        .on_press(Message::Toggle(block))
        .style(move |_, _| button::Style {
            text_color: c.text,
            ..button::Style::default()
        });
        let actions = row(actions.into_iter().filter(|_| !closed))
            .spacing(8)
            .align_y(Alignment::Center)
            .wrap()
            .align_x(iced::alignment::Horizontal::Right);
        let head: Element<'a, Message> = Head::new(title, actions).into();
        let body = column![head].push((!closed).then_some(content)).spacing(12);
        let height = match closed {
            true => Length::Shrink,
            false => Length::Fill,
        };
        container(ui::panel(None, body, c))
            .width(Length::FillPortion(1))
            .height(height)
            .into()
    }
}

/// At the head of a box: the logo of the team, or its initials.
fn logo_head<'a>(logo: Option<&ui::TeamLogo>, initials: &str, c: Colors) -> Element<'a, Message> {
    let picture = logo.cloned().map_or_else(
        || ui::TeamPicture::Initials(initials.to_string()),
        ui::TeamPicture::Logo,
    );
    ui::team_picture(&picture, 36.0, c)
}

fn match_row<'a>(
    m: &HomeMatch,
    logo: Option<&ui::TeamLogo>,
    s: &'static Strings,
    selected: bool,
    message: Option<Message>,
    c: Colors,
) -> Element<'a, Message> {
    // what a match not over goes on with
    let next = match m.next {
        NextSetKind::Resume { set_number } => {
            Some(format!("{} · {} {set_number}", s.resume, s.set_n))
        }
        NextSetKind::Start { set_number } => {
            Some(format!("{} · {} {set_number}", s.start, s.set_n))
        }
        NextSetKind::Over => None,
    };
    let bold = theme::fonts().bold;
    // the opponent first, as on Android: the head already tells the team
    let body = row![
        column![
            ui::body(m.opponent.clone()).font(bold),
            text(
                [
                    Some(m.team_name.clone()),
                    Some(s.day(&m.date)),
                    (m.standing == MatchStanding::ToPlay).then(|| s.to_play.to_string()),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ")
            )
            .size(SIZE_SMALL)
            .color(c.muted),
        ]
        .push(next.map(|next| text(next).size(SIZE_SMALL).color(c.accent)))
        .spacing(2)
        .width(Length::Fill),
    ]
    // no score for a match not started
    .extend((m.standing != MatchStanding::ToPlay).then(|| {
        text(format!("{}–{}", m.won, m.lost))
            .size(22)
            .font(bold)
            .color(ui::score_color(
                m.won,
                m.lost,
                m.standing == MatchStanding::Over,
                c,
            ))
            .into()
    }))
    .align_y(Alignment::Center)
    .spacing(10);
    ui::box_row(
        logo_head(logo, &m.team_initials, c),
        body,
        selected,
        message,
        c,
    )
}

/// The rows of a block, scrolled; `empty` when there are none.
fn list<'a>(items: Vec<Element<'a, Message>>, empty: &str, c: Colors) -> Element<'a, Message> {
    let body: Element<'a, Message> = match items.is_empty() {
        true => text(empty.to_string())
            .size(SIZE_BODY)
            .color(c.muted)
            .into(),
        false => Column::with_children(items).spacing(8).into(),
    };
    scrollable(body).height(Length::Fill).into()
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{scouting::tests::PNG, stats::tests::team_with_two_matches, strings::strings},
        scout4all_core::{
            scouting::Progress,
            shapes::enums::{EvalEnum, LanguageEnum, TeamSideEnum},
        },
        scout4all_mobile::MatchListing,
        std::sync::Arc,
    };

    fn app(dir: &tempfile::TempDir) -> Arc<Scout4All> {
        Scout4All::for_tests(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap()
    }

    /// The set in progress of `m` (we serve) played to its end, then the
    /// others until the match is over: all aces.
    fn play_out(core: &Scout4All, team: &str, m: &str) {
        let ids: Vec<String> = core
            .team(team.into())
            .unwrap()
            .players
            .into_iter()
            .map(|p| p.id)
            .collect();
        let mut scouting = core.scouting(team.into(), m.into()).unwrap();
        loop {
            while scouting.choose_eval(EvalEnum::Perfect).unwrap() != Progress::SetOver {}
            let Ok(start) = core.set_start(team.into(), m.into()) else {
                break;
            };
            start.choose_serving_team(TeamSideEnum::Us);
            for (position, id) in (1..=6).zip(&ids) {
                start.place(position, Some(id.clone())).unwrap();
            }
            start.choose_setter(Some(ids[0].clone())).unwrap();
            start.choose_libero(Some(ids[6].clone())).unwrap();
            core.start_set(start).unwrap();
            scouting = core.scouting(team.into(), m.into()).unwrap();
        }
    }

    /// Enter opens the row chosen of each panel: a match in progress to
    /// scout on, a recent one on its page, a team; the week empty nothing.
    #[test]
    fn enter_opens_the_row_of_each_panel() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = team_with_two_matches(&core);
        let over = core
            .matches(team.clone())
            .unwrap()
            .into_iter()
            .find_map(|m| match m {
                MatchListing::Match { id, opponent, .. } if opponent == "Arezzo" => Some(id),
                _ => None,
            })
            .unwrap();
        play_out(&core, &team, &over);
        let mut home = HomeScreen::new(&core, core.home().unwrap());
        let resumed = home.home.in_progress[0].id.clone();
        assert_ne!(resumed, over);
        assert_eq!(home.panel_name(), "in progress");
        assert!(matches!(
            home.key(&core, s, key!(enter)),
            Outcome::Scout(Open { team_id, match_id, start: false })
                if team_id == team && match_id == resumed
        ));
        home.key(&core, s, key!(right));
        assert_eq!(home.panel_name(), "recent");
        assert!(matches!(
            home.key(&core, s, key!(enter)),
            Outcome::Match(t, m) if t == team && m == over
        ));
        home.key(&core, s, key!(right));
        assert_eq!(home.panel_name(), "teams");
        let chosen = (0..home.home.teams.len())
            .find(|&i| team_id(&home.home.teams[i]).as_deref() == Some(team.as_str()))
            .unwrap();
        for _ in 0..chosen {
            home.key(&core, s, key!(down));
        }
        assert!(matches!(
            home.key(&core, s, key!(enter)),
            Outcome::Team(t) if t == team
        ));
        home.key(&core, s, key!(right));
        assert_eq!(home.panel_name(), "week");
        assert!(matches!(home.key(&core, s, key!(enter)), Outcome::Stay));
    }

    /// i, c, g and n do the same from every panel, the week too.
    #[test]
    fn the_letters_of_the_home_work_from_every_panel() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        for panel in 0..PANELS {
            let mut home = HomeScreen::new(&core, core.home().unwrap());
            home.panel = panel;
            let name = home.panel_name();
            assert!(
                matches!(home.key(&core, s, key!(i)), Outcome::Import),
                "{name}"
            );
            assert!(
                matches!(home.key(&core, s, key!(c)), Outcome::Competitions),
                "{name}"
            );
            assert!(
                matches!(home.key(&core, s, key!(g)), Outcome::Venues),
                "{name}"
            );
            home.key(&core, s, key!(n));
            assert!(home.focus_id().is_some(), "{name}");
        }
    }

    /// The rows of the matches have the logo of their team, the teams
    /// without one none.
    #[test]
    fn the_matches_have_the_logo_of_their_team() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = team_with_two_matches(&core);
        core.set_team_logo(team.clone(), Some(PNG.to_vec()))
            .unwrap();
        let tutorial = core.tutorial_team().unwrap();
        let home = HomeScreen::new(&core, core.home().unwrap());
        assert!(home.logo_of(&team).is_some());
        assert!(home.logo_of(&tutorial).is_none());
        assert!(home.logo_of("not a team").is_none());
    }

    /// ␣ closes the block of the list chosen and opens it again, and it's
    /// remembered; closed, its rows are out of reach and the matches are
    /// one stop of ← →.
    #[test]
    fn space_closes_and_opens_the_block_of_the_list() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        team_with_two_matches(&core);
        let mut home = HomeScreen::new(&core, core.home().unwrap());
        home.key(&core, s, key!(space));
        assert!(core.home().unwrap().matches_closed);
        assert!(matches!(home.key(&core, s, key!(enter)), Outcome::Stay));
        home.key(&core, s, key!(right));
        assert_eq!(home.panel_name(), "teams");
        home.key(&core, s, key!(space));
        assert!(matches!(home.key(&core, s, key!(enter)), Outcome::Stay));
        home.key(&core, s, key!(right));
        home.key(&core, s, key!(space));
        let read = core.home().unwrap();
        assert!(read.week_closed && read.matches_closed && read.teams_closed);
        home.key(&core, s, key!(space));
        assert!(!core.home().unwrap().week_closed);
    }

    /// After the teams, the line of the hidden ones lists them, and Enter
    /// on one shows it on the home again.
    #[test]
    fn a_hidden_team_is_shown_again_from_the_end_of_the_teams() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = team_with_two_matches(&core);
        core.set_hidden(team.clone(), true).unwrap();
        let mut home = HomeScreen::new(&core, core.home().unwrap());
        home.panel = TEAMS;
        let teams = home.home.teams.len();
        assert_eq!(home.count(TEAMS), teams + 1);
        home.selected[TEAMS] = teams;
        home.key(&core, s, key!(enter));
        assert_eq!(home.count(TEAMS), teams + 2);
        home.key(&core, s, key!(down));
        home.key(&core, s, key!(enter));
        assert!(home.home.hidden.is_empty());
        assert_eq!(home.count(TEAMS), teams + 1);
        assert!(!core.team_hidden(team).unwrap());
    }
}
