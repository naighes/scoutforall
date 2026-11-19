//! Starting a set, whatever the user interface: which set comes next, or
//! which one to resume, then who serves, the six players on court, the
//! setter among them and the liberos. Only the active players of the team
//! play: a removed player stays in the team only for the matches played.

use crate::{
    localization::Labels,
    providers::set_writer::SetWriter,
    scouting::ScoutingSession,
    shapes::{
        enums::TeamSideEnum,
        r#match::{MatchEntry, MatchStatus},
        player::PlayerEntry,
        set::SetEntry,
        team::TeamEntry,
    },
};
use std::{collections::HashSet, sync::Arc};
use uuid::Uuid;

/// The fewest active players a team needs for a fallback libero.
pub const MIN_PLAYERS_FOR_FALLBACK_LIBERO: usize = 8;

/// What scouting a match goes on with.
#[derive(Debug, Clone)]
pub enum NextSet {
    /// a new set, to start with the form
    Start(SetStartForm),
    /// the set in progress
    Resume(SetEntry),
    /// the match is over
    Over,
}

/// What scouting the match of `status`, of `team`, goes on with. A new set
/// after the first proposes the lineup the last one started with.
pub fn next_set(status: &MatchStatus, team: &TeamEntry) -> NextSet {
    match (
        status.match_finished,
        &status.last_incomplete_set,
        status.next_set_number,
        &status.last_set,
    ) {
        (false, Some(set), _, _) => NextSet::Resume(set.clone()),
        (false, None, Some(set_number), Some(last)) => NextSet::Start(
            SetStartForm::new(set_number, Some(last.serving_team)).with_lineup_of(last, team),
        ),
        (false, None, Some(set_number), None) => {
            NextSet::Start(SetStartForm::new(set_number, None))
        }
        _ => NextSet::Over,
    }
}

/// What the scout chose so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetStartForm {
    pub set_number: u8,
    pub serving_team: Option<TeamSideEnum>,
    /// the players in positions 1 to 6
    pub positions: [Option<Uuid>; 6],
    pub setter: Option<Uuid>,
    pub libero: Option<Uuid>,
    pub fallback_libero: Option<Uuid>,
}

/// A valid form, the arguments of `SetWriter::create`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetStartInput {
    pub set_number: u8,
    pub serving_team: TeamSideEnum,
    pub positions: [Uuid; 6],
    pub setter: Uuid,
    pub libero: Uuid,
    pub fallback_libero: Option<Uuid>,
    /// the start of B, for a match scouted on both sides
    pub opponent: Option<SideInput>,
}

/// The valid start of one team: lineup, setter and liberos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideInput {
    pub positions: [Uuid; 6],
    pub setter: Uuid,
    pub libero: Uuid,
    pub fallback_libero: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetStartError {
    MissingServingTeam,
    /// the empty position, from 1 to 6
    MissingPlayer(u8),
    MissingSetter,
    MissingLibero,
    /// a player not active in the team, on court twice, a setter off
    /// court, a libero on court
    InvalidPlayer,
    CouldNotStartSet,
    CouldNotComputeSnapshot,
}

impl SetStartError {
    pub fn message(&self, labels: &Labels) -> String {
        use SetStartError::*;
        match self {
            MissingServingTeam => labels.serving_team_is_required.to_string(),
            MissingPlayer(position) => labels
                .player_in_position_is_required
                .replace("{}", &position.to_string()),
            MissingSetter => labels.setter_is_required.to_string(),
            MissingLibero => labels.libero_is_required.to_string(),
            InvalidPlayer => labels.invalid_player_selection.to_string(),
            CouldNotStartSet => labels.could_not_start_set.to_string(),
            CouldNotComputeSnapshot => labels.could_not_compute_snapshot.to_string(),
        }
    }
}

impl SetStartForm {
    /// The form of set `set_number`: after the first set, the team that
    /// didn't serve first in the previous one serves.
    pub fn new(set_number: u8, last_serving_team: Option<TeamSideEnum>) -> Self {
        let serving_team = match (set_number, last_serving_team) {
            (1 | 5, _) | (_, None) => None,
            (_, Some(TeamSideEnum::Us)) => Some(TeamSideEnum::Them),
            (_, Some(TeamSideEnum::Them)) => Some(TeamSideEnum::Us),
        };
        Self {
            set_number,
            serving_team,
            positions: [None; 6],
            setter: None,
            libero: None,
            fallback_libero: None,
        }
    }

    /// The form with the lineup `set` started with: who is no longer active
    /// in `team` leaves an empty choice.
    pub fn with_lineup_of(mut self, set: &SetEntry, team: &TeamEntry) -> Self {
        let active: HashSet<Uuid> = team.active_players().iter().map(|p| p.id).collect();
        let only_active = |id: Uuid| Some(id).filter(|id| active.contains(id));
        for (position, id) in set.initial_positions.iter().enumerate() {
            self.place(position, only_active(*id));
        }
        let on_court = self.on_court();
        self.setter = only_active(set.setter).filter(|s| on_court.contains(s));
        self.libero = only_active(set.libero).filter(|l| !on_court.contains(l));
        self.fallback_libero = set.fallback_libero.and_then(only_active).filter(|f| {
            self.fallback_libero_choices(team)
                .iter()
                .any(|p| p.id == *f)
        });
        self
    }

    /// Whether the scout chooses who serves: the toss decides it in the
    /// first set and in the tie-break.
    pub fn chooses_serving_team(&self) -> bool {
        matches!(self.set_number, 1 | 5)
    }

    /// Puts `player` in `position` (0 for position 1), or empties it. A
    /// setter no longer on court and a libero now on court are dropped.
    pub fn place(&mut self, position: usize, player: Option<Uuid>) {
        if let Some(slot) = self.positions.get_mut(position) {
            *slot = player;
        }
        let on_court = self.on_court();
        self.setter = self.setter.filter(|s| on_court.contains(s));
        self.libero = self.libero.filter(|l| !on_court.contains(l));
        self.fallback_libero = self.fallback_libero.filter(|l| !on_court.contains(l));
    }

    /// The players that can go in `position`: the active ones neither in
    /// another position nor liberos.
    pub fn position_choices<'a>(
        &self,
        team: &'a TeamEntry,
        position: usize,
    ) -> Vec<&'a PlayerEntry> {
        let taken: HashSet<Uuid> = self
            .positions
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != position)
            .filter_map(|(_, p)| *p)
            .chain(self.libero)
            .chain(self.fallback_libero)
            .collect();
        team.active_players()
            .into_iter()
            .filter(|p| !taken.contains(&p.id))
            .collect()
    }

    /// The players that can be the setter: those on court, by position.
    pub fn setter_choices<'a>(&self, team: &'a TeamEntry) -> Vec<&'a PlayerEntry> {
        let active = team.active_players();
        self.positions
            .iter()
            .flatten()
            .filter_map(|id| active.iter().find(|p| p.id == *id).copied())
            .collect()
    }

    /// The players that can be the libero: the active ones off court.
    pub fn libero_choices<'a>(&self, team: &'a TeamEntry) -> Vec<&'a PlayerEntry> {
        self.off_court(team, self.fallback_libero)
    }

    /// The players that can be the fallback libero: the active ones off
    /// court, other than the libero; none when the team has fewer than
    /// `MIN_PLAYERS_FOR_FALLBACK_LIBERO` active players.
    pub fn fallback_libero_choices<'a>(&self, team: &'a TeamEntry) -> Vec<&'a PlayerEntry> {
        match team.active_players().len() {
            n if n < MIN_PLAYERS_FOR_FALLBACK_LIBERO => vec![],
            _ => self.off_court(team, self.libero),
        }
    }

    /// What starts the set. The first error, in the order of the fields,
    /// when the form isn't valid.
    pub fn validate(self, team: &TeamEntry) -> Result<SetStartInput, SetStartError> {
        self.validate_both(team, None)
    }

    /// What starts the set scouted on both sides: this form is the one of
    /// A, `opponent` the form and the team of B. Who serves is the one of
    /// this form (B's own is ignored: who doesn't serve receives). The
    /// errors of A come first, then those of B.
    pub fn validate_both(
        self,
        team: &TeamEntry,
        opponent: Option<(&SetStartForm, &TeamEntry)>,
    ) -> Result<SetStartInput, SetStartError> {
        let serving_team = self.serving_team.ok_or(SetStartError::MissingServingTeam)?;
        let ours = self.validate_side(team)?;
        let theirs = opponent
            .map(|(form, team)| form.validate_side(team))
            .transpose()?;
        Ok(SetStartInput {
            set_number: self.set_number,
            serving_team,
            positions: ours.positions,
            setter: ours.setter,
            libero: ours.libero,
            fallback_libero: ours.fallback_libero,
            opponent: theirs,
        })
    }

    /// The lineup of one team: the first error, in the order of the
    /// fields, when it isn't valid. Who serves isn't checked.
    pub fn validate_side(&self, team: &TeamEntry) -> Result<SideInput, SetStartError> {
        use SetStartError::*;
        let active: HashSet<Uuid> = team.active_players().iter().map(|p| p.id).collect();
        let on_court = self.on_court();
        let missing = self.positions.iter().position(Option::is_none);
        let positions: Option<[Uuid; 6]> = self
            .positions
            .iter()
            .copied()
            .collect::<Option<Vec<_>>>()
            .and_then(|p| p.try_into().ok());
        let lineup_valid = on_court.len() == 6 && on_court.is_subset(&active);
        let liberos_valid = self
            .libero
            .is_some_and(|l| active.contains(&l) && !on_court.contains(&l))
            && self
                .fallback_libero
                .is_none_or(|f| self.fallback_libero_choices(team).iter().any(|p| p.id == f));
        match (missing, positions, self.setter, self.libero) {
            (Some(i), _, _, _) => Err(MissingPlayer(i as u8 + 1)),
            _ if !lineup_valid => Err(InvalidPlayer),
            (_, _, None, _) => Err(MissingSetter),
            (_, _, Some(setter), _) if !on_court.contains(&setter) => Err(InvalidPlayer),
            (_, _, _, None) => Err(MissingLibero),
            _ if !liberos_valid => Err(InvalidPlayer),
            (None, Some(positions), Some(setter), Some(libero)) => Ok(SideInput {
                positions,
                setter,
                libero,
                fallback_libero: self.fallback_libero,
            }),
            _ => Err(InvalidPlayer),
        }
    }

    fn on_court(&self) -> HashSet<Uuid> {
        self.positions.iter().flatten().copied().collect()
    }

    /// The active players off court, other than `other_libero`.
    fn off_court<'a>(
        &self,
        team: &'a TeamEntry,
        other_libero: Option<Uuid>,
    ) -> Vec<&'a PlayerEntry> {
        let on_court = self.on_court();
        team.active_players()
            .into_iter()
            .filter(|p| !on_court.contains(&p.id) && Some(p.id) != other_libero)
            .collect()
    }
}

/// Saves the set that `input` starts, and scouts it. With the start of B
/// the set is scouted on both sides: B's serving team is the other one.
pub async fn start(
    current_match: MatchEntry,
    input: SetStartInput,
    set_writer: Arc<dyn SetWriter + Send + Sync>,
) -> Result<ScoutingSession, SetStartError> {
    let set = match &input.opponent {
        None => set_writer
            .create(
                &current_match,
                input.set_number,
                input.serving_team,
                input.positions,
                input.libero,
                input.fallback_libero,
                input.setter,
                vec![], // no events at the start of the set
            )
            .await
            .map_err(|_| SetStartError::CouldNotStartSet)?,
        Some(b) => {
            let a = SetEntry::new(
                input.set_number,
                input.serving_team,
                input.positions,
                input.libero,
                input.fallback_libero,
                input.setter,
            )
            .map_err(|_| SetStartError::CouldNotStartSet)?;
            let b = SetEntry::new(
                input.set_number,
                match input.serving_team {
                    TeamSideEnum::Us => TeamSideEnum::Them,
                    TeamSideEnum::Them => TeamSideEnum::Us,
                },
                b.positions,
                b.libero,
                b.fallback_libero,
                b.setter,
            )
            .map_err(|_| SetStartError::CouldNotStartSet)?;
            set_writer
                .create_both(&current_match, &a, &b)
                .await
                .map_err(|_| SetStartError::CouldNotStartSet)?;
            a
        }
    };
    resume(current_match, set, set_writer)
}

/// Scouts `set` again, from its saved events.
pub fn resume(
    current_match: MatchEntry,
    set: SetEntry,
    set_writer: Arc<dyn SetWriter + Send + Sync>,
) -> Result<ScoutingSession, SetStartError> {
    set.compute_snapshot(current_match.max_substitutions)
        .map_err(|_| SetStartError::CouldNotComputeSnapshot)
        .map(|snapshot| ScoutingSession::new(current_match, set, snapshot, set_writer))
}
