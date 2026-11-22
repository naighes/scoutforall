//! The forms of the sessions made from others: the duplicate of a session,
//! the series ("repeat": a band for each day of the week between two days)
//! and the change of the series of a session, in a group.

use {
    super::forms::{
        Edited, clash_warning, day_of, microcycle_options, microcycle_value, split, venue_at,
        venue_options, venue_value,
    },
    crate::{
        calendar::text_of,
        form::{self, Field, Form, Outcome, Value},
        strings::Strings,
        theme::{self, Colors},
        ui,
    },
    chrono::NaiveDate,
    iced::{
        Alignment, Element, Length,
        widget::{Column, column, container, scrollable, text},
    },
    scout4all_core::training::series::SeriesScope,
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{
        MobileError, Scout4All,
        training::{DuplicateDraft, SeriesBand, SeriesChange, SeriesDraft},
    },
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

const FROM: usize = 0;
const TO: usize = 1;
const LOCATION: usize = 2;
/// the time of the band of the weekday `d` (0 Monday), then its minutes
const fn band_time(d: usize) -> usize {
    3 + 2 * d
}

/// The series to make: the days between, the place, a band for each
/// weekday written (an empty time leaves the day out). A session that
/// overlaps another is told in a dialog, which offers to make the others.
pub struct SeriesEditor {
    pub form: Form,
    team_id: String,
    venues: Vec<(String, String)>,
    /// the sessions that overlap, written, when the dialog is open
    pub conflicts: Vec<String>,
    /// another team holds the gym on some of the days and the user was
    /// told: Enter again goes on
    warned: bool,
}

impl SeriesEditor {
    pub fn new(
        core: &Scout4All,
        s: &Strings,
        team_id: String,
        day: Option<NaiveDate>,
    ) -> Result<SeriesEditor, MobileError> {
        let venues = venue_options(s, core, &team_id, None);
        let minutes = core.new_session_draft().duration;
        let from = day.map(text_of).unwrap_or_default();
        let mut fields = vec![
            Field::masked(s.from_day, &from, form::Mask::Date, s.date_placeholder),
            Field::masked(s.to_day, "", form::Mask::Date, s.date_placeholder),
            Field {
                value: venue_value(&venues, None),
                ..Field::choice(s.session_location, vec![], 0)
            },
        ];
        let mut sections = vec![];
        for (d, name) in s.weekdays.iter().enumerate() {
            sections.push((band_time(d), name.to_string()));
            fields.push(Field::masked(
                s.time,
                "",
                form::Mask::Time,
                s.time_placeholder,
            ));
            fields.push(Field::text(s.session_duration, minutes.clone()));
        }
        let mut form = Form::new(s.repeat_sessions, fields);
        form.sections = sections;
        let editor = SeriesEditor {
            form,
            team_id,
            venues,
            conflicts: vec![],
            warned: false,
        };
        Ok(editor)
    }

    fn draft(&self) -> SeriesDraft {
        let venue = venue_at(&self.venues, self.form.index(LOCATION));
        SeriesDraft {
            from: self.form.text(FROM),
            to: self.form.text(TO),
            bands: (0..7)
                .filter_map(|d| {
                    let time = self.form.text(band_time(d));
                    (time.trim().len() == 5).then(|| SeriesBand {
                        weekday: u8::try_from(d + 1).unwrap_or(1),
                        time,
                        duration_min: self.form.text(band_time(d) + 1).trim().parse().unwrap_or(0),
                        venue: venue.clone(),
                    })
                })
                .collect(),
        }
    }

    /// The series made; with a conflict and not `create_others`, the
    /// dialog opens and nothing is written.
    fn make(&mut self, core: &Scout4All, s: &Strings, create_others: bool) -> Edited {
        if day_of(&self.form.text(FROM)).is_none() || day_of(&self.form.text(TO)).is_none() {
            self.form.focused = FROM;
        }
        match core.create_series(self.team_id.clone(), self.draft(), create_others) {
            Ok(result) => match (result.created.first(), result.conflicts.is_empty()) {
                (Some(first), _) => Edited::Saved(first.id.clone()),
                (None, false) => {
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
                (None, true) => {
                    self.form.error = Some(s.no_sessions.to_string());
                    Edited::Stay
                }
            },
            Err(e) => {
                self.conflicts.clear();
                self.form.error = Some(e.to_string());
                Edited::Stay
            }
        }
    }

    fn outcome(&mut self, core: &Scout4All, s: &Strings, outcome: Outcome) -> Edited {
        match outcome {
            Outcome::Stay => Edited::Stay,
            Outcome::Cancel => Edited::Cancel,
            Outcome::Changed(_) => {
                self.warned = false;
                Edited::Stay
            }
            Outcome::Save => {
                self.form.error = None;
                let draft = self.draft();
                let busy = draft.bands.iter().any(|b| b.venue.is_some());
                if busy && !self.warned {
                    let warning = clash_warning(
                        s.save_anyway_hint,
                        core.series_venue_clashes(self.team_id.clone(), draft),
                    );
                    self.warned = warning.is_some();
                    if warning.is_some() {
                        self.form.error = warning;
                        return Edited::Stay;
                    }
                }
                self.make(core, s, false)
            }
        }
    }

    pub fn update(&mut self, core: &Scout4All, s: &Strings, message: form::Message) -> Edited {
        let outcome = self.form.update(message);
        self.outcome(core, s, outcome)
    }

    pub fn key(&mut self, core: &Scout4All, s: &Strings, ck: KeyCombination) -> Edited {
        match (self.conflicts.is_empty(), ck) {
            (false, k) if k == key!(enter) => self.make(core, s, true),
            (false, k) if k == BACK_KEY => {
                self.conflicts.clear();
                Edited::Stay
            }
            (false, _) => Edited::Stay,
            (true, _) => {
                let outcome = self.form.key(ck);
                self.outcome(core, s, outcome)
            }
        }
    }

    /// The answer of the dialog of the conflicts: make the others (or, with
    /// `false`, go back to the form).
    pub fn answer(&mut self, core: &Scout4All, s: &Strings, others: bool) -> Edited {
        match others {
            true => self.make(core, s, true),
            false => {
                self.conflicts.clear();
                Edited::Stay
            }
        }
    }

    /// The dialog of the conflicts, `confirm` making the others.
    pub fn dialog<'a, M: Clone + 'a>(
        &'a self,
        s: &'static Strings,
        c: Colors,
        confirm: M,
        dismiss: M,
    ) -> Element<'a, M> {
        let rows: Vec<Element<'a, M>> = self
            .conflicts
            .iter()
            .map(|day| ui::body(day.clone()).into())
            .collect();
        let question = ui::panel(
            None,
            column![
                text(s.repeat_sessions)
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
}

const SCOPE: usize = 0;
const CHANGE_LOCATION: usize = 1;
const CHANGE_TIME: usize = 2;
const CHANGE_MINUTES: usize = 3;
const REMOVE: usize = 4;

const SCOPES: [SeriesScope; 3] = [
    SeriesScope::This,
    SeriesScope::ThisAndFollowing,
    SeriesScope::All,
];

/// The change of the series of a session: where it applies, the place, the
/// time and the minutes (only what is changed is sent); or the removal of
/// the planned sessions it applies to.
pub struct SeriesChangeEditor {
    pub form: Form,
    session_id: String,
    venues: Vec<(String, String)>,
    venue: Option<String>,
    time: String,
    minutes: String,
}

impl SeriesChangeEditor {
    pub fn new(
        core: &Scout4All,
        s: &Strings,
        team_id: String,
        session_id: String,
    ) -> Result<SeriesChangeEditor, MobileError> {
        let record = core.training_session(session_id.clone())?;
        let venues = venue_options(s, core, &team_id, record.venue_id.as_deref());
        let time = record.start.get(11..16).unwrap_or_default().to_string();
        let minutes = record.duration_min.to_string();
        let fields = vec![
            Field::choice(
                s.series_scope,
                vec![
                    s.scope_this.to_string(),
                    s.scope_following.to_string(),
                    s.scope_all.to_string(),
                ],
                2,
            ),
            Field {
                value: venue_value(&venues, record.venue_id.as_deref()),
                ..Field::choice(s.session_location, vec![], 0)
            },
            Field::masked(s.time, &time, form::Mask::Time, s.time_placeholder),
            Field::text(s.session_duration, minutes.clone()),
            Field::check(s.remove_series_check, false, false),
        ];
        let editor = SeriesChangeEditor {
            form: Form::new(s.edit_series, fields),
            session_id,
            venues,
            venue: record.venue_id,
            time,
            minutes,
        };
        Ok(editor)
    }

    fn scope(&self) -> SeriesScope {
        SCOPES
            .get(self.form.index(SCOPE))
            .copied()
            .unwrap_or(SeriesScope::All)
    }

    /// `Some` of what is not as the session has it.
    fn change(&self) -> SeriesChange {
        let differs = |field: usize, old: &str| {
            let new = self.form.text(field);
            (new.trim() != old).then(|| new.trim().to_string())
        };
        let venue = venue_at(&self.venues, self.form.index(CHANGE_LOCATION));
        SeriesChange {
            // the gym goes when the empty choice is made, and changes when another is
            clear_venue: venue.is_none() && self.venue.is_some(),
            venue: venue.filter(|v| self.venue.as_ref() != Some(v)),
            time: differs(CHANGE_TIME, &self.time),
            duration_min: differs(CHANGE_MINUTES, &self.minutes).map(|m| m.parse().unwrap_or(0)),
        }
    }

    fn save(&mut self, core: &Scout4All) -> Edited {
        let result = match checked(&self.form, REMOVE) {
            true => core
                .remove_series(self.session_id.clone(), self.scope())
                .map(|_| ()),
            false => core
                .update_series(self.session_id.clone(), self.scope(), self.change())
                .map(|_| ()),
        };
        match result {
            Ok(()) => Edited::Saved(self.session_id.clone()),
            Err(e) => {
                self.form.error = Some(e.to_string());
                Edited::Stay
            }
        }
    }

    fn outcome(&mut self, core: &Scout4All, outcome: Outcome) -> Edited {
        match outcome {
            Outcome::Stay => Edited::Stay,
            Outcome::Cancel => Edited::Cancel,
            Outcome::Changed(_) => Edited::Stay,
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
