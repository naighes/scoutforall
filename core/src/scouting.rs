//! Scouting a set, whatever the user interface: the scout chooses the
//! event, then the player, then the evaluation (with the set or serve) or
//! the replacement; or the player first (see [`ChoiceOrder`]). The event is
//! saved once complete, and only when it applies: otherwise the set
//! wouldn't replay anymore. Undo goes back a choice, or removes the last
//! saved event and resumes it.

use crate::{
    analytics::finished::{FinishedMatches, GlobalFinishedMatches},
    localization::Labels,
    providers::set_writer::SetWriter,
    shapes::{
        enums::{EvalEnum, EventTypeEnum},
        r#match::MatchEntry,
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        stats::Stats,
        technique::{ServeTypeEnum, TechniqueEnum},
    },
};
use chrono::Utc;
use std::{fmt, sync::Arc};
use uuid::Uuid;

/// What the scout chooses now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoutingStep {
    Event,
    Player,
    Eval,
    Replacement,
}

/// A player to choose: on court for the event, or a replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerChoice {
    /// the place in the list, e.g. the key the TUI binds to the player
    pub index: u8,
    pub id: Uuid,
    pub name: String,
    pub number: u8,
    pub role: String,
}

/// The order of the choices of the scout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ChoiceOrder {
    /// the event, then the player: the TUI, with a key for each event
    #[default]
    EventFirst,
    /// the player who touches the ball, then the event only if the player
    /// can play more than one (`player_events`); our serve goes straight to
    /// its evaluation. The other events (`other_events`: theirs, the
    /// faults, the changes of the lineup) still come before their player.
    PlayerFirst,
}

/// The events a player plays touching the ball, but the serve: with the
/// player first, they're chosen after the player.
fn played_by_the_player(event: EventTypeEnum) -> bool {
    use EventTypeEnum::*;
    matches!(event, P | A | D | B | FB)
}

/// How a choice went on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// more to choose
    Choosing,
    /// the event is saved, the next one starts
    Saved,
    /// the event is saved and wins the set
    SetOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoutingError {
    EventNotAvailable(EventTypeEnum),
    MaxSubstitutionsReached,
    NoPlayerSelected,
    InvalidPlayerSelection,
    EvalNotAllowed(EventTypeEnum),
    /// a choice that doesn't fit the current step
    InvalidSelection,
    CouldNotAddEvent,
    CouldNotUndo,
    CouldNotComputeSnapshot,
}

impl ScoutingError {
    pub fn message(&self, labels: &Labels) -> String {
        use ScoutingError::*;
        match self {
            EventNotAvailable(event) => labels
                .event_is_not_available
                .replace("{}", &event.to_string()),
            MaxSubstitutionsReached => labels.max_substitutions_reached.to_string(),
            NoPlayerSelected => labels.no_player_selected.to_string(),
            InvalidPlayerSelection => labels.invalid_player_selection.to_string(),
            EvalNotAllowed(event) => labels
                .evaluation_not_allowed_for_event
                .replace("{}", &event.to_string()),
            InvalidSelection => labels.invalid_selection.to_string(),
            CouldNotAddEvent => labels.could_not_add_event.to_string(),
            CouldNotUndo => labels.could_not_undo.to_string(),
            CouldNotComputeSnapshot => labels.could_not_compute_snapshot.to_string(),
        }
    }
}

pub struct ScoutingSession {
    current_match: MatchEntry,
    set: SetEntry,
    snapshot: Snapshot,
    /// the statistics of the sets before this one
    earlier_stats: Stats,
    step: ScoutingStep,
    event: Option<EventTypeEnum>,
    player: Option<Uuid>,
    /// the set or serve chosen along with the evaluation
    technique: Option<TechniqueEnum>,
    order: ChoiceOrder,
    set_writer: Arc<dyn SetWriter + Send + Sync>,
    finished: Arc<dyn FinishedMatches>,
}

impl fmt::Debug for ScoutingSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScoutingSession")
            .field("match", &self.current_match.id)
            .field("set", &self.set.set_number)
            .field("step", &self.step)
            .field("event", &self.event)
            .field("player", &self.player)
            .field("technique", &self.technique)
            .field("order", &self.order)
            .finish_non_exhaustive()
    }
}

impl ScoutingSession {
    pub fn new(
        current_match: MatchEntry,
        set: SetEntry,
        snapshot: Snapshot,
        set_writer: Arc<dyn SetWriter + Send + Sync>,
    ) -> Self {
        let earlier_stats = current_match
            .sets
            .iter()
            .filter(|s| s.set_number < set.set_number)
            .filter_map(|s| s.compute_snapshot(current_match.max_substitutions).ok())
            .fold(Stats::new(), |mut stats, s| {
                stats.merge(&s.stats);
                stats
            });
        Self {
            current_match,
            set,
            snapshot,
            earlier_stats,
            step: ScoutingStep::Event,
            event: None,
            player: None,
            technique: None,
            order: ChoiceOrder::EventFirst,
            set_writer,
            finished: Arc::new(GlobalFinishedMatches),
        }
    }

    /// The session sending the finished match to `finished`, not to the
    /// global queue.
    pub fn with_finished(mut self, finished: Arc<dyn FinishedMatches>) -> Self {
        self.finished = finished;
        self
    }

    /// The session with the choices in `order`.
    pub fn with_order(mut self, order: ChoiceOrder) -> Self {
        self.order = order;
        self.start_next();
        self
    }

    pub fn order(&self) -> ChoiceOrder {
        self.order
    }

    pub fn current_match(&self) -> &MatchEntry {
        &self.current_match
    }

    pub fn set(&self) -> &SetEntry {
        &self.set
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    /// The statistics of the match up to now: the sets before, and this
    /// one so far.
    pub fn match_stats(&self) -> Stats {
        let mut stats = self.earlier_stats.clone();
        stats.merge(&self.snapshot.stats);
        stats
    }

    /// The sets of the match up to this one, in order: the ones before, and
    /// this one as scouted so far.
    pub fn sets(&self) -> Vec<&SetEntry> {
        let mut sets: Vec<&SetEntry> = self
            .current_match
            .sets
            .iter()
            .filter(|s| s.set_number < self.set.set_number)
            .collect();
        sets.sort_by_key(|s| s.set_number);
        sets.push(&self.set);
        sets
    }

    pub fn step(&self) -> ScoutingStep {
        self.step
    }

    /// The event being chosen, if any.
    pub fn event(&self) -> Option<EventTypeEnum> {
        self.event
    }

    /// The player of the event, once chosen.
    pub fn player(&self) -> Option<Uuid> {
        self.player
    }

    /// The set or serve that the evaluation saves with the event.
    pub fn technique(&self) -> Option<TechniqueEnum> {
        self.technique
    }

    /// The events that can follow.
    pub fn available_events(&self) -> &[EventTypeEnum] {
        self.snapshot.get_available_options()
    }

    /// The players to choose for the event: the ones on court who can play
    /// it, or the ones who can be replaced.
    pub fn player_choices(&self) -> Vec<PlayerChoice> {
        self.choices_for(self.event)
    }

    fn choices_for(&self, event: Option<EventTypeEnum>) -> Vec<PlayerChoice> {
        let lineup = &self.snapshot.current_lineup;
        let choices = match event {
            Some(EventTypeEnum::R) => lineup.get_replaceable_lineup_choices(),
            _ => lineup.get_lineup_choices(),
        };
        choices
            .into_iter()
            .filter_map(|(index, (role, id))| {
                id.filter(|id| match event {
                    Some(EventTypeEnum::R) | None => lineup.find_position(id).is_some(),
                    // only the diagonal: the libero follows the setter, so
                    // anyone else would move it, even to the front row; the
                    // saved sets with another one stay readable
                    Some(EventTypeEnum::CS) => lineup.get_setter_diagonal() == Some(*id),
                    Some(event) => self.snapshot.can_play(event, id),
                })
                .and_then(|id| self.current_match.team.find_player(id))
                .map(|player| PlayerChoice {
                    index,
                    id: player.id,
                    name: player.name.clone(),
                    number: player.number,
                    role,
                })
            })
            .collect()
    }

    /// With the player first, the events the player can play now: none
    /// once an event is chosen, nor while our serve waits for its
    /// evaluation.
    pub fn player_events(&self, player: &Uuid) -> Vec<EventTypeEnum> {
        match (self.step, self.event) {
            (ScoutingStep::Event, None) => self
                .available_events()
                .iter()
                .copied()
                .filter(|e| played_by_the_player(*e))
                .filter(|e| self.choices_for(Some(*e)).iter().any(|c| c.id == *player))
                .collect(),
            _ => vec![],
        }
    }

    /// With the player first, the events that come before their player,
    /// if any: the ones of the opponent, the faults, the changes of the
    /// lineup. Available before a player is chosen, and while our serve
    /// waits for its evaluation.
    pub fn other_events(&self) -> Vec<EventTypeEnum> {
        match (self.step, self.player, self.serve_pending()) {
            (ScoutingStep::Event, None, _) | (_, _, true) => self
                .available_events()
                .iter()
                .copied()
                .filter(|e| *e != EventTypeEnum::S && !played_by_the_player(*e))
                .collect(),
            _ => vec![],
        }
    }

    /// With the player first, the serve chosen by itself: it waits for the
    /// evaluation, or gives way to another event.
    fn serve_pending(&self) -> bool {
        self.order == ChoiceOrder::PlayerFirst
            && self.step == ScoutingStep::Eval
            && self.event == Some(EventTypeEnum::S)
    }

    /// The players who can replace the chosen one.
    pub fn replacement_choices(&self) -> Vec<PlayerChoice> {
        match (self.event, self.player) {
            (Some(EventTypeEnum::R), Some(replaced)) => self
                .snapshot
                .current_lineup
                .get_available_replacements(&self.current_match.team, replaced)
                .into_iter()
                .map(|(index, player)| PlayerChoice {
                    index,
                    id: player.id,
                    name: player.name.clone(),
                    number: player.number,
                    role: player
                        .role
                        .map_or_else(|| "-".to_string(), |r| r.to_string()),
                })
                .collect(),
            _ => vec![],
        }
    }

    pub fn available_evals(&self) -> Vec<EvalEnum> {
        self.event
            .map(|event| event.available_evals())
            .unwrap_or_default()
    }

    /// The sets or serves to choose from along with the evaluation: the
    /// sets the attacker can receive, or the serves.
    pub fn available_techniques(&self) -> Vec<TechniqueEnum> {
        match (self.event, self.player) {
            (Some(EventTypeEnum::A), Some(attacker)) => self
                .snapshot
                .available_set_types(&attacker)
                .into_iter()
                .map(TechniqueEnum::Set)
                .collect(),
            (Some(EventTypeEnum::S), Some(_)) => ServeTypeEnum::ALL
                .into_iter()
                .map(TechniqueEnum::Serve)
                .collect(),
            _ => vec![],
        }
    }

    /// The events of the match up to now: the ones of the sets before, then
    /// the ones of this set.
    fn match_events(&self) -> impl DoubleEndedIterator<Item = &EventEntry> {
        self.current_match
            .sets
            .iter()
            .filter(|s| s.set_number < self.set.set_number)
            .flat_map(|s| s.events.iter())
            .chain(self.set.events.iter())
    }

    /// The set or serve chosen before the scout picks one: the set told by
    /// the attacker and the touch before, the last serve of the server in
    /// the match.
    fn preselected_technique(&self) -> Option<TechniqueEnum> {
        match (self.event, self.player) {
            (Some(EventTypeEnum::A), Some(attacker)) => self
                .snapshot
                .preselected_set_type(&attacker)
                .map(TechniqueEnum::Set),
            (Some(EventTypeEnum::S), Some(server)) => Some(TechniqueEnum::Serve(
                ServeTypeEnum::preselected(&server, self.match_events()),
            )),
            _ => None,
        }
    }

    /// The scout chooses the event: the server is known, the events of the
    /// opponent and the change of libero are saved at once, the others need
    /// the player.
    pub async fn choose_event(&mut self, event: EventTypeEnum) -> Result<Progress, ScoutingError> {
        match (self.serve_pending(), self.step, self.player) {
            // the serve chosen by itself gives way, and comes back if the
            // event isn't possible
            (true, _, _) if event != EventTypeEnum::S => {
                self.reset();
                let chosen = self.choose_event_now(event).await;
                if chosen.is_err() {
                    self.start_next();
                }
                chosen
            }
            // the event of the player chosen first
            (false, ScoutingStep::Event, Some(player))
                if self.player_events(&player).contains(&event) =>
            {
                self.play(event, player).await
            }
            // another event, without the player
            (false, ScoutingStep::Event, Some(_)) => {
                self.player = None;
                self.choose_event_now(event).await
            }
            _ => self.choose_event_now(event).await,
        }
    }

    /// With the player first, the scout chooses the player who touches the
    /// ball, among the players on court with `player_events`: then the
    /// event, if the player can play more than one, or the evaluation, or
    /// the event is saved.
    pub async fn choose_player_first(&mut self, player: Uuid) -> Result<Progress, ScoutingError> {
        match (self.step, self.player_events(&player).as_slice()) {
            (ScoutingStep::Event, [event]) => self.play(*event, player).await,
            (ScoutingStep::Event, [_, _, ..]) => {
                self.player = Some(player);
                Ok(Progress::Choosing)
            }
            _ => Err(ScoutingError::InvalidPlayerSelection),
        }
    }

    /// The event of a player chosen first.
    async fn play(
        &mut self,
        event: EventTypeEnum,
        player: Uuid,
    ) -> Result<Progress, ScoutingError> {
        self.event = Some(event);
        self.step = ScoutingStep::Player;
        self.choose_player(player).await
    }

    async fn choose_event_now(&mut self, event: EventTypeEnum) -> Result<Progress, ScoutingError> {
        use EventTypeEnum::*;
        let available = self.available_events().contains(&event);
        let substitutions_left = !self
            .snapshot
            .current_lineup
            .was_max_number_of_substitutions_reached();
        match (self.step, available, event) {
            (ScoutingStep::Event, true, R) if !substitutions_left => {
                Err(ScoutingError::MaxSubstitutionsReached)
            }
            (ScoutingStep::Event, true, S) => {
                self.event = Some(S);
                self.player = self.snapshot.current_lineup.get_serving_player();
                self.step = ScoutingStep::Eval;
                self.technique = self.preselected_technique();
                Ok(Progress::Choosing)
            }
            (ScoutingStep::Event, true, e) if e.requires_player() => {
                self.event = Some(e);
                self.step = ScoutingStep::Player;
                Ok(Progress::Choosing)
            }
            (ScoutingStep::Event, true, OE | OS | OF | CL) => {
                self.save(EventEntry {
                    timestamp: Utc::now(),
                    event_type: event,
                    eval: None,
                    player: None,
                    target_player: None,
                    technique: None,
                })
                .await
            }
            (ScoutingStep::Event, _, _) => {
                self.event = None;
                Err(ScoutingError::EventNotAvailable(event))
            }
            _ => Err(ScoutingError::InvalidSelection),
        }
    }

    /// The scout chooses the player of the event, among `player_choices`:
    /// then the evaluation, the replacement, or the event is saved.
    pub async fn choose_player(&mut self, player: Uuid) -> Result<Progress, ScoutingError> {
        let valid = self.player_choices().iter().any(|p| p.id == player);
        match (self.step, valid, self.event) {
            (ScoutingStep::Player, true, Some(event)) if event.requires_evaluation() => {
                self.player = Some(player);
                self.step = ScoutingStep::Eval;
                self.technique = self.preselected_technique();
                Ok(Progress::Choosing)
            }
            (ScoutingStep::Player, true, Some(EventTypeEnum::R)) => {
                self.player = Some(player);
                self.step = ScoutingStep::Replacement;
                Ok(Progress::Choosing)
            }
            (ScoutingStep::Player, true, Some(event)) => {
                self.save(EventEntry {
                    timestamp: Utc::now(),
                    event_type: event,
                    eval: None,
                    player: Some(player),
                    target_player: None,
                    technique: None,
                })
                .await
            }
            _ => Err(ScoutingError::InvalidPlayerSelection),
        }
    }

    /// The scout chooses who replaces the player, among
    /// `replacement_choices`: the substitution is saved.
    pub async fn choose_replacement(
        &mut self,
        replacement: Uuid,
    ) -> Result<Progress, ScoutingError> {
        let valid = self
            .replacement_choices()
            .iter()
            .any(|p| p.id == replacement);
        match (self.step, self.event, self.player, valid) {
            (ScoutingStep::Replacement, Some(event), Some(replaced), true) => {
                self.save(EventEntry {
                    timestamp: Utc::now(),
                    event_type: event,
                    eval: None,
                    player: Some(replaced),
                    target_player: Some(replacement),
                    technique: None,
                })
                .await
            }
            (ScoutingStep::Replacement, _, None, _) => Err(ScoutingError::NoPlayerSelected),
            _ => Err(ScoutingError::InvalidPlayerSelection),
        }
    }

    /// The scout changes the set or serve, among `available_techniques`:
    /// the evaluation saves it with the event.
    pub fn choose_technique(&mut self, technique: TechniqueEnum) -> Result<(), ScoutingError> {
        match (self.step, self.available_techniques().contains(&technique)) {
            (ScoutingStep::Eval, true) => {
                self.technique = Some(technique);
                Ok(())
            }
            _ => Err(ScoutingError::InvalidSelection),
        }
    }

    /// The scout evaluates the event, among `available_evals`: it's saved
    /// with the set or serve.
    pub async fn choose_eval(&mut self, eval: EvalEnum) -> Result<Progress, ScoutingError> {
        use EventTypeEnum::*;
        match (self.step, self.event) {
            (ScoutingStep::Eval, Some(event @ (A | B | P | D | S)))
                if event.available_evals().contains(&eval) =>
            {
                self.save(EventEntry {
                    timestamp: Utc::now(),
                    event_type: event,
                    eval: Some(eval),
                    player: self.player,
                    target_player: None,
                    technique: self.technique,
                })
                .await
            }
            (ScoutingStep::Eval, Some(event)) => Err(ScoutingError::EvalNotAllowed(event)),
            _ => Err(ScoutingError::InvalidSelection),
        }
    }

    /// Whether the scout made a choice that `undo` takes back, instead of
    /// removing the last saved event: the serve chosen by itself isn't one.
    pub fn is_halfway(&self) -> bool {
        use ChoiceOrder::*;
        use ScoutingStep::*;
        !matches!(
            (self.order, self.step, self.event, self.player),
            (_, Event, _, None) | (PlayerFirst, Eval, Some(EventTypeEnum::S), _)
        )
    }

    /// Goes back a choice: before the first one, removes the last saved
    /// event and resumes it from its last choice.
    pub async fn undo(&mut self) -> Result<(), ScoutingError> {
        use ChoiceOrder::*;
        use ScoutingStep::*;
        match (self.order, self.step, self.event, self.player) {
            // as `is_halfway` says
            (_, Event, _, None) => self.undo_last_event().await,
            // the serve chosen by itself: nothing to go back to
            (PlayerFirst, Eval, Some(EventTypeEnum::S), _) => self.undo_last_event().await,
            // the player chosen first; the server isn't chosen
            (_, Event, _, Some(_))
            | (_, Player, _, _)
            | (EventFirst, Eval, Some(EventTypeEnum::S), _) => {
                self.start_next();
                Ok(())
            }
            (PlayerFirst, Eval, _, Some(player)) => {
                self.back_to_player(player);
                Ok(())
            }
            (PlayerFirst, Eval, _, None) => {
                self.start_next();
                Ok(())
            }
            (EventFirst, Eval, _, _) => {
                self.player = None;
                self.technique = None;
                self.step = Player;
                Ok(())
            }
            (_, Replacement, _, _) => {
                self.player = None;
                self.step = Player;
                Ok(())
            }
        }
    }

    /// Nothing chosen.
    fn reset(&mut self) {
        self.event = None;
        self.player = None;
        self.technique = None;
        self.step = ScoutingStep::Event;
    }

    /// The next event, from its first choice: with the player first, our
    /// serve goes straight to its evaluation.
    fn start_next(&mut self) {
        self.reset();
        let serving = self.available_events().contains(&EventTypeEnum::S);
        if self.order == ChoiceOrder::PlayerFirst && serving {
            self.event = Some(EventTypeEnum::S);
            self.player = self.snapshot.current_lineup.get_serving_player();
            self.step = ScoutingStep::Eval;
            self.technique = self.preselected_technique();
        }
    }

    /// With the player first, back to the event of the player, or to the
    /// player if there's no event to choose.
    fn back_to_player(&mut self, player: Uuid) {
        self.reset();
        match self.player_events(&player).len() {
            0 | 1 => self.start_next(),
            _ => self.player = Some(player),
        }
    }

    async fn undo_last_event(&mut self) -> Result<(), ScoutingError> {
        use EventTypeEnum::*;
        let removed = match self
            .set_writer
            .remove_last_event(&self.current_match, self.set.set_number)
            .await
        {
            Ok(Some(removed)) => removed,
            // nothing to undo
            Ok(None) => return Ok(()),
            Err(_) => return Err(ScoutingError::CouldNotUndo),
        };
        self.set.events.pop();
        // the set ended by the event is open again
        if let Some(ended) = self
            .current_match
            .sets
            .iter_mut()
            .find(|s| s.set_number == self.set.set_number)
        {
            *ended = self.set.clone();
        }
        // the snapshot replays the events left
        let snapshot = self
            .set
            .compute_snapshot(self.current_match.max_substitutions)
            .map_err(|_| ScoutingError::CouldNotComputeSnapshot)?;
        self.snapshot = snapshot;
        self.reset();
        match (self.order, removed.event_type, removed.player) {
            (_, A | B | D | P | S, Some(player)) => {
                self.event = Some(removed.event_type);
                self.player = Some(player);
                self.step = ScoutingStep::Eval;
                // the set or serve of the removed event, if it was saved
                self.technique = removed.technique.or_else(|| self.preselected_technique());
            }
            (_, R, Some(player)) => {
                self.event = Some(R);
                self.player = Some(player);
                self.step = ScoutingStep::Replacement;
            }
            (ChoiceOrder::PlayerFirst, FB, Some(player)) => self.back_to_player(player),
            (_, _, Some(player)) => {
                self.event = Some(removed.event_type);
                self.player = Some(player);
                self.step = ScoutingStep::Player;
            }
            _ => self.start_next(),
        }
        Ok(())
    }

    /// Saves the event if it applies, then starts the next one.
    pub(crate) async fn save(&mut self, event: EventEntry) -> Result<Progress, ScoutingError> {
        let mut snapshot = self.snapshot.clone();
        snapshot
            .add_event(&event)
            .map_err(|_| ScoutingError::CouldNotAddEvent)?;
        self.set_writer
            .append_event(&self.current_match, self.set.set_number, &event)
            .await
            .map_err(|_| ScoutingError::CouldNotAddEvent)?;
        self.snapshot = snapshot;
        self.set.events.push(event);
        match self.snapshot.get_set_winner(self.set.set_number) {
            None => {
                self.start_next();
                Ok(Progress::Saved)
            }
            Some(_) => {
                self.reset();
                self.end_set().await;
                Ok(Progress::SetOver)
            }
        }
    }

    /// The set joins the match; a finished match is queued for the
    /// statistics, if the user allows them.
    async fn end_set(&mut self) {
        match self
            .current_match
            .sets
            .iter_mut()
            .find(|s| s.set_number == self.set.set_number)
        {
            Some(existing) => *existing = self.set.clone(),
            None => self.current_match.sets.push(self.set.clone()),
        }
        let finished = self
            .current_match
            .get_status()
            .is_ok_and(|status| status.match_finished);
        if finished {
            self.finished
                .finished(self.current_match.team.id, self.current_match.id.clone())
                .await;
        }
    }
}
