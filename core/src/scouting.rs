//! Scouting a set, whatever the user interface: the scout chooses the
//! event, then the player, then the evaluation (with the set or serve) or
//! the replacement; or the player first (see [`ChoiceOrder`]). The event is
//! saved once complete, and only when it applies: otherwise the set
//! wouldn't replay anymore. Undo goes back a choice, or removes the last
//! saved event and resumes it.

use crate::{
    analytics::finished::{FinishedMatches, GlobalFinishedMatches},
    domain::Side,
    localization::Labels,
    providers::set_writer::SetWriter,
    rally::{self, Touches},
    shapes::{
        court_zone::{
            CourtCell, CourtSide, CourtTarget, CourtZoneEnum, ServeStartEnum, TargetKind,
        },
        enums::{EvalEnum, EventTypeEnum, PhaseEnum},
        flags::{BlockersEnum, EventFlags, ReceptionSchemeEnum},
        r#match::MatchEntry,
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        stats::Stats,
        team::TeamEntry,
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
/// player first, they're chosen after the player. Not our freeball, a team
/// event.
fn played_by_the_player(event: EventTypeEnum) -> bool {
    use EventTypeEnum::*;
    matches!(event, P | A | D | B)
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
    /// the reception told while the ball isn't theirs to serve
    ReceptionNotNow,
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
            ReceptionNotNow => labels.reception_not_now.to_string(),
        }
    }
}

/// The other team of a set scouted on both sides: its team, its start and
/// the touches of the set in order.
struct Theirs {
    team: TeamEntry,
    start_a: SetEntry,
    start_b: SetEntry,
    /// the touches of both sides, in order
    touches: Vec<(Side, EventEntry)>,
    snapshot: Snapshot,
    /// the statistics of B in the sets before this one
    earlier_stats: Stats,
    /// the inverted P1 of B, written on every touch of B saved
    inverted_p1: bool,
}

pub struct ScoutingSession {
    current_match: MatchEntry,
    /// the set from A's side
    set: SetEntry,
    /// A's snapshot
    snapshot: Snapshot,
    /// B, when scouting both sides
    theirs: Option<Theirs>,
    /// the side the scout is choosing for: the one the ball is on, unless
    /// the scout swapped it (`swap_side`)
    side: Side,
    /// the statistics of the sets before this one
    earlier_stats: Stats,
    step: ScoutingStep,
    event: Option<EventTypeEnum>,
    player: Option<Uuid>,
    /// the set or serve chosen along with the evaluation
    technique: Option<TechniqueEnum>,
    /// where the ball went, if the scout tells it along with the
    /// evaluation
    target: Option<CourtTarget>,
    /// how many blocked, if the scout tells it along with the evaluation
    blockers: Option<BlockersEnum>,
    /// where our serve starts from, if the scout tells it along with the
    /// evaluation: at first where the server served from last in the match
    serve_start: Option<ServeStartEnum>,
    /// the inverted P1, written on every event saved
    inverted_p1: bool,
    /// how many receive their next serve, written on the reception or on
    /// their ace; three again once the rally is over
    reception: ReceptionSchemeEnum,
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
            .field("target", &self.target)
            .field("blockers", &self.blockers)
            .field("serve_start", &self.serve_start)
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
        let inverted_p1 = last_inverted_p1(&current_match, &set);
        Self {
            current_match,
            set,
            snapshot,
            theirs: None,
            side: Side::A,
            earlier_stats,
            step: ScoutingStep::Event,
            event: None,
            player: None,
            technique: None,
            target: None,
            blockers: None,
            serve_start: None,
            inverted_p1,
            reception: ReceptionSchemeEnum::default(),
            order: ChoiceOrder::EventFirst,
            set_writer,
            finished: Arc::new(GlobalFinishedMatches),
        }
    }

    /// The session scouting both sides: `touches` are the set as saved (the
    /// starts of the sides and the touches so far), `team` the team of B.
    pub fn with_theirs(mut self, team: TeamEntry, touches: Touches) -> Result<Self, ScoutingError> {
        let Touches { a, b, touches } = touches;
        let (snapshot_a, snapshot_b) =
            rally::replay(&a, &b, &touches, self.current_match.max_substitutions)
                .map_err(|_| ScoutingError::CouldNotComputeSnapshot)?;
        self.set.events = rally::project(&touches)
            .a
            .into_iter()
            .map(|p| p.entry)
            .collect();
        self.snapshot = snapshot_a;
        let inverted_p1 = touches
            .iter()
            .rev()
            .find(|(side, _)| *side == Side::B)
            .is_some_and(|(_, e)| e.flags.inverted_p1());
        self.theirs = Some(Theirs {
            team,
            start_a: a,
            start_b: b,
            touches,
            snapshot: snapshot_b,
            earlier_stats: Stats::new(),
            inverted_p1,
        });
        self.start_next();
        Ok(self)
    }

    /// The sets of B before this one, from B's side: their statistics, and
    /// the inverted P1 a new set goes on with, as for A. Nothing scouting
    /// A alone.
    pub fn with_earlier_of_b(mut self, earlier: &[SetEntry]) -> Self {
        let max_substitutions = self.current_match.max_substitutions;
        if let Some(theirs) = &mut self.theirs {
            theirs.earlier_stats = earlier
                .iter()
                .filter(|s| s.set_number < self.set.set_number)
                .filter_map(|s| s.compute_snapshot(max_substitutions).ok())
                .fold(Stats::new(), |mut stats, s| {
                    stats.merge(&s.stats);
                    stats
                });
            let started = theirs.touches.iter().any(|(side, _)| *side == Side::B);
            if !started {
                let b_set = theirs.start_b.clone();
                theirs.inverted_p1 = earlier_inverted_p1(earlier, &b_set);
            }
        }
        self
    }

    /// Whether both sides are scouted.
    pub fn is_both(&self) -> bool {
        self.theirs.is_some()
    }

    /// The side the scout is choosing for now.
    pub fn side(&self) -> Side {
        self.side
    }

    /// The touches of both sides so far, in order; none scouting A alone.
    pub fn touches(&self) -> &[(Side, EventEntry)] {
        self.theirs.as_ref().map_or(&[], |t| &t.touches)
    }

    /// The team of `side`: B's only when both sides are scouted.
    pub fn team_of(&self, side: Side) -> Option<&TeamEntry> {
        match (side, &self.theirs) {
            (Side::A, _) => Some(&self.current_match.team),
            (Side::B, Some(theirs)) => Some(&theirs.team),
            (Side::B, None) => None,
        }
    }

    /// The snapshot of `side`: B's only when both sides are scouted.
    pub fn snapshot_of(&self, side: Side) -> Option<&Snapshot> {
        match (side, &self.theirs) {
            (Side::A, _) => Some(&self.snapshot),
            (Side::B, Some(theirs)) => Some(&theirs.snapshot),
            (Side::B, None) => None,
        }
    }

    /// The snapshot of the side the scout is choosing for.
    fn active(&self) -> &Snapshot {
        self.snapshot_of(self.side).unwrap_or(&self.snapshot)
    }

    /// The team of the side the scout is choosing for.
    fn active_team(&self) -> &TeamEntry {
        self.team_of(self.side).unwrap_or(&self.current_match.team)
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

    /// The snapshot of the side the scout is choosing for (A's when
    /// scouting one side).
    pub fn snapshot(&self) -> &Snapshot {
        self.active()
    }

    /// The statistics of the match up to now: the sets before, and this
    /// one so far.
    pub fn match_stats(&self) -> Stats {
        let mut stats = self.earlier_stats.clone();
        stats.merge(&self.snapshot.stats);
        stats
    }

    /// The same for `side`: B's only when both sides are scouted.
    pub fn match_stats_of(&self, side: Side) -> Option<Stats> {
        match (side, &self.theirs) {
            (Side::A, _) => Some(self.match_stats()),
            (Side::B, Some(theirs)) => {
                let mut stats = theirs.earlier_stats.clone();
                stats.merge(&theirs.snapshot.stats);
                Some(stats)
            }
            (Side::B, None) => None,
        }
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

    /// Whether the P1 is inverted: in side out, the outside hitter attacks
    /// from 4 and the opposite from 2. As the last event of the match says
    /// until the scout changes it.
    pub fn inverted_p1(&self) -> bool {
        self.inverted_p1_of(self.side)
    }

    /// The inverted P1 of `side` (A's when scouting one side).
    pub fn inverted_p1_of(&self, side: Side) -> bool {
        match (side, &self.theirs) {
            (Side::B, Some(theirs)) => theirs.inverted_p1,
            _ => self.inverted_p1,
        }
    }

    /// Whether the inverted P1 makes a difference now: in P1 (the setter in
    /// position 1) in side out only, as the scouting shows it.
    pub fn inverted_p1_applies(&self) -> bool {
        let lineup = &self.active().current_lineup;
        lineup.get_current_phase() == PhaseEnum::SideOut
            && lineup.get_current_rotation().is_ok_and(|r| r == 0)
    }

    /// The scout inverts the P1, or makes it canonical again: the events
    /// saved from now on say so.
    pub fn set_inverted_p1(&mut self, inverted: bool) {
        match (self.side, &mut self.theirs) {
            (Side::B, Some(theirs)) => theirs.inverted_p1 = inverted,
            _ => self.inverted_p1 = inverted,
        }
    }

    /// How many receive their next serve: three unless told otherwise.
    pub fn reception(&self) -> ReceptionSchemeEnum {
        self.reception
    }

    /// Whether the reception can be told: they serve, the rally not yet
    /// started.
    pub fn can_tell_reception(&self) -> bool {
        self.active().current_lineup.get_current_phase() == PhaseEnum::SideOut
            && self.active().get_last_touch().is_none()
    }

    /// The scout tells how many receive their next serve: the next scheme,
    /// three, four, two, then three again.
    pub fn next_reception(&mut self) -> Result<ReceptionSchemeEnum, ScoutingError> {
        use ReceptionSchemeEnum::*;
        match self.can_tell_reception() {
            true => {
                self.reception = match self.reception {
                    Three => Four,
                    Four => Two,
                    Two => Three,
                };
                Ok(self.reception)
            }
            false => Err(ScoutingError::ReceptionNotNow),
        }
    }

    /// The set or serve that the evaluation saves with the event.
    pub fn technique(&self) -> Option<TechniqueEnum> {
        self.technique
    }

    /// Where the ball went, if told.
    pub fn target(&self) -> Option<CourtTarget> {
        self.target
    }

    /// How many blocked, if told.
    pub fn blockers(&self) -> Option<BlockersEnum> {
        self.blockers
    }

    /// The ones the scout can tell now, along with the evaluation: against
    /// our attack and on our block (see `BlockersEnum::of`).
    pub fn available_blockers(&self) -> &'static [BlockersEnum] {
        match (self.step, self.event) {
            (ScoutingStep::Eval, Some(event)) => BlockersEnum::of(event),
            _ => &[],
        }
    }

    /// The scout tells how many blocked, or takes it back (`None`): the
    /// evaluation saves it with the event. Never required.
    pub fn choose_blockers(&mut self, blockers: Option<BlockersEnum>) -> Result<(), ScoutingError> {
        match blockers {
            Some(b) if !self.available_blockers().contains(&b) => {
                Err(ScoutingError::InvalidSelection)
            }
            _ if self.available_blockers().is_empty() => Err(ScoutingError::InvalidSelection),
            _ => {
                self.blockers = blockers;
                Ok(())
            }
        }
    }

    /// The scout tells the next one: from the first, none told again after
    /// the last (see `BlockersEnum::next`).
    pub fn next_blockers(&mut self) -> Result<Option<BlockersEnum>, ScoutingError> {
        let next = self
            .event
            .and_then(|event| BlockersEnum::next(event, self.blockers));
        self.choose_blockers(next).map(|_| next)
    }

    /// Where our serve starts from, if told.
    pub fn serve_start(&self) -> Option<ServeStartEnum> {
        self.serve_start
    }

    /// Whether the scout can tell where our serve starts from, along with
    /// the evaluation.
    pub fn serve_start_applies(&self) -> bool {
        matches!(
            (self.step, self.event),
            (ScoutingStep::Eval, Some(EventTypeEnum::S))
        )
    }

    /// The scout tells where our serve starts from, or takes it back
    /// (`None`): the evaluation saves it with the serve. Never required.
    pub fn choose_serve_start(
        &mut self,
        start: Option<ServeStartEnum>,
    ) -> Result<(), ScoutingError> {
        match self.serve_start_applies() {
            true => {
                self.serve_start = start;
                Ok(())
            }
            false => Err(ScoutingError::InvalidSelection),
        }
    }

    /// The scout touches a zone our serve starts from: told, or taken back
    /// when it was.
    pub fn touch_serve_start(&mut self, start: ServeStartEnum) -> Result<(), ScoutingError> {
        let touched = (self.serve_start != Some(start)).then_some(start);
        self.choose_serve_start(touched)
    }

    /// Whether the scout can tell where the ball went, along with the
    /// evaluation: for our serve, on their court, and for our reception,
    /// on ours.
    pub fn targets_apply(&self) -> bool {
        self.target_side().is_some()
    }

    /// The court where the ball goes, when the scout can tell where.
    pub fn target_side(&self) -> Option<CourtSide> {
        match (self.step, self.event) {
            (ScoutingStep::Eval, Some(event)) => CourtSide::of(event),
            _ => None,
        }
    }

    /// How the scout tells where the ball goes, when the scout can: a cell
    /// or two, or a zone at the net for our block.
    pub fn target_kind(&self) -> Option<TargetKind> {
        match (self.step, self.event) {
            (ScoutingStep::Eval, Some(event)) => TargetKind::of(event),
            _ => None,
        }
    }

    /// The scout tells where the ball went, or takes it back (`None`): the
    /// evaluation saves it with the event. Never required; only as the
    /// event tells it (see `TargetKind::fits`).
    pub fn choose_target(&mut self, target: Option<CourtTarget>) -> Result<(), ScoutingError> {
        match (self.target_kind(), target) {
            (Some(_), None) => {
                self.target = None;
                self.blockers = None;
                Ok(())
            }
            (Some(kind), Some(t)) if kind.fits(&t) => {
                self.target = Some(t);
                Ok(())
            }
            _ => Err(ScoutingError::InvalidSelection),
        }
    }

    /// The scout touches a cell of the court: alone, or along with the one
    /// told next to it; touched again, taken back (see
    /// `CourtTarget::touched`). Not for the block, at the net.
    pub fn touch_target(&mut self, cell: CourtCell) -> Result<(), ScoutingError> {
        self.choose_target(CourtTarget::touched(self.target, cell))
    }

    /// The scout touches a zone at the net, for our block: told, or taken
    /// back when it was.
    pub fn touch_zone(&mut self, zone: CourtZoneEnum) -> Result<(), ScoutingError> {
        match self.target_kind() {
            Some(TargetKind::Net) => {
                self.choose_target(CourtTarget::touched_zone(self.target, zone))
            }
            _ => Err(ScoutingError::InvalidSelection),
        }
    }

    /// The events that can follow.
    pub fn available_events(&self) -> Vec<EventTypeEnum> {
        self.events_of(self.side)
    }

    /// The events `side` can play now: scouting both sides, the ones the
    /// sequence of the set admits (`rally::admits`), with at least one of
    /// their evaluations.
    fn events_of(&self, side: Side) -> Vec<EventTypeEnum> {
        let Some(snapshot) = self.snapshot_of(side) else {
            return vec![];
        };
        snapshot
            .get_available_options()
            .iter()
            .copied()
            .filter(|event| self.theirs.is_none() || self.admits(side, *event, None))
            .collect()
    }

    /// Whether the touch of `side` (with `eval`, or any it can have) is
    /// admitted after the touches so far. Always, scouting one side.
    fn admits(&self, side: Side, event: EventTypeEnum, eval: Option<EvalEnum>) -> bool {
        let (Some(theirs), Some(own)) = (&self.theirs, self.snapshot_of(side)) else {
            return true;
        };
        let touch = |eval| EventEntry {
            timestamp: Utc::now(),
            event_type: event,
            eval,
            player: None,
            target_player: None,
            technique: None,
            flags: Default::default(),
        };
        let admitted = |eval| {
            rally::admits(
                &theirs.touches,
                &self.snapshot,
                &theirs.snapshot,
                side,
                &touch(eval),
            )
        };
        match (eval, event.requires_evaluation()) {
            (Some(eval), _) => admitted(Some(eval)),
            (None, true) => own
                .available_evals(event)
                .into_iter()
                .any(|e| admitted(Some(e))),
            (None, false) => admitted(None),
        }
    }

    /// The events to choose from, each with its side: the active side's,
    /// and the other side's where the ball could be on either (a touch
    /// skipped) and for the changes of the lineup and the points without a
    /// cause, always (see TODO/001, §5.3). Scouting one side: A's.
    pub fn event_choices(&self) -> Vec<(Side, EventTypeEnum)> {
        let own = self.events_of(self.side);
        let other = self.events_of(self.side.other());
        let ambiguous =
            own.iter().any(|e| !is_neutral(*e)) && other.iter().any(|e| !is_neutral(*e));
        let still = self.ball_still();
        own.iter()
            .map(|e| (self.side, *e))
            .chain(
                other
                    .iter()
                    .filter(|e| match is_neutral(**e) {
                        true => still || matches!(e, EventTypeEnum::PW | EventTypeEnum::PL),
                        false => ambiguous,
                    })
                    .map(|e| (self.side.other(), *e)),
            )
            .collect()
    }

    /// Whether the ball is still: the rally hasn't started, on neither side.
    fn ball_still(&self) -> bool {
        self.snapshot.get_last_touch().is_none()
            && self
                .theirs
                .as_ref()
                .is_none_or(|t| t.snapshot.get_last_touch().is_none())
    }

    /// The side the scout is choosing for next: scouting both sides, the
    /// only one with a touch of the ball to play, or the one the ball is on
    /// when it could be either.
    fn deduced_side(&self) -> Side {
        let touching = |side| self.events_of(side).iter().any(|e| !is_neutral(*e));
        match (&self.theirs, touching(Side::A), touching(Side::B)) {
            (None, _, _) => Side::A,
            (Some(_), true, false) => Side::A,
            (Some(_), false, true) => Side::B,
            (Some(theirs), _, _) => rally::ball_side(&theirs.touches, &self.snapshot),
        }
    }

    /// The scout swaps the side the choices are for (F5): the ball isn't
    /// where it seems. Before anything is chosen; the next event goes back
    /// to the side the ball is on.
    pub fn swap_side(&mut self) -> Result<Side, ScoutingError> {
        match (self.theirs.is_some(), self.step, self.event) {
            (true, ScoutingStep::Event, None) => {
                self.side = self.side.other();
                self.start_next_on_side();
                Ok(self.side)
            }
            _ => Err(ScoutingError::InvalidSelection),
        }
    }

    /// The scout chooses the event of `side` among `event_choices`.
    pub async fn choose_event_of(
        &mut self,
        side: Side,
        event: EventTypeEnum,
    ) -> Result<Progress, ScoutingError> {
        match (self.step, self.event_choices().contains(&(side, event))) {
            (ScoutingStep::Event, true) => {
                let before = self.side;
                self.side = side;
                let chosen = self.choose_event(event).await;
                if chosen.is_err() {
                    self.side = before;
                }
                chosen
            }
            (ScoutingStep::Event, false) => Err(ScoutingError::EventNotAvailable(event)),
            _ => Err(ScoutingError::InvalidSelection),
        }
    }

    /// The players to choose for the event: the ones on court who can play
    /// it, or the ones who can be replaced (the one out for the libero
    /// too).
    pub fn player_choices(&self) -> Vec<PlayerChoice> {
        self.choices_for(self.event)
    }

    fn choices_for(&self, event: Option<EventTypeEnum>) -> Vec<PlayerChoice> {
        let lineup = &self.active().current_lineup;
        let choices = match event {
            Some(EventTypeEnum::R) => lineup.get_replaceable_lineup_choices(),
            _ => lineup.get_lineup_choices(),
        };
        choices
            .into_iter()
            .filter_map(|(index, (role, id))| {
                id.filter(|id| match event {
                    // on court, or out for the libero: still in the game
                    Some(EventTypeEnum::R) => lineup.is_in_the_game(id),
                    None => lineup.find_position(id).is_some(),
                    // only the diagonal: the libero follows the setter, so
                    // anyone else would move it, even to the front row; the
                    // saved sets with another one stay readable
                    Some(EventTypeEnum::CS) => lineup.get_setter_diagonal() == Some(*id),
                    Some(event) => self.active().can_play(event, id),
                })
                .and_then(|id| self.active_team().find_player(id))
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
                .active()
                .current_lineup
                .get_available_replacements(self.active_team(), replaced)
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

    /// The evaluations of the event chosen, after the one before it (see
    /// `Snapshot::available_evals`).
    pub fn available_evals(&self) -> Vec<EvalEnum> {
        self.event
            .map(|event| {
                self.active()
                    .available_evals(event)
                    .into_iter()
                    .filter(|eval| self.admits(self.side, event, Some(*eval)))
                    .collect()
            })
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

    /// Where the server starts from before the scout tells it: where they
    /// served from last in the match, if told.
    fn preselected_serve_start(&self) -> Option<ServeStartEnum> {
        match (self.event, self.player) {
            (Some(EventTypeEnum::S), Some(server)) => {
                ServeStartEnum::preselected(&server, self.match_events())
            }
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
                self.player = self.active().current_lineup.get_serving_player();
                self.step = ScoutingStep::Eval;
                self.technique = self.preselected_technique();
                self.serve_start = self.preselected_serve_start();
                Ok(Progress::Choosing)
            }
            (ScoutingStep::Event, true, e) if e.asks_player() => {
                self.event = Some(e);
                self.step = ScoutingStep::Player;
                Ok(Progress::Choosing)
            }
            (ScoutingStep::Event, true, OE | OS | OF | FB | PW | PL | CL | LR) => {
                self.save(EventEntry {
                    timestamp: Utc::now(),
                    event_type: event,
                    eval: None,
                    player: None,
                    target_player: None,
                    technique: None,
                    flags: Default::default(),
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
                self.serve_start = self.preselected_serve_start();
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
                    flags: Default::default(),
                })
                .await
            }
            _ => Err(ScoutingError::InvalidPlayerSelection),
        }
    }

    /// Whether the event can be the team's, no player chosen (see
    /// `choose_team`): our fault, e.g. a rotation fault.
    pub fn team_choice(&self) -> bool {
        matches!(
            (self.step, self.event),
            (ScoutingStep::Player, Some(event)) if !event.requires_player()
        )
    }

    /// The team makes the event, no player: it's saved.
    pub async fn choose_team(&mut self) -> Result<Progress, ScoutingError> {
        match (self.team_choice(), self.event) {
            (true, Some(event)) => {
                self.save(EventEntry {
                    timestamp: Utc::now(),
                    event_type: event,
                    eval: None,
                    player: None,
                    target_player: None,
                    technique: None,
                    flags: Default::default(),
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
                    flags: Default::default(),
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
                if self.available_evals().contains(&eval) =>
            {
                self.save(EventEntry {
                    timestamp: Utc::now(),
                    event_type: event,
                    eval: Some(eval),
                    player: self.player,
                    target_player: None,
                    technique: self.technique,
                    flags: EventFlags::default()
                        .with_target(
                            self.target
                                .filter(|t| self.target_kind().is_some_and(|k| k.fits(t))),
                        )
                        .with_blockers(
                            self.blockers
                                .filter(|b| self.available_blockers().contains(b)),
                        )
                        .with_serve_start(self.serve_start.filter(|_| self.serve_start_applies())),
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
                self.target = None;
                self.blockers = None;
                self.serve_start = None;
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
        self.target = None;
        self.blockers = None;
        self.serve_start = None;
        self.step = ScoutingStep::Event;
    }

    /// The next event, from its first choice: with the player first, our
    /// serve goes straight to its evaluation.
    fn start_next(&mut self) {
        self.side = self.deduced_side();
        self.start_next_on_side();
    }

    /// The next event of the side chosen, from its first choice.
    fn start_next_on_side(&mut self) {
        self.reset();
        let serving = self.available_events().contains(&EventTypeEnum::S);
        if self.order == ChoiceOrder::PlayerFirst && serving {
            self.event = Some(EventTypeEnum::S);
            self.player = self.active().current_lineup.get_serving_player();
            self.step = ScoutingStep::Eval;
            self.technique = self.preselected_technique();
            self.serve_start = self.preselected_serve_start();
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
        let removed = match &self.theirs {
            Some(_) => self.undo_last_touch().await,
            None => self.undo_last_own_event().await,
        };
        let removed = match removed {
            Ok(Some(removed)) => removed,
            // nothing to undo
            Ok(None) => return Ok(()),
            Err(e) => return Err(e),
        };
        // the set ended by the event is open again
        if let Some(ended) = self
            .current_match
            .sets
            .iter_mut()
            .find(|s| s.set_number == self.set.set_number)
        {
            *ended = self.set.clone();
        }
        self.reset();
        // the reception told for the serve again, if the event removed
        // took it
        self.reception = match (self.can_tell_reception(), removed.event_type) {
            (true, EventTypeEnum::P | EventTypeEnum::OS) => removed.flags.reception(),
            (true, _) => self.reception,
            (false, _) => ReceptionSchemeEnum::default(),
        };
        match (self.order, removed.event_type, removed.player) {
            (_, A | B | D | P | S, Some(player)) => {
                self.event = Some(removed.event_type);
                self.player = Some(player);
                self.step = ScoutingStep::Eval;
                // the set or serve of the removed event, if it was saved
                self.technique = removed.technique.or_else(|| self.preselected_technique());
                // and where it went, if told
                self.target = removed
                    .flags
                    .target()
                    .filter(|t| self.target_kind().is_some_and(|k| k.fits(t)));
                // and how many blocked
                self.blockers = removed
                    .flags
                    .blockers()
                    .filter(|b| self.available_blockers().contains(b));
                // and where our serve started from
                self.serve_start = removed
                    .flags
                    .serve_start()
                    .filter(|_| self.serve_start_applies());
            }
            (_, R, Some(player)) => {
                self.event = Some(R);
                self.player = Some(player);
                self.step = ScoutingStep::Replacement;
            }
            // our freeball, also one naming its player (scouted before it
            // became a team event), is chosen again with the others
            (_, FB, _) => self.start_next(),
            (_, _, Some(player)) => {
                self.event = Some(removed.event_type);
                self.player = Some(player);
                self.step = ScoutingStep::Player;
            }
            _ => self.start_next(),
        }
        Ok(())
    }

    /// The last event of A removed (scouting A alone): the snapshot replays
    /// the events left.
    async fn undo_last_own_event(&mut self) -> Result<Option<EventEntry>, ScoutingError> {
        let removed = match self
            .set_writer
            .remove_last_event(&self.current_match, self.set.set_number)
            .await
        {
            Ok(Some(removed)) => removed,
            Ok(None) => return Ok(None),
            Err(_) => return Err(ScoutingError::CouldNotUndo),
        };
        self.set.events.pop();
        self.snapshot = self
            .set
            .compute_snapshot(self.current_match.max_substitutions)
            .map_err(|_| ScoutingError::CouldNotComputeSnapshot)?;
        Ok(Some(removed))
    }

    /// The last touch of the set removed (scouting both sides), of
    /// whichever side: the sides replay the touches left, and the scout
    /// goes back to the side that played it.
    async fn undo_last_touch(&mut self) -> Result<Option<EventEntry>, ScoutingError> {
        let Some(theirs) = &self.theirs else {
            return Err(ScoutingError::CouldNotUndo);
        };
        let (side, removed) = match self
            .set_writer
            .remove_last_touch(&self.current_match, self.set.set_number)
            .await
        {
            Ok(Some(removed)) => removed,
            Ok(None) => return Ok(None),
            Err(_) => return Err(ScoutingError::CouldNotUndo),
        };
        let mut touches = theirs.touches.clone();
        touches.pop();
        self.replay_touches(touches)?;
        self.side = side;
        Ok(Some(removed))
    }

    /// The sides at the touches: their snapshots, and the set from A's
    /// side.
    fn replay_touches(&mut self, touches: Vec<(Side, EventEntry)>) -> Result<(), ScoutingError> {
        let Some(theirs) = &mut self.theirs else {
            return Err(ScoutingError::CouldNotComputeSnapshot);
        };
        let (a, b) = rally::replay(
            &theirs.start_a,
            &theirs.start_b,
            &touches,
            self.current_match.max_substitutions,
        )
        .map_err(|_| ScoutingError::CouldNotComputeSnapshot)?;
        self.set.events = rally::project(&touches)
            .a
            .into_iter()
            .map(|p| p.entry)
            .collect();
        self.snapshot = a;
        theirs.snapshot = b;
        theirs.touches = touches;
        Ok(())
    }

    /// Saves the touch of the side being scouted, in the sequence of the
    /// set: admitted by `rally::admits`, and the two sides still replaying.
    async fn save_touch(&mut self, event: &EventEntry) -> Result<(), ScoutingError> {
        let Some(theirs) = &self.theirs else {
            return Err(ScoutingError::CouldNotAddEvent);
        };
        let admitted = rally::admits(
            &theirs.touches,
            &self.snapshot,
            &theirs.snapshot,
            self.side,
            event,
        );
        let touches: Vec<(Side, EventEntry)> = theirs
            .touches
            .iter()
            .cloned()
            .chain([(self.side, event.clone())])
            .collect();
        let replays = rally::replay(
            &theirs.start_a,
            &theirs.start_b,
            &touches,
            self.current_match.max_substitutions,
        )
        .is_ok();
        match admitted && replays {
            true => {
                self.set_writer
                    .append_touch(&self.current_match, self.set.set_number, self.side, event)
                    .await
                    .map_err(|_| ScoutingError::CouldNotAddEvent)?;
                self.replay_touches(touches)
            }
            false => Err(ScoutingError::CouldNotAddEvent),
        }
    }

    /// Saves the event if it applies, then starts the next one.
    pub(crate) async fn save(&mut self, event: EventEntry) -> Result<Progress, ScoutingError> {
        // the reception goes on the reception, or on their ace before it
        let received = match (event.event_type, self.can_tell_reception()) {
            (EventTypeEnum::P, _) | (EventTypeEnum::OS, true) => Some(self.reception),
            _ => None,
        };
        let flags = event.flags.with_inverted_p1(self.inverted_p1());
        let event = EventEntry {
            flags: received.map_or(flags, |scheme| flags.with_reception(scheme)),
            ..event
        };
        let points = self.points();
        match self.theirs {
            Some(_) => self.save_touch(&event).await?,
            None => {
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
            }
        }
        // told for one serve: written, or the rally over without it
        if received.is_some() || self.points() != points {
            self.reception = ReceptionSchemeEnum::default();
        }
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

    /// The points of the set so far.
    fn points(&self) -> u16 {
        u16::from(self.snapshot.score_us) + u16::from(self.snapshot.score_them)
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

/// Whether the event is no touch of the ball: a change of the lineup, a
/// fault, a point without a cause.
fn is_neutral(event: EventTypeEnum) -> bool {
    use EventTypeEnum::*;
    matches!(event, R | CL | LR | LF | CS | F | PW | PL)
}

/// The inverted P1 of the last event of the match, in this set or in the
/// sets before: a new set goes on as the last one ended.
fn last_inverted_p1(current_match: &MatchEntry, set: &SetEntry) -> bool {
    earlier_inverted_p1(&current_match.sets, set)
}

/// The same from the sets `sets`, those before `set` counting.
fn earlier_inverted_p1(sets: &[SetEntry], set: &SetEntry) -> bool {
    let mut earlier: Vec<&SetEntry> = sets
        .iter()
        .filter(|s| s.set_number < set.set_number)
        .collect();
    earlier.sort_by_key(|s| s.set_number);
    earlier
        .iter()
        .flat_map(|s| s.events.iter())
        .chain(set.events.iter())
        .next_back()
        .is_some_and(|e| e.flags.inverted_p1())
}
