use crate::{
    constants::{
        DEFAULT_SET_PARTIALS, DEFAULT_SET_TARGET_SCORE, TIE_BREAK_SET_PARTIALS,
        TIE_BREAK_SET_TARGET_SCORE,
    },
    errors::{AppError, SnapshotError},
    localization::Labels,
    shapes::{
        court_zone::TargetKind,
        enums::{
            EvalEnum, EventTypeEnum, FriendlyName, LineupRulesEnum, PhaseEnum, RoleEnum,
            TeamSideEnum, ZoneEnum,
        },
        flags::BlockersEnum,
        flags::EventFlags,
        lineup::Lineup,
        refusal::Refusal,
        set::SetEntry,
        stats::{
            AttackKey, EventKey, FirstRallyKey, FreeballKey, PossessionKey, RallyKey, Situation,
            Stats, Touch,
        },
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
    pub flags: EventFlags,
}

/// An event as it's saved, one row of the events file. The technique and
/// the flags are the last columns: the older versions, up to v0.4.3 and up
/// to v0.10.1, don't write them and skip them when reading, as long as
/// every row has them (v0.5.0 and later skip them anyway).
#[derive(Serialize, Deserialize)]
struct EventRecord {
    timestamp: DateTime<Utc>,
    event_type: EventTypeEnum,
    player: Option<Uuid>,
    eval: Option<EvalEnum>,
    target_player: Option<Uuid>,
    #[serde(default)]
    technique: Option<String>,
    #[serde(default)]
    flags: Option<String>,
}

impl TryFrom<EventRecord> for EventEntry {
    type Error = AppError;
    fn try_from(record: EventRecord) -> Result<Self, Self::Error> {
        let technique = record
            .technique
            .filter(|code| !code.is_empty())
            .map(|code| TechniqueEnum::parse(record.event_type, &code))
            .transpose()?;
        let flags = EventFlags::parse(record.flags.as_deref().unwrap_or_default())?;
        Ok(EventEntry {
            timestamp: record.timestamp,
            event_type: record.event_type,
            player: record.player,
            eval: record.eval,
            target_player: record.target_player,
            technique,
            flags,
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
            flags: Some(event.flags.to_string()),
        }
    }
}

impl Display for EventEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "event={:?} player={:?} eval={:?} target={:?} technique={:?} flags={:?}",
            self.event_type, self.player, self.eval, self.target_player, self.technique, self.flags
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
    // our possession in the side out being played, 0 out of one
    possession: u8,
    // the ball on our side to play, in that possession
    ball_ours: bool,
    // the situation of the rally being played
    rally: Situation,
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
            possession: 0,
            ball_ours: false,
            rally: Situation::default(),
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

    /// The name of the event to choose next: the libero replacement says
    /// what it does now, the libero going out or coming back in.
    pub fn next_event_name(&self, event: EventTypeEnum, labels: &Labels) -> &'static str {
        match (event, self.current_lineup.is_libero_out()) {
            (EventTypeEnum::LR, false) => labels.libero_out,
            (EventTypeEnum::LR, true) => labels.libero_in,
            _ => event.friendly_name(labels),
        }
    }

    /// Whether the player can play the event: a player on court, but the
    /// libero doesn't attack, block or become the setter, a back row player
    /// doesn't block, and the serve is of the player in position 1. The
    /// player out for the libero can be substituted too (the libero never,
    /// as the lineup checks).
    pub fn can_play(&self, event_type: EventTypeEnum, player_id: &Uuid) -> bool {
        self.play_refusal(event_type, player_id).is_none()
    }

    /// Why the player can't play the event, if they can't.
    fn play_refusal(&self, event_type: EventTypeEnum, player_id: &Uuid) -> Option<Refusal> {
        use EventTypeEnum::*;
        let lineup = &self.current_lineup;
        let is_libero = lineup.get_current_libero() == *player_id;
        let player = *player_id;
        match (
            event_type,
            lineup.find_position(player_id).is_some(),
            lineup.is_in_the_game(player_id),
        ) {
            (R, _, false) | (LF, _, false) => Some(Refusal::NotOnCourt(player)),
            (LF, _, true) if is_libero => Some(Refusal::Libero(player)),
            (R | LF, _, true) => None,
            (_, false, _) => Some(Refusal::NotOnCourt(player)),
            (S, true, _) if lineup.get_serving_player() != Some(player) => {
                Some(Refusal::NotServing(player))
            }
            (A | CS | B, true, _) if is_libero => Some(Refusal::Libero(player)),
            (B, true, _) if lineup.is_back_row_player(player_id) => {
                Some(Refusal::BackRowBlock(player))
            }
            (_, true, _) => None,
        }
    }

    /// Whether the player can come in for a substitution: a player not in
    /// the game (on court or out for the libero), and not a libero (the
    /// other substitution rules are checked by the lineup).
    fn can_come_in(&self, player_id: &Uuid) -> bool {
        let lineup = &self.current_lineup;
        !lineup.is_in_the_game(player_id)
            && lineup.get_current_libero() != *player_id
            && lineup.get_fallback_libero() != Some(*player_id)
    }

    /// Checks that the event can follow the ones before it, whether it comes
    /// from the scouting or from a saved set: the set isn't over, the event
    /// is among the available ones, and it has the evaluation and the
    /// players it needs (and none it doesn't).
    fn validate(&self, event: &EventEntry) -> Result<(), AppError> {
        match self.event_refusal(event) {
            Some(reason) => Err(AppError::Snapshot(SnapshotError::InvalidEvent(format!(
                "{event}: {reason}"
            )))),
            None => Ok(()),
        }
    }

    /// Why the event can't follow the ones before it, the substitutions'
    /// rules too: what the import of a file tells the user.
    pub fn refusal(&self, event: &EventEntry) -> Option<Refusal> {
        self.event_refusal(event).or_else(|| {
            match (event.event_type, event.player, event.target_player) {
                (EventTypeEnum::R, Some(replaced), Some(replacement)) => self
                    .current_lineup
                    .substitution_refusal(&replaced, &replacement),
                _ => None,
            }
        })
    }

    fn event_refusal(&self, event: &EventEntry) -> Option<Refusal> {
        self.sequence_issue(event)
            .or_else(|| Self::evaluation_issue(event))
            .or_else(|| self.player_issue(event))
            .or_else(|| Self::technique_issue(event))
            .or_else(|| self.replacement_issue(event))
    }

    fn sequence_issue(&self, event: &EventEntry) -> Option<Refusal> {
        match (
            self.get_set_winner(self.set_number),
            self.available_options.contains(&event.event_type),
        ) {
            (Some(_), _) => Some(Refusal::SetOver),
            (None, false) => Some(Refusal::NotNow {
                available: self.available_options.clone(),
            }),
            (None, true) => None,
        }
    }

    fn evaluation_issue(event: &EventEntry) -> Option<Refusal> {
        match (event.event_type.requires_evaluation(), event.eval) {
            (true, None) => Some(Refusal::MissingEvaluation),
            (true, Some(eval)) if !event.event_type.available_evals().contains(&eval) => {
                Some(Refusal::EvaluationNotAllowed(eval))
            }
            (false, Some(_)) => Some(Refusal::UnexpectedEvaluation),
            _ => None,
        }
    }

    fn technique_issue(event: &EventEntry) -> Option<Refusal> {
        match event.technique {
            Some(technique) if !technique.fits(event.event_type) => {
                Some(Refusal::TechniqueNotAllowed(technique))
            }
            _ => None,
        }
    }

    fn player_issue(&self, event: &EventEntry) -> Option<Refusal> {
        let event_type = event.event_type;
        match (
            event_type.requires_player(),
            event_type.accepts_player(),
            event.player,
        ) {
            (true, _, None) => Some(Refusal::MissingPlayer),
            (_, true, Some(player)) => self.play_refusal(event_type, &player),
            (_, false, Some(_)) => Some(Refusal::UnexpectedPlayer),
            _ => None,
        }
    }

    fn replacement_issue(&self, event: &EventEntry) -> Option<Refusal> {
        match (event.event_type, event.target_player) {
            (EventTypeEnum::R, None) => Some(Refusal::MissingReplacement),
            (EventTypeEnum::R, Some(replacement)) if !self.can_come_in(&replacement) => {
                Some(Refusal::CantComeIn(replacement))
            }
            (EventTypeEnum::R, Some(_)) | (_, None) => None,
            (_, Some(_)) => Some(Refusal::UnexpectedReplacement),
        }
    }

    /// The zone a player attacks from, told by their role and row (the
    /// scouting doesn't ask for it):
    /// - in the front row, the setter and the middle blockers attack from
    ///   3, the outside hitter from 4 and the opposite from 2, but in
    ///   rotation 0 (setter in position 1, "S1" in the report) in side
    ///   out, where they receive and attack from the side they stand on
    ///   (outside hitter 2, opposite 4), unless the P1 is inverted;
    /// - in the back row, the outside hitter attacks from 8 (pipe) and the
    ///   opposite from 9 (from position 1);
    /// - the others (back row setter and middle blocker, libero, a player
    ///   not on court) have no zone: their attacks still count, but not in
    ///   the distribution and the counter-attacks.
    fn get_attack_zone(&self, player_id: &Uuid, inverted_p1: bool) -> Option<ZoneEnum> {
        use PhaseEnum::*;
        use RoleEnum::*;
        use ZoneEnum::*;
        let role = self.current_lineup.get_role(player_id);
        let back = self.current_lineup.is_back_row_player(player_id);
        // the canonical P1 in side out: the outside hitter from 2, the
        // opposite from 4; inverted, as everywhere else
        let canonical_p1 = self
            .current_lineup
            .get_current_rotation()
            .is_ok_and(|r| r == 0)
            && self.current_lineup.get_current_phase() == SideOut
            && !inverted_p1;
        match (role, back, canonical_p1) {
            (Ok(Setter | MiddleBlocker), false, _) => Some(Three),
            (Ok(OutsideHitter), false, true) => Some(Two),
            (Ok(OutsideHitter), false, false) => Some(Four),
            (Ok(OutsideHitter), true, _) => Some(Eight),
            (Ok(OppositeHitter), false, true) => Some(Four),
            (Ok(OppositeHitter), false, false) => Some(Two),
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
    /// received, until the rally ends, with the P1 as the last event says;
    /// before, in the order of rotation, which the positions must respect at
    /// the serve.
    pub fn get_playing_positions(&self) -> [Uuid; 6] {
        let lineup = &self.current_lineup;
        let rotation = lineup.get_positions();
        let inverted_p1 = self
            .last_event
            .as_ref()
            .is_some_and(|e| e.flags.inverted_p1());
        match self.get_formation() {
            Formation::Rally => lineup.get_rally_positions(inverted_p1).unwrap_or(rotation),
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

    /// Collects the statistics of an event, in the phase and rotation it was
    /// played in: called before the event updates the lineup, and with the
    /// event before it still in `last_event`.
    fn record_stats(&mut self, event: &EventEntry) -> Result<(), AppError> {
        use EventTypeEnum::*;
        let phase = self.current_lineup.get_current_phase();
        let rotation = self.current_lineup.get_current_rotation()?;
        // the zone of an attack comes from the role of the attacker
        let zone = match event.event_type {
            A => event
                .player
                .and_then(|p| self.get_attack_zone(&p, event.flags.inverted_p1())),
            _ => None,
        };
        // every event of the rally: an action missing its evaluation can't
        // be scored, and the substitutions and libero and setter changes
        // aren't part of the rally; the player only of the events asking
        // one (our freeball named it once)
        let skip = matches!(event.event_type, R | CL | LR | LF | CS)
            || (event.event_type.requires_evaluation() && event.eval.is_none());
        let situation = match skip {
            true => self.rally,
            false => self.rally_situation(event, phase, rotation),
        };
        self.rally = situation;
        let role =
            |player: &Option<Uuid>| player.and_then(|p| self.current_lineup.get_role(&p).ok());
        let set_type = match event.technique {
            Some(TechniqueEnum::Set(t)) => Some(t),
            _ => None,
        };
        if !skip {
            self.stats.events.add(EventKey {
                event_type: event.event_type,
                phase,
                rotation,
                player: event.player.filter(|_| event.event_type.asks_player()),
                zone,
                // where our serve or attack went, or where we received,
                // dug or blocked theirs, if told as the event tells it
                target: event
                    .flags
                    .target()
                    .filter(|t| TargetKind::of(event.event_type).is_some_and(|k| k.fits(t))),
                // how many blocked against our attack or on our block, if told
                blockers: event
                    .flags
                    .blockers()
                    .filter(|b| BlockersEnum::of(event.event_type).contains(b)),
                eval: event.eval,
                situation,
                touch: event.event_type.asks_player().then(|| Touch {
                    role: role(&event.player),
                    // our touch before in the same rally
                    previous: self
                        .last_event
                        .as_ref()
                        .filter(|e| e.event_type.rally_winner(e.eval).is_none())
                        .and_then(|e| e.eval),
                    set_type,
                }),
            });
        }
        if let Some(winner) = event.event_type.rally_winner(event.eval) {
            self.stats.rallies.add(RallyKey {
                phase,
                rotation,
                winner,
                situation,
            });
        }
        // the attacks, for the distribution and the counter-attacks: they
        // need the zone of the attack. Every attack of ours is built by us:
        // with the ball on their side before it, their attack and our dig or
        // block weren't scouted
        if let (A, Some(player), Some(zone), Some(eval), Some(prev)) = (
            event.event_type,
            event.player,
            zone,
            event.eval,
            &self.last_event,
        ) {
            self.stats.attacks.add(AttackKey {
                phase,
                rotation,
                player,
                zone,
                eval,
                prev_type: prev.event_type,
                prev_eval: prev.eval,
                situation,
                role: role(&Some(player)),
                set_type,
            });
        }
        self.record_first_rally(event, rotation);
        self.record_freeball(event, phase, rotation);
        self.record_possession(event, phase, rotation);
        // a new rally: a new situation
        if event.event_type.rally_winner(event.eval).is_some() {
            self.rally = Situation::default();
        }
        Ok(())
    }

    /// The situation of the rally once the event is played: our reception
    /// and the formation receiving it (told on the reception, or on their
    /// ace with no reception), and the P1 inverted or not, in P1 in side out.
    fn rally_situation(&self, event: &EventEntry, phase: PhaseEnum, rotation: u8) -> Situation {
        use EventTypeEnum::*;
        let side_out = phase == PhaseEnum::SideOut;
        Situation {
            reception: match (side_out, event.event_type) {
                (true, P) => event.eval.or(self.rally.reception),
                _ => self.rally.reception,
            },
            scheme: match (side_out, event.event_type) {
                (false, _) => None,
                (true, P | OS) => self.rally.scheme.or(Some(event.flags.reception())),
                (true, _) => self.rally.scheme,
            },
            inverted_p1: (side_out && rotation == 0).then(|| event.flags.inverted_p1()),
        }
    }

    /// Their freeball (`OF`) is exploited when we win the rally before they
    /// attack again, whoever scores the point (our kill, their error, their
    /// fault, our block on the freeball itself). It isn't when they win the
    /// rally, when they attack again (our dig or block comes next, even a
    /// block point) or when they send us another freeball, which is counted
    /// on its own. Their error after
    /// digging our attack (`A-`, `OE`) counts as exploited: the scouting
    /// doesn't tell whether they attacked.
    fn record_freeball(&mut self, event: &EventEntry, phase: PhaseEnum, rotation: u8) {
        use EventTypeEnum::*;
        // our block on their freeball itself, not on an attack after it
        let on_the_freeball = self.last_event.as_ref().is_some_and(|e| e.event_type == OF);
        let exploited = match (
            self.pending_freeball,
            event.event_type,
            event.event_type.rally_winner(event.eval),
            on_the_freeball,
        ) {
            (false, _, _, _) => None,
            // even our block point comes after their attack
            (true, D | OF, _, _) | (true, B, _, false) => Some(false),
            (true, _, Some(winner), _) => Some(winner == TeamSideEnum::Us),
            (true, _, None, _) => None,
        };
        if let Some(exploited) = exploited {
            self.stats.freeballs.add(FreeballKey {
                phase,
                rotation,
                exploited,
                situation: self.rally,
            });
        }
        self.pending_freeball = match (event.event_type, exploited) {
            (OF, _) => true,
            (_, Some(_)) => false,
            (_, None) => self.pending_freeball,
        };
    }

    /// The possessions of a side out: the first comes with their serve,
    /// unless they miss it or we fault before it; another one each time the
    /// ball comes back to our side to play: our dig keeping it (also over
    /// the net, `/`), our block keeping it (`+`), their freeball (`OF`), or
    /// our touch after an attack of ours they couldn't counter (`+`) or
    /// blocked but covered (`!`), or after a block of theirs. The rally's
    /// point goes to the last possession, also when scored on their side
    /// (their error, their kill, our block): the ones before are lost.
    fn record_possession(&mut self, event: &EventEntry, phase: PhaseEnum, rotation: u8) {
        use EvalEnum::*;
        use EventTypeEnum::*;
        let kept = |eval: Option<EvalEnum>| {
            matches!(eval, Some(Perfect | Positive | Exclamative | Negative))
        };
        let (possession, starts) = match (
            phase,
            self.possession,
            self.ball_ours,
            event.event_type,
            event.eval,
        ) {
            (PhaseEnum::Break, _, _, _, _) | (_, _, _, R | CL | LR | LF | CS, _) => {
                (self.possession, false)
            }
            (_, 0, _, P | OS, _) => (1, true),
            (_, 0, _, _, _) => (0, false),
            (_, n, false, D, Some(Perfect | Positive | Exclamative | Negative | Over))
            | (_, n, false, B, Some(Positive))
            | (_, n, false, OF | A | FB, _) => (n.saturating_add(1), true),
            (_, n, _, _, _) => (n, false),
        };
        if let (true, Some(before @ 1..)) = (starts, possession.checked_sub(1)) {
            self.stats.possessions.add(PossessionKey {
                rotation,
                possession: before,
                won: false,
                situation: self.rally,
            });
        }
        match (possession, event.event_type.rally_winner(event.eval)) {
            (1.., Some(winner)) => {
                self.stats.possessions.add(PossessionKey {
                    rotation,
                    possession,
                    won: winner == TeamSideEnum::Us,
                    situation: self.rally,
                });
                self.possession = 0;
                self.ball_ours = false;
            }
            _ => {
                self.possession = possession;
                self.ball_ours = match (event.event_type, event.eval) {
                    (P | D, eval) => kept(eval),
                    (B, Some(Positive)) | (OF, _) => true,
                    (A | FB | B, _) => false,
                    _ => self.ball_ours,
                };
            }
        }
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
            situation: self.rally,
        });
    }

    /// The events available before a serve: in side out when they serve,
    /// in break point when we do.
    pub fn get_rally_start_options(&self, phase: PhaseEnum) -> Vec<EventTypeEnum> {
        use EventTypeEnum::*;
        let mut options = match phase {
            // the libero for a player last: the rarest
            PhaseEnum::SideOut => vec![OS, OE, F, P, R, CL, LR, CS, LF],
            PhaseEnum::Break => vec![OE, F, S, R, CL, LR, CS, LF],
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
            // order: OS, OE, F, A, S, P, D, B, FB, OF, R, CL, LR, CS, LF
            //
            // the ball on our side, to play on our second or third touch:
            // our attack or our freeball (`FB`). Never a freeball on a first
            // touch: our reception or dig going straight over is `/`.
            //
            // on their side, they attack (our dig or block comes next) or
            // they don't: after our attack or block `+` we play the ball
            // again, their freeball (`OF`) optional; after our serve `/`
            // their reception came back, not a freeball; after our serve
            // `+`, `!` or `-`, our attack or block `-` they attack or send
            // a freeball (`OF`): the evaluation tells their reception or
            // dig, the freeball what they made of it, as our reception or
            // dig and our freeball (`FB`) do: a perfect reception the setter
            // sends over is `#`, then the freeball. A ball sent over on
            // purpose to score (the setter's dump) is an attack; one sent
            // out is an error. Our attack or our freeball
            // can still come when their attack and our dig or block weren't
            // scouted: the dig is optional.
            ((OS, None), serve_them.clone()),
            ((OE, None), serve_us.clone()),
            ((B, Some(Error)), serve_them.clone()),
            ((B, Some(Over)), serve_them.clone()),
            ((B, Some(Perfect)), serve_us.clone()),
            ((B, Some(Positive)), vec![OE, F, A, FB, OF]),
            ((B, Some(Negative)), vec![OS, OE, F, A, D, B, FB, OF]),
            ((A, Some(Error)), serve_them.clone()),
            ((A, Some(Over)), serve_them.clone()),
            ((A, Some(Negative)), vec![OS, OE, F, A, D, B, FB, OF]),
            ((A, Some(Perfect)), serve_us.clone()),
            ((A, Some(Positive)), vec![OE, F, A, FB, OF]),
            ((A, Some(Exclamative)), vec![OE, F, A, FB]),
            ((D, Some(Error)), serve_them.clone()),
            ((D, Some(Over)), vec![OS, OE, F, A, D, B, FB, OF]),
            ((D, Some(Exclamative)), vec![OE, F, A, FB]),
            ((D, Some(Negative)), vec![OE, F, A, FB]),
            ((D, Some(Positive)), vec![OE, F, A, FB]),
            ((D, Some(Perfect)), vec![OE, F, A, FB]),
            ((F, None), serve_them.clone()),
            ((P, Some(Error)), serve_them.clone()),
            ((P, Some(Over)), vec![OS, OE, F, A, D, B, FB, OF]),
            ((P, Some(Exclamative)), vec![OE, F, A, FB]),
            ((P, Some(Negative)), vec![OE, F, A, FB]),
            ((P, Some(Positive)), vec![OE, F, A, FB]),
            ((P, Some(Perfect)), vec![OE, F, A, FB]),
            ((S, Some(Error)), serve_them.clone()),
            ((S, Some(Perfect)), serve_us.clone()),
            ((S, Some(Positive)), vec![OS, OE, F, A, D, B, FB, OF]),
            ((S, Some(Exclamative)), vec![OS, OE, F, A, D, B, FB, OF]),
            ((S, Some(Over)), vec![OE, A, F, B, FB]),
            ((S, Some(Negative)), vec![OS, OE, F, A, D, B, FB, OF]),
            // our freeball, our reception or dig going straight over: they
            // attack, or mess it up and send a freeball back
            ((FB, None), vec![OS, OE, F, A, D, B, FB, OF]),
            // their freeball isn't a dig of ours, but what comes next is the
            // same as after one keeping the ball, and their point when it
            // falls on our court untouched; our block too, a ball crossing
            // the net close to it, as after their reception straight over
            // (our serve `/`): the scout tells a block from an attack
            ((OF, None), vec![OS, OE, F, A, B, FB]),
        ]
        .into_iter()
        .collect();
        match (event.event_type, event.eval) {
            (R | CL | LR | LF | CS, _) => self.available_options.clone(),
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
        if event.event_type == EventTypeEnum::LR {
            // the libero out, or back in
            self.current_lineup.toggle_libero()?;
        }
        if let (EventTypeEnum::LF, Some(player)) = (event.event_type, event.player) {
            // the libero for the player, in the back row
            self.current_lineup.set_libero_for(&player)?;
        }
        if event.event_type == EventTypeEnum::CS {
            // change setter
            if let Some(new_setter) = event.player {
                self.current_lineup.set_current_setter(&new_setter)?;
            }
        }
        self.current_lineup.update(event)?;
        if !matches!(
            event.event_type,
            EventTypeEnum::R
                | EventTypeEnum::CL
                | EventTypeEnum::LR
                | EventTypeEnum::LF
                | EventTypeEnum::CS
        ) {
            self.last_event = Some(event.clone());
        }
        Ok(())
    }
}
