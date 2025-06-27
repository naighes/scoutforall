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
use std::{collections::HashSet, usize};
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
    ) -> Result<Lineup, AppError> {
        let rotation = Lineup::get_rotation(players, &current_setter);
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

    pub fn get_rotation(players: [Uuid; 6], current_setter: &Uuid) -> u8 {
        players
            .iter()
            .position(|p| p == current_setter)
            .map(|x| x as u8)
            .expect("critical error") // TODO: may be we don't want to handle the error this way
    }

    pub fn get_current_rotation(&self) -> u8 {
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
        // TODO: test substitutions carefully
        let libero_slot = Lineup::get_libero_slot(self.get_current_rotation())?;
        match (
            self.phase,
            self.get_current_rotation(),
            self.idle_player,
            self.libero_replacement,
        ) {
            (PhaseEnum::Break, 1, Some(idle), Some(replacement))
            | (PhaseEnum::Break, 4, Some(idle), Some(replacement)) => {
                // put replacement to serve
                self.try_set(&replacement, libero_slot)?;
                // put idle player in
                self.try_set(&idle, (libero_slot + 3) % 6)?;
                // ...which is also the next replacement for libero
                self.libero_replacement = Some(idle);
                // no idle player
                self.idle_player = None;
            }
            (PhaseEnum::SideOut, 1, None, _) | (PhaseEnum::SideOut, 4, None, _) => {
                // player who served pulled out
                self.idle_player = Some(self.get(libero_slot as u8));
                // ...so replace this player with the libero
                let libero = self.current_libero;
                self.try_set(&libero, libero_slot)?;
            }
            _ => {}
        };
        Ok(())
    }

    fn update_phase(&mut self, event: &EventEntry) {
        if let Some(next_phase) = self.get_next_phase(&event) {
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

    pub fn add_substitution(
        &mut self,
        replaced: &Uuid,
        replacement: &Uuid,
    ) -> Result<(), AppError> {
        if let Some(pos) = self.find_position(replaced) {
            // TODO: handle error!
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
        }
        Ok(())
    }

    pub fn get_substitutions(&self) -> Vec<SubstitutionRecord> {
        self.substitutions.clone()
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

    pub fn get_role(&self, player_id: &Uuid) -> RoleEnum {
        let setter_index = self
            .players
            .iter()
            .position(|id| *id == self.current_setter)
            .expect("setter not found in current lineup");
        let player_index = self
            .players
            .iter()
            .position(|id| *id == *player_id)
            .expect("player not found in current lineup");
        let offset = (player_index + 6 - setter_index) % 6;
        match offset {
            0 => RoleEnum::Setter,
            1 => RoleEnum::OutsideHitter,
            2 => RoleEnum::MiddleBlocker,
            3 => RoleEnum::OppositeHitter,
            4 => RoleEnum::OutsideHitter,
            5 => RoleEnum::MiddleBlocker,
            _ => unreachable!(),
        }
    }

    pub fn get_oh2(&self) -> Uuid {
        self.get_role_from_offset(4)
    }

    pub fn get_oh1(&self) -> Uuid {
        self.get_role_from_offset(1)
    }

    pub fn get_mb1(&self) -> Uuid {
        let id = self.get_role_from_offset(5);
        match (id == self.current_libero, self.idle_player) {
            (true, Some(mb)) => mb,
            (false, _) => id,
            _ => panic!("oh my"), // TODO: ERROR!
        }
    }

    pub fn get_mb2(&self) -> Uuid {
        let id = self.get_role_from_offset(2);
        match (id == self.current_libero, self.idle_player) {
            (true, Some(mb)) => mb,
            (false, _) => id,
            _ => id, // TODO: ERROR!
        }
    }

    pub fn get_opposite(&self) -> Uuid {
        self.get_role_from_offset(3)
    }

    pub fn get_setter(&self) -> Uuid {
        self.get_role_from_offset(6)
    }

    fn get_role_from_offset(&self, offset_from_setter: usize) -> Uuid {
        let setter_index = self
            .players
            .iter()
            .position(|id| *id == self.current_setter)
            .expect("setter not found in current lineup");
        let index = (setter_index + offset_from_setter) % 6;
        self.players[index]
    }

    pub fn get_serving_player(&self) -> Uuid {
        let serving_index = 0; // the player in position 1 (index 0) is always the server
        self.players[serving_index]
    }

    // TODO: not safe!
    pub fn get(&self, index: u8) -> Uuid {
        self.players[index as usize]
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

    pub fn get_repleceable_lineup(&self) -> Vec<(u8, (&'static str, Uuid))> {
        if self.substitutions.len() >= MAX_SUBSTITUTIONS {
            return vec![];
        }
        let options: Vec<(u8, (&'static str, Uuid))> = vec![
            (1, ("setter", self.get_setter())),
            (2, ("outside hitter 1", self.get_oh1())),
            (3, ("middle blocker 2", self.get_mb2())),
            (4, ("opposite", self.get_opposite())),
            (5, ("outside hitter 2", self.get_oh2())),
            (6, ("middle blocker 1", self.get_mb1())),
        ];
        let already_replaced: HashSet<Uuid> =
            self.substitutions.iter().map(|s| s.replaced).collect();
        options
            .into_iter()
            .filter(|(_, (_, id))| !already_replaced.contains(id))
            .collect()
    }

    pub fn get_available_replacements<'a>(
        &self,
        team: &'a TeamEntry,
        replaced_id: Uuid,
    ) -> Vec<(u8, &'a PlayerEntry)> {
        // current lineup
        let options: Vec<Uuid> = vec![
            self.get_setter(),
            self.get_oh1(),
            self.get_mb2(),
            self.get_opposite(),
            self.get_oh2(),
            self.get_mb1(),
        ];
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
}
