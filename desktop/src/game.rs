//! A match: its result, its sets, what comes next. Enter starts or resumes
//! the set to scout, `e` edits the match.

use {
    crate::{
        editors::{Edited, Editor},
        form,
        strings::Strings,
        team::day,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, Id, column, container, row, space, text},
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{MatchDetails, MobileError, NextSetKind, Scout4All},
    std::sync::Arc,
};

#[derive(Debug, Clone)]
pub enum Message {
    Next,
    Edit,
    Stats,
    Export,
    Form(form::Message),
}

pub enum Outcome {
    Stay,
    Back,
    /// a set to start (`true`) or to resume
    Scout(bool),
    Stats,
    /// the archive of the match, to save
    Export,
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
        match (message, &mut self.editor) {
            (Message::Form(m), Some(editor)) => {
                let edited = editor.update(&self.core, s, m);
                self.edited(edited);
                Outcome::Stay
            }
            (_, Some(_)) | (Message::Form(_), None) => Outcome::Stay,
            (Message::Next, None) => self.next(),
            (Message::Stats, None) => Outcome::Stats,
            (Message::Export, None) => Outcome::Export,
            (Message::Edit, None) => {
                self.edit(s);
                Outcome::Stay
            }
        }
    }

    pub fn key(&mut self, s: &Strings, ck: KeyCombination) -> Outcome {
        match (&mut self.editor, ck) {
            (Some(editor), ck) => {
                let edited = editor.key(&self.core, s, ck);
                self.edited(edited);
                Outcome::Stay
            }
            (None, k) if k == BACK_KEY => Outcome::Back,
            (None, k) if k == key!(enter) => self.next(),
            (None, k) if k == key!(s) => Outcome::Stats,
            (None, k) if k == key!(x) => Outcome::Export,
            (None, k) if k == key!(e) => {
                self.edit(s);
                Outcome::Stay
            }
            _ => Outcome::Stay,
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
        match &self.editor {
            Some(_) => form::Form::hints(s),
            None => [
                self.next_label(s)
                    .map(|next| ("Enter".to_string(), next))
                    .into_iter()
                    .collect::<Vec<_>>(),
                vec![
                    ("s".to_string(), s.stats.to_string()),
                    ("e".to_string(), s.edit_match.to_string()),
                    ("x".to_string(), s.export_match.to_string()),
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
        let score = row![
            column![
                text(self.team.clone()).size(SIZE_BODY).font(bold),
                text(d.opponent.clone()).size(SIZE_BODY).color(c.muted),
            ]
            .spacing(6)
            .width(Length::Fill),
            column![
                text(d.won.to_string()).size(40).font(bold).color(c.accent),
                text(d.lost.to_string()).size(40).font(bold),
            ]
            .align_x(Alignment::End),
        ]
        .align_y(Alignment::Center);
        let place = match d.home {
            true => s.home_game,
            false => s.away_game,
        };
        let next: Element<'a, Message> = match self.next_label(s) {
            Some(label) => ui::pill(&label, true, Some(Message::Next), c),
            None => text(s.match_over).size(SIZE_BODY).color(c.muted).into(),
        };
        let notes: Element<'a, Message> = match d.notes.trim().is_empty() {
            true => space::vertical().height(0).into(),
            false => column![
                text(s.notes).size(SIZE_SMALL).color(c.muted),
                ui::body(d.notes.clone())
            ]
            .spacing(2)
            .into(),
        };
        let summary = ui::panel(
            Some(s.a_match),
            column![
                score,
                text(format!("{} · {place}", day(&d.date)))
                    .size(SIZE_SMALL)
                    .color(c.muted),
                notes,
                space::vertical().height(6),
                row![
                    next,
                    ui::pill(s.stats, false, Some(Message::Stats), c),
                    ui::pill(s.edit_match, false, Some(Message::Edit), c),
                    ui::pill(
                        &format!("↑ {}", s.export_match),
                        false,
                        Some(Message::Export),
                        c
                    ),
                ]
                .spacing(10)
                .wrap()
                .vertical_spacing(8),
            ]
            .spacing(12),
            c,
        );
        let sets = d.sets.iter().map(|set| {
            row![
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
            .align_y(Alignment::Center)
            .into()
        });
        let sets: Element<'a, Message> = match d.sets.is_empty() {
            true => ui::body(s.no_rallies).color(c.muted).into(),
            false => Column::with_children(sets).spacing(8).into(),
        };
        let error: Element<'a, Message> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => space::vertical().height(0).into(),
        };
        column![
            error,
            row![
                container(summary).width(Length::FillPortion(3)),
                container(ui::panel(Some(s.sets), sets, c)).width(Length::FillPortion(2)),
            ]
            .spacing(14)
        ]
        .spacing(10)
        .into()
    }
}
