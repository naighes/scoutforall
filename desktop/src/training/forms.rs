//! The forms of the training: a session (or a weekly series of them), an
//! activity, and the search of an
//! activity written before, to copy. The fields of `form`, saved through
//! `mobile`, which checks them; an error takes the keys to its field.

use scout4all_mobile::clock::ClockExt;
use {
    super::scope::{Answer, SCOPES, ScopeQuestion},
    crate::{
        calendar::text_of,
        form::{self, Field, Form, Mask, Outcome, Suggestion, Value},
        strings::Strings,
        theme::{self, Colors},
        ui::{self, fill},
    },
    chrono::{Datelike, NaiveDate, NaiveTime},
    iced::{
        Alignment, Element, Length,
        widget::{Column, column, container, scrollable, text},
    },
    scout4all_core::training::{ActivityStatus, SessionStatus, cycles::Level, series::SeriesScope},
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{
        MobileError, Scout4All,
        training::{
            ActivityDraft, ActivityHitRecord, CycleRecord, SeriesBand, SeriesDraft, SessionDraft,
            TrainingField,
        },
        venues::VenueClashRecord,
    },
};

const EMPTY: &str = "–";

use crate::venue_choice::{NEW_KEY, NewVenue, Step, new_hint, venue_field};
pub(super) use crate::venue_choice::{venue_at, venue_options, venue_value};

/// What a key or a click did to a form.
pub enum Edited {
    Stay,
    Cancel,
    /// saved: the id of the session or of the activity
    Saved(String),
    /// the activity found, to copy
    Copy(ActivityHitRecord),
    /// the sessions of a series made: the calendar shows them
    Repeated,
}

const KIND: usize = 0;
const DATE: usize = 1;
const TIME: usize = 2;
const DURATION: usize = 3;
const LOCATION: usize = 4;
const UNTIL: usize = 5;
const TITLE: usize = 6;
const OBJECTIVES: usize = 7;
const NOTES: usize = 8;
const REMARKS: usize = 9;
const ACTUAL: usize = 10;
const MICROCYCLE: usize = 11;

/// The microcycles of the team that hold the day written, and the one
/// `current` even when the day is out of it: id and name.
pub(super) fn microcycle_options(
    s: &Strings,
    core: &Scout4All,
    team_id: &str,
    date: &str,
    current: Option<&str>,
) -> Vec<(String, String)> {
    let containing = day_of(date)
        .and_then(|day| {
            core.cycles_containing(team_id.to_string(), Level::Microcycle, text_of(day))
                .ok()
        })
        .unwrap_or_default();
    let kept = current.and_then(|id| core.cycle(Level::Microcycle, id.to_string()).ok());
    let mut options: Vec<(String, String)> = containing
        .iter()
        .map(|c| (c.id.clone(), cycle_label(s, c)))
        .collect();
    if let Some(kept) = kept.filter(|k| !options.iter().any(|(id, _)| *id == k.id)) {
        options.push((kept.id.clone(), cycle_label(s, &kept)));
    }
    options
}

/// What a save says when other teams hold the gym then (nothing when none
/// does): their names and times, and that Enter again saves anyway.
pub(crate) fn clash_warning(
    hint: &str,
    clashes: Result<Vec<VenueClashRecord>, MobileError>,
) -> Option<String> {
    clashes.ok().filter(|found| !found.is_empty()).map(|found| {
        found
            .iter()
            .map(|c| c.message.clone())
            .chain([hint.to_string()])
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// The choice of a microcycle: the empty one, then `options`; `chosen` is
/// the position among them.
pub(super) fn microcycle_value(options: &[(String, String)], chosen: Option<usize>) -> Value {
    Value::Choice {
        options: std::iter::once(EMPTY.to_string())
            .chain(options.iter().map(|(_, name)| name.clone()))
            .collect(),
        index: chosen.map_or(0, |at| at + 1),
    }
}

/// The text of a draft's start: its day, its time.
pub(super) fn split(start: &Option<String>) -> (String, String) {
    match start {
        Some(start) => (
            start.get(..10).unwrap_or_default().to_string(),
            start.get(11..16).unwrap_or_default().to_string(),
        ),
        None => (String::new(), String::new()),
    }
}

pub(super) fn day_of(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y-%m-%d").ok()
}

pub(super) fn cycle_label(s: &Strings, cycle: &CycleRecord) -> String {
    let window = s.window(&cycle.first_day, &cycle.last_day);
    match cycle.title.is_empty() {
        true => window,
        false => format!("{} ({window})", cycle.title),
    }
}

/// The form of a session, new or saved. A new one may be a weekly series
/// instead: an entry after the other (Continue), all made at the end
/// (Finish), each entry a series of its own. A saved one in a series asks
/// where its time, duration and gym go.
pub struct SessionEditor {
    pub form: Form,
    team_id: String,
    id: Option<String>,
    status: SessionStatus,
    venues: Vec<(String, String)>,
    /// the microcycles of the choice after the empty one: id and name
    microcycles: Vec<(String, String)>,
    /// another team holds the gym then and the user was told: Enter again
    /// saves
    warned: bool,
    /// the entries of the series written before the one in the form
    entries: Vec<SeriesDraft>,
    /// the sessions of the series that overlap, written, when the dialog
    /// that offers to make the others is open
    pub conflicts: Vec<String>,
    /// the question of the series of a saved session, when asked
    pub asking: Option<ScopeQuestion>,
    /// a new gym being written, over the form
    new_venue: Option<NewVenue>,
    /// the words of the forms (the choices are written again on a change)
    s: &'static Strings,
}

/// The choice of a question over the form (the scope of a series), not of a
/// field: `form::Message::Choose(PICK, i)`.
pub const PICK: usize = usize::MAX;

impl SessionEditor {
    /// A new session of the team (on `day` if given) or the saved one.
    pub fn new(
        core: &Scout4All,
        s: &'static Strings,
        team_id: String,
        id: Option<String>,
        day: Option<NaiveDate>,
    ) -> Result<SessionEditor, MobileError> {
        SessionEditor::at(core, s, team_id, id, day.map(|d| (d, None)))
    }

    /// The same, the new one at a day and maybe a time (a click on the
    /// calendar).
    pub fn at(
        core: &Scout4All,
        s: &'static Strings,
        team_id: String,
        id: Option<String>,
        when: Option<(NaiveDate, Option<NaiveTime>)>,
    ) -> Result<SessionEditor, MobileError> {
        let draft = match &id {
            Some(id) => core.session_draft(id.clone())?,
            None => core.new_session_draft(),
        };
        let venues = venue_options(s, core, &team_id, draft.venue.as_deref());
        let (date, time) = split(&draft.start);
        let (date, time) = match (&id, when) {
            (None, Some((day, at))) => (
                text_of(day),
                at.map_or(time, |t| t.format("%H:%M").to_string()),
            ),
            _ => (date, time),
        };
        let fields = vec![
            Field::segments(
                s.create_what,
                vec![s.single_session.to_string(), s.weekly_series.to_string()],
                0,
            ),
            Field::masked(s.date, &date, Mask::Date, s.date_placeholder)
                .with_calendar(core.clock().today()),
            Field::masked(s.time, &time, Mask::Time, s.time_placeholder),
            Field::text(s.session_duration, draft.duration.clone()),
            venue_field(s, &venues, draft.venue.as_deref()),
            Field::masked(s.until_day, "", Mask::Date, s.date_placeholder)
                .with_calendar(core.clock().today()),
            Field::text(s.session_title, draft.title.clone()),
            Field::lines(s.objectives, draft.objectives.clone()),
            Field::lines(s.notes, draft.notes.clone()),
            Field::lines(s.final_remarks, draft.final_remarks.clone()),
            Field::text(s.session_actual, draft.actual_duration.clone()),
            Field::choice(s.microcycle, vec![EMPTY.to_string()], 0),
        ];
        let mut editor = SessionEditor {
            form: Form::new(
                match id {
                    Some(_) => s.edit_session,
                    None => s.new_session,
                },
                fields,
            ),
            team_id,
            id,
            status: draft.status,
            venues,
            microcycles: vec![],
            warned: false,
            entries: vec![],
            conflicts: vec![],
            asking: None,
            new_venue: None,
            s,
        };
        editor.form.focused = DATE;
        editor.form.busy = busy_days(core, &editor.team_id);
        editor.offer_microcycles(core, draft.microcycle.as_deref());
        editor.shape();
        Ok(editor)
    }

    /// The field the keys are on; none over a question or a dialog.
    /// What became of the new gym: made, it's the one chosen.
    fn venue_step(&mut self, step: Step) -> Edited {
        match step {
            Step::Stay => {}
            Step::Close => self.new_venue = None,
            Step::Made(id, name) => {
                self.new_venue = None;
                self.warned = false;
                self.venues.push((id.clone(), name));
                if let Some(field) = self.form.fields.get_mut(LOCATION) {
                    field.value = venue_value(&self.venues, Some(&id));
                }
            }
        }
        Edited::Stay
    }

    pub fn focus_id(&self) -> Option<iced::widget::Id> {
        if let Some(venue) = &self.new_venue {
            return Some(venue.focus_id());
        }
        (self.asking.is_none() && self.conflicts.is_empty()).then(|| self.form.focus_id())
    }

    /// A weekly series is being written.
    pub fn is_series(&self) -> bool {
        self.id.is_none() && self.form.index(KIND) == 1
    }

    /// The fields, the title and the buttons of the kind chosen: a series
    /// has no title, objectives, notes or microcycle (its sessions are
    /// born bare), but its last day; a saved session can't become one.
    fn shape(&mut self) {
        let series = self.is_series();
        self.form.hidden = match (self.id.is_some(), series) {
            (true, _) => vec![KIND, UNTIL],
            (false, false) => vec![UNTIL],
            (false, true) => vec![TITLE, OBJECTIVES, NOTES, REMARKS, ACTUAL, MICROCYCLE],
        };
        let s = self.s;
        self.form.title = match (self.id.is_some(), series) {
            (true, _) => s.edit_session.to_string(),
            (false, false) => s.new_session.to_string(),
            (false, true) => format!(
                "{} · {}",
                s.new_series,
                fill(s.series_entry, &[(self.entries.len() + 1).to_string()])
            ),
        };
        match series {
            true => {
                self.form.another = Some((theme::icons::ADD, s.continue_entry.to_string()));
                self.form.actions = Some((
                    (theme::icons::CLOSE, s.cancel.to_string()),
                    (theme::icons::CHECK, s.finish.to_string()),
                ));
            }
            false => {
                self.form.another = None;
                self.form.actions = None;
            }
        }
    }

    /// The microcycles that hold the day written, and the one the session is
    /// in (`current`) even when the day moved out of it: saved, it says so;
    /// the empty choice takes it out. With none chosen, the one that holds
    /// the day is proposed.
    fn offer_microcycles(&mut self, core: &Scout4All, current: Option<&str>) {
        let options =
            microcycle_options(self.s, core, &self.team_id, &self.form.text(DATE), current);
        let chosen = match current {
            Some(id) => options.iter().position(|(o, _)| o == id),
            None => (self.id.is_none() && !options.is_empty()).then_some(0),
        };
        self.form.fields = std::mem::take(&mut self.form.fields)
            .into_iter()
            .enumerate()
            .map(|(i, field)| match i == MICROCYCLE {
                true => Field {
                    value: microcycle_value(&options, chosen),
                    ..field
                },
                false => field,
            })
            .collect();
        self.microcycles = options;
    }

    /// The microcycle chosen now.
    fn microcycle(&self) -> Option<String> {
        self.form
            .index(MICROCYCLE)
            .checked_sub(1)
            .and_then(|at| self.microcycles.get(at))
            .map(|(id, _)| id.clone())
    }

    fn outcome(&mut self, core: &Scout4All, outcome: Outcome) -> Edited {
        match outcome {
            Outcome::Stay => Edited::Stay,
            Outcome::Cancel => Edited::Cancel,
            Outcome::Changed(KIND) => {
                self.form.clear();
                self.shape();
                Edited::Stay
            }
            Outcome::Changed(DATE) => {
                // the day moved: the microcycle that holds it is offered
                self.warned = false;
                let current = self.microcycle();
                self.offer_microcycles(core, current.as_deref());
                Edited::Stay
            }
            Outcome::Changed(_) => {
                self.warned = false;
                Edited::Stay
            }
            Outcome::Save if self.is_series() => self.finish(core),
            Outcome::Save => self.save(core),
        }
    }

    fn draft(&self) -> SessionDraft {
        let (date, time) = (self.form.text(DATE), self.form.text(TIME));
        SessionDraft {
            start: (day_of(&date).is_some() && time.trim().len() == 5)
                .then(|| format!("{}T{}", date.trim(), time.trim())),
            duration: self.form.text(DURATION),
            actual_duration: self.form.text(ACTUAL),
            venue: venue_at(&self.venues, self.form.index(LOCATION)),
            title: self.form.text(TITLE),
            objectives: self.form.text(OBJECTIVES),
            notes: self.form.text(NOTES),
            final_remarks: self.form.text(REMARKS),
            microcycle: self.microcycle(),
            status: self.status,
        }
    }

    /// The warning of the gym held by another team then, once: the next save
    /// goes through.
    fn warning(&mut self, core: &Scout4All) -> Option<String> {
        let draft = self.draft();
        let (venue, start) = (draft.venue?, draft.start?);
        let minutes = draft.duration.trim().parse().ok()?;
        match self.warned {
            true => None,
            false => {
                let warning = clash_warning(
                    self.s.save_anyway_hint,
                    core.session_venue_clashes(self.team_id.clone(), venue, start, minutes),
                );
                self.warned = warning.is_some();
                warning
            }
        }
    }

    fn save(&mut self, core: &Scout4All) -> Edited {
        if let Some(warning) = self.warning(core) {
            self.form.warn(warning);
            return Edited::Stay;
        }
        // a session of a series whose time, duration or gym changed: where
        // does it go?
        let asks = self.id.as_ref().is_some_and(|id| {
            core.asks_series_scope(id.clone(), Some(self.draft()))
                .unwrap_or(false)
        });
        match asks {
            true => {
                self.asking = Some(ScopeQuestion::default());
                Edited::Stay
            }
            false => self.write(core, SeriesScope::This),
        }
    }

    /// The session written; its time, duration and gym also to the others
    /// of its series the scope reaches.
    fn write(&mut self, core: &Scout4All, scope: SeriesScope) -> Edited {
        self.asking = None;
        let saved = match (&self.id, scope) {
            (Some(id), SeriesScope::ThisAndFollowing | SeriesScope::All) => {
                core.save_session_in_series(id.clone(), self.draft(), scope)
            }
            _ => core.save_session(self.team_id.clone(), self.id.clone(), self.draft()),
        };
        match saved {
            Ok(saved) => Edited::Saved(saved.id),
            Err(e) => {
                self.refused(&e);
                Edited::Stay
            }
        }
    }

    /// The session not written: why under the wrong field, or in a dialog.
    fn refused(&mut self, e: &MobileError) {
        match e {
            MobileError::InvalidTraining { field, .. } => self.form.reject(
                match field {
                    TrainingField::Duration => DURATION,
                    TrainingField::ActualDuration => ACTUAL,
                    _ => DATE,
                },
                e.to_string(),
            ),
            _ => self.form.fail(e.to_string()),
        }
    }

    /// The entry in the form, if it's whole: the field that's wrong if not.
    fn entry(&self) -> Result<SeriesDraft, usize> {
        let (date, time) = (self.form.text(DATE), self.form.text(TIME));
        let first = day_of(&date).ok_or(DATE)?;
        let at = NaiveTime::parse_from_str(time.trim(), "%H:%M").map_err(|_| TIME)?;
        let minutes: u16 = self
            .form
            .text(DURATION)
            .trim()
            .parse()
            .ok()
            .filter(|m| *m > 0)
            .ok_or(DURATION)?;
        let last = day_of(&self.form.text(UNTIL))
            .filter(|last| *last >= first)
            .ok_or(UNTIL)?;
        Ok(SeriesDraft {
            from: text_of(first),
            to: text_of(last),
            bands: vec![SeriesBand {
                weekday: u8::try_from(first.weekday().number_from_monday()).unwrap_or(1),
                time: at.format("%H:%M").to_string(),
                duration_min: minutes,
                venue: venue_at(&self.venues, self.form.index(LOCATION)),
            }],
        })
    }

    /// The entry in the form, or the error on its wrong field.
    fn checked_entry(&mut self) -> Option<SeriesDraft> {
        match self.entry() {
            Ok(entry) => Some(entry),
            Err(field) => {
                self.form.reject(field, self.s.entry_invalid);
                None
            }
        }
    }

    /// Continue: the entry kept, the form ready for the next one (the same
    /// time, minutes, gym and last day; the day after).
    fn next_entry(&mut self) -> Edited {
        if let Some(entry) = self.checked_entry() {
            let next = day_of(&entry.from)
                .and_then(|d| d.succ_opt())
                .map(text_of)
                .unwrap_or_default();
            self.entries.push(entry);
            self.form.update(form::Message::Input(DATE, next));
            self.form.focused = DATE;
            self.form.clear();
            self.warned = false;
            self.shape();
        }
        Edited::Stay
    }

    /// All the entries, the one in the form last.
    fn all_entries(&mut self) -> Option<Vec<SeriesDraft>> {
        let last = self.checked_entry()?;
        Some(self.entries.iter().cloned().chain([last]).collect())
    }

    /// Finish: the gyms held by other teams told once, then the series
    /// made.
    fn finish(&mut self, core: &Scout4All) -> Edited {
        let Some(entries) = self.all_entries() else {
            return Edited::Stay;
        };
        let gyms = entries
            .iter()
            .any(|e| e.bands.iter().any(|b| b.venue.is_some()));
        if gyms && !self.warned {
            let warning = clash_warning(
                self.s.save_anyway_hint,
                core.series_venue_clashes(self.team_id.clone(), entries.clone()),
            );
            self.warned = warning.is_some();
            if warning.is_some() {
                self.form.notice = warning.map(crate::notice::Notice::warning);
                return Edited::Stay;
            }
        }
        self.make(core, entries, false)
    }

    /// The series made; with a conflict and not `create_others`, the
    /// dialog opens and nothing is written.
    fn make(&mut self, core: &Scout4All, entries: Vec<SeriesDraft>, create_others: bool) -> Edited {
        match core.create_series(self.team_id.clone(), entries, create_others) {
            Ok(result) => match (result.created.is_empty(), result.conflicts.is_empty()) {
                (false, _) => Edited::Repeated,
                (true, false) => {
                    self.conflicts = result
                        .conflicts
                        .iter()
                        .map(|c| {
                            format!(
                                "{} {}",
                                c.start.get(..10).unwrap_or_default(),
                                c.start.get(11..16).unwrap_or_default()
                            )
                        })
                        .collect();
                    Edited::Stay
                }
                (true, true) => {
                    self.form.fail(self.s.no_sessions.to_string());
                    Edited::Stay
                }
            },
            Err(e) => {
                self.conflicts.clear();
                self.form.fail(e.to_string());
                Edited::Stay
            }
        }
    }

    /// The answer of the dialog of the conflicts: make the others, or back
    /// to the form.
    fn answer_conflicts(&mut self, core: &Scout4All, others: bool) -> Edited {
        match (others, self.all_entries()) {
            (true, Some(entries)) => self.make(core, entries, true),
            _ => {
                self.conflicts.clear();
                Edited::Stay
            }
        }
    }

    pub fn update(&mut self, core: &Scout4All, message: form::Message) -> Edited {
        if let Some(venue) = &mut self.new_venue {
            let step = venue.update(core, message);
            return self.venue_step(step);
        }
        match (self.asking.is_some(), self.conflicts.is_empty(), message) {
            (true, _, form::Message::Choose(PICK, i)) => match SCOPES.get(i) {
                Some(scope) => self.write(core, *scope),
                None => Edited::Stay,
            },
            (true, _, form::Message::Cancel) => {
                self.asking = None;
                Edited::Stay
            }
            (true, _, _) => Edited::Stay,
            (_, false, form::Message::Save) => self.answer_conflicts(core, true),
            (_, false, form::Message::Cancel) => self.answer_conflicts(core, false),
            (_, false, _) => Edited::Stay,
            (_, true, form::Message::Another) if self.is_series() => self.next_entry(),
            (_, true, form::Message::Details(LOCATION)) => {
                self.new_venue = Some(NewVenue::new(self.s));
                Edited::Stay
            }
            (_, true, message) => {
                let outcome = self.form.update(message);
                self.outcome(core, outcome)
            }
        }
    }

    pub fn key(&mut self, core: &Scout4All, ck: KeyCombination) -> Edited {
        if let Some(venue) = &mut self.new_venue {
            let step = venue.key(core, ck);
            return self.venue_step(step);
        }
        if self.asking.is_none()
            && self.conflicts.is_empty()
            && self.form.focused == LOCATION
            && ck == NEW_KEY
        {
            self.new_venue = Some(NewVenue::new(self.s));
            return Edited::Stay;
        }
        let continues = ck == CONTINUE_KEY && self.is_series();
        match (&mut self.asking, self.conflicts.is_empty()) {
            (Some(question), _) => match question.key(ck) {
                Answer::Stay => Edited::Stay,
                Answer::Cancel => {
                    self.asking = None;
                    Edited::Stay
                }
                Answer::Chosen(scope) => self.write(core, scope),
            },
            (None, false) => match ck {
                k if k == key!(enter) => self.answer_conflicts(core, true),
                k if k == BACK_KEY => self.answer_conflicts(core, false),
                _ => Edited::Stay,
            },
            (None, true) if continues => self.next_entry(),
            (None, true) => {
                let outcome = self.form.key(ck);
                self.outcome(core, outcome)
            }
        }
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        if self.new_venue.is_some() {
            return Form::hints(s);
        }
        let new_venue = (self.form.focused == LOCATION).then(|| new_hint(s));
        match (&self.asking, self.conflicts.is_empty(), self.is_series()) {
            (Some(_), _, _) => ScopeQuestion::hints(s, s.save),
            (None, false, _) => vec![
                ("Enter".to_string(), s.create_others.to_string()),
                ("Esc".to_string(), s.cancel.to_string()),
            ],
            (None, true, true) => self
                .form
                .on_field(s, Form::hints(s))
                .into_iter()
                .map(|(k, what)| match k.as_str() {
                    "Enter" | "⌘Enter" | "⌃Enter" => (k, s.finish.to_string()),
                    _ => (k, what),
                })
                .chain([(CONTINUE_HINT.to_string(), s.continue_entry.to_string())])
                .chain(new_venue)
                .collect(),
            (None, true, false) => self
                .form
                .on_field(s, Form::hints(s))
                .into_iter()
                .chain(new_venue)
                .collect(),
        }
    }

    /// The form, the question over it or the dialog of the conflicts; with
    /// the entries written before beside a series.
    pub fn view<'a, M: Clone + 'a>(
        &'a self,
        s: &'static Strings,
        c: Colors,
        on: impl Fn(form::Message) -> M + Copy + 'a,
    ) -> Element<'a, M> {
        if let Some(venue) = &self.new_venue {
            return venue.view(s, c, on);
        }
        match (&self.asking, self.conflicts.is_empty()) {
            (Some(question), _) => question.view(
                s,
                c,
                s.scope_edit_text,
                (theme::icons::CHECK, s.save),
                move |scope| {
                    on(form::Message::Choose(
                        PICK,
                        SCOPES.iter().position(|o| *o == scope).unwrap_or(0),
                    ))
                },
                on(form::Message::Cancel),
            ),
            (None, false) => conflicts_dialog(
                s,
                c,
                &self.conflicts,
                on(form::Message::Save),
                on(form::Message::Cancel),
            ),
            (None, true) => {
                let aside = (!self.entries.is_empty()).then(|| self.entries_panel(s, c));
                let width = match aside.is_some() {
                    true => 1080,
                    false => 720,
                };
                container(container(self.form.view(s, c, on, aside)).max_width(width))
                    .center_x(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
        }
    }

    /// The entries written before: the weekday and its first day, the time,
    /// the minutes and the last day.
    fn entries_panel<'a, M: 'a>(&self, s: &'static Strings, c: Colors) -> Element<'a, M> {
        let rows: Vec<Element<'a, M>> = self
            .entries
            .iter()
            .flat_map(|e| {
                e.bands.iter().map(move |b| {
                    let weekday = s
                        .weekdays
                        .get(usize::from(b.weekday.saturating_sub(1)))
                        .copied()
                        .unwrap_or_default();
                    ui::body(format!(
                        "{weekday} {} · {}′ · {}",
                        b.time,
                        b.duration_min,
                        s.window(&e.from, &e.to)
                    ))
                    .into()
                })
            })
            .collect();
        ui::panel(
            Some(s.series_entries),
            Column::with_children(rows).spacing(6),
            c,
        )
    }
}

/// The key that keeps the entry of a series and goes on to the next.
const CONTINUE_KEY: KeyCombination = key!(alt - enter);
const CONTINUE_HINT: &str = match cfg!(target_os = "macos") {
    true => "⌥Enter",
    false => "Alt+Enter",
};

/// The dialog of the sessions of a series that overlap others, `confirm`
/// making the others.
fn conflicts_dialog<'a, M: Clone + 'a>(
    s: &'static Strings,
    c: Colors,
    conflicts: &[String],
    confirm: M,
    dismiss: M,
) -> Element<'a, M> {
    let rows: Vec<Element<'a, M>> = conflicts
        .iter()
        .map(|day| ui::body(day.clone()).into())
        .collect();
    let question = ui::panel(
        None,
        column![
            text(s.new_series)
                .size(22)
                .font(theme::fonts().bold)
                .color(c.text),
            ui::body(s.series_conflicts),
            scrollable(Column::with_children(rows).spacing(3)).height(Length::Shrink),
            ui::answers(
                Some((theme::icons::CLOSE, s.cancel, dismiss)),
                theme::icons::CHECK,
                s.create_others,
                Some(confirm),
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

const NAME: usize = 0;
const DESCRIPTION: usize = 1;
const PLANNED: usize = 2;
const ACTIVITY_ACTUAL: usize = 3;
const ACTIVITY_NOTES: usize = 4;
const STATUS: usize = 5;

const STATUSES: [ActivityStatus; 3] = [
    ActivityStatus::Planned,
    ActivityStatus::Completed,
    ActivityStatus::Skipped,
];

/// The form of an activity of a session, new or saved.
pub struct ActivityEditor {
    pub form: Form,
    session_id: String,
    id: Option<String>,
}

impl ActivityEditor {
    /// A new activity (empty, or `copy` of one found) or the saved one.
    pub fn new(
        core: &Scout4All,
        s: &Strings,
        session_id: String,
        id: Option<String>,
        copy: Option<ActivityHitRecord>,
    ) -> Result<ActivityEditor, MobileError> {
        let draft: ActivityDraft = match (&id, copy) {
            (Some(id), _) => core.activity_draft(session_id.clone(), id.clone())?,
            (None, Some(hit)) => core.activity_draft_copying(hit)?,
            (None, None) => core.new_activity_draft(),
        };
        let names = [s.status_planned, s.status_completed, s.status_skipped];
        let fields = vec![
            Field::text(s.name, draft.name),
            Field::lines(s.activity_description, draft.description),
            Field::text(s.activity_planned, draft.planned),
            Field::text(s.activity_actual, draft.actual),
            Field::lines(s.notes, draft.notes),
            Field::choice(
                s.status_planned,
                names.iter().map(|n| n.to_string()).collect(),
                STATUSES
                    .iter()
                    .position(|x| *x == draft.status)
                    .unwrap_or(0),
            ),
        ];
        Ok(ActivityEditor {
            form: Form::new(
                match id {
                    Some(_) => s.edit_activity,
                    None => s.new_activity,
                },
                fields,
            ),
            session_id,
            id,
        })
    }

    fn save(&mut self, core: &Scout4All) -> Edited {
        let draft = ActivityDraft {
            name: self.form.text(NAME),
            description: self.form.text(DESCRIPTION),
            planned: self.form.text(PLANNED),
            actual: self.form.text(ACTIVITY_ACTUAL),
            notes: self.form.text(ACTIVITY_NOTES),
            status: STATUSES
                .get(self.form.index(STATUS))
                .copied()
                .unwrap_or(ActivityStatus::Planned),
        };
        match core.save_activity(self.session_id.clone(), self.id.clone(), draft) {
            Ok(saved) => Edited::Saved(saved.id),
            Err(e) => {
                match &e {
                    MobileError::InvalidTraining { field, .. } => self.form.reject(
                        match field {
                            TrainingField::Planned => PLANNED,
                            TrainingField::Actual => ACTIVITY_ACTUAL,
                            _ => NAME,
                        },
                        e.to_string(),
                    ),
                    _ => self.form.fail(e.to_string()),
                }
                Edited::Stay
            }
        }
    }

    fn outcome(&mut self, core: &Scout4All, outcome: Outcome) -> Edited {
        match outcome {
            Outcome::Cancel => Edited::Cancel,
            Outcome::Save => self.save(core),
            Outcome::Stay | Outcome::Changed(_) => Edited::Stay,
        }
    }

    pub fn update(&mut self, core: &Scout4All, message: form::Message) -> Edited {
        let outcome = self.form.update(message);
        self.outcome(core, outcome)
    }

    pub fn key(&mut self, core: &Scout4All, ck: KeyCombination) -> Edited {
        let outcome = self.form.key(ck);
        self.outcome(core, outcome)
    }
}

const QUERY: usize = 0;
const ONLY_TEAM: usize = 1;

/// The search of the activities written before: the name typed, the ones
/// found offered; Enter on one takes it as a new activity.
pub struct CopyEditor {
    pub form: Form,
    team_id: String,
    hits: Vec<ActivityHitRecord>,
    /// the hit taken
    taken: Option<usize>,
}

impl CopyEditor {
    pub fn new(core: &Scout4All, s: &Strings, team_id: String) -> CopyEditor {
        let mut form = Form::new(
            s.copy_activity,
            vec![
                Field::search(s.name, ""),
                Field::check(s.only_this_team, core.activity_search_team_only(), false),
            ],
        );
        form.sections = vec![];
        let mut editor = CopyEditor {
            form,
            team_id,
            hits: vec![],
            taken: None,
        };
        editor.search(core);
        editor
    }

    fn search(&mut self, core: &Scout4All) {
        self.hits = core
            .search_activities(self.team_id.clone(), self.form.text(QUERY))
            .unwrap_or_default();
        let offered = self
            .hits
            .iter()
            .map(|h| Suggestion {
                label: format!(
                    "{} · {} {}",
                    h.name,
                    h.team_name,
                    h.session_start.get(..10).unwrap_or_default()
                ),
                value: h.name.clone(),
                picture: None,
            })
            .collect();
        self.form.suggest(QUERY, offered);
    }

    fn outcome(&mut self, core: &Scout4All, s: &Strings, outcome: Outcome) -> Edited {
        match outcome {
            Outcome::Stay => Edited::Stay,
            Outcome::Cancel => Edited::Cancel,
            Outcome::Changed(QUERY) => {
                self.taken = None;
                self.search(core);
                Edited::Stay
            }
            Outcome::Changed(ONLY_TEAM) => {
                let _ = core.set_activity_search_team_only(self.only_team());
                self.search(core);
                Edited::Stay
            }
            Outcome::Changed(_) => Edited::Stay,
            Outcome::Save => match self.taken.or((!self.hits.is_empty()).then_some(0)) {
                Some(at) => match self.hits.get(at) {
                    Some(hit) => Edited::Copy(hit.clone()),
                    None => Edited::Stay,
                },
                None => {
                    self.form.fail(s.no_activities.to_string());
                    Edited::Stay
                }
            },
        }
    }

    fn only_team(&self) -> bool {
        matches!(
            self.form.fields.get(ONLY_TEAM).map(|f| &f.value),
            Some(Value::Check { on: true, .. })
        )
    }

    pub fn update(&mut self, core: &Scout4All, s: &Strings, message: form::Message) -> Edited {
        if let form::Message::Take(QUERY, n) = message {
            self.taken = Some(n);
        }
        let outcome = self.form.update(message);
        self.outcome(core, s, outcome)
    }

    pub fn key(&mut self, core: &Scout4All, s: &Strings, ck: KeyCombination) -> Edited {
        let entered = match (self.form.offering(), self.form.lit()) {
            (true, Some(lit))
                if self.form.focused == QUERY && (ck == key!(enter) || ck == key!(tab)) =>
            {
                Some(lit)
            }
            _ => None,
        };
        if let Some(lit) = entered {
            self.taken = Some(lit);
            // Enter copies the one offered; Tab only takes its name
            return match ck == key!(enter) {
                true => self
                    .hits
                    .get(lit)
                    .map_or(Edited::Stay, |hit| Edited::Copy(hit.clone())),
                false => {
                    let outcome = self.form.key(ck);
                    self.outcome(core, s, outcome)
                }
            };
        }
        let outcome = self.form.key(ck);
        self.outcome(core, s, outcome)
    }
}

/// The days of the team that already hold a session or a match, to mark in
/// the calendars of the forms.
pub(super) fn busy_days(
    core: &Scout4All,
    team_id: &str,
) -> std::collections::HashSet<chrono::NaiveDate> {
    core.training_agenda_between(Some(team_id.to_string()), None, None)
        .unwrap_or_default()
        .iter()
        .filter_map(|d| crate::calendar::day_of(&d.date))
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::unreachable)]
mod tests {
    use {
        super::*,
        crate::form::Message,
        scout4all_core::shapes::{
            country::CountryCode,
            enums::{GenderEnum, TeamClassificationEnum},
        },
        scout4all_mobile::venues::VenueDraft,
        std::sync::Arc,
    };

    fn two_teams() -> (tempfile::TempDir, Arc<Scout4All>, String, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let core =
            Scout4All::for_tests(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
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
        let venue = core
            .save_venue(
                None,
                VenueDraft {
                    name: "Terontola".into(),
                    ..VenueDraft::default()
                },
            )
            .unwrap()
            .id;
        (dir, core, a, b, venue)
    }

    /// `n` on the gym, or its "+": a small form with the name, the gym made
    /// and chosen; an empty name isn't taken, Esc gives up.
    #[test]
    fn a_gym_is_made_from_the_session_form() {
        let s = crate::strings::strings(scout4all_core::shapes::enums::LanguageEnum::It);
        let (_dir, core, a, ..) = two_teams();
        let mut editor = SessionEditor::new(&core, s, a, None, None).unwrap();
        editor.update(&core, Message::Details(LOCATION));
        editor.key(&core, key!(esc));
        assert!(editor.new_venue.is_none());
        editor.form.focused = LOCATION;
        editor.key(&core, key!(n));
        editor.key(&core, key!(enter));
        assert!(editor.new_venue.is_some());
        assert_eq!(core.venues().unwrap().len(), 1);
        editor.update(&core, Message::Input(0, "Palazzetto".into()));
        editor.key(&core, key!(enter));
        assert!(editor.new_venue.is_none());
        let made = core
            .venues()
            .unwrap()
            .into_iter()
            .find(|v| v.name == "Palazzetto")
            .map(|v| v.id);
        assert_eq!(editor.draft().venue, made);
        assert!(made.is_some());
    }

    /// Another team holds the gym then: the first save tells which one and
    /// writes nothing, the second saves; changing a field asks again.
    #[test]
    fn a_session_in_a_gym_held_by_another_team_is_told_first() {
        let s = crate::strings::strings(scout4all_core::shapes::enums::LanguageEnum::It);
        let (_dir, core, a, b, venue) = two_teams();
        let held = SessionDraft {
            start: Some("2026-03-10T18:00".into()),
            duration: "90".into(),
            venue: Some(venue),
            ..core.new_session_draft()
        };
        core.save_session(a.clone(), None, held).unwrap();
        let mut editor = SessionEditor::new(&core, s, b.clone(), None, None).unwrap();
        for (field, text) in [(DATE, "2026-03-10"), (TIME, "18:30"), (DURATION, "60")] {
            editor.update(&core, Message::Input(field, text.into()));
        }
        editor.update(&core, Message::Choose(LOCATION, 1));
        assert!(matches!(editor.update(&core, Message::Save), Edited::Stay));
        let told = editor.form.told().unwrap();
        assert!(told.contains("Terontola") && told.contains(s.save_anyway_hint));
        assert!(core.training_sessions(b.clone()).unwrap().is_empty());
        // another time: asked again
        editor.update(&core, Message::Input(TIME, "18:45".into()));
        assert!(matches!(editor.update(&core, Message::Save), Edited::Stay));
        assert!(core.training_sessions(b.clone()).unwrap().is_empty());
        // Enter again saves
        assert!(matches!(
            editor.update(&core, Message::Save),
            Edited::Saved(_)
        ));
        assert_eq!(core.training_sessions(b).unwrap().len(), 1);
    }

    /// A free gym, or none chosen, saves at once.
    #[test]
    fn a_free_gym_saves_without_asking() {
        let s = crate::strings::strings(scout4all_core::shapes::enums::LanguageEnum::It);
        let (_dir, core, _a, b, _venue) = two_teams();
        let mut editor = SessionEditor::new(&core, s, b.clone(), None, None).unwrap();
        for (field, text) in [(DATE, "2026-03-10"), (TIME, "18:30"), (DURATION, "60")] {
            editor.update(&core, Message::Input(field, text.into()));
        }
        editor.update(&core, Message::Choose(LOCATION, 1));
        assert!(matches!(
            editor.update(&core, Message::Save),
            Edited::Saved(_)
        ));
    }

    /// A wrong field of the session is told under it, not in a dialog.
    #[test]
    fn a_wrong_duration_is_told_under_its_field() {
        let dir = tempfile::tempdir().unwrap();
        let core =
            Scout4All::for_tests(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = core.tutorial_team().unwrap();
        let s = crate::strings::strings(scout4all_core::shapes::enums::LanguageEnum::It);
        let mut editor = SessionEditor::new(&core, s, team, None, None).unwrap();
        editor.update(&core, Message::Input(DATE, "2026-03-02".into()));
        editor.update(&core, Message::Input(TIME, "18:00".into()));
        editor.update(&core, Message::Input(DURATION, "0".into()));
        editor.key(&core, scout4all_keys::key!(enter));
        assert_eq!(
            editor.form.invalid.as_ref().map(|(at, _)| *at),
            Some(DURATION)
        );
    }
}
