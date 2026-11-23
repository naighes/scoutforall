//! The sheet of a session: its data, its activities in order, its
//! attendance; edit it, complete it, cancel or reopen it, delete it.
//! ← → ⇥ between the activities and the attendance; ↑ ↓ in the activities,
//! ⇧↑ ⇧↓ to move one, Enter to edit it (the attendance, to write it).

use {
    super::{
        forms::{ActivityEditor, CopyEditor, Edited, SessionEditor},
        roll::{self, Done, Roll},
        series::{DuplicateEditor, SeriesChangeEditor},
    },
    crate::{
        form::{self, Form},
        strings::Strings,
        theme::{self, Colors, SIZE_SMALL},
        ui,
        venue_choice::venue_label,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, Id, column, container, row, scrollable, space, text},
    },
    scout4all_core::training::{ActivityStatus, AttendanceStatus, Move, SessionStatus},
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{
        MobileError, Scout4All,
        training::{ActivityRecord, AttendanceSheetRecord, SessionRecord},
        venues::VenueRecord,
    },
    std::sync::Arc,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Panel {
    Activities,
    Attendance,
}

/// The planned activities to mark completed with the session: unchecked by a
/// click or a space.
struct Checklist {
    items: Vec<(String, String, bool)>,
    selected: usize,
}

enum Mode {
    Browse,
    Session(Box<SessionEditor>),
    Activity(Box<ActivityEditor>),
    Copy(Box<CopyEditor>),
    Duplicate(Box<DuplicateEditor>),
    Series(Box<SeriesChangeEditor>),
    Roll(Box<Roll>),
    /// the options of the PDF of the sheet
    Pdf(Box<Form>),
    Complete(Checklist),
    DeleteSession,
    /// id and name
    DeleteActivity(String, String),
}

#[derive(Debug, Clone)]
pub enum Message {
    Panel(Panel),
    Activity(usize),
    NewActivity,
    CopyActivity,
    MoveUp,
    MoveDown,
    DeleteActivity,
    Attendance,
    EditSession,
    Duplicate,
    EditSeries,
    Pdf,
    Complete,
    CancelSession,
    Reopen,
    DeleteSession,
    Confirm,
    Dismiss,
    Check(usize),
    Form(form::Message),
    Roll(roll::Message),
}

pub enum Outcome {
    Stay,
    /// back to the list
    Back,
    /// the sheet of another session (the duplicate)
    Opened(String),
    /// the PDF of the sheet, to share
    Export(crate::transfer::Export),
}

pub struct SessionPage {
    core: Arc<Scout4All>,
    team_id: String,
    pub id: String,
    record: SessionRecord,
    /// the gym of the session, read with it (not at every frame)
    venue: Option<VenueRecord>,
    activities: Vec<ActivityRecord>,
    attendance: AttendanceSheetRecord,
    panel: Panel,
    activity: usize,
    mode: Mode,
    error: Option<String>,
}

/// The gym of a session, removed too.
fn venue_of(core: &Scout4All, record: &SessionRecord) -> Option<VenueRecord> {
    record
        .venue_id
        .as_ref()
        .and_then(|id| core.venue(id.clone()).ok())
}

impl SessionPage {
    pub fn new(
        core: Arc<Scout4All>,
        team_id: String,
        id: String,
    ) -> Result<SessionPage, MobileError> {
        let record = core.training_session(id.clone())?;
        let activities = core.session_activities(id.clone())?;
        let attendance = core.session_attendance(id.clone())?;
        let venue = venue_of(&core, &record);
        Ok(SessionPage {
            core,
            team_id,
            id,
            record,
            venue,
            activities,
            attendance,
            panel: Panel::Activities,
            activity: 0,
            mode: Mode::Browse,
            error: None,
        })
    }

    #[cfg(test)]
    pub fn mode_name(&self) -> &'static str {
        match self.mode {
            Mode::Browse => "browse",
            Mode::Session(_) => "session",
            Mode::Activity(_) => "activity",
            Mode::Copy(_) => "copy",
            Mode::Duplicate(_) => "duplicate",
            Mode::Series(_) => "series",
            Mode::Roll(_) => "roll",
            Mode::Pdf(_) => "pdf",
            Mode::Complete(_) => "complete",
            Mode::DeleteSession | Mode::DeleteActivity(..) => "delete",
        }
    }

    pub fn title(&self, s: &Strings) -> String {
        let name = match self.record.title.is_empty() {
            true => s.training.to_string(),
            false => self.record.title.clone(),
        };
        format!("{name} · {}", s.day(&self.record.start))
    }

    pub fn focus_id(&self) -> Option<Id> {
        match &self.mode {
            Mode::Session(e) => Some(e.form.focus_id()),
            Mode::Activity(e) => Some(e.form.focus_id()),
            Mode::Copy(e) => Some(e.form.focus_id()),
            Mode::Duplicate(e) => Some(e.form.focus_id()),
            Mode::Series(e) => Some(e.form.focus_id()),
            Mode::Roll(r) => r.focus_id(),
            Mode::Pdf(f) => Some(f.focus_id()),
            _ => None,
        }
    }

    /// Everything read again after a change.
    fn reload(&mut self) {
        let read = (
            self.core.training_session(self.id.clone()),
            self.core.session_activities(self.id.clone()),
            self.core.session_attendance(self.id.clone()),
        );
        match read {
            (Ok(record), Ok(activities), Ok(attendance)) => {
                self.venue = venue_of(&self.core, &record);
                self.record = record;
                self.activities = activities;
                self.attendance = attendance;
                self.activity = self.activity.min(self.activities.len().saturating_sub(1));
                self.error = None;
            }
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => self.error = Some(e.to_string()),
        }
    }

    fn done<T>(&mut self, result: Result<T, MobileError>) {
        match result {
            Ok(_) => self.reload(),
            Err(e) => self.error = Some(e.to_string()),
        }
        self.mode = Mode::Browse;
    }

    fn edit_session(&mut self, s: &'static Strings) {
        match SessionEditor::new(
            &self.core,
            s,
            self.team_id.clone(),
            Some(self.id.clone()),
            None,
        ) {
            Ok(editor) => self.mode = Mode::Session(Box::new(editor)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn duplicate(&mut self, s: &'static Strings) {
        match DuplicateEditor::new(&self.core, s, self.team_id.clone(), self.id.clone()) {
            Ok(editor) => self.mode = Mode::Duplicate(Box::new(editor)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// The change of the group, for a session of a series.
    fn edit_series(&mut self, s: &Strings) {
        if self.record.series_id.is_none() {
            return;
        }
        match SeriesChangeEditor::new(&self.core, s, self.team_id.clone(), self.id.clone()) {
            Ok(editor) => self.mode = Mode::Series(Box::new(editor)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn edit_activity(
        &mut self,
        s: &Strings,
        index: Option<usize>,
        copy: Option<scout4all_mobile::training::ActivityHitRecord>,
    ) {
        let id = index
            .and_then(|i| self.activities.get(i))
            .map(|a| a.id.clone());
        match ActivityEditor::new(&self.core, s, self.id.clone(), id, copy) {
            Ok(editor) => self.mode = Mode::Activity(Box::new(editor)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn open_roll(&mut self) {
        match Roll::new(&self.core, self.id.clone(), self.team_id.clone()) {
            Ok(roll) => self.mode = Mode::Roll(Box::new(roll)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// The completion: the planned activities asked about, if any.
    fn ask_completion(&mut self) {
        if self.record.status != SessionStatus::Planned {
            return;
        }
        let items: Vec<(String, String, bool)> = self
            .activities
            .iter()
            .filter(|a| a.status == ActivityStatus::Planned)
            .map(|a| (a.id.clone(), a.name.clone(), true))
            .collect();
        match items.is_empty() {
            true => {
                let result = self.core.complete_session(self.id.clone(), vec![]);
                self.done(result);
            }
            false => self.mode = Mode::Complete(Checklist { items, selected: 0 }),
        }
    }

    fn complete(&mut self) {
        let Mode::Complete(list) = &self.mode else {
            return;
        };
        let chosen = list
            .items
            .iter()
            .filter(|(_, _, on)| *on)
            .map(|(id, _, _)| id.clone())
            .collect();
        let result = self.core.complete_session(self.id.clone(), chosen);
        self.done(result);
    }

    fn set_status(&mut self, status: SessionStatus) {
        if status == SessionStatus::Cancelled && self.record.status != SessionStatus::Planned {
            return;
        }
        if status == SessionStatus::Planned && self.record.status == SessionStatus::Planned {
            return;
        }
        let result = self.core.set_session_status(self.id.clone(), status);
        self.done(result);
    }

    fn ask_activity_deletion(&mut self) {
        if let Some(a) = self.activities.get(self.activity) {
            self.mode = Mode::DeleteActivity(a.id.clone(), a.name.clone());
        }
    }

    fn move_activity(&mut self, to: Move) {
        let Some(a) = self.activities.get(self.activity) else {
            return;
        };
        let last = self.activities.len().saturating_sub(1);
        let at = match (to, self.activity) {
            (Move::Up, 0) => return,
            (Move::Down, i) if i >= last => return,
            (Move::Up, i) => i - 1,
            (Move::Down, i) => i + 1,
        };
        match self.core.move_activity(a.id.clone(), to) {
            Ok(()) => {
                self.activity = at;
                self.reload();
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// The form with the two options of the PDF of the sheet.
    fn ask_pdf(&mut self, s: &Strings) {
        let fields = vec![
            form::Field::check(s.sheet_participants, false, false),
            form::Field::check(s.sheet_notes, false, false),
        ];
        self.mode = Mode::Pdf(Box::new(Form::new(s.session_sheet_pdf, fields)));
    }

    fn pdf_outcome(&mut self, outcome: form::Outcome) -> Outcome {
        match (&self.mode, outcome) {
            (Mode::Pdf(f), form::Outcome::Save) => {
                let on = |i: usize| {
                    matches!(
                        f.fields.get(i).map(|x| &x.value),
                        Some(form::Value::Check { on: true, .. })
                    )
                };
                let export = crate::transfer::Export::SessionSheet(self.id.clone(), on(0), on(1));
                self.mode = Mode::Browse;
                return Outcome::Export(export);
            }
            (_, form::Outcome::Cancel) => self.mode = Mode::Browse,
            _ => {}
        }
        Outcome::Stay
    }

    fn confirm(&mut self) -> Outcome {
        match &self.mode {
            Mode::Complete(_) => self.complete(),
            Mode::DeleteActivity(id, _) => {
                let result = self.core.remove_activity(id.clone());
                self.done(result);
            }
            Mode::DeleteSession => match self.core.remove_session(self.id.clone()) {
                Ok(()) => return Outcome::Back,
                Err(e) => {
                    self.error = Some(e.to_string());
                    self.mode = Mode::Browse;
                }
            },
            _ => {}
        }
        Outcome::Stay
    }

    fn edited(&mut self, s: &Strings, edited: Edited) -> Outcome {
        match (&self.mode, edited) {
            (_, Edited::Stay) => {}
            (_, Edited::Cancel) => self.mode = Mode::Browse,
            // the duplicate: its sheet opens
            (Mode::Duplicate(_), Edited::Saved(id)) => return Outcome::Opened(id),
            // the series changed or removed: the session may be gone
            (Mode::Series(_), Edited::Saved(_)) => {
                self.mode = Mode::Browse;
                if self.core.training_session(self.id.clone()).is_err() {
                    return Outcome::Back;
                }
                self.reload();
            }
            (_, Edited::Saved(_)) => {
                self.mode = Mode::Browse;
                self.reload();
            }
            (_, Edited::Copy(hit)) => self.edit_activity(s, None, Some(hit)),
        }
        Outcome::Stay
    }

    fn rolled(&mut self, done: Done) {
        match done {
            Done::Stay => {}
            Done::Cancel => self.mode = Mode::Browse,
            Done::Saved => {
                self.mode = Mode::Browse;
                self.reload();
            }
        }
    }

    pub fn update(&mut self, s: &'static Strings, message: Message) -> Outcome {
        match (&mut self.mode, message) {
            (Mode::Session(e), Message::Form(m)) => {
                let edited = e.update(&self.core, m);
                return self.edited(s, edited);
            }
            (Mode::Activity(e), Message::Form(m)) => {
                let edited = e.update(&self.core, m);
                return self.edited(s, edited);
            }
            (Mode::Copy(e), Message::Form(m)) => {
                let edited = e.update(&self.core, s, m);
                return self.edited(s, edited);
            }
            (Mode::Duplicate(e), Message::Form(m)) => {
                let edited = e.update(&self.core, m);
                return self.edited(s, edited);
            }
            (Mode::Series(e), Message::Form(m)) => {
                let edited = e.update(&self.core, m);
                return self.edited(s, edited);
            }
            (Mode::Pdf(f), Message::Form(m)) => {
                let outcome = f.update(m);
                return self.pdf_outcome(outcome);
            }
            (Mode::Roll(r), Message::Roll(m)) => {
                let done = r.update(&self.core, s, m);
                self.rolled(done);
            }
            (Mode::Complete(list), Message::Check(i)) => {
                if let Some(item) = list.items.get_mut(i) {
                    item.2 = !item.2;
                }
                list.selected = i;
            }
            (Mode::Browse, message) => return self.browse(s, message),
            (_, Message::Confirm) => return self.confirm(),
            (_, Message::Dismiss) => self.mode = Mode::Browse,
            _ => {}
        }
        Outcome::Stay
    }

    fn browse(&mut self, s: &'static Strings, message: Message) -> Outcome {
        match message {
            Message::Panel(panel) => self.panel = panel,
            Message::Activity(i) => {
                self.panel = Panel::Activities;
                self.activity = i;
                self.edit_activity(s, Some(i), None);
            }
            Message::NewActivity => self.edit_activity(s, None, None),
            Message::CopyActivity => {
                self.mode = Mode::Copy(Box::new(CopyEditor::new(
                    &self.core,
                    s,
                    self.team_id.clone(),
                )))
            }
            Message::MoveUp => self.move_activity(Move::Up),
            Message::MoveDown => self.move_activity(Move::Down),
            Message::DeleteActivity => self.ask_activity_deletion(),
            Message::Attendance => self.open_roll(),
            Message::EditSession => self.edit_session(s),
            Message::Duplicate => self.duplicate(s),
            Message::EditSeries => self.edit_series(s),
            Message::Pdf => self.ask_pdf(s),
            Message::Complete => self.ask_completion(),
            Message::CancelSession => self.set_status(SessionStatus::Cancelled),
            Message::Reopen => self.set_status(SessionStatus::Planned),
            Message::DeleteSession => self.mode = Mode::DeleteSession,
            Message::Confirm
            | Message::Dismiss
            | Message::Check(_)
            | Message::Form(_)
            | Message::Roll(_) => {}
        }
        Outcome::Stay
    }

    pub fn key(&mut self, s: &'static Strings, ck: KeyCombination) -> Outcome {
        match &mut self.mode {
            Mode::Session(e) => {
                let edited = e.key(&self.core, ck);
                return self.edited(s, edited);
            }
            Mode::Activity(e) => {
                let edited = e.key(&self.core, ck);
                return self.edited(s, edited);
            }
            Mode::Copy(e) => {
                let edited = e.key(&self.core, s, ck);
                return self.edited(s, edited);
            }
            Mode::Duplicate(e) => {
                let edited = e.key(&self.core, ck);
                return self.edited(s, edited);
            }
            Mode::Series(e) => {
                let edited = e.key(&self.core, ck);
                return self.edited(s, edited);
            }
            Mode::Pdf(f) => {
                let outcome = f.key(ck);
                return self.pdf_outcome(outcome);
            }
            Mode::Roll(r) => {
                let done = r.key(&self.core, s, ck);
                self.rolled(done);
            }
            Mode::Complete(list) => {
                let count = list.items.len();
                match ck {
                    k if k == key!(enter) => return self.confirm(),
                    k if k == BACK_KEY => self.mode = Mode::Browse,
                    k if k == key!(down) && count > 0 => {
                        list.selected = (list.selected + 1) % count
                    }
                    k if k == key!(up) && count > 0 => {
                        list.selected = list.selected.checked_sub(1).unwrap_or(count - 1)
                    }
                    k if k == key!(space) => {
                        let at = list.selected;
                        if let Some(item) = list.items.get_mut(at) {
                            item.2 = !item.2;
                        }
                    }
                    _ => {}
                }
            }
            Mode::DeleteSession | Mode::DeleteActivity(..) => match ck {
                k if k == key!(enter) => return self.confirm(),
                k if k == BACK_KEY => self.mode = Mode::Browse,
                _ => {}
            },
            Mode::Browse => return self.browse_key(s, ck),
        }
        Outcome::Stay
    }

    fn browse_key(&mut self, s: &'static Strings, ck: KeyCombination) -> Outcome {
        let count = self.activities.len();
        let switch = [
            key!(left),
            key!(right),
            key!(tab),
            key!(shift - tab),
            key!(shift - backtab),
            key!(backtab),
        ]
        .contains(&ck);
        match (self.panel, ck) {
            (_, k) if k == BACK_KEY => return Outcome::Back,
            (_, _) if switch => {
                self.panel = match self.panel {
                    Panel::Activities => Panel::Attendance,
                    Panel::Attendance => Panel::Activities,
                }
            }
            (_, k) if k == key!(e) => self.edit_session(s),
            (_, k) if k == key!(d) => self.duplicate(s),
            (_, k) if k == key!(g) => self.edit_series(s),
            (_, k) if k == key!(p) => self.ask_pdf(s),
            (_, k) if k == key!(c) => self.ask_completion(),
            (_, k) if k == key!(x) => self.set_status(SessionStatus::Cancelled),
            (_, k) if k == key!(r) => self.set_status(SessionStatus::Planned),
            (_, k) if k == key!(shift - backspace) || k == key!(shift - delete) => {
                self.mode = Mode::DeleteSession
            }
            (Panel::Activities, k) if k == key!(down) && count > 0 => {
                self.activity = (self.activity + 1) % count
            }
            (Panel::Activities, k) if k == key!(up) && count > 0 => {
                self.activity = self.activity.checked_sub(1).unwrap_or(count - 1)
            }
            (Panel::Activities, k) if k == key!(shift - up) => self.move_activity(Move::Up),
            (Panel::Activities, k) if k == key!(shift - down) => self.move_activity(Move::Down),
            (Panel::Activities, k) if k == key!(n) => self.edit_activity(s, None, None),
            (Panel::Activities, k) if k == key!(v) => {
                self.mode = Mode::Copy(Box::new(CopyEditor::new(
                    &self.core,
                    s,
                    self.team_id.clone(),
                )))
            }
            (Panel::Activities, k) if k == key!(enter) && count > 0 => {
                self.edit_activity(s, Some(self.activity), None)
            }
            (Panel::Activities, k) if (k == key!(backspace) || k == key!(delete)) && count > 0 => {
                self.ask_activity_deletion()
            }
            (Panel::Attendance, k) if k == key!(enter) => self.open_roll(),
            _ => {}
        }
        Outcome::Stay
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match &self.mode {
            Mode::Session(e) => e.form.on_field(Form::hints(s)),
            Mode::Activity(e) => e.form.on_field(Form::hints(s)),
            Mode::Copy(_) | Mode::Duplicate(_) | Mode::Series(_) | Mode::Pdf(_) => Form::hints(s),
            Mode::Roll(r) => r.hints(s),
            Mode::Complete(_) => vec![
                ("↑ ↓".to_string(), String::new()),
                (s.space.to_string(), s.change.to_string()),
                ("Enter".to_string(), s.complete_session.to_string()),
                ("Esc".to_string(), s.cancel.to_string()),
            ],
            Mode::DeleteSession | Mode::DeleteActivity(..) => vec![
                ("Enter".to_string(), s.delete.to_string()),
                ("Esc".to_string(), s.cancel.to_string()),
            ],
            Mode::Browse => {
                let planned = self.record.status == SessionStatus::Planned;
                let panel = match self.panel {
                    Panel::Activities => vec![
                        ("↑ ↓".to_string(), String::new()),
                        ("⇧↑ ⇧↓".to_string(), s.move_up.to_string()),
                        ("Enter".to_string(), s.edit.to_string()),
                        ("n".to_string(), s.new_activity.to_string()),
                        ("v".to_string(), s.copy_activity.to_string()),
                        ("⌫".to_string(), s.remove.to_string()),
                    ],
                    Panel::Attendance => vec![("Enter".to_string(), s.attendance.to_string())],
                };
                [
                    vec![("← → ⇥".to_string(), s.panel.to_string())],
                    panel,
                    vec![
                        ("e".to_string(), s.edit_session.to_string()),
                        ("d".to_string(), s.duplicate_session.to_string()),
                        ("p".to_string(), s.session_sheet_pdf.to_string()),
                    ],
                    match self.record.series_id {
                        Some(_) => vec![("g".to_string(), s.edit_series.to_string())],
                        None => vec![],
                    },
                    match planned {
                        true => vec![
                            ("c".to_string(), s.complete_session.to_string()),
                            ("x".to_string(), s.cancel_session.to_string()),
                        ],
                        false => vec![("r".to_string(), s.reopen_session.to_string())],
                    },
                    vec![
                        ("⇧⌫".to_string(), s.delete.to_string()),
                        ("Esc".to_string(), s.back.to_string()),
                    ],
                ]
                .concat()
            }
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let body: Element<'a, Message> = match &self.mode {
            Mode::Session(e) => centered(e.form.view(s, c, Message::Form, None)),
            Mode::Activity(e) => centered(e.form.view(s, c, Message::Form, None)),
            Mode::Copy(e) => centered(e.form.view(s, c, Message::Form, None)),
            Mode::Duplicate(e) => centered(e.form.view(s, c, Message::Form, None)),
            Mode::Series(e) => centered(e.form.view(s, c, Message::Form, None)),
            Mode::Pdf(f) => centered(f.view(s, c, Message::Form, None)),
            Mode::Roll(r) => r.view(s, c).map(Message::Roll),
            Mode::Complete(list) => self.completion(list, s, c),
            Mode::DeleteSession => ui::confirm_deletion(
                s,
                s.delete_session_text,
                Message::Confirm,
                Message::Dismiss,
                c,
            ),
            Mode::DeleteActivity(_, name) => ui::confirm_deletion(
                s,
                &format!("{name}\n{}", s.delete_activity_text),
                Message::Confirm,
                Message::Dismiss,
                c,
            ),
            Mode::Browse => row![
                container(self.info(s, c)).width(Length::FillPortion(3)),
                container(self.activities_panel(s, c)).width(Length::FillPortion(4)),
                container(self.attendance_panel(s, c)).width(Length::FillPortion(3)),
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

    fn completion<'a>(
        &'a self,
        list: &'a Checklist,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let rows: Vec<Element<'a, Message>> = list
            .items
            .iter()
            .enumerate()
            .map(|(i, (_, name, on))| {
                ui::list_row(
                    ui::body(format!("{} {name}", if *on { "☑" } else { "☐" })),
                    i == list.selected,
                    Some(Message::Check(i)),
                    c,
                )
            })
            .collect();
        let question = ui::panel(
            None,
            column![
                text(s.complete_session)
                    .size(22)
                    .font(theme::fonts().bold)
                    .color(c.text),
                ui::body(s.complete_activities),
                scrollable(Column::with_children(rows).spacing(3)).height(Length::Shrink),
                ui::answers(
                    Some((theme::icons::CLOSE, s.cancel, Message::Dismiss)),
                    theme::icons::CHECK,
                    s.complete_session,
                    Some(Message::Confirm),
                    c
                )
            ]
            .spacing(18),
            c,
        );
        container(container(question).max_width(560))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }

    fn status_text(&self, s: &'static Strings) -> String {
        match self.record.status {
            SessionStatus::Planned => s.status_planned,
            SessionStatus::Completed => s.status_completed,
            SessionStatus::Cancelled => s.cancelled,
        }
        .to_string()
    }

    fn info<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let r = &self.record;
        let line = |label: &str, value: String| -> Element<'a, Message> {
            match value.is_empty() {
                true => space::vertical().height(0).into(),
                false => column![
                    text(label.to_string()).size(SIZE_SMALL).color(c.muted),
                    ui::body(value)
                ]
                .spacing(2)
                .into(),
            }
        };
        let when = format!(
            "{} · {}",
            s.day(&r.start),
            r.start.get(11..16).unwrap_or_default()
        );
        let duration = match r.actual_duration_min {
            Some(actual) => format!(
                "{} {} ({} {})",
                r.duration_min, s.min_short, actual, s.min_short
            ),
            None => format!("{} {}", r.duration_min, s.min_short),
        };
        let mut state = self.status_text(s);
        if r.to_close {
            state = format!("{state} · {}", s.to_close);
        }
        let planned = r.status == SessionStatus::Planned;
        let mut actions: Vec<Element<'a, Message>> = vec![
            ui::action(
                theme::icons::EDIT,
                s.edit_session,
                Message::EditSession,
                false,
                c,
            )
            .width(Length::Fill)
            .into(),
        ];
        actions.push(
            ui::action(
                theme::icons::ADD,
                s.duplicate_session,
                Message::Duplicate,
                false,
                c,
            )
            .width(Length::Fill)
            .into(),
        );
        actions.push(
            ui::action(
                theme::icons::SAVE,
                s.session_sheet_pdf,
                Message::Pdf,
                false,
                c,
            )
            .width(Length::Fill)
            .into(),
        );
        if r.series_id.is_some() {
            actions.push(
                ui::action(
                    theme::icons::EDIT,
                    s.edit_series,
                    Message::EditSeries,
                    false,
                    c,
                )
                .width(Length::Fill)
                .into(),
            );
        }
        match planned {
            true => {
                actions.push(
                    ui::action(
                        theme::icons::CHECK,
                        s.complete_session,
                        Message::Complete,
                        true,
                        c,
                    )
                    .width(Length::Fill)
                    .into(),
                );
                actions.push(
                    ui::action(
                        theme::icons::CLOSE,
                        s.cancel_session,
                        Message::CancelSession,
                        false,
                        c,
                    )
                    .width(Length::Fill)
                    .into(),
                );
            }
            false => actions.push(
                ui::action(
                    theme::icons::BACK,
                    s.reopen_session,
                    Message::Reopen,
                    false,
                    c,
                )
                .width(Length::Fill)
                .into(),
            ),
        }
        actions.push(
            ui::action(
                theme::icons::DELETE,
                s.delete,
                Message::DeleteSession,
                false,
                c,
            )
            .width(Length::Fill)
            .into(),
        );
        ui::panel(
            Some(s.training),
            scrollable(
                column![
                    text(match r.title.is_empty() {
                        true => s.training.to_string(),
                        false => r.title.clone(),
                    })
                    .size(22)
                    .font(theme::fonts().bold),
                    // the state under the title, as the lists show it
                    text(state).size(SIZE_SMALL).color(c.accent),
                    line(s.date, when),
                    line(s.session_duration, duration),
                    line(
                        s.session_location,
                        self.venue
                            .as_ref()
                            .map(|v| venue_label(s, &v.name, v.deleted))
                            .unwrap_or_default(),
                    ),
                    line(s.objectives, r.objectives.clone()),
                    line(s.notes, r.notes.clone()),
                    line(s.final_remarks, r.final_remarks.clone()),
                    space::vertical().height(6),
                    Column::with_children(actions).spacing(10),
                ]
                .spacing(12)
                .width(Length::Fill)
                .padding(iced::Padding {
                    right: 14.0,
                    ..iced::Padding::ZERO
                }),
            )
            .height(Length::Fill),
            c,
        )
    }

    fn activities_panel<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let active = self.panel == Panel::Activities;
        let rows: Vec<Element<'a, Message>> = self
            .activities
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let minutes = match (a.planned_min, a.actual_min) {
                    (Some(p), Some(x)) => format!("{p} ({x}) {}", s.min_short),
                    (Some(m), None) | (None, Some(m)) => format!("{m} {}", s.min_short),
                    (None, None) => String::new(),
                };
                let state = match a.status {
                    ActivityStatus::Planned => String::new(),
                    ActivityStatus::Completed => s.status_completed.to_string(),
                    ActivityStatus::Skipped => s.status_skipped.to_string(),
                };
                let head = row![
                    text(format!("{}.", i + 1))
                        .size(SIZE_SMALL)
                        .color(c.muted)
                        .width(28),
                    ui::name(a.name.clone()),
                    text(minutes).size(SIZE_SMALL).color(c.muted),
                    text(state).size(SIZE_SMALL).color(c.accent),
                ]
                .spacing(10)
                .align_y(Alignment::Center);
                let body: Element<'a, Message> = match a.description.is_empty() {
                    true => head.into(),
                    false => column![
                        head,
                        text(a.description.clone()).size(SIZE_SMALL).color(c.muted)
                    ]
                    .spacing(2)
                    .into(),
                };
                ui::list_row(
                    body,
                    active && i == self.activity,
                    Some(Message::Activity(i)),
                    c,
                )
            })
            .collect();
        let list: Element<'a, Message> = match rows.is_empty() {
            true => text(s.no_activities).size(SIZE_SMALL).color(c.muted).into(),
            false => scrollable(Column::with_children(rows).spacing(3))
                .height(Length::Fill)
                .into(),
        };
        // the panel is a third of the window: the title keeps the new one
        // only, the rest go under the list, on two lines if they must
        let tools = iced::widget::Row::with_children([
            ui::pill(s.copy_activity, false, Some(Message::CopyActivity), c),
            ui::pill("↑", false, Some(Message::MoveUp), c),
            ui::pill("↓", false, Some(Message::MoveDown), c),
            ui::pill(s.remove, false, Some(Message::DeleteActivity), c),
        ])
        .spacing(8)
        .wrap()
        .vertical_spacing(8);
        ui::panel_with(
            s.activities,
            vec![ui::pill(
                s.new_activity,
                false,
                Some(Message::NewActivity),
                c,
            )],
            column![list, tools].spacing(10),
            c,
        )
    }

    fn attendance_panel<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let active = self.panel == Panel::Attendance;
        let sheet = &self.attendance;
        let count =
            |status: AttendanceStatus| sheet.rows.iter().filter(|r| r.status == status).count();
        let body: Element<'a, Message> = match sheet.recorded {
            false => text(s.attendance_not_recorded)
                .size(SIZE_SMALL)
                .color(c.muted)
                .into(),
            true => {
                let rows: Vec<Element<'a, Message>> = sheet
                    .rows
                    .iter()
                    .map(|r| {
                        let status = match r.status {
                            AttendanceStatus::Present => s.present,
                            AttendanceStatus::Late => s.late,
                            AttendanceStatus::Absent => s.absent,
                            AttendanceStatus::Unavailable => s.unavailable,
                        };
                        row![
                            text(r.number.map(|n| format!("#{n}")).unwrap_or_default())
                                .size(SIZE_SMALL)
                                .color(c.muted)
                                .width(40),
                            ui::name(r.name.clone()),
                            text(status).size(SIZE_SMALL).color(c.muted),
                        ]
                        .spacing(8)
                        .into()
                    })
                    .collect();
                column![
                    text(format!(
                        "{} {} · {} {} · {} {} · {} {}",
                        count(AttendanceStatus::Present),
                        s.present,
                        count(AttendanceStatus::Late),
                        s.late,
                        count(AttendanceStatus::Absent),
                        s.absent,
                        count(AttendanceStatus::Unavailable),
                        s.unavailable
                    ))
                    .size(SIZE_SMALL)
                    .color(c.muted),
                    scrollable(Column::with_children(rows).spacing(3)).height(Length::Fill),
                ]
                .spacing(8)
                .into()
            }
        };
        let panel = ui::panel_with(
            s.attendance,
            vec![ui::pill(s.edit, active, Some(Message::Attendance), c)],
            body,
            c,
        );
        iced::widget::mouse_area(panel)
            .on_press(Message::Panel(Panel::Attendance))
            .into()
    }
}

fn centered<'a>(form: Element<'a, Message>) -> Element<'a, Message> {
    container(container(form).max_width(720))
        .center_x(Length::Fill)
        .height(Length::Fill)
        .into()
}
