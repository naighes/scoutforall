use crate::{
    constants::MAX_SUBSTITUTIONS,
    errors::{AppError, SnapshotError},
    shapes::{
        enums::{EvalEnum, EventTypeEnum, PhaseEnum, RoleEnum},
        player::PlayerEntry,
        snapshot::EventEntry,
        team::TeamEntry,
    },
};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SubstitutionRecord {
    pub replacement: Uuid, // who's in
    pub replaced: Uuid,    // who's out
}

#[derive(Debug, Clone)]
pub struct Lineup {
    players: [Uuid; 6],
    phase: PhaseEnum,
    previous_phase: Option<PhaseEnum>,
    current_setter: Uuid,
    current_libero: Uuid,
    fallback_libero: Option<Uuid>,
    substitutions: Vec<SubstitutionRecord>,
    libero_replacement: Option<Uuid>,
    idle_player: Option<Uuid>,
}

impl Lineup {
    pub fn new(
        players: [Uuid; 6],
        phase: PhaseEnum,
        current_setter: Uuid,
        current_libero: Uuid,
        fallback_libero: Option<Uuid>,
    ) -> Result<Lineup, AppError> {
        let rotation = Lineup::get_rotation(players, &current_setter)?;
        let libero_slot = Lineup::get_libero_slot(rotation)?;
        // libero_replacement will be the next player replacing the libero
        let libero_replacement = players[(libero_slot + 3) % 6];
        // idle_player is the player replaced by libero
        let idle_player = players[libero_slot];
        let has_libero = Lineup::has_libero(rotation, phase);
        let mut result = Lineup {
            players,
            phase,
            previous_phase: None,
            substitutions: vec![],
            current_setter,
            current_libero,
            fallback_libero,
            libero_replacement: if has_libero {
                Some(libero_replacement)
            } else {
                None
            },
            idle_player: if has_libero { Some(idle_player) } else { None },
        };
        if has_libero {
            // set is started: put the libero in
            result.try_set(&current_libero, libero_slot)?;
        }
        Ok(result)
    }

    pub fn get_rotation(players: [Uuid; 6], current_setter: &Uuid) -> Result<u8, AppError> {
        players
            .iter()
            .position(|p| p == current_setter)
            .map(|x| x as u8)
            .ok_or_else(|| {
                AppError::Snapshot(SnapshotError::LineupError(
                    "could not get the current rotation".to_string(),
                ))
            })
    }

    pub fn get_current_rotation(&self) -> Result<u8, AppError> {
        Lineup::get_rotation(self.players, &self.current_setter)
    }

    fn get_next_phase(&mut self, event: &EventEntry) -> Option<PhaseEnum> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        use PhaseEnum::*;
        match (event.event_type, event.eval, self.get_current_phase()) {
            (OS | F, _, Break) => Some(SideOut),
            (OE, _, SideOut) => Some(Break),
            (S, Some(Error), _) => Some(SideOut),
            (B | A, Some(Perfect), SideOut) => Some(Break),
            (B | A, Some(Error | Over), Break) => Some(SideOut),
            (D, Some(Error), Break) => Some(SideOut),
            _ => None,
        }
    }

    pub fn update(&mut self, event: &EventEntry) -> Result<(), AppError> {
        self.update_phase(event);
        self.update_libero()
    }

    fn update_libero(&mut self) -> Result<(), AppError> {
        let rotation = self.get_current_rotation()?;
        let libero_slot = Lineup::get_libero_slot(rotation)?;
        let player_at_libero_slot = self.get(libero_slot);
        match (
            self.phase,
            rotation,
            self.idle_player,
            self.libero_replacement,
            player_at_libero_slot,
        ) {
            (PhaseEnum::Break, 1, Some(idle), Some(replacement), _)
            | (PhaseEnum::Break, 4, Some(idle), Some(replacement), _) => {
                // put replacement to serve
                self.try_set(&replacement, libero_slot)?;
                // put idle player in
                self.try_set(&idle, (libero_slot + 3) % 6)?;
                // ...which is also the next replacement for libero
                self.libero_replacement = Some(idle);
                // no idle player
                self.idle_player = None;
            }
            (PhaseEnum::SideOut, 1, None, _, Some(player_at_libero_slot))
            | (PhaseEnum::SideOut, 4, None, _, Some(player_at_libero_slot)) => {
                // player who served pulled out
                self.idle_player = Some(player_at_libero_slot);
                // ...so replace this player with the libero
                let libero = self.current_libero;
                self.try_set(&libero, libero_slot)?;
            }
            _ => {}
        };
        Ok(())
    }

    pub fn swap_libero(&mut self) -> Result<(), AppError> {
        if let Some(fallback) = self.fallback_libero {
            let old_current = self.current_libero;
            self.current_libero = fallback;
            self.fallback_libero = Some(old_current);
            let has_libero = self
                .get_current_rotation()
                .map(|r| Lineup::has_libero(r, self.get_current_phase()));
            let libero_slot = self
                .get_current_rotation()
                .and_then(Lineup::get_libero_slot);
            let libero = self.current_libero;
            if let (Ok(true), Ok(slot)) = (has_libero, libero_slot) {
                self.try_set(&libero, slot)?;
            }
        }
        Ok(())
    }

    pub fn get_fallback_libero(&self) -> Option<Uuid> {
        self.fallback_libero
    }

    pub fn set_current_setter(&mut self, new_setter: &Uuid) -> Result<(), AppError> {
        if self.find_position(new_setter).is_some() {
            self.current_setter = *new_setter;
            Ok(())
        } else {
            Err(AppError::Snapshot(SnapshotError::LineupError(
                "could not find the new setter in the lineup".to_string(),
            )))
        }
    }

    fn update_phase(&mut self, event: &EventEntry) {
        if let Some(next_phase) = self.get_next_phase(event) {
            if self.phase == PhaseEnum::SideOut && next_phase == PhaseEnum::Break {
                self.rotate_clockwise();
            }
            self.previous_phase = Some(self.phase);
            self.phase = next_phase;
        }
    }

    pub fn get_current_phase(&self) -> PhaseEnum {
        self.phase
    }

    pub fn get_current_libero(&self) -> Uuid {
        self.current_libero
    }

    pub fn find_position(&self, player_id: &Uuid) -> Option<usize> {
        self.players.iter().position(|id| id == player_id)
    }

    fn try_set(&mut self, player_id: &Uuid, position: usize) -> Result<(), AppError> {
        if position < self.players.len() {
            self.players[position] = *player_id;
            Ok(())
        } else {
            Err(AppError::Snapshot(SnapshotError::LineupError(format!(
                "invalid position {}",
                position
            ))))
        }
    }

    fn rotate_clockwise(&mut self) {
        let first = self.players[0];
        for i in 0..5 {
            self.players[i] = self.players[i + 1];
        }
        self.players[5] = first;
    }

    pub fn is_back_row_player(&self, player_id: &Uuid) -> bool {
        if let Some((index, _)) = self
            .players
            .iter()
            .enumerate()
            .find(|(_, id)| *id == player_id)
        {
            // back row
            index == 0 || index == 4 || index == 5
        } else {
            false
        }
    }

    pub fn get_role(&self, player_id: &Uuid) -> Result<RoleEnum, AppError> {
        let not_found_error = || {
            AppError::Snapshot(SnapshotError::LineupError(
                "could not get the role: player not found".to_string(),
            ))
        };
        let setter_index = self
            .find_position(&self.current_setter)
            .ok_or_else(not_found_error)?;
        let player_index = self.find_position(player_id).ok_or_else(not_found_error)?;
        let player = self.players.get(player_index).ok_or_else(not_found_error)?;
        let offset = (player_index + 6 - setter_index) % 6;
        let rotation = self.get_current_rotation().map_err(|_| {
            AppError::Snapshot(SnapshotError::LineupError(
                "could not get the current rotation".to_string(),
            ))
        })?;
        let role = match offset {
            0 => RoleEnum::Setter,
            1 | 4 => RoleEnum::OutsideHitter,
            3 => RoleEnum::OppositeHitter,
            2 | 5 => {
                if Lineup::has_libero(rotation, self.phase) && self.is_back_row_player(player) {
                    RoleEnum::Libero
                } else {
                    RoleEnum::MiddleBlocker
                }
            }
            _ => {
                return Err(AppError::Snapshot(SnapshotError::LineupError(
                    "invalid offset in rotation".to_string(),
                )))
            }
        };
        Ok(role)
    }

    pub fn get_oh2(&self) -> Option<Uuid> {
        self.get_role_from_offset(4)
    }

    pub fn get_oh1(&self) -> Option<Uuid> {
        self.get_role_from_offset(1)
    }

    pub fn get_mb1(&self) -> Option<Uuid> {
        match self.get_role_from_offset(5) {
            None => None,
            Some(id) => match (id == self.current_libero, self.idle_player) {
                (true, Some(mb)) => Some(mb),
                (false, _) => Some(id),
                _ => None,
            },
        }
    }

    pub fn get_mb2(&self) -> Option<Uuid> {
        match self.get_role_from_offset(2) {
            None => None,
            Some(id) => match (id == self.current_libero, self.idle_player) {
                (true, Some(mb)) => Some(mb),
                (false, _) => Some(id),
                _ => None,
            },
        }
    }

    pub fn get_opposite(&self) -> Option<Uuid> {
        self.get_role_from_offset(3)
    }

    pub fn get_setter(&self) -> Option<Uuid> {
        self.get_role_from_offset(6)
    }

    fn get_role_from_offset(&self, offset_from_setter: usize) -> Option<Uuid> {
        let setter_index = self
            .players
            .iter()
            .position(|id| *id == self.current_setter)?;
        let index = (setter_index + offset_from_setter) % 6;
        self.get(index)
    }

    pub fn get_serving_player(&self) -> Option<Uuid> {
        let serving_index = 0; // the player in position 1 (index 0) is always the server
        self.get(serving_index)
    }

    pub fn get(&self, index: usize) -> Option<Uuid> {
        self.players.get(index).copied()
    }

    fn get_libero_slot(rotation: u8) -> Result<usize, AppError> {
        match rotation {
            0 => Ok(5),
            1 => Ok(0),
            2 => Ok(4),
            3 => Ok(5),
            4 => Ok(0),
            5 => Ok(4),
            _ => Err(AppError::Snapshot(SnapshotError::LineupError(
                "could not fimd a libero slot".to_string(),
            ))),
        }
    }

    fn has_libero(rotation: u8, phase: PhaseEnum) -> bool {
        phase != PhaseEnum::Break || (rotation != 1 && rotation != 4)
    }

    pub fn get_involved_players(&self) -> Vec<Uuid> {
        let mut set: HashSet<Uuid> = self.players.iter().cloned().collect();
        set.insert(self.current_libero);
        if let Some(libero_replacement) = self.libero_replacement {
            set.insert(libero_replacement);
        }
        if let Some(idle_player) = self.idle_player {
            set.insert(idle_player);
        }
        set.extend(
            self.substitutions
                .iter()
                .flat_map(|s| [s.replacement, s.replaced]),
        );
        set.into_iter().collect()
    }

    pub fn has_setter_at_pos(&self, position: usize) -> bool {
        if let Some(player) = self.players.get(position) {
            *player == self.current_setter
        } else {
            false
        }
    }

    pub fn has_libero_at_pos(&self, position: usize) -> bool {
        if let Some(player) = self.players.get(position) {
            *player == self.current_libero
        } else {
            false
        }
    }

    /* substitutions */

    #[cfg(test)]
    pub fn get_substitutions(&self) -> Vec<SubstitutionRecord> {
        self.substitutions.clone()
    }

    fn was_player_already_replaced(&self, player_id: &Uuid) -> bool {
        self.substitutions.iter().any(|s| s.replaced == *player_id)
    }

    fn was_player_already_used(&self, player_id: &Uuid) -> bool {
        self.substitutions
            .iter()
            .any(|s| s.replacement == *player_id)
    }

    fn get_enforced_replacement(&self, replaced: &Uuid) -> Option<Uuid> {
        self.substitutions
            .iter()
            .find(|p| p.replacement == *replaced)
            .map(|p| p.replaced)
    }

    fn was_max_number_of_substitutions_reached(&self) -> bool {
        self.substitutions.len() == MAX_SUBSTITUTIONS
    }

    pub fn add_substitution(
        &mut self,
        replaced: &Uuid,
        replacement: &Uuid,
    ) -> Result<(), AppError> {
        if let Some(pos) = self.find_position(replaced) {
            // check if already replaced previously
            if self.was_player_already_replaced(replaced) {
                return Err(AppError::Snapshot(SnapshotError::LineupError(format!(
                    "player {:?} was already replaced",
                    replaced
                ))));
            }
            // check if the replacement was already used
            if self.was_player_already_used(replacement) {
                return Err(AppError::Snapshot(SnapshotError::LineupError(format!(
                    "player {:?} was already a replacement",
                    replacement
                ))));
            }
            // libero cannot be replaced
            if self.get_current_libero() == *replaced {
                return Err(AppError::Snapshot(SnapshotError::LineupError(
                    "cannot replace the libero player".to_string(),
                )));
            }
            // check if max substitutions count was reached
            if self.was_max_number_of_substitutions_reached() {
                return Err(AppError::Snapshot(SnapshotError::LineupError(
                    "max number of substitutions was reached".to_string(),
                )));
            }
            // closed change
            match self.get_enforced_replacement(replaced) {
                None => {}
                Some(enforced) => {
                    if enforced != *replacement {
                        return Err(AppError::Snapshot(SnapshotError::LineupError(format!(
                            "player {:?} can be only replaced by player {:?}",
                            replaced, enforced,
                        ))));
                    }
                }
            }

            self.try_set(replacement, pos)?;
            // add entry into substitutions
            self.substitutions.push(SubstitutionRecord {
                replaced: *replaced,
                replacement: *replacement,
            });
            // refresh current_setter if needed
            if self.current_setter == *replaced {
                self.current_setter = *replacement;
            }
            Ok(())
        } else {
            Err(AppError::Snapshot(SnapshotError::LineupError(format!(
                "could not find player {} in the lineup",
                replaced
            ))))
        }
    }

    pub fn get_lineup_choices(&self) -> Vec<(u8, (String, Option<Uuid>))> {
        let lineup: Vec<Option<Uuid>> = vec![
            self.get_setter(),
            self.get_oh1(),
            self.get_mb2(),
            self.get_opposite(),
            self.get_oh2(),
            self.get_mb1(),
            Some(self.get_current_libero()),
        ];
        lineup
            .into_iter()
            .enumerate()
            .filter_map(|(i, id)| {
                id.map(|uuid| {
                    (
                        i as u8,
                        match i {
                            0 => ("setter".to_string(), Some(uuid)),
                            1 => ("outside hitter 1".to_string(), Some(uuid)),
                            2 => ("middle blocker 2".to_string(), Some(uuid)),
                            3 => ("opposite".to_string(), Some(uuid)),
                            4 => ("outside hitter 2".to_string(), Some(uuid)),
                            5 => ("middle blocker 1".to_string(), Some(uuid)),
                            6 => ("libero".to_string(), Some(uuid)),
                            _ => ("unknown".to_string(), None),
                        },
                    )
                })
            })
            .map(|(i, (role, id))| (i + 1, (role, id)))
            .collect()
    }

    pub fn get_replaceable_lineup_choices(&self) -> Vec<(u8, (String, Option<Uuid>))> {
        if self.substitutions.len() >= MAX_SUBSTITUTIONS {
            return vec![];
        }
        let already_replaced: HashSet<Uuid> =
            self.substitutions.iter().map(|s| s.replaced).collect();
        let options: Vec<(u8, (String, Option<Uuid>))> = self
            .get_lineup_choices()
            .iter()
            // libero is not replaceable
            .filter(|(_, (_, id))| {
                if let Some(id) = id {
                    *id != self.current_libero
                } else {
                    false
                }
            })
            .cloned()
            .collect();
        options
            .into_iter()
            // pull out already replaced players
            .filter(|(_, (_, id))| {
                if let Some(id) = id {
                    !already_replaced.contains(id)
                } else {
                    false
                }
            })
            .collect()
    }

    pub fn get_available_replacements<'a>(
        &self,
        team: &'a TeamEntry,
        replaced_id: Uuid,
    ) -> Vec<(u8, &'a PlayerEntry)> {
        // current lineup
        let options: Vec<Uuid> = [
            self.get_setter(),
            self.get_oh1(),
            self.get_mb2(),
            self.get_opposite(),
            self.get_oh2(),
            self.get_mb1(),
        ]
        .iter()
        .filter_map(|x| *x)
        .collect();
        // all substitutions involving replaced_id
        let subs: Vec<&SubstitutionRecord> = self
            .substitutions
            .iter()
            .filter(|s| s.replaced == replaced_id || s.replacement == replaced_id)
            .collect();
        match subs.len() {
            1 => {
                // still open
                let s = subs[0];
                if s.replacement == replaced_id {
                    if let Some(player) = team.players.iter().find(|p| p.id == s.replaced) {
                        return vec![(1, player)];
                    }
                }
                // should not happen
                return vec![];
            }
            2 => {
                // closed change
                return vec![];
            }
            _ => {
                // never replaced
            }
        }
        // already used in substitutions
        let excluded: HashSet<Uuid> = self
            .substitutions
            .iter()
            .flat_map(|s| vec![s.replaced, s.replacement])
            .collect();
        team.players
            .iter()
            .filter(|p| {
                !options.contains(&p.id)
                    && !excluded.contains(&p.id)
                    && p.id != self.get_current_libero()
            })
            .enumerate()
            .map(|(i, p)| ((i + 1) as u8, p))
            .collect()
    }
}
