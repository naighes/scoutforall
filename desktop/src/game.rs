//! A match: its result, its sets, what comes next. Enter starts or resumes
//! the set to scout, `e` edits the match, `v` opens its video. A match the
//! team plays as B, scouted by A only, shows only its result as B sees it:
//! Enter opens it as A.

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
        widget::{Column, Id, column, container, row, space, text},
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{MatchDetails, MobileError, NextSetKind, Scout4All, ScoutedBy},
    std::sync::Arc,
};

#[derive(Debug, Clone)]
pub enum Message {
    Next,
    Edit,
    Stats,
    Export,
    Report,
    Video,
    Record,
    /// the events of the set, to correct them
    Set(u8),
    Form(form::Message),
    /// the match seen by B: open it as A, who scouted it
    OpenAsA,
}

pub enum Outcome {
    Stay,
    Back,
    /// a set to start (`true`) or to resume
    Scout(bool),
    Stats,
    /// the archive of the match, to save
    Export,
    /// the report of the match as a PDF, to save
    Report,
    /// the video of the match
    Video,
    /// a camera to record the match with (or the recording stopped)
    Record,
    /// the events of the set, to correct them
    Events(u8),
    /// the match as this team A sees it, who scouted it
    OpenAs(String),
}

pub struct MatchScreen {
    core: Arc<Scout4All>,
    pub team_id: String,
    pub match_id: String,
    team: String,
    details: MatchDetails,
    editor: Option<Box<Editor>>,
    error: Option<String>,
}

impl MatchScreen {
    pub fn new(
        core: Arc<Scout4All>,
        team_id: String,
        match_id: String,
    ) -> Result<MatchScreen, MobileError> {
        let team = core.team(team_id.clone())?.name;
        let details = core.match_details(team_id.clone(), match_id.clone())?;
        Ok(MatchScreen {
            core,
            team_id,
            match_id,
            team,
            details,
            editor: None,
            error: None,
        })
    }

    pub fn title(&self) -> String {
        format!("{} – {}", self.team, self.details.opponent)
    }

    pub fn focus_id(&self) -> Option<Id> {
        self.editor.as_ref().map(|e| e.focus_id())
    }

    /// Team A, when the team is B of the match, scouted by A only.
    fn scouted_by(&self) -> Option<&ScoutedBy> {
        self.details.scouted_by.as_ref()
    }

    fn open_as_a(&self) -> Outcome {
        match self.scouted_by() {
            Some(a) => Outcome::OpenAs(a.team_id.clone()),
            None => Outcome::Stay,
        }
    }

    fn next(&self) -> Outcome {
        match self.details.next {
            NextSetKind::Start { .. } => Outcome::Scout(true),
            NextSetKind::Resume { .. } => Outcome::Scout(false),
            NextSetKind::Over => Outcome::Stay,
        }
    }

    fn edit(&mut self, s: &Strings) {
        match Editor::a_match(
            s,
            &self.core,
            self.team_id.clone(),
            Some(self.match_id.clone()),
        ) {
            Ok(editor) => self.editor = Some(Box::new(editor)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn edited(&mut self, edited: Edited) {
        match edited {
            Edited::Stay => {}
            Edited::Cancel => self.editor = None,
            Edited::Saved(_) => {
                self.editor = None;
                match self
                    .core
                    .match_details(self.team_id.clone(), self.match_id.clone())
                {
                    Ok(details) => self.details = details,
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
        }
    }

    pub fn update(&mut self, s: &Strings, message: Message) -> Outcome {
        let seen_by_b = self.scouted_by().is_some();
        match (message, &mut self.editor) {
            (Message::Form(m), Some(editor)) => {
                let edited = editor.update(&self.core, s, m);
                self.edited(edited);
                Outcome::Stay
            }
            (_, Some(_)) | (Message::Form(_), None) => Outcome::Stay,
            (Message::OpenAsA, None) => self.open_as_a(),
            // seen by B: only the result
            (_, None) if seen_by_b => Outcome::Stay,
            (Message::Next, None) => self.next(),
            (Message::Stats, None) => Outcome::Stats,
            (Message::Export, None) => Outcome::Export,
            (Message::Report, None) => Outcome::Report,
            (Message::Video, None) => Outcome::Video,
            (Message::Record, None) => Outcome::Record,
            (Message::Set(n), None) => Outcome::Events(n),
            (Message::Edit, None) => {
                self.edit(s);
                Outcome::Stay
            }
        }
    }

    pub fn key(&mut self, s: &Strings, ck: KeyCombination) -> Outcome {
        let seen_by_b = self.scouted_by().is_some();
        match (&mut self.editor, ck) {
            (Some(editor), ck) => {
                let edited = editor.key(&self.core, s, ck);
                self.edited(edited);
                Outcome::Stay
            }
            (None, k) if k == BACK_KEY => Outcome::Back,
            (None, k) if seen_by_b => match k == key!(enter) {
                true => self.open_as_a(),
                false => Outcome::Stay,
            },
            (None, k) if k == key!(enter) => self.next(),
            (None, k) if k == key!(s) => Outcome::Stats,
            (None, k) if k == key!(x) => Outcome::Export,
            (None, k) if k == key!(p) => Outcome::Report,
            (None, k) if k == key!(v) => Outcome::Video,
            (None, k) if k == key!(r) => Outcome::Record,
            (None, k) if k == key!(e) => {
                self.edit(s);
                Outcome::Stay
            }
            // a digit: the events of that set, if played
            (None, k) => match crate::keymap::digit(&k) {
                Some(n) if self.details.sets.iter().any(|set| set.number == n) => {
                    Outcome::Events(n)
                }
                _ => Outcome::Stay,
            },
        }
    }

    fn next_label(&self, s: &Strings) -> Option<String> {
        match self.details.next {
            NextSetKind::Start { set_number } => {
                Some(format!("{} · {} {set_number}", s.start, s.set_n))
            }
            NextSetKind::Resume { set_number } => {
                Some(format!("{} · {} {set_number}", s.resume, s.set_n))
            }
            NextSetKind::Over => None,
        }
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match (&self.editor, self.scouted_by()) {
            (Some(editor), _) => editor.hints(s),
            (None, Some(a)) => vec![
                (
                    "Enter".to_string(),
                    ui::fill(s.open_as_team, std::slice::from_ref(&a.team_name)),
                ),
                ("Esc".to_string(), s.back.to_string()),
            ],
            (None, None) => [
                self.next_label(s)
                    .map(|next| ("Enter".to_string(), next))
                    .into_iter()
                    .collect::<Vec<_>>(),
                vec![
                    ("s".to_string(), s.stats.to_string()),
                    ("e".to_string(), s.edit_match.to_string()),
                    ("x".to_string(), s.export_match.to_string()),
                    ("p".to_string(), s.match_report.to_string()),
                    ("v".to_string(), s.video.to_string()),
                    ("r".to_string(), s.record_video.to_string()),
                    (
                        format!("1–{}", self.details.sets.len().max(1)),
                        s.set_events.to_string(),
                    ),
                    ("Esc".to_string(), s.back.to_string()),
                ],
            ]
            .concat(),
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        if let Some(editor) = &self.editor {
            return editor.view(s, c, Message::Form);
        }
        let d = &self.details;
        let bold = theme::fonts().bold;
        // won or lost, once over: a quiet mark by the score
        let result: Element<'a, Message> = match (self.next_label(s), d.won > d.lost) {
            (Some(_), _) => space::horizontal().width(0).into(),
            (None, true) => ui::tip(ui::icon(theme::icons::WON, 30.0, c.won), s.match_won, c),
            (None, false) => ui::tip(ui::icon(theme::icons::LOST, 30.0, c.lost), s.match_lost, c),
        };
        let score = row![
            column![
                text(self.team.clone()).size(22).font(bold),
                text(d.opponent.clone()).size(22).color(c.muted),
            ]
            .spacing(16)
            .width(Length::Fill),
            result,
            column![
                text(d.won.to_string()).size(44).font(bold).color(c.accent),
                text(d.lost.to_string()).size(44).font(bold),
            ]
            .align_x(Alignment::End),
        ]
        .spacing(18)
        .align_y(Alignment::Center);
        let (place_icon, place) = match d.home {
            true => (theme::icons::HOME, s.home_game),
            false => (theme::icons::AWAY, s.away_game),
        };
        let fact = |icon: &'static [u8], what: String| {
            row![
                ui::icon(icon, 18.0, c.muted),
                text(what).size(SIZE_BODY).color(c.muted),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
        };
        let facts = row![
            fact(theme::icons::CALENDAR, s.day(&d.date)),
            fact(place_icon, place.to_string()),
        ]
        .spacing(24);
        let notes: Element<'a, Message> = match d.notes.trim().is_empty() {
            true => space::vertical().height(0).into(),
            false => column![
                text(s.notes).size(SIZE_SMALL).color(c.muted),
                ui::body(d.notes.clone())
            ]
            .spacing(2)
            .into(),
        };
        let next: Element<'a, Message> = match self.next_label(s) {
            Some(label) => {
                ui::action(theme::icons::VOLLEYBALL, &label, Message::Next, true, c).into()
            }
            None => space::horizontal().width(0).into(),
        };
        let action = |icon: &'static [u8], label: &str, message: Message| {
            ui::action(icon, label, message, false, c)
        };
        let actions: Element<'a, Message> = match self.scouted_by() {
            // seen by B: why only the result, and A's side to open
            Some(a) => column![
                ui::body(ui::fill(
                    s.scouted_by_text,
                    &[a.team_name.clone(), self.team.clone()],
                ))
                .color(c.muted),
                ui::action(
                    theme::icons::VOLLEYBALL,
                    &ui::fill(s.open_as_team, std::slice::from_ref(&a.team_name)),
                    Message::OpenAsA,
                    true,
                    c,
                ),
            ]
            .spacing(14)
            .into(),
            None => ui::action_grid(
                vec![
                    action(theme::icons::STATS, s.stats, Message::Stats),
                    action(theme::icons::EDIT, s.edit_match, Message::Edit),
                    action(theme::icons::SHARE, s.export_match, Message::Export),
                    action(theme::icons::PDF, s.match_report, Message::Report),
                    action(theme::icons::VIDEO, s.video, Message::Video),
                    action(theme::icons::RECORD, s.record_video, Message::Record),
                ],
                3,
            ),
        };
        let summary = ui::panel(
            Some(s.a_match),
            column![
                space::vertical().height(4),
                score,
                facts,
                notes,
                space::vertical(),
                next,
                actions,
            ]
            .spacing(18)
            .height(Length::Fill),
            c,
        );
        let sets = d.sets.iter().map(|set| {
            let line = row![
                text(format!("{} {}", s.set_n, set.number))
                    .size(SIZE_BODY)
                    .color(c.muted)
                    .width(Length::Fill),
                text(format!("{:>2}", set.us))
                    .size(22)
                    .font(bold)
                    .color(match set.us > set.them {
                        true => c.accent,
                        false => c.text,
                    }),
                text("–").size(22).color(c.muted),
                text(format!("{:<2}", set.them)).size(22).font(bold),
            ]
            .spacing(10)
            .align_y(Alignment::Center);
            // seen by B: no events of its side to open
            let open = match self.scouted_by() {
                Some(_) => None,
                None => Some(Message::Set(set.number)),
            };
            ui::list_row(line, false, open, c)
        });
        let sets: Element<'a, Message> = match d.sets.is_empty() {
            true => ui::body(s.no_rallies).color(c.muted).into(),
            false => iced::widget::scrollable(Column::with_children(sets).spacing(8))
                .height(Length::Fill)
                .into(),
        };
        let error: Element<'a, Message> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => space::vertical().height(0).into(),
        };
        column![
            error,
            row![
                container(summary).width(Length::FillPortion(3)),
                container(ui::panel(
                    Some(s.sets),
                    column![sets].height(Length::Fill).width(Length::Fill),
                    c
                ))
                .width(Length::FillPortion(2)),
            ]
            .spacing(14)
            .height(Length::Fill)
        ]
        .spacing(10)
        .into()
    }
}
