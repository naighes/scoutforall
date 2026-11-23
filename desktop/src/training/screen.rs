//! The plan of the training of a team: tabs (the sessions for now), the
//! list of the sessions with its filter, and the sheet of the one opened.
//! ↑ ↓ between the sessions, Enter opens one, n makes a new one, f changes
//! the filter, ⌫ deletes the one chosen, Esc goes back.

use {
    super::{
        calendar::{self, CalendarTab},
        cycles::{self, CyclesTab, Leave},
        forms::{Edited, SessionEditor},
        history::{self, HistoryTab},
        presence::{self, PresenceTab},
        series::SeriesEditor,
        session::{self, SessionPage},
    },
    crate::{
        form::{self, Form},
        strings::Strings,
        theme::{Colors, SIZE_SMALL},
        ui,
    },
    chrono::{Datelike, Local, Months, NaiveDate},
    iced::{
        Alignment, Element, Length,
        widget::{Column, Id, column, container, row, scrollable, space, text},
    },
    scout4all_core::training::{SessionStatus, cycles::Level},
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

/// The tabs, the one shown chosen.
fn tabs<'a>(s: &'static Strings, shown: Tab, c: Colors) -> Element<'a, Message> {
    let pills = row([
        (Tab::Calendar, s.calendar),
        (Tab::Sessions, s.sessions),
        (Tab::Cycles, s.cycles),
        (Tab::Presence, s.attendance),
        (Tab::History, s.history),
    ]
    .map(|(tab, label)| {
        let go = (tab != shown).then_some(Message::Tab(tab));
        // the words are in the middle of sentences elsewhere: a tab starts
        // with a capital, as on Android
        ui::pill(&ui::capitalized(label), tab == shown, go, c)
    }))
    .spacing(6);
    let export = ui::tip(
        ui::pill(s.export_training, false, Some(Message::Export), c),
        s.training_notes_warning,
        c,
    );
    // the gyms are where sessions are held: they're added from here too,
    // not only from the home
    let venues = ui::pill(s.venues, false, Some(Message::Venues), c);
    row![pills, space::horizontal(), venues, export]
        .spacing(6)
        .into()
}

fn filter_name(s: &Strings, filter: Filter) -> &'static str {
    match filter {
        Filter::All => s.sessions_all,
        Filter::Planned => s.filter_planned,
        Filter::ToClose => s.filter_to_close,
        Filter::Completed => s.filter_completed,
        Filter::Cancelled => s.filter_cancelled,
        Filter::NoAttendance => s.filter_no_attendance,
    }
}

/// The days the list is kept to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Period {
    Any,
    Week,
    Month,
    PreviousMonth,
    NextMonth,
}

const PERIODS: [Period; 5] = [
    Period::Any,
    Period::Week,
    Period::Month,
    Period::PreviousMonth,
    Period::NextMonth,
];

pub(super) fn period_name(s: &Strings, period: Period) -> &'static str {
    match period {
        Period::Any => s.period_any,
        Period::Week => s.this_week,
        Period::Month => s.this_month,
        Period::PreviousMonth => s.previous_month,
        Period::NextMonth => s.next_month,
    }
}

impl Period {
    /// The first and the last day, both included; none for any date.
    pub fn days(self, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
        let month = |shift: i32| {
            let first = today.with_day(1)?;
            let first = match shift {
                0 => first,
                s if s < 0 => first.checked_sub_months(Months::new(s.unsigned_abs()))?,
                s => first.checked_add_months(Months::new(s.unsigned_abs()))?,
            };
            let last = first.checked_add_months(Months::new(1))?.pred_opt()?;
            Some((first, last))
        };
        match self {
            Period::Any => None,
            Period::Week => {
                let monday = today.checked_sub_days(chrono::Days::new(
                    today.weekday().num_days_from_monday().into(),
                ))?;
                Some((monday, monday.checked_add_days(chrono::Days::new(6))?))
            }
            Period::Month => month(0),
            Period::PreviousMonth => month(-1),
            Period::NextMonth => month(1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tab {
    Calendar,
    Sessions,
    Cycles,
    Presence,
    History,
}

enum Mode {
    List,
    Page(Box<SessionPage>),
    New(Box<SessionEditor>),
    Series(Box<SeriesEditor>),
    /// id of the session to delete
    Delete(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    Filter(Filter),
    Period(Period),
    Row(usize),
    New,
    Repeat,
    /// the box of a row clicked: the session marked or no more
    Mark(usize),
    Confirm,
    Dismiss,
    Form(form::Message),
    Page(session::Message),
    Tab(Tab),
    Cycles(cycles::Message),
    History(history::Message),
    Presence(presence::Message),
    Calendar(calendar::Message),
    /// the sessions marked, in a new microcycle
    Create,
    /// the archive of the training of the team
    Export,
    /// the gyms, to add or change one
    Venues,
}

pub enum Outcome {
    Stay,
    /// a match, to open
    Match(String),
    /// back to the team
    Back,
    /// a file to share: the archive, a sheet
    Export(crate::transfer::Export),
    /// the gyms: the ones of the sessions are added there
    Venues,
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
    period: Period,
    selected: usize,
    /// the sessions marked to make a microcycle
    marked: HashSet<String>,
    tab: Tab,
    calendar: CalendarTab,
    cycles: CyclesTab,
    history: HistoryTab,
    presence: PresenceTab,
    mode: Mode,
    /// the new session was asked from the sheet of a microcycle: saved or
    /// not, that sheet comes back
    from_cycle: bool,
    error: Option<String>,
}

impl TrainingScreen {
    pub fn new(
        core: Arc<Scout4All>,
        team_id: String,
        open: Open,
    ) -> Result<TrainingScreen, MobileError> {
        let team_name = core.team(team_id.clone())?.name;
        let mut screen = TrainingScreen {
            calendar: CalendarTab::new(core.clone(), team_id.clone()),
            cycles: CyclesTab::new(core.clone(), team_id.clone()),
            history: HistoryTab::new(core.clone(), team_id.clone()),
            presence: PresenceTab::new(core.clone(), team_id.clone()),
            core,
            team_id,
            team_name,
            sessions: vec![],
            recorded: HashSet::new(),
            filter: Filter::All,
            period: Period::Any,
            selected: 0,
            marked: HashSet::new(),
            tab: Tab::Sessions,
            mode: Mode::List,
            from_cycle: false,
            error: None,
        };
        screen.reload();
        match open {
            Open::Sessions => {}
            Open::Session(id) => screen.open_session(&id),
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
            Mode::Series(editor) if editor.conflicts.is_empty() => Some(editor.form.focus_id()),
            Mode::Series(_) => None,
            Mode::List if self.tab == Tab::Cycles => self.cycles.focus_id(),
            Mode::List | Mode::Delete(_) => None,
        }
    }

    /// Where the list is, to keep the session chosen in sight.
    pub fn scroll(&self) -> Option<(Id, f32)> {
        match self.mode {
            Mode::List if self.tab == Tab::Cycles => self.cycles.scroll(),
            Mode::List if self.tab == Tab::History => self.history.scroll(),
            Mode::List if self.tab == Tab::Presence => self.presence.scroll(),
            Mode::List => {
                let last = self.shown().len().saturating_sub(1).max(1);
                Some((Id::new(LIST), self.selected as f32 / last as f32))
            }
            _ => None,
        }
    }

    /// The sessions read again, the latest first; the one chosen stays.
    fn reload(&mut self) {
        self.calendar.reload();
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
        let days = self.period.days(Local::now().date_naive());
        let inside = days.is_none_or(|(from, to)| {
            r.start
                .get(..10)
                .and_then(|d| d.parse::<NaiveDate>().ok())
                .is_some_and(|d| from <= d && d <= to)
        });
        inside
            && match self.filter {
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

    fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
        match tab {
            Tab::Calendar => self.calendar.reload(),
            Tab::Sessions => self.reload(),
            Tab::Cycles => self.cycles.reload(),
            Tab::Presence => self.presence.reload(),
            Tab::History => self.history.reload(),
        }
    }

    /// The tab after (or before) the one shown, around.
    fn step_tab(&mut self, forward: bool) {
        const TABS: [Tab; 5] = [
            Tab::Calendar,
            Tab::Sessions,
            Tab::Cycles,
            Tab::Presence,
            Tab::History,
        ];
        if let Some(next) = ui::cycle(&TABS, &self.tab, forward) {
            self.set_tab(next);
        }
    }

    /// The marked sessions in a new microcycle, made on the cycles tab.
    fn create_microcycle(&mut self, s: &Strings) {
        let ids: Vec<String> = self
            .sessions
            .iter()
            .filter(|r| self.marked.contains(&r.id))
            .map(|r| r.id.clone())
            .collect();
        if ids.is_empty() {
            return;
        }
        self.marked.clear();
        self.tab = Tab::Cycles;
        self.cycles.create_from(s, Level::Microcycle, ids);
    }

    fn toggle_mark(&mut self, index: usize) {
        let free = self
            .shown()
            .get(index)
            .filter(|r| r.microcycle_id.is_none())
            .map(|r| r.id.clone());
        if let Some(id) = free.filter(|id| !self.marked.remove(id)) {
            self.marked.insert(id);
        }
    }

    /// What the cycles tab did.
    fn cycled(&mut self, s: &'static Strings, leave: Leave) -> Outcome {
        match leave {
            Leave::Stay => Outcome::Stay,
            Leave::Back => Outcome::Back,
            Leave::Export(export) => Outcome::Export(export),
            Leave::NewSession(day) => {
                match SessionEditor::new(&self.core, s, self.team_id.clone(), None, Some(day)) {
                    Ok(editor) => {
                        self.mode = Mode::New(Box::new(editor));
                        self.from_cycle = true;
                    }
                    Err(e) => self.error = Some(e.to_string()),
                }
                Outcome::Stay
            }
            Leave::Session(id) => {
                self.tab = Tab::Sessions;
                self.reload();
                self.open_session(&id);
                Outcome::Stay
            }
        }
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

    /// What the calendar asked for.
    fn calendared(&mut self, s: &'static Strings, leave: calendar::Leave) -> Outcome {
        match leave {
            calendar::Leave::Stay => {}
            calendar::Leave::Session(id) => {
                self.tab = Tab::Sessions;
                self.reload();
                self.open_session(&id);
            }
            calendar::Leave::Match(id) => return Outcome::Match(id),
            calendar::Leave::NewSession(day) => {
                match SessionEditor::new(&self.core, s, self.team_id.clone(), None, Some(day)) {
                    Ok(editor) => self.mode = Mode::New(Box::new(editor)),
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            calendar::Leave::Repeat(day) => {
                match SeriesEditor::new(&self.core, s, self.team_id.clone(), Some(day)) {
                    Ok(editor) => self.mode = Mode::Series(Box::new(editor)),
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
        }
        Outcome::Stay
    }

    fn new_session(&mut self, s: &'static Strings) {
        match SessionEditor::new(&self.core, s, self.team_id.clone(), None, None) {
            Ok(editor) => self.mode = Mode::New(Box::new(editor)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn repeat(&mut self, s: &Strings) {
        match SeriesEditor::new(&self.core, s, self.team_id.clone(), None) {
            Ok(editor) => self.mode = Mode::Series(Box::new(editor)),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// The series made: the list shows them.
    fn series_edited(&mut self, edited: Edited) {
        match edited {
            Edited::Cancel => self.mode = Mode::List,
            Edited::Saved(_) => {
                self.mode = Mode::List;
                self.filter = Filter::All;
                self.reload();
            }
            Edited::Stay | Edited::Copy(_) => {}
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
        let from_cycle = self.from_cycle;
        match edited {
            Edited::Stay | Edited::Copy(_) => return,
            Edited::Cancel => self.mode = Mode::List,
            // made from a microcycle: its sheet again, the session in it
            Edited::Saved(_) if from_cycle => {
                self.reload();
                self.cycles.refresh();
                self.mode = Mode::List;
            }
            // the new session opens on its sheet
            Edited::Saved(id) => {
                self.reload();
                self.open_session(&id);
            }
        }
        self.from_cycle = false;
    }

    fn paged(&mut self, outcome: session::Outcome) -> Outcome {
        match outcome {
            session::Outcome::Stay => {}
            session::Outcome::Export(export) => return Outcome::Export(export),
            session::Outcome::Back => {
                self.mode = Mode::List;
                self.reload();
            }
            session::Outcome::Opened(id) => {
                self.reload();
                self.open_session(&id);
            }
        }
        Outcome::Stay
    }

    pub fn update(&mut self, s: &'static Strings, message: Message) -> Outcome {
        match (&mut self.mode, message) {
            (Mode::Page(page), Message::Page(m)) => {
                let outcome = page.update(s, m);
                return self.paged(outcome);
            }
            (Mode::New(editor), Message::Form(m)) => {
                let edited = editor.update(&self.core, m);
                self.edited(edited);
            }
            (Mode::Series(editor), Message::Form(m)) => {
                let edited = editor.update(&self.core, s, m);
                self.series_edited(edited);
            }
            (Mode::Series(editor), Message::Confirm) => {
                let edited = editor.answer(&self.core, s, true);
                self.series_edited(edited);
            }
            (Mode::Series(editor), Message::Dismiss) => {
                let edited = editor.answer(&self.core, s, false);
                self.series_edited(edited);
            }
            (Mode::Delete(_), Message::Confirm) => self.delete(),
            (Mode::Delete(_), Message::Dismiss) => self.mode = Mode::List,
            (Mode::List, Message::Tab(tab)) => self.set_tab(tab),
            (Mode::List, Message::Cycles(m)) if self.tab == Tab::Cycles => {
                let leave = self.cycles.update(s, m);
                return self.cycled(s, leave);
            }
            (Mode::List, Message::History(m)) if self.tab == Tab::History => self.history.update(m),
            (Mode::List, Message::Presence(m)) if self.tab == Tab::Presence => {
                self.presence.update(m)
            }
            (Mode::List, Message::Calendar(m)) if self.tab == Tab::Calendar => {
                let leave = self.calendar.update(m);
                return self.calendared(s, leave);
            }
            (Mode::List, Message::Period(period)) => {
                self.period = period;
                self.selected = 0;
            }
            (Mode::List, Message::Create) => self.create_microcycle(s),
            (Mode::List, Message::Export) => return self.export_outcome(),
            (Mode::List, Message::Venues) => return Outcome::Venues,
            (Mode::List, Message::Filter(filter)) => {
                self.filter = filter;
                self.selected = 0;
            }
            (Mode::List, Message::Row(i)) => {
                self.selected = i;
                self.open_selected();
            }
            (Mode::List, Message::New) => self.new_session(s),
            (Mode::List, Message::Repeat) => self.repeat(s),
            (Mode::List, Message::Mark(i)) => {
                self.selected = i;
                self.toggle_mark(i);
            }
            _ => {}
        }
        Outcome::Stay
    }

    fn export_outcome(&self) -> Outcome {
        Outcome::Export(crate::transfer::Export::Training(self.team_id.clone()))
    }

    pub fn key(&mut self, s: &'static Strings, ck: KeyCombination) -> Outcome {
        // x shares the archive from any tab (the cycles one, when it isn't in a form)
        let on_tabs =
            matches!(self.mode, Mode::List) && (self.tab != Tab::Cycles || self.cycles.on_list());
        if on_tabs && ck == key!(x) {
            return self.export_outcome();
        }
        match &mut self.mode {
            Mode::Page(page) => {
                let outcome = page.key(s, ck);
                return self.paged(outcome);
            }
            Mode::New(editor) => {
                let edited = editor.key(&self.core, ck);
                self.edited(edited);
            }
            Mode::Series(editor) => {
                let edited = editor.key(&self.core, s, ck);
                self.series_edited(edited);
            }
            Mode::Delete(_) => match ck {
                k if k == key!(enter) => self.delete(),
                k if k == BACK_KEY => self.mode = Mode::List,
                _ => {}
            },
            Mode::List if self.tab == Tab::Cycles => {
                if self.cycles.on_list() && (ck == key!(tab) || ck == key!(shift - tab)) {
                    self.step_tab(ck == key!(tab));
                    return Outcome::Stay;
                }
                let leave = self.cycles.key(s, ck);
                return self.cycled(s, leave);
            }
            Mode::List if self.tab == Tab::Calendar => match ck {
                k if k == BACK_KEY => return Outcome::Back,
                k if k == key!(tab) || k == key!(shift - tab) => self.step_tab(k == key!(tab)),
                k => {
                    let leave = self.calendar.key(k);
                    return self.calendared(s, leave);
                }
            },
            Mode::List if self.tab == Tab::Presence => match ck {
                k if k == BACK_KEY => return Outcome::Back,
                k if k == key!(tab) || k == key!(shift - tab) => self.step_tab(k == key!(tab)),
                k => self.presence.key(k),
            },
            Mode::List if self.tab == Tab::History => match ck {
                k if k == BACK_KEY => return Outcome::Back,
                k if k == key!(tab) || k == key!(shift - tab) => self.step_tab(k == key!(tab)),
                k => self.history.key(k),
            },
            Mode::List => return self.list_key(s, ck),
        }
        Outcome::Stay
    }

    fn list_key(&mut self, s: &'static Strings, ck: KeyCombination) -> Outcome {
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
            k if k == key!(tab) || k == key!(shift - tab) => self.step_tab(k == key!(tab)),
            k if k == key!(p) => self.step_period(),
            k if k == key!(space) => self.toggle_mark(self.selected),
            k if k == key!(m) => self.create_microcycle(s),
            k if k == key!(n) => self.new_session(s),
            k if k == key!(r) => self.repeat(s),
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

    fn step_period(&mut self) {
        if let Some(next) = ui::cycle(&PERIODS, &self.period, true) {
            self.period = next;
            self.selected = 0;
        }
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match &self.mode {
            Mode::Page(page) => page.hints(s),
            Mode::New(editor) => editor.form.on_field(Form::hints(s)),
            Mode::Series(editor) if !editor.conflicts.is_empty() => vec![
                ("Enter".to_string(), s.create_others.to_string()),
                ("Esc".to_string(), s.cancel.to_string()),
            ],
            Mode::Series(_) => Form::hints(s),
            Mode::Delete(_) => vec![
                ("Enter".to_string(), s.delete.to_string()),
                ("Esc".to_string(), s.cancel.to_string()),
            ],
            Mode::List if self.tab == Tab::Cycles => self.cycles.hints(s),
            Mode::List if self.tab == Tab::Calendar => self.calendar.hints(s),
            Mode::List if self.tab == Tab::Presence => self.presence.hints(s),
            Mode::List if self.tab == Tab::History => self.history.hints(s),
            Mode::List => vec![
                ("↑ ↓".to_string(), String::new()),
                ("space".to_string(), s.select_row.to_string()),
                ("m".to_string(), s.create_microcycle.to_string()),
                ("⇥".to_string(), s.cycles.to_string()),
                ("Enter".to_string(), s.open.to_string()),
                ("← → f".to_string(), s.filter.to_string()),
                ("p".to_string(), s.period.to_string()),
                ("n".to_string(), s.new_session.to_string()),
                ("r".to_string(), s.repeat_sessions.to_string()),
                ("x".to_string(), s.export_training.to_string()),
                ("⌫".to_string(), s.delete.to_string()),
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
            Mode::Series(editor) if !editor.conflicts.is_empty() => {
                editor.dialog(s, c, Message::Confirm, Message::Dismiss)
            }
            Mode::Series(editor) => {
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
            Mode::List if self.tab == Tab::Cycles => column![
                tabs(s, Tab::Cycles, c),
                self.cycles.view(s, c).map(Message::Cycles)
            ]
            .spacing(10)
            .height(Length::Fill)
            .into(),
            Mode::List if self.tab == Tab::Calendar => column![
                tabs(s, Tab::Calendar, c),
                self.calendar.view(s, c).map(Message::Calendar)
            ]
            .spacing(10)
            .height(Length::Fill)
            .into(),
            Mode::List if self.tab == Tab::Presence => column![
                tabs(s, Tab::Presence, c),
                self.presence.view(s, c).map(Message::Presence)
            ]
            .spacing(10)
            .height(Length::Fill)
            .into(),
            Mode::List if self.tab == Tab::History => column![
                tabs(s, Tab::History, c),
                self.history.view(s, c).map(Message::History)
            ]
            .spacing(10)
            .height(Length::Fill)
            .into(),
            Mode::List => self.list(s, c),
        };
        column![error, body].spacing(10).into()
    }

    fn list<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let periods = row(PERIODS.iter().map(|p| {
            ui::pill(
                period_name(s, *p),
                *p == self.period,
                Some(Message::Period(*p)),
                c,
            )
        }))
        .spacing(6)
        .wrap();
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
        // only what can be done now: the microcycle once sessions are
        // marked; a session is deleted from its sheet (or ⌫ on its row)
        let mut add: Vec<Element<'a, Message>> = vec![
            text(ui::fill(s.sessions_of, &[shown.len().to_string()]))
                .size(SIZE_SMALL)
                .color(c.muted)
                .into(),
            ui::pill(s.new_session, false, Some(Message::New), c),
            ui::pill(s.repeat_sessions, false, Some(Message::Repeat), c),
        ];
        if !self.marked.is_empty() {
            add.push(ui::pill(
                &format!("{} ({})", s.create_microcycle, self.marked.len()),
                true,
                Some(Message::Create),
                c,
            ));
        }
        column![
            tabs(s, Tab::Sessions, c),
            ui::panel_with(
                s.sessions,
                add,
                column![filters, periods, list].spacing(12),
                c
            )
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
            text(format!("{} {}", r.duration_min, s.min_short))
                .size(SIZE_SMALL)
                .color(c.muted),
            text(flags.join(" · ")).size(SIZE_SMALL).color(c.accent),
        ]
        .spacing(14)
        .align_y(Alignment::Center);
        // the box of a free session marks it with a click too, not only
        // with space: the row itself opens the session
        let mark = match (r.microcycle_id.is_none(), self.marked.contains(&r.id)) {
            (true, marked) => ui::mark_box(marked, Message::Mark(index), c),
            (false, _) => space::horizontal().width(ui::MARK_WIDTH).into(),
        };
        row![
            mark,
            ui::list_row(line, index == self.selected, Some(Message::Row(index)), c)
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
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
        let cycles = screen.tab == Tab::Cycles && !page;
        let wrap = |m: FormMessage| match (page, roll) {
            _ if cycles => Message::Cycles(cycles::Message::Form(m)),
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
        let mut screen = TrainingScreen::new(core.clone(), team.clone(), Open::Sessions).unwrap();
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

    /// The period keeps the sessions of its days, both ends included; a
    /// month before and after reads the right ends; `p` steps through them.
    #[test]
    fn the_period_keeps_the_sessions_of_its_days() {
        let day = |d: &str| d.parse::<NaiveDate>().unwrap();
        let today = day("2026-03-11");
        assert_eq!(Period::Any.days(today), None);
        assert_eq!(
            Period::Week.days(today),
            Some((day("2026-03-09"), day("2026-03-15")))
        );
        assert_eq!(
            Period::Month.days(today),
            Some((day("2026-03-01"), day("2026-03-31")))
        );
        assert_eq!(
            Period::PreviousMonth.days(day("2026-03-31")),
            Some((day("2026-02-01"), day("2026-02-28")))
        );
        assert_eq!(
            Period::NextMonth.days(day("2025-12-31")),
            Some((day("2026-01-01"), day("2026-01-31")))
        );
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        let now = Local::now().date_naive();
        let far = now.checked_add_months(Months::new(24)).unwrap();
        let near = session(&core, &team, &format!("{now}T03:00"), "Oggi");
        let later = session(&core, &team, &format!("{far}T18:00"), "Lontana");
        let mut screen = TrainingScreen::new(core, team, Open::Sessions).unwrap();
        let ids = |screen: &TrainingScreen| -> Vec<String> {
            screen.shown().iter().map(|r| r.id.clone()).collect()
        };
        assert_eq!(ids(&screen), vec![later.clone(), near.clone()]);
        screen.key(s, key!(p));
        assert_eq!(screen.period, Period::Week);
        assert_eq!(ids(&screen), vec![near.clone()]);
        screen.update(s, Message::Period(Period::NextMonth));
        assert!(ids(&screen).is_empty() || ids(&screen) == vec![near.clone()]);
        screen.update(s, Message::Period(Period::Any));
        assert_eq!(ids(&screen).len(), 2);
    }

    /// Tab goes round sessions, cycles, history, calendar and back; the
    /// history has nothing to change.
    #[test]
    fn tab_goes_round_the_five_tabs() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        let mut screen = TrainingScreen::new(core, team, Open::Sessions).unwrap();
        for tab in [
            Tab::Cycles,
            Tab::Presence,
            Tab::History,
            Tab::Calendar,
            Tab::Sessions,
        ] {
            screen.key(s, key!(tab));
            assert_eq!(screen.tab, tab);
        }
        screen.key(s, key!(shift - tab));
        assert_eq!(screen.tab, Tab::Calendar);
        assert!(matches!(screen.key(s, BACK_KEY), Outcome::Back));
    }

    /// A day of the calendar opens a new session on that day; saved, it
    /// opens on its sheet; an overlap is told on the form, not saved.
    #[test]
    fn a_new_session_on_a_day_opens_on_its_sheet_and_an_overlap_is_refused() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        session(&core, &team, "2026-03-02T18:00", "Prima");
        let day = NaiveDate::from_ymd_opt(2026, 3, 2).unwrap();
        let mut screen = TrainingScreen::new(core.clone(), team.clone(), Open::Sessions).unwrap();
        screen.calendared(s, calendar::Leave::NewSession(day));
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
            TrainingScreen::new(core.clone(), team.clone(), Open::Session(id.clone())).unwrap();
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
            TrainingScreen::new(core.clone(), team, Open::Session(id.clone())).unwrap();
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

    /// The duplicate: a week later, the activities copied; it opens on the
    /// new sheet. An overlap is told on the form and nothing is written.
    #[test]
    fn a_session_is_duplicated_and_an_overlap_is_refused() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        let id = session(&core, &team, "2026-03-02T18:00", "Prima");
        core.save_activity(
            id.clone(),
            None,
            ActivityDraft {
                name: "Palleggio".into(),
                ..core.new_activity_draft()
            },
        )
        .unwrap();
        let busy = session(&core, &team, "2026-03-09T18:30", "Occupata");
        let mut screen =
            TrainingScreen::new(core.clone(), team.clone(), Open::Session(id.clone())).unwrap();
        screen.key(s, key!(d));
        let Mode::Page(page) = &screen.mode else {
            panic!("the sheet")
        };
        assert_eq!(page.mode_name(), "duplicate");
        screen.key(s, key!(enter));
        assert_eq!(core.training_sessions(team.clone()).unwrap().len(), 2);
        // another hour: it is saved and its sheet opens
        type_in(&mut screen, 1, "20:00");
        screen.key(s, key!(enter));
        let sessions = core.training_sessions(team.clone()).unwrap();
        assert_eq!(sessions.len(), 3);
        let copy = sessions
            .iter()
            .find(|r| r.id != id && r.id != busy)
            .unwrap();
        assert_eq!(copy.start, "2026-03-09T20:00");
        assert!(matches!(&screen.mode, Mode::Page(p) if p.id == copy.id));
        assert_eq!(core.session_activities(copy.id.clone()).unwrap().len(), 1);
    }

    /// The series: a session on the way is told in a dialog and nothing is
    /// made; the others are made on Enter. The group is then changed from
    /// the sheet of one of them, from it on.
    #[test]
    fn a_series_tells_its_conflicts_and_is_changed_as_a_group() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        session(&core, &team, "2026-03-09T20:00", "Occupata");
        let mut screen = TrainingScreen::new(core.clone(), team.clone(), Open::Sessions).unwrap();
        screen.key(s, key!(r));
        assert!(matches!(screen.mode, Mode::Series(_)));
        type_in(&mut screen, 0, "2026-03-02");
        type_in(&mut screen, 1, "2026-03-16");
        type_in(&mut screen, 3, "20:30");
        type_in(&mut screen, 4, "120");
        screen.key(s, key!(enter));
        let Mode::Series(editor) = &screen.mode else {
            panic!("the dialog")
        };
        assert_eq!(editor.conflicts, vec!["2026-03-09 20:30".to_string()]);
        assert_eq!(core.training_sessions(team.clone()).unwrap().len(), 1);
        // Esc goes back to the form, Enter again asks again
        screen.key(s, BACK_KEY);
        assert!(matches!(&screen.mode, Mode::Series(e) if e.conflicts.is_empty()));
        screen.key(s, key!(enter));
        screen.key(s, key!(enter));
        assert!(matches!(screen.mode, Mode::List));
        let sessions = core.training_sessions(team.clone()).unwrap();
        assert_eq!(sessions.len(), 3);
        let first = sessions
            .iter()
            .find(|r| r.start == "2026-03-02T20:30")
            .unwrap();
        assert!(first.series_id.is_some());
        // from the last on: the time moves
        let last = sessions
            .iter()
            .find(|r| r.start == "2026-03-16T20:30")
            .unwrap()
            .id
            .clone();
        let mut screen =
            TrainingScreen::new(core.clone(), team.clone(), Open::Session(last.clone())).unwrap();
        screen.key(s, key!(g));
        screen.update(
            s,
            Message::Page(session::Message::Form(FormMessage::Choose(0, 1))),
        );
        type_in(&mut screen, 2, "19:00");
        screen.key(s, key!(enter));
        let starts: Vec<String> = core
            .training_sessions(team.clone())
            .unwrap()
            .into_iter()
            .map(|r| r.start)
            .collect();
        assert!(starts.contains(&"2026-03-02T20:30".to_string()));
        assert!(starts.contains(&"2026-03-16T19:00".to_string()));
        // removed from the first on: the sheet closes
        let mut screen =
            TrainingScreen::new(core.clone(), team.clone(), Open::Session(first.id.clone()))
                .unwrap();
        screen.key(s, key!(g));
        screen.update(
            s,
            Message::Page(session::Message::Form(FormMessage::Choose(4, 1))),
        );
        screen.key(s, key!(enter));
        assert!(matches!(screen.mode, Mode::List));
        assert_eq!(core.training_sessions(team).unwrap().len(), 1);
    }
    /// Sessions marked with space become a microcycle (window proposed from
    /// the first Monday); the sheet gives the totals; the microcycle is
    /// duplicated and the two marked make a mesocycle.
    #[test]
    fn marked_sessions_make_a_microcycle_and_microcycles_a_mesocycle() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        session(&core, &team, "2026-03-02T18:00", "Lunedì");
        session(&core, &team, "2026-03-04T18:00", "Mercoledì");
        let mut screen = TrainingScreen::new(core.clone(), team.clone(), Open::Sessions).unwrap();
        screen.key(s, key!(space));
        screen.key(s, key!(down));
        screen.key(s, key!(space));
        screen.key(s, key!(m));
        assert_eq!(screen.tab, Tab::Cycles);
        assert!(screen.focus_id().is_some());
        screen.key(s, key!(enter));
        let micros = core.cycles(team.clone(), Level::Microcycle).unwrap();
        assert_eq!(micros.len(), 1);
        assert_eq!(micros[0].first_day, "2026-03-02");
        assert!(
            core.training_sessions(team.clone())
                .unwrap()
                .iter()
                .all(|r| r.microcycle_id.as_deref() == Some(micros[0].id.as_str()))
        );
        // the sheet: totals and the two sessions
        let (totals, children) = screen.cycles.card_totals().unwrap();
        assert_eq!(totals.planned, 2);
        assert_eq!(totals.planned_minutes, 180);
        assert_eq!(children, 2);
        // duplicated a week later
        screen.key(s, key!(d));
        screen.key(s, key!(enter));
        assert_eq!(
            core.cycles(team.clone(), Level::Microcycle).unwrap().len(),
            2
        );
        // the two free microcycles make a mesocycle
        screen.key(s, BACK_KEY);
        screen.key(s, key!(space));
        screen.key(s, key!(down));
        screen.key(s, key!(space));
        screen.key(s, key!(m));
        screen.key(s, key!(enter));
        let mesos = core.cycles(team.clone(), Level::Mesocycle).unwrap();
        assert_eq!(mesos.len(), 1);
        assert_eq!(mesos[0].weeks, Some(2));
        assert!(
            core.cycles(team, Level::Microcycle)
                .unwrap()
                .iter()
                .all(|c| c.parent_id.as_deref() == Some(mesos[0].id.as_str()))
        );
    }

    /// The same with the mouse: a click on the box of a row marks it (the
    /// row itself opens the session), Create makes the microcycle only
    /// from what is marked; on the cycles tab the same for the mesocycle.
    #[test]
    fn the_boxes_mark_with_a_click_and_create_needs_something_marked() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        session(&core, &team, "2026-03-02T18:00", "Lunedì");
        session(&core, &team, "2026-03-04T18:00", "Mercoledì");
        let mut screen = TrainingScreen::new(core.clone(), team.clone(), Open::Sessions).unwrap();
        // nothing marked: Create does nothing
        screen.update(s, Message::Create);
        assert_eq!(screen.tab, Tab::Sessions);
        // a box clicked twice is no more marked; a click marks without
        // opening the session
        screen.update(s, Message::Mark(0));
        screen.update(s, Message::Mark(0));
        assert!(screen.marked.is_empty());
        screen.update(s, Message::Mark(0));
        screen.update(s, Message::Mark(1));
        assert!(matches!(screen.mode, Mode::List));
        assert_eq!(screen.marked.len(), 2);
        screen.update(s, Message::Create);
        assert_eq!(screen.tab, Tab::Cycles);
        screen.key(s, key!(enter));
        assert_eq!(
            core.cycles(team.clone(), Level::Microcycle).unwrap().len(),
            1
        );
        // the cycles tab: a new microcycle, empty, then the two marked by
        // click make a mesocycle
        screen.key(s, BACK_KEY);
        screen.update(s, Message::Cycles(cycles::Message::New));
        type_in(&mut screen, 1, "2026-03-16");
        screen.key(s, key!(enter));
        screen.key(s, BACK_KEY);
        screen.update(s, Message::Cycles(cycles::Message::Create));
        assert!(
            core.cycles(team.clone(), Level::Mesocycle)
                .unwrap()
                .is_empty()
        );
        screen.update(s, Message::Cycles(cycles::Message::Mark(0)));
        screen.update(s, Message::Cycles(cycles::Message::Mark(1)));
        screen.update(s, Message::Cycles(cycles::Message::Create));
        screen.key(s, key!(enter));
        assert_eq!(core.cycles(team, Level::Mesocycle).unwrap().len(), 1);
    }

    /// The sheet of a cycle makes its children: an empty program offers a
    /// new mesocycle (in it, from its first day) and nothing to add, since
    /// no mesocycle is free; saved, the program's sheet comes back with it.
    /// The same down to a microcycle, whose new session lands in it. A free
    /// one in the window brings the choice to add the existing ones.
    #[test]
    fn the_sheet_of_a_cycle_makes_its_children_and_adds_only_free_ones() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        let program = core
            .save_cycle(
                team.clone(),
                Level::Program,
                None,
                scout4all_mobile::training::CycleDraft {
                    title: "Stagione".into(),
                    start: Some("2026-03-02".into()),
                    end: Some("2026-06-28".into()),
                    ..core.new_cycle_draft()
                },
                vec![],
                false,
            )
            .unwrap();
        let mut screen = TrainingScreen::new(core.clone(), team.clone(), Open::Sessions).unwrap();
        screen.update(s, Message::Tab(Tab::Cycles));
        screen.update(s, Message::Cycles(cycles::Message::Level(Level::Program)));
        screen.key(s, key!(enter));
        let keys = |screen: &TrainingScreen| -> Vec<String> {
            screen.hints(s).into_iter().map(|(k, _)| k).collect()
        };
        // nothing free: no "a", and "a" does nothing
        assert!(keys(&screen).contains(&"n".to_string()));
        assert!(!keys(&screen).contains(&"a".to_string()));
        screen.key(s, key!(a));
        assert!(screen.cycles.on_card());
        // a new mesocycle, already in the program, from its first day
        screen.key(s, key!(n));
        type_in(&mut screen, 2, "4");
        screen.key(s, key!(enter));
        let mesos = core.cycles(team.clone(), Level::Mesocycle).unwrap();
        assert_eq!(mesos.len(), 1);
        assert_eq!(mesos[0].parent_id.as_deref(), Some(program.id.as_str()));
        assert_eq!(mesos[0].first_day, "2026-03-02");
        assert_eq!(screen.cycles.card_totals().map(|(_, n)| n), Some(1));
        // a cancelled form brings back the program's sheet, at its level
        screen.update(s, Message::Cycles(cycles::Message::NewChild));
        screen.key(s, BACK_KEY);
        assert!(screen.cycles.on_card());
        assert_eq!(screen.cycles.card_totals().map(|(_, n)| n), Some(1));
        // down to a microcycle, then a session in it
        screen.key(s, key!(enter));
        screen.key(s, key!(n));
        type_in(&mut screen, 2, "1");
        screen.key(s, key!(enter));
        let micros = core.cycles(team.clone(), Level::Microcycle).unwrap();
        assert_eq!(micros.len(), 1);
        assert_eq!(micros[0].parent_id.as_deref(), Some(mesos[0].id.as_str()));
        screen.key(s, key!(enter));
        screen.key(s, key!(n));
        assert!(matches!(screen.mode, Mode::New(_)));
        screen.update(s, Message::Form(FormMessage::Focus(1)));
        screen.update(s, Message::Form(FormMessage::Input(1, "18:00".into())));
        screen.key(s, key!(enter));
        assert!(matches!(screen.mode, Mode::List));
        assert!(screen.cycles.on_card());
        assert_eq!(screen.cycles.card_totals().map(|(_, n)| n), Some(1));
        // a free session in the window: now the existing ones can be added
        session(&core, &team, "2026-03-04T18:00", "Libera");
        screen.key(s, BACK_KEY);
        screen.key(s, key!(enter));
        assert!(keys(&screen).contains(&"a".to_string()));
        screen.key(s, key!(a));
        assert!(!screen.cycles.on_card());
    }

    /// A shorter window that leaves a child out stops the save and says so;
    /// Enter again detaches it. Deleting a microcycle frees its sessions.
    #[test]
    fn a_shorter_window_detaches_what_falls_out_and_a_deletion_frees_children() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        session(&core, &team, "2026-03-02T18:00", "Prima");
        session(&core, &team, "2026-03-09T18:00", "Seconda");
        let mut screen = TrainingScreen::new(core.clone(), team.clone(), Open::Sessions).unwrap();
        screen.key(s, key!(space));
        screen.key(s, key!(down));
        screen.key(s, key!(space));
        screen.key(s, key!(m));
        screen.key(s, key!(enter));
        assert_eq!(
            core.cycles(team.clone(), Level::Microcycle).unwrap()[0].weeks,
            Some(2)
        );
        // one week only: the second session falls out
        screen.key(s, key!(e));
        type_in(&mut screen, 2, "1");
        screen.key(s, key!(enter));
        let Mode::List = screen.mode else {
            panic!("the list")
        };
        assert!(screen.cycles.form_error().is_some());
        assert_eq!(
            core.cycles(team.clone(), Level::Microcycle).unwrap()[0].weeks,
            Some(2)
        );
        screen.key(s, key!(enter));
        let micro = core.cycles(team.clone(), Level::Microcycle).unwrap();
        assert_eq!(micro[0].weeks, Some(1));
        let attached = |core: &Scout4All| {
            core.training_sessions(team.clone())
                .unwrap()
                .iter()
                .filter(|r| r.microcycle_id.is_some())
                .count()
        };
        assert_eq!(attached(&core), 1);
        // deleted: the session left is free, not gone
        screen.key(s, key!(backspace));
        screen.key(s, key!(enter));
        assert!(
            core.cycles(team.clone(), Level::Microcycle)
                .unwrap()
                .is_empty()
        );
        assert_eq!(core.training_sessions(team.clone()).unwrap().len(), 2);
        assert_eq!(attached(&core), 0);
        assert!(screen.cycles.on_list());
    }

    /// The sheet of a cycle names what it edits and what it makes by its
    /// level; the deletion asks first, and `a` deletes what it contains too.
    #[test]
    fn the_sheet_names_its_level_and_a_deletion_can_take_the_children() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        session(&core, &team, "2026-03-02T18:00", "Lunedì");
        session(&core, &team, "2026-03-04T18:00", "Mercoledì");
        let mut screen = TrainingScreen::new(core.clone(), team.clone(), Open::Sessions).unwrap();
        screen.update(s, Message::Mark(0));
        screen.update(s, Message::Mark(1));
        screen.update(s, Message::Create);
        screen.key(s, key!(enter));
        let hints = |screen: &TrainingScreen| -> Vec<String> {
            screen.hints(s).into_iter().map(|(_, what)| what).collect()
        };
        let shown = hints(&screen);
        assert!(shown.iter().any(|h| h == s.edit_microcycle));
        // a new session in it; no free one to add
        assert!(shown.iter().any(|h| h == s.new_session));
        assert!(!shown.iter().any(|h| h == s.add_sessions));
        assert!(shown.iter().any(|h| h == s.delete));
        // asked: Esc keeps it
        screen.key(s, key!(backspace));
        assert!(hints(&screen).iter().any(|h| h == s.remove_with_children));
        screen.key(s, BACK_KEY);
        assert_eq!(
            core.cycles(team.clone(), Level::Microcycle).unwrap().len(),
            1
        );
        // `a`: the sessions go with it
        screen.key(s, key!(backspace));
        screen.key(s, key!(a));
        assert!(
            core.cycles(team.clone(), Level::Microcycle)
                .unwrap()
                .is_empty()
        );
        assert!(core.training_sessions(team).unwrap().is_empty());
        assert!(screen.cycles.on_list());
    }
    fn press(screen: &mut TrainingScreen, k: KeyCombination) -> Outcome {
        screen.key(strings(LanguageEnum::It), k)
    }

    fn microcycle(core: &Scout4All, team: &str, title: &str, start: &str) {
        let draft = scout4all_mobile::training::CycleDraft {
            title: title.into(),
            start: Some(start.into()),
            weeks: "1".into(),
            ..core.new_cycle_draft()
        };
        core.save_cycle(team.into(), Level::Microcycle, None, draft, vec![], false)
            .unwrap();
    }

    /// The calendar shows the week of the day with the sessions and the
    /// bands of its microcycles; the month shows the days of the month with
    /// the bands of the mesocycles; PgDn moves the period, Home goes back.
    #[test]
    fn the_calendar_reads_the_week_the_month_and_the_bands() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        session(&core, &team, "2026-03-10T18:00", "Martedì");
        session(&core, &team, "2026-03-25T18:00", "Fine mese");
        microcycle(&core, &team, "Carico", "2026-03-09");
        let today = NaiveDate::from_ymd_opt(2026, 3, 11).unwrap();
        let mut tab = super::super::calendar::CalendarTab::at(core.clone(), team.clone(), today);
        assert_eq!(tab.days().len(), 7);
        assert_eq!(tab.days().iter().map(|d| d.events.len()).sum::<usize>(), 1);
        assert_eq!(tab.bands().len(), 1);
        assert_eq!(tab.bands()[0].title, "Carico");
        tab.key(key!(pagedown));
        assert_eq!(tab.anchor(), NaiveDate::from_ymd_opt(2026, 3, 18).unwrap());
        assert!(tab.bands().is_empty());
        tab.key(key!(home));
        assert_eq!(tab.anchor(), today);
        tab.key(key!(m));
        assert_eq!(tab.days().len(), 31);
        assert_eq!(tab.days().iter().map(|d| d.events.len()).sum::<usize>(), 2);
        let _ = s;
    }

    /// The bands of a row of days: the days under a cycle together, the
    /// free ones apart, the days out of the month under none.
    #[test]
    fn the_bands_group_the_days_under_a_cycle() {
        use super::super::calendar::segments;
        let day = |d: u32| NaiveDate::from_ymd_opt(2026, 3, d);
        let cycle = |id: &str, first: &str, last: &str| scout4all_mobile::training::CycleRecord {
            id: id.into(),
            team_id: "t".into(),
            level: Level::Microcycle,
            parent_id: None,
            title: id.into(),
            weeks: Some(1),
            first_day: first.into(),
            last_day: last.into(),
            window: String::new(),
            objectives: String::new(),
            notes: String::new(),
        };
        let cycles = [
            cycle("a", "2026-03-09", "2026-03-11"),
            cycle("b", "2026-03-12", "2026-03-12"),
        ];
        let row = [None, day(9), day(10), day(11), day(12), day(13), None];
        let got: Vec<(usize, usize, Option<String>)> = segments(&row, &cycles)
            .into_iter()
            .map(|(i, n, c)| (i, n, c.map(|c| c.id.clone())))
            .collect();
        assert_eq!(
            got,
            vec![
                (0, 1, None),
                (1, 3, Some("a".into())),
                (4, 1, Some("b".into())),
                (5, 2, None),
            ]
        );
    }

    /// Enter on an event opens the session (on the sessions tab) or the
    /// match; on a free day a new session on that day; `r` a series from
    /// the day; in the month Enter goes to the week of the day.
    #[test]
    fn the_calendar_opens_events_and_creates_on_a_day() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        let id = session(&core, &team, "2026-03-10T18:00", "Martedì");
        let mut screen = TrainingScreen::new(core.clone(), team.clone(), Open::Sessions).unwrap();
        screen.calendar = super::super::calendar::CalendarTab::at(
            core.clone(),
            team.clone(),
            NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
        );
        press(&mut screen, key!(shift - tab));
        assert_eq!(screen.tab, Tab::Calendar);
        press(&mut screen, key!(enter));
        assert_eq!(screen.tab, Tab::Sessions);
        let Mode::Page(page) = &screen.mode else {
            panic!("the event opens its sheet")
        };
        let _ = (page, &id);
        press(&mut screen, BACK_KEY);
        press(&mut screen, key!(shift - tab));
        assert_eq!(screen.tab, Tab::Calendar);
        // a free day: a new session on it
        press(&mut screen, key!(right));
        press(&mut screen, key!(enter));
        assert!(matches!(screen.mode, Mode::New(_)));
        press(&mut screen, BACK_KEY);
        assert!(matches!(screen.mode, Mode::List));
        // a series from the day
        press(&mut screen, key!(r));
        assert!(matches!(screen.mode, Mode::Series(_)));
        press(&mut screen, BACK_KEY);
        // the month: Enter comes back to the week of the day
        press(&mut screen, key!(m));
        press(&mut screen, key!(enter));
        assert_eq!(
            screen.calendar.view_shown(),
            super::super::calendar::View::Week
        );
        assert!(matches!(press(&mut screen, BACK_KEY), Outcome::Back));
    }

    /// x shares the archive of the training from the list; p on a session
    /// asks for the options of its sheet and shares the PDF; p on a
    /// microcycle shares its plan. The files are really made.
    #[test]
    fn the_archive_and_the_pdfs_are_shared() {
        use crate::transfer::{self, Export};
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        let id = session(&core, &team, "2026-03-10T18:00", "Martedì");
        microcycle(&core, &team, "Settimana", "2026-03-09");
        let mut screen = TrainingScreen::new(core.clone(), team.clone(), Open::Sessions).unwrap();
        let Outcome::Export(export) = press(&mut screen, key!(x)) else {
            panic!("x shares the archive");
        };
        assert!(matches!(&export, Export::Training(t) if *t == team));
        let (name, bytes) = transfer::archive(&core, &export).unwrap();
        assert!(name.ends_with(".zip") && !bytes.is_empty());
        // the sheet
        screen.key(s, key!(enter));
        assert!(matches!(press(&mut screen, key!(p)), Outcome::Stay));
        press(&mut screen, key!(space));
        let Outcome::Export(export) = press(&mut screen, key!(enter)) else {
            panic!("Enter shares the sheet");
        };
        assert!(matches!(&export, Export::SessionSheet(i, true, false) if *i == id));
        let (name, bytes) = transfer::archive(&core, &export).unwrap();
        assert!(name.ends_with(".pdf") && bytes.starts_with(b"%PDF"));
        // Esc closes the options without sharing
        press(&mut screen, key!(p));
        assert!(matches!(press(&mut screen, BACK_KEY), Outcome::Stay));
        // the plan of the microcycle
        press(&mut screen, BACK_KEY);
        screen.update(s, Message::Tab(Tab::Cycles));
        press(&mut screen, key!(enter));
        let Outcome::Export(export) = press(&mut screen, key!(p)) else {
            panic!("p shares the plan");
        };
        let (_, bytes) = transfer::archive(&core, &export).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    /// `t` shows every team's events, each with its team; `v` limits them to
    /// a gym (and then none); the ones that share a gym with another team
    /// at the same time are marked; what another team holds is not opened.
    #[test]
    fn the_calendar_of_all_teams_filters_by_gym_and_marks_the_overlaps() {
        use super::super::calendar::{CalendarTab, Leave};
        use scout4all_core::shapes::{
            country::CountryCode,
            enums::{GenderEnum, TeamClassificationEnum},
        };
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let a = core.tutorial_team().unwrap();
        let b = core
            .save_team(
                None,
                scout4all_core::team_form::TeamForm {
                    name: "Vigili".to_string(),
                    classification: Some(TeamClassificationEnum::Regional),
                    gender: Some(GenderEnum::Men),
                    country: CountryCode::known("IT"),
                    ..core.new_team_form(2026)
                },
            )
            .unwrap();
        let gym = core
            .save_venue(
                None,
                scout4all_mobile::venues::VenueDraft {
                    name: "Terontola".into(),
                    ..Default::default()
                },
            )
            .unwrap()
            .id;
        let today = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        for (team, time) in [(&a, "18:00"), (&b, "18:30")] {
            core.save_session(
                team.clone(),
                None,
                SessionDraft {
                    start: Some(format!("{today}T{time}")),
                    duration: "90".into(),
                    venue: Some(gym.clone()),
                    ..core.new_session_draft()
                },
            )
            .unwrap();
        }
        let mut tab = CalendarTab::at(core.clone(), a.clone(), today);
        let events = |tab: &CalendarTab| -> Vec<(String, bool)> {
            tab.days()
                .iter()
                .flat_map(|d| d.events.iter())
                .map(|e| (e.team_id.clone(), e.shared_venue))
                .collect()
        };
        assert_eq!(events(&tab), vec![(a.clone(), true)]);
        tab.key(key!(t));
        assert_eq!(events(&tab), vec![(a.clone(), true), (b.clone(), true)]);
        // the event of the other team is not opened from here
        tab.key(key!(down));
        let _ = tab.key(key!(down));
        assert_eq!(tab.key(key!(enter)), Leave::Stay);
        tab.key(key!(v));
        assert_eq!(events(&tab).len(), 2);
        tab.key(key!(v));
        assert_eq!(events(&tab).len(), 2);
        tab.key(key!(v));
        tab.key(key!(t));
        assert_eq!(events(&tab), vec![(a, true)]);
    }
}
