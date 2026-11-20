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

/// The event being corrected and the side before it: what correcting an
/// event of A alone (`EventEdit`) and a touch of both sides
/// (`touch_edit::TouchEdit`) share.
// public only to be named by `Correcting`: its fields and methods are the
// crate's
#[derive(Debug, Clone)]
pub struct Draft {
    /// the side as it was before the event, its own point of view
    pub(crate) before: Snapshot,
    /// the event as corrected so far
    pub(crate) event: EventEntry,
}

impl Draft {
    /// What of the event can change: the evaluation for the events having
    /// one, the set or serve for the attacks and the serves.
    pub(crate) fn fields(&self) -> Vec<EditField> {
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

    /// The types that could come at that point, played by the same player
    /// (or by none, as the event), but the substitutions and the libero and
    /// setter changes.
    pub(crate) fn event_types(&self) -> impl Iterator<Item = EventTypeEnum> + '_ {
        let event = &self.event;
        self.before
            .get_available_options()
            .iter()
            .copied()
            .filter(|t| !t.changes_lineup())
            .filter(|t| t.asks_player() == event.event_type.asks_player())
            .filter(|t| match event.player {
                Some(p) => self.before.can_play(*t, &p),
                // the team's fault: no event of a player
                None => !t.requires_player(),
            })
    }

    /// The evaluations of `event_type` the side offers at that point; only
    /// none for a type having none.
    pub(crate) fn evals_of(&self, event_type: EventTypeEnum) -> Vec<Option<EvalEnum>> {
        match event_type.requires_evaluation() {
            true => self
                .before
                .available_evals(event_type)
                .into_iter()
                .map(Some)
                .collect(),
            false => vec![None],
        }
    }

    /// The evaluation among `evals` for another type: the one it has when
    /// there, else the first.
    pub(crate) fn kept_or_first(&self, evals: &[Option<EvalEnum>]) -> Option<EvalEnum> {
        match evals.contains(&self.event.eval) {
            true => self.event.eval,
            false => evals.first().copied().flatten(),
        }
    }

    /// The sets the attacker can receive, the serves, and no technique, as
    /// the events saved before it was scouted.
    pub(crate) fn techniques(&self) -> Vec<Option<TechniqueEnum>> {
        self.techniques_of(self.event.event_type)
    }

    fn techniques_of(&self, event_type: EventTypeEnum) -> Vec<Option<TechniqueEnum>> {
        let techniques: Vec<TechniqueEnum> = match (event_type, self.event.player) {
            (EventTypeEnum::A, Some(attacker)) => self
                .before
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

    /// The event as `event_type` with `eval`: the technique kept when the
    /// type allows it; where a serve went and started from, if it stays a
    /// serve.
    pub(crate) fn retyped(&self, event_type: EventTypeEnum, eval: Option<EvalEnum>) -> EventEntry {
        let techniques = self.techniques_of(event_type);
        let serving = event_type == EventTypeEnum::S;
        let mut event = self.event.clone();
        event.event_type = event_type;
        // our freeball named its player once: not another event
        event.player = event.player.filter(|_| event_type.asks_player());
        event.eval = eval;
        event.technique = event.technique.filter(|t| techniques.contains(&Some(*t)));
        event.flags = event
            .flags
            .with_target(event.flags.target().filter(|_| serving))
            .with_serve_start(event.flags.serve_start().filter(|_| serving));
        event
    }

    /// Another evaluation, among `allowed`.
    pub(crate) fn choose_eval(&mut self, eval: EvalEnum, allowed: &[EvalEnum]) {
        if allowed.contains(&eval) {
            self.event.eval = Some(eval);
        }
    }

    /// Another set or serve, or none, among `techniques`.
    pub(crate) fn choose_technique(&mut self, technique: Option<TechniqueEnum>) {
        if self.techniques().contains(&technique) {
            self.event.technique = technique;
        }
    }
}

/// What correcting an event of A alone (`EventEdit`) and a touch of both
/// sides (`touch_edit::TouchEdit`) offer the same way: each tells its draft,
/// the types the event can become and the evaluations of a type; the rest
/// follows from those.
pub trait Correcting {
    fn draft(&self) -> &Draft;

    fn draft_mut(&mut self) -> &mut Draft;

    fn index(&self) -> usize;

    /// The types the event can become.
    fn event_types(&self) -> Vec<EventTypeEnum>;

    /// The evaluations of `event_type` the event can take at that point:
    /// only none for a type having none.
    fn evals_of(&self, event_type: EventTypeEnum) -> Vec<Option<EvalEnum>>;

    /// The event as corrected so far.
    fn event(&self) -> &EventEntry {
        &self.draft().event
    }

    /// Its side before the event: the score, the rotation, the phase as
    /// that side sees them.
    fn before(&self) -> &Snapshot {
        &self.draft().before
    }

    /// What of the event can change (`Draft::fields`).
    fn fields(&self) -> Vec<EditField> {
        self.draft().fields()
    }

    /// The evaluations of its type, none for an event having none.
    fn evals(&self) -> Vec<EvalEnum> {
        self.evals_of(self.event().event_type)
            .into_iter()
            .flatten()
            .collect()
    }

    /// The sets and the serves, and none (`Draft::techniques`).
    fn techniques(&self) -> Vec<Option<TechniqueEnum>> {
        self.draft().techniques()
    }

    /// Another type, among `event_types`: it keeps the evaluation when the
    /// type takes it, else the first it takes (`Draft::retyped`).
    fn choose_event_type(&mut self, event_type: EventTypeEnum) {
        let event = self.event_types().contains(&event_type).then(|| {
            let eval = self.draft().kept_or_first(&self.evals_of(event_type));
            self.draft().retyped(event_type, eval)
        });
        if let Some(event) = event {
            self.draft_mut().event = event;
        }
    }

    /// Another evaluation, among `evals`.
    fn choose_eval(&mut self, eval: EvalEnum) {
        let allowed = self.evals();
        self.draft_mut().choose_eval(eval, &allowed);
    }

    /// Another set or serve, or none, among `techniques`.
    fn choose_technique(&mut self, technique: Option<TechniqueEnum>) {
        self.draft_mut().choose_technique(technique);
    }
}

/// An event of a set being corrected.
#[derive(Debug, Clone)]
pub struct EventEdit {
    set: SetEntry,
    index: usize,
    max_substitutions: u8,
    draft: Draft,
}

impl EventEdit {
    /// Starts correcting the event at `index` of the set.
    pub fn new(
        set: &SetEntry,
        index: usize,
        max_substitutions: u8,
    ) -> Result<Self, EventEditError> {
        let event = set.events.get(index).ok_or(EventEditError::Unreadable)?;
        match event.event_type.changes_lineup() {
            true => Err(EventEditError::NotEditable),
            false => set
                .snapshot_before(index, max_substitutions)
                .map(|before| EventEdit {
                    set: set.clone(),
                    index,
                    max_substitutions,
                    draft: Draft {
                        before,
                        event: event.clone(),
                    },
                })
                .map_err(|_| EventEditError::Unreadable),
        }
    }

    /// The set with the event corrected, to save: only if the events after
    /// it still fit and the set ends as it did.
    pub fn apply(&self) -> Result<SetEntry, EventEditError> {
        self.set
            .with_event(self.index, self.draft.event.clone(), self.max_substitutions)
            .map_err(|e| match e {
                AppError::Match(MatchError::SetOutcomeChanged(_)) => {
                    EventEditError::ChangesTheResult
                }
                _ => EventEditError::DoesNotFit,
            })
    }
}

impl Correcting for EventEdit {
    fn draft(&self) -> &Draft {
        &self.draft
    }

    fn draft_mut(&mut self) -> &mut Draft {
        &mut self.draft
    }

    fn index(&self) -> usize {
        self.index
    }

    /// The events it can become (`Draft::event_types`).
    fn event_types(&self) -> Vec<EventTypeEnum> {
        self.draft.event_types().collect()
    }

    /// The evaluations of the type after the events before it.
    fn evals_of(&self, event_type: EventTypeEnum) -> Vec<Option<EvalEnum>> {
        self.draft.evals_of(event_type)
    }
}
