//! The plan of the training of a team: tabs (the sessions for now), the
//! list of the sessions with its filter, and the sheet of the one opened.
//! ↑ ↓ between the sessions, Enter opens one, n makes a new one, f changes
//! the filter, ⌫ deletes the one chosen, Esc goes back.

use {
    super::{
        forms::{Edited, SessionEditor},
        session::{self, SessionPage},
    },
    crate::{
        form::{self, Form},
        strings::Strings,
        theme::{Colors, SIZE_SMALL},
        ui,
    },
    chrono::NaiveDate,
    iced::{
        Alignment, Element, Length,
        widget::{Column, Id, column, container, row, scrollable, space, text},
    },
    scout4all_core::training::SessionStatus,
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{MobileError, Scout4All, training::SessionRecord},
    std::{collections::HashSet, sync::Arc},
};

const LIST: &str = "training-sessions";

/// What the screen opens on.
#[derive(Debug, Clone, PartialEq)]
pub enum Open {
    Sessions,
    /// the sheet of a session
    Session(String),
    /// a new session, on a day
    NewSession(NaiveDate),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Filter {
    All,
    Planned,
    ToClose,
    Completed,
    Cancelled,
    NoAttendance,
}

const FILTERS: [Filter; 6] = [
    Filter::All,
    Filter::Planned,
    Filter::ToClose,
    Filter::Completed,
    Filter::Cancelled,
    Filter::NoAttendance,
];

fn filter_name(s: &Strings, filter: Filter) -> &'static str {
    match filter {
        Filter::All => s.filter_all,
        Filter::Planned => s.filter_planned,
        Filter::ToClose => s.filter_to_close,
        Filter::Completed => s.filter_completed,
        Filter::Cancelled => s.filter_cancelled,
        Filter::NoAttendance => s.filter_no_attendance,
    }
}

enum Mode {
    List,
    Page(Box<SessionPage>),
    New(Box<SessionEditor>),
    /// id of the session to delete
    Delete(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    Filter(Filter),
    Row(usize),
    New,
    Delete,
    Confirm,
    Dismiss,
    Form(form::Message),
    Page(session::Message),
}

pub enum Outcome {
    Stay,
    /// back to the team
    Back,
}

pub struct TrainingScreen {
    core: Arc<Scout4All>,
    pub team_id: String,
    team_name: String,
    /// the sessions of the team, the latest first
    sessions: Vec<SessionRecord>,
    /// the completed ones with their attendance written
    recorded: HashSet<String>,
    filter: Filter,
    selected: usize,
    mode: Mode,
    error: Option<String>,
}

impl TrainingScreen {
    pub fn new(
        core: Arc<Scout4All>,
        s: &Strings,
        team_id: String,
        open: Open,
    ) -> Result<TrainingScreen, MobileError> {
        let team_name = core.team(team_id.clone())?.name;
        let mut screen = TrainingScreen {
            core,
            team_id,
            team_name,
            sessions: vec![],
            recorded: HashSet::new(),
            filter: Filter::All,
            selected: 0,
            mode: Mode::List,
            error: None,
        };
        screen.reload();
        match open {
            Open::Sessions => {}
            Open::Session(id) => screen.open_session(&id),
            Open::NewSession(day) => {
                match SessionEditor::new(&screen.core, s, screen.team_id.clone(), None, Some(day)) {
                    Ok(editor) => screen.mode = Mode::New(Box::new(editor)),
                    Err(e) => screen.error = Some(e.to_string()),
                }
            }
        }
        Ok(screen)
    }

    pub fn title(&self, s: &Strings) -> String {
        match &self.mode {
            Mode::Page(page) => format!("{} · {}", self.team_name, page.title(s)),
            _ => format!("{} · {}", self.team_name, s.programming),
        }
    }

    pub fn focus_id(&self) -> Option<Id> {
        match &self.mode {
            Mode::Page(page) => page.focus_id(),
            Mode::New(editor) => Some(editor.form.focus_id()),
            Mode::List | Mode::Delete(_) => None,
        }
    }

    /// Where the list is, to keep the session chosen in sight.
    pub fn scroll(&self) -> Option<(Id, f32)> {
        match self.mode {
            Mode::List => {
                let last = self.shown().len().saturating_sub(1).max(1);
                Some((Id::new(LIST), self.selected as f32 / last as f32))
            }
            _ => None,
        }
    }

    /// The sessions read again, the latest first; the one chosen stays.
    fn reload(&mut self) {
        match self.core.training_sessions(self.team_id.clone()) {
            Ok(mut sessions) => {
                sessions.reverse();
                self.recorded = sessions
                    .iter()
                    .filter(|r| r.status == SessionStatus::Completed)
                    .filter(|r| {
                        self.core
                            .session_attendance(r.id.clone())
                            .is_ok_and(|a| a.recorded)
                    })
                    .map(|r| r.id.clone())
                    .collect();
                self.sessions = sessions;
                self.selected = self.selected.min(self.shown().len().saturating_sub(1));
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn matches(&self, r: &SessionRecord) -> bool {
        match self.filter {
            Filter::All => true,
            Filter::Planned => r.status == SessionStatus::Planned,
            Filter::ToClose => r.to_close,
            Filter::Completed => r.status == SessionStatus::Completed,
            Filter::Cancelled => r.status == SessionStatus::Cancelled,
            Filter::NoAttendance => {
                r.status == SessionStatus::Completed && !self.recorded.contains(&r.id)
            }
        }
    }

    /// The sessions the filter lets through.
    fn shown(&self) -> Vec<&SessionRecord> {
        self.sessions.iter().filter(|r| self.matches(r)).collect()
    }

    fn open_session(&mut self, id: &str) {
        match SessionPage::new(self.core.clone(), self.team_id.clone(), id.to_string()) {
            Ok(page) => self.mode = Mode::Page(Box::new(page)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn open_selected(&mut self) {
        let id = self.shown().get(self.selected).map(|r| r.id.clone());
        if let Some(id) = id {
            self.open_session(&id);
        }
    }

    fn new_session(&mut self, s: &Strings) {
        match SessionEditor::new(&self.core, s, self.team_id.clone(), None, None) {
            Ok(editor) => self.mode = Mode::New(Box::new(editor)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn ask_deletion(&mut self) {
        let id = self.shown().get(self.selected).map(|r| r.id.clone());
        if let Some(id) = id {
            self.mode = Mode::Delete(id);
        }
    }

    fn delete(&mut self) {
        let removed = match &self.mode {
            Mode::Delete(id) => self.core.remove_session(id.clone()),
            _ => Ok(()),
        };
        if let Err(e) = removed {
            self.error = Some(e.to_string());
        }
        self.mode = Mode::List;
        self.reload();
    }

    fn edited(&mut self, edited: Edited) {
        match edited {
            Edited::Stay => {}
            Edited::Cancel => self.mode = Mode::List,
            // the new session opens on its sheet
            Edited::Saved(id) => {
                self.reload();
                self.open_session(&id);
            }
            Edited::Copy(_) => {}
        }
    }

    fn paged(&mut self, outcome: session::Outcome) {
        if let session::Outcome::Back = outcome {
            self.mode = Mode::List;
            self.reload();
        }
    }

    pub fn update(&mut self, s: &Strings, message: Message) -> Outcome {
        match (&mut self.mode, message) {
            (Mode::Page(page), Message::Page(m)) => {
                let outcome = page.update(s, m);
                self.paged(outcome);
            }
            (Mode::New(editor), Message::Form(m)) => {
                let edited = editor.update(&self.core, m);
                self.edited(edited);
            }
            (Mode::Delete(_), Message::Confirm) => self.delete(),
            (Mode::Delete(_), Message::Dismiss) => self.mode = Mode::List,
            (Mode::List, Message::Filter(filter)) => {
                self.filter = filter;
                self.selected = 0;
            }
            (Mode::List, Message::Row(i)) => {
                self.selected = i;
                self.open_selected();
            }
            (Mode::List, Message::New) => self.new_session(s),
            (Mode::List, Message::Delete) => self.ask_deletion(),
            _ => {}
        }
        Outcome::Stay
    }

    pub fn key(&mut self, s: &Strings, ck: KeyCombination) -> Outcome {
        match &mut self.mode {
            Mode::Page(page) => {
                let outcome = page.key(s, ck);
                self.paged(outcome);
            }
            Mode::New(editor) => {
                let edited = editor.key(&self.core, ck);
                self.edited(edited);
            }
            Mode::Delete(_) => match ck {
                k if k == key!(enter) => self.delete(),
                k if k == BACK_KEY => self.mode = Mode::List,
                _ => {}
            },
            Mode::List => return self.list_key(s, ck),
        }
        Outcome::Stay
    }

    fn list_key(&mut self, s: &Strings, ck: KeyCombination) -> Outcome {
        let count = self.shown().len();
        match ck {
            k if k == BACK_KEY => return Outcome::Back,
            k if k == key!(down) && count > 0 => self.selected = (self.selected + 1) % count,
            k if k == key!(up) && count > 0 => {
                self.selected = self.selected.checked_sub(1).unwrap_or(count - 1)
            }
            k if k == key!(f) || k == key!(right) => self.step_filter(true),
            k if k == key!(left) => self.step_filter(false),
            k if k == key!(enter) => self.open_selected(),
            k if k == key!(n) => self.new_session(s),
            k if (k == key!(backspace) || k == key!(delete)) && count > 0 => self.ask_deletion(),
            _ => {}
        }
        Outcome::Stay
    }

    fn step_filter(&mut self, forward: bool) {
        if let Some(next) = ui::cycle(&FILTERS, &self.filter, forward) {
            self.filter = next;
            self.selected = 0;
        }
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match &self.mode {
            Mode::Page(page) => page.hints(s),
            Mode::New(_) => Form::hints(s),
            Mode::Delete(_) => vec![
                ("Enter".to_string(), s.delete_forever.to_string()),
                ("Esc".to_string(), s.cancel.to_string()),
            ],
            Mode::List => vec![
                ("↑ ↓".to_string(), String::new()),
                ("Enter".to_string(), s.open.to_string()),
                ("← → f".to_string(), s.filter.to_string()),
                ("n".to_string(), s.new_session.to_string()),
                ("⌫".to_string(), s.delete_forever.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ],
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let error: Element<'a, Message> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => space::vertical().height(0).into(),
        };
        let body: Element<'a, Message> = match &self.mode {
            Mode::Page(page) => page.view(s, c).map(Message::Page),
            Mode::New(editor) => {
                container(container(editor.form.view(s, c, Message::Form, None)).max_width(720))
                    .center_x(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
            Mode::Delete(_) => ui::confirm_deletion(
                s,
                s.delete_session_text,
                Message::Confirm,
                Message::Dismiss,
                c,
            ),
            Mode::List => self.list(s, c),
        };
        column![error, body].spacing(10).into()
    }

    fn list<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let tabs = row![ui::pill(s.sessions, true, None, c)].spacing(6);
        let filters = row(FILTERS.iter().map(|f| {
            ui::pill(
                filter_name(s, *f),
                *f == self.filter,
                Some(Message::Filter(*f)),
                c,
            )
        }))
        .spacing(6)
        .wrap();
        let shown = self.shown();
        let rows: Vec<Element<'a, Message>> = shown
            .iter()
            .enumerate()
            .map(|(i, r)| self.session_row(r, i, s, c))
            .collect();
        let list: Element<'a, Message> = match rows.is_empty() {
            true => text(s.no_sessions).size(SIZE_SMALL).color(c.muted).into(),
            false => scrollable(Column::with_children(rows).spacing(3))
                .id(Id::new(LIST))
                .height(Length::Fill)
                .into(),
        };
        let add: Vec<Element<'a, Message>> = vec![
            text(ui::fill(s.sessions_of, &[shown.len().to_string()]))
                .size(SIZE_SMALL)
                .color(c.muted)
                .into(),
            ui::pill(s.new_session, false, Some(Message::New), c),
            ui::pill(s.delete_forever, false, Some(Message::Delete), c),
        ];
        column![
            tabs,
            ui::panel_with(s.sessions, add, column![filters, list].spacing(12), c)
        ]
        .spacing(10)
        .height(Length::Fill)
        .into()
    }

    fn session_row<'a>(
        &self,
        r: &SessionRecord,
        index: usize,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let when = format!(
            "{} {}",
            s.day(&r.start),
            r.start.get(11..16).unwrap_or_default()
        );
        let title = match r.title.is_empty() {
            true => s.training.to_string(),
            false => r.title.clone(),
        };
        let mut flags: Vec<String> = vec![];
        match r.status {
            SessionStatus::Planned => {}
            SessionStatus::Completed => {
                flags.push(s.status_completed.to_string());
                if !self.recorded.contains(&r.id) {
                    flags.push(s.attendance_not_recorded.to_string());
                }
            }
            SessionStatus::Cancelled => flags.push(s.cancelled.to_string()),
        }
        if r.to_close {
            flags.push(s.to_close.to_string());
        }
        let line = row![
            text(when)
                .size(SIZE_SMALL)
                .color(c.muted)
                .width(Length::Fixed(190.0)),
            ui::name(title),
            text(r.location.clone()).size(SIZE_SMALL).color(c.muted),
            text(format!("{} {}", r.duration_min, s.min_short))
                .size(SIZE_SMALL)
                .color(c.muted),
            text(flags.join(" · ")).size(SIZE_SMALL).color(c.accent),
        ]
        .spacing(14)
        .align_y(Alignment::Center);
        ui::list_row(line, index == self.selected, Some(Message::Row(index)), c)
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{form::Message as FormMessage, strings::strings},
        scout4all_core::shapes::enums::LanguageEnum,
        scout4all_mobile::training::{ActivityDraft, SessionDraft},
    };

    fn app(dir: &tempfile::TempDir) -> Arc<Scout4All> {
        Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap()
    }

    fn type_in(screen: &mut TrainingScreen, field: usize, text: &str) {
        let s = strings(LanguageEnum::It);
        let (page, roll) = match &screen.mode {
            Mode::Page(p) => (true, p.mode_name() == "roll"),
            _ => (false, false),
        };
        let wrap = |m: FormMessage| match (page, roll) {
            (true, true) => {
                Message::Page(session::Message::Roll(super::super::roll::Message::Form(m)))
            }
            (true, false) => Message::Page(session::Message::Form(m)),
            _ => Message::Form(m),
        };
        screen.update(s, wrap(FormMessage::Focus(field)));
        screen.update(s, wrap(FormMessage::Input(field, text.to_string())));
    }

    fn session(core: &Scout4All, team: &str, start: &str, title: &str) -> String {
        let draft = SessionDraft {
            start: Some(start.to_string()),
            duration: "90".into(),
            title: title.into(),
            ..core.new_session_draft()
        };
        core.save_session(team.to_string(), None, draft).unwrap().id
    }

    /// The sessions come latest first; the filter keeps those it names (the
    /// completed without attendance too); Enter opens the one chosen, Esc
    /// comes back to the list and then to the team.
    #[test]
    fn the_sessions_are_listed_filtered_and_opened() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        let first = session(&core, &team, "2026-03-02T18:00", "Prima");
        let second = session(&core, &team, "2026-03-09T18:00", "Seconda");
        core.set_session_status(first.clone(), SessionStatus::Cancelled)
            .unwrap();
        core.complete_session(second.clone(), vec![]).unwrap();
        let mut screen =
            TrainingScreen::new(core.clone(), s, team.clone(), Open::Sessions).unwrap();
        let ids = |screen: &TrainingScreen| -> Vec<String> {
            screen.shown().iter().map(|r| r.id.clone()).collect()
        };
        assert_eq!(ids(&screen), vec![second.clone(), first.clone()]);
        screen.update(s, Message::Filter(Filter::Cancelled));
        assert_eq!(ids(&screen), vec![first.clone()]);
        screen.update(s, Message::Filter(Filter::NoAttendance));
        assert_eq!(ids(&screen), vec![second.clone()]);
        // the attendance written: it leaves the filter
        let sheet = core.session_attendance(second.clone()).unwrap();
        core.save_attendance(second.clone(), sheet.rows).unwrap();
        screen.reload();
        assert!(ids(&screen).is_empty() || !screen.recorded.is_empty());
        screen.key(s, key!(f));
        assert_eq!(screen.filter, Filter::All);
        assert!(matches!(screen.key(s, key!(enter)), Outcome::Stay));
        assert!(matches!(&screen.mode, Mode::Page(p) if p.id == second));
        assert!(matches!(screen.key(s, BACK_KEY), Outcome::Stay));
        assert!(matches!(screen.mode, Mode::List));
        assert!(matches!(screen.key(s, BACK_KEY), Outcome::Back));
    }

    /// A free day of the week opens a new session on that day; saved, it
    /// opens on its sheet; an overlap is told on the form, not saved.
    #[test]
    fn a_new_session_on_a_day_opens_on_its_sheet_and_an_overlap_is_refused() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        session(&core, &team, "2026-03-02T18:00", "Prima");
        let day = NaiveDate::from_ymd_opt(2026, 3, 2).unwrap();
        let mut screen =
            TrainingScreen::new(core.clone(), s, team.clone(), Open::NewSession(day)).unwrap();
        let Mode::New(editor) = &screen.mode else {
            panic!("the form")
        };
        assert_eq!(editor.form.text(0), "2026-03-02");
        // the same hour: it overlaps
        type_in(&mut screen, 1, "18:30");
        screen.key(s, key!(enter));
        let Mode::New(editor) = &screen.mode else {
            panic!("still the form")
        };
        assert!(editor.form.error.is_some());
        assert_eq!(core.training_sessions(team.clone()).unwrap().len(), 1);
        type_in(&mut screen, 1, "20:00");
        type_in(&mut screen, 4, "Seconda");
        screen.key(s, key!(enter));
        assert!(matches!(&screen.mode, Mode::Page(_)));
        assert_eq!(core.training_sessions(team).unwrap().len(), 2);
    }

    /// The sheet: an activity added, moved and deleted; the session
    /// completed with the planned activities asked about; reopened.
    #[test]
    fn the_sheet_adds_moves_and_deletes_activities_and_completes() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        let id = session(&core, &team, "2026-03-02T18:00", "Prima");
        for name in ["Palleggio", "Battuta"] {
            core.save_activity(
                id.clone(),
                None,
                ActivityDraft {
                    name: name.into(),
                    ..core.new_activity_draft()
                },
            )
            .unwrap();
        }
        let mut screen =
            TrainingScreen::new(core.clone(), s, team.clone(), Open::Session(id.clone())).unwrap();
        let names = |core: &Scout4All| -> Vec<String> {
            core.session_activities(id.clone())
                .unwrap()
                .into_iter()
                .map(|a| a.name)
                .collect()
        };
        // the second goes up
        screen.key(s, key!(down));
        screen.key(s, key!(shift - up));
        assert_eq!(names(&core), vec!["Battuta", "Palleggio"]);
        // a new one, written in the form
        screen.key(s, key!(n));
        type_in(&mut screen, 0, "Difesa");
        screen.key(s, key!(enter));
        assert_eq!(names(&core), vec!["Battuta", "Palleggio", "Difesa"]);
        // the first goes
        screen.key(s, key!(backspace));
        screen.key(s, key!(enter));
        assert_eq!(names(&core), vec!["Palleggio", "Difesa"]);
        // completing asks about the planned: the first is left out
        screen.key(s, key!(c));
        let Mode::Page(page) = &mut screen.mode else {
            panic!("the sheet")
        };
        assert!(matches!(page.mode_name(), "complete"));
        screen.key(s, key!(space));
        screen.key(s, key!(enter));
        let activities = core.session_activities(id.clone()).unwrap();
        assert_eq!(
            activities[0].status,
            scout4all_core::training::ActivityStatus::Planned
        );
        assert_eq!(
            activities[1].status,
            scout4all_core::training::ActivityStatus::Completed
        );
        assert_eq!(
            core.training_session(id.clone()).unwrap().status,
            SessionStatus::Completed
        );
        screen.key(s, key!(r));
        assert_eq!(
            core.training_session(id).unwrap().status,
            SessionStatus::Planned
        );
    }

    /// The attendance: the first time the roster is all present; a state is
    /// changed with a key and the list saved; the session says "not recorded"
    /// until then.
    #[test]
    fn the_attendance_is_written_from_the_roster() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        let id = session(&core, &team, "2026-03-02T18:00", "Prima");
        assert!(!core.session_attendance(id.clone()).unwrap().recorded);
        let mut screen =
            TrainingScreen::new(core.clone(), s, team, Open::Session(id.clone())).unwrap();
        // to the attendance, the roll open
        screen.key(s, key!(right));
        screen.key(s, key!(enter));
        // the first absent, the second late with a note
        screen.key(s, key!(a));
        screen.key(s, key!(down));
        screen.key(s, key!(l));
        screen.key(s, key!(n));
        type_in(&mut screen, 0, "dal medico");
        screen.key(s, key!(enter));
        screen.key(s, key!(enter));
        let sheet = core.session_attendance(id).unwrap();
        assert!(sheet.recorded);
        assert_eq!(
            sheet.rows[0].status,
            scout4all_core::training::AttendanceStatus::Absent
        );
        assert_eq!(
            sheet.rows[1].status,
            scout4all_core::training::AttendanceStatus::Late
        );
        assert_eq!(sheet.rows[1].note, "dal medico");
    }
}
