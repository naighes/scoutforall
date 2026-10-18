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
        stats::{AttackKey, EventKey, FirstRallyKey, RallyKey, Stats},
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
pub struct EventEntry {
    pub timestamp: DateTime<Utc>,
    pub event_type: EventTypeEnum,
    pub player: Option<Uuid>,
    pub eval: Option<EvalEnum>,
    pub target_player: Option<Uuid>,
}

impl Display for EventEntry {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "event={:?} player={:?} eval={:?} target={:?}",
            self.event_type, self.player, self.eval, self.target_player
        )
    }
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub score_us: u8,
    pub score_them: u8,
    pub stats: Stats,
    pub current_lineup: Lineup,
    pub last_event: Option<EventEntry>,
    pub partials: Vec<(u8, u8)>, // (us, them)
    set_number: u8,
    // the events that can come next
    available_options: Vec<EventTypeEnum>,
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
            set_number: set_entry.set_number,
            available_options: vec![],
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

    fn set_score(&mut self, event: &EventEntry) {
        if let Some(side) = event.event_type.rally_winner(event.eval) {
            let (score, opponent_score) = match side {
                TeamSideEnum::Us => (&mut self.score_us, self.score_them),
                TeamSideEnum::Them => (&mut self.score_them, self.score_us),
            };
            *score += 1;
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
    /// an attack or block of ours they couldn't counter (`+`, we play the
    /// ball again), or after an attack of ours blocked but covered (`!`). An attack after anything else (e.g. after an
    /// overpass of ours) still counts among the attacks, but not in the
    /// distribution and the counter-attacks.
    fn is_attack_setup(prev_type: EventTypeEnum, prev_eval: EvalEnum) -> bool {
        use EvalEnum::*;
        use EventTypeEnum::*;
        matches!(
            (prev_type, prev_eval),
            (D | P, Perfect | Positive | Exclamative | Negative)
                | (S, Over)
                | (A | B, Positive)
                | (A, Exclamative)
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
        ) && let Some(prev_eval) = prev.eval
            && Self::is_attack_setup(prev.event_type, prev_eval)
        {
            self.stats.attacks.add(AttackKey {
                phase,
                rotation,
                player,
                zone,
                eval,
                prev_type: prev.event_type,
                prev_eval,
            });
        }
        self.record_first_rally(event, rotation);
        Ok(())
    }

    /// The first rally of a side out goes from their serve to the end of
    /// our first attack; it's recorded by the event ending it:
    /// - their serve error (`OE`), their ace (`OS`) or our fault (`F`)
    ///   before the reception;
    /// - a reception error, or an overpass (the rally goes on, but our first
    ///   attack is lost);
    /// - after a reception keeping the ball, our attack (whatever its
    ///   outcome), their error or our fault.
    fn record_first_rally(&mut self, event: &EventEntry, rotation: u8) {
        use EvalEnum::*;
        use EventTypeEnum::*;
        let prev = self.last_event.as_ref().map(|e| (e.event_type, e.eval));
        let reception_eval = match (event.event_type, event.eval, prev) {
            (OE | OS | F, _, _) if self.get_serving_team() == Some(TeamSideEnum::Them) => None,
            (P, Some(Error | Over), _) => event.eval,
            (
                A | OE | F,
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
            // order: OS, OE, F, A, S, P, D, B, R, CL, CS
            ((OS, None), serve_them.clone()),
            ((OE, None), serve_us.clone()),
            ((B, Some(Error)), serve_them.clone()),
            ((B, Some(Over)), serve_them.clone()),
            ((B, Some(Perfect)), serve_us.clone()),
            ((B, Some(Positive)), vec![OE, F, A]),
            ((B, Some(Negative)), vec![OS, OE, F, A, D, B]),
            ((A, Some(Error)), serve_them.clone()),
            ((A, Some(Over)), serve_them.clone()),
            ((A, Some(Negative)), vec![OS, OE, F, A, D, B]),
            ((A, Some(Perfect)), serve_us.clone()),
            ((A, Some(Positive)), vec![OE, F, A]),
            ((A, Some(Exclamative)), vec![OE, F, A]),
            ((D, Some(Error)), serve_them.clone()),
            ((D, Some(Over)), vec![OS, OE, F, A, D, B]),
            ((D, Some(Exclamative)), vec![OE, F, A]),
            ((D, Some(Negative)), vec![OE, F, A]),
            ((D, Some(Positive)), vec![OE, F, A]),
            ((D, Some(Perfect)), vec![OE, F, A]),
            ((F, None), serve_them.clone()),
            ((P, Some(Error)), serve_them.clone()),
            ((P, Some(Over)), vec![OS, OE, F, A, D, B]),
            ((P, Some(Exclamative)), vec![OE, F, A]),
            ((P, Some(Negative)), vec![OE, F, A]),
            ((P, Some(Positive)), vec![OE, F, A]),
            ((P, Some(Perfect)), vec![OE, F, A]),
            ((S, Some(Error)), serve_them.clone()),
            ((S, Some(Perfect)), serve_us.clone()),
            ((S, Some(Positive)), vec![OS, OE, F, D, B]),
            ((S, Some(Over)), vec![OE, A, F]),
            ((S, Some(Negative)), vec![OS, OE, F, D, B]),
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
