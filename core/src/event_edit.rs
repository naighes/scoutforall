//! Correcting an event of a set already scouted: its type, evaluation and
//! set or serve. What it can become at that point of the set, and the set
//! with it, as long as the events after it still fit and the set ends as it
//! did (see `SetEntry::with_event`).
use crate::errors::{AppError, MatchError};
use crate::localization::Labels;
use crate::shapes::enums::{EvalEnum, EventTypeEnum};
use crate::shapes::set::SetEntry;
use crate::shapes::snapshot::{EventEntry, Snapshot};
use crate::shapes::technique::{ServeTypeEnum, TechniqueEnum};

/// What of an event can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    EventType,
    Eval,
    Technique,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventEditError {
    /// a substitution, a libero or setter change: the lineups after it
    /// depend on it
    NotEditable,
    /// the events after it don't fit anymore
    DoesNotFit,
    /// the set would end otherwise
    ChangesTheResult,
    /// no such event, or the set doesn't replay up to it
    Unreadable,
}

impl EventEditError {
    pub fn message(&self, labels: &Labels) -> &'static str {
        match self {
            EventEditError::NotEditable => labels.event_cannot_be_edited,
            EventEditError::DoesNotFit => labels.edit_does_not_fit_the_set,
            EventEditError::ChangesTheResult => labels.edit_changes_the_set_result,
            EventEditError::Unreadable => labels.could_not_compute_snapshot,
        }
    }
}

/// An event of a set being corrected.
#[derive(Debug, Clone)]
pub struct EventEdit {
    set: SetEntry,
    index: usize,
    max_substitutions: u8,
    /// the set as it was before the event
    before: Snapshot,
    event: EventEntry,
}

impl EventEdit {
    /// Starts correcting the event at `index` of the set.
    pub fn new(
        set: &SetEntry,
        index: usize,
        max_substitutions: u8,
    ) -> Result<Self, EventEditError> {
        use EventTypeEnum::*;
        let event = set.events.get(index).ok_or(EventEditError::Unreadable)?;
        match event.event_type {
            R | CL | CS => Err(EventEditError::NotEditable),
            _ => set
                .snapshot_before(index, max_substitutions)
                .map(|before| EventEdit {
                    set: set.clone(),
                    index,
                    max_substitutions,
                    before,
                    event: event.clone(),
                })
                .map_err(|_| EventEditError::Unreadable),
        }
    }

    pub fn index(&self) -> usize {
        self.index
    }

    /// The event as corrected so far.
    pub fn event(&self) -> &EventEntry {
        &self.event
    }

    /// The set before the event: the score, the rotation, the phase.
    pub fn before(&self) -> &Snapshot {
        &self.before
    }

    /// What of the event can change: the evaluation for the events having
    /// one, the set or serve for the attacks and the serves.
    pub fn fields(&self) -> Vec<EditField> {
        [
            Some(EditField::EventType),
            self.event
                .event_type
                .requires_evaluation()
                .then_some(EditField::Eval),
            matches!(self.event.event_type, EventTypeEnum::A | EventTypeEnum::S)
                .then_some(EditField::Technique),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// The events it can become: the ones that could come at that point,
    /// played by the same player (or by none, as the event), but the
    /// substitutions and the libero and setter changes.
    pub fn event_types(&self) -> Vec<EventTypeEnum> {
        use EventTypeEnum::*;
        let event = &self.event;
        self.before
            .get_available_options()
            .iter()
            .copied()
            .filter(|t| !matches!(t, R | CL | CS))
            .filter(|t| t.requires_player() == event.event_type.requires_player())
            .filter(|t| event.player.is_none_or(|p| self.before.can_play(*t, &p)))
            .collect()
    }

    /// The evaluations of its type, none for an event having none.
    pub fn evals(&self) -> Vec<EvalEnum> {
        match self.event.event_type.requires_evaluation() {
            true => self.event.event_type.available_evals(),
            false => vec![],
        }
    }

    /// The sets the attacker can receive, the serves, and no technique, as
    /// the events saved before it was scouted.
    pub fn techniques(&self) -> Vec<Option<TechniqueEnum>> {
        Self::techniques_of(&self.before, self.event.event_type, &self.event)
    }

    fn techniques_of(
        before: &Snapshot,
        event_type: EventTypeEnum,
        event: &EventEntry,
    ) -> Vec<Option<TechniqueEnum>> {
        let techniques: Vec<TechniqueEnum> = match (event_type, event.player) {
            (EventTypeEnum::A, Some(attacker)) => before
                .available_set_types(&attacker)
                .into_iter()
                .map(TechniqueEnum::Set)
                .collect(),
            (EventTypeEnum::S, _) => ServeTypeEnum::ALL
                .into_iter()
                .map(TechniqueEnum::Serve)
                .collect(),
            _ => vec![],
        };
        std::iter::once(None)
            .chain(techniques.into_iter().map(Some))
            .collect()
    }

    /// Another type, among `event_types`: it keeps the evaluation and the
    /// technique when it allows them, else the first evaluation and no
    /// technique.
    pub fn choose_event_type(&mut self, event_type: EventTypeEnum) {
        if !self.event_types().contains(&event_type) {
            return;
        }
        let evals = match event_type.requires_evaluation() {
            true => event_type.available_evals(),
            false => vec![],
        };
        let techniques = Self::techniques_of(&self.before, event_type, &self.event);
        let event = &mut self.event;
        event.event_type = event_type;
        event.eval = event
            .eval
            .filter(|e| evals.contains(e))
            .or_else(|| evals.first().copied());
        event.technique = event.technique.filter(|t| techniques.contains(&Some(*t)));
    }

    /// Another evaluation, among `evals`.
    pub fn choose_eval(&mut self, eval: EvalEnum) {
        if self.evals().contains(&eval) {
            self.event.eval = Some(eval);
        }
    }

    /// Another set or serve, or none, among `techniques`.
    pub fn choose_technique(&mut self, technique: Option<TechniqueEnum>) {
        if self.techniques().contains(&technique) {
            self.event.technique = technique;
        }
    }

    /// The set with the event corrected, to save: only if the events after
    /// it still fit and the set ends as it did.
    pub fn apply(&self) -> Result<SetEntry, EventEditError> {
        self.set
            .with_event(self.index, self.event.clone(), self.max_substitutions)
            .map_err(|e| match e {
                AppError::Match(MatchError::SetOutcomeChanged(_)) => {
                    EventEditError::ChangesTheResult
                }
                _ => EventEditError::DoesNotFit,
            })
    }
}
