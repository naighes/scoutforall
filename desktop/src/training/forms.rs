//! The forms of the training: a session, an activity, and the search of an
//! activity written before, to copy. The fields of `form`, saved through
//! `mobile`, which checks them; an error takes the keys to its field.

use {
    crate::{
        form::{self, Field, Form, Mask, Outcome, Suggestion, Value},
        strings::Strings,
    },
    chrono::NaiveDate,
    scout4all_core::training::{ActivityStatus, SessionStatus, cycles::Level},
    scout4all_keys::{KeyCombination, key},
    scout4all_mobile::{
        MobileError, Scout4All,
        training::{ActivityDraft, ActivityHitRecord, CycleRecord, SessionDraft, TrainingField},
    },
};

const EMPTY: &str = "–";

/// What a key or a click did to a form.
pub enum Edited {
    Stay,
    Cancel,
    /// saved: the id of the session or of the activity
    Saved(String),
    /// the activity found, to copy
    Copy(ActivityHitRecord),
}

const DATE: usize = 0;
const TIME: usize = 1;
const DURATION: usize = 2;
const LOCATION: usize = 3;
const TITLE: usize = 4;
const OBJECTIVES: usize = 5;
const NOTES: usize = 6;
const REMARKS: usize = 7;
const ACTUAL: usize = 8;
const MICROCYCLE: usize = 9;

/// The microcycles of the team that hold the day written, and the one
/// `current` even when the day is out of it: id and name.
pub(super) fn microcycle_options(
    core: &Scout4All,
    team_id: &str,
    date: &str,
    current: Option<&str>,
) -> Vec<(String, String)> {
    let containing = day_of(date)
        .and_then(|day| {
            core.cycles_containing(
                team_id.to_string(),
                Level::Microcycle,
                day.format("%Y-%m-%d").to_string(),
            )
            .ok()
        })
        .unwrap_or_default();
    let kept = current.and_then(|id| core.cycle(Level::Microcycle, id.to_string()).ok());
    let mut options: Vec<(String, String)> = containing
        .iter()
        .map(|c| (c.id.clone(), cycle_label(c)))
        .collect();
    if let Some(kept) = kept.filter(|k| !options.iter().any(|(id, _)| *id == k.id)) {
        options.push((kept.id.clone(), cycle_label(&kept)));
    }
    options
}

/// The places written before that fit what is written (all, when the
/// arrow of the drop-down opened them).
pub(super) fn location_suggestions(
    locations: &[String],
    written: &str,
    all: bool,
) -> Vec<Suggestion> {
    let needle = written.to_lowercase();
    locations
        .iter()
        .filter(|l| all || needle.is_empty() || l.to_lowercase().contains(&needle))
        .map(|l| Suggestion {
            label: l.clone(),
            value: l.clone(),
        })
        .collect()
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

pub(super) fn cycle_label(cycle: &CycleRecord) -> String {
    match cycle.title.is_empty() {
        true => cycle.window.clone(),
        false => format!("{} ({})", cycle.title, cycle.window),
    }
}

/// The form of a session, new or saved.
pub struct SessionEditor {
    pub form: Form,
    team_id: String,
    id: Option<String>,
    status: SessionStatus,
    locations: Vec<String>,
    /// the microcycles of the choice after the empty one: id and name
    microcycles: Vec<(String, String)>,
}

impl SessionEditor {
    /// A new session of the team (on `day` if given) or the saved one.
    pub fn new(
        core: &Scout4All,
        s: &Strings,
        team_id: String,
        id: Option<String>,
        day: Option<NaiveDate>,
    ) -> Result<SessionEditor, MobileError> {
        let draft = match &id {
            Some(id) => core.session_draft(id.clone())?,
            None => core.new_session_draft(),
        };
        let locations = core.training_locations(team_id.clone())?;
        let (date, time) = split(&draft.start);
        let date = match (&id, day) {
            (None, Some(day)) => day.format("%Y-%m-%d").to_string(),
            _ => date,
        };
        let fields = vec![
            Field::masked(s.date, &date, Mask::Date, s.date_placeholder),
            Field::masked(s.time, &time, Mask::Time, s.time_placeholder),
            Field::text(s.session_duration, draft.duration.clone()),
            Field::dropdown(s.session_location, draft.location.clone()),
            Field::text(s.session_title, draft.title.clone()),
            Field::text(s.objectives, draft.objectives.clone()),
            Field::text(s.notes, draft.notes.clone()),
            Field::text(s.final_remarks, draft.final_remarks.clone()),
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
            locations,
            microcycles: vec![],
        };
        editor.offer_locations();
        editor.offer_microcycles(core, draft.microcycle.as_deref());
        Ok(editor)
    }

    fn offer_locations(&mut self) {
        let offered = location_suggestions(
            &self.locations,
            &self.form.text(LOCATION),
            self.form.browsing(LOCATION),
        );
        self.form.suggest(LOCATION, offered);
    }

    /// The microcycles that hold the day written, and the one the session is
    /// in (`current`) even when the day moved out of it: saved, it says so;
    /// the empty choice takes it out. With none chosen, the one that holds
    /// the day is proposed.
    fn offer_microcycles(&mut self, core: &Scout4All, current: Option<&str>) {
        let options = microcycle_options(core, &self.team_id, &self.form.text(DATE), current);
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
            Outcome::Changed(DATE) => {
                // the day moved: the microcycle that holds it is offered
                let current = self.microcycle();
                self.offer_microcycles(core, current.as_deref());
                Edited::Stay
            }
            Outcome::Changed(LOCATION) => {
                self.offer_locations();
                Edited::Stay
            }
            Outcome::Changed(_) => Edited::Stay,
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
            location: self.form.text(LOCATION),
            title: self.form.text(TITLE),
            objectives: self.form.text(OBJECTIVES),
            notes: self.form.text(NOTES),
            final_remarks: self.form.text(REMARKS),
            microcycle: self.microcycle(),
            status: self.status,
        }
    }

    fn save(&mut self, core: &Scout4All) -> Edited {
        match core.save_session(self.team_id.clone(), self.id.clone(), self.draft()) {
            Ok(saved) => Edited::Saved(saved.id),
            Err(e) => {
                if let MobileError::InvalidTraining { field, .. } = &e {
                    self.form.focused = match field {
                        TrainingField::Duration => DURATION,
                        TrainingField::ActualDuration => ACTUAL,
                        _ => DATE,
                    };
                }
                self.form.error = Some(e.to_string());
                Edited::Stay
            }
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
            Field::text(s.activity_description, draft.description),
            Field::text(s.activity_planned, draft.planned),
            Field::text(s.activity_actual, draft.actual),
            Field::text(s.notes, draft.notes),
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
                if let MobileError::InvalidTraining { field, .. } = &e {
                    self.form.focused = match field {
                        TrainingField::Planned => PLANNED,
                        TrainingField::Actual => ACTIVITY_ACTUAL,
                        _ => NAME,
                    };
                }
                self.form.error = Some(e.to_string());
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
                    self.form.error = Some(s.no_activities.to_string());
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
