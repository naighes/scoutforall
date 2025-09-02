use crate::{
    constants::{DEFAULT_SET_TARGET_SCORE, TIE_BREAK_SET_TARGET_SCORE},
    errors::AppError,
    shapes::{
        enums::{
            ErrorTypeEnum, EvalEnum, EventTypeEnum, PhaseEnum, RoleEnum, TeamSideEnum, ZoneEnum,
        },
        lineup::Lineup,
        set::SetEntry,
        stats::Stats,
    },
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::{self, Display, Formatter},
    usize,
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
    pub pending_touch: Option<Uuid>,
}

// snapshot should be SetSnapshot, and it should guarantees set invariants
impl Snapshot {
    pub fn new(set_entry: &SetEntry) -> Result<Self, AppError> {
        let current_lineup = Lineup::new(
            set_entry.initial_positions.clone(),
            match set_entry.serving_team {
                TeamSideEnum::Us => PhaseEnum::Break,
                TeamSideEnum::Them => PhaseEnum::SideOut,
            },
            set_entry.setter.clone(),
            set_entry.libero.clone(),
        )?;
        Ok(Snapshot {
            score_us: 0,
            score_them: 0,
            stats: Stats::new(),
            current_lineup,
            last_event: None,
            pending_touch: None,
        })
    }

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

    fn set_distribution_stats(&mut self, event: &EventEntry) -> Result<(), AppError> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        let rotation = self.current_lineup.get_current_rotation()?;
        let prev_event = match self.last_event.as_ref() {
            Some(e) => e,
            None => return Ok(()),
        };
        let prev_eval = match prev_event.eval {
            Some(eval) => eval,
            None => return Ok(()),
        };
        let current_eval = match event.eval {
            Some(eval) => eval,
            None => return Ok(()),
        };
        let attack_zone = match event.player.and_then(|p| self.get_attack_zone(&p)) {
            Some(zone) => zone,
            None => return Ok(()),
        };
        if match (
            event.event_type,
            prev_event.event_type,
            prev_eval,
            current_eval,
        ) {
            (A, D | P, Perfect | Positive | Exclamative | Negative, _) => true,
            (A, A | B, Positive, _) => true,
            _ => false,
        } {
            self.stats.distribution.add(
                self.current_lineup.get_current_phase(),
                rotation,
                attack_zone,
                current_eval,
                prev_eval,
            );
        }
        Ok(())
    }

    fn has_scored(&mut self, event: &EventEntry) -> Option<TeamSideEnum> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        match (event.event_type, event.eval) {
            (OE, _) | (B | A | S, Some(Perfect)) => Some(TeamSideEnum::Us),
            (B | A, Some(Error | Over))
            | (D, Some(Error))
            | (P, Some(Error))
            | (S, Some(Error))
            | (F, _)
            | (OS, _) => Some(TeamSideEnum::Them),
            _ => None,
        }
    }

    fn set_score_stats(&mut self, event: &EventEntry) {
        match &self.has_scored(event) {
            Some(TeamSideEnum::Us) => {
                self.score_us += 1;
            }
            Some(TeamSideEnum::Them) => {
                self.score_them += 1;
            }
            _ => {}
        }
    }

    fn set_phase_count_stats(&mut self, event: &EventEntry) -> Result<(), AppError> {
        let phase = self.current_lineup.get_current_phase();
        let rotation = self.current_lineup.get_current_rotation()?;
        match &self.has_scored(event) {
            Some(TeamSideEnum::Us) => {
                self.stats.scored_points.add(phase, rotation);
                self.stats.phases.add(phase, rotation);
            }
            Some(TeamSideEnum::Them) => {
                self.stats.phases.add(phase, rotation);
            }
            _ => {}
        };
        Ok(())
    }

    fn set_pending_touch(&mut self, event: &EventEntry) {
        use EventTypeEnum::*;
        match (event.event_type, event.player) {
            (D | P, Some(_)) => {
                self.pending_touch = event.player;
            }
            _ => {
                self.pending_touch = None;
            }
        }
    }

    pub fn get_serving_team(&self) -> Option<TeamSideEnum> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        use PhaseEnum::*;
        use TeamSideEnum::*;
        match (
            self.last_event.as_ref().map(|e| e.event_type),
            self.last_event.as_ref().and_then(|e| e.eval),
            self.current_lineup.get_current_phase(),
        ) {
            (None, _, Break) | (Some(OE), _, _) | (Some(B | A | S), Some(Perfect), _) => Some(Us),
            (None, _, SideOut)
            | (Some(B | A), Some(Error | Over), _)
            | (Some(D | P | S), Some(Error), _)
            | (Some(F | OS), _, _) => Some(Them),
            _ => None,
        }
    }

    fn set_possessions_stats(&mut self, event: &EventEntry) -> Result<(), AppError> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        use TeamSideEnum::*;
        let us_was_serving = self.get_serving_team() == Some(Us);
        let them_was_serving = self.get_serving_team() == Some(Them);
        let rotation = self.current_lineup.get_current_rotation()?;
        match (
            event.event_type,
            event.eval,
            them_was_serving,
            us_was_serving,
        ) {
            (P | D | S, _, _, _)
            | (A | B, Some(Positive), _, _) // TODO: should perfect block and perfect attack count as possession?
            | (OS, _, true, _)
            | (F, _, _, true) => Ok(self.stats.possessions.add(
                self.current_lineup.get_current_phase(),
                rotation,
            )),
            _ => Ok(())
        }
    }

    fn set_opponent_errors_stats(&mut self, event: &EventEntry) -> Result<(), AppError> {
        let rotation = self.current_lineup.get_current_rotation()?;
        if event.event_type == EventTypeEnum::OE {
            Ok(self
                .stats
                .opponent_errors
                .add(self.current_lineup.get_current_phase(), rotation))
        } else {
            Ok(())
        }
    }

    fn set_errors_stats(&mut self, event: &EventEntry) -> Result<(), AppError> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        let rotation = self.current_lineup.get_current_rotation()?;
        let error_type = match (event.event_type, event.eval) {
            (A, Some(Error)) | (S, Some(Error)) | (B, Some(Over)) | (F, _) => {
                Some(ErrorTypeEnum::Unforced)
            }
            (A, Some(Over)) | (B, Some(Error)) | (P, Some(Error)) | (D, Some(Error)) => {
                Some(ErrorTypeEnum::Forced)
            }
            _ => None,
        };
        match (error_type, event.player) {
            (Some(err), Some(player)) => Ok(self.stats.errors.add(
                self.current_lineup.get_current_phase(),
                rotation,
                player,
                err,
            )),
            _ => Ok(()),
        }
    }

    fn set_points_stats(&mut self, event: &EventEntry) -> Result<(), AppError> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        use TeamSideEnum::*;
        let them_was_serving = self.get_serving_team() == Some(Them);
        let rotation = self.current_lineup.get_current_rotation()?;
        if matches!(
            (event.event_type, event.eval, them_was_serving,),
            (OE, _, false) | (B, Some(Perfect), _) | (A, Some(Perfect), _) | (S, Some(Perfect), _)
        ) {
            Ok(self
                .stats
                .earned_points
                .add(self.current_lineup.get_current_phase(), rotation))
        } else {
            Ok(())
        }
    }

    fn set_events_stats(&mut self, event: &EventEntry) -> Result<(), AppError> {
        use EventTypeEnum::*;
        let zone = event.player.and_then(|p| self.get_attack_zone(&p));
        let rotation = self.current_lineup.get_current_rotation()?;
        match (event.event_type, event.eval, event.player, zone) {
            (B | D | P | S, Some(ev), Some(player), _) => Ok(self.stats.events.add(
                event.event_type,
                self.current_lineup.get_current_phase(),
                rotation,
                Some(player),
                None,
                Some(ev),
            )),
            (A, Some(ev), Some(player), Some(z)) => Ok(self.stats.events.add(
                event.event_type,
                self.current_lineup.get_current_phase(),
                rotation,
                Some(player),
                Some(z),
                Some(ev),
            )),
            _ => Ok(()),
        }
    }

    fn get_available_options(
        &self,
        event: &EventEntry,
        current_available_options: Vec<EventTypeEnum>,
    ) -> Vec<EventTypeEnum> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        let serve_them = vec![OE, OS, F, P, R];
        let serve_us = vec![OE, F, S, R];
        let options_map: HashMap<_, _> = [
            // order: OS, OE, F, A, S, P, D, B, R
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
            (R, _) => current_available_options,
            key => options_map.get(&key).cloned().unwrap_or_default(),
        }
    }

    fn set_counter_attack_stats(&mut self, event: &EventEntry) -> Result<(), AppError> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        let rotation = self.current_lineup.get_current_rotation()?;
        match (
            event.event_type,
            &self.last_event,
            event.player.and_then(|p| self.get_attack_zone(&p)),
        ) {
            (A, Some(last_event), Some(zone)) => match (
                last_event.event_type,
                last_event.eval,
                &event.player,
                event.eval,
            ) {
                // TODO: block and attack? just dig?
                (D, Some(Perfect | Positive | Negative | Exclamative), Some(p), Some(ev)) => {
                    Ok(self.stats.counter_attack.add(
                        self.current_lineup.get_current_phase(),
                        rotation,
                        *p,
                        zone,
                        ev,
                    ))
                }
                _ => Ok(()),
            },
            _ => Ok(()),
        }
    }

    fn set_attack_stats(&mut self, event: &EventEntry) -> Result<(), AppError> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        let rotation = self.current_lineup.get_current_rotation()?;
        if let Some(prev_eval) = self.last_event.as_ref().and_then(|e| e.eval) {
            match (
                event.event_type,
                self.last_event.as_ref().map(|e| e.event_type), // prev event type
                prev_eval,                                      // prev eval
                event.eval,                                     // current eval
                event.player,                                   // current player
                event.player.and_then(|p| self.get_attack_zone(&p)), // zone
            ) {
                (
                    A,
                    Some(D | P),
                    Exclamative | Negative | Perfect | Positive,
                    Some(eval),
                    Some(player),
                    Some(zone),
                )
                | (A, Some(S), Over, Some(eval), Some(player), Some(zone))
                | (A, Some(A | B), Positive, Some(eval), Some(player), Some(zone)) => {
                    Ok(self.stats.attack.add(
                        self.current_lineup.get_current_phase(),
                        rotation,
                        player,
                        zone,
                        eval,
                        prev_eval,
                    ))
                }
                _ => Ok(()),
            }
        } else {
            Ok(())
        }
    }

    pub fn add_event(
        &mut self,
        event: &EventEntry,
        current_available_options: Vec<EventTypeEnum>,
    ) -> Result<Vec<EventTypeEnum>, AppError> {
        self.set_score_stats(&event);
        self.set_phase_count_stats(&event)?;
        self.set_pending_touch(&event);
        self.set_possessions_stats(&event)?;
        self.set_opponent_errors_stats(&event)?;
        self.set_errors_stats(&event)?;
        self.set_points_stats(&event)?;
        self.set_events_stats(&event)?;
        self.set_counter_attack_stats(&event)?;
        self.set_attack_stats(&event)?;
        self.set_distribution_stats(event)?;
        let available_options = self.get_available_options(event, current_available_options);
        if event.event_type == EventTypeEnum::R {
            // replace lineup entry
            if let (Some(replaced), Some(replacement)) = (event.player, event.target_player) {
                self.current_lineup
                    .add_substitution(&replaced, &replacement)?;
            }
        }
        self.current_lineup.update(&event)?;
        if event.event_type != EventTypeEnum::R {
            self.last_event = Some(event.clone());
        }
        Ok(available_options)
    }
}
