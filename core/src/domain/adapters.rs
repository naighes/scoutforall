//! The rows of the database as the types of today and back: `TeamEntry`,
//! `MatchEntry`, `SetEntry` stay what the replay, the statistics and the
//! interfaces use (TODO/001, step 6), from A's side.
//!
//! - A team is its row, its memberships and their persons: a player
//!   removed from the team, or a person deleted, is a removed player.
//! - A match from the files gets its id, its team B and where it came
//!   from (`LegacyId`); its roster copies number and role from the team
//!   (D9), the whole active team when the match has none (up to v0.10.1).
//!   Read back, the team of the match has the numbers and roles of its
//!   roster, and plays at home only when A does (a neutral court isn't).
//! - The events of a set are A's, in their order, the undone ones left
//!   out.

use crate::{
    domain::{
        Event, Home, LegacyId, Match, MatchSet, Membership, Person, RosterEntry, ScoutingMode,
        SetLineup, Side, Team, TeamRef,
    },
    errors::{AppError, IOError},
    shapes::{r#match::MatchEntry, player::PlayerEntry, set::SetEntry, team::TeamEntry},
};
use uuid::Uuid;

fn not_consistent(message: String) -> AppError {
    AppError::IO(IOError::Msg(message))
}

/// A team with its members, in the order of its players.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamRows {
    pub row: Team,
    pub persons: Vec<Person>,
    pub members: Vec<Membership>,
}

impl TeamRows {
    pub fn of(team: &TeamEntry) -> Self {
        TeamRows {
            row: Team {
                id: team.id,
                name: team.name.clone(),
                year: team.year,
                classification: team.classification,
                gender: team.gender,
                country: team.country,
                deleted: false,
            },
            persons: team
                .players
                .iter()
                .map(|p| Person {
                    id: p.id,
                    name: p.name.clone(),
                    birth_date: p.birth_date,
                    height_cm: p.height_cm,
                    notes: p.notes.clone(),
                    deleted: false,
                })
                .collect(),
            members: team
                .players
                .iter()
                .map(|p| Membership {
                    team_id: team.id,
                    person_id: p.id,
                    number: p.number,
                    role: p.role,
                    deleted: p.deleted,
                })
                .collect(),
        }
    }

    pub fn entry(&self) -> Result<TeamEntry, AppError> {
        let players = self
            .members
            .iter()
            .map(|m| self.player(m))
            .collect::<Result<_, _>>()?;
        Ok(TeamEntry {
            name: self.row.name.clone(),
            year: self.row.year,
            players,
            id: self.row.id,
            classification: self.row.classification,
            gender: self.row.gender,
            country: self.row.country,
        })
    }

    fn player(&self, member: &Membership) -> Result<PlayerEntry, AppError> {
        let person = self.persons.iter().find(|p| p.id == member.person_id);
        match (member.team_id == self.row.id, person) {
            (true, Some(person)) => Ok(PlayerEntry {
                id: person.id,
                name: person.name.clone(),
                role: member.role,
                number: member.number,
                deleted: member.deleted || person.deleted,
                birth_date: person.birth_date,
                height_cm: person.height_cm,
                notes: person.notes.clone(),
            }),
            (false, _) => Err(not_consistent(format!(
                "member {} of team {}, not of team {}",
                member.person_id, member.team_id, self.row.id
            ))),
            (true, None) => Err(not_consistent(format!(
                "member {} of team {} without the person",
                member.person_id, self.row.id
            ))),
        }
    }
}

/// A match with the rosters of its teams and its sets.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchRows {
    pub row: Match,
    pub roster: Vec<RosterEntry>,
    pub sets: Vec<SetRows>,
}

impl MatchRows {
    /// The match of the files, as match `id` against `team_b`, scouted on
    /// A's side; `legacy` where it was.
    pub fn of(
        entry: &MatchEntry,
        id: Uuid,
        team_b: TeamRef,
        legacy: Option<LegacyId>,
    ) -> Result<Self, AppError> {
        let team = &entry.team;
        let roster: Vec<Uuid> = match entry.roster.as_slice() {
            [] => team.active_players().iter().map(|p| p.id).collect(),
            ids => ids.to_vec(),
        };
        let roster = roster
            .iter()
            .map(|person_id| {
                team.find_player(*person_id)
                    .map(|p| RosterEntry {
                        match_id: id,
                        team_id: team.id,
                        person_id: p.id,
                        number: p.number,
                        role: p.role,
                    })
                    .ok_or_else(|| {
                        not_consistent(format!(
                            "player {person_id} of the roster of match {} not in team {}",
                            entry.id, team.id
                        ))
                    })
            })
            .collect::<Result<_, _>>()?;
        let sets = entry
            .sets
            .iter()
            .map(|s| SetRows::of(s, id))
            .collect::<Result<_, _>>()?;
        Ok(MatchRows {
            row: Match {
                id,
                team_a: team.id,
                team_b,
                home: Home::from_a(entry.home),
                scouting: ScoutingMode::A,
                start: entry.date,
                duration_min: None,
                friendly: false,
                competition_id: None,
                stage: None,
                round: None,
                max_substitutions: entry.max_substitutions,
                notes: entry.notes.clone(),
                source: entry.source,
                video_clock: entry.video_clock,
                legacy,
                deleted: false,
            },
            roster,
            sets,
        })
    }

    /// The match from A's side: `team_a` its team A, `team_b` its team B
    /// when it's a team (for its name).
    pub fn entry(&self, team_a: &TeamEntry, team_b: Option<&Team>) -> Result<MatchEntry, AppError> {
        let opponent = match (&self.row.team_b, team_b) {
            (TeamRef::Name(name), _) => Ok(name.clone()),
            (TeamRef::Team(id), Some(b)) if b.id == *id => Ok(b.name.clone()),
            (TeamRef::Team(id), _) => Err(not_consistent(format!(
                "match {}: team B {id} not given",
                self.row.id
            ))),
        }?;
        let team = self.team_of_a(team_a)?;
        let mut sets: Vec<&SetRows> = self.sets.iter().filter(|s| !s.row.deleted).collect();
        sets.sort_by_key(|s| s.row.number);
        let sets = sets
            .into_iter()
            .map(SetRows::entry)
            .collect::<Result<_, _>>()?;
        let roster = team
            .players
            .iter()
            .map(|p| p.id)
            .filter(|id| self.in_roster_of_a(*id).is_some())
            .collect::<Vec<_>>();
        Ok(MatchEntry {
            opponent,
            date: self.row.start,
            id: self.row.id.to_string(),
            team: team.with_roster(&roster),
            home: self.row.home.is_a(),
            max_substitutions: self.row.max_substitutions,
            notes: self.row.notes.clone(),
            roster,
            sets,
            source: self.row.source,
            video_clock: self.row.video_clock,
        })
    }

    fn in_roster_of_a(&self, person_id: Uuid) -> Option<&RosterEntry> {
        self.roster
            .iter()
            .find(|r| r.team_id == self.row.team_a && r.person_id == person_id)
    }

    /// Team A with the numbers and roles of the roster of the match.
    fn team_of_a(&self, team_a: &TeamEntry) -> Result<TeamEntry, AppError> {
        let missing = self
            .roster
            .iter()
            .filter(|r| r.team_id == self.row.team_a)
            .find(|r| team_a.find_player(r.person_id).is_none());
        match (team_a.id == self.row.team_a, missing) {
            (false, _) => Err(not_consistent(format!(
                "match {}: team A is {}, not {}",
                self.row.id, self.row.team_a, team_a.id
            ))),
            (true, Some(r)) => Err(not_consistent(format!(
                "match {}: player {} of the roster not in team {}",
                self.row.id, r.person_id, team_a.id
            ))),
            (true, None) => Ok(TeamEntry {
                players: team_a
                    .players
                    .iter()
                    .map(|p| match self.in_roster_of_a(p.id) {
                        Some(r) => PlayerEntry {
                            number: r.number,
                            role: r.role,
                            ..p.clone()
                        },
                        None => p.clone(),
                    })
                    .collect(),
                ..team_a.clone()
            }),
        }
    }
}

/// A set with the lineups of its sides and its events.
#[derive(Debug, Clone, PartialEq)]
pub struct SetRows {
    pub row: MatchSet,
    pub lineups: Vec<SetLineup>,
    pub events: Vec<Event>,
}

impl SetRows {
    /// A set of match `match_id` from A's side: its lineup is A's, its
    /// events too, each with an id of its own.
    pub fn of(set: &SetEntry, match_id: Uuid) -> Result<Self, AppError> {
        let events = set
            .events
            .iter()
            .enumerate()
            .map(|(i, e)| {
                u32::try_from(i)
                    .map(|seq| Event {
                        id: Uuid::new_v4(),
                        match_id,
                        set_number: set.set_number,
                        seq,
                        side: Side::A,
                        entry: e.clone(),
                        deleted: false,
                    })
                    .map_err(|_| not_consistent(format!("event {i} of set {}", set.set_number)))
            })
            .collect::<Result<_, _>>()?;
        Ok(SetRows {
            row: MatchSet {
                match_id,
                number: set.set_number,
                serving: Side::from_a(set.serving_team),
                lineup_rules: set.lineup_rules,
                deleted: false,
            },
            lineups: vec![SetLineup {
                match_id,
                set_number: set.set_number,
                side: Side::A,
                positions: set.initial_positions,
                libero: set.libero,
                fallback_libero: set.fallback_libero,
                setter: set.setter,
            }],
            events,
        })
    }

    /// The set from A's side.
    pub fn entry(&self) -> Result<SetEntry, AppError> {
        let lineup = self
            .lineups
            .iter()
            .find(|l| l.side == Side::A)
            .ok_or_else(|| {
                not_consistent(format!(
                    "set {} of match {} without the lineup of A",
                    self.row.number, self.row.match_id
                ))
            })?;
        let mut events: Vec<&Event> = self
            .events
            .iter()
            .filter(|e| e.side == Side::A && !e.deleted)
            .collect();
        events.sort_by_key(|e| e.seq);
        Ok(SetEntry {
            set_number: self.row.number,
            serving_team: self.row.serving.to_a(),
            initial_positions: lineup.positions,
            libero: lineup.libero,
            fallback_libero: lineup.fallback_libero,
            setter: lineup.setter,
            lineup_rules: self.row.lineup_rules,
            events: events.into_iter().map(|e| e.entry.clone()).collect(),
        })
    }
}
