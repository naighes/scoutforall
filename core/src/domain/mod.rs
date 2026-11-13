//! The data as the database keeps it (TODO/001, schema v1 in
//! `migrations/0001_schema.sql`): persons apart from the teams they're
//! members of, matches between team A and team B (a team or only a name),
//! competitions, the roster of each team in a match with its numbers and
//! roles, sets with a lineup per side, one event per touch, the video of a
//! match.
//!
//! One type per table, the columns as Rust values; `sql` converts each
//! value to and from the database, `adapters` turns the rows into the
//! types of today (`TeamEntry`, `MatchEntry`, `SetEntry`, `VideoProfile`)
//! and back: the replay, the statistics and the interfaces keep using
//! those.

pub mod adapters;
pub mod sql;

use crate::localization::Labels;
use crate::shapes::{
    country::CountryCode,
    enums::{GenderEnum, LineupRulesEnum, RoleEnum, TeamClassificationEnum, TeamSideEnum},
    r#match::MatchSource,
    snapshot::EventEntry,
};
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use std::num::NonZeroI64;
use uuid::Uuid;

/// A person, in one place whatever the teams (D5). Deleted: only the id
/// and the flag are kept (D16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub id: Uuid,
    pub name: String,
    pub birth_date: Option<NaiveDate>,
    /// in centimetres
    pub height_cm: Option<u16>,
    pub notes: String,
    pub deleted: bool,
}

/// A team of a season (it has the year).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Team {
    pub id: Uuid,
    pub name: String,
    pub year: u16,
    pub classification: Option<TeamClassificationEnum>,
    pub gender: Option<GenderEnum>,
    pub country: Option<CountryCode>,
    pub deleted: bool,
}

/// A person in a team, with the team's number and role, at its place in
/// the list of the team's players. Deleted: removed from the team, the
/// person stays, and its place too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    pub team_id: Uuid,
    pub person_id: Uuid,
    pub position: u16,
    pub number: u8,
    pub role: Option<RoleEnum>,
    pub deleted: bool,
}

/// How a competition is played (D12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompetitionFormat {
    League,
    Cup,
    Tournament,
}

impl CompetitionFormat {
    pub const ALL: [CompetitionFormat; 3] = [
        CompetitionFormat::League,
        CompetitionFormat::Cup,
        CompetitionFormat::Tournament,
    ];

    pub fn name(self, labels: &Labels) -> &'static str {
        match self {
            CompetitionFormat::League => labels.format_league,
            CompetitionFormat::Cup => labels.format_cup,
            CompetitionFormat::Tournament => labels.format_tournament,
        }
    }
}

/// A competition, shared by the teams that play it (D12): its name is the
/// local one ("Serie C femminile – girone B").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Competition {
    pub id: Uuid,
    pub name: String,
    pub format: CompetitionFormat,
    pub season: Option<String>,
    pub gender: Option<GenderEnum>,
    pub country: Option<CountryCode>,
    /// the default of its matches
    pub friendly: bool,
    pub deleted: bool,
}

/// The phase of a competition a match is played in, as FIVB and CEV name
/// them, the playouts too (D12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    Regular,
    Pool,
    Qualification,
    Playoff,
    Playout,
    FinalRound,
    Placement,
    Other,
}

impl Stage {
    pub const ALL: [Stage; 8] = [
        Stage::Regular,
        Stage::Pool,
        Stage::Qualification,
        Stage::Playoff,
        Stage::Playout,
        Stage::FinalRound,
        Stage::Placement,
        Stage::Other,
    ];

    pub fn name(self, labels: &Labels) -> &'static str {
        match self {
            Stage::Regular => labels.stage_regular,
            Stage::Pool => labels.stage_pool,
            Stage::Qualification => labels.stage_qualification,
            Stage::Playoff => labels.stage_playoff,
            Stage::Playout => labels.stage_playout,
            Stage::FinalRound => labels.stage_final_round,
            Stage::Placement => labels.stage_placement,
            Stage::Other => labels.stage_other,
        }
    }
}

/// One of the two teams of a match: A, the team one starts from, or B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    A,
    B,
}

impl Side {
    pub const ALL: [Side; 2] = [Side::A, Side::B];

    /// The side of a team as A sees it: we are A.
    pub fn from_a(side: TeamSideEnum) -> Self {
        match side {
            TeamSideEnum::Us => Side::A,
            TeamSideEnum::Them => Side::B,
        }
    }

    /// The side as A sees it, see `from_a`.
    pub fn to_a(self) -> TeamSideEnum {
        match self {
            Side::A => TeamSideEnum::Us,
            Side::B => TeamSideEnum::Them,
        }
    }
}

/// Team B of a match: a team, or only its name (D3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeamRef {
    Team(Uuid),
    Name(String),
}

/// Where a match is played (D3): a fact of the match, not of a point of
/// view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Home {
    A,
    B,
    /// tournaments, finals
    Neutral,
}

impl Home {
    pub const ALL: [Home; 3] = [Home::A, Home::B, Home::Neutral];

    /// The home of the files: whether we, team A, played at home.
    pub fn from_a(home: bool) -> Self {
        match home {
            true => Home::A,
            false => Home::B,
        }
    }

    /// Whether A plays at home: not on a neutral court (as the upload
    /// tells it, D14).
    pub fn is_a(self) -> bool {
        self == Home::A
    }

    /// Whether B plays at home: not on a neutral court.
    pub fn is_b(self) -> bool {
        self == Home::B
    }
}

/// Who is scouted in a match (D3): team A, or both teams (only when B is a
/// team, with its players).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScoutingMode {
    A,
    Both,
}

impl ScoutingMode {
    pub const ALL: [ScoutingMode; 2] = [ScoutingMode::A, ScoutingMode::Both];
}

/// Where a match was before the database: the id of its folder and the
/// team of the folder (the upload queue, the reports refer to them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyId {
    pub id: String,
    pub team: Uuid,
}

/// A match between team A and team B: in programme without sets, played
/// with them (D11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub id: Uuid,
    pub team_a: Uuid,
    pub team_b: TeamRef,
    pub home: Home,
    pub scouting: ScoutingMode,
    pub start: DateTime<FixedOffset>,
    pub duration_min: Option<u16>,
    pub friendly: bool,
    pub competition_id: Option<Uuid>,
    pub stage: Option<Stage>,
    /// free text: "Giornata 7", "Semifinale – gara 2"
    pub round: Option<String>,
    pub max_substitutions: u8,
    pub notes: String,
    pub source: MatchSource,
    pub video_clock: bool,
    pub legacy: Option<LegacyId>,
    pub deleted: bool,
}

/// A player on the score sheet of a team in a match, with the number and
/// the role of that match (D9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterEntry {
    pub match_id: Uuid,
    pub team_id: Uuid,
    pub person_id: Uuid,
    pub number: u8,
    pub role: Option<RoleEnum>,
}

/// A set of a match; its lineups and events are rows of their own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchSet {
    pub match_id: Uuid,
    pub number: u8,
    pub serving: Side,
    /// none: v1, the sets saved before the rules were
    pub lineup_rules: Option<LineupRulesEnum>,
    pub deleted: bool,
}

/// The starting lineup of a side in a set: B's only when both teams are
/// scouted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetLineup {
    pub match_id: Uuid,
    pub set_number: u8,
    pub side: Side,
    /// from P1 to P6
    pub positions: [Uuid; 6],
    pub libero: Uuid,
    pub fallback_libero: Option<Uuid>,
    pub setter: Uuid,
}

/// An event (D8), at its place in the set, of a side and written as that
/// side sees it: scouting A, every event is A's (`OS`, `OE`, `OF` tell
/// what B did, as today); scouting both, the touches of each side. An
/// undone event keeps its place, deleted.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: Uuid,
    pub match_id: Uuid,
    pub set_number: u8,
    pub seq: u32,
    pub side: Side,
    pub entry: EventEntry,
    pub deleted: bool,
}

/// The video of a match (`VideoProfile`), of this device: its files are
/// paths of this computer. Its files and corrections are rows of their
/// own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchVideo {
    pub match_id: Uuid,
    /// chosen by the user; none: found by itself (`video::anchor`)
    pub anchor: Option<VideoAnchor>,
}

/// The time of an event and the moment of the video it happens at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoAnchor {
    pub at: DateTime<Utc>,
    /// from the start of the first file
    pub offset_ms: i64,
    /// the event chosen by the user, if any: the time is its own
    pub event: Option<Uuid>,
}

/// A file of the video of a match, at its place among them: they're
/// played one after the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchVideoFile {
    pub match_id: Uuid,
    pub position: u16,
    /// relative to the data folder when in it, absolute otherwise
    pub path: String,
    pub start: Option<DateTime<FixedOffset>>,
}

/// An event of a match put at its own time: `by_ms` from where the anchor
/// puts it (earlier when negative); where the anchor puts it isn't one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoCorrection {
    pub match_id: Uuid,
    pub event_id: Uuid,
    pub by_ms: NonZeroI64,
}
