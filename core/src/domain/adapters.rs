//! The rows of the database as the types of today and back: `TeamEntry`,
//! `MatchEntry`, `SetEntry` stay what the replay, the statistics and the
//! interfaces use (TODO/001, step 6), from A's side.
//!
//! - A team is its row, its memberships and their persons: a player
//!   removed from the team, or a person deleted, is a removed player; the
//!   players are in the order of the places of their memberships. A player
//!   removed from the team shows only what the matches show (name, number,
//!   role), as `player_form::removed`: the person keeps the rest.
//! - A match from the files gets its id, its team B and where it came
//!   from (`LegacyId`); its roster copies number and role from the team
//!   (D9), the whole active team when the match has none (up to v0.10.1).
//!   Read back, the team of the match has the numbers and roles of its
//!   roster, and plays at home only when A does (a neutral court isn't).
//! - The events of a set are A's, in their order, the undone ones left
//!   out.
//! - The video refers to an event by its id; the profile of today by the
//!   set and the index among the events of the set (`EventRef`), as
//!   `SetEntry` has them. An event the profile refers to that isn't one
//!   of those (a typo, one undone since) isn't written; read back, a
//!   reference to an event no longer among them (undone, of a removed set,
//!   of B) is left out: the anchor keeps its time, the correction goes.

use crate::{
    domain::{
        Event, Home, LegacyId, Match, MatchSet, MatchVideo, MatchVideoFile, Membership, Person,
        RosterEntry, ScoutingMode, SetLineup, Side, Team, TeamRef, VideoAnchor, VideoCorrection,
    },
    errors::{AppError, IOError},
    player_form::removed,
    shapes::{r#match::MatchEntry, player::PlayerEntry, set::SetEntry, team::TeamEntry},
    video::{Anchor, Correction, EventRef, VIDEO_PROFILE_VERSION, VideoFile, VideoProfile},
};
use std::{collections::BTreeMap, num::NonZeroI64};
use uuid::Uuid;

fn not_consistent(message: String) -> AppError {
    AppError::IO(IOError::Msg(message))
}

/// A team with its members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamRows {
    pub row: Team,
    pub persons: Vec<Person>,
    pub members: Vec<Membership>,
}

impl TeamRows {
    pub fn of(team: &TeamEntry) -> Result<Self, AppError> {
        let members = team
            .players
            .iter()
            .enumerate()
            .map(|(i, p)| {
                u16::try_from(i)
                    .map(|position| Membership {
                        team_id: team.id,
                        person_id: p.id,
                        position,
                        number: p.number,
                        role: p.role,
                        deleted: p.deleted,
                    })
                    .map_err(|_| not_consistent(format!("player {i} of team {}", team.id)))
            })
            .collect::<Result<_, _>>()?;
        Ok(TeamRows {
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
            members,
        })
    }

    pub fn entry(&self) -> Result<TeamEntry, AppError> {
        let mut members: Vec<&Membership> = self.members.iter().collect();
        members.sort_by_key(|m| m.position);
        let players = members
            .into_iter()
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
            })
            .map(|player| match member.deleted {
                // the person keeps them, for the other teams
                true => removed(&player),
                false => player,
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
        let events = self.events_of_a();
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

    /// The events of A not undone, in their order: those of `SetEntry`.
    pub fn events_of_a(&self) -> Vec<&Event> {
        let mut events: Vec<&Event> = self
            .events
            .iter()
            .filter(|e| e.side == Side::A && !e.deleted)
            .collect();
        events.sort_by_key(|e| e.seq);
        events
    }
}

/// The events of the sets of a match as `SetEntry` has them, by
/// `EventRef` and by id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventsOfA {
    by_ref: BTreeMap<(u8, usize), Uuid>,
    by_id: BTreeMap<Uuid, EventRef>,
}

impl EventsOfA {
    pub fn of(sets: &[SetRows]) -> Self {
        let pairs: Vec<(EventRef, Uuid)> = sets
            .iter()
            .filter(|s| !s.row.deleted)
            .flat_map(|s| {
                s.events_of_a()
                    .into_iter()
                    .enumerate()
                    .map(|(index, e)| {
                        (
                            EventRef {
                                set_number: s.row.number,
                                index,
                            },
                            e.id,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        EventsOfA {
            by_ref: pairs
                .iter()
                .map(|(r, id)| ((r.set_number, r.index), *id))
                .collect(),
            by_id: pairs.into_iter().map(|(r, id)| (id, r)).collect(),
        }
    }

    pub fn id(&self, event: EventRef) -> Option<Uuid> {
        self.by_ref.get(&(event.set_number, event.index)).copied()
    }

    pub fn event_ref(&self, id: Uuid) -> Option<EventRef> {
        self.by_id.get(&id).copied()
    }
}

/// The video of a match with its files and its corrections: the
/// corrections in the order of the ids of their events (a set: the order
/// of the profile doesn't count).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoRows {
    pub row: MatchVideo,
    pub files: Vec<MatchVideoFile>,
    pub corrections: Vec<VideoCorrection>,
}

impl VideoRows {
    /// The profile of match `match_id`, its events those of `events`.
    /// Every event it refers to must be one of them. A correction of 0
    /// isn't one (the event where the anchor puts it); of two for the same
    /// event, the first counts (`VideoProfile::correction`).
    pub fn of(
        profile: &VideoProfile,
        match_id: Uuid,
        events: &EventsOfA,
    ) -> Result<Self, AppError> {
        let id_of = |event: EventRef| {
            events.id(event).ok_or_else(|| {
                not_consistent(format!(
                    "video of match {match_id}: no event {} of set {}",
                    event.index, event.set_number
                ))
            })
        };
        let anchor = profile
            .anchor
            .as_ref()
            .map(|a| {
                a.event.map(id_of).transpose().map(|event| VideoAnchor {
                    at: a.at,
                    offset_ms: a.offset_ms,
                    event,
                })
            })
            .transpose()?;
        let files = profile
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| {
                u16::try_from(i)
                    .map(|position| MatchVideoFile {
                        match_id,
                        position,
                        path: f.path.clone(),
                        start: f.start,
                    })
                    .map_err(|_| not_consistent(format!("video file {i} of match {match_id}")))
            })
            .collect::<Result<_, _>>()?;
        let mut corrections: BTreeMap<Uuid, VideoCorrection> = BTreeMap::new();
        for c in &profile.corrections {
            let event_id = id_of(c.event)?;
            if let Some(by_ms) = NonZeroI64::new(c.by_ms) {
                corrections.entry(event_id).or_insert(VideoCorrection {
                    match_id,
                    event_id,
                    by_ms,
                });
            }
        }
        Ok(VideoRows {
            row: MatchVideo { match_id, anchor },
            files,
            corrections: corrections.into_values().collect(),
        })
    }

    /// The profile from A's side, its events those of `events`: the
    /// corrections in the order of their events.
    pub fn profile(&self, events: &EventsOfA) -> VideoProfile {
        let mut files: Vec<&MatchVideoFile> = self.files.iter().collect();
        files.sort_by_key(|f| f.position);
        let mut corrections: Vec<Correction> = self
            .corrections
            .iter()
            .filter_map(|c| {
                events.event_ref(c.event_id).map(|event| Correction {
                    event,
                    by_ms: c.by_ms.get(),
                })
            })
            .collect();
        corrections.sort_by_key(|c| (c.event.set_number, c.event.index));
        VideoProfile {
            version: VIDEO_PROFILE_VERSION,
            files: files
                .into_iter()
                .map(|f| VideoFile {
                    path: f.path.clone(),
                    start: f.start,
                })
                .collect(),
            anchor: self.row.anchor.map(|a| Anchor {
                at: a.at,
                offset_ms: a.offset_ms,
                event: a.event.and_then(|id| events.event_ref(id)),
            }),
            corrections,
        }
    }
}
