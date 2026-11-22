//! The cycles of the training of a team: the programs, the mesocycles and
//! the microcycles, one level at a time (← → change it), the sheet of one
//! with its totals and what it contains, and the forms to make, change,
//! fill, duplicate and delete them.

use {
    super::forms::{Edited, clash_warning, cycle_label, day_of},
    crate::{
        form::{self, Field, Form, Mask, Outcome, Value},
        strings::Strings,
        theme::{self, Colors, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, Id, column, container, row, scrollable, space, text},
    },
    scout4all_core::training::cycles::Level,
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{
        MobileError, Scout4All,
        training::{
            CycleDraft, CycleRecord, DuplicateCycleDraft, TotalsRecord, TrainingField,
            TrainingScope,
        },
    },
    std::{collections::HashSet, sync::Arc},
};

const LIST: &str = "training-cycles";
const LEVELS: [Level; 3] = [Level::Program, Level::Mesocycle, Level::Microcycle];

pub fn level_name(s: &Strings, level: Level) -> &'static str {
    match level {
        Level::Program => s.program,
        Level::Mesocycle => s.mesocycle,
        Level::Microcycle => s.microcycle,
    }
}

fn level_plural(s: &Strings, level: Level) -> &'static str {
    match level {
        Level::Program => s.programs,
        Level::Mesocycle => s.mesocycles,
        Level::Microcycle => s.microcycles,
    }
}

fn create_name(s: &Strings, level: Level) -> &'static str {
    match level {
        Level::Program => s.create_program,
        Level::Mesocycle => s.create_mesocycle,
        Level::Microcycle => s.create_microcycle,
    }
}

const TITLE: usize = 0;
const START: usize = 1;
/// the weeks of a mesocycle or a microcycle, the last day of a program
const SPAN: usize = 2;
const OBJECTIVES: usize = 3;
const NOTES: usize = 4;
const PARENT: usize = 5;

/// The form of a cycle, new or saved; the children a new one takes are the
/// free ones it was made from.
pub struct CycleEditor {
    pub form: Form,
    team_id: String,
    level: Level,
    id: Option<String>,
    children: Vec<String>,
    /// the cycles of the choice after the empty one: id and name
    parents: Vec<(String, String)>,
    /// the children that fall out of the window were told: Enter again
    /// detaches them
    detach: bool,
}

impl CycleEditor {
    pub fn new(
        core: &Scout4All,
        s: &Strings,
        team_id: String,
        level: Level,
        id: Option<String>,
        seed: Option<CycleDraft>,
        children: Vec<String>,
    ) -> Result<CycleEditor, MobileError> {
        let draft = match (&id, seed) {
            (Some(id), _) => core.cycle_draft(level, id.clone())?,
            (None, Some(seed)) => seed,
            (None, None) => core.new_cycle_draft(),
        };
        let start = draft.start.clone().unwrap_or_default();
        let parent_level = level.parent();
        let parents: Vec<(String, String)> = match parent_level {
            Some(up) => core
                .cycles(team_id.clone(), up)?
                .iter()
                .map(|c| (c.id.clone(), cycle_label(c)))
                .collect(),
            None => vec![],
        };
        // a new cycle is proposed in the one that holds its first day
        let proposed = match (&draft.parent, &id, parent_level, day_of(&start)) {
            (None, None, Some(up), Some(day)) => core
                .cycles_containing(team_id.clone(), up, day.format("%Y-%m-%d").to_string())
                .ok()
                .and_then(|found| found.into_iter().next())
                .map(|c| c.id),
            _ => None,
        };
        let chosen = draft
            .parent
            .clone()
            .or(proposed)
            .and_then(|p| parents.iter().position(|(id, _)| *id == p));
        let span = match level {
            Level::Program => Field::masked(
                s.cycle_end,
                &draft.end.clone().unwrap_or_default(),
                Mask::Date,
                s.date_placeholder,
            ),
            _ => Field::text(s.cycle_weeks, draft.weeks.clone()),
        };
        let mut fields = vec![
            Field::text(s.session_title, draft.title.clone()),
            Field::masked(s.cycle_start, &start, Mask::Date, s.date_placeholder),
            span,
            Field::text(s.objectives, draft.objectives.clone()),
            Field::text(s.notes, draft.notes.clone()),
        ];
        if parent_level.is_some() {
            fields.push(Field::choice(
                s.cycle_parent,
                std::iter::once("–".to_string())
                    .chain(parents.iter().map(|(_, name)| name.clone()))
                    .collect(),
                chosen.map_or(0, |at| at + 1),
            ));
        }
        let title = format!(
            "{} · {}",
            match id {
                Some(_) => s.edit_cycle,
                None => s.new_cycle,
            },
            level_name(s, level)
        );
        Ok(CycleEditor {
            form: Form::new(&title, fields),
            team_id,
            level,
            id,
            children,
            parents,
            detach: false,
        })
    }

    fn parent(&self) -> Option<String> {
        match self.level.parent() {
            Some(_) => self
                .form
                .index(PARENT)
                .checked_sub(1)
                .and_then(|at| self.parents.get(at))
                .map(|(id, _)| id.clone()),
            None => None,
        }
    }

    fn draft(&self) -> CycleDraft {
        let start = self.form.text(START);
        let span = self.form.text(SPAN);
        let valid = |text: &str| day_of(text).map(|d| d.format("%Y-%m-%d").to_string());
        CycleDraft {
            title: self.form.text(TITLE),
            start: valid(&start),
            weeks: match self.level {
                Level::Program => String::new(),
                _ => span.clone(),
            },
            end: match self.level {
                Level::Program => valid(&span),
                _ => None,
            },
            objectives: self.form.text(OBJECTIVES),
            notes: self.form.text(NOTES),
            parent: self.parent(),
        }
    }

    fn save(&mut self, core: &Scout4All, s: &Strings) -> Edited {
        let saved = core.save_cycle(
            self.team_id.clone(),
            self.level,
            self.id.clone(),
            self.draft(),
            self.children.clone(),
            self.detach,
        );
        match saved {
            Ok(cycle) => Edited::Saved(cycle.id),
            Err(e) => {
                self.form.focused = match &e {
                    MobileError::InvalidTraining { field, .. } => match field {
                        TrainingField::Start => START,
                        _ => SPAN,
                    },
                    _ => START,
                };
                self.detach = matches!(e, MobileError::ChildrenOutside { .. });
                self.form.error = Some(match self.detach {
                    true => format!("{}\n{}", e, s.detach_hint),
                    false => e.to_string(),
                });
                Edited::Stay
            }
        }
    }

    fn outcome(&mut self, core: &Scout4All, s: &Strings, outcome: Outcome) -> Edited {
        match outcome {
            Outcome::Stay => Edited::Stay,
            Outcome::Cancel => Edited::Cancel,
            // what changes the window asks again about the children
            Outcome::Changed(START | SPAN) => {
                self.detach = false;
                Edited::Stay
            }
            Outcome::Changed(_) => Edited::Stay,
            Outcome::Save => self.save(core, s),
        }
    }

    pub fn update(&mut self, core: &Scout4All, s: &Strings, message: form::Message) -> Edited {
        let outcome = self.form.update(message);
        self.outcome(core, s, outcome)
    }

    pub fn key(&mut self, core: &Scout4All, s: &Strings, ck: KeyCombination) -> Edited {
        let outcome = self.form.key(ck);
        self.outcome(core, s, outcome)
    }
}

/// What a small form on a cycle asks.
enum Kind {
    /// the free children to put in: their ids, in the order of the checks
    Add(Vec<String>),
    Duplicate,
    Remove,
}

/// A small form over the cycle of the sheet.
struct Overlay {
    form: Form,
    kind: Kind,
    /// another team holds the gym on some of the days and the user was
    /// told: Enter again saves
    warned: bool,
}

fn checked(form: &Form, field: usize) -> bool {
    matches!(
        form.fields.get(field).map(|f| &f.value),
        Some(Value::Check { on: true, .. })
    )
}

/// A row of what a cycle contains: a cycle, or a session of a microcycle.
struct Child {
    id: String,
    label: String,
}

/// The sheet of a cycle.
struct Card {
    record: CycleRecord,
    totals: TotalsRecord,
    children: Vec<Child>,
    selected: usize,
}

enum Mode {
    List,
    Card,
    Editor(Box<CycleEditor>),
    Overlay(Box<Overlay>),
}

#[derive(Debug, Clone)]
pub enum Message {
    Level(Level),
    Row(usize),
    New,
    Create,
    Edit,
    Add,
    Duplicate,
    Pdf,
    Delete,
    Child(usize),
    Form(form::Message),
}

pub enum Leave {
    Stay,
    /// back to the plan
    Back,
    /// the sheet of a session
    Session(String),
    /// the PDF of the plan of the microcycle, to share
    Export(crate::transfer::Export),
}

pub struct CyclesTab {
    core: Arc<Scout4All>,
    team_id: String,
    level: Level,
    cycles: Vec<CycleRecord>,
    selected: usize,
    /// the free ones marked to be put in a cycle of the level above
    marked: HashSet<String>,
    card: Option<Card>,
    mode: Mode,
    notice: Option<String>,
    error: Option<String>,
}

impl CyclesTab {
    pub fn new(core: Arc<Scout4All>, team_id: String) -> CyclesTab {
        let mut tab = CyclesTab {
            core,
            team_id,
            level: Level::Microcycle,
            cycles: vec![],
            selected: 0,
            marked: HashSet::new(),
            card: None,
            mode: Mode::List,
            notice: None,
            error: None,
        };
        tab.reload();
        tab
    }

    #[cfg(test)]
    pub fn card_totals(&self) -> Option<(&TotalsRecord, usize)> {
        self.card.as_ref().map(|c| (&c.totals, c.children.len()))
    }

    #[cfg(test)]
    pub fn form_error(&self) -> Option<String> {
        match &self.mode {
            Mode::Editor(e) => e.form.error.clone(),
            _ => None,
        }
    }

    /// True when the tab is on its list, to let the plan take its keys.
    pub fn on_list(&self) -> bool {
        matches!(self.mode, Mode::List)
    }

    pub fn focus_id(&self) -> Option<Id> {
        match &self.mode {
            Mode::Editor(e) => Some(e.form.focus_id()),
            Mode::Overlay(o) => Some(o.form.focus_id()),
            Mode::List | Mode::Card => None,
        }
    }

    pub fn scroll(&self) -> Option<(Id, f32)> {
        match self.mode {
            Mode::List => {
                let last = self.cycles.len().saturating_sub(1).max(1);
                Some((Id::new(LIST), self.selected as f32 / last as f32))
            }
            _ => None,
        }
    }

    pub fn reload(&mut self) {
        match self.core.cycles(self.team_id.clone(), self.level) {
            Ok(cycles) => {
                self.cycles = cycles;
                self.selected = self.selected.min(self.cycles.len().saturating_sub(1));
                self.marked
                    .retain(|id| self.cycles.iter().any(|c| c.id == *id));
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn set_level(&mut self, level: Level) {
        self.level = level;
        self.selected = 0;
        self.marked.clear();
        self.reload();
    }

    /// A form for a new cycle of `level` made from the free `children`
    /// (sessions for a microcycle).
    pub fn create_from(&mut self, s: &Strings, level: Level, children: Vec<String>) {
        let seed = match level {
            Level::Microcycle => self.core.proposed_microcycle(children.clone()),
            _ => self.core.proposed_container(level, children.clone()),
        };
        match seed {
            Ok(seed) => self.set_editor(s, level, None, seed, children),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn set_editor(
        &mut self,
        s: &Strings,
        level: Level,
        id: Option<String>,
        seed: Option<CycleDraft>,
        children: Vec<String>,
    ) {
        match CycleEditor::new(
            &self.core,
            s,
            self.team_id.clone(),
            level,
            id,
            seed,
            children,
        ) {
            Ok(editor) => {
                self.level = level;
                self.mode = Mode::Editor(Box::new(editor));
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// The sheet of the cycle, at its level.
    pub fn open(&mut self, level: Level, id: &str) {
        if level != self.level {
            self.set_level(level);
        }
        self.reload();
        match self.build_card(level, id) {
            Ok(card) => {
                if let Some(at) = self.cycles.iter().position(|c| c.id == id) {
                    self.selected = at;
                }
                self.card = Some(card);
                self.mode = Mode::Card;
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn build_card(&self, level: Level, id: &str) -> Result<Card, MobileError> {
        let record = self.core.cycle(level, id.to_string())?;
        let totals = self.core.training_totals(
            self.team_id.clone(),
            TrainingScope::Cycle {
                level,
                id: id.to_string(),
            },
        )?;
        let children = self
            .core
            .cycle_children(level, id.to_string())?
            .into_iter()
            .map(|c| {
                let label = match level.child() {
                    Some(down) => self
                        .core
                        .cycle(down, c.id.clone())
                        .map(|k| cycle_label(&k))
                        .unwrap_or(c.window),
                    None => self
                        .core
                        .training_session(c.id.clone())
                        .map(|k| format!("{} · {}", k.start.replace('T', " "), k.title))
                        .unwrap_or(c.window),
                };
                Child { id: c.id, label }
            })
            .collect();
        Ok(Card {
            record,
            totals,
            children,
            selected: 0,
        })
    }

    /// The sheet read again after a change; the list too.
    fn refresh(&mut self) {
        let at = self
            .card
            .as_ref()
            .map(|c| (c.record.level, c.record.id.clone(), c.selected));
        self.reload();
        if let Some((level, id, selected)) = at {
            match self.build_card(level, &id) {
                Ok(mut card) => {
                    card.selected = selected.min(card.children.len().saturating_sub(1));
                    self.card = Some(card);
                }
                Err(_) => {
                    self.card = None;
                    self.mode = Mode::List;
                }
            }
        }
    }

    fn selected_id(&self) -> Option<String> {
        self.cycles.get(self.selected).map(|c| c.id.clone())
    }

    fn open_selected(&mut self) {
        if let Some(id) = self.selected_id() {
            self.open(self.level, &id);
        }
    }

    fn close_forms(&mut self) {
        self.mode = match self.card {
            Some(_) => Mode::Card,
            None => Mode::List,
        };
    }

    fn toggle_mark(&mut self) {
        let free = self
            .cycles
            .get(self.selected)
            .filter(|c| c.parent_id.is_none());
        if let Some(c) = free {
            let id = c.id.clone();
            if !self.marked.remove(&id) {
                self.marked.insert(id);
            }
        }
    }

    /// The cycle of the level above made from the ones marked.
    fn create_container(&mut self, s: &Strings) {
        let Some(up) = self.level.parent() else {
            return;
        };
        let ids: Vec<String> = self
            .cycles
            .iter()
            .filter(|c| self.marked.contains(&c.id))
            .map(|c| c.id.clone())
            .collect();
        if ids.is_empty() {
            return;
        }
        self.create_from(s, up, ids);
    }

    fn new_cycle(&mut self, s: &Strings) {
        let level = self.level;
        self.set_editor(s, level, None, None, vec![]);
    }

    fn edit_card(&mut self, s: &Strings) {
        let target = self
            .card
            .as_ref()
            .map(|c| (c.record.level, c.record.id.clone()));
        if let Some((level, id)) = target {
            self.set_editor(s, level, Some(id), None, vec![]);
        }
    }

    fn start_add(&mut self, s: &Strings) {
        let Some((level, id)) = self
            .card
            .as_ref()
            .map(|c| (c.record.level, c.record.id.clone()))
        else {
            return;
        };
        match self.core.free_children(level, id) {
            Ok(free) if free.is_empty() => self.notice = Some(s.nothing_to_add.to_string()),
            Ok(free) => {
                let fields = free
                    .iter()
                    .map(|c| Field::check(&c.window, false, false))
                    .collect();
                self.mode = Mode::Overlay(Box::new(Overlay {
                    form: Form::new(s.add_children, fields),
                    kind: Kind::Add(free.into_iter().map(|c| c.id).collect()),
                    warned: false,
                }));
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn start_duplicate(&mut self, s: &Strings) {
        if self
            .card
            .as_ref()
            .is_none_or(|c| c.record.level != Level::Microcycle)
        {
            return;
        }
        let fields = vec![
            Field::text(s.shift_weeks, "1"),
            Field::text(s.copies, "1"),
            Field::check(s.copy_objectives, true, false),
            Field::check(s.copy_activities, true, false),
        ];
        self.mode = Mode::Overlay(Box::new(Overlay {
            form: Form::new(s.duplicate_microcycle, fields),
            kind: Kind::Duplicate,
            warned: false,
        }));
    }

    fn start_remove(&mut self, s: &Strings) {
        if self.card.is_none() && self.selected_id().is_none() {
            return;
        }
        let fields = vec![Field::check(s.remove_with_children, false, false)];
        self.mode = Mode::Overlay(Box::new(Overlay {
            form: Form::new(s.delete_forever, fields),
            kind: Kind::Remove,
            warned: false,
        }));
    }

    /// The cycle in front: the sheet's, or the one chosen in the list.
    fn target(&self) -> Option<(Level, String)> {
        match &self.card {
            Some(card) => Some((card.record.level, card.record.id.clone())),
            None => self.selected_id().map(|id| (self.level, id)),
        }
    }

    fn overlay_done(&mut self, s: &Strings, outcome: Outcome) {
        match outcome {
            Outcome::Cancel => self.close_forms(),
            Outcome::Save => self.overlay_save(s),
            Outcome::Stay => {}
            Outcome::Changed(_) => {
                if let Mode::Overlay(overlay) = &mut self.mode {
                    overlay.warned = false;
                }
            }
        }
    }

    fn overlay_save(&mut self, s: &Strings) {
        let Some((level, id)) = self.target() else {
            self.close_forms();
            return;
        };
        let had_parent = self
            .card
            .as_ref()
            .is_some_and(|c| c.record.parent_id.is_some());
        let Mode::Overlay(overlay) = &mut self.mode else {
            return;
        };
        let done: Result<Option<String>, MobileError> = match &overlay.kind {
            Kind::Add(ids) => {
                let chosen: Vec<String> = ids
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| checked(&overlay.form, *i))
                    .map(|(_, id)| id.clone())
                    .collect();
                self.core
                    .add_children(level, id.clone(), chosen)
                    .map(|_| None)
            }
            Kind::Duplicate => {
                let number = |field: usize| overlay.form.text(field).trim().parse().unwrap_or(0);
                let draft = DuplicateCycleDraft {
                    shift_weeks: number(0),
                    copies: number(1),
                    copy_objectives: checked(&overlay.form, 2),
                    copy_activities: checked(&overlay.form, 3),
                };
                if !overlay.warned {
                    let warning = clash_warning(
                        s.save_anyway_hint,
                        self.core.microcycle_venue_clashes(id.clone(), draft.clone()),
                    );
                    overlay.warned = warning.is_some();
                    if warning.is_some() {
                        overlay.form.error = warning;
                        return;
                    }
                }
                self.core
                    .duplicate_microcycle(id.clone(), draft)
                    .map(|copies| {
                        let free = copies.iter().filter(|c| c.parent_id.is_none()).count();
                        (free > 0 && had_parent)
                            .then(|| ui::fill(s.copies_free, &[free.to_string()]))
                    })
            }
            Kind::Remove => self
                .core
                .remove_cycle(level, id.clone(), checked(&overlay.form, 0))
                .map(|_| None),
        };
        match done {
            Ok(notice) => {
                let removed = matches!(overlay.kind, Kind::Remove);
                self.notice = notice;
                match removed {
                    true => {
                        self.card = None;
                        self.mode = Mode::List;
                        self.reload();
                    }
                    false => {
                        self.mode = Mode::Card;
                        self.refresh();
                    }
                }
            }
            Err(e) => overlay.form.error = Some(e.to_string()),
        }
    }

    fn edited(&mut self, edited: Edited) {
        match edited {
            Edited::Stay | Edited::Copy(_) => {}
            Edited::Cancel => self.close_forms(),
            Edited::Saved(id) => {
                let level = match &self.mode {
                    Mode::Editor(e) => e.level,
                    _ => self.level,
                };
                self.open(level, &id);
            }
        }
    }

    fn open_child(&mut self, index: usize) -> Leave {
        let Some(card) = &self.card else {
            return Leave::Stay;
        };
        let Some(child) = card.children.get(index) else {
            return Leave::Stay;
        };
        let id = child.id.clone();
        match card.record.level.child() {
            Some(down) => {
                self.open(down, &id);
                Leave::Stay
            }
            None => Leave::Session(id),
        }
    }

    pub fn update(&mut self, s: &Strings, message: Message) -> Leave {
        self.notice = None;
        match (&mut self.mode, message) {
            (Mode::Editor(editor), Message::Form(m)) => {
                let edited = editor.update(&self.core, s, m);
                self.edited(edited);
            }
            (Mode::Overlay(overlay), Message::Form(m)) => {
                let outcome = overlay.form.update(m);
                self.overlay_done(s, outcome);
            }
            (Mode::List, Message::Level(level)) => self.set_level(level),
            (Mode::List, Message::Row(i)) => {
                self.selected = i;
                self.open_selected();
            }
            (Mode::List, Message::New) => self.new_cycle(s),
            (Mode::List, Message::Create) => self.create_container(s),
            (Mode::List, Message::Delete) => self.start_remove(s),
            (Mode::Card, Message::Edit) => self.edit_card(s),
            (Mode::Card, Message::Add) => self.start_add(s),
            (Mode::Card, Message::Duplicate) => self.start_duplicate(s),
            (Mode::Card, Message::Pdf) => return self.plan_pdf(),
            (Mode::Card, Message::Delete) => self.start_remove(s),
            (Mode::Card, Message::Child(i)) => return self.open_child(i),
            _ => {}
        }
        Leave::Stay
    }

    pub fn key(&mut self, s: &Strings, ck: KeyCombination) -> Leave {
        self.notice = None;
        match &mut self.mode {
            Mode::Editor(editor) => {
                let edited = editor.key(&self.core, s, ck);
                self.edited(edited);
            }
            Mode::Overlay(overlay) => {
                let outcome = overlay.form.key(ck);
                self.overlay_done(s, outcome);
            }
            Mode::Card => return self.card_key(s, ck),
            Mode::List => return self.list_key(s, ck),
        }
        Leave::Stay
    }

    fn card_key(&mut self, s: &Strings, ck: KeyCombination) -> Leave {
        let count = self.card.as_ref().map_or(0, |c| c.children.len());
        match ck {
            k if k == BACK_KEY => {
                self.card = None;
                self.mode = Mode::List;
                self.reload();
            }
            k if k == key!(down) && count > 0 => {
                if let Some(card) = &mut self.card {
                    card.selected = (card.selected + 1) % count;
                }
            }
            k if k == key!(up) && count > 0 => {
                if let Some(card) = &mut self.card {
                    card.selected = card.selected.checked_sub(1).unwrap_or(count - 1);
                }
            }
            k if k == key!(enter) => {
                let at = self.card.as_ref().map_or(0, |c| c.selected);
                return self.open_child(at);
            }
            k if k == key!(e) => self.edit_card(s),
            k if k == key!(a) => self.start_add(s),
            k if k == key!(d) => self.start_duplicate(s),
            k if k == key!(p) => return self.plan_pdf(),
            k if k == key!(backspace) || k == key!(delete) => self.start_remove(s),
            _ => {}
        }
        Leave::Stay
    }

    /// The PDF of the plan of the card, when it is a microcycle's.
    fn plan_pdf(&self) -> Leave {
        match &self.card {
            Some(card) if card.record.level == Level::Microcycle => {
                Leave::Export(crate::transfer::Export::Microcycle(card.record.id.clone()))
            }
            _ => Leave::Stay,
        }
    }

    fn list_key(&mut self, s: &Strings, ck: KeyCombination) -> Leave {
        let count = self.cycles.len();
        match ck {
            k if k == BACK_KEY => return Leave::Back,
            k if k == key!(down) && count > 0 => self.selected = (self.selected + 1) % count,
            k if k == key!(up) && count > 0 => {
                self.selected = self.selected.checked_sub(1).unwrap_or(count - 1)
            }
            k if k == key!(right) => self.step_level(true),
            k if k == key!(left) => self.step_level(false),
            k if k == key!(enter) => self.open_selected(),
            k if k == key!(space) => self.toggle_mark(),
            k if k == key!(m) => self.create_container(s),
            k if k == key!(n) => self.new_cycle(s),
            k if (k == key!(backspace) || k == key!(delete)) && count > 0 => self.start_remove(s),
            _ => {}
        }
        Leave::Stay
    }

    fn step_level(&mut self, forward: bool) {
        if let Some(next) = ui::cycle(&LEVELS, &self.level, forward) {
            self.set_level(next);
        }
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match &self.mode {
            Mode::Editor(_) | Mode::Overlay(_) => Form::hints(s),
            Mode::Card => {
                let mut hints = vec![
                    ("↑ ↓".to_string(), String::new()),
                    ("Enter".to_string(), s.open.to_string()),
                    ("e".to_string(), s.edit_cycle.to_string()),
                    ("a".to_string(), s.add_children.to_string()),
                ];
                if self
                    .card
                    .as_ref()
                    .is_some_and(|c| c.record.level == Level::Microcycle)
                {
                    hints.push(("d".to_string(), s.duplicate_microcycle.to_string()));
                    hints.push(("p".to_string(), s.plan_pdf.to_string()));
                }
                hints.push(("⌫".to_string(), s.delete_forever.to_string()));
                hints.push(("Esc".to_string(), s.back.to_string()));
                hints
            }
            Mode::List => {
                let mut hints = vec![
                    ("↑ ↓".to_string(), String::new()),
                    ("Enter".to_string(), s.open.to_string()),
                    ("← →".to_string(), s.cycles.to_string()),
                    ("n".to_string(), s.new_cycle.to_string()),
                ];
                if let Some(up) = self.level.parent() {
                    hints.push(("space".to_string(), s.select_row.to_string()));
                    hints.push(("m".to_string(), create_name(s, up).to_string()));
                }
                hints.push(("⌫".to_string(), s.delete_forever.to_string()));
                hints.push(("Esc".to_string(), s.back.to_string()));
                hints
            }
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let notice: Element<'a, Message> = match (&self.error, &self.notice) {
            (Some(e), _) => ui::body(e.clone()).color(c.danger).into(),
            (None, Some(n)) => ui::body(n.clone()).color(c.accent).into(),
            (None, None) => space::vertical().height(0).into(),
        };
        let centered = |form: Element<'a, Message>| -> Element<'a, Message> {
            container(container(form).max_width(720))
                .center_x(Length::Fill)
                .height(Length::Fill)
                .into()
        };
        let body: Element<'a, Message> = match &self.mode {
            Mode::Editor(e) => centered(e.form.view(s, c, Message::Form, None)),
            Mode::Overlay(o) => centered(o.form.view(s, c, Message::Form, None)),
            Mode::Card => self.card_view(s, c),
            Mode::List => self.list(s, c),
        };
        column![notice, body].spacing(10).into()
    }

    fn list<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let levels = row(LEVELS.iter().map(|l| {
            ui::pill(
                level_plural(s, *l),
                *l == self.level,
                Some(Message::Level(*l)),
                c,
            )
        }))
        .spacing(6);
        let rows: Vec<Element<'a, Message>> = self
            .cycles
            .iter()
            .enumerate()
            .map(|(i, cycle)| self.cycle_row(cycle, i, c))
            .collect();
        let list: Element<'a, Message> = match rows.is_empty() {
            true => text(s.no_cycles).size(SIZE_SMALL).color(c.muted).into(),
            false => scrollable(Column::with_children(rows).spacing(3))
                .id(Id::new(LIST))
                .height(Length::Fill)
                .into(),
        };
        let mut actions: Vec<Element<'a, Message>> = vec![
            text(ui::fill(s.cycles_of, &[self.cycles.len().to_string()]))
                .size(SIZE_SMALL)
                .color(c.muted)
                .into(),
            ui::pill(s.new_cycle, false, Some(Message::New), c),
        ];
        if let Some(up) = self.level.parent() {
            actions.push(ui::pill(
                create_name(s, up),
                false,
                (!self.marked.is_empty()).then_some(Message::Create),
                c,
            ));
        }
        actions.push(ui::pill(s.delete_forever, false, Some(Message::Delete), c));
        ui::panel_with(s.cycles, actions, column![levels, list].spacing(12), c)
    }

    fn cycle_row<'a>(&self, cycle: &CycleRecord, index: usize, c: Colors) -> Element<'a, Message> {
        let mark = match (self.level.parent(), cycle.parent_id.is_none()) {
            (Some(_), true) => match self.marked.contains(&cycle.id) {
                true => "☑",
                false => "☐",
            },
            _ => " ",
        };
        let parent = cycle
            .parent_id
            .as_ref()
            .and_then(|id| {
                self.core
                    .cycle(self.level.parent()?, id.clone())
                    .ok()
                    .map(|p| cycle_label(&p))
            })
            .unwrap_or_default();
        let line = row![
            text(mark).size(SIZE_SMALL).color(c.muted),
            text(cycle.window.clone())
                .size(SIZE_SMALL)
                .color(c.muted)
                .width(Length::Fixed(230.0)),
            ui::name(cycle.title.clone()),
            text(parent).size(SIZE_SMALL).color(c.accent),
        ]
        .spacing(14)
        .align_y(Alignment::Center);
        ui::list_row(line, index == self.selected, Some(Message::Row(index)), c)
    }

    fn card_view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let Some(card) = &self.card else {
            return space::vertical().height(0).into();
        };
        let r = &card.record;
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
        let time = |minutes: u32| format!("{} h {:02} {}", minutes / 60, minutes % 60, s.min_short);
        let t = &card.totals;
        let counts = format!(
            "{} {} · {} {} · {} {}",
            t.planned, s.status_planned, t.completed, s.status_completed, t.cancelled, s.cancelled
        );
        let parent = r
            .parent_id
            .as_ref()
            .zip(r.level.parent())
            .and_then(|(id, up)| self.core.cycle(up, id.clone()).ok())
            .map(|p| cycle_label(&p))
            .unwrap_or_default();
        let mut actions: Vec<Element<'a, Message>> = vec![
            ui::action(theme::icons::EDIT, s.edit_cycle, Message::Edit, false, c)
                .width(Length::Fill)
                .into(),
            ui::action(theme::icons::ADD, s.add_children, Message::Add, false, c)
                .width(Length::Fill)
                .into(),
        ];
        if r.level == Level::Microcycle {
            actions.push(
                ui::action(
                    theme::icons::ADD,
                    s.duplicate_microcycle,
                    Message::Duplicate,
                    false,
                    c,
                )
                .width(Length::Fill)
                .into(),
            );
        }
        if r.level == Level::Microcycle {
            actions.push(
                ui::action(theme::icons::SAVE, s.plan_pdf, Message::Pdf, false, c)
                    .width(Length::Fill)
                    .into(),
            );
        }
        actions.push(
            ui::action(
                theme::icons::DELETE,
                s.delete_forever,
                Message::Delete,
                false,
                c,
            )
            .width(Length::Fill)
            .into(),
        );
        let info = ui::panel(
            Some(level_name(s, r.level)),
            column![
                line(s.session_title, r.title.clone()),
                line(s.cycle_start, r.window.clone()),
                line(s.cycle_parent, parent),
                line(s.objectives, r.objectives.clone()),
                line(s.notes, r.notes.clone()),
                line(s.cycle_totals, counts),
                line(s.planned_time, time(t.planned_minutes)),
                line(s.actual_time, time(t.actual_minutes)),
                Column::with_children(actions).spacing(8),
            ]
            .spacing(10),
            c,
        );
        let rows: Vec<Element<'a, Message>> = card
            .children
            .iter()
            .enumerate()
            .map(|(i, child)| {
                ui::list_row(
                    ui::body(child.label.clone()),
                    i == card.selected,
                    Some(Message::Child(i)),
                    c,
                )
            })
            .collect();
        let children: Element<'a, Message> = match rows.is_empty() {
            true => text(match r.level {
                Level::Microcycle => s.no_sessions,
                _ => s.no_cycles,
            })
            .size(SIZE_SMALL)
            .color(c.muted)
            .into(),
            false => scrollable(Column::with_children(rows).spacing(3))
                .height(Length::Fill)
                .into(),
        };
        row![
            container(info).width(Length::FillPortion(3)),
            container(ui::panel(Some(s.cycle_contains), children, c))
                .width(Length::FillPortion(4))
                .height(Length::Fill),
        ]
        .spacing(14)
        .height(Length::Fill)
        .into()
    }
}
