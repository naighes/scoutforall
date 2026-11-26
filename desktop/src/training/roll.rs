//! The attendance of a session: a row for each player, their state changed
//! with a key (p present, l late, a absent, u unavailable) or a click, a
//! note on each, guests of other teams (or a new person, in no team) and
//! the players that joined later added; a guest can make up for a session
//! of their own team (r, or the box). Saved as it is: the list is the one
//! written, from then on.

use {
    crate::{
        form::{self, Field, Form, Outcome, Suggestion},
        strings::Strings,
        theme::{self, Colors, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, column, container, row, scrollable, space, text},
    },
    scout4all_core::training::AttendanceStatus,
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{
        MobileError, Scout4All,
        training::{AttendanceRowRecord, GuestChoice},
    },
};

const GUESTS_OFFERED: usize = 20;

/// What the guest search offers last, for a person in no team made from
/// the name written.
const NEW_PERSON: &str = "";

#[derive(Debug, Clone)]
pub enum Message {
    Row(usize),
    /// the state of a row, to the next one
    Cycle(usize),
    /// the makeup box of a guest row
    Makeup(usize),
    Note,
    AddGuest,
    AddMissing,
    Remove,
    Save,
    Cancel,
    Form(form::Message),
}

/// What a key or a click did.
pub enum Done {
    Stay,
    Cancel,
    Saved,
}

/// A form over the list: the note of a row, or the guest to add.
enum Over {
    Note(Form),
    Guest {
        form: Form,
        choices: Vec<GuestChoice>,
        offered: Vec<String>,
        taken: Option<usize>,
    },
}

pub struct Roll {
    session_id: String,
    team_id: String,
    rows: Vec<AttendanceRowRecord>,
    selected: usize,
    over: Option<Over>,
    error: Option<String>,
}

/// The state after `status` when it is changed with a click or a key.
fn next(status: AttendanceStatus) -> AttendanceStatus {
    match status {
        AttendanceStatus::Present => AttendanceStatus::Late,
        AttendanceStatus::Late => AttendanceStatus::Absent,
        AttendanceStatus::Absent => AttendanceStatus::Unavailable,
        AttendanceStatus::Unavailable => AttendanceStatus::Present,
    }
}

fn status_name(s: &Strings, status: AttendanceStatus) -> &'static str {
    match status {
        AttendanceStatus::Present => s.present,
        AttendanceStatus::Late => s.late,
        AttendanceStatus::Absent => s.absent,
        AttendanceStatus::Unavailable => s.unavailable,
    }
}

impl Roll {
    pub fn new(core: &Scout4All, session_id: String, team_id: String) -> Result<Roll, MobileError> {
        let sheet = core.session_attendance(session_id.clone())?;
        Ok(Roll {
            session_id,
            team_id,
            rows: sheet.rows,
            selected: 0,
            over: None,
            error: None,
        })
    }

    fn selected_row(&mut self) -> Option<&mut AttendanceRowRecord> {
        self.rows.get_mut(self.selected)
    }

    fn set(&mut self, status: AttendanceStatus) {
        if let Some(row) = self.selected_row() {
            row.status = status;
        }
    }

    fn open_note(&mut self, s: &Strings) {
        let Some(row) = self.rows.get(self.selected) else {
            return;
        };
        let mut form = Form::new(
            &row.name,
            vec![Field::text(s.attendance_note, row.note.clone())],
        );
        form.plain = false;
        self.over = Some(Over::Note(form));
    }

    fn open_guest(&mut self, core: &Scout4All, s: &Strings) {
        match core.guest_choices(self.team_id.clone()) {
            Ok(choices) => {
                let mut roll_form = Form::new(s.add_guest, vec![Field::search(s.name, "")]);
                let offered = Roll::offer(s, &mut roll_form, &choices, &self.rows);
                self.over = Some(Over::Guest {
                    form: roll_form,
                    choices,
                    offered,
                    taken: None,
                });
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// The people that match the name written, not in the list yet, then a
    /// new person with that name; the ids in the order the form offers them.
    fn offer(
        s: &Strings,
        form: &mut Form,
        choices: &[GuestChoice],
        rows: &[AttendanceRowRecord],
    ) -> Vec<String> {
        let written = form.text(0).trim().to_string();
        let needle = written.to_lowercase();
        let found: Vec<&GuestChoice> = choices
            .iter()
            .filter(|c| !rows.iter().any(|r| r.person_id == c.person_id))
            .filter(|c| needle.is_empty() || c.name.to_lowercase().contains(&needle))
            .take(GUESTS_OFFERED)
            .collect();
        form.suggest(
            0,
            found
                .iter()
                .map(|c| Suggestion {
                    label: format!(
                        "{} · {}",
                        c.name,
                        c.teams
                            .iter()
                            .map(|t| format!("{} {} #{}", t.name, t.year, t.number))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    value: c.name.clone(),
                })
                .chain((!written.is_empty()).then(|| Suggestion {
                    label: ui::fill(s.new_person, std::slice::from_ref(&written)),
                    value: written.clone(),
                }))
                .collect(),
        );
        found
            .iter()
            .map(|c| c.person_id.clone())
            .chain((!written.is_empty()).then(|| NEW_PERSON.to_string()))
            .collect()
    }

    /// What went wrong doing something: the app tells it over the screen.
    pub fn failure(&self) -> &Option<String> {
        &self.error
    }

    pub fn failure_mut(&mut self) -> &mut Option<String> {
        &mut self.error
    }

    fn toggle_makeup(&mut self, i: usize) {
        if let Some(row) = self.rows.get_mut(i).filter(|r| r.guest) {
            row.makeup = !row.makeup;
        }
    }

    fn add_missing(&mut self, core: &Scout4All) {
        match core.missing_players(self.session_id.clone()) {
            Ok(missing) => {
                let new: Vec<AttendanceRowRecord> = missing
                    .into_iter()
                    .filter(|m| !self.rows.iter().any(|r| r.person_id == m.person_id))
                    .collect();
                self.rows.extend(new);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn remove(&mut self) {
        if self.rows.get(self.selected).is_some_and(|r| r.guest) {
            self.rows.remove(self.selected);
            self.selected = self.selected.min(self.rows.len().saturating_sub(1));
        }
    }

    fn save(&mut self, core: &Scout4All) -> Done {
        match core.save_attendance(self.session_id.clone(), self.rows.clone()) {
            Ok(()) => Done::Saved,
            Err(e) => {
                self.error = Some(e.to_string());
                Done::Stay
            }
        }
    }

    /// The guest taken, in the list: one of the registry, or a new person
    /// with the name written.
    fn add_guest(&mut self, core: &Scout4All, taken: Option<usize>) -> bool {
        let Some(Over::Guest {
            form,
            choices,
            offered,
            ..
        }) = &self.over
        else {
            return false;
        };
        let id = taken
            .or((!offered.is_empty()).then_some(0))
            .and_then(|at| offered.get(at));
        let row = |person: &GuestChoice| AttendanceRowRecord {
            person_id: person.person_id.clone(),
            name: person.name.clone(),
            number: person.teams.first().map(|t| t.number),
            status: AttendanceStatus::Present,
            note: String::new(),
            guest: true,
            // a player of another team of the user makes up by default
            makeup: !person.teams.is_empty(),
        };
        let made = match id.map(String::as_str) {
            None => return false,
            Some(NEW_PERSON) => core
                .create_guest(form.text(0))
                .map(|person| row(&person))
                .map_err(|e| e.to_string()),
            Some(id) => match choices.iter().find(|c| c.person_id == id) {
                Some(person) => Ok(row(person)),
                None => return false,
            },
        };
        self.over = None;
        match made {
            Ok(made) => {
                self.rows.push(made);
                self.selected = self.rows.len().saturating_sub(1);
                true
            }
            Err(e) => {
                self.error = Some(e);
                false
            }
        }
    }

    fn over_outcome(&mut self, core: &Scout4All, s: &Strings, outcome: Outcome) {
        match (outcome, &mut self.over) {
            (Outcome::Cancel, _) => self.over = None,
            (Outcome::Save, Some(Over::Note(form))) => {
                let note = form.text(0);
                if let Some(row) = self.rows.get_mut(self.selected) {
                    row.note = note;
                }
                self.over = None;
            }
            (Outcome::Save, Some(Over::Guest { taken, .. })) => {
                let taken = *taken;
                self.add_guest(core, taken);
            }
            (
                Outcome::Changed(0),
                Some(Over::Guest {
                    form,
                    choices,
                    offered,
                    taken,
                }),
            ) => {
                *taken = None;
                *offered = Roll::offer(s, form, choices, &self.rows);
            }
            _ => {}
        }
    }

    pub fn update(&mut self, core: &Scout4All, s: &Strings, message: Message) -> Done {
        match (message, &mut self.over) {
            (Message::Form(m), Some(over)) => {
                let outcome = match over {
                    Over::Note(form) => form.update(m),
                    Over::Guest { form, taken, .. } => {
                        if let form::Message::Take(0, n) = m {
                            *taken = Some(n);
                        }
                        form.update(m)
                    }
                };
                self.over_outcome(core, s, outcome);
                Done::Stay
            }
            (_, Some(_)) => Done::Stay,
            (Message::Row(i), None) => {
                self.selected = i;
                Done::Stay
            }
            (Message::Makeup(i), None) => {
                self.selected = i;
                self.toggle_makeup(i);
                Done::Stay
            }
            (Message::Cycle(i), None) => {
                self.selected = i;
                if let Some(row) = self.rows.get_mut(i) {
                    row.status = next(row.status);
                }
                Done::Stay
            }
            (Message::Note, None) => {
                self.open_note(s);
                Done::Stay
            }
            (Message::AddGuest, None) => {
                self.open_guest(core, s);
                Done::Stay
            }
            (Message::AddMissing, None) => {
                self.add_missing(core);
                Done::Stay
            }
            (Message::Remove, None) => {
                self.remove();
                Done::Stay
            }
            (Message::Save, None) => self.save(core),
            (Message::Cancel, None) => Done::Cancel,
            (Message::Form(_), None) => Done::Stay,
        }
    }

    pub fn key(&mut self, core: &Scout4All, s: &Strings, ck: KeyCombination) -> Done {
        // the guest offered, taken with Enter: it joins the list right away
        let entered = match &self.over {
            Some(Over::Guest { form, .. }) if ck == key!(enter) && form.offering() => form.lit(),
            _ => None,
        };
        if entered.is_some() && self.add_guest(core, entered) {
            return Done::Stay;
        }
        if let Some(over) = &mut self.over {
            let outcome = match over {
                Over::Note(form) => form.key(ck),
                Over::Guest { form, taken, .. } => {
                    if form.offering() && ck == key!(tab) {
                        *taken = form.lit().or(*taken);
                    }
                    form.key(ck)
                }
            };
            self.over_outcome(core, s, outcome);
            return Done::Stay;
        }
        let count = self.rows.len();
        match ck {
            k if k == BACK_KEY => return Done::Cancel,
            k if k == key!(enter) => return self.save(core),
            k if k == key!(down) && count > 0 => self.selected = (self.selected + 1) % count,
            k if k == key!(up) && count > 0 => {
                self.selected = self.selected.checked_sub(1).unwrap_or(count - 1)
            }
            k if k == key!(space) || k == key!(right) => {
                if let Some(row) = self.selected_row() {
                    row.status = next(row.status);
                }
            }
            k if k == key!(p) => self.set(AttendanceStatus::Present),
            k if k == key!(l) => self.set(AttendanceStatus::Late),
            k if k == key!(a) => self.set(AttendanceStatus::Absent),
            k if k == key!(u) => self.set(AttendanceStatus::Unavailable),
            k if k == key!(n) => self.open_note(s),
            k if k == key!(g) => self.open_guest(core, s),
            k if k == key!(m) => self.add_missing(core),
            k if k == key!(r) => self.toggle_makeup(self.selected),
            k if k == key!(backspace) || k == key!(delete) => self.remove(),
            _ => {}
        }
        Done::Stay
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match &self.over {
            Some(Over::Note(form) | Over::Guest { form, .. }) => form.on_field(s, Form::hints(s)),
            None => vec![
                ("↑ ↓".to_string(), String::new()),
                (format!("← → {}", s.space), s.change.to_string()),
                (
                    "p l a u".to_string(),
                    format!("{} {} {} {}", s.present, s.late, s.absent, s.unavailable),
                ),
                ("n".to_string(), s.attendance_note.to_string()),
                ("g".to_string(), s.add_guest.to_string()),
                ("m".to_string(), s.add_missing.to_string()),
                ("r".to_string(), s.makeup.to_string()),
                ("⌫".to_string(), s.remove.to_string()),
                ("Enter".to_string(), s.save.to_string()),
                ("Esc".to_string(), s.cancel.to_string()),
            ],
        }
    }

    pub fn focus_id(&self) -> Option<iced::widget::Id> {
        match &self.over {
            Some(Over::Note(form) | Over::Guest { form, .. }) => Some(form.focus_id()),
            None => None,
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        if let Some(Over::Note(form) | Over::Guest { form, .. }) = &self.over {
            return container(container(form.view(s, c, Message::Form, None)).max_width(620))
                .center_x(Length::Fill)
                .into();
        }
        let status_color = |status: AttendanceStatus| match status {
            AttendanceStatus::Present => c.won,
            AttendanceStatus::Late => c.accent,
            AttendanceStatus::Absent => c.lost,
            AttendanceStatus::Unavailable => c.muted,
        };
        let rows: Vec<Element<'a, Message>> = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let number = r.number.map(|n| format!("#{n}")).unwrap_or_default();
                let mut name = match r.name.is_empty() {
                    true => "–".to_string(),
                    false => r.name.clone(),
                };
                if r.guest {
                    name = format!("{name} ({})", s.guest);
                }
                // the box of a guest, the room of it on the others
                let makeup: Element<'a, Message> = match r.guest {
                    true => row![
                        ui::mark_box(r.makeup, Message::Makeup(i), c),
                        text(s.makeup).size(SIZE_SMALL).color(c.muted),
                    ]
                    .align_y(Alignment::Center)
                    .width(150)
                    .into(),
                    false => space::horizontal().width(150).into(),
                };
                let line = row![
                    text(number).size(SIZE_SMALL).color(c.muted).width(46),
                    ui::name(name),
                    text(r.note.clone())
                        .size(SIZE_SMALL)
                        .color(c.muted)
                        .width(Length::FillPortion(2)),
                    makeup,
                    iced::widget::button(
                        text(status_name(s, r.status))
                            .size(SIZE_SMALL)
                            .color(status_color(r.status))
                            .font(theme::fonts().bold)
                    )
                    .style(iced::widget::button::text)
                    .width(110)
                    .on_press(Message::Cycle(i)),
                ]
                .spacing(10)
                .align_y(Alignment::Center);
                ui::list_row(line, i == self.selected, Some(Message::Row(i)), c)
            })
            .collect();
        let buttons = ui::answers(
            Some((theme::icons::CLOSE, s.cancel, Message::Cancel)),
            theme::icons::CHECK,
            s.save,
            Some(Message::Save),
            c,
        );
        let actions: Vec<Element<'a, Message>> = vec![
            ui::pill(s.add_guest, false, Some(Message::AddGuest), c),
            ui::pill(s.add_missing, false, Some(Message::AddMissing), c),
            ui::pill(s.attendance_note, false, Some(Message::Note), c),
            ui::pill(s.remove, false, Some(Message::Remove), c),
        ];
        ui::panel_with(
            s.attendance,
            actions,
            column![
                scrollable(Column::with_children(rows).spacing(3)).height(Length::Fill),
                buttons
            ]
            .spacing(10),
            c,
        )
    }
}
