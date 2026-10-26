use crate::{
    constants::{
        DEFAULT_SET_PARTIALS, DEFAULT_SET_TARGET_SCORE, TIE_BREAK_SET_PARTIALS,
        TIE_BREAK_SET_TARGET_SCORE,
    },
    errors::{AppError, SnapshotError},
    shapes::{
        enums::{
            EvalEnum, EventTypeEnum, LineupRulesEnum, PhaseEnum, RoleEnum, TeamSideEnum, ZoneEnum,
        },
        lineup::Lineup,
        set::SetEntry,
        stats::{AttackKey, EventKey, FirstRallyKey, FreeballKey, RallyKey, Stats},
        technique::{SetTypeEnum, TechniqueEnum},
    },
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::{self, Display, Formatter},
};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "EventRecord", into = "EventRecord")]
pub struct EventEntry {
    pub timestamp: DateTime<Utc>,
    pub event_type: EventTypeEnum,
    pub player: Option<Uuid>,
    pub eval: Option<EvalEnum>,
    pub target_player: Option<Uuid>,
    pub technique: Option<TechniqueEnum>,
}

/// An event as it's saved, one row of the events file. The technique is
/// the last column: the older versions, up to v0.4.3, don't write it and
/// skip it when reading, as long as every row has it.
#[derive(Serialize, Deserialize)]
struct EventRecord {
    timestamp: DateTime<Utc>,
    event_type: EventTypeEnum,
    player: Option<Uuid>,
    eval: Option<EvalEnum>,
    target_player: Option<Uuid>,
    #[serde(default)]
    technique: Option<String>,
}

impl TryFrom<EventRecord> for EventEntry {
    type Error = AppError;
    fn try_from(record: EventRecord) -> Result<Self, Self::Error> {
        let technique = record
            .technique
            .filter(|code| !code.is_empty())
            .map(|code| TechniqueEnum::parse(record.event_type, &code))
            .transpose()?;
        Ok(EventEntry {
            timestamp: record.timestamp,
            event_type: record.event_type,
            player: record.player,
            eval: record.eval,
            target_player: record.target_player,
            technique,
        })
    }
}

impl From<EventEntry> for EventRecord {
    fn from(event: EventEntry) -> Self {
        EventRecord {
            timestamp: event.timestamp,
            event_type: event.event_type,
            player: event.player,
            eval: event.eval,
            target_player: event.target_player,
            technique: event.technique.map(|t| t.to_string()),
        }
    }
}

impl Display for EventEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "event={:?} player={:?} eval={:?} target={:?} technique={:?}",
            self.event_type, self.player, self.eval, self.target_player, self.technique
        )
    }
}

/// How the team stands on court: the positions must respect the order of
/// rotation until the serve, then the players go where they play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Formation {
    /// we serve: in the order of rotation, the server behind the end line
    Serve,
    /// they serve: in the order of rotation, spread to receive
    Reception,
    /// after our serve or our reception, until the rally ends
    Rally,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub score_us: u8,
    pub score_them: u8,
    pub stats: Stats,
    pub current_lineup: Lineup,
    pub last_event: Option<EventEntry>,
    pub partials: Vec<(u8, u8)>, // (us, them)
    /// the team winning each rally, in order
    pub points: Vec<TeamSideEnum>,
    set_number: u8,
    // the events that can come next
    available_options: Vec<EventTypeEnum>,
    // their freeball (`OF`) not yet known to be exploited or not
    pending_freeball: bool,
}

// snapshot should be SetSnapshot, and it should guarantees set invariants
impl Snapshot {
    /// The snapshot before the first event, with the lineup rules saved with
    /// the set (`SetEntry::compute_snapshot` finds them for the sets saved
    /// without).
    pub fn new(set_entry: &SetEntry, max_substitutions: u8) -> Result<Self, AppError> {
        let rules = set_entry.lineup_rules.unwrap_or(LineupRulesEnum::CURRENT);
        Self::with_rules(set_entry, max_substitutions, rules)
    }

    pub fn with_rules(
        set_entry: &SetEntry,
        max_substitutions: u8,
        rules: LineupRulesEnum,
    ) -> Result<Self, AppError> {
        let current_lineup = Lineup::new(
            set_entry.initial_positions,
            match set_entry.serving_team {
                TeamSideEnum::Us => PhaseEnum::Break,
                TeamSideEnum::Them => PhaseEnum::SideOut,
            },
            set_entry.setter,
            set_entry.libero,
            set_entry.fallback_libero,
            max_substitutions,
            rules,
        )?;
        let mut snapshot = Snapshot {
            score_us: 0,
            score_them: 0,
            stats: Stats::new(),
            current_lineup,
            last_event: None,
            partials: vec![],
            points: vec![],
            set_number: set_entry.set_number,
            available_options: vec![],
            pending_freeball: false,
        };
        // the events available before the first serve: also the ones after a
        // substitution, setter or libero change opening the set
        snapshot.available_options =
            snapshot.get_rally_start_options(snapshot.current_lineup.get_current_phase());
        Ok(snapshot)
    }

    /// The events that can come next.
    pub fn get_available_options(&self) -> &[EventTypeEnum] {
        &self.available_options
    }

    /// Whether the player can play the event: a player on court, but the
    /// libero doesn't attack, block or become the setter, a back row player
    /// doesn't block, and the serve is of the player in position 1.
    pub fn can_play(&self, event_type: EventTypeEnum, player_id: &Uuid) -> bool {
        use EventTypeEnum::*;
        let lineup = &self.current_lineup;
        let is_libero = lineup.get_current_libero() == *player_id;
        lineup.find_position(player_id).is_some()
            && match event_type {
                S => lineup.get_serving_player() == Some(*player_id),
                A | CS => !is_libero,
                B => !is_libero && !lineup.is_back_row_player(player_id),
                _ => true,
            }
    }

    /// Whether the player can come in for a substitution: a player not on
    /// court, and not a libero (the other substitution rules are checked by
    /// the lineup).
    fn can_come_in(&self, player_id: &Uuid) -> bool {
        let lineup = &self.current_lineup;
        lineup.find_position(player_id).is_none()
            && lineup.get_current_libero() != *player_id
            && lineup.get_fallback_libero() != Some(*player_id)
    }

    /// Checks that the event can follow the ones before it, whether it comes
    /// from the scouting or from a saved set: the set isn't over, the event
    /// is among the available ones, and it has the evaluation and the
    /// players it needs (and none it doesn't).
    fn validate(&self, event: &EventEntry) -> Result<(), AppError> {
        match self
            .sequence_issue(event)
            .or_else(|| Self::evaluation_issue(event))
            .or_else(|| self.player_issue(event))
            .or_else(|| Self::technique_issue(event))
            .or_else(|| self.replacement_issue(event))
        {
            Some(reason) => Err(AppError::Snapshot(SnapshotError::InvalidEvent(format!(
                "{event}: {reason}"
            )))),
            None => Ok(()),
        }
    }

    fn sequence_issue(&self, event: &EventEntry) -> Option<String> {
        match (
            self.get_set_winner(self.set_number),
            self.available_options.contains(&event.event_type),
        ) {
            (Some(_), _) => Some("the set is over".to_string()),
            (None, false) => Some(format!(
                "not available, the available events are {:?}",
                self.available_options
            )),
            (None, true) => None,
        }
    }

    fn evaluation_issue(event: &EventEntry) -> Option<String> {
        match (event.event_type.requires_evaluation(), event.eval) {
            (true, None) => Some("missing evaluation".to_string()),
            (true, Some(eval)) if !event.event_type.available_evals().contains(&eval) => {
                Some(format!("evaluation {eval} not allowed"))
            }
            (false, Some(_)) => Some("unexpected evaluation".to_string()),
            _ => None,
        }
    }

    fn technique_issue(event: &EventEntry) -> Option<String> {
        match event.technique {
            Some(technique) if !technique.fits(event.event_type) => {
                Some(format!("technique {technique} not allowed"))
            }
            _ => None,
        }
    }

    fn player_issue(&self, event: &EventEntry) -> Option<String> {
        match (event.event_type.requires_player(), event.player) {
            (true, None) => Some("missing player".to_string()),
            (true, Some(player)) if !self.can_play(event.event_type, &player) => {
                Some(format!("player {player} can't play it"))
            }
            (false, Some(_)) => Some("unexpected player".to_string()),
            _ => None,
        }
    }

    fn replacement_issue(&self, event: &EventEntry) -> Option<String> {
        match (event.event_type, event.target_player) {
            (EventTypeEnum::R, None) => Some("missing replacement".to_string()),
            (EventTypeEnum::R, Some(replacement)) if !self.can_come_in(&replacement) => {
                Some(format!("player {replacement} can't come in"))
            }
            (EventTypeEnum::R, Some(_)) | (_, None) => None,
            (_, Some(_)) => Some("unexpected replacement".to_string()),
        }
    }

    /// The zone a player attacks from, told by their role and row (the
    /// scouting doesn't ask for it):
    /// - in the front row, the setter and the middle blockers attack from
    ///   3, the outside hitter from 4 and the opposite from 2, but in
    ///   rotation 0 (setter in position 1, "S1" in the report) in side
    ///   out, where they receive
    ///   and attack from the side they stand on (outside hitter 2,
    ///   opposite 4);
    /// - in the back row, the outside hitter attacks from 8 (pipe) and the
    ///   opposite from 9 (from position 1);
    /// - the others (back row setter and middle blocker, libero, a player
    ///   not on court) have no zone: their attacks still count, but not in
    ///   the distribution and the counter-attacks.
    fn get_attack_zone(&self, player_id: &Uuid) -> Option<ZoneEnum> {
        use PhaseEnum::*;
        use RoleEnum::*;
        use ZoneEnum::*;
        let role = self.current_lineup.get_role(player_id);
        let back = self.current_lineup.is_back_row_player(player_id);
        let rotation = self.current_lineup.get_current_rotation();
        match (role, back, rotation) {
            (Ok(Setter | MiddleBlocker), false, _) => Some(Three),
            (Ok(OutsideHitter), false, Ok(rotation)) => Some(
                if rotation == 0 && self.current_lineup.get_current_phase() == SideOut {
                    Two
                } else {
                    Four
                },
            ),
            (Ok(OutsideHitter), true, _) => Some(Eight),
            (Ok(OppositeHitter), false, Ok(rotation)) => Some(
                if rotation == 0 && self.current_lineup.get_current_phase() == SideOut {
                    Four
                } else {
                    Two
                },
            ),
            (Ok(OppositeHitter), true, _) => Some(Nine),
            _ => None,
        }
    }

    /// The sets the attacker can receive, told by their role and row (see
    /// [`SetTypeEnum::available`]): none for a player not on court.
    pub fn available_set_types(&self, attacker: &Uuid) -> Vec<SetTypeEnum> {
        self.current_lineup
            .get_role(attacker)
            .map(|role| {
                SetTypeEnum::available(role, self.current_lineup.is_back_row_player(attacker))
            })
            .unwrap_or_default()
    }

    /// The set chosen for the attacker before the scout picks one, told by
    /// their role and row and by the touch before (see
    /// [`SetTypeEnum::preselected`]).
    pub fn preselected_set_type(&self, attacker: &Uuid) -> Option<SetTypeEnum> {
        let previous = self.last_event.as_ref().map(|e| (e.event_type, e.eval));
        self.current_lineup
            .get_role(attacker)
            .ok()
            .and_then(|role| {
                SetTypeEnum::preselected(
                    role,
                    self.current_lineup.is_back_row_player(attacker),
                    previous,
                )
            })
    }

    pub fn get_set_winner(&self, set_number: u8) -> Option<TeamSideEnum> {
        let target_score = if set_number == 5 {
            TIE_BREAK_SET_TARGET_SCORE
        } else {
            DEFAULT_SET_TARGET_SCORE
        };
        let us = self.score_us;
        let them = self.score_them;
        if us >= target_score && us >= them + 2 {
            Some(TeamSideEnum::Us)
        } else if them >= target_score && them >= us + 2 {
            Some(TeamSideEnum::Them)
        } else {
            None
        }
    }

    /// The points of the set in order: the team scoring each one, with the
    /// score it reaches.
    pub fn point_sequence(&self) -> Vec<(TeamSideEnum, u8)> {
        self.points
            .iter()
            .scan((0u8, 0u8), |(us, them), side| {
                let score = match side {
                    TeamSideEnum::Us => {
                        *us += 1;
                        *us
                    }
                    TeamSideEnum::Them => {
                        *them += 1;
                        *them
                    }
                };
                Some((*side, score))
            })
            .collect()
    }

    fn set_score(&mut self, event: &EventEntry) {
        if let Some(side) = event.event_type.rally_winner(event.eval) {
            let (score, opponent_score) = match side {
                TeamSideEnum::Us => (&mut self.score_us, self.score_them),
                TeamSideEnum::Them => (&mut self.score_them, self.score_us),
            };
            *score += 1;
            self.points.push(side);
            // keep track of the partials
            let partials = if self.set_number == 5 {
                TIE_BREAK_SET_PARTIALS
            } else {
                DEFAULT_SET_PARTIALS
            };
            if partials.contains(&*score) && *score > opponent_score {
                self.partials.push((self.score_us, self.score_them));
            }
        }
    }

    /// How the team stands on court now (see `Formation`).
    pub fn get_formation(&self) -> Formation {
        let options = &self.available_options;
        match (
            self.current_lineup.get_current_phase(),
            options.contains(&EventTypeEnum::S),
            options.contains(&EventTypeEnum::P),
        ) {
            (PhaseEnum::Break, true, _) => Formation::Serve,
            (PhaseEnum::SideOut, _, true) => Formation::Reception,
            _ => Formation::Rally,
        }
    }

    /// The players by position as they play now: where they play
    /// (`Lineup::get_rally_positions`) once our serve is made or theirs is
    /// received, until the rally ends; before, in the order of rotation,
    /// which the positions must respect at the serve.
    pub fn get_playing_positions(&self) -> [Uuid; 6] {
        let lineup = &self.current_lineup;
        let rotation = lineup.get_positions();
        match self.get_formation() {
            Formation::Rally => lineup.get_rally_positions().unwrap_or(rotation),
            Formation::Serve | Formation::Reception => rotation,
        }
    }

    /// The last touch of the rally being played, with the side of the net
    /// it leaves the ball on (see `EventTypeEnum::ball_side_after`); `None`
    /// before the serve.
    pub fn get_last_touch(&self) -> Option<(&EventEntry, TeamSideEnum)> {
        self.last_event
            .as_ref()
            .and_then(|e| e.event_type.ball_side_after(e.eval).map(|side| (e, side)))
    }

    /// The side of the net the ball is on: after the last touch while a
    /// rally is played, before the serve with the team serving.
    pub fn get_ball_side(&self) -> TeamSideEnum {
        match (
            self.get_last_touch(),
            self.current_lineup.get_current_phase(),
        ) {
            (Some((_, side)), _) => side,
            (None, PhaseEnum::Break) => TeamSideEnum::Us,
            (None, PhaseEnum::SideOut) => TeamSideEnum::Them,
        }
    }

    /// The fundamental worth watching now, and the players on court who
    /// play it: the reception while we wait for their serve (the outside
    /// hitters and the libero); the attack while the ball is on our side in
    /// a rally (the outside hitters, the opposite, the middle blocker in the
    /// front row). `None` otherwise.
    pub fn get_spotlight(&self) -> Option<(EventTypeEnum, Vec<Uuid>)> {
        use EventTypeEnum::*;
        use RoleEnum::*;
        let lineup = &self.current_lineup;
        match (self.get_formation(), self.get_ball_side()) {
            (Formation::Reception, _) => Some(P),
            (Formation::Rally, TeamSideEnum::Us) => Some(A),
            _ => None,
        }
        .map(|fundamental| {
            let players = self
                .get_playing_positions()
                .into_iter()
                .filter(|id| {
                    matches!(
                        (
                            fundamental,
                            lineup.get_role(id),
                            lineup.is_back_row_player(id)
                        ),
                        (P, Ok(OutsideHitter | Libero), _)
                            | (A, Ok(OutsideHitter | OppositeHitter), _)
                            | (A, Ok(MiddleBlocker), false)
                    )
                })
                .collect();
            (fundamental, players)
        })
    }

    /// The team serving next: the one serving the set at its start, then
    /// the one winning the last rally. `None` while a rally is being played.
    pub fn get_serving_team(&self) -> Option<TeamSideEnum> {
        match &self.last_event {
            None => Some(match self.current_lineup.get_current_phase() {
                PhaseEnum::Break => TeamSideEnum::Us,
                PhaseEnum::SideOut => TeamSideEnum::Them,
            }),
            Some(last) => last.event_type.rally_winner(last.eval),
        }
    }

    /// Whether an attack after this event was built by us: after a
    /// reception or a dig keeping the ball on our side (`#`, `+`, `!`, `-`),
    /// after their reception coming straight back to us (serve `/`), after
    /// their freeball (`OF`), after an attack or block of ours they couldn't
    /// counter (`+`, we play the ball again), or after an attack of ours
    /// blocked but covered (`!`). An attack after anything else (e.g. after
    /// an overpass of ours) still counts among the attacks, but not in the
    /// distribution and the counter-attacks.
    fn is_attack_setup(prev_type: EventTypeEnum, prev_eval: Option<EvalEnum>) -> bool {
        use EvalEnum::*;
        use EventTypeEnum::*;
        matches!(
            (prev_type, prev_eval),
            (D | P, Some(Perfect | Positive | Exclamative | Negative))
                | (S, Some(Over))
                | (OF, None)
                | (A | B, Some(Positive))
                | (A, Some(Exclamative))
        )
    }

    /// Collects the statistics of an event, in the phase and rotation it was
    /// played in: called before the event updates the lineup, and with the
    /// event before it still in `last_event`.
    fn record_stats(&mut self, event: &EventEntry) -> Result<(), AppError> {
        use EventTypeEnum::*;
        let phase = self.current_lineup.get_current_phase();
        let rotation = self.current_lineup.get_current_rotation()?;
        // the zone of an attack comes from the role of the attacker
        let zone = match event.event_type {
            A => event.player.and_then(|p| self.get_attack_zone(&p)),
            _ => None,
        };
        // every event of the rally: an action missing its evaluation can't
        // be scored, and the substitutions and libero and setter changes
        // aren't part of the rally
        let skip = matches!(event.event_type, R | CL | CS)
            || (event.event_type.requires_evaluation() && event.eval.is_none());
        if !skip {
            self.stats.events.add(EventKey {
                event_type: event.event_type,
                phase,
                rotation,
                player: event.player,
                zone,
                eval: event.eval,
            });
        }
        if let Some(winner) = event.event_type.rally_winner(event.eval) {
            self.stats.rallies.add(RallyKey {
                phase,
                rotation,
                winner,
            });
        }
        // the attacks built by us, for the distribution and the
        // counter-attacks: they need the zone of the attack
        if let (A, Some(player), Some(zone), Some(eval), Some(prev)) = (
            event.event_type,
            event.player,
            zone,
            event.eval,
            &self.last_event,
        ) && Self::is_attack_setup(prev.event_type, prev.eval)
        {
            self.stats.attacks.add(AttackKey {
                phase,
                rotation,
                player,
                zone,
                eval,
                prev_type: prev.event_type,
                prev_eval: prev.eval,
            });
        }
        self.record_first_rally(event, rotation);
        self.record_freeball(event, phase, rotation);
        Ok(())
    }

    /// Their freeball (`OF`) is exploited when we win the rally before they
    /// attack again, whoever scores the point (our kill, their error, their
    /// fault). It isn't when they win the rally, when they attack again (our
    /// dig or block comes next, even a block point) or when they send us
    /// another freeball, which is counted on its own. Their error after
    /// digging our attack (`A-`, `OE`) counts as exploited: the scouting
    /// doesn't tell whether they attacked.
    fn record_freeball(&mut self, event: &EventEntry, phase: PhaseEnum, rotation: u8) {
        use EventTypeEnum::*;
        let exploited = match (
            self.pending_freeball,
            event.event_type,
            event.event_type.rally_winner(event.eval),
        ) {
            (false, _, _) => None,
            // even our block point comes after their attack
            (true, D | B | OF, _) => Some(false),
            (true, _, Some(winner)) => Some(winner == TeamSideEnum::Us),
            (true, _, None) => None,
        };
        if let Some(exploited) = exploited {
            self.stats.freeballs.add(FreeballKey {
                phase,
                rotation,
                exploited,
            });
        }
        self.pending_freeball = match (event.event_type, exploited) {
            (OF, _) => true,
            (_, Some(_)) => false,
            (_, None) => self.pending_freeball,
        };
    }

    /// The first rally of a side out goes from their serve to the end of
    /// our first attack; it's recorded by the event ending it:
    /// - their serve error (`OE`), their ace (`OS`) or our fault (`F`)
    ///   before the reception;
    /// - a reception error, or an overpass (the rally goes on, but our first
    ///   attack is lost);
    /// - after a reception keeping the ball, our attack (whatever its
    ///   outcome), their error or our fault, or our freeball (the rally goes
    ///   on, but our first attack is lost).
    fn record_first_rally(&mut self, event: &EventEntry, rotation: u8) {
        use EvalEnum::*;
        use EventTypeEnum::*;
        let prev = self.last_event.as_ref().map(|e| (e.event_type, e.eval));
        let reception_eval = match (event.event_type, event.eval, prev) {
            (OE | OS | F, _, _) if self.get_serving_team() == Some(TeamSideEnum::Them) => None,
            (P, Some(Error | Over), _) => event.eval,
            (
                A | OE | F | FB,
                _,
                Some((P, Some(eval @ (Perfect | Positive | Exclamative | Negative)))),
            ) => Some(eval),
            _ => return,
        };
        self.stats.first_rallies.add(FirstRallyKey {
            rotation,
            reception_eval,
            ending: event.event_type,
            ending_eval: event.eval,
        });
    }

    /// The events available before a serve: in side out when they serve,
    /// in break point when we do.
    pub fn get_rally_start_options(&self, phase: PhaseEnum) -> Vec<EventTypeEnum> {
        use EventTypeEnum::*;
        let mut options = match phase {
            PhaseEnum::SideOut => vec![OS, OE, F, P, R, CL, CS],
            PhaseEnum::Break => vec![OE, F, S, R, CL, CS],
        };
        if self.current_lineup.get_fallback_libero().is_none() {
            options.retain(|o| *o != CL);
        }
        options
    }

    fn get_next_options(&self, event: &EventEntry) -> Vec<EventTypeEnum> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        let serve_them = self.get_rally_start_options(PhaseEnum::SideOut);
        let serve_us = self.get_rally_start_options(PhaseEnum::Break);
        let options_map: HashMap<_, _> = [
            // order: OS, OE, F, A, S, P, D, B, FB, OF, R, CL, CS
            //
            // the ball on our side, to play on our second or third touch:
            // our attack or our freeball (`FB`); on their side: their
            // freeball (`OF`) besides their attack. Never a freeball on a
            // first touch: our reception or dig going straight over is `/`.
            ((OS, None), serve_them.clone()),
            ((OE, None), serve_us.clone()),
            ((B, Some(Error)), serve_them.clone()),
            ((B, Some(Over)), serve_them.clone()),
            ((B, Some(Perfect)), serve_us.clone()),
            ((B, Some(Positive)), vec![OE, F, A, FB]),
            ((B, Some(Negative)), vec![OS, OE, F, A, D, B, OF]),
            ((A, Some(Error)), serve_them.clone()),
            ((A, Some(Over)), serve_them.clone()),
            ((A, Some(Negative)), vec![OS, OE, F, A, D, B, OF]),
            ((A, Some(Perfect)), serve_us.clone()),
            ((A, Some(Positive)), vec![OE, F, A, FB]),
            ((A, Some(Exclamative)), vec![OE, F, A, FB]),
            ((D, Some(Error)), serve_them.clone()),
            ((D, Some(Over)), vec![OS, OE, F, A, D, B, OF]),
            ((D, Some(Exclamative)), vec![OE, F, A, FB]),
            ((D, Some(Negative)), vec![OE, F, A, FB]),
            ((D, Some(Positive)), vec![OE, F, A, FB]),
            ((D, Some(Perfect)), vec![OE, F, A, FB]),
            ((F, None), serve_them.clone()),
            ((P, Some(Error)), serve_them.clone()),
            ((P, Some(Over)), vec![OS, OE, F, A, D, B, OF]),
            ((P, Some(Exclamative)), vec![OE, F, A, FB]),
            ((P, Some(Negative)), vec![OE, F, A, FB]),
            ((P, Some(Positive)), vec![OE, F, A, FB]),
            ((P, Some(Perfect)), vec![OE, F, A, FB]),
            ((S, Some(Error)), serve_them.clone()),
            ((S, Some(Perfect)), serve_us.clone()),
            ((S, Some(Positive)), vec![OS, OE, F, D, B, OF]),
            ((S, Some(Over)), vec![OE, A, F, FB]),
            ((S, Some(Negative)), vec![OS, OE, F, D, B, OF]),
            // our freeball: as after our attack they dig (`A-`), and they
            // can send another one back
            ((FB, None), vec![OS, OE, F, A, D, B, OF]),
            // their freeball isn't a dig of ours, but what comes next is the
            // same as after one keeping the ball, and their point when it
            // falls on our court untouched
            ((OF, None), vec![OS, OE, F, A, FB]),
        ]
        .into_iter()
        .collect();
        match (event.event_type, event.eval) {
            (R | CL | CS, _) => self.available_options.clone(),
            key => options_map.get(&key).cloned().unwrap_or_default(),
        }
    }

    /// Applies the event, once validated: an invalid event leaves the
    /// snapshot as it was.
    pub fn add_event(&mut self, event: &EventEntry) -> Result<(), AppError> {
        self.validate(event)?;
        if event.event_type == EventTypeEnum::R {
            // replace lineup entry: it's the only change to the lineup that
            // can fail, so before anything else is updated
            if let (Some(replaced), Some(replacement)) = (event.player, event.target_player) {
                self.current_lineup
                    .add_substitution(&replaced, &replacement)?;
            }
        }
        self.set_score(event);
        self.record_stats(event)?;
        self.available_options = self.get_next_options(event);
        if event.event_type == EventTypeEnum::CL
            && self.current_lineup.get_fallback_libero().is_some()
        {
            // change libero
            self.current_lineup.swap_libero()?;
        }
        if event.event_type == EventTypeEnum::CS {
            // change setter
            if let Some(new_setter) = event.player {
                self.current_lineup.set_current_setter(&new_setter)?;
            }
        }
        self.current_lineup.update(event)?;
        if event.event_type != EventTypeEnum::R
            && event.event_type != EventTypeEnum::CL
            && event.event_type != EventTypeEnum::CS
        {
            self.last_event = Some(event.clone());
        }
        Ok(())
    }
}
