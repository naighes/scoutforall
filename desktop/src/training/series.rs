//! The form of a session made from another: its duplicate. (The weekly
//! series are written in the form of a new session, `forms`.)

use {
    super::forms::{Edited, clash_warning, microcycle_options, microcycle_value, split},
    crate::{
        form::{self, Field, Form, Outcome, Value},
        strings::Strings,
    },
    scout4all_keys::KeyCombination,
    scout4all_mobile::{MobileError, Scout4All, training::DuplicateDraft},
};

const DATE: usize = 0;
const TIME: usize = 1;
const MICROCYCLE: usize = 2;
const OBJECTIVES: usize = 3;
const ACTIVITIES: usize = 4;

fn checked(form: &Form, field: usize) -> bool {
    matches!(
        form.fields.get(field).map(|f| &f.value),
        Some(Value::Check { on: true, .. })
    )
}

/// The form of the duplicate of a session: when, the microcycle, what to
/// copy besides the title, the place and the minutes.
pub struct DuplicateEditor {
    pub form: Form,
    team_id: String,
    session_id: String,
    microcycles: Vec<(String, String)>,
    /// another team holds the gym then and the user was told: Enter again
    /// saves
    warned: bool,
    /// the words of the forms (the choices are written again on a change)
    s: &'static Strings,
}

impl DuplicateEditor {
    pub fn new(
        core: &Scout4All,
        s: &'static Strings,
        team_id: String,
        session_id: String,
    ) -> Result<DuplicateEditor, MobileError> {
        let proposed = core.proposed_duplicate(session_id.clone())?;
        let (date, time) = split(&Some(proposed.start.clone()));
        let fields = vec![
            Field::masked(s.date, &date, form::Mask::Date, s.date_placeholder),
            Field::masked(s.time, &time, form::Mask::Time, s.time_placeholder),
            Field::choice(s.microcycle, vec!["–".to_string()], 0),
            Field::check(s.copy_objectives, proposed.copy_objectives, false),
            Field::check(s.copy_activities, proposed.copy_activities, false),
        ];
        let mut editor = DuplicateEditor {
            form: Form::new(s.duplicate_session, fields),
            team_id,
            session_id,
            microcycles: vec![],
            warned: false,
            s,
        };
        editor.offer_microcycles(core, proposed.microcycle_id.as_deref());
        Ok(editor)
    }

    fn offer_microcycles(&mut self, core: &Scout4All, current: Option<&str>) {
        let options =
            microcycle_options(self.s, core, &self.team_id, &self.form.text(DATE), current);
        let chosen = current.and_then(|id| options.iter().position(|(o, _)| o == id));
        if let Some(field) = self.form.fields.get_mut(MICROCYCLE) {
            field.value = microcycle_value(&options, chosen);
        }
        self.microcycles = options;
    }

    fn microcycle(&self) -> Option<String> {
        self.form
            .index(MICROCYCLE)
            .checked_sub(1)
            .and_then(|at| self.microcycles.get(at))
            .map(|(id, _)| id.clone())
    }

    fn save(&mut self, core: &Scout4All) -> Edited {
        let (date, time) = (self.form.text(DATE), self.form.text(TIME));
        let draft = DuplicateDraft {
            start: format!("{}T{}", date.trim(), time.trim()),
            microcycle_id: self.microcycle(),
            copy_objectives: checked(&self.form, OBJECTIVES),
            copy_activities: checked(&self.form, ACTIVITIES),
        };
        if !self.warned {
            let warning = clash_warning(
                self.s.save_anyway_hint,
                core.duplicate_venue_clashes(self.session_id.clone(), draft.clone()),
            );
            self.warned = warning.is_some();
            if warning.is_some() {
                self.form.error = warning;
                return Edited::Stay;
            }
        }
        match core.duplicate_session(self.session_id.clone(), draft) {
            Ok(saved) => Edited::Saved(saved.id),
            Err(e) => {
                self.form.focused = DATE;
                self.form.error = Some(e.to_string());
                Edited::Stay
            }
        }
    }

    fn outcome(&mut self, core: &Scout4All, outcome: Outcome) -> Edited {
        match outcome {
            Outcome::Stay => Edited::Stay,
            Outcome::Cancel => Edited::Cancel,
            Outcome::Changed(DATE) => {
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

    pub fn update(&mut self, core: &Scout4All, message: form::Message) -> Edited {
        let outcome = self.form.update(message);
        self.outcome(core, outcome)
    }

    pub fn key(&mut self, core: &Scout4All, ck: KeyCombination) -> Edited {
        let outcome = self.form.key(ck);
        self.outcome(core, outcome)
    }
}
