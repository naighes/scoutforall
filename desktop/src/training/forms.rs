//! The forms of the training: a session, an activity, and the search of an
//! activity written before, to copy. The fields of `form`, saved through
//! `mobile`, which checks them; an error takes the keys to its field.

use {
    crate::{
        calendar::text_of,
        form::{self, Field, Form, Mask, Outcome, Suggestion, Value},
        strings::Strings,
    },
    chrono::NaiveDate,
    scout4all_core::training::{ActivityStatus, SessionStatus, cycles::Level},
    scout4all_keys::{KeyCombination, key},
    scout4all_mobile::{
        MobileError, Scout4All,
        training::{ActivityDraft, ActivityHitRecord, CycleRecord, SessionDraft, TrainingField},
        venues::VenueClashRecord,
    },
};

const EMPTY: &str = "–";

pub(super) use crate::venue_choice::{venue_at, venue_options, venue_value};

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

/// The form of a session, new or saved.
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
    /// the words of the forms (the choices are written again on a change)
    s: &'static Strings,
}

impl SessionEditor {
    /// A new session of the team (on `day` if given) or the saved one.
    pub fn new(
        core: &Scout4All,
        s: &'static Strings,
        team_id: String,
        id: Option<String>,
        day: Option<NaiveDate>,
    ) -> Result<SessionEditor, MobileError> {
        let draft = match &id {
            Some(id) => core.session_draft(id.clone())?,
            None => core.new_session_draft(),
        };
        let venues = venue_options(s, core, &team_id, draft.venue.as_deref());
        let (date, time) = split(&draft.start);
        let date = match (&id, day) {
            (None, Some(day)) => text_of(day),
            _ => date,
        };
        let fields = vec![
            Field::masked(s.date, &date, Mask::Date, s.date_placeholder),
            Field::masked(s.time, &time, Mask::Time, s.time_placeholder),
            Field::text(s.session_duration, draft.duration.clone()),
            Field {
                value: venue_value(&venues, draft.venue.as_deref()),
                ..Field::choice(s.session_location, vec![], 0)
            },
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
            s,
        };
        editor.offer_microcycles(core, draft.microcycle.as_deref());
        Ok(editor)
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
            self.form.error = Some(warning);
            return Edited::Stay;
        }
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
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
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
        let told = editor.form.error.clone().unwrap();
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
}
