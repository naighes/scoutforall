use crate::{
    constants::LEGACY_MAX_SUBSTITUTIONS,
    errors::AppError,
    shapes::{
        enums::TeamSideEnum,
        set::{ReplayIssue, SetEntry},
        snapshot::Snapshot,
        team::TeamEntry,
    },
    util::sanitize_filename,
};
use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug)]
pub struct MatchStatus {
    pub us_wins: u8,
    pub them_wins: u8,
    pub next_set_number: Option<u8>,
    pub last_incomplete_set: Option<SetEntry>,
    pub match_finished: bool,
    /// the last set won or lost, if any
    pub last_set: Option<SetEntry>,
}

/// What a list shows of a match: kept in the cache of the database (see
/// `providers::sqlite::summary`), so a list doesn't read nor replay its
/// sets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchSummary {
    pub id: String,
    pub opponent: String,
    pub date: DateTime<FixedOffset>,
    pub home: bool,
    /// the sets scouted, also the one going on
    pub sets: u8,
    pub us_wins: u8,
    pub them_wins: u8,
    pub finished: bool,
    /// the set not over, to go on with
    pub resume: Option<u8>,
    /// the set to start, when none is going on and the match isn't over
    pub next_set_number: Option<u8>,
}

impl MatchSummary {
    pub fn of(m: &MatchEntry, status: &MatchStatus) -> Self {
        MatchSummary {
            id: m.id.clone(),
            opponent: m.opponent.clone(),
            date: m.date,
            home: m.home,
            sets: u8::try_from(m.sets.len()).unwrap_or(u8::MAX),
            us_wins: status.us_wins,
            them_wins: status.them_wins,
            finished: status.match_finished,
            resume: status.last_incomplete_set.as_ref().map(|s| s.set_number),
            next_set_number: status.next_set_number,
        }
    }

    /// The summary of A's side as team B sees it: `opponent` is A, `home`
    /// whether B plays at home, the sets won the other way; nothing to go
    /// on with, as B isn't scouted.
    pub fn seen_by_b(self, opponent: String, home: bool) -> Self {
        MatchSummary {
            opponent,
            home,
            us_wins: self.them_wins,
            them_wins: self.us_wins,
            resume: None,
            next_set_number: None,
            ..self
        }
    }
}

/// A match a team plays, as its list shows it: from its side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayedMatch {
    pub summary: MatchSummary,
    /// team A, when the team is B of a match scouted from A's side only:
    /// the summary is A's, seen by B
    pub scouted_by: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MatchEntry {
    pub opponent: String,
    pub date: DateTime<FixedOffset>,
    #[serde(skip_serializing, skip_deserializing)]
    pub id: String,
    /// The team as the match sees it (`TeamEntry::with_roster`).
    #[serde(skip_serializing, skip_deserializing)]
    pub team: TeamEntry,
    pub home: bool,
    // matches saved before the number was configurable used the old rules
    #[serde(default = "legacy_max_substitutions")]
    pub max_substitutions: u8,
    // since v0.10, written only when given: see `PlayerEntry::notes`
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    // since v0.11, the players on the score sheet; written only when given:
    // the older versions read the file as before, with the whole team, and
    // the matches saved before have none
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roster: Vec<Uuid>,
    #[serde(skip_serializing, skip_deserializing)]
    pub sets: Vec<SetEntry>,
    // since v0.12, written only for the matches not scouted here: the older
    // versions ignore it
    #[serde(default, skip_serializing_if = "MatchSource::is_scouted")]
    pub source: MatchSource,
    // since v0.15, written only when true: the times of the events are the
    // seconds in the video of the match from midnight of its day, as the
    // DataVolley files linked to their video tell (`video::Anchor`)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub video_clock: bool,
}

/// Where the events of a match come from.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchSource {
    /// scouted with this application
    #[default]
    Scout4all,
    /// imported from a DataVolley file
    Dvw,
}

impl MatchSource {
    pub const ALL: [MatchSource; 2] = [MatchSource::Scout4all, MatchSource::Dvw];

    pub fn is_scouted(&self) -> bool {
        *self == MatchSource::Scout4all
    }
}

fn legacy_max_substitutions() -> u8 {
    LEGACY_MAX_SUBSTITUTIONS
}

impl MatchEntry {
    /// The id of a match against the opponent on the date: one a day.
    pub fn id_of(opponent: &str, date: &DateTime<FixedOffset>) -> String {
        format!(
            "{}_{}",
            date.format("%Y-%m-%d"),
            sanitize_filename(opponent)
        )
    }

    /// The first event of its sets that doesn't replay, if any.
    pub fn replay_issue(&self) -> Option<ReplayIssue> {
        self.sets
            .iter()
            .find_map(|s| s.replay_issue(self.max_substitutions))
    }

    pub fn get_status(&self) -> Result<MatchStatus, AppError> {
        let mut us_wins = 0;
        let mut them_wins = 0;
        let mut next_set_number = 1;
        let mut last_incomplete_set = None;
        let mut last_set: Option<SetEntry> = None;
        // scan all sets
        for set_number in 1..=5 {
            if let Some(set_entry) = self.sets.iter().find(|s| s.set_number == set_number) {
                // set found
                let snapshot = &self.get_set_snapshot(set_number)?;
                if let Some(snapshot) = snapshot {
                    match snapshot.get_set_winner(set_number) {
                        Some(TeamSideEnum::Us) => {
                            last_set = Some(set_entry.clone());
                            us_wins += 1;
                        }
                        Some(TeamSideEnum::Them) => {
                            last_set = Some(set_entry.clone());
                            them_wins += 1;
                        }
                        None => {
                            last_incomplete_set = Some(set_entry.clone());
                            break;
                        }
                    }
                    if us_wins == 3 || them_wins == 3 {
                        return Ok(MatchStatus {
                            us_wins,
                            them_wins,
                            next_set_number: None,
                            last_incomplete_set: None,
                            match_finished: true,
                            last_set,
                        });
                    }
                } else {
                    last_incomplete_set = Some(set_entry.clone());
                    break;
                }
            } else {
                next_set_number = set_number;
                break;
            }
            next_set_number = set_number + 1;
        }
        Ok(MatchStatus {
            us_wins,
            them_wins,
            next_set_number: Some(next_set_number),
            last_incomplete_set,
            match_finished: false,
            last_set,
        })
    }

    /// Who played in a set of the match: on court, the liberos, in an
    /// event.
    pub fn played(&self) -> HashSet<Uuid> {
        self.sets
            .iter()
            .flat_map(|s| {
                s.initial_positions
                    .iter()
                    .copied()
                    .chain([s.libero, s.setter])
                    .chain(s.fallback_libero)
                    .chain(
                        s.events
                            .iter()
                            .flat_map(|e| e.player.into_iter().chain(e.target_player)),
                    )
            })
            .collect()
    }

    /// The score of each set, ours first, in the order of the sets.
    pub fn set_scores(&self) -> Result<Vec<(u8, u8)>, AppError> {
        let mut sets: Vec<&SetEntry> = self.sets.iter().collect();
        sets.sort_by_key(|s| s.set_number);
        sets.into_iter()
            .map(|s| {
                s.compute_snapshot(self.max_substitutions)
                    .map(|snapshot| (snapshot.score_us, snapshot.score_them))
            })
            .collect()
    }

    pub fn get_set_snapshot(&self, set_number: u8) -> Result<Option<Snapshot>, AppError> {
        let s = match self.sets.iter().find(|s| s.set_number == set_number) {
            Some(set) => set,
            None => return Ok(None),
        };
        let snapshot = s.compute_snapshot(self.max_substitutions)?;
        Ok(Some(snapshot))
    }
}
