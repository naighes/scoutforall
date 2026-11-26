//! The core of scout4all for the native apps: UniFFI generates the Kotlin
//! and Swift bindings of what's exported here.
//!
//! The API is blocking for now: the app calls it off the main thread
//! (`Dispatchers.IO` on Android) and a tokio runtime here drives the async
//! providers of the core. The final app must never block the UI: the plan is
//! to move to the `async fn` of UniFFI.

use crate::clock::{Clock, ClockExt, SystemClock};
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use scout4all_core::{
    analytics::{
        finished::{FinishedMatches, QueuedFinishedMatches},
        queue_manager::QueueManager,
        upload::{UploadStep, upload_next, upload_url},
    },
    assets::{LogoBackdrop, LogoTone},
    benchmark::{Measure, band, grade},
    competition_form::{CompetitionForm, CompetitionFormError},
    db,
    detail::{Breakdown, FUNDAMENTALS, ServePath, Tone, by_player, by_rotation_in},
    domain::{CompetitionFormat, ScoutingMode, Side as ScoutSide, Stage},
    dvw::{
        self,
        import::{
            ImportedSides, MatchPlan, OpponentChoice as DvwOpponentChoice, OpponentPlan,
            TeamChoice as DvwTeamChoice, TeamIssue, TeamPlan, default_max_substitutions,
            imported_already, imported_already_message, match_date, match_plan, opponent_plan,
            save_import, season_year, similar_teams, similarity, team_name, team_plan,
        },
        model::{DvwFile, MatchDate as DvwDate, Side},
    },
    errors::{AppError, IOError, TrainingError},
    event_edit::{Correcting, EditField, EventEdit},
    example,
    legacy::{
        archive::file_team_id,
        migrate::{Backup, migrate},
    },
    localization::{Labels, Language},
    logging::logger::{init_logger, log_error, log_info},
    match_form::{CompetitionInput, MatchForm, MatchFormError, RosterNumber},
    names,
    overview::{Errors, Fundamental, Overview, distribution_in, stats_of},
    player_form::{self, PlayerForm, PlayerFormError, name_the_removed},
    player_profile::{PlayerSeasonStats, age, has_training},
    providers::{
        match_reader::MatchReader,
        match_writer::MatchWriter,
        set_writer::SetWriter,
        sqlite::{
            Database,
            archive::{self as archives, Imported, Kind, Preview},
            competitions::SqliteCompetitions,
            legacy_archive::{self, Opened, import_legacy_match, import_legacy_team},
            matches::{SqliteMatchReader, SqliteMatchWriter},
            persons::SqlitePersons,
            queue::SqliteQueue,
            sets::{SqliteSetReader, SqliteSetWriter},
            teams::{SqliteTeamReader, SqliteTeamWriter},
            training::SqliteTraining,
            upload::SqliteUploads,
            video::SqliteVideoProfiles,
        },
        team_reader::TeamReader,
        team_writer::TeamWriter,
        training_totals::TrainingTotals,
    },
    scouting::{ChoiceOrder, PlayerChoice, Progress, ScoutingError, ScoutingSession, ScoutingStep},
    season::{self, SeasonError, SeasonMember},
    set_start::{self, MIN_PLAYERS_FOR_FALLBACK_LIBERO, NextSet, SetStartForm, next_set},
    settings::{ActivitySearch, Consent, PinnedTeams, TeamDefaults, VideoSeconds},
    shapes::{
        country::CountryCode,
        court_zone::{
            CourtCell, CourtSide, CourtTarget, CourtZoneEnum, ServeStartEnum, SubzoneEnum,
            TargetKind,
        },
        enums::{
            EvalEnum, EventTypeEnum, FriendlyName, GenderEnum, LanguageEnum, PhaseEnum, RoleEnum,
            TeamClassificationEnum, TeamSideEnum, ZoneEnum,
        },
        flags::{BlockersEnum, ReceptionSchemeEnum},
        r#match::{MatchEntry, MatchSummary, PlayedMatch},
        player::{DominantHand, PlayerEntry},
        set::SetEntry,
        snapshot::{EventEntry, Formation},
        stats::{
            AttackSummary, DigsEnum, EvalSet, EventKey, EventPick, Metric, RoleSet, SetTypeSet,
            SituationFilter, Stats, StatsFilter,
        },
        team::TeamEntry,
        technique::{ServeTypeEnum, SetTypeEnum, TechniqueEnum},
    },
    storage::{
        Store,
        fs::FsStore,
        layout::{self, CONFIG, UNREADABLE_CONFIG},
    },
    team_form::{TeamForm, TeamFormError},
    touch_edit::TouchEdit,
    training::totals::Scope as TrainingScope,
    trend::{Replay, Series, TrendPoint},
    video::{EventRef, StoredProfile, VideoProfile, VideoProfiles, place},
};
use sender::{FIRST_RETRY, LAST_RETRY, UploadSender};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, from_slice, to_value, to_vec_pretty};
use std::{
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::runtime::{Builder, Runtime};
use uuid::Uuid;

uniffi::setup_scaffolding!();

mod analysis;
pub mod clock;
pub mod sender;
pub use analysis::*;
pub mod strip;
pub mod training;
pub mod venues;
pub mod video;

// the types of the core the app sees as they are

/// The seconds of the video, in the settings: how far ← → and the buttons
/// jump, how long before an event the video goes to it out of clip mode.
#[uniffi::remote(Record)]
pub struct VideoSeconds {
    pub step_s: u32,
    pub lead_s: u32,
}

#[uniffi::remote(Enum)]
pub enum TeamClassificationEnum {
    TopInternational,
    HighNational,
    NationalMidLevel,
    NationalLowLevel,
    Regional,
    LocalDivision,
}

#[uniffi::remote(Enum)]
pub enum ServeStartEnum {
    Z1,
    Z9,
    Z6,
    Z7,
    Z5,
}

#[uniffi::remote(Enum)]
pub enum GenderEnum {
    Men,
    Women,
}

#[uniffi::remote(Enum)]
pub enum RoleEnum {
    Libero,
    OppositeHitter,
    Setter,
    OutsideHitter,
    MiddleBlocker,
}

#[uniffi::remote(Enum)]
pub enum BlockersEnum {
    NoBlock,
    One,
    Two,
    Three,
    Hole,
}

#[uniffi::remote(Enum)]
pub enum TeamSideEnum {
    Us,
    Them,
}

#[uniffi::remote(Enum)]
pub enum EventTypeEnum {
    S,
    P,
    A,
    D,
    B,
    F,
    OS,
    OE,
    FB,
    OF,
    R,
    CL,
    LR,
    LF,
    CS,
    PW,
    PL,
}

#[uniffi::remote(Enum)]
pub enum EvalEnum {
    Perfect,
    Positive,
    Exclamative,
    Over,
    Error,
    Negative,
}

#[uniffi::remote(Enum)]
pub enum SetTypeEnum {
    One,
    Two,
    Three,
    Four,
    C,
    DoubleC,
    B,
    Seven,
    Fast,
    High,
    Five,
    Six,
    Super,
    Nine,
    Quick,
    Pipe,
    Eight,
}

#[uniffi::remote(Enum)]
pub enum ServeTypeEnum {
    Float,
    Spin,
}

#[uniffi::remote(Enum)]
pub enum CourtZoneEnum {
    Z1,
    Z2,
    Z3,
    Z4,
    Z5,
    Z6,
    Z7,
    Z8,
    Z9,
    Z43,
    Z32,
    Z78,
    Z89,
    Z56,
    Z61,
}

#[uniffi::remote(Enum)]
pub enum TechniqueEnum {
    Set(SetTypeEnum),
    Serve(ServeTypeEnum),
}

#[uniffi::remote(Enum)]
pub enum ScoutingStep {
    Event,
    Player,
    Eval,
    Replacement,
}

#[uniffi::remote(Enum)]
pub enum Series {
    Reception,
    Attack,
    CounterAttack,
    Serve,
    SideOut,
    BreakPoint,
}

#[uniffi::remote(Enum)]
pub enum PhaseEnum {
    Break,
    SideOut,
}

#[uniffi::remote(Enum)]
pub enum DigsEnum {
    All,
    Defense,
    Cover,
}

#[uniffi::remote(Enum)]
pub enum Tone {
    Excellent,
    Good,
    Fair,
    Poor,
    Bad,
}

#[uniffi::remote(Enum)]
pub enum ReceptionSchemeEnum {
    Three,
    Four,
    Two,
}

#[uniffi::remote(Enum)]
pub enum EditField {
    EventType,
    Eval,
    Technique,
}

#[uniffi::remote(Enum)]
pub enum Progress {
    Choosing,
    Saved,
    SetOver,
}

#[uniffi::remote(Record)]
pub struct PlayerForm {
    pub name: String,
    pub role: Option<RoleEnum>,
    pub number: String,
    // empty by default: a form made in Kotlin without them is as before
    #[uniffi(default = "")]
    pub birth_date: String,
    #[uniffi(default = "")]
    pub height_cm: String,
    #[uniffi(default = "")]
    pub notes: String,
    #[uniffi(default = None)]
    pub nationality: Option<CountryCode>,
    #[uniffi(default = None)]
    pub dominant_hand: Option<DominantHand>,
    #[uniffi(default = "")]
    pub federation_id: String,
    #[uniffi(default = None)]
    pub person: Option<Uuid>,
}

#[uniffi::remote(Record)]
pub struct TeamForm {
    pub name: String,
    pub classification: Option<TeamClassificationEnum>,
    pub gender: Option<GenderEnum>,
    pub country: Option<CountryCode>,
    pub year: String,
}

#[uniffi::remote(Record)]
pub struct MatchForm {
    pub opponent: String,
    pub date: Option<MatchDate>,
    pub home: bool,
    pub max_substitutions: String,
    #[uniffi(default = "")]
    pub notes: String,
    #[uniffi(default = None)]
    pub roster: Option<Vec<Uuid>>,
    #[uniffi(default = None)]
    pub opponent_team: Option<Uuid>,
    #[uniffi(default = false)]
    pub create_opponent: bool,
    #[uniffi(default = false)]
    pub neutral: bool,
    #[uniffi(default = false)]
    pub friendly: bool,
    #[uniffi(default = "")]
    pub competition: String,
    #[uniffi(default = None)]
    pub competition_id: Option<Uuid>,
    #[uniffi(default = None)]
    pub competition_format: Option<CompetitionFormat>,
    #[uniffi(default = None)]
    pub stage: Option<Stage>,
    #[uniffi(default = "")]
    pub round: String,
    #[uniffi(default = [])]
    pub numbers: Vec<RosterNumber>,
    #[uniffi(default = false)]
    pub both: bool,
    #[uniffi(default = None)]
    pub venue: Option<Uuid>,
}

/// The form of a competition: edited in their list, created in the form
/// of a match.
#[uniffi::remote(Record)]
pub struct CompetitionForm {
    pub name: String,
    pub format: CompetitionFormat,
    pub season: String,
    pub gender: Option<GenderEnum>,
    pub country: Option<CountryCode>,
    pub friendly: bool,
}

#[uniffi::remote(Record)]
pub struct RosterNumber {
    pub player: Uuid,
    pub number: u8,
    pub role: Option<RoleEnum>,
}

#[uniffi::remote(Enum)]
pub enum CompetitionFormat {
    League,
    Cup,
    Tournament,
}

#[uniffi::remote(Enum)]
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

// the id of a player, as text for the app
uniffi::custom_type!(Uuid, String, {
    remote,
    lower: |id| id.to_string(),
    try_lift: |id| Ok(Uuid::parse_str(&id)?),
});

// the ISO 3166-1 code of a country, as text for the app
uniffi::custom_type!(CountryCode, String, {
    remote,
    lower: |code| code.to_string(),
    try_lift: |code| Ok(CountryCode::parse(&code).ok_or_else(|| {
        AppError::IO(IOError::Msg(format!("invalid country: {code}")))
    })?),
});

/// RFC 3339 text for the app, with the offset of where the match is played.
pub type MatchDate = DateTime<FixedOffset>;

uniffi::custom_type!(MatchDate, String, {
    remote,
    lower: |date| date.to_rfc3339(),
    try_lift: |date| Ok(DateTime::parse_from_rfc3339(&date)?),
});

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    #[error("{reason}")]
    Failed { reason: String },
    /// no face in the photo chosen for a player
    #[error("{reason}")]
    NoFace { reason: String },
    /// more than one face in the photo chosen for a player
    #[error("{reason}")]
    ManyFaces { count: u32, reason: String },
    /// a field of the team form, which the app shows next to it
    #[error("{reason}")]
    InvalidTeam { field: TeamField, reason: String },
    /// a field of the player form
    #[error("{reason}")]
    InvalidPlayer { field: PlayerField, reason: String },
    /// a field of the match form
    #[error("{reason}")]
    InvalidMatch { field: MatchField, reason: String },
    /// a field of the form of a competition
    #[error("{reason}")]
    InvalidCompetition {
        field: CompetitionField,
        reason: String,
    },
    /// a choice of the scout that doesn't apply, or an event not saved
    #[error("{reason}")]
    Scouting { reason: String },
    /// another player of the team with the same name: saved once the user
    /// confirms it
    #[error("{reason}")]
    Namesake { reason: String },
    /// a field of a form of the training
    #[error("{reason}")]
    InvalidTraining {
        field: training::TrainingField,
        reason: String,
    },
    /// a session or a cycle that overlaps others of the team: each is told
    /// in the reason
    #[error("{reason}")]
    Overlap { reason: String },
    /// a window that would leave out what is in the cycle: saved once the
    /// user agrees to detach it
    #[error("{reason}")]
    ChildrenOutside { reason: String },
}

/// The language of the app, for the errors converted with `?` (see
/// `From<AppError>`): the one of the last `Scout4All` made, which follows
/// its `set_language`.
#[cfg(not(test))]
static ERRORS_LANGUAGE: std::sync::RwLock<Option<Language>> = std::sync::RwLock::new(None);

#[cfg(not(test))]
fn set_errors_language(language: Language) {
    *ERRORS_LANGUAGE
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(language);
}

#[cfg(not(test))]
fn errors_labels() -> &'static Labels {
    ERRORS_LANGUAGE
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .map_or_else(|| Language::default().labels(), Language::labels)
}

// the tests run at the same time, each on its thread, each with its app: one
// language of the errors per thread, or a test would read another's
#[cfg(test)]
thread_local! {
    static ERRORS_LANGUAGE: std::cell::RefCell<Option<Language>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn set_errors_language(language: Language) {
    ERRORS_LANGUAGE.with(|l| *l.borrow_mut() = Some(language));
}

#[cfg(test)]
fn errors_labels() -> &'static Labels {
    ERRORS_LANGUAGE.with(|l| {
        l.borrow()
            .as_ref()
            .map_or_else(|| Language::default().labels(), Language::labels)
    })
}

impl From<AppError> for MobileError {
    fn from(e: AppError) -> Self {
        // the messages of the core are in the labels of the domain, for the
        // user as they are; the files and the network in the language
        let reason = e.message(errors_labels());
        match e {
            AppError::Training(TrainingError::Overlap(_) | TrainingError::CycleOverlap(_)) => {
                MobileError::Overlap { reason }
            }
            AppError::Training(TrainingError::ChildrenOutside(_)) => {
                MobileError::ChildrenOutside { reason }
            }
            _ => MobileError::Failed { reason },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TeamField {
    Name,
    Classification,
    Gender,
    Country,
    Year,
}

impl From<TeamFormError> for TeamField {
    fn from(e: TeamFormError) -> Self {
        match e {
            TeamFormError::EmptyName => TeamField::Name,
            TeamFormError::MissingClassification => TeamField::Classification,
            TeamFormError::MissingGender => TeamField::Gender,
            TeamFormError::MissingCountry => TeamField::Country,
            TeamFormError::InvalidYear => TeamField::Year,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PlayerField {
    Name,
    Role,
    Number,
    BirthDate,
    Height,
}

#[uniffi::remote(Enum)]
pub enum DominantHand {
    Right,
    Left,
}

impl From<PlayerFormError> for PlayerField {
    fn from(e: PlayerFormError) -> Self {
        match e {
            PlayerFormError::EmptyName => PlayerField::Name,
            PlayerFormError::MissingRole => PlayerField::Role,
            PlayerFormError::InvalidNumber | PlayerFormError::NumberInUse => PlayerField::Number,
            PlayerFormError::InvalidBirthDate => PlayerField::BirthDate,
            PlayerFormError::InvalidHeight => PlayerField::Height,
        }
    }
}

/// The fields of the form of a competition an error can be about: the
/// country is the app's to find among the ones it offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CompetitionField {
    Name,
    Country,
}

impl From<CompetitionFormError> for CompetitionField {
    fn from(e: CompetitionFormError) -> Self {
        match e {
            CompetitionFormError::EmptyName | CompetitionFormError::AlreadyThere => {
                CompetitionField::Name
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MatchField {
    Opponent,
    Date,
    MaxSubstitutions,
    Roster,
}

impl From<MatchFormError> for MatchField {
    fn from(e: MatchFormError) -> Self {
        match e {
            MatchFormError::EmptyOpponent
            | MatchFormError::OpponentIsTeamA
            | MatchFormError::ScoutingBothNeedsTeam => MatchField::Opponent,
            MatchFormError::InvalidDate => MatchField::Date,
            MatchFormError::InvalidMaxSubstitutions | MatchFormError::MaxSubstitutionsBelowUsed => {
                MatchField::MaxSubstitutions
            }
            MatchFormError::RosterTooSmall
            | MatchFormError::RosterTooLarge
            | MatchFormError::RosterWithoutPlayed
            | MatchFormError::RosterWithoutLibero
            | MatchFormError::InvalidRosterNumber
            | MatchFormError::RosterNumberInUse => MatchField::Roster,
        }
    }
}

/// A role as the form offers it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RoleChoice {
    pub value: RoleEnum,
    pub name: String,
}

/// A player that can be on the score sheet of a match, and whether it is.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RosterPlayer {
    pub id: Uuid,
    pub name: String,
    pub number: u8,
    pub role: Option<String>,
    pub chosen: bool,
    /// played in a set of the match: on the score sheet for good
    pub played: bool,
    /// the role named by `role`, to correct it
    #[uniffi(default = None)]
    pub role_value: Option<RoleEnum>,
}

/// The match form with the players of its roster, by number.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MatchRoster {
    pub form: MatchForm,
    pub players: Vec<RosterPlayer>,
}

/// A player as the team page shows it.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct Player {
    pub id: String,
    pub name: String,
    pub number: u8,
    pub role: Option<String>,
    /// in place of a missing photo
    pub initials: String,
    /// what the form starts from, to edit the player
    pub form: PlayerForm,
}

/// A person of the registry that can be added to a team (D5): playing in
/// another team, in a past season, or in none any more.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ExistingPlayer {
    pub id: Uuid,
    pub name: String,
    /// the name with the teams they play in, to tell namesakes apart:
    /// "Anna Rossi (Cortona 2025)"
    pub label: String,
    /// what the player form starts from once chosen: their details, the
    /// number and the role of their most recent team
    pub form: PlayerForm,
}

/// A player of the team as the new season offers them (D15).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SeasonPlayer {
    pub id: Uuid,
    pub name: String,
    pub number: u8,
    pub role: Option<RoleEnum>,
    /// the role as the team shows it
    pub role_name: Option<String>,
    /// going on to the new season
    pub chosen: bool,
}

/// The new season of a team: the form of the new team, the players to
/// take there.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NewSeason {
    pub form: TeamForm,
    pub players: Vec<SeasonPlayer>,
}

/// A classification as the form offers it.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct ClassificationChoice {
    pub value: TeamClassificationEnum,
    pub name: String,
    pub description: String,
}

/// A country as the form and the settings offer it.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct CountryChoice {
    pub value: CountryCode,
    pub name: String,
}

/// A stage of a competition as the form of a match offers it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct StageChoice {
    pub value: Stage,
    pub name: String,
}

/// A format of a competition as the form of a match offers it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FormatChoice {
    pub value: CompetitionFormat,
    pub name: String,
}

/// What team B of a match may be, as the form offers it: a team (with
/// its year), or a name met before.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OpponentChoice {
    pub team: Option<Uuid>,
    pub name: String,
    pub year: Option<u16>,
}

/// A competition as the form of a match offers it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CompetitionChoice {
    pub id: Uuid,
    pub name: String,
    pub season: Option<String>,
    pub format: CompetitionFormat,
    /// the default of its matches
    pub friendly: bool,
}

/// A competition in their list: its season and format, how many matches
/// are played in it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CompetitionListing {
    pub id: Uuid,
    pub name: String,
    pub season: Option<String>,
    pub format: String,
    pub matches: u32,
}

/// A gender as the form offers it.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct GenderChoice {
    pub value: GenderEnum,
    pub name: String,
}

/// A team as its page shows it.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct TeamDetails {
    pub id: String,
    pub name: String,
    pub year: u16,
    pub classification: Option<String>,
    pub gender: Option<String>,
    pub country: Option<String>,
    /// the active players, by number
    pub players: Vec<Player>,
    /// what the form starts from, to edit the team
    pub form: TeamForm,
}

/// A team in the same season that a player is active in.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ProfileTeam {
    pub id: String,
    pub name: String,
    pub year: u16,
    pub number: u8,
    pub role: Option<RoleEnum>,
}

/// What the profile of a player in one team shows beyond the form: the
/// season totals and what's worked out from the person.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlayerProfile {
    /// in place of a missing photo
    pub initials: String,
    /// today, none without a birth date
    pub age: Option<u32>,
    pub matches_played: u32,
    pub matches_scouted: u32,
    pub sets_played: u32,
    pub points: u32,
    pub attack_points: u32,
    pub block_points: u32,
    pub serve_points: u32,
    /// whether to show the attendance (`player_profile::has_training`)
    pub has_training: bool,
    /// present includes late; absent sessions form the denominator
    pub attendance_percent: Option<u8>,
    pub other_teams: Vec<ProfileTeam>,
}

/// A team as the list shows it: one that can't be read keeps its folder.
#[derive(Debug, PartialEq, uniffi::Enum)]
pub enum TeamListing {
    Team {
        id: String,
        name: String,
        year: u16,
        players: u32,
        /// kept at the top of the list (see `set_pinned`)
        pinned: bool,
    },
    Unreadable {
        folder: String,
    },
}

/// A match as the team page lists it: one that can't be read, or whose sets
/// don't replay, keeps its folder.
#[derive(Debug, PartialEq, uniffi::Enum)]
pub enum MatchListing {
    Match {
        id: String,
        opponent: String,
        home: bool,
        /// RFC 3339, with the offset of where it was played: the app formats
        /// it in its language
        date: String,
        /// the sets won by the team and by the opponent
        won: u8,
        lost: u8,
        finished: bool,
        /// team A, when the team is B of a match scouted from A's side
        /// only: the result as B sees it, nothing to scout from here
        scouted_by: Option<String>,
    },
    Unreadable {
        folder: String,
        /// why, when it's known: e.g. a set this version doesn't replay
        reason: Option<String>,
    },
}

/// What scouting the match goes on with.
fn next_kind(
    status: &scout4all_core::shapes::r#match::MatchStatus,
    team: &TeamEntry,
) -> NextSetKind {
    match next_set(status, team) {
        NextSet::Start(form) => NextSetKind::Start {
            set_number: form.set_number,
        },
        NextSet::Resume(set) => NextSetKind::Resume {
            set_number: set.set_number,
        },
        NextSet::Over => NextSetKind::Over,
    }
}

/// What scouting goes on with, as `next_kind` tells it from the status:
/// from the summary of the match.
fn next_of(m: &MatchSummary) -> NextSetKind {
    match (m.finished, m.resume, m.next_set_number) {
        (false, Some(set_number), _) => NextSetKind::Resume { set_number },
        (false, None, Some(set_number)) => NextSetKind::Start { set_number },
        _ => NextSetKind::Over,
    }
}

/// A match as the home lists it, with its team: how it stands and what
/// scouting goes on with.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct HomeMatch {
    pub team_id: String,
    pub team_name: String,
    pub id: String,
    pub opponent: String,
    pub home: bool,
    /// RFC 3339, as in `MatchListing`
    pub date: String,
    pub won: u8,
    pub lost: u8,
    pub next: NextSetKind,
}

/// The home: the matches not over, the last ones over, of every team, the
/// most recent first; then the teams.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct Home {
    pub in_progress: Vec<HomeMatch>,
    pub recent: Vec<HomeMatch>,
    pub teams: Vec<TeamListing>,
}

/// How many matches over the home lists.
pub const RECENT_MATCHES: usize = 5;

/// An archive to share or to save, and the name of its file.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct ExportedArchive {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

/// An archive imported: the team or the match, and what to tell of what
/// was here already (the data here kept), if anything.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct ImportedArchive {
    pub id: String,
    /// a line each, in the language of the app
    pub summary: Option<String>,
}

impl ImportedArchive {
    fn of(imported: &Imported, labels: &Labels) -> Self {
        ImportedArchive {
            id: imported.id.to_string(),
            summary: imported.summary(labels),
        }
    }
}

/// Moves into the database what the versions before left in the data
/// folder, a copy of the folder beside it first (`<folder>-backup-<day>.zip`):
/// what to tell at the start, if anything. A migration that can't start
/// changes nothing, and is tried again at the next start.
fn migrate_folder(
    runtime: &Runtime,
    store: &dyn Store,
    db: &Database,
    data_dir: &Path,
    labels: &Labels,
) -> Option<String> {
    let backup = Backup {
        dir: data_dir
            .parent()
            .map_or_else(|| data_dir.to_path_buf(), Path::to_path_buf),
        prefix: data_dir.file_name().map_or_else(
            || "scout4all".to_string(),
            |name| name.to_string_lossy().into_owned(),
        ),
    };
    match runtime.block_on(migrate(store, db, &backup, labels)) {
        Ok(migrated) => {
            if migrated.backup.is_some() {
                log_info(&format!(
                    "data folder moved into the database: {} teams, {} matches, {} left",
                    migrated.teams,
                    migrated.matches,
                    migrated.left.len()
                ));
            }
            migrated.notice(labels)
        }
        Err(e) => {
            log_error(&format!("could not move the data folder: {e}"));
            Some(labels.migration_failed.replace("{}", &e.to_string()))
        }
    }
}

/// A team of a DataVolley file: the one playing at home or the visiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DvwSide {
    Home,
    Visiting,
}

impl From<DvwSide> for Side {
    fn from(side: DvwSide) -> Self {
        match side {
            DvwSide::Home => Side::Home,
            DvwSide::Visiting => Side::Visiting,
        }
    }
}

/// The final score of a set of a DataVolley file.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct DvwSetScore {
    pub home: u8,
    pub visiting: u8,
}

/// What a DataVolley file holds, to choose what to import.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct DvwPreview {
    pub home: String,
    pub visiting: String,
    /// The day of the match, `YYYY-MM-DD`: one when the file tells it, two
    /// when it reads two ways (day first, then month first), none when it
    /// has none: the user chooses or writes it.
    pub days: Vec<String>,
    /// The first year of the season, when it can be told.
    pub season: Option<u16>,
    pub sets: Vec<DvwSetScore>,
}

/// A team of ours like the one of the file, and why.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct DvwSimilarTeam {
    pub id: String,
    pub name: String,
    pub year: u16,
    pub same_name: bool,
    /// the players of the file found in the team, of `players`
    pub same_players: u32,
    pub players: u32,
}

/// How a team of a DataVolley file is imported, as the user chose.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DvwChoice {
    pub side: DvwSide,
    /// `YYYY-MM-DD`
    pub day: String,
    pub max_substitutions: u8,
    /// The team of ours it goes into; a new team when none.
    pub team_id: Option<String>,
    /// Of the new team, required as by the form of a team.
    pub classification: Option<TeamClassificationEnum>,
    pub gender: Option<GenderEnum>,
    pub country: Option<CountryCode>,
    /// Team B: the other team of the file.
    pub opponent: DvwOpponent,
}

/// Team B of an imported match, as the user chose (D3).
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum DvwOpponent {
    /// only its name, as written in the file
    Name,
    /// a team of ours, with the players of the file added
    Team { id: String },
    /// a new team with the name and the players of the file, of the
    /// season, level, gender and country of team A
    NewTeam,
}

/// The import checked, ready to save.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct DvwReady {
    pub team: String,
    pub sets: u32,
    pub new_players: u32,
    /// Team B, its name, and the players of the file added to it when a
    /// team (none when only a name).
    pub opponent: String,
    pub opponent_new_players: Option<u32>,
    /// The players of team A with another number in the file: the number
    /// of the match, the team unchanged.
    pub match_numbers: u32,
    /// What the file tells of the competition, as saved: "Serie C
    /// Maschile · Regular season · Andata – giornata 1"; none when
    /// nothing.
    pub competition: Option<String>,
    /// Why the match goes in only from the side of team A when team B is a
    /// team: the touches of B left out (TODO/001); none on both sides or
    /// against a name.
    pub one_side: Option<String>,
}

/// What an import saved.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct DvwImported {
    pub team_id: String,
    pub match_id: String,
}

/// A team a match may be imported into.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct TeamChoice {
    pub id: String,
    pub name: String,
}

/// What an archive holds, to confirm importing it: only an archive that
/// can be imported.
#[derive(Debug, PartialEq, uniffi::Enum)]
pub enum ArchivePreview {
    Team {
        name: String,
        year: u16,
        players: u32,
    },
    Match {
        opponent: String,
        home: bool,
        /// RFC 3339, as in `MatchListing`
        date: String,
        won: u8,
        lost: u8,
        /// the teams of its players without it, by name: usually one
        teams: Vec<TeamChoice>,
    },
    /// the training of a team: cycles and sessions with their activities
    /// and attendance
    Training {
        name: String,
        year: u16,
        sessions: u32,
        cycles: u32,
    },
}

/// The points of a set, the team's first.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct SetScore {
    pub number: u8,
    pub us: u8,
    pub them: u8,
}

/// The points of each set of the match, in order.
fn set_scores(m: &MatchEntry) -> Result<Vec<SetScore>, AppError> {
    let mut numbers: Vec<u8> = m.sets.iter().map(|s| s.set_number).collect();
    numbers.sort_unstable();
    numbers
        .into_iter()
        .map(|number| {
            m.get_set_snapshot(number).map(|snapshot| SetScore {
                number,
                us: snapshot.as_ref().map_or(0, |s| s.score_us),
                them: snapshot.as_ref().map_or(0, |s| s.score_them),
            })
        })
        .collect()
}

/// A match over: the sets won and lost, the points of each.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MatchResult {
    pub won: u8,
    pub lost: u8,
    pub sets: Vec<SetScore>,
}

/// What scouting a match goes on with.
#[derive(Debug, PartialEq, uniffi::Enum)]
pub enum NextSetKind {
    Start { set_number: u8 },
    Resume { set_number: u8 },
    Over,
}

/// A match as its page shows it.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct MatchDetails {
    pub id: String,
    pub opponent: String,
    pub home: bool,
    /// RFC 3339, as in `MatchListing`
    pub date: String,
    pub won: u8,
    pub lost: u8,
    pub finished: bool,
    /// the sets played or in progress, by number
    pub sets: Vec<SetScore>,
    pub next: NextSetKind,
    /// what the scout noted about the match, empty if nothing
    pub notes: String,
    /// as in `MatchListing`: the match seen by team B, scouted by A
    pub scouted_by: Option<ScoutedBy>,
    /// scouted on both sides: the statistics of each team can be read
    /// (`MatchSide`)
    pub both_sides: bool,
    /// the gym it's played in, if one is set
    pub venue_id: Option<String>,
}

/// The team of a match whose statistics are read: A, the team that
/// scouted it, or B, only of a match scouted on both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, uniffi::Enum)]
pub enum MatchSide {
    #[default]
    A,
    B,
}

/// Team A of a match seen by B: the team that scouted it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ScoutedBy {
    pub team_id: String,
    pub team_name: String,
}

/// What removing a team does to its matches (D16), to confirm it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TeamRemovalCounts {
    /// the matches against another team: they stay to it
    pub kept: u32,
    /// the matches against a name: gone with the team
    pub removed: u32,
    /// the sessions of training: gone with the team
    pub sessions: u32,
}

/// A player to choose when starting a set.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LineupPlayer {
    pub id: String,
    pub name: String,
    pub number: u8,
    pub role: Option<RoleEnum>,
    pub role_name: Option<String>,
}

/// What the scout chose so far to start a set.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct Lineup {
    pub set_number: u8,
    /// the names of the team and of the opponent, to say who serves
    pub team: String,
    pub opponent: String,
    /// whether the scout chooses who serves: in the first set and in the
    /// tie-break
    pub chooses_serving_team: bool,
    pub serving_team: Option<TeamSideEnum>,
    /// the players in positions 1 to 6
    pub positions: Vec<Option<LineupPlayer>>,
    pub setter: Option<LineupPlayer>,
    pub libero: Option<LineupPlayer>,
    pub fallback_libero: Option<LineupPlayer>,
    /// whether the team has enough players for a fallback libero
    pub allows_fallback_libero: bool,
    /// nothing missing: the set can start
    pub complete: bool,
}

/// Starting a set of a match: the choices so far, which the core checks.
/// Nothing is saved until `Scout4All::start_set`.
#[derive(uniffi::Object)]
pub struct SetStart {
    team: TeamEntry,
    match_id: String,
    opponent: String,
    form: Mutex<SetStartForm>,
    /// the start of B, scouting both sides: who serves isn't chosen there
    of_b: bool,
    labels: &'static Labels,
}

#[uniffi::export]
impl SetStart {
    pub fn lineup(&self) -> Lineup {
        let form = self.form();
        let player = |id: Option<Uuid>| {
            id.and_then(|id| self.team.find_player(id))
                .map(|p| lineup_player(p, self.labels))
        };
        Lineup {
            set_number: form.set_number,
            team: self.team.name.clone(),
            opponent: self.opponent.clone(),
            chooses_serving_team: form.chooses_serving_team() && !self.of_b,
            serving_team: form.serving_team,
            positions: form.positions.iter().map(|id| player(*id)).collect(),
            setter: player(form.setter),
            libero: player(form.libero),
            fallback_libero: player(form.fallback_libero),
            allows_fallback_libero: self.team.active_players().len()
                >= MIN_PLAYERS_FOR_FALLBACK_LIBERO,
            complete: match self.of_b {
                true => form.validate_side(&self.team).is_ok(),
                false => form.clone().validate(&self.team).is_ok(),
            },
        }
    }

    pub fn choose_serving_team(&self, serving_team: TeamSideEnum) {
        self.form().serving_team = Some(serving_team);
    }

    /// Puts the player in `position` (1 to 6), or empties it.
    pub fn place(&self, position: u8, player: Option<String>) -> Result<(), MobileError> {
        let player = player.as_deref().map(parse_id).transpose()?;
        self.form().place(index(position), player);
        Ok(())
    }

    pub fn choose_setter(&self, player: Option<String>) -> Result<(), MobileError> {
        self.form().setter = player.as_deref().map(parse_id).transpose()?;
        Ok(())
    }

    pub fn choose_libero(&self, player: Option<String>) -> Result<(), MobileError> {
        self.form().libero = player.as_deref().map(parse_id).transpose()?;
        Ok(())
    }

    pub fn choose_fallback_libero(&self, player: Option<String>) -> Result<(), MobileError> {
        self.form().fallback_libero = player.as_deref().map(parse_id).transpose()?;
        Ok(())
    }

    /// The lineup of the tutorial, the example team receiving in P6.
    pub fn tutorial_lineup(&self) {
        example::tutorial_lineup(&mut self.form(), &self.team);
    }

    /// The players that can go in `position` (1 to 6), by number.
    pub fn position_choices(&self, position: u8) -> Vec<LineupPlayer> {
        self.choices(self.form().position_choices(&self.team, index(position)))
    }

    /// The players on court, by number.
    pub fn setter_choices(&self) -> Vec<LineupPlayer> {
        self.choices(self.form().setter_choices(&self.team))
    }

    pub fn libero_choices(&self) -> Vec<LineupPlayer> {
        self.choices(self.form().libero_choices(&self.team))
    }

    pub fn fallback_libero_choices(&self) -> Vec<LineupPlayer> {
        self.choices(self.form().fallback_libero_choices(&self.team))
    }
}

impl SetStart {
    /// The form, even if a call panicked while holding it: each change
    /// leaves it whole.
    fn form(&self) -> MutexGuard<'_, SetStartForm> {
        self.form
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn choices(&self, players: Vec<&PlayerEntry>) -> Vec<LineupPlayer> {
        let mut choices: Vec<LineupPlayer> = players
            .into_iter()
            .map(|p| lineup_player(p, self.labels))
            .collect();
        choices.sort_by_key(|p| p.number);
        choices
    }
}

/// A player on court while scouting.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CourtPlayer {
    pub id: String,
    pub name: String,
    pub number: u8,
    pub setter: bool,
    pub libero: bool,
    /// whether the scout can choose the player now
    pub selectable: bool,
    /// whether the player plays the fundamental worth watching now (see
    /// `Snapshot::get_spotlight`), and their efficiency in it over the
    /// match so far, none before their first
    pub spotlight: bool,
    pub efficiency: Option<f64>,
    /// how good the efficiency is for the level of the team, from 0 (red)
    /// to 1 (green), none without the level
    pub grade: Option<f64>,
    /// where the player stands, in meters from the left sideline and from
    /// the net, seen from the bench
    pub x: f32,
    pub y: f32,
    /// one of the three spread to receive the serve: the others hide
    /// beneath them, so where they overlap a touch goes to the receiver
    pub receives: bool,
}

/// The middle of the zone of each position, 1 to 6: the front row in the 3
/// meters before the attack line, the back row in the 6 behind it.
const ZONES: [(f32, f32); 6] = [
    (7.5, 6.0),
    (7.5, 1.5),
    (4.5, 1.5),
    (1.5, 1.5),
    (1.5, 6.0),
    (4.5, 6.0),
];

/// Where the players of each position, 1 to 6, stand to receive with three
/// (the libero and the outside hitters), by rotation (the position of the
/// setter less 1), in a 5-1 with the canonical P1. The setter hides near
/// the net, or behind a receiver, to go to the net; the opposite in the
/// back row hides; the positions respect the order of rotation at the
/// serve. Validated by the user (2026-10-24), then laid out so that the
/// players and their names don't overlap on a phone (360dp wide): on four
/// lines (at the net, 3, 5 and 7.5 meters), side by side only 3 meters
/// apart.
const RECEPTION: [[(f32, f32); 6]; 6] = [
    // P1: O 4, C2 3 at the net; S2, L, S1 receive; the setter behind S1
    [
        (7.5, 7.5),
        (7.5, 5.0),
        (4.5, 0.8),
        (1.5, 0.8),
        (1.5, 5.2),
        (4.5, 5.3),
    ],
    // P2: C2 4, the setter 2 at the net; S1, S2, L receive; O hides left
    [
        (7.5, 5.3),
        (7.5, 0.8),
        (1.5, 5.0),
        (1.2, 0.8),
        (1.2, 7.6),
        (4.5, 5.3),
    ],
    // P3: the setter 3, C1 2 at the net; S1, L, S2 receive; O hides right
    [
        (7.5, 5.2),
        (7.5, 0.8),
        (4.5, 0.8),
        (1.5, 5.0),
        (4.5, 5.2),
        (6.0, 7.6),
    ],
    // P4: the setter 4, C1 3 at the net; S1, L, S2 receive; O hides right
    [
        (7.5, 7.5),
        (7.5, 5.0),
        (4.5, 0.8),
        (1.5, 0.8),
        (1.5, 5.2),
        (4.5, 5.3),
    ],
    // P5: C1 4, O 2 at the net, the setter behind C1; S2, S1, L receive
    [
        (7.5, 5.5),
        (7.5, 0.8),
        (1.5, 5.4),
        (1.2, 0.8),
        (1.3, 3.1),
        (4.5, 5.6),
    ],
    // P6: O 3, C2 2 at the net, the setter behind them; S2, L, S1 receive
    [
        (7.5, 5.4),
        (7.5, 0.8),
        (3.5, 0.8),
        (1.5, 5.0),
        (4.5, 5.4),
        (6.0, 3.1),
    ],
];

/// The positions of the three that receive in `RECEPTION`, by rotation: the
/// outside hitters and the libero, in the back row in place of a middle
/// blocker.
const RECEIVERS: [[usize; 3]; 6] = [
    [2, 5, 6],
    [1, 3, 6],
    [1, 4, 5],
    [2, 5, 6],
    [1, 3, 6],
    [1, 4, 5],
];

/// A player to choose for the event, or as a replacement.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ScoutingPlayer {
    pub id: String,
    pub name: String,
    pub number: u8,
    pub role: String,
    /// in place of a missing photo
    pub initials: String,
    /// off the court while the libero plays in their place: to show apart,
    /// as the court doesn't (see `PlayerChoice::out_for_the_libero`)
    pub out_for_the_libero: bool,
}

/// An event as the scout chooses it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct EventChoice {
    pub value: EventTypeEnum,
    pub name: String,
    /// scouting both sides: the event is B's
    #[uniffi(default)]
    pub side_b: bool,
}

/// An evaluation of the event being chosen, e.g. "ace" for a perfect serve.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct EvalChoice {
    pub value: EvalEnum,
    /// what the scouts write: #, +, !, /, -, =
    pub symbol: String,
    pub name: String,
    pub description: Option<String>,
}

/// A set or serve to save with the evaluation.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TechniqueChoice {
    pub value: TechniqueEnum,
    /// what the scouts write, e.g. "A" or "F"
    pub code: String,
    /// the word for it, if any, e.g. "high" or "float"
    pub short_name: Option<String>,
    /// e.g. "A (high)" or "F (float)"
    pub name: String,
    /// what the evaluation saves now, chosen or preselected
    pub chosen: bool,
}

/// How many blocked, to tell along with the evaluation of our attack or
/// of our block.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BlockersChoice {
    pub value: BlockersEnum,
    /// e.g. "2 blockers"
    pub name: String,
    /// what the evaluation saves now
    pub chosen: bool,
}

/// A zone our serve starts from, behind our end line, to tell along with
/// its evaluation.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ServeStartChoice {
    pub value: ServeStartEnum,
    /// 1, 9, 6, 7 or 5
    pub label: String,
    /// what the evaluation saves now, told or remembered for the server
    pub chosen: bool,
}

/// A cell of the court (a subzone of DataVolley), or a whole zone at the
/// net for our block, to tell where the ball went.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CourtCellChoice {
    /// 1 to 9
    pub zone: u8,
    /// "A" to "D"; empty for a whole zone at the net
    pub subzone: String,
    /// as the scout reads it: "1A", "4"
    pub label: String,
    /// what the evaluation saves now, alone or with the one next to it
    pub chosen: bool,
}

/// An event saved in the set.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ScoutedEvent {
    pub event: EventTypeEnum,
    pub name: String,
    pub player: Option<ScoutingPlayer>,
    pub eval: Option<String>,
    /// what the scouts write: #, +, !, /, -, =
    pub symbol: Option<String>,
    /// e.g. "A" or "F"
    pub technique: Option<String>,
    /// where the ball went, if told: e.g. "4" or "4/3"
    pub target: Option<String>,
    /// how many blocked, if told: e.g. "2 blockers"
    pub blockers: Option<String>,
    /// where our serve started from, if told: e.g. "1"
    pub serve_start: Option<String>,
    /// scouting both sides: the event is B's
    #[uniffi(default)]
    pub side_b: bool,
}

/// An event of a set, in the list to correct it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SetEvent {
    /// its place in the set, from 0
    pub index: u32,
    pub event: ScoutedEvent,
    /// the score after it, the team's first
    pub score_us: u8,
    pub score_them: u8,
    /// not a substitution, nor a libero or setter change
    pub editable: bool,
}

/// The events of a set, the first first.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SetEvents {
    pub set_number: u8,
    pub team: String,
    pub opponent: String,
    /// scouted on both sides: each event says whose it is
    pub both: bool,
    pub events: Vec<SetEvent>,
}

/// An event being corrected, and what it can become.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CorrectionState {
    pub index: u32,
    pub event: ScoutedEvent,
    /// the set before it: the score, the team's first, the rotation, the
    /// phase
    pub score_us: u8,
    pub score_them: u8,
    pub rotation: String,
    pub phase: String,
    /// what of the event can change
    pub fields: Vec<EditField>,
    pub event_types: Vec<EventChoice>,
    pub evals: Vec<EvalChoice>,
    pub eval: Option<EvalEnum>,
    /// the sets the attacker can receive, or the serves; `chosen` marks the
    /// one of the event, none when no technique is
    pub techniques: Vec<TechniqueChoice>,
}

/// What the scouting screen shows.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ScoutingState {
    pub set_number: u8,
    pub team: String,
    pub opponent: String,
    pub home: bool,
    /// the points of this set, the team's first
    pub score_us: u8,
    pub score_them: u8,
    /// the sets won by the team and by the opponent
    pub won: u8,
    pub lost: u8,
    pub serving_team: TeamSideEnum,
    /// the rotation, by the position of the setter: P1 to P6 in Italian,
    /// S1 to S6 in English
    pub rotation: String,
    /// the players in positions 1 to 6, where they play: once our serve is
    /// made or theirs received, the setter or the opposite in 2 and 1, the
    /// middle blocker in 3 and 5, the outside hitter in 4 and 6, but the
    /// front row of the canonical P1 (see `Snapshot::get_playing_positions`)
    pub court: Vec<CourtPlayer>,
    pub substitutions_left: u8,
    pub step: ScoutingStep,
    /// whether the scout made a choice that `undo` takes back; otherwise it
    /// removes the last saved event (see `ScoutingSession::is_halfway`)
    pub halfway: bool,
    /// the event being chosen, and its player once chosen
    pub event: Option<EventChoice>,
    pub player: Option<ScoutingPlayer>,
    /// the events of the set, the last first
    pub events: Vec<ScoutedEvent>,
    /// the side of the net the ball is on
    pub ball: TeamSideEnum,
    /// the last touch of the rally being played, none before the serve
    pub last_touch: Option<ScoutedEvent>,
    /// in P1 in side out the outside hitter attacks from 4 and the opposite
    /// from 2: kept until changed, also in the next set
    pub inverted_p1: bool,
    /// whether the inverted P1 makes a difference now, in P1 in side out
    /// only: shown then
    pub inverted_p1_applies: bool,
    /// how many receive their next serve, 3 unless told; told only before
    /// their serve
    pub reception: u8,
    pub reception_open: bool,
    /// whether the team can make the event chosen, no player: our fault
    /// (e.g. a rotation fault), see `choose_team`
    pub team_choice: bool,
    /// scouting both sides: the scores, the court and the choices are of
    /// the side on screen, B's when `side_b`
    #[uniffi(default)]
    pub both: bool,
    #[uniffi(default)]
    pub side_b: bool,
    /// scouting both sides: the rotation of the side not on screen, as
    /// `rotation` is the one on screen's (empty scouting one side)
    #[uniffi(default)]
    pub other_rotation: String,
}

/// The sets the statistics are of: one of the match, or all of them so
/// far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, uniffi::Enum)]
pub enum StatsScope {
    Set { number: u8 },
    Match,
}

/// The events scored in a fundamental (see `overview::Fundamental`).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FundamentalStats {
    pub event: EventTypeEnum,
    pub name: String,
    pub total: u32,
    /// -100 to 100, none with no events
    pub efficiency: Option<f64>,
    pub perfect: u32,
    pub errors: u32,
    /// how good the efficiency is for the level of the team, from 0 (red)
    /// to 1 (green), none without a band or without the level (see
    /// `benchmark` in the core)
    pub grade: Option<f64>,
}

/// The rallies won in a phase.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PhaseStats {
    pub percentage: f64,
    pub won: u32,
    pub rallies: u32,
    /// the points won minus the ones lost
    pub balance: i64,
    pub grade: Option<f64>,
}

/// The possessions of the side outs of a number, and the ones won (see
/// `PossessionSummary` in the core).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PossessionStats {
    /// 1st, 2nd, 3rd+ in English
    pub name: String,
    pub total: u32,
    pub won: u32,
    /// `None` with no possessions
    pub percentage: Option<f64>,
}

/// Attacks built by us (see `AttackSummary` in the core).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AttackStats {
    pub total: u32,
    pub kills: u32,
    pub errors: u32,
    pub blocked: u32,
    pub efficiency: f64,
    pub grade: Option<f64>,
}

/// The errors made in a fundamental, of a kind.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ErrorCount {
    pub event: EventTypeEnum,
    pub count: u32,
}

/// The attacks set to a zone: 4, 3, 2 in the front row, 8 (pipe) and 9
/// (from 1) in the back row.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ZoneStats {
    pub zone: u8,
    /// of the attacks of the distribution, 0 to 100
    pub share: f64,
    pub attacks: AttackStats,
}

/// The band of a figure for the level of the team, in percent: yellow
/// in the middle, red half a band below, green half a band above.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct GradeBand {
    pub low: f64,
    pub high: f64,
}

/// A figure after every rally of a set, from its first.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SetTrend {
    pub set_number: u8,
    /// how far the set had gone after each rally, in points of the team
    /// ahead (see `Replay::progress` in the core): to draw the sets side
    /// by side
    pub progress: Vec<f64>,
    /// so far in the set
    pub values: Vec<Option<f64>>,
    /// of its last touches in the set (see `RECENT_TOUCHES` in the core)
    pub recent: Vec<Option<f64>>,
    /// the errors of the figure in each rally: all of them, and only the
    /// unforced ones (see `Replay::errors` in the core)
    pub errors: Vec<u32>,
    pub unforced: Vec<u32>,
}

/// The points of a set in order, as the score sheet has them under the
/// trend (and the report).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SetPoints {
    pub set_number: u8,
    pub points: Vec<PointScored>,
}

/// A point: scored by us or by them, and the score it reached.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct PointScored {
    pub us: bool,
    pub score: u8,
}

/// How a figure went in each set, to compare the sets, and where it is in
/// the match.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TrendLine {
    pub series: Series,
    pub name: String,
    pub band: Option<GradeBand>,
    pub sets: Vec<SetTrend>,
    /// in the match so far, and of its last touches in the match
    pub overall: Option<f64>,
    pub overall_recent: Option<f64>,
    /// their grades for the level of the team
    pub overall_grade: Option<f64>,
    pub overall_recent_grade: Option<f64>,
}

/// What the coach wants to know while the match is played (see
/// `overview::Overview` in the core).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LiveStats {
    pub scope: StatsScope,
    /// the sets of the match so far, this one included: the scopes to
    /// choose from
    pub sets: Vec<u8>,
    /// reception, attack, serve, block, dig
    pub fundamentals: Vec<FundamentalStats>,
    /// whether the figures of the team, of its rallies (side out, break
    /// point, by possession, the rotations, their errors and points), are
    /// shown: not with players chosen, a rally has none
    pub team_figures: bool,
    pub side_out: Option<PhaseStats>,
    /// the first, second and later possessions of the side outs
    pub side_out_possessions: Vec<PossessionStats>,
    pub break_points: Option<PhaseStats>,
    pub first_attack: Option<AttackStats>,
    pub counter_attack: Option<AttackStats>,
    pub unforced_errors: u32,
    /// serve, attack, block (invasion), fault
    pub unforced_by_event: Vec<ErrorCount>,
    pub forced_errors: u32,
    /// attack (blocked), block, reception (their aces), dig
    pub forced_by_event: Vec<ErrorCount>,
    pub aces: u32,
    pub aces_against: u32,
    pub opponent_errors: u32,
    pub opponent_points: u32,
    /// the distribution of the setter: all the attacks, the ones on our
    /// reception, the counter-attacks
    pub distribution: Vec<ZoneStats>,
    pub first_attack_distribution: Vec<ZoneStats>,
    pub counter_attack_distribution: Vec<ZoneStats>,
    /// whether the figures have a grade: the team has a level and a
    /// gender
    pub graded: bool,
    /// reception, attack, counter-attack, serve, side out, break point, by
    /// set: always of every set so far, whatever the scope
    pub trend: Vec<TrendLine>,
    /// the points of every set so far, in order, whatever the scope
    pub points: Vec<SetPoints>,
    /// our team and theirs, to name the rows of the points
    pub team_name: String,
    pub opponent: String,
    /// the six rotations, in order, played or not
    pub rotations: Vec<RotationStats>,
    /// what the figures above are narrowed to (not the trend, nor the
    /// points: those are of every rally)
    pub filter: DetailFilter,
    /// the rotations and the phases to narrow them to
    pub rotation_choices: Vec<RotationChoice>,
    pub phase_choices: Vec<PhaseChoice>,
}

/// The phases won in a rotation, and all its rallies.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RotationStats {
    /// 0 to 5, as `DetailFilter::rotation`
    pub rotation: u8,
    /// P1 to P6 in Italian, S1 to S6 in English
    pub name: String,
    /// every rally of the rotation, in the phase of the filter if any:
    /// never graded
    pub total: Option<PhaseStats>,
    pub side_out: Option<PhaseStats>,
    pub break_points: Option<PhaseStats>,
}

/// What the detail of a fundamental is narrowed to: none keeps them all.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, uniffi::Record)]
#[serde(default)]
pub struct DetailFilter {
    pub phase: Option<PhaseEnum>,
    /// 0 to 5, the setter in position 1 to 6: none keeps them all
    #[uniffi(default)]
    pub rotations: Vec<u8>,
    /// the ids of the players: none keeps them all
    #[uniffi(default)]
    pub players: Vec<String>,
    // the situation (see `SituationFilter` in the core): empty or none
    // keeps them all
    /// the roles of the player in the rotation
    #[uniffi(default)]
    pub roles: Vec<RoleEnum>,
    /// our reception in the rally
    #[uniffi(default)]
    pub reception: Vec<EvalEnum>,
    /// our touch before, in the rally
    #[uniffi(default)]
    pub previous: Vec<EvalEnum>,
    /// the set of the attack
    #[uniffi(default)]
    pub set_types: Vec<SetTypeEnum>,
    /// how many received their serve
    #[uniffi(default)]
    pub scheme: Option<ReceptionSchemeEnum>,
    /// in P1 in side out, the P1 inverted or not
    #[uniffi(default)]
    pub inverted_p1: Option<bool>,
    /// the end of the set (from 20, from 10 in the tie-break) or before
    #[uniffi(default)]
    pub late: Option<bool>,
    /// a close score (two points apart at most) or not
    #[uniffi(default)]
    pub close: Option<bool>,
    /// for our serve, where it went only from this zone; the clips only
    /// from there
    #[uniffi(default)]
    pub serve_start: Option<ServeStartEnum>,
    /// the digs counted as defense: the ones that aren't covers, only the
    /// covers; none, all of them (the other fundamentals count every event)
    #[uniffi(default)]
    pub digs: Option<DigsEnum>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PhaseChoice {
    pub value: PhaseEnum,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RotationChoice {
    pub value: u8,
    pub name: String,
}

/// The events with an evaluation.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct EvalStats {
    pub eval: EvalEnum,
    /// what the scouts write: #, +, !, /, -, =
    pub symbol: String,
    pub name: String,
    pub tone: Tone,
    pub count: u32,
    /// of the events of the fundamental, 0 to 100
    pub share: f64,
}

/// The attacks set to a zone (see `ZoneStats`) and the kills among them.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DetailZone {
    pub zone: u8,
    pub share: f64,
    pub conversion: f64,
}

/// The rallies it takes to win a point.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RalliesPerPoint {
    pub per_point: f64,
    pub won: u32,
    pub rallies: u32,
}

/// A player in a fundamental, to compare.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlayerStats {
    pub player: ScoutingPlayer,
    pub total: u32,
    pub efficiency: Option<f64>,
    pub positiveness: Option<f64>,
    /// only for a serve, an attack or a block
    pub points: Option<u32>,
    pub errors: u32,
    pub grade: Option<f64>,
}

/// A fundamental in detail (see `detail::Detail` in the core).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FundamentalDetail {
    pub total: u32,
    /// the evaluations given at least once, from the best to the worst
    pub evals: Vec<EvalStats>,
    pub efficiency: Option<f64>,
    pub grade: Option<f64>,
    pub positiveness: Option<f64>,
    pub errors: u32,
    pub unforced_errors: u32,
    /// none with a player: the rallies have none
    pub phases_per_point: Option<RalliesPerPoint>,
    pub faults: u32,
    pub points: Option<u32>,
    /// for an attack
    pub zones: Vec<DetailZone>,
    /// for a serve, an attack, a reception, a dig or a block: how many had
    /// their zone told, and the court a row each from the top (theirs for the
    /// serve and the attack, the nine zones as `CourtZoneEnum::zones_shown`;
    /// ours for the reception and the dig, with the conflict zones between
    /// them as `CourtZoneEnum::shown`; for the block one row, the zones at
    /// the net as `CourtZoneEnum::net_shown`); empty with no zone told
    pub targets_told: u32,
    pub targets: Vec<Vec<TargetStats>>,
    /// for a reception or a dig, all the conflict zones together
    pub on_conflicts: Option<ConflictStats>,
    /// for an attack or a block: how many had how many blocked told, and
    /// each of the ones it can tell; empty with none told
    pub blockers_told: u32,
    pub blockers: Vec<BlockersStats>,
    /// for a serve: how many had where it started from told, and the five
    /// zones it starts from as shown below their court (5, 7, 6, 9, 1 from
    /// the left, see `ServeStartEnum::SHOWN`); empty with none told
    pub serve_starts_told: u32,
    pub serve_starts: Vec<ServeStartStats>,
    /// for a serve: where they went from where they started, both told, the
    /// most first (to draw them the fewest first); only from the zone of
    /// the filter; empty with none
    pub serve_paths: Vec<ServePathStats>,
    /// the players who played it, the most involved first; with the
    /// filter but its player
    pub players: Vec<PlayerStats>,
}

/// Our attacks against so many of theirs at the block, or our blocks with
/// so many of ours, of the ones told.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BlockersStats {
    pub value: BlockersEnum,
    /// e.g. "2 blockers"
    pub name: String,
    pub count: u32,
    /// 0 to 100
    pub share: f64,
    /// -100 to 100: none on too few to tell
    pub efficiency: Option<f64>,
}

/// Our serves started from a zone, of the ones with where they started
/// from told.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ServeStartStats {
    pub value: ServeStartEnum,
    /// 1, 9, 6, 7 or 5
    pub label: String,
    pub count: u32,
    /// 0 to 100
    pub share: f64,
    /// -100 to 100: none on too few to tell
    pub efficiency: Option<f64>,
    /// the errors among them: they went nowhere on their court
    pub errors: u32,
}

/// Our serves from a zone of our end line to a zone of their court, of
/// the ones with both told. Where on the whole court, in meters, as our
/// server sees it: `x` from 0 to 9 from the left, `y` from 0 at their end
/// line to 9 at the net and 18 at ours (where the serve starts, the zone
/// under its fifth of the end line).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ServePathStats {
    pub start: ServeStartEnum,
    pub zone: CourtZoneEnum,
    /// "1 → 5"
    pub label: String,
    pub from_x: f64,
    pub from_y: f64,
    pub to_x: f64,
    pub to_y: f64,
    /// two cells of different zones count half in each
    pub count: f64,
    /// of the serves with both told, 0 to 100
    pub share: f64,
    /// -100 to 100: none on too few to tell
    pub efficiency: Option<f64>,
}

/// Our receptions on any conflict zone, of the ones whose zone was told.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ConflictStats {
    pub count: u32,
    /// 0 to 100
    pub share: f64,
    /// -100 to 100: none on too few to tell
    pub efficiency: Option<f64>,
}

/// Our serves or attacks to a zone of their court, or our receptions in
/// one of ours or on a conflict zone, of the ones whose zone was told.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TargetStats {
    pub value: CourtZoneEnum,
    /// as the scout reads it: "4", "4/3"
    pub label: String,
    /// one of the nine zones, not a conflict zone
    pub zone: bool,
    /// two cells of different zones count half in each zone
    pub count: f64,
    /// of the serves whose zone was told, 0 to 100
    pub share: f64,
    /// -100 to 100: none on too few serves to tell
    pub efficiency: Option<f64>,
}

/// Their freeballs we exploited.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Exploited {
    pub percentage: f64,
    pub exploited: u32,
    pub total: u32,
}

/// The freeballs we gave, the ones they gave us and how many of these we
/// exploited: team events, with a player none given and the others not
/// told.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FreeballStats {
    pub given: u32,
    pub received: Option<u32>,
    pub exploited: Option<Exploited>,
}

/// A fundamental, or the freeballs, narrowed by the filter, with the
/// choices of the filter.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct DetailStats {
    pub scope: StatsScope,
    pub sets: Vec<u8>,
    pub event: EventTypeEnum,
    pub filter: DetailFilter,
    /// serve, reception, dig, block, attack, freeballs
    pub fundamentals: Vec<EventChoice>,
    /// the players with an event of the fundamental in the match so far
    /// (and the one chosen, even with none), by their number
    pub players: Vec<ScoutingPlayer>,
    pub rotations: Vec<RotationChoice>,
    pub phases: Vec<PhaseChoice>,
    /// one of them: the fundamental, or the freeballs
    pub fundamental: Option<FundamentalDetail>,
    pub freeballs: Option<FreeballStats>,
}

/// Scouting the set in progress of a match, the player first (see
/// `ChoiceOrder::PlayerFirst` in the core): every complete event is saved at
/// once.
#[derive(uniffi::Object)]
pub struct Scouting {
    runtime: Arc<Runtime>,
    session: Mutex<ScoutingSession>,
    labels: &'static Labels,
}

#[uniffi::export]
impl Scouting {
    pub fn state(&self) -> ScoutingState {
        let session = self.session();
        let snapshot = session.snapshot();
        let lineup = &snapshot.current_lineup;
        let current_match = session.current_match();
        let team = session.team();
        let side = session.side();
        let (won, lost) = current_match.get_status().map_or((0, 0), |s| match side {
            ScoutSide::A => (s.us_wins, s.them_wins),
            ScoutSide::B => (s.them_wins, s.us_wins),
        });
        let choices = session.player_choices();
        let selectable = |id: &Uuid| match (session.step(), session.player()) {
            (ScoutingStep::Event, None) => !session.player_events(id).is_empty(),
            (ScoutingStep::Player, _) => choices.iter().any(|c| c.id == *id),
            _ => false,
        };
        let setter = lineup.get_setter();
        let libero = lineup.get_current_libero();
        let (spots, receivers) = match (snapshot.get_formation(), lineup.get_current_rotation()) {
            (Formation::Reception, Ok(rotation)) => (
                RECEPTION
                    .get(usize::from(rotation))
                    .copied()
                    .unwrap_or(ZONES),
                RECEIVERS
                    .get(usize::from(rotation))
                    .copied()
                    .unwrap_or_default(),
            ),
            _ => (ZONES, [0; 3]),
        };
        let spotlight = snapshot.get_spotlight();
        let stats = spotlight.as_ref().map(|_| session.match_stats());
        let efficiency = |id: Uuid| {
            spotlight
                .as_ref()
                .filter(|(_, players)| players.contains(&id))
                .zip(stats.as_ref())
                .map(|((fundamental, _), stats)| {
                    stats.metric_percentage(
                        *fundamental,
                        Metric::Efficiency,
                        &StatsFilter::new(None, None, Some(id)),
                    )
                })
        };
        let court = snapshot
            .get_playing_positions()
            .into_iter()
            .zip(spots)
            .zip(1..)
            .filter_map(|((id, spot), position)| team.find_player(id).map(|p| (p, spot, position)))
            .map(|(p, (x, y), position)| CourtPlayer {
                id: p.id.to_string(),
                name: p.name.clone(),
                number: p.number,
                setter: setter == Some(p.id),
                libero: libero == p.id,
                selectable: selectable(&p.id),
                spotlight: efficiency(p.id).is_some(),
                efficiency: efficiency(p.id).flatten(),
                grade: spotlight
                    .as_ref()
                    .and_then(|(fundamental, _)| Measure::of(*fundamental))
                    .zip(efficiency(p.id).flatten())
                    .and_then(|(m, v)| grade(team.classification, team.gender, m, v)),
                x,
                y,
                receives: receivers.contains(&position),
            })
            .collect();
        let events: Vec<ScoutedEvent> = match session.is_both() {
            true => session
                .touches()
                .iter()
                .rev()
                .filter_map(|(of, e)| {
                    session.team_of(*of).map(|t| ScoutedEvent {
                        side_b: *of == ScoutSide::B,
                        ..self.scouted_event(t, e)
                    })
                })
                .collect(),
            false => {
                let mut events: Vec<&EventEntry> = session.set().events.iter().collect();
                events.sort_by_key(|e| e.timestamp);
                events
                    .into_iter()
                    .rev()
                    .map(|e| self.scouted_event(team, e))
                    .collect()
            }
        };
        ScoutingState {
            set_number: session.set().set_number,
            team: team.name.clone(),
            opponent: session
                .team_of(side.other())
                .map_or_else(|| current_match.opponent.clone(), |t| t.name.clone()),
            // the home side of the match is A's: B is on screen away
            home: current_match.home == (side == ScoutSide::A),
            score_us: snapshot.score_us,
            score_them: snapshot.score_them,
            won,
            lost,
            serving_team: match lineup.get_current_phase() {
                PhaseEnum::Break => TeamSideEnum::Us,
                PhaseEnum::SideOut => TeamSideEnum::Them,
            },
            rotation: lineup
                .get_current_rotation()
                .map_or_else(|_| "-".to_string(), |r| rotation_name(r, self.labels)),
            court,
            substitutions_left: lineup
                .get_max_substitutions()
                .saturating_sub(u8::try_from(lineup.get_substitutions().len()).unwrap_or(u8::MAX)),
            step: session.step(),
            halfway: session.is_halfway(),
            event: session.event().map(|e| self.event_choice(&session, e)),
            player: session
                .player()
                .and_then(|id| team.find_player(id))
                .map(|p| self.scouting_player(p)),
            events,
            ball: snapshot.get_ball_side(),
            last_touch: snapshot
                .get_last_touch()
                .map(|(e, _)| self.scouted_event(team, e)),
            inverted_p1: session.inverted_p1(),
            inverted_p1_applies: session.inverted_p1_applies(),
            reception: session.reception().players(),
            reception_open: session.can_tell_reception(),
            team_choice: session.team_choice(),
            both: session.is_both(),
            side_b: side == ScoutSide::B,
            other_rotation: session
                .snapshot_of(side.other())
                .and_then(|s| s.current_lineup.get_current_rotation().ok())
                .map_or_else(String::new, |r| rotation_name(r, self.labels)),
        }
    }

    /// The events that can follow: scouting both sides, with their side
    /// (the active one's first), see `ScoutingSession::event_choices`.
    pub fn event_choices(&self) -> Vec<EventChoice> {
        let session = self.session();
        session
            .event_choices()
            .into_iter()
            .map(|(of, e)| EventChoice {
                side_b: of == ScoutSide::B,
                ..self.event_choice(&session, e)
            })
            .collect()
    }

    /// The scout chooses the event of a side, among `event_choices`.
    pub fn choose_event_of(
        &self,
        side_b: bool,
        event: EventTypeEnum,
    ) -> Result<Progress, MobileError> {
        let side = match side_b {
            true => ScoutSide::B,
            false => ScoutSide::A,
        };
        let mut session = self.session();
        self.scouted(self.runtime.block_on(session.choose_event_of(side, event)))
    }

    /// Swaps the side the choices are for (F5): true when it is B's now.
    pub fn swap_side(&self) -> Result<bool, MobileError> {
        let mut session = self.session();
        self.scouted(session.swap_side().map(|side| side == ScoutSide::B))
    }

    /// The events that can follow.
    pub fn available_events(&self) -> Vec<EventChoice> {
        let session = self.session();
        session
            .available_events()
            .iter()
            .map(|e| self.event_choice(&session, *e))
            .collect()
    }

    /// The events the player can play now, before an event is chosen.
    pub fn player_events(&self, player: String) -> Result<Vec<EventChoice>, MobileError> {
        let player = parse_id(&player)?;
        let session = self.session();
        Ok(session
            .player_events(&player)
            .into_iter()
            .map(|e| self.event_choice(&session, e))
            .collect())
    }

    /// The events that come before their player: the opponent's, the
    /// faults, the changes of the lineup; scouting both sides, the other
    /// side's too, with their side.
    pub fn other_events(&self) -> Vec<EventChoice> {
        let session = self.session();
        session
            .other_events()
            .into_iter()
            .map(|(of, e)| EventChoice {
                side_b: of == ScoutSide::B,
                ..self.event_choice(&session, e)
            })
            .collect()
    }

    /// The player who touches the ball: then the event, if the player can
    /// play more than one, or the evaluation.
    pub fn choose_player_first(&self, player: String) -> Result<Progress, MobileError> {
        let player = parse_id(&player)?;
        let mut session = self.session();
        self.scouted(self.runtime.block_on(session.choose_player_first(player)))
    }

    /// The players who can play the event, or be replaced: setter, outside
    /// hitter 1, middle blocker 2, opposite, outside hitter 2, middle
    /// blocker 1, libero.
    pub fn player_choices(&self) -> Vec<ScoutingPlayer> {
        let session = self.session();
        self.choices(&session, session.player_choices())
    }

    /// The players who can replace the chosen one.
    pub fn replacement_choices(&self) -> Vec<ScoutingPlayer> {
        let session = self.session();
        self.choices(&session, session.replacement_choices())
    }

    pub fn available_evals(&self) -> Vec<EvalChoice> {
        let session = self.session();
        session
            .event()
            .map(|event| {
                session
                    .available_evals()
                    .into_iter()
                    .map(|eval| eval_choice(eval, event, self.labels))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The sets the attacker can receive, or the serves.
    pub fn available_techniques(&self) -> Vec<TechniqueChoice> {
        let session = self.session();
        session
            .available_techniques()
            .into_iter()
            .map(|t| technique_choice(t, session.technique() == Some(t), self.labels))
            .collect()
    }

    /// How many blocked, to tell along with the evaluation: against our
    /// attack (none too) and on our block; empty for the other events.
    pub fn available_blockers(&self) -> Vec<BlockersChoice> {
        let session = self.session();
        session
            .available_blockers()
            .iter()
            .map(|b| BlockersChoice {
                value: *b,
                name: b.name(self.labels).to_string(),
                chosen: session.blockers() == Some(*b),
            })
            .collect()
    }

    /// The zones our serve starts from, as shown below their court (5, 7,
    /// 6, 9, 1 from the left): along with the evaluation of our serve,
    /// where the server served from last chosen; empty for the other
    /// events.
    pub fn serve_starts(&self) -> Vec<ServeStartChoice> {
        let session = self.session();
        match session.serve_start_applies() {
            true => ServeStartEnum::SHOWN
                .iter()
                .map(|z| ServeStartChoice {
                    value: *z,
                    label: z.to_string(),
                    chosen: session.serve_start() == Some(*z),
                })
                .collect(),
            false => vec![],
        }
    }

    /// The scout touches a zone our serve starts from: told, or taken back
    /// when it was. Saved with the evaluation, never required.
    pub fn touch_serve_start(&self, start: ServeStartEnum) -> Result<(), MobileError> {
        let touched = self.session().touch_serve_start(start);
        self.scouted(touched)
    }

    /// The scout tells how many blocked, or takes it back (`None`). Saved
    /// with the evaluation, never required.
    pub fn choose_blockers(&self, blockers: Option<BlockersEnum>) -> Result<(), MobileError> {
        let chosen = self.session().choose_blockers(blockers);
        self.scouted(chosen)
    }

    pub fn choose_event(&self, event: EventTypeEnum) -> Result<Progress, MobileError> {
        let mut session = self.session();
        self.scouted(self.runtime.block_on(session.choose_event(event)))
    }

    pub fn choose_player(&self, player: String) -> Result<Progress, MobileError> {
        let player = parse_id(&player)?;
        let mut session = self.session();
        self.scouted(self.runtime.block_on(session.choose_player(player)))
    }

    /// The team makes the event, no player: our fault.
    pub fn choose_team(&self) -> Result<Progress, MobileError> {
        let mut session = self.session();
        self.scouted(self.runtime.block_on(session.choose_team()))
    }

    pub fn choose_replacement(&self, player: String) -> Result<Progress, MobileError> {
        let player = parse_id(&player)?;
        let mut session = self.session();
        self.scouted(self.runtime.block_on(session.choose_replacement(player)))
    }

    /// Saved with the events from now on.
    pub fn set_inverted_p1(&self, inverted: bool) {
        self.session().set_inverted_p1(inverted);
    }

    /// The next reception scheme for their serve: 3, 4, 2, then 3 again.
    pub fn next_reception(&self) -> Result<(), MobileError> {
        let told = self.session().next_reception().map(|_| ());
        self.scouted(told)
    }

    pub fn choose_technique(&self, technique: TechniqueEnum) -> Result<(), MobileError> {
        let chosen = self.session().choose_technique(technique);
        self.scouted(chosen)
    }

    /// The court where the ball goes: for our serve or attack theirs as
    /// our server sees it (their end line at the top), for our reception
    /// or dig ours as seen from the bench (the net at the top); six rows of
    /// six cells, two by two in a zone. For our block one row, the zones at
    /// the net on ours (4, 3, 2: see `target_at_net`). Empty when it can't
    /// be told now.
    pub fn target_court(&self) -> Vec<Vec<CourtCellChoice>> {
        let session = self.session();
        let chosen = session.target();
        match (session.target_side(), session.target_kind()) {
            (Some(side), Some(TargetKind::Net)) => vec![
                CourtZoneEnum::net_shown(side)
                    .iter()
                    .filter_map(|zone| zone.number().map(|n| (zone, n)))
                    .map(|(zone, n)| CourtCellChoice {
                        zone: n,
                        subzone: String::new(),
                        label: zone.to_string(),
                        chosen: chosen == Some(CourtTarget::Zone(n)),
                    })
                    .collect(),
            ],
            (Some(side), _) => CourtCell::shown(side)
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| CourtCellChoice {
                            zone: cell.zone_number(),
                            subzone: cell.subzone().to_string(),
                            label: cell.to_string(),
                            chosen: chosen.is_some_and(|t| t.has(cell)),
                        })
                        .collect()
                })
                .collect(),
            (None, _) => vec![],
        }
    }

    /// Whether the court is told at the net, only a whole zone: for our
    /// block.
    pub fn target_at_net(&self) -> bool {
        self.session().target_kind() == Some(TargetKind::Net)
    }

    /// Whether the court where the ball goes is ours (the net at the top),
    /// not theirs (the net at the bottom).
    pub fn target_ours(&self) -> bool {
        self.session().target_side() == Some(CourtSide::Ours)
    }

    /// What's told of where the ball went: "1D", "1D-6A (6/1)"; none
    /// when nothing is.
    pub fn target_told(&self) -> Option<String> {
        self.session().target().map(|t| t.described())
    }

    /// The scout touches a cell: alone, or along with the one told next to
    /// it; touched again, taken back. With no subzone, a whole zone at the
    /// net, for our block. Saved with the evaluation, never required.
    pub fn touch_target(&self, zone: u8, subzone: String) -> Result<(), MobileError> {
        let cell = subzone
            .chars()
            .next()
            .and_then(SubzoneEnum::of)
            .and_then(|s| CourtCell::new(zone, s));
        let touched = match (subzone.is_empty(), cell, CourtZoneEnum::numbered(zone)) {
            (true, _, Some(zone)) => self.session().touch_zone(zone),
            (false, Some(cell), _) => self.session().touch_target(cell),
            _ => Err(ScoutingError::InvalidSelection),
        };
        self.scouted(touched)
    }

    pub fn choose_eval(&self, eval: EvalEnum) -> Result<Progress, MobileError> {
        let mut session = self.session();
        self.scouted(self.runtime.block_on(session.choose_eval(eval)))
    }

    /// The result of the match once the set just over ended it, none while
    /// it goes on.
    pub fn match_result(&self) -> Result<Option<MatchResult>, MobileError> {
        let session = self.session();
        let m = session.current_match();
        let status = m.get_status()?;
        Ok(match status.match_finished {
            true => Some(MatchResult {
                won: status.us_wins,
                lost: status.them_wins,
                sets: set_scores(m)?,
            }),
            false => None,
        })
    }

    /// Goes back a choice, or removes the last saved event.
    pub fn undo(&self) -> Result<(), MobileError> {
        let mut session = self.session();
        self.scouted(self.runtime.block_on(session.undo()))
    }

    /// The statistics of a set of the match so far, or of all of them,
    /// with their trend rally by rally, the figures narrowed by the filter.
    pub fn stats(&self, scope: StatsScope, filter: DetailFilter) -> Result<LiveStats, MobileError> {
        let session = self.session();
        let current_match = session.current_match();
        live_stats(
            &session.sets(),
            scope,
            current_match.max_substitutions,
            (&current_match.team, &current_match.opponent),
            self.labels,
            filter,
        )
    }

    /// The same for B of a match scouted on both sides: its sets as B sees
    /// them, B's team first. `None` scouting only A.
    pub fn stats_of_b(
        &self,
        scope: StatsScope,
        filter: DetailFilter,
    ) -> Result<Option<LiveStats>, MobileError> {
        let session = self.session();
        let current_match = session.current_match();
        side_b(&session)
            .map(|(sets, team)| {
                live_stats(
                    &sets.iter().collect::<Vec<_>>(),
                    scope,
                    current_match.max_substitutions,
                    (team, &current_match.team.name),
                    self.labels,
                    filter,
                )
            })
            .transpose()
    }

    /// A fundamental of a set of the match so far, or of all of them,
    /// narrowed by the filter.
    pub fn detail(
        &self,
        scope: StatsScope,
        event: EventTypeEnum,
        filter: DetailFilter,
    ) -> Result<DetailStats, MobileError> {
        let session = self.session();
        let current_match = session.current_match();
        detail_stats(
            &session.sets(),
            current_match.max_substitutions,
            &current_match.team,
            self.labels,
            scope,
            event,
            filter,
        )
    }

    /// The same for B of a match scouted on both sides.
    pub fn detail_of_b(
        &self,
        scope: StatsScope,
        event: EventTypeEnum,
        filter: DetailFilter,
    ) -> Result<Option<DetailStats>, MobileError> {
        let session = self.session();
        side_b(&session)
            .map(|(sets, team)| {
                detail_stats(
                    &sets.iter().collect::<Vec<_>>(),
                    session.current_match().max_substitutions,
                    team,
                    self.labels,
                    scope,
                    event,
                    filter,
                )
            })
            .transpose()
    }
}

/// B's sets so far and its team, scouting both sides; `None` scouting A.
fn side_b(session: &ScoutingSession) -> Option<(Vec<SetEntry>, &TeamEntry)> {
    session.sets_of_b().zip(session.team_of(ScoutSide::B))
}

/// The statistics of a set of the match, or of all of them, with their
/// trend rally by rally: `all` are the sets of the match so far, in order.
fn live_stats(
    all: &[&SetEntry],
    scope: StatsScope,
    max_substitutions: u8,
    (team, opponent): (&TeamEntry, &str),
    labels: &Labels,
    filter: DetailFilter,
) -> Result<LiveStats, MobileError> {
    let stats = stats_of(&in_scope(all, scope), max_substitutions)?;
    let (trend, points) = trend_lines(all, max_substitutions, team, labels)?;
    Ok(LiveStats {
        scope,
        filter: filter.clone(),
        sets: all.iter().map(|s| s.set_number).collect(),
        trend,
        points,
        team_name: team.name.clone(),
        opponent: opponent.to_string(),
        ..summary(&stats, &narrowed(&filter)?, team, labels)
    })
}

/// What the coach wants to know of the statistics, narrowed by the
/// filter: with no sets to choose and no trend (see `live_stats`).
fn summary(stats: &Stats, filter: &StatsFilter, team: &TeamEntry, labels: &Labels) -> LiveStats {
    let level = |measure: Measure, value: Option<f64>| {
        value.and_then(|v| grade(team.classification, team.gender, measure, v))
    };
    let attack_stats = |a: AttackSummary| AttackStats {
        total: a.total,
        kills: a.kills,
        errors: a.errors,
        blocked: a.blocked,
        efficiency: a.efficiency(),
        grade: level(Measure::Attack, Some(a.efficiency())),
    };
    let o = Overview::narrowed(stats, filter);
    let fundamental = |event: EventTypeEnum, f: Fundamental| FundamentalStats {
        event,
        name: event.friendly_name(labels).to_string(),
        total: f.total,
        efficiency: f.efficiency,
        perfect: f.perfect,
        errors: f.errors,
        grade: Measure::of(event).and_then(|m| level(m, f.efficiency)),
    };
    let phase = |p: Option<(f64, u32, u32)>, measure: Option<Measure>| {
        p.map(|(percentage, won, rallies)| PhaseStats {
            percentage,
            won,
            rallies,
            balance: 2 * i64::from(won) - i64::from(rallies),
            grade: measure.and_then(|m| level(m, Some(percentage))),
        })
    };
    let errors = |e: &Errors| {
        e.by_event
            .iter()
            .map(|(event, count)| ErrorCount {
                event: *event,
                count: *count,
            })
            .collect()
    };
    let zones = |counter_attack| {
        distribution_in(stats, counter_attack, filter)
            .into_iter()
            .map(|z| ZoneStats {
                zone: zone_number(z.zone),
                share: z.share,
                attacks: attack_stats(z.attacks),
            })
            .collect()
    };
    LiveStats {
        scope: StatsScope::Match,
        sets: vec![],
        fundamentals: vec![
            fundamental(EventTypeEnum::P, o.reception),
            fundamental(EventTypeEnum::A, o.attack),
            fundamental(EventTypeEnum::S, o.serve),
            fundamental(EventTypeEnum::B, o.block),
            fundamental(EventTypeEnum::D, o.dig),
        ],
        team_figures: !filter.of_players(),
        side_out: phase(o.side_out, Some(Measure::SideOut)),
        side_out_possessions: o
            .side_out_possessions
            .iter()
            .zip(labels.possessions)
            .map(|((_, summary), name)| PossessionStats {
                name: name.to_string(),
                total: summary.map_or(0, |s| s.total),
                won: summary.map_or(0, |s| s.won),
                percentage: summary.map(|s| s.finalization()),
            })
            .collect(),
        break_points: phase(o.break_points, Some(Measure::BreakPoint)),
        first_attack: o.first_attack.map(attack_stats),
        counter_attack: o.counter_attack.map(attack_stats),
        unforced_errors: o.unforced_errors.total,
        unforced_by_event: errors(&o.unforced_errors),
        forced_errors: o.forced_errors.total,
        forced_by_event: errors(&o.forced_errors),
        aces: o.aces,
        aces_against: o.aces_against,
        opponent_errors: o.opponent_errors,
        opponent_points: o.opponent_points,
        distribution: zones(None),
        first_attack_distribution: zones(Some(false)),
        counter_attack_distribution: zones(Some(true)),
        graded: team.classification.is_some() && team.gender.is_some(),
        trend: vec![],
        points: vec![],
        team_name: team.name.clone(),
        opponent: String::new(),
        rotations: by_rotation_in(stats, filter)
            .into_iter()
            .map(|r| RotationStats {
                rotation: r.rotation,
                name: rotation_name(r.rotation, labels),
                total: phase(r.total, None),
                side_out: phase(r.side_out, Some(Measure::SideOut)),
                break_points: phase(r.break_points, Some(Measure::BreakPoint)),
            })
            .collect(),
        filter: DetailFilter::default(),
        rotation_choices: rotation_choices(labels),
        phase_choices: phase_choices(labels),
    }
}

/// The rotations to narrow the figures to, the six of them.
fn rotation_choices(labels: &Labels) -> Vec<RotationChoice> {
    (0..6)
        .map(|r| RotationChoice {
            value: r,
            name: rotation_name(r, labels),
        })
        .collect()
}

/// The phases to narrow the figures to.
fn phase_choices(labels: &Labels) -> Vec<PhaseChoice> {
    [PhaseEnum::SideOut, PhaseEnum::Break]
        .into_iter()
        .map(|p| PhaseChoice {
            value: p,
            name: p.friendly_name(labels).to_string(),
        })
        .collect()
}

/// The sets of the scope, among the sets of the match so far.
fn in_scope<'a>(all: &[&'a SetEntry], scope: StatsScope) -> Vec<&'a SetEntry> {
    all.iter()
        .copied()
        .filter(|s| match scope {
            StatsScope::Set { number } => s.set_number == number,
            StatsScope::Match => true,
        })
        .collect()
}

/// A fundamental (one of `detail::FUNDAMENTALS`) of a set of the match, or
/// of all of them, narrowed by the filter: `all` are the sets of the match
/// so far, in order.
fn detail_stats(
    all: &[&SetEntry],
    max_substitutions: u8,
    team: &TeamEntry,
    labels: &Labels,
    scope: StatsScope,
    event: EventTypeEnum,
    filter: DetailFilter,
) -> Result<DetailStats, MobileError> {
    let stats = stats_of(&in_scope(all, scope), max_substitutions)?;
    let everyone = stats_of(all, max_substitutions)?;
    Ok(DetailStats {
        scope,
        sets: all.iter().map(|s| s.set_number).collect(),
        ..detail_of(&stats, &everyone, &[team], labels, event, filter)?
    })
}

/// A fundamental (one of `detail::FUNDAMENTALS`) of the statistics, narrowed
/// by the filter: the players to choose are the ones with an event of
/// the fundamental in `everyone` (and the one chosen), found in the first of `teams` having them (the team as each
/// match saw it); the grades by the level of the first team. With no sets
/// to choose (see `detail_stats`).
fn detail_of(
    stats: &Stats,
    everyone: &Stats,
    teams: &[&TeamEntry],
    labels: &Labels,
    event: EventTypeEnum,
    filter: DetailFilter,
) -> Result<DetailStats, MobileError> {
    let narrowed = narrowed(&filter)?;
    let team = teams.first().copied().cloned().unwrap_or_default();
    let team = &team;
    let level = |value: Option<f64>| {
        Measure::of(event)
            .zip(value)
            .and_then(|(m, v)| grade(team.classification, team.gender, m, v))
    };
    let player_of = |id: Uuid| {
        teams
            .iter()
            .find_map(|t| t.find_player(id).map(|p| scouting_player(p, labels)))
    };
    let (fundamental, freeballs) =
        match Breakdown::from_start(stats, event, &narrowed, filter.serve_start) {
            Breakdown::Fundamental(d) => (
                Some(FundamentalDetail {
                    total: d.total,
                    evals: d
                        .evals
                        .iter()
                        .map(|e| EvalStats {
                            eval: e.eval,
                            symbol: e.eval.to_string(),
                            name: e.eval.friendly_name(event, labels).to_string(),
                            tone: e.tone,
                            count: e.count,
                            share: e.share,
                        })
                        .collect(),
                    efficiency: d.efficiency,
                    grade: level(d.efficiency),
                    positiveness: d.positiveness,
                    errors: d.errors,
                    unforced_errors: d.unforced_errors,
                    phases_per_point: d.phases_per_point.map(|(per_point, won, rallies)| {
                        RalliesPerPoint {
                            per_point,
                            won,
                            rallies,
                        }
                    }),
                    faults: d.faults,
                    points: d.points,
                    zones: d
                        .zones
                        .iter()
                        .map(|z| DetailZone {
                            zone: zone_number(z.zone),
                            share: z.share,
                            conversion: z.conversion,
                        })
                        .collect(),
                    targets_told: d.targets_told,
                    targets: match (d.targets.is_empty(), d.conflicts.is_empty()) {
                        (true, _) => vec![],
                        (false, conflicts) => {
                            let side = CourtSide::of(event).unwrap_or(CourtSide::Theirs);
                            let rows = match (TargetKind::of(event), conflicts) {
                                (Some(TargetKind::Net), _) => vec![CourtZoneEnum::net_shown(side)],
                                (_, true) => CourtZoneEnum::zones_shown(side),
                                (_, false) => CourtZoneEnum::shown(side),
                            };
                            rows.iter()
                                .map(|row| {
                                    row.iter()
                                        .filter_map(|zone| {
                                            d.targets
                                                .iter()
                                                .chain(d.conflicts.iter())
                                                .find(|t| t.zone == *zone)
                                        })
                                        .map(|t| TargetStats {
                                            value: t.zone,
                                            label: t.zone.to_string(),
                                            zone: t.zone.is_zone(),
                                            count: t.count,
                                            share: t.share,
                                            efficiency: t.efficiency,
                                        })
                                        .collect()
                                })
                                .collect()
                        }
                    },
                    on_conflicts: d.on_conflicts.map(|on| ConflictStats {
                        count: on.count,
                        share: on.share,
                        efficiency: on.efficiency,
                    }),
                    blockers_told: d.blockers_told,
                    blockers: d
                        .blockers
                        .iter()
                        .map(|b| BlockersStats {
                            value: b.blockers,
                            name: b.blockers.name(labels).to_string(),
                            count: b.count,
                            share: b.share,
                            efficiency: b.efficiency,
                        })
                        .collect(),
                    serve_starts_told: d.serve_starts_told,
                    serve_starts: ServeStartEnum::SHOWN
                        .iter()
                        .filter_map(|z| d.serve_starts.iter().find(|s| s.start == *z))
                        .map(|s| ServeStartStats {
                            value: s.start,
                            label: s.start.to_string(),
                            count: s.count,
                            share: s.share,
                            efficiency: s.efficiency,
                            errors: s.errors,
                        })
                        .collect(),
                    serve_paths: d.serve_paths.iter().filter_map(serve_path).collect(),
                    players: by_player(stats, event, &narrowed)
                        .into_iter()
                        .filter_map(|line| {
                            player_of(line.player).map(|player| PlayerStats {
                                player,
                                total: line.total,
                                efficiency: line.efficiency,
                                positiveness: line.positiveness,
                                points: line.points,
                                errors: line.errors,
                                grade: level(line.efficiency),
                            })
                        })
                        .collect(),
                }),
                None,
            ),
            Breakdown::Freeballs(f) => (
                None,
                Some(FreeballStats {
                    given: f.given,
                    received: f.received,
                    exploited: f.exploited.map(|(percentage, total, exploited)| Exploited {
                        percentage,
                        exploited,
                        total,
                    }),
                }),
            ),
        };
    // who has the fundamental, and the ones chosen even with none
    let players = choosable(involved(everyone, event), &narrowed, teams, labels);
    let fundamental_name = |e: EventTypeEnum| match e {
        EventTypeEnum::FB => labels.freeballs.to_string(),
        _ => e.friendly_name(labels).to_string(),
    };
    Ok(DetailStats {
        scope: StatsScope::Match,
        sets: vec![],
        event,
        filter,
        fundamentals: FUNDAMENTALS
            .iter()
            .map(|e| EventChoice {
                value: *e,
                name: fundamental_name(*e),
                side_b: false,
            })
            .collect(),
        players,
        rotations: rotation_choices(labels),
        phases: phase_choices(labels),
        fundamental,
        freeballs,
    })
}

/// The statistics filter of the app's filter.
fn narrowed(filter: &DetailFilter) -> Result<StatsFilter, MobileError> {
    let players = filter
        .players
        .iter()
        .map(|p| parse_id(p))
        .collect::<Result<Vec<_>, _>>()?;
    let some = |evals: &[EvalEnum]| (!evals.is_empty()).then(|| EvalSet::of(evals));
    Ok(StatsFilter::new(filter.phase, None, None)
        .with_rotations(&filter.rotations)
        .with_players(players)
        .with_situation(SituationFilter {
            roles: RoleSet::of(&filter.roles),
            reception: some(&filter.reception),
            previous: some(&filter.previous),
            set_types: (!filter.set_types.is_empty()).then(|| SetTypeSet::of(&filter.set_types)),
            scheme: filter.scheme,
            inverted_p1: filter.inverted_p1,
            late: filter.late,
            close: filter.close,
        })
        .with_digs(filter.digs.unwrap_or_default()))
}

/// The players with an event of ours of the kind `event` in the
/// statistics, by their id.
fn involved(stats: &Stats, event: EventTypeEnum) -> Vec<Uuid> {
    with_events(stats, |k| k.event_type == event)
}

/// The players with an event of ours of any kind in the statistics (a
/// touch of any fundamental, a fault), by their id: a libero only
/// receiving and digging too, a player never on court not.
pub(crate) fn touched(stats: &Stats) -> Vec<Uuid> {
    with_events(stats, |_| true)
}

/// The players of the events of ours `keep` keeps, by their id, once.
fn with_events(stats: &Stats, keep: impl Fn(&EventKey) -> bool) -> Vec<Uuid> {
    let mut players: Vec<Uuid> = stats
        .events
        .iter(|k| k.player.is_some() && keep(k))
        .filter_map(|(k, _)| k.player)
        .collect();
    players.sort_unstable();
    players.dedup();
    players
}

/// The players a filter offers: `ids`, and the ones chosen even with no
/// event (the filter shown, to take them off); each found in the first of
/// `teams` having it (the team as it is, then as each match saw it), one
/// found in none left out; by their number.
pub(crate) fn choosable(
    mut ids: Vec<Uuid>,
    narrowed: &StatsFilter,
    teams: &[&TeamEntry],
    labels: &Labels,
) -> Vec<ScoutingPlayer> {
    for chosen in &narrowed.players {
        if !ids.contains(chosen) {
            ids.push(*chosen);
        }
    }
    let mut players: Vec<ScoutingPlayer> = ids
        .into_iter()
        .filter_map(|id| {
            teams
                .iter()
                .find_map(|t| t.find_player(id).map(|p| scouting_player(p, labels)))
        })
        .collect();
    players.sort_by_key(|p| p.number);
    players
}

/// The rotation by the position of the setter: P1 to P6 in Italian, S1 to
/// S6 in English.
fn rotation_name(rotation: u8, labels: &Labels) -> String {
    format!("{}{}", labels.setter_prefix, rotation + 1)
}

fn eval_choice(eval: EvalEnum, event: EventTypeEnum, labels: &Labels) -> EvalChoice {
    EvalChoice {
        value: eval,
        symbol: eval.to_string(),
        name: eval.friendly_name(event, labels),
        description: eval.friendly_description(event, labels),
    }
}

fn technique_choice(t: TechniqueEnum, chosen: bool, labels: &Labels) -> TechniqueChoice {
    TechniqueChoice {
        value: t,
        code: t.to_string(),
        short_name: match t {
            TechniqueEnum::Set(set) => set.friendly_name(labels),
            TechniqueEnum::Serve(serve) => Some(serve.friendly_name(labels)),
        }
        .map(str::to_string),
        name: t.description(labels),
        chosen,
    }
}

fn scouted_event(team: &TeamEntry, e: &EventEntry, labels: &Labels) -> ScoutedEvent {
    ScoutedEvent {
        event: e.event_type,
        name: e.event_type.friendly_name(labels).to_string(),
        player: e
            .player
            .and_then(|id| team.find_player(id))
            .map(|p| scouting_player(p, labels)),
        eval: e.eval.map(|v| v.friendly_name(e.event_type, labels)),
        symbol: e.eval.map(|v| v.to_string()),
        technique: e.technique.map(|t| t.to_string()),
        target: e.flags.target().map(|t| t.described()),
        blockers: e.flags.blockers().map(|b| b.name(labels).to_string()),
        serve_start: e.flags.serve_start().map(|z| z.to_string()),
        side_b: false,
    }
}

/// The player of `team`, the role in its gender.
fn scouting_player(p: &PlayerEntry, labels: &Labels) -> ScoutingPlayer {
    ScoutingPlayer {
        id: p.id.to_string(),
        name: p.name.clone(),
        number: p.number,
        role: p
            .role
            .map_or_else(String::new, |r| r.friendly_name(labels).to_string()),
        initials: names::initials(&p.name),
        out_for_the_libero: false,
    }
}

/// A serve path drawn on the whole court (see `ServePathStats`): from the
/// fifth of our end line of its start to its zone of their court, both
/// ends moved off the middle so that the arrows don't lie on each other:
/// the start towards the column it goes to, the end towards the fifth it
/// came from and, for the zones of a column not to be on one line, to the
/// left at their end line, to the right at the net (within the zone). None for a zone not one of the nine.
fn serve_path(p: &ServePath) -> Option<ServePathStats> {
    let (row, column) = CourtZoneEnum::zones_shown(CourtSide::Theirs)
        .iter()
        .enumerate()
        .find_map(|(r, row)| row.iter().position(|z| *z == p.zone).map(|c| (r, c)))?;
    let fifth = ServeStartEnum::SHOWN.iter().position(|s| *s == p.start)?;
    let middle = |i: usize, of: f64| (i as f64 + 0.5) * of;
    Some(ServePathStats {
        start: p.start,
        zone: p.zone,
        label: format!("{} → {}", p.start, p.zone),
        from_x: middle(fifth, 9.0 / 5.0) + (column as f64 - 1.0) * 0.5,
        from_y: 18.0,
        to_x: (middle(column, 3.0) + (fifth as f64 - 2.0) * 0.45 + (row as f64 - 1.0) * 0.4)
            .clamp(column as f64 * 3.0 + 0.3, column as f64 * 3.0 + 2.7),
        to_y: middle(row, 3.0),
        count: p.count,
        share: p.share,
        efficiency: p.efficiency,
    })
}

/// The number scouts call a zone by.
fn zone_number(zone: ZoneEnum) -> u8 {
    use ZoneEnum::*;
    match zone {
        One => 1,
        Two => 2,
        Three => 3,
        Four => 4,
        Five => 5,
        Six => 6,
        Seven => 7,
        Eight => 8,
        Nine => 9,
    }
}

/// Each figure in each set, from its first rally, and in the match.
fn trend_lines(
    sets: &[&SetEntry],
    max_substitutions: u8,
    team: &TeamEntry,
    labels: &Labels,
) -> Result<(Vec<TrendLine>, Vec<SetPoints>), MobileError> {
    // replayed once: each set is a part of the match
    let match_replay = Replay::new(sets, max_substitutions)?;
    let set_replays: Vec<(u8, Replay)> = sets
        .iter()
        .map(|set| (set.set_number, match_replay.set(set.set_number)))
        .collect();
    let values = |points: Vec<TrendPoint<Option<f64>>>| {
        points.into_iter().map(|p| p.value).collect::<Vec<_>>()
    };
    let last = |points: Vec<TrendPoint<Option<f64>>>| points.last().and_then(|p| p.value);
    let points = set_replays
        .iter()
        .map(|(set_number, replay)| SetPoints {
            set_number: *set_number,
            points: replay
                .point_sequence()
                .into_iter()
                .map(|(side, score)| PointScored {
                    us: side == TeamSideEnum::Us,
                    score,
                })
                .collect(),
        })
        .collect();
    let level = |series: Series, value: Option<f64>| {
        value.and_then(|v| grade(team.classification, team.gender, series.measure(), v))
    };
    let lines = Series::ALL
        .iter()
        .map(|series| {
            let overall = last(match_replay.series(*series, false));
            let overall_recent = last(match_replay.series(*series, true));
            TrendLine {
                series: *series,
                name: series_name(*series, labels),
                band: team
                    .classification
                    .zip(team.gender)
                    .map(|(level, gender)| band(level, gender, series.measure()))
                    .map(|b| GradeBand {
                        low: b.low,
                        high: b.high,
                    }),
                sets: set_replays
                    .iter()
                    .map(|(set_number, replay)| SetTrend {
                        set_number: *set_number,
                        progress: replay.progress(),
                        values: values(replay.series(*series, false)),
                        recent: values(replay.series(*series, true)),
                        errors: replay.errors(*series, false),
                        unforced: replay.errors(*series, true),
                    })
                    .collect(),
                overall,
                overall_recent,
                overall_grade: level(*series, overall),
                overall_recent_grade: level(*series, overall_recent),
            }
        })
        .collect();
    Ok((lines, points))
}

fn series_name(series: Series, labels: &Labels) -> String {
    match series {
        Series::Reception => EventTypeEnum::P.friendly_name(labels),
        Series::Attack => EventTypeEnum::A.friendly_name(labels),
        Series::CounterAttack => labels.counter_attack,
        Series::Serve => EventTypeEnum::S.friendly_name(labels),
        Series::SideOut => PhaseEnum::SideOut.friendly_name(labels),
        Series::BreakPoint => PhaseEnum::Break.friendly_name(labels),
    }
    .to_string()
}

impl Scouting {
    /// The session, even if a call panicked while holding it: each choice
    /// leaves it whole.
    fn session(&self) -> MutexGuard<'_, ScoutingSession> {
        self.session
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn scouted<T>(&self, result: Result<T, ScoutingError>) -> Result<T, MobileError> {
        result.map_err(|e| MobileError::Scouting {
            reason: e.message(self.labels),
        })
    }

    /// The players of the team, with their role in the language of the app.
    fn choices(
        &self,
        session: &ScoutingSession,
        choices: Vec<PlayerChoice>,
    ) -> Vec<ScoutingPlayer> {
        choices
            .into_iter()
            .filter_map(|c| {
                session.team().find_player(c.id).map(|p| ScoutingPlayer {
                    out_for_the_libero: c.out_for_the_libero,
                    ..self.scouting_player(p)
                })
            })
            .collect()
    }

    /// The event with its name, the libero replacement saying what it does
    /// now: out, or back in. Of the session already locked by the caller.
    fn event_choice(&self, session: &ScoutingSession, event: EventTypeEnum) -> EventChoice {
        EventChoice {
            value: event,
            name: session
                .snapshot()
                .next_event_name(event, self.labels)
                .to_string(),
            side_b: false,
        }
    }

    fn scouted_event(&self, team: &TeamEntry, e: &EventEntry) -> ScoutedEvent {
        scouted_event(team, e, self.labels)
    }

    fn scouting_player(&self, p: &PlayerEntry) -> ScoutingPlayer {
        scouting_player(p, self.labels)
    }
}

/// An event of the match, in the list beside its video (desktop only).
#[derive(Debug, Clone, PartialEq)]
pub struct VideoEvent {
    pub set_number: u8,
    pub event: SetEvent,
    /// when it was scouted (or in the video, for a match on the video
    /// clock): what an anchor ties to the video
    pub timestamp: DateTime<Utc>,
    /// where it is in the video, from the start of the first file
    pub offset_ms: Option<i64>,
    /// put at its own time by the user: how far from where the anchor puts
    /// it (`VideoProfile::correction`)
    pub corrected_ms: Option<i64>,
    /// what the profile keeps it by: its own place in the stream of its
    /// side, or, of B's, the event of A of the same touch (`stored_ref`)
    pub stored: EventRef,
    /// after it, for the rally to come: our rotation ("-" without the
    /// setter on the court) and the phase, to show over the video (the
    /// score is the event's)
    pub rotation: String,
    pub phase: PhaseEnum,
}

/// The events behind a statistic of the detail of a fundamental, to see
/// them in the video (desktop only): the scope, the fundamental and the
/// filter of the detail, and, when chosen, some evaluations, the zone of
/// the net of the attacks (2, 3, 4, 8, 9) and a zone of the court
/// (`TargetStats::value`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, uniffi::Record)]
pub struct ClipPick {
    pub scope: StatsScope,
    pub event: EventTypeEnum,
    pub filter: DetailFilter,
    pub evals: Vec<EvalEnum>,
    pub zone: Option<u8>,
    pub target: Option<CourtZoneEnum>,
}

/// For the apps written in Rust (the desktop app), not exported to the
/// bindings: they drive the scouting from the keyboard as the TUI does.
impl Scout4All {
    /// What time it is, for the app: the clock the core asks too.
    pub fn clock(&self) -> Arc<dyn Clock> {
        self.clock.clone()
    }

    /// Scouts the set in progress of the match, choosing in `order`: the
    /// keyboard chooses the event first, the touch the player.
    pub fn scouting_in_order(
        &self,
        team_id: &str,
        match_id: &str,
        order: ChoiceOrder,
    ) -> Result<Arc<Scouting>, MobileError> {
        let m = self.read_match(team_id, match_id)?;
        match next_set(&m.get_status()?, &m.team) {
            NextSet::Resume(set) => self.resumed(team_id, match_id, m, set).map(|session| {
                Arc::new(Scouting {
                    runtime: self.runtime.clone(),
                    session: Mutex::new(
                        session
                            .with_order(order)
                            .with_finished(self.finished_matches()),
                    ),
                    labels: self.language.labels(),
                })
            }),
            NextSet::Start(_) | NextSet::Over => Err(MobileError::Failed {
                reason: format!("no set in progress in {match_id}"),
            }),
        }
    }

    /// The set in progress scouted again: on both sides when it has the
    /// touches of B. Why the match or B can't be read is told as it is.
    fn resumed(
        &self,
        team_id: &str,
        match_id: &str,
        m: MatchEntry,
        set: SetEntry,
    ) -> Result<ScoutingSession, MobileError> {
        let labels = self.language.labels();
        let failed = |e: set_start::SetStartError| MobileError::Failed {
            reason: e.message(labels),
        };
        match self.set_reader.read_touches(&m, set.set_number)? {
            None => set_start::resume(m, set, self.set_writer.clone()).map_err(failed),
            Some(touches) => {
                let b = self.read_match_of(team_id, match_id, MatchSide::B)?;
                set_start::resume_both(m, set, touches, b.team, self.set_writer.clone())
                    .map(|session| session.with_earlier_of_b(&b.sets))
                    .map_err(failed)
            }
        }
    }

    /// The match and the profile of its video (`video` in the core): only
    /// the desktop app plays it.
    pub fn video(
        &self,
        team_id: &str,
        match_id: &str,
    ) -> Result<(MatchEntry, StoredProfile), MobileError> {
        self.video_of(team_id, match_id, MatchSide::A)
    }

    /// The match of team A `team_id` as the side sees it, with the profile
    /// of its video: it's A's, B's events are placed by their touches
    /// (`origins_of_b`).
    pub fn video_of(
        &self,
        team_id: &str,
        match_id: &str,
        side: MatchSide,
    ) -> Result<(MatchEntry, StoredProfile), MobileError> {
        let m = self.read_match_of(team_id, match_id, side)?;
        let profile = self
            .runtime
            .block_on(self.videos.read(&self.read_team(team_id)?.id, &m.id))
            .map_err(|e| unreadable(&e, self.language.labels().unreadable_match))?;
        Ok((m, profile))
    }

    /// The events of the match, set by set, and where each is in its
    /// video, the files `durations_ms` long (none: the video and the events
    /// not lined up).
    ///
    /// The profile corrects the events of A: of B's (`origins`, from
    /// `origins_of_b`) each takes the correction of the event of A born
    /// from the same touch, if there is one.
    pub fn video_events(
        &self,
        m: &MatchEntry,
        profile: &VideoProfile,
        durations_ms: &[u64],
        origins: Option<&Origins>,
    ) -> Result<Vec<VideoEvent>, MobileError> {
        let mut sets: Vec<&SetEntry> = m.sets.iter().collect();
        sets.sort_by_key(|s| s.set_number);
        let mut events = vec![];
        for set in sets {
            let labels = self.language.labels();
            let rows = set_event_rows(m, set, labels)?;
            let states = set.states(m.max_substitutions)?;
            events.extend(
                set.events
                    .iter()
                    .zip(rows)
                    .zip(states)
                    .map(|((e, row), state)| {
                        let stored = stored_ref(origins, set.set_number, row.index as usize);
                        let corrected_ms = profile.correction(stored);
                        VideoEvent {
                            set_number: set.set_number,
                            event: row,
                            timestamp: e.timestamp,
                            offset_ms: place(profile, m, durations_ms, e.timestamp)
                                .map(|ms| ms + corrected_ms.unwrap_or(0)),
                            corrected_ms,
                            stored,
                            rotation: state
                                .rotation
                                .map_or_else(|| "-".to_string(), |r| rotation_name(r, labels)),
                            phase: state.phase,
                        }
                    }),
            );
        }
        Ok(events)
    }

    /// The events of the match the statistics counted for the pick, in
    /// order: the ones to see one after the other in the video.
    pub fn match_clips(
        &self,
        team_id: &str,
        match_id: &str,
        pick: &ClipPick,
    ) -> Result<Vec<EventRef>, MobileError> {
        self.match_clips_of(team_id, match_id, MatchSide::A, pick)
    }

    /// The same for the side: B's events, by their own indexes.
    pub fn match_clips_of(
        &self,
        team_id: &str,
        match_id: &str,
        side: MatchSide,
        pick: &ClipPick,
    ) -> Result<Vec<EventRef>, MobileError> {
        let m = self.read_match_of(team_id, match_id, side)?;
        clips_of(&m, pick)
    }

    /// For each event of B's side, set by set, the event of A's born from
    /// the same touch (`SqliteMatchReader::origins_of_b`): what the
    /// corrections of the video, saved by A's events, are read by.
    pub fn origins_of_b(&self, team_id: &str, match_id: &str) -> Result<Origins, MobileError> {
        let team = self.read_team(team_id)?;
        self.match_reader
            .origins_of_b(&team, match_id)
            .map_err(|e| unreadable(&e, self.language.labels().unreadable_match))
    }

    /// Saves the profile of the video of the match.
    pub fn save_video(&self, m: &MatchEntry, profile: &VideoProfile) -> Result<(), MobileError> {
        self.save_video_of(&m.team.id.to_string(), m, profile)
    }

    /// The same for the match of team A `team_id`, as either side sees it
    /// (the profile is A's: the match of B has B's team).
    pub fn save_video_of(
        &self,
        team_id: &str,
        m: &MatchEntry,
        profile: &VideoProfile,
    ) -> Result<(), MobileError> {
        self.runtime
            .block_on(
                self.videos
                    .write(&self.read_team(team_id)?.id, &m.id, profile),
            )
            .map_err(|e| MobileError::Failed {
                reason: e.to_string(),
            })
    }
}

/// For the apps written in Rust, not exported to the bindings: the session
/// of the core, for what the keyboard does as in the TUI (the players by
/// their index, the cursor on the court, how many blocked).
impl Scouting {
    /// Runs `f` on the session, with the runtime for its async choices.
    /// `f` must not call the other methods of `Scouting`: they lock the
    /// session again.
    pub fn with_session<R>(&self, f: impl FnOnce(&mut ScoutingSession, &Runtime) -> R) -> R {
        let mut session = self.session();
        f(&mut session, &self.runtime)
    }

    /// The error in the language of the app.
    pub fn message(&self, e: &ScoutingError) -> String {
        e.message(self.labels)
    }
}

/// What is being corrected: an event of a set of A alone, or a touch of a
/// set scouted on both sides, with the starts of the sides to save it and
/// the team of B.
// one at a time, kept for a correction: no need to box
#[allow(clippy::large_enum_variant)]
enum Editing {
    Own(EventEdit),
    Both {
        edit: TouchEdit,
        a: SetEntry,
        b: SetEntry,
        team_b: TeamEntry,
    },
}

/// Correcting an event of a set (see `event_edit` in the core), or a touch
/// of a set of both sides (`touch_edit`): saved with
/// `Scout4All::save_correction`.
#[derive(uniffi::Object)]
pub struct Correction {
    current_match: MatchEntry,
    edit: Mutex<Editing>,
    labels: &'static Labels,
}

#[uniffi::export]
impl Correction {
    pub fn state(&self) -> CorrectionState {
        let edit = self.edit();
        let labels = self.labels;
        let correcting = edit.correcting();
        let (event, before) = (correcting.event(), correcting.before());
        let (team, side_b) = match &*edit {
            Editing::Both {
                edit: e, team_b, ..
            } if e.side() == scout4all_core::domain::Side::B => (team_b, true),
            _ => (&self.current_match.team, false),
        };
        CorrectionState {
            index: u32::try_from(correcting.index()).unwrap_or(u32::MAX),
            event: ScoutedEvent {
                side_b,
                ..scouted_event(team, event, labels)
            },
            score_us: before.score_us,
            score_them: before.score_them,
            rotation: before
                .current_lineup
                .get_current_rotation()
                .map_or_else(|_| "-".to_string(), |r| rotation_name(r, labels)),
            phase: before
                .current_lineup
                .get_current_phase()
                .friendly_name(labels)
                .to_string(),
            fields: correcting.fields(),
            event_types: correcting
                .event_types()
                .into_iter()
                .map(|e| EventChoice {
                    value: e,
                    name: e.friendly_name(labels).to_string(),
                    side_b,
                })
                .collect(),
            evals: correcting
                .evals()
                .into_iter()
                .map(|eval| eval_choice(eval, event.event_type, labels))
                .collect(),
            eval: event.eval,
            techniques: correcting
                .techniques()
                .into_iter()
                .flatten()
                .map(|t| technique_choice(t, event.technique == Some(t), labels))
                .collect(),
        }
    }

    pub fn choose_event_type(&self, event: EventTypeEnum) {
        self.edit().correcting_mut().choose_event_type(event);
    }

    pub fn choose_eval(&self, eval: EvalEnum) {
        self.edit().correcting_mut().choose_eval(eval);
    }

    /// A set or serve, or none.
    pub fn choose_technique(&self, technique: Option<TechniqueEnum>) {
        self.edit().correcting_mut().choose_technique(technique);
    }
}

impl Editing {
    /// The correction, whichever it is.
    fn correcting(&self) -> &dyn Correcting {
        match self {
            Editing::Own(edit) => edit,
            Editing::Both { edit, .. } => edit,
        }
    }

    fn correcting_mut(&mut self) -> &mut dyn Correcting {
        match self {
            Editing::Own(edit) => edit,
            Editing::Both { edit, .. } => edit,
        }
    }
}

impl Correction {
    fn edit(&self) -> MutexGuard<'_, Editing> {
        self.edit
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The data of the app, in a folder like `~/.scout4all`: the database
/// (`scout4all.db`) and the settings (`config.json`, `clips.json`). The
/// folders of the teams of the versions before the database move into it
/// when the app starts (`legacy::migrate`).
#[derive(uniffi::Object)]
pub struct Scout4All {
    /// shared with the sets being scouted
    runtime: Arc<Runtime>,
    language: Language,
    db: Database,
    team_reader: SqliteTeamReader,
    team_writer: SqliteTeamWriter,
    match_reader: SqliteMatchReader,
    set_reader: SqliteSetReader,
    match_writer: SqliteMatchWriter,
    competitions: SqliteCompetitions,
    persons: SqlitePersons,
    training: SqliteTraining,
    set_writer: Arc<dyn SetWriter + Send + Sync>,
    videos: SqliteVideoProfiles,
    /// for `config.json`
    store: Arc<dyn Store>,
    /// what the start has to tell (the migration of the data folder),
    /// until the app asks for it
    notice: Mutex<Option<String>>,
    /// the matches to upload for the statistics
    queue: Arc<UploadQueueManager>,
    /// whether the user allows the statistics, as in `config.json`
    uploads_enabled: Arc<AtomicBool>,
    /// sends the queue in the background, once `send_in_background`
    sender: Arc<UploadSender>,
    /// what time it is: the system's, or the one the app gives
    clock: Arc<dyn Clock>,
}

type UploadQueueManager = QueueManager<SqliteQueue, SqliteQueue>;

#[uniffi::export]
impl Scout4All {
    /// The data in `data_dir`, created if missing; `language` is a code
    /// like `"it"`, English when it isn't supported. Opens the database
    /// (an error when it can't be opened, e.g. saved by a newer version)
    /// and moves into it what the versions before left in the folder,
    /// after a copy of it beside it (`startup_notice` tells what stayed).
    #[uniffi::constructor]
    pub fn new(data_dir: String, language: String) -> Result<Arc<Self>, MobileError> {
        Self::with_clock(data_dir, language, Arc::new(SystemClock))
    }

    /// As `new`, asking `clock` what time it is: the app's own, so that
    /// the app and the core tell the same time (a fixed one in the tests).
    #[uniffi::constructor]
    pub fn with_clock(
        data_dir: String,
        language: String,
        clock: Arc<dyn Clock>,
    ) -> Result<Arc<Self>, MobileError> {
        let data_dir = PathBuf::from(data_dir);
        let language = Language::new(LanguageEnum::from_str(&language).unwrap_or(LanguageEnum::En));
        set_errors_language(language.clone());
        std::fs::create_dir_all(&data_dir).map_err(|e| failed(AppError::IO(IOError::from(e))))?;
        let store: Arc<dyn Store> = Arc::new(FsStore::new(&data_dir));
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| MobileError::Failed {
                reason: e.to_string(),
            })?;
        let db = open_database(&data_dir.join(layout::DATABASE), language.labels())?;
        let notice = migrate_folder(&runtime, &*store, &db, &data_dir, language.labels());
        let queue = Arc::new(SqliteQueue::new(db.clone()));
        let app = Self {
            runtime: Arc::new(runtime),
            queue: Arc::new(QueueManager::new(queue.clone(), queue)),
            uploads_enabled: Arc::new(AtomicBool::new(false)),
            sender: UploadSender::new(FIRST_RETRY, LAST_RETRY),
            team_reader: SqliteTeamReader::new(db.clone()),
            team_writer: SqliteTeamWriter::new(db.clone()),
            match_writer: SqliteMatchWriter::new(db.clone()),
            competitions: SqliteCompetitions::new(db.clone()),
            persons: SqlitePersons::new(db.clone()),
            training: SqliteTraining::new(db.clone()),
            set_writer: Arc::new(SqliteSetWriter::new(db.clone(), language.clone())),
            match_reader: SqliteMatchReader::new(db.clone(), language.clone()),
            set_reader: SqliteSetReader::new(db.clone(), language.clone()),
            videos: SqliteVideoProfiles(db.clone()),
            db,
            store,
            notice: Mutex::new(notice),
            language,
            clock,
        };
        // unreadable settings: nothing sent until the terms are accepted
        let enabled = app.read_config().is_ok_and(|c| c.consent.uploads_enabled());
        app.uploads_enabled.store(enabled, Ordering::Relaxed);
        Ok(Arc::new(app))
    }

    /// What the start has to tell, once: what of the data folder of the
    /// versions before couldn't be moved into the database (with where its
    /// copy is), or why nothing could. None after the first call.
    pub fn startup_notice(&self) -> Option<String> {
        self.notice
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    /// The teams, the pinned ones at the top, each group by name.
    pub fn teams(&self) -> Result<Vec<TeamListing>, MobileError> {
        // settings that can't be read: no pins, the teams all the same
        let pins = self.read_config().map(|c| c.pinned).unwrap_or_default();
        let teams = self.runtime.block_on(self.team_reader.read_all())?;
        Ok(pins
            .sorted(teams)
            .into_iter()
            .map(|listed| match listed {
                Ok(team) => TeamListing::Team {
                    id: team.id.to_string(),
                    players: count(&team),
                    pinned: pins.is_pinned(&team.id),
                    name: team.name,
                    year: team.year,
                },
                Err(unreadable) => TeamListing::Unreadable {
                    folder: unreadable.id,
                },
            })
            .collect())
    }

    /// Pins the team at the top of the list, or unpins it.
    pub fn set_pinned(&self, team_id: String, pinned: bool) -> Result<(), MobileError> {
        let id = parse_id(&team_id)?;
        self.update_config(|config| AppConfig {
            pinned: config.pinned.with(id, pinned),
            ..config
        })
    }

    /// The team with this id.
    pub fn team(&self, id: String) -> Result<TeamDetails, MobileError> {
        let team = self.read_team(&id)?;
        let labels = self.language.labels();
        Ok(TeamDetails {
            id: team.id.to_string(),
            name: team.name.clone(),
            year: team.year,
            classification: team
                .classification
                .map(|c| c.friendly_name(labels).to_string()),
            gender: team.gender.map(|g| g.friendly_name(labels).to_string()),
            country: team.country.map(|c| c.name(self.language.get())),
            players: players(&team, labels),
            // a team saved before v0.12: the country of the settings
            form: TeamForm {
                country: team
                    .country
                    .or_else(|| self.default_country().map(|c| c.value)),
                ..TeamForm::of(&team)
            },
        })
    }

    /// The player's participation, points, training presence and other
    /// teams in this season.
    pub fn player_profile(
        &self,
        team_id: String,
        player_id: String,
    ) -> Result<PlayerProfile, MobileError> {
        let player_id = parse_id(&player_id)?;
        let team = self.read_team(&team_id)?;
        let player = team
            .find_player(player_id)
            .filter(|player| !player.deleted)
            .ok_or_else(|| {
                failed(format!(
                    "player {player_id} is not active in team {team_id}"
                ))
            })?;
        let matches = self
            .runtime
            .block_on(self.match_reader.read_scouted(&team))?;
        let stats = PlayerSeasonStats::of(player_id, &matches)?;

        let totals = self
            .runtime
            .block_on(self.training.totals(&team.id, TrainingScope::All))?;
        let attendance = self.runtime.block_on(
            self.training
                .attendance_report(&team.id, TrainingScope::All),
        )?;
        let attendance_percent = attendance
            .players
            .iter()
            .find(|player| player.person_id == player_id)
            .and_then(|player| player.percent());
        let other_teams = self
            .persons
            .teams_of(&player_id)?
            .into_iter()
            .filter(|other| other.team != team.id && other.year == team.year)
            .map(|other| ProfileTeam {
                id: other.team.to_string(),
                name: other.name,
                year: other.year,
                number: other.number,
                role: other.role,
            })
            .collect();

        Ok(PlayerProfile {
            initials: names::initials(&player.name),
            age: player
                .birth_date
                .and_then(|birth| age(birth, self.clock.today())),
            matches_played: stats.matches_played,
            matches_scouted: stats.matches_scouted,
            sets_played: stats.sets_played,
            points: stats.points(),
            attack_points: stats.attack_points,
            block_points: stats.block_points,
            serve_points: stats.serve_points,
            has_training: has_training(&totals),
            attendance_percent,
            other_teams,
        })
    }

    /// The matches the team plays, as A or as B, from its side: the most
    /// recent first, then the unreadable ones.
    pub fn matches(&self, team_id: String) -> Result<Vec<MatchListing>, MobileError> {
        let team = self.read_team(&team_id)?;
        Ok(self
            .runtime
            .block_on(self.match_reader.played(&team))?
            .into_iter()
            .map(|listed| match listed {
                Ok(PlayedMatch {
                    summary: m,
                    scouted_by,
                }) => MatchListing::Match {
                    date: m.date.to_rfc3339(),
                    id: m.id,
                    opponent: m.opponent,
                    home: m.home,
                    won: m.us_wins,
                    lost: m.them_wins,
                    finished: m.finished,
                    scouted_by: scouted_by.map(|a| a.to_string()),
                },
                Err(unreadable) => MatchListing::Unreadable {
                    folder: unreadable.id,
                    reason: unreadable.reason,
                },
            })
            .collect())
    }

    /// What team B of a match of the team may be, offered as it's written:
    /// the other teams, by name and the most recent year first, then the
    /// names it met that aren't a team, each once.
    pub fn opponent_choices(&self, team_id: String) -> Result<Vec<OpponentChoice>, MobileError> {
        let team = self.read_team(&team_id)?;
        let mut teams: Vec<TeamEntry> = self
            .runtime
            .block_on(self.team_reader.read_all())?
            .into_iter()
            .flatten()
            .filter(|t| t.id != team.id)
            .collect();
        teams.sort_by(|a, b| (a.name.to_lowercase(), b.year).cmp(&(b.name.to_lowercase(), a.year)));
        let taken: Vec<String> = teams
            .iter()
            .map(|t| t.name.trim().to_lowercase())
            .chain(std::iter::once(team.name.trim().to_lowercase()))
            .collect();
        let mut met: Vec<String> = self
            .runtime
            .block_on(self.match_reader.read_summaries(&team))?
            .into_iter()
            .flatten()
            .map(|m| m.opponent.trim().to_string())
            .filter(|name| !name.is_empty() && !taken.contains(&name.to_lowercase()))
            .collect();
        met.sort_by_key(|name| name.to_lowercase());
        met.dedup_by_key(|name| name.to_lowercase());
        Ok(teams
            .into_iter()
            .map(|t| OpponentChoice {
                team: Some(t.id),
                name: t.name,
                year: Some(t.year),
            })
            .chain(met.into_iter().map(|name| OpponentChoice {
                team: None,
                name,
                year: None,
            }))
            .collect())
    }

    /// The competitions to choose for a match, by name, the most recent
    /// season first.
    pub fn competitions(&self) -> Result<Vec<CompetitionChoice>, MobileError> {
        Ok(self
            .competitions
            .read_all()?
            .into_iter()
            .map(|c| CompetitionChoice {
                id: c.id,
                name: c.name,
                season: c.season,
                format: c.format,
                friendly: c.friendly,
            })
            .collect())
    }

    /// The competitions in their list, by name, the most recent season
    /// first, each with its matches.
    pub fn competition_listings(&self) -> Result<Vec<CompetitionListing>, MobileError> {
        let labels = self.language.labels();
        Ok(self
            .competitions
            .read_counted()?
            .into_iter()
            .map(|(c, matches)| CompetitionListing {
                id: c.id,
                name: c.name,
                season: c.season,
                format: c.format.name(labels).to_string(),
                matches,
            })
            .collect())
    }

    /// The form of the competition, to edit it.
    pub fn competition_form(&self, id: Uuid) -> Result<CompetitionForm, MobileError> {
        Ok(CompetitionForm::of(&self.competition(id)?))
    }

    /// Saves the competition as edited: its matches keep their own
    /// friendly.
    pub fn save_competition(&self, id: Uuid, form: CompetitionForm) -> Result<(), MobileError> {
        let existing = self.competition(id)?;
        let edited = form
            .validate(&existing, &self.competitions.read_all()?)
            .map_err(|e| MobileError::InvalidCompetition {
                field: e.into(),
                reason: e.message(self.language.labels()),
            })?;
        Ok(self.competitions.update(&edited)?)
    }

    /// Removes the competition: its matches stay, without it.
    pub fn delete_competition(&self, id: Uuid) -> Result<(), MobileError> {
        Ok(self.competitions.remove(id)?)
    }

    /// The stages of a competition a match may be played in.
    pub fn stages(&self) -> Vec<StageChoice> {
        let labels = self.language.labels();
        Stage::ALL
            .iter()
            .map(|s| StageChoice {
                value: *s,
                name: s.name(labels).to_string(),
            })
            .collect()
    }

    /// The formats of a new competition.
    pub fn competition_formats(&self) -> Vec<FormatChoice> {
        let labels = self.language.labels();
        CompetitionFormat::ALL
            .iter()
            .map(|f| FormatChoice {
                value: *f,
                name: f.name(labels).to_string(),
            })
            .collect()
    }

    /// The home: the matches of every team not over and the last ones
    /// over, the most recent first, then the teams. What can't be read is
    /// left out of the matches; the teams list it.
    pub fn home(&self) -> Result<Home, MobileError> {
        let teams = self.teams()?;
        let mut matches: Vec<(DateTime<FixedOffset>, HomeMatch)> = Vec::new();
        for team in self
            .runtime
            .block_on(self.team_reader.read_all())?
            .into_iter()
            .flatten()
        {
            let read = self
                .runtime
                .block_on(self.match_reader.read_summaries(&team))?;
            matches.extend(read.into_iter().flatten().map(|m| {
                let next = next_of(&m);
                (
                    m.date,
                    HomeMatch {
                        team_id: team.id.to_string(),
                        team_name: team.name.clone(),
                        date: m.date.to_rfc3339(),
                        id: m.id,
                        opponent: m.opponent,
                        home: m.home,
                        won: m.us_wins,
                        lost: m.them_wins,
                        next,
                    },
                )
            }));
        }
        // the same time: by team, whatever order the teams were read in
        matches.sort_by(|(a, x), (b, y)| b.cmp(a).then_with(|| x.team_id.cmp(&y.team_id)));
        let (recent, in_progress): (Vec<HomeMatch>, Vec<HomeMatch>) = matches
            .into_iter()
            .map(|(_, m)| m)
            .partition(|m| m.next == NextSetKind::Over);
        Ok(Home {
            in_progress,
            recent: recent.into_iter().take(RECENT_MATCHES).collect(),
            teams,
        })
    }

    /// A match of the team, with its sets; a match it's B of, scouted by A
    /// only, as B sees it: nothing to scout from its side.
    pub fn match_details(
        &self,
        team_id: String,
        match_id: String,
    ) -> Result<MatchDetails, MobileError> {
        let team = self.read_team(&team_id)?;
        let seen_by_b = self.match_reader.played_as_b(&team, &match_id)?;
        let team_a = seen_by_b
            .as_ref()
            .map_or(team_id, |(a, _)| a.id.to_string());
        let m = self.read_match(&team_a, &match_id)?;
        let (row, _) = self.match_reader.details(&m.team, &match_id)?;
        let status = m.get_status()?;
        let sets = set_scores(&m)?;
        let details = MatchDetails {
            date: m.date.to_rfc3339(),
            id: m.id.clone(),
            opponent: m.opponent.clone(),
            home: m.home,
            won: status.us_wins,
            lost: status.them_wins,
            finished: status.match_finished,
            sets,
            next: next_kind(&status, &m.team),
            notes: m.notes.clone(),
            scouted_by: None,
            both_sides: row.scouting == ScoutingMode::Both,
            venue_id: row.venue_id.map(|v| v.to_string()),
        };
        Ok(match seen_by_b {
            Some((a, home)) => MatchDetails {
                opponent: a.name.clone(),
                home,
                won: details.lost,
                lost: details.won,
                sets: details
                    .sets
                    .into_iter()
                    .map(|set| SetScore {
                        us: set.them,
                        them: set.us,
                        ..set
                    })
                    .collect(),
                next: NextSetKind::Over,
                scouted_by: Some(ScoutedBy {
                    team_id: a.id.to_string(),
                    team_name: a.name,
                }),
                ..details
            },
            None => details,
        })
    }

    /// What removing the team does to its matches, to confirm it.
    pub fn team_removal(&self, team_id: String) -> Result<TeamRemovalCounts, MobileError> {
        let removal = self.team_reader.removal(&parse_id(&team_id)?)?;
        Ok(TeamRemovalCounts {
            kept: u32::try_from(removal.kept).unwrap_or(u32::MAX),
            removed: u32::try_from(removal.removed).unwrap_or(u32::MAX),
            sessions: u32::try_from(removal.sessions).unwrap_or(u32::MAX),
        })
    }

    /// The statistics of a set of the match, or of all of them, as while
    /// scouting, of the team of the side.
    pub fn match_stats(
        &self,
        team_id: String,
        match_id: String,
        side: MatchSide,
        scope: StatsScope,
        filter: DetailFilter,
    ) -> Result<LiveStats, MobileError> {
        let m = self.read_match_of(&team_id, &match_id, side)?;
        let mut sets: Vec<&SetEntry> = m.sets.iter().collect();
        sets.sort_by_key(|s| s.set_number);
        live_stats(
            &sets,
            scope,
            m.max_substitutions,
            (&m.team, &m.opponent),
            self.language.labels(),
            filter,
        )
    }

    /// A fundamental of a set of the match, or of all of them, narrowed by
    /// the filter, as while scouting, of the team of the side.
    pub fn match_detail(
        &self,
        team_id: String,
        match_id: String,
        side: MatchSide,
        scope: StatsScope,
        event: EventTypeEnum,
        filter: DetailFilter,
    ) -> Result<DetailStats, MobileError> {
        let m = self.read_match_of(&team_id, &match_id, side)?;
        let mut sets: Vec<&SetEntry> = m.sets.iter().collect();
        sets.sort_by_key(|s| s.set_number);
        detail_stats(
            &sets,
            m.max_substitutions,
            &m.team,
            self.language.labels(),
            scope,
            event,
            filter,
        )
    }

    /// The events of a set of the match, to correct them: the touches of
    /// both sides for a set scouted on both.
    pub fn set_events(
        &self,
        team_id: String,
        match_id: String,
        set_number: u8,
    ) -> Result<SetEvents, MobileError> {
        let m = self.read_match(&team_id, &match_id)?;
        let set = find_set(&m, set_number)?;
        let (both, events) = match self.set_reader.read_touches(&m, set_number)? {
            None => (false, set_event_rows(&m, set, self.language.labels())?),
            Some(touches) => {
                let b = self.read_match_of(&team_id, &match_id, MatchSide::B)?;
                (
                    true,
                    touch_rows(&m, &b.team, &touches, self.language.labels())?,
                )
            }
        };
        Ok(SetEvents {
            set_number,
            team: m.team.name.clone(),
            opponent: m.opponent.clone(),
            both,
            events,
        })
    }

    /// Starts correcting an event of a set of the match, a touch of either
    /// side for a set scouted on both.
    pub fn correction(
        &self,
        team_id: String,
        match_id: String,
        set_number: u8,
        index: u32,
    ) -> Result<Arc<Correction>, MobileError> {
        let m = self.read_match(&team_id, &match_id)?;
        let labels = self.language.labels();
        let index = usize::try_from(index).unwrap_or(usize::MAX);
        let failed = |e: scout4all_core::event_edit::EventEditError| MobileError::Failed {
            reason: e.message(labels).to_string(),
        };
        let edit = match self.set_reader.read_touches(&m, set_number)? {
            None => Editing::Own(
                EventEdit::new(find_set(&m, set_number)?, index, m.max_substitutions)
                    .map_err(failed)?,
            ),
            Some(touches) => Editing::Both {
                edit: TouchEdit::new(
                    &touches.a,
                    &touches.b,
                    &touches.touches,
                    index,
                    m.max_substitutions,
                )
                .map_err(failed)?,
                team_b: self.read_match_of(&team_id, &match_id, MatchSide::B)?.team,
                a: touches.a,
                b: touches.b,
            },
        };
        Ok(Arc::new(Correction {
            current_match: m,
            edit: Mutex::new(edit),
            labels,
        }))
    }

    /// Saves the set with the event corrected (with its pair, on both
    /// sides), if the events after it still fit and the set ends as it did.
    pub fn save_correction(&self, correction: Arc<Correction>) -> Result<(), MobileError> {
        let labels = self.language.labels();
        let failed = |e: scout4all_core::event_edit::EventEditError| MobileError::Failed {
            reason: e.message(labels).to_string(),
        };
        let saved = match &*correction.edit() {
            Editing::Own(edit) => {
                let set = edit.apply().map_err(failed)?;
                self.runtime
                    .block_on(self.set_writer.update(&correction.current_match, &set))
            }
            Editing::Both { edit, a, b, .. } => {
                let touches = edit.apply().map_err(failed)?;
                self.runtime.block_on(self.set_writer.update_sides(
                    &correction.current_match,
                    a,
                    Some(b),
                    &touches,
                ))
            }
        };
        saved?;
        // as the TUI: a finished match is uploaded again with the events
        // corrected (the set ends as it did)
        let m = &correction.current_match;
        if m.get_status().is_ok_and(|status| status.match_finished) {
            self.runtime
                .block_on(self.finished_matches().finished(m.team.id, m.id.clone()));
        }
        Ok(())
    }

    /// The start of the next set of the match, to choose who serves and the
    /// lineup.
    pub fn set_start(
        &self,
        team_id: String,
        match_id: String,
    ) -> Result<Arc<SetStart>, MobileError> {
        let m = self.read_match(&team_id, &match_id)?;
        match next_set(&m.get_status()?, &m.team) {
            NextSet::Start(form) => Ok(Arc::new(SetStart {
                team: m.team,
                match_id: m.id,
                opponent: m.opponent,
                form: Mutex::new(form),
                of_b: false,
                labels: self.language.labels(),
            })),
            NextSet::Resume(_) | NextSet::Over => Err(MobileError::Failed {
                reason: format!("no set to start in {match_id}"),
            }),
        }
    }

    /// The start of B in the same set, to choose its lineup: only for a
    /// match scouted on both sides, `None` for the others. The lineup of
    /// its last set is proposed.
    pub fn set_start_of_b(
        &self,
        team_id: String,
        match_id: String,
    ) -> Result<Option<Arc<SetStart>>, MobileError> {
        let team = self.read_team(&team_id)?;
        let both = self.match_reader.details(&team, &match_id)?.0.scouting == ScoutingMode::Both;
        match both {
            false => Ok(None),
            true => {
                let m = self.read_match_of(&team_id, &match_id, MatchSide::B)?;
                match next_set(&m.get_status()?, &m.team) {
                    NextSet::Start(form) => Ok(Some(Arc::new(SetStart {
                        team: m.team,
                        match_id: match_id.clone(),
                        opponent: self.read_match(&team_id, &match_id)?.team.name,
                        form: Mutex::new(form),
                        of_b: true,
                        labels: self.language.labels(),
                    }))),
                    NextSet::Resume(_) | NextSet::Over => Err(MobileError::Failed {
                        reason: format!("no set to start in {match_id}"),
                    }),
                }
            }
        }
    }

    /// Saves the set that `set_start` and `of_b` start, on both sides.
    pub fn start_set_both(
        &self,
        set_start: Arc<SetStart>,
        of_b: Arc<SetStart>,
    ) -> Result<(), MobileError> {
        let team_id = set_start.team.id.to_string();
        let m = self.read_match(&team_id, &set_start.match_id)?;
        let b = self.read_match_of(&team_id, &set_start.match_id, MatchSide::B)?;
        let failed = |e: set_start::SetStartError| MobileError::Failed {
            reason: e.message(self.language.labels()),
        };
        let form_b = of_b.form().clone();
        let input = set_start
            .form()
            .clone()
            .validate_both(&m.team, Some((&form_b, &b.team)))
            .map_err(failed)?;
        self.runtime
            .block_on(set_start::start(
                m,
                input,
                self.set_writer.clone(),
                Some(b.team),
            ))
            .map(|_| ())
            .map_err(failed)
    }

    /// Scouts the set in progress of the match.
    pub fn scouting(
        &self,
        team_id: String,
        match_id: String,
    ) -> Result<Arc<Scouting>, MobileError> {
        self.scouting_in_order(&team_id, &match_id, ChoiceOrder::PlayerFirst)
    }

    /// Saves the set that `set_start` starts.
    pub fn start_set(&self, set_start: Arc<SetStart>) -> Result<(), MobileError> {
        let m = self.read_match(&set_start.team.id.to_string(), &set_start.match_id)?;
        let failed = |e: set_start::SetStartError| MobileError::Failed {
            reason: e.message(self.language.labels()),
        };
        let input = set_start.form().clone().validate(&m.team).map_err(failed)?;
        self.runtime
            .block_on(set_start::start(m, input, self.set_writer.clone(), None))
            .map(|_| ())
            .map_err(failed)
    }

    /// The form of a new match, on `date` (now, where it's played).
    pub fn new_match_form(&self, date: MatchDate) -> MatchForm {
        MatchForm::new(date)
    }

    /// The match of the tutorial of the team, on `date`, ready to save: a
    /// new one, also when the tutorial is seen again.
    pub fn tutorial_match_form(
        &self,
        team_id: String,
        date: MatchDate,
    ) -> Result<MatchForm, MobileError> {
        let team = self.read_team(&team_id)?;
        // the matches by opponent and day, as the tutorial numbers them
        let taken: Vec<String> = self
            .runtime
            .block_on(self.match_reader.read_summaries(&team))?
            .into_iter()
            .filter_map(Result::ok)
            .map(|m| MatchEntry::id_of(&m.opponent, &m.date))
            .collect();
        Ok(example::tutorial_match(
            self.language.labels(),
            date,
            &taken,
        ))
    }

    /// Checks the fields of the form before its roster, the next step: the
    /// error of the first one not valid.
    pub fn check_match_fields(
        &self,
        team_id: String,
        match_id: Option<String>,
        form: MatchForm,
    ) -> Result<(), MobileError> {
        let team = self.read_team(&team_id)?;
        let existing = match_id
            .map(|id| self.read_match(&team_id, &id))
            .transpose()?;
        match form.validate(&team, existing.as_ref()) {
            Err(e) if !e.is_roster() => Err(MobileError::InvalidMatch {
                field: e.into(),
                reason: e.message(self.language.labels()),
            }),
            _ => Ok(()),
        }
    }

    /// The roster of the form of a match of the team, new (no `match_id`)
    /// or saved, with `toggled` put on the score sheet or taken off.
    pub fn match_roster(
        &self,
        team_id: String,
        match_id: Option<String>,
        form: MatchForm,
        toggled: Option<Uuid>,
    ) -> Result<MatchRoster, MobileError> {
        self.roster_after(team_id, match_id, form, |form, team, existing| {
            if let Some(player) = toggled {
                form.toggle(player, team, existing);
            }
            Ok(())
        })
    }

    /// The match form with `player` given `number` and `role` in this
    /// match (D9), and the players of its roster; an error, for the
    /// roster, when the number isn't valid or another player on it has it.
    pub fn correct_roster_number(
        &self,
        team_id: String,
        match_id: Option<String>,
        form: MatchForm,
        player: Uuid,
        number: String,
        role: Option<RoleEnum>,
    ) -> Result<MatchRoster, MobileError> {
        let labels = self.language.labels();
        self.roster_after(team_id, match_id, form, |form, team, existing| {
            form.correct(player, &number, role, team, existing)
                .map_err(|e| MobileError::InvalidMatch {
                    field: e.into(),
                    reason: e.message(labels),
                })
        })
    }

    /// The roster of team B of a match of team A scouted on both sides: its
    /// players, with `toggled` put on the score sheet or taken off. As the
    /// roster of A, with the form of the roster only (`MatchForm::default`
    /// the first time).
    pub fn roster_of_b(
        &self,
        team_id: String,
        match_id: String,
        form: MatchForm,
        toggled: Option<Uuid>,
    ) -> Result<MatchRoster, MobileError> {
        self.roster_of_b_after(team_id, match_id, form, |form, team, existing| {
            if let Some(player) = toggled {
                form.toggle(player, team, existing);
            }
            Ok(())
        })
    }

    /// `correct_roster_number` for team B.
    pub fn correct_roster_of_b_number(
        &self,
        team_id: String,
        match_id: String,
        form: MatchForm,
        player: Uuid,
        number: String,
        role: Option<RoleEnum>,
    ) -> Result<MatchRoster, MobileError> {
        let labels = self.language.labels();
        self.roster_of_b_after(team_id, match_id, form, |form, team, existing| {
            form.correct(player, &number, role, team, existing)
                .map_err(|e| MobileError::InvalidMatch {
                    field: e.into(),
                    reason: e.message(labels),
                })
        })
    }

    /// Saves the roster of team B as the form has it: as for A, from
    /// `MIN_ROSTER` to `MAX_ROSTER` players with a number each.
    pub fn save_roster_of_b(
        &self,
        team_id: String,
        match_id: String,
        form: MatchForm,
    ) -> Result<(), MobileError> {
        let m = self.read_match_of(&team_id, &match_id, MatchSide::B)?;
        let numbers =
            form.roster_of_b_numbers(&m.team, Some(&m))
                .map_err(|e| MobileError::InvalidMatch {
                    field: e.into(),
                    reason: e.message(self.language.labels()),
                })?;
        let a = self.read_match(&team_id, &match_id)?;
        self.runtime
            .block_on(self.match_writer.save_roster_of_b(&a, &numbers))?;
        Ok(())
    }

    /// The form of a match of the team, to edit it.
    pub fn match_form(&self, team_id: String, match_id: String) -> Result<MatchForm, MobileError> {
        let m = self.read_match(&team_id, &match_id)?;
        let (row, competition) = self.match_reader.details(&m.team, &match_id)?;
        Ok(MatchForm::of(&m, &row, competition.as_ref()))
    }

    /// Creates a match of the team (no `match_id`) or updates the one with
    /// `match_id`; returns its id.
    pub fn save_match(
        &self,
        team_id: String,
        match_id: Option<String>,
        form: MatchForm,
    ) -> Result<String, MobileError> {
        let team = self.read_team(&team_id)?;
        let existing = match_id
            .map(|id| {
                self.runtime
                    .block_on(self.match_reader.read_single(&team, &id))
            })
            .transpose()?;
        let input =
            form.validate(&team, existing.as_ref())
                .map_err(|e| MobileError::InvalidMatch {
                    field: e.into(),
                    reason: e.message(self.language.labels()),
                })?;
        let saved = match existing {
            Some(m) => self
                .runtime
                .block_on(self.match_writer.update(&m, input))
                .map(|()| m.id),
            None => self
                .runtime
                .block_on(self.match_writer.create(&team, input))
                .map(|m| m.id),
        };
        Ok(saved?)
    }

    /// An empty form for a new team, with the year given and the country
    /// of the settings.
    pub fn new_team_form(&self, year: u16) -> TeamForm {
        TeamForm {
            name: String::new(),
            classification: None,
            gender: None,
            country: self.default_country().map(|c| c.value),
            year: year.to_string(),
        }
    }

    /// The countries whose code is `query`, then those whose name has it
    /// in it, by name in the language of the app; all of them for an empty
    /// query.
    pub fn countries(&self, query: String) -> Vec<CountryChoice> {
        CountryCode::search(&query, self.language.get())
            .into_iter()
            .map(|(value, name)| CountryChoice {
                value,
                name: name.to_string(),
            })
            .collect()
    }

    /// The name of the country in the language of the app.
    pub fn country_name(&self, country: CountryCode) -> String {
        country.name(self.language.get())
    }

    pub fn classifications(&self) -> Vec<ClassificationChoice> {
        let labels = self.language.labels();
        TeamClassificationEnum::ALL
            .iter()
            .map(|c| ClassificationChoice {
                value: *c,
                name: c.friendly_name(labels).to_string(),
                description: c.friendly_description(labels).to_string(),
            })
            .collect()
    }

    pub fn genders(&self) -> Vec<GenderChoice> {
        let labels = self.language.labels();
        GenderEnum::ALL
            .iter()
            .map(|g| GenderChoice {
                value: *g,
                name: g.friendly_name(labels).to_string(),
            })
            .collect()
    }

    /// The roles to give a player, the same in every team.
    pub fn roles(&self) -> Vec<RoleChoice> {
        let labels = self.language.labels();
        RoleEnum::ALL
            .iter()
            .map(|r| RoleChoice {
                value: *r,
                name: r.friendly_name(labels).to_string(),
            })
            .collect()
    }

    /// Adds a player to the team (no `player_id`), a person of the registry
    /// when the form has one, or updates the one with `player_id`; returns
    /// its id.
    #[uniffi::method(default(namesake_confirmed = false))]
    pub fn save_player(
        &self,
        team_id: String,
        player_id: Option<String>,
        form: PlayerForm,
        namesake_confirmed: bool,
    ) -> Result<String, MobileError> {
        let mut team = self.read_team(&team_id)?;
        // a person of the registry chosen: the player they were, removed
        // from the team, comes back
        let existing = match (player_id, form.person) {
            (Some(id), _) => Some(find_player(&team, &id)?),
            (None, Some(person)) => team.find_player(person).cloned(),
            (None, None) => None,
        };
        let labels = self.language.labels();
        let namesake = player_form::namesake(&form, &team, existing.as_ref())
            .filter(|_| !namesake_confirmed)
            .map(|p| player_form::namesake_message(p, labels));
        let input = form
            .validate(&team, existing.as_ref(), self.clock.today())
            .map_err(|e| MobileError::InvalidPlayer {
                field: e.into(),
                reason: e.message(labels),
            })?;
        // the form valid first: then the namesake, to confirm
        if let Some(reason) = namesake {
            return Err(MobileError::Namesake { reason });
        }
        let player = self
            .runtime
            .block_on(self.team_writer.save_player(input, &mut team))?;
        Ok(player.id.to_string())
    }

    /// Removes a player, who stays for the matches already played.
    pub fn remove_player(&self, team_id: String, player_id: String) -> Result<(), MobileError> {
        let mut team = self.read_team(&team_id)?;
        let player = find_player(&team, &player_id)?;
        self.runtime
            .block_on(self.team_writer.remove_player(player.id, &mut team))?;
        Ok(())
    }

    /// The form of a new player of the team: the first number no player of
    /// it has, from 1 (none when they're all taken).
    pub fn new_player_form(&self, team_id: String) -> Result<PlayerForm, MobileError> {
        let team = self.read_team(&team_id)?;
        let taken: Vec<u8> = team.active_players().iter().map(|p| p.number).collect();
        let free = (1..=99u8).find(|n| !taken.contains(n));
        Ok(PlayerForm {
            number: free.map(|n| n.to_string()).unwrap_or_default(),
            ..PlayerForm::default()
        })
    }

    /// The person of the registry with the name written (case, accents and
    /// spaces aside) that can be added to the team: the one to ask about
    /// before a new person with the same name is made.
    pub fn registry_namesake(
        &self,
        team_id: String,
        name: String,
    ) -> Result<Option<ExistingPlayer>, MobileError> {
        let written = scout4all_core::dvw::import::normalized(&name);
        Ok(match written.trim().is_empty() {
            true => None,
            false => self
                .existing_players(team_id)?
                .into_iter()
                .find(|p| scout4all_core::dvw::import::normalized(&p.name) == written),
        })
    }

    /// The persons of the registry that can be added to the team, by name.
    pub fn existing_players(&self, team_id: String) -> Result<Vec<ExistingPlayer>, MobileError> {
        let team = parse_id(&team_id)?;
        Ok(self
            .persons
            .choices(&team)?
            .into_iter()
            .map(|c| {
                let teams: Vec<String> = c
                    .teams
                    .iter()
                    .map(|t| format!("{} {}", t.name, t.year))
                    .collect();
                let label = match teams.is_empty() {
                    true => c.person.name.clone(),
                    false => format!("{} ({})", c.person.name, teams.join(", ")),
                };
                let latest = c.teams.first();
                ExistingPlayer {
                    id: c.person.id,
                    name: c.person.name.clone(),
                    label,
                    form: PlayerForm {
                        name: c.person.name,
                        role: latest.and_then(|t| t.role),
                        number: latest.map(|t| t.number.to_string()).unwrap_or_default(),
                        birth_date: c
                            .person
                            .birth_date
                            .map(|d| d.format(player_form::BIRTH_DATE).to_string())
                            .unwrap_or_default(),
                        height_cm: c
                            .person
                            .height_cm
                            .map(|h| h.to_string())
                            .unwrap_or_default(),
                        notes: c.person.notes,
                        nationality: c.person.nationality,
                        dominant_hand: c.person.dominant_hand,
                        federation_id: c.person.federation_id,
                        person: Some(c.person.id),
                    },
                }
            })
            .collect())
    }

    /// The other teams the player plays in, the most recent first:
    /// "Cortona U19 (2026)". Their details are the same there (D5).
    pub fn other_teams(
        &self,
        team_id: String,
        player_id: String,
    ) -> Result<Vec<String>, MobileError> {
        let team = parse_id(&team_id)?;
        Ok(self
            .persons
            .teams_of(&parse_id(&player_id)?)?
            .into_iter()
            .filter(|t| t.team != team)
            .map(|t| format!("{} ({})", t.name, t.year))
            .collect())
    }

    /// Removes the player from the registry (D16), so from every team: in
    /// the matches played they stay, as a removed player with their number.
    pub fn delete_person(&self, player_id: String) -> Result<(), MobileError> {
        Ok(self.persons.remove(&parse_id(&player_id)?)?)
    }

    /// The new season of the team (D15): its form, the year after, and
    /// its active players, all chosen.
    pub fn new_season(&self, team_id: String) -> Result<NewSeason, MobileError> {
        let team = self.read_team(&team_id)?;
        Ok(NewSeason {
            form: season::team_form(&team),
            players: self.season_players(&team, &season::members(&team)),
        })
    }

    /// The players of the new season with `player` given `number` and
    /// `role`, by number again.
    pub fn correct_season_player(
        &self,
        team_id: String,
        players: Vec<SeasonPlayer>,
        player: Uuid,
        number: String,
        role: Option<RoleEnum>,
    ) -> Result<Vec<SeasonPlayer>, MobileError> {
        let team = self.read_team(&team_id)?;
        let members = season::correct(&season_members(&players), player, &number, role)
            .map_err(|e| self.season_error(e))?;
        Ok(self.season_players(&team, &members))
    }

    /// Creates the team of the new season with the players chosen: the same
    /// persons, in one more team; returns its id. The old team stays.
    pub fn save_new_season(
        &self,
        team_id: String,
        form: TeamForm,
        players: Vec<SeasonPlayer>,
    ) -> Result<String, MobileError> {
        let team = self.read_team(&team_id)?;
        let input = form.validate(None).map_err(|e| MobileError::InvalidTeam {
            field: e.into(),
            reason: e.message(self.language.labels()),
        })?;
        let input = season::new_team(&team, input, &season_members(&players))
            .map_err(|e| self.season_error(e))?;
        let saved = self.runtime.block_on(self.team_writer.save(input))?;
        // as a team created by hand: at the top of the list
        if let Err(e) = self.set_pinned(saved.id.to_string(), true) {
            log_error(&format!("could not pin the new team: {e:?}"));
        }
        Ok(saved.id.to_string())
    }

    /// Removes the team with its matches, for good; unpinned. The folder
    /// of a team that can't be read is its id all the same.
    pub fn delete_team(&self, team_id: String) -> Result<(), MobileError> {
        let id = parse_id(&team_id)?;
        self.runtime.block_on(self.team_writer.remove(&id))?;
        // the team is gone all the same when the settings can't be written
        if let Err(e) = self.set_pinned(team_id, false) {
            log_error(&format!("could not unpin the removed team: {e:?}"));
        }
        Ok(())
    }

    /// Removes the match with its sets, for good; `match_id` may be the
    /// folder of a match that can't be read.
    pub fn delete_match(&self, team_id: String, match_id: String) -> Result<(), MobileError> {
        let id = parse_id(&team_id)?;
        self.runtime
            .block_on(self.match_writer.remove(&id, &match_id))?;
        Ok(())
    }

    /// Creates a team (no `id`) or updates the one with `id`; returns its id.
    pub fn save_team(&self, id: Option<String>, form: TeamForm) -> Result<String, MobileError> {
        let existing = id.map(|id| self.read_team(&id)).transpose()?;
        let input = form
            .validate(existing.as_ref())
            .map_err(|e| MobileError::InvalidTeam {
                field: e.into(),
                reason: e.message(self.language.labels()),
            })?;
        let team = self.runtime.block_on(self.team_writer.save(input))?;
        // created by hand: at the top of the list; the team is there all
        // the same when the settings can't be written
        if existing.is_none()
            && let Err(e) = self.set_pinned(team.id.to_string(), true)
        {
            log_error(&format!("could not pin the new team: {e:?}"));
        }
        Ok(team.id.to_string())
    }

    /// The logo of the team as PNG, if it has one.
    pub fn team_logo(&self, team_id: String) -> Result<Option<Vec<u8>>, MobileError> {
        Ok(scout4all_core::assets::team_logo(&self.db, &team_id)?.map(|image| image.data))
    }

    /// The image the file holds becomes the logo of the team (shrunk, as
    /// PNG), `None` removes it; the team is left as it is if the image
    /// can't be read.
    pub fn set_team_logo(
        &self,
        team_id: String,
        bytes: Option<Vec<u8>>,
    ) -> Result<(), MobileError> {
        Ok(scout4all_core::assets::store_team_logo(
            &self.db,
            &team_id,
            bytes.as_deref(),
        )?)
    }

    /// The photo of the person (the player's id is theirs) as PNG, if they
    /// have one. Previously saved JPEGs remain readable.
    pub fn person_photo(&self, person_id: String) -> Result<Option<Vec<u8>>, MobileError> {
        Ok(scout4all_core::assets::person_photo(&self.db, &person_id)?.map(|image| image.data))
    }

    /// The person in the photo, cropped to a square with a transparent
    /// background (PNG): for the preview, then `set_person_photo`. No face or
    /// many faces are errors of their own.
    pub fn crop_face(&self, bytes: Vec<u8>) -> Result<Vec<u8>, MobileError> {
        use scout4all_core::faces::{PhotoError, crop_face};
        crop_face(&bytes).map_err(|e| match e {
            PhotoError::NoFace => MobileError::NoFace {
                reason: e.to_string(),
            },
            PhotoError::ManyFaces(n) => MobileError::ManyFaces {
                count: u32::try_from(n).unwrap_or(u32::MAX),
                reason: e.to_string(),
            },
            PhotoError::Image(e) => e.into(),
        })
    }

    /// The cropped person (`core::faces::crop_face`) becomes the photo of the
    /// person (as PNG), `None` removes it.
    pub fn set_person_photo(
        &self,
        person_id: String,
        bytes: Option<Vec<u8>>,
    ) -> Result<(), MobileError> {
        Ok(scout4all_core::assets::store_person_photo(
            &self.db,
            &person_id,
            bytes.as_deref(),
        )?)
    }

    /// The archive of the team, its players without its matches.
    pub fn export_team(&self, team_id: String) -> Result<ExportedArchive, MobileError> {
        let id = parse_id(&team_id)?;
        // a team removed or not there: not exported
        self.read_team(&team_id)?;
        let bytes = archives::export(&self.db, archives::Subject::Team(id))?;
        Ok(ExportedArchive {
            file_name: format!("{id}.zip"),
            bytes,
        })
    }

    /// The archive of a match of the team, with its teams.
    pub fn export_match(
        &self,
        team_id: String,
        match_id: String,
    ) -> Result<ExportedArchive, MobileError> {
        let m = self.read_match(&team_id, &match_id)?;
        let id = Uuid::parse_str(&m.id).map_err(failed)?;
        let bytes = archives::export(&self.db, archives::Subject::Match(id))?;
        Ok(ExportedArchive {
            file_name: format!("{id}.zip"),
            bytes,
        })
    }

    /// The report of the match as a PDF, in the language of the app: the
    /// file to save or print, as the TUI opens it.
    pub fn match_pdf(&self, team_id: String, match_id: String) -> Result<Vec<u8>, MobileError> {
        use scout4all_core::reporting::pdf::{render_both_pdf, render_match_pdf};
        let m = self.read_match(&team_id, &match_id)?;
        let labels = self.language.labels();
        // a match scouted on both sides has the opponent's pages too
        match self
            .match_reader
            .read_of_b(&self.read_team(&team_id)?, &match_id)
        {
            Ok(Some(mut b)) => {
                name_the_removed(&mut b.team, labels);
                Ok(render_both_pdf(&m, &b, labels)?)
            }
            Ok(None) => Ok(render_match_pdf(&m, labels)?),
            Err(e) => Err(unreadable(&e, labels.unreadable_match)),
        }
    }

    /// What the archive holds, unless it can't be imported: e.g. a match
    /// of a version before the database of none of the teams, or not of
    /// `team_id` when it's imported into that team, or there already.
    /// `file_name` is the name of the file it comes from, when known: the
    /// archives exported up to v0.9.0 have the id of the team only there.
    /// An archive of the database brings its teams with it: the team of a
    /// match is its own.
    pub fn preview_archive(
        &self,
        bytes: Vec<u8>,
        file_name: Option<String>,
        team_id: Option<String>,
    ) -> Result<ArchivePreview, MobileError> {
        let labels = self.language.labels();
        let team_preview = |team: TeamEntry| ArchivePreview::Team {
            players: count(&team),
            name: team.name,
            year: team.year,
        };
        let match_preview =
            |m: &MatchEntry, won: u8, lost: u8, teams: Vec<TeamChoice>| ArchivePreview::Match {
                date: m.date.to_rfc3339(),
                opponent: m.opponent.clone(),
                home: m.home,
                won,
                lost,
                teams,
            };
        let choice = |t: &TeamEntry| TeamChoice {
            id: t.id.to_string(),
            name: t.name.clone(),
        };
        Ok(match legacy_archive::open(&self.db, &bytes, labels)? {
            Opened::Database(_) => match archives::preview(&bytes, &self.language)? {
                Preview::Team(team) => team_preview(team),
                Preview::Match(m) => match_preview(
                    &m.entry,
                    m.status.us_wins,
                    m.status.them_wins,
                    vec![choice(&m.team)],
                ),
                Preview::Training(t) => ArchivePreview::Training {
                    name: t.team.name,
                    year: t.team.year,
                    sessions: count_of(t.sessions),
                    cycles: count_of(t.cycles),
                },
            },
            // up to v0.9.0 the id only in the name of the file
            Opened::LegacyTeam { team, id } => {
                match id.or_else(|| file_name.as_deref().and_then(file_team_id)) {
                    Some(_) => team_preview(team),
                    None => return Err(failed(labels.team_archive_without_id)),
                }
            }
            Opened::LegacyMatch { archived, teams } => {
                let mut choices: Vec<TeamChoice> = teams
                    .iter()
                    .filter(|t| team_id.as_ref().is_none_or(|id| t.id.to_string() == *id))
                    .map(choice)
                    .collect();
                if choices.is_empty() {
                    return Err(failed(labels.match_of_another_team));
                }
                choices.sort_by_key(|t| t.name.to_lowercase());
                let status = archived.get_status()?;
                match_preview(&archived, status.us_wins, status.them_wins, choices)
            }
        })
    }

    /// Adds the team of the archive, or merges it into the one here (the
    /// data here win): its id and what to tell of it.
    pub fn import_team(
        &self,
        bytes: Vec<u8>,
        file_name: Option<String>,
    ) -> Result<ImportedArchive, MobileError> {
        let labels = self.language.labels();
        let imported = match legacy_archive::open(&self.db, &bytes, labels)? {
            Opened::Database(manifest) if manifest.kind == Kind::Team => {
                archives::import(&self.db, &bytes, &self.language)?
            }
            Opened::LegacyTeam { .. } => {
                let file_id = file_name.as_deref().and_then(file_team_id);
                import_legacy_team(&self.db, &bytes, file_id, &self.language)?
            }
            Opened::Database(_) | Opened::LegacyMatch { .. } => {
                return Err(failed(labels.invalid_archive));
            }
        };
        self.sender.request();
        Ok(ImportedArchive::of(&imported, labels))
    }

    /// What the DataVolley file holds: the teams as the import names them,
    /// without the year of the season.
    pub fn preview_dvw(&self, bytes: Vec<u8>) -> Result<DvwPreview, MobileError> {
        let file = self.parse_dvw(&bytes)?;
        let day = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
        Ok(DvwPreview {
            home: team_name(&file, Side::Home),
            visiting: team_name(&file, Side::Visiting),
            days: match file.match_info.date {
                DvwDate::Known(d) => vec![day(d)],
                DvwDate::Ambiguous(a, b) => vec![day(a), day(b)],
                DvwDate::Missing => vec![],
            },
            season: season_year(&file),
            sets: file
                .sets
                .iter()
                .map(|s| DvwSetScore {
                    home: s.final_score.0,
                    visiting: s.final_score.1,
                })
                .collect(),
        })
    }

    /// The substitutions per set proposed for a match of the day
    /// (`YYYY-MM-DD`, none when not known): the rules of the time.
    pub fn dvw_max_substitutions(&self, day: Option<String>) -> u8 {
        default_max_substitutions(day.as_deref().and_then(|d| parse_day(d).ok()))
    }

    /// Our teams like the one of the side, the most alike first; `first`,
    /// the team the import was asked from, before them all even if not
    /// alike.
    pub fn dvw_similar_teams(
        &self,
        bytes: Vec<u8>,
        side: DvwSide,
        first: Option<String>,
    ) -> Result<Vec<DvwSimilarTeam>, MobileError> {
        let file = self.parse_dvw(&bytes)?;
        let teams = self.readable_teams()?;
        let side = side.into();
        let first = first.and_then(|id| teams.iter().find(|t| t.id.to_string() == id));
        let similar = similar_teams(&file, side, &teams)
            .into_iter()
            .filter(|s| first.is_none_or(|f| f.id != s.team.id));
        Ok(first
            .map(|team| similarity(&file, side, team))
            .into_iter()
            .chain(similar)
            .map(|s| DvwSimilarTeam {
                id: s.team.id.to_string(),
                name: s.team.name.clone(),
                year: s.team.year,
                same_name: s.same_name,
                same_players: count_of(s.same_players),
                players: count_of(s.players),
            })
            .collect())
    }

    /// Our teams like team B of the side, the most alike first, team A
    /// `team_id` left out; `first`, the team of the side imported before,
    /// before them all even if not alike.
    pub fn dvw_similar_opponents(
        &self,
        bytes: Vec<u8>,
        side: DvwSide,
        team_id: Option<String>,
        first: Option<String>,
    ) -> Result<Vec<DvwSimilarTeam>, MobileError> {
        let other = match side {
            DvwSide::Home => DvwSide::Visiting,
            DvwSide::Visiting => DvwSide::Home,
        };
        Ok(self
            .dvw_similar_teams(bytes, other, first)?
            .into_iter()
            .filter(|t| team_id.as_ref() != Some(&t.id))
            .collect())
    }

    /// The import as chosen, checked: what it saves, or why it can't.
    pub fn plan_dvw(&self, bytes: Vec<u8>, choice: DvwChoice) -> Result<DvwReady, MobileError> {
        let file = self.parse_dvw(&bytes)?;
        let (team, opponent, plan) = self.dvw_plans(&file, &choice)?;
        let labels = self.language.labels();
        let competition = [
            plan.input.competition.as_ref().map(|c| match c {
                CompetitionInput::New { name, .. } => name.clone(),
                CompetitionInput::Existing(id) => id.to_string(),
            }),
            plan.input.stage.map(|s| s.name(labels).to_string()),
            plan.input.round.clone(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        Ok(DvwReady {
            team: team.team.name.clone(),
            sets: count_of(plan.entry.sets.len()),
            new_players: count_of(team.new_players.len()),
            opponent: opponent.name().to_string(),
            opponent_new_players: opponent.team().map(|b| count_of(b.new_players.len())),
            match_numbers: count_of(team.match_numbers.len()),
            competition: Some(competition).filter(|c| !c.is_empty()),
            one_side: match &plan.sides {
                ImportedSides::A(Some(issue)) => Some(
                    labels
                        .dvw_one_side
                        .replace("{team}", &team.team.name)
                        .replace("{opponent}", opponent.name())
                        .replace("{reason}", &issue.describe(labels)),
                ),
                ImportedSides::A(None) | ImportedSides::Both(_) => None,
            },
        })
    }

    /// Saves the import as chosen; a match imported already refused. The
    /// match queued asks the sender (`finished_matches`).
    pub fn import_dvw(
        &self,
        bytes: Vec<u8>,
        choice: DvwChoice,
    ) -> Result<DvwImported, MobileError> {
        let file = self.parse_dvw(&bytes)?;
        let (team, opponent, plan) = self.dvw_plans(&file, &choice)?;
        let m = self.runtime.block_on(save_import(
            &team,
            &opponent,
            &plan,
            &self.team_writer,
            &self.match_reader,
            &self.match_writer,
            &*self.set_writer,
            &*self.finished_matches(),
        ))?;
        Ok(DvwImported {
            team_id: team.team.id.to_string(),
            match_id: m.id,
        })
    }

    /// Adds the match of the archive: one of a version before the
    /// database into the team `team_id` (one of those of its preview), one
    /// of the database with its own teams. Its id and what to tell of it.
    pub fn import_match(
        &self,
        bytes: Vec<u8>,
        team_id: String,
    ) -> Result<ImportedArchive, MobileError> {
        let labels = self.language.labels();
        let imported = match legacy_archive::open(&self.db, &bytes, labels)? {
            Opened::Database(manifest) if manifest.kind == Kind::Match => {
                archives::import(&self.db, &bytes, &self.language)?
            }
            Opened::LegacyMatch { .. } => {
                import_legacy_match(&self.db, &bytes, parse_id(&team_id)?, &self.language)?
            }
            Opened::Database(_) | Opened::LegacyTeam { .. } => {
                return Err(failed(labels.invalid_archive));
            }
        };
        self.sender.request();
        Ok(ImportedArchive::of(&imported, labels))
    }

    /// The language of the labels from now on, a code like `"it"` (English
    /// when it isn't supported): the app follows the language chosen for it
    /// in the system.
    pub fn set_language(&self, language: String) {
        self.language
            .set(LanguageEnum::from_str(&language).unwrap_or(LanguageEnum::En));
    }

    /// Whether the user has seen the current terms of use: not when
    /// `config.json` is missing or can't be read, so they're shown.
    pub fn terms_seen(&self) -> bool {
        match self.read_config() {
            Ok(config) => config.consent.terms_seen(),
            Err(e) => {
                log_error(&format!("could not read the settings: {e}"));
                false
            }
        }
    }

    /// The terms of use: the statistics sent unless turned off, below them.
    pub fn terms(&self) -> Terms {
        let labels = self.language.labels();
        Terms {
            title: labels.terms_of_use.to_string(),
            paragraphs: labels.terms(labels.terms_upload).map(String::from).to_vec(),
        }
    }

    /// The licenses of the parts made by others the app carries, in the
    /// settings beside the terms.
    pub fn licenses(&self) -> Licenses {
        let labels = self.language.labels();
        Licenses {
            title: labels.third_party_licenses.to_string(),
            intro: labels.third_party_intro.to_string(),
            parts: scout4all_core::licenses::THIRD_PARTIES
                .iter()
                .map(|p| ThirdPartyLicense {
                    name: p.name.to_string(),
                    purpose: p.purpose.label(labels).to_string(),
                    source: p.source.to_string(),
                    license: p.license.to_string(),
                    text: p.text.trim().to_string(),
                })
                .collect(),
        }
    }

    /// Saves the current terms of use as seen, keeping the other settings
    /// (those of the TUI too). A new `config.json` has the language too,
    /// which the TUI needs.
    pub fn accept_terms(&self) -> Result<(), MobileError> {
        self.update_consent(Consent::with_terms_seen)
    }

    /// On a new installation, the example team, and the tutorial to show
    /// once the terms are accepted: whether it was added. Nothing on an
    /// update. To call at startup, before the teams are listed.
    pub fn add_example_team_if_new(&self) -> Result<bool, MobileError> {
        let added = self.runtime.block_on(example::add_example_team_if_new(
            &*self.store,
            &self.team_reader,
            &self.team_writer,
            self.language.labels(),
        ))?;
        match added {
            Some(team) => self
                .update_tutorial(|_| Tutorial {
                    tutorial_pending: true,
                    example_team: Some(team.id.to_string()),
                })
                .map(|()| true),
            None => Ok(false),
        }
    }

    /// Whether the tutorial is still to show by itself.
    pub fn tutorial_pending(&self) -> bool {
        self.read_config()
            .is_ok_and(|config| config.tutorial.tutorial_pending)
    }

    /// The tutorial seen, or skipped: not shown by itself again.
    pub fn tutorial_seen(&self) -> Result<(), MobileError> {
        self.update_tutorial(|tutorial| Tutorial {
            tutorial_pending: false,
            ..tutorial
        })
    }

    /// The id of the example team the tutorial scouts with: a new one when
    /// it was removed or lost some of its players.
    pub fn tutorial_team(&self) -> Result<String, MobileError> {
        let kept = self
            .read_config()
            .ok()
            .and_then(|config| config.tutorial.example_team)
            .and_then(|id| self.read_team(&id).ok())
            .filter(example::is_example_team);
        match kept {
            Some(team) => Ok(team.id.to_string()),
            None => {
                let team = self.runtime.block_on(example::add_example_team(
                    &self.team_writer,
                    self.language.labels(),
                ))?;
                let id = team.id.to_string();
                self.update_tutorial(|tutorial| Tutorial {
                    example_team: Some(id.clone()),
                    ..tutorial
                })?;
                Ok(id)
            }
        }
    }

    /// The country a new team starts with, from the settings.
    pub fn default_country(&self) -> Option<CountryChoice> {
        self.read_config()
            .ok()
            .and_then(|config| config.team_defaults.country)
            .map(|value| CountryChoice {
                value,
                name: value.name(self.language.get()),
            })
    }

    /// Saves the country a new team starts with, keeping the other
    /// settings as `accept_terms`.
    pub fn set_default_country(&self, country: CountryCode) -> Result<(), MobileError> {
        self.update_team_defaults(|_| TeamDefaults {
            country: Some(country),
        })
    }

    /// No country for a new team: its field starts empty (the desktop; on
    /// Android the next start guesses one again).
    pub fn clear_default_country(&self) -> Result<(), MobileError> {
        self.update_team_defaults(|_| TeamDefaults { country: None })
    }

    /// When the settings have no country for a new team yet, the first of
    /// the `hints` of the system telling one (the SIM, the network, the
    /// locale, in this order), else the one of the language. To call at
    /// startup after `add_example_team_if_new`, which tells a new
    /// installation by its missing `config.json`.
    pub fn guess_default_country(&self, hints: Vec<String>) -> Result<(), MobileError> {
        let guess = CountryCode::guess(hints.iter().map(String::as_str), self.language.get());
        match (self.default_country(), guess) {
            (None, Some(country)) => self.set_default_country(country),
            _ => Ok(()),
        }
    }

    /// The seconds of the video chosen in the settings, the defaults
    /// without.
    pub fn video_seconds(&self) -> VideoSeconds {
        self.read_config()
            .map(|config| config.video)
            .unwrap_or_default()
    }

    /// Saves the seconds of the video, in their bounds, as `accept_terms`
    /// does: the ones saved.
    pub fn set_video_seconds(&self, seconds: VideoSeconds) -> Result<VideoSeconds, MobileError> {
        let bounded = seconds.bounded();
        self.update_config(|config| AppConfig {
            video: bounded,
            ..config
        })?;
        Ok(bounded)
    }

    /// Whether the user allows sending the statistics of the matches.
    pub fn analytics_enabled(&self) -> bool {
        self.read_config()
            .is_ok_and(|config| config.consent.analytics_enabled)
    }

    /// The choice of the user about the statistics, saved as `accept_terms`
    /// does. Turned off, the matches already queued stay, unsent.
    pub fn set_analytics_enabled(&self, enabled: bool) -> Result<(), MobileError> {
        self.update_consent(|consent| Consent {
            analytics_enabled: enabled,
            ..consent
        })
    }

    /// Uploads the queued matches, one after the other, until one fails:
    /// whether some are left to try again later (no network, the API down).
    /// Nothing to do when the user turned the statistics off or the app was
    /// built without the URL of the API. It blocks for a while: the system
    /// calls it in the background, when there's a network.
    pub fn upload_pending(&self) -> Uploads {
        let consent = self.read_config().map(|config| config.consent);
        match (self.uploads_enabled.load(Ordering::Relaxed), upload_url()) {
            (true, Some(url)) => self.upload_pending_to(&url),
            (true, None) => {
                log_info("statistics not sent: the app was built without ANALYTICS_UPLOAD_URL");
                Uploads::Done
            }
            (false, _) => {
                let why = match consent {
                    Ok(c) if !c.terms_seen() => "the terms of use are not accepted",
                    Ok(_) => "the user turned them off",
                    Err(_) => "the settings can't be read",
                };
                log_info(&format!("statistics not sent: {why}"));
                Uploads::Done
            }
        }
    }
}

impl Scout4All {
    /// Sends the queue now and whenever a match is queued (finished,
    /// imported), again with a backoff while some are left, until the app
    /// closes: the desktop app, which has no WorkManager. Once is enough.
    pub fn send_in_background(self: &Arc<Self>) {
        let core = Arc::downgrade(self);
        self.sender.start(Box::new(move || {
            core.upgrade()
                .map_or(Uploads::Done, |core| core.upload_pending())
        }));
    }

    /// Sends the queue in the background now, as a match queued does: after
    /// the terms accepted or the statistics turned on. Nothing before
    /// `send_in_background`.
    pub fn send_uploads(&self) {
        self.sender.request();
    }
}

/// The queue of the core, which then asks the sender to send it.
struct SentFinishedMatches {
    queued: QueuedFinishedMatches<SqliteQueue, SqliteQueue>,
    sender: Arc<UploadSender>,
}

#[async_trait::async_trait]
impl FinishedMatches for SentFinishedMatches {
    async fn finished(&self, team_id: Uuid, match_id: String) {
        self.queued.finished(team_id, match_id).await;
        self.sender.request();
    }
}

/// What's left after `upload_pending`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Uploads {
    /// nothing to send now
    Done,
    /// an upload failed: to try again later
    Retry,
}

/// Writes the log of the core in `data_dir`. The log is one for the
/// process: the app starts it once, the tests leave it off (each of them has
/// its own data, the log would go to the last one's).
#[uniffi::export]
pub fn start_log(data_dir: String) {
    init_logger(PathBuf::from(data_dir).join("scout4all.log"));
}

/// A name of a player in little room: each long word cut to its first
/// letters ("Ale. Mag."), no guess about which is the surname.
#[uniffi::export]
pub fn abbreviated_name(name: String) -> String {
    scout4all_core::names::abbreviated(&name)
}

/// The names offered as `written` is written: the ones that begin with it
/// first, then the ones with a word that does, then the ones with it inside.
#[uniffi::export]
pub fn offered_names(names: Vec<String>, written: String) -> Vec<String> {
    scout4all_core::names::offered(&names, &written)
}

#[uniffi::remote(Enum)]
pub enum LogoTone {
    Light,
    Dark,
    Mixed,
}

#[uniffi::remote(Enum)]
pub enum LogoBackdrop {
    None,
    Dark,
    Light,
}

/// The tone of the logo the bytes hold, to decide its backdrop once per
/// logo.
#[uniffi::export]
pub fn logo_tone(bytes: Vec<u8>) -> LogoTone {
    scout4all_core::assets::logo_tone(&bytes)
}

/// What goes behind a logo of `tone` on the theme so it doesn't vanish.
#[uniffi::export]
pub fn logo_backdrop(tone: LogoTone, dark_theme: bool) -> LogoBackdrop {
    tone.backdrop(dark_theme)
}

/// The seconds a setting can take, both ends in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct SecondsRange {
    pub min: u32,
    pub max: u32,
}

impl From<std::ops::RangeInclusive<u32>> for SecondsRange {
    fn from(range: std::ops::RangeInclusive<u32>) -> Self {
        SecondsRange {
            min: *range.start(),
            max: *range.end(),
        }
    }
}

/// How far the video can jump back and forward (`VideoSeconds::step_s`).
#[uniffi::export]
pub fn video_step_range() -> SecondsRange {
    scout4all_core::settings::STEP_SECONDS.into()
}

/// How long before an event the video can go to it
/// (`VideoSeconds::lead_s`).
#[uniffi::export]
pub fn video_lead_range() -> SecondsRange {
    scout4all_core::settings::LEAD_SECONDS.into()
}

/// The version of the app (the same as the core, the TUI, the CLI).
#[uniffi::export]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// The terms of use, to show a paragraph each.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Terms {
    pub title: String,
    pub paragraphs: Vec<String>,
}

/// The licenses of the parts made by others the app carries: the title, a
/// line before them, then each part.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Licenses {
    pub title: String,
    pub intro: String,
    pub parts: Vec<ThirdPartyLicense>,
}

/// A part made by others: its name, what it's for, where it comes from, the
/// name of its license and its full text (in English, as written).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ThirdPartyLicense {
    pub name: String,
    pub purpose: String,
    pub source: String,
    pub license: String,
    pub text: String,
}

/// The key of the language in `config.json`.
const LANGUAGE: &str = "language";

/// `config.json`: the settings shared by the apps, and those of the others
/// (the keys and the language of the TUI), kept as they are.
#[derive(Debug, Default, Serialize, Deserialize)]
struct AppConfig {
    #[serde(flatten)]
    consent: Consent,
    #[serde(flatten)]
    tutorial: Tutorial,
    #[serde(flatten)]
    team_defaults: TeamDefaults,
    #[serde(flatten)]
    pinned: PinnedTeams,
    #[serde(flatten)]
    video: VideoSeconds,
    #[serde(flatten)]
    activity_search: ActivitySearch,
    #[serde(flatten)]
    other: Map<String, Value>,
}

/// The tutorial of the scouting, in `config.json`: missing, it was seen, as
/// on an update.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Tutorial {
    /// still to show by itself: only a new installation has it
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    tutorial_pending: bool,
    /// the example team it scouts with
    #[serde(default, skip_serializing_if = "Option::is_none")]
    example_team: Option<String>,
}

/// The database of the app, its errors in `labels` (not in the global
/// language of the errors, which another app may change meanwhile).
fn open_database(file: &Path, labels: &Labels) -> Result<Database, MobileError> {
    db::open(file)
        .map(Database::new)
        .map_err(|e| MobileError::Failed {
            reason: e.message(labels),
        })
}

fn failed(e: impl std::fmt::Display) -> MobileError {
    MobileError::Failed {
        reason: e.to_string(),
    }
}

impl Scout4All {
    /// Where the scouting sends a finished match: the queue, if the user
    /// allows the statistics.
    fn finished_matches(&self) -> Arc<dyn FinishedMatches> {
        Arc::new(SentFinishedMatches {
            queued: QueuedFinishedMatches {
                queue: self.queue.clone(),
                enabled: self.uploads_enabled.clone(),
            },
            sender: self.sender.clone(),
        })
    }

    fn upload_pending_to(&self, url: &str) -> Uploads {
        loop {
            let source = SqliteUploads::new(self.db.clone(), self.language.clone());
            let step = self
                .runtime
                .block_on(upload_next(&source, &self.queue, url));
            // each sent or dropped match leaves the queue: it ends
            match step {
                Ok(UploadStep::Uploaded | UploadStep::Dropped) => continue,
                Ok(UploadStep::Empty) => return Uploads::Done,
                Ok(UploadStep::Failed) => return Uploads::Retry,
                Err(e) => {
                    log_error(&format!("could not read the upload queue: {e}"));
                    return Uploads::Retry;
                }
            }
        }
    }

    /// Changes the consent in `config.json`, keeping the other settings
    /// (those of the TUI too); a new one has the language too, which the
    /// TUI needs.
    fn update_consent(&self, change: impl FnOnce(Consent) -> Consent) -> Result<(), MobileError> {
        self.update_config(|config| AppConfig {
            consent: change(config.consent),
            ..config
        })
    }

    /// Changes the country of a new team in `config.json`, as
    /// `update_consent`.
    fn update_team_defaults(
        &self,
        change: impl FnOnce(TeamDefaults) -> TeamDefaults,
    ) -> Result<(), MobileError> {
        self.update_config(|config| AppConfig {
            team_defaults: change(config.team_defaults),
            ..config
        })
    }

    /// Changes the tutorial in `config.json`, as `update_consent`.
    fn update_tutorial(
        &self,
        change: impl FnOnce(Tutorial) -> Tutorial,
    ) -> Result<(), MobileError> {
        self.update_config(|config| AppConfig {
            tutorial: change(config.tutorial),
            ..config
        })
    }

    fn update_config(
        &self,
        change: impl FnOnce(AppConfig) -> AppConfig,
    ) -> Result<(), MobileError> {
        let config = change(self.read_config().or_else(|e| self.set_aside_config(&e))?);
        let language = match config.other.contains_key(LANGUAGE) {
            true => None,
            false => Some(to_value(self.language.get()).map_err(failed)?),
        };
        let config = AppConfig {
            other: config
                .other
                .into_iter()
                .chain(language.map(|l| (LANGUAGE.to_string(), l)))
                .collect(),
            ..config
        };
        let bytes = to_vec_pretty(&config).map_err(failed)?;
        self.runtime.block_on(self.store.write(CONFIG, &bytes))?;
        self.uploads_enabled
            .store(config.consent.uploads_enabled(), Ordering::Relaxed);
        Ok(())
    }

    /// A `config.json` that can't be read goes beside it, as it is, and the
    /// defaults take its place: kept, the terms would come back at every
    /// start, and the choice about the statistics could never be saved.
    fn set_aside_config(&self, error: &MobileError) -> Result<AppConfig, MobileError> {
        log_error(&format!("could not read the settings, set aside: {error}"));
        if let Some(bytes) = self.runtime.block_on(self.store.read(CONFIG))? {
            self.runtime
                .block_on(self.store.write(UNREADABLE_CONFIG, &bytes))?;
        }
        Ok(AppConfig::default())
    }

    /// `config.json`, the defaults when missing.
    fn read_config(&self) -> Result<AppConfig, MobileError> {
        match self.runtime.block_on(self.store.read(CONFIG))? {
            Some(bytes) => from_slice(&bytes).map_err(failed),
            None => Ok(AppConfig::default()),
        }
    }

    /// The teams that can be read.
    fn parse_dvw(&self, bytes: &[u8]) -> Result<DvwFile, MobileError> {
        dvw::parse(bytes).map_err(|e| MobileError::Failed {
            reason: e.describe(self.language.labels()),
        })
    }

    /// The team and the match of the import as chosen, or why not, in the
    /// language of the app.
    fn dvw_plans(
        &self,
        file: &DvwFile,
        choice: &DvwChoice,
    ) -> Result<(TeamPlan, OpponentPlan, MatchPlan), MobileError> {
        let labels = self.language.labels();
        let failed = |reason| MobileError::Failed { reason };
        let day = parse_day(&choice.day)?;
        let existing = choice
            .team_id
            .as_deref()
            .map(|id| self.read_team(id))
            .transpose()?;
        let team_choice = match &existing {
            Some(team) => DvwTeamChoice::Existing(team),
            None => DvwTeamChoice::New {
                classification: choice.classification,
                gender: choice.gender,
                country: choice.country,
            },
        };
        let side = choice.side.into();
        let team = team_plan(file, side, team_choice).map_err(|e| match e {
            // shown next to its field, as by the form of a team
            TeamIssue::Incomplete(missing) => MobileError::InvalidTeam {
                field: missing.into(),
                reason: missing.message(labels),
            },
            other => failed(other.describe(labels)),
        })?;
        let b = match &choice.opponent {
            DvwOpponent::Team { id } => Some(self.read_team(id)?),
            DvwOpponent::Name | DvwOpponent::NewTeam => None,
        };
        let opponent_choice = match (&choice.opponent, &b) {
            (DvwOpponent::Team { .. }, Some(b)) => DvwOpponentChoice::Existing(b),
            (DvwOpponent::NewTeam, _) => DvwOpponentChoice::New,
            (DvwOpponent::Name | DvwOpponent::Team { .. }, _) => DvwOpponentChoice::Name,
        };
        let opponent = opponent_plan(file, side, &team.team, opponent_choice)
            .map_err(|e| failed(e.describe(labels)))?;
        let date = match_date(file, day, self.clock.offset());
        let plan = match_plan(
            file,
            side,
            &team,
            &opponent,
            date,
            choice.max_substitutions,
            labels,
        )
        .map_err(|e| failed(e.describe(labels)))?;
        // refused, not overwritten: to import it again the user deletes it
        match self
            .runtime
            .block_on(imported_already(&plan, &opponent, &self.match_reader))?
        {
            Some(saved) => Err(failed(imported_already_message(&saved, labels))),
            None => Ok((team, opponent, plan)),
        }
    }

    fn readable_teams(&self) -> Result<Vec<TeamEntry>, MobileError> {
        Ok(self
            .runtime
            .block_on(self.team_reader.read_all())?
            .into_iter()
            .filter_map(Result::ok)
            .collect())
    }

    /// The competition not removed.
    fn competition(&self, id: Uuid) -> Result<scout4all_core::domain::Competition, MobileError> {
        self.competitions
            .read(id)?
            .filter(|c| !c.deleted)
            .ok_or_else(|| MobileError::Failed {
                reason: format!("competition {id} not found"),
            })
    }

    /// The match form changed by `change`, with the players of its roster.
    fn roster_after(
        &self,
        team_id: String,
        match_id: Option<String>,
        form: MatchForm,
        change: impl FnOnce(&mut MatchForm, &TeamEntry, Option<&MatchEntry>) -> Result<(), MobileError>,
    ) -> Result<MatchRoster, MobileError> {
        // the team as it is, also with the players not on the roster
        let team = self.read_team(&team_id)?;
        let existing = match_id
            .map(|id| self.read_match(&team_id, &id))
            .transpose()?;
        self.roster_with(team, existing, form, change)
    }

    /// `roster_after` for the team and the match as given: the match of
    /// team A, or the one seen by B.
    fn roster_with(
        &self,
        team: TeamEntry,
        existing: Option<MatchEntry>,
        form: MatchForm,
        change: impl FnOnce(&mut MatchForm, &TeamEntry, Option<&MatchEntry>) -> Result<(), MobileError>,
    ) -> Result<MatchRoster, MobileError> {
        let mut form = form;
        change(&mut form, &team, existing.as_ref())?;
        let labels = self.language.labels();
        let players = form
            .roster_choices(&team, existing.as_ref())
            .into_iter()
            .map(|c| RosterPlayer {
                id: c.player.id,
                name: c.player.name,
                number: c.player.number,
                role: c.player.role.map(|r| r.friendly_name(labels).to_string()),
                chosen: c.chosen,
                played: c.played,
                role_value: c.player.role,
            })
            .collect();
        Ok(MatchRoster { form, players })
    }

    /// The roster of B for a match of team A `team_id` scouted on both
    /// sides: B as a team, the match as B sees it.
    fn roster_of_b_after(
        &self,
        team_id: String,
        match_id: String,
        form: MatchForm,
        change: impl FnOnce(&mut MatchForm, &TeamEntry, Option<&MatchEntry>) -> Result<(), MobileError>,
    ) -> Result<MatchRoster, MobileError> {
        let m = self.read_match_of(&team_id, &match_id, MatchSide::B)?;
        let previous = self
            .match_reader
            .previous_roster_of_b(&self.read_team(&team_id)?, &match_id)
            .map_err(|e| unreadable(&e, self.language.labels().unreadable_match))?
            .unwrap_or_default();
        self.roster_with(m.team.clone(), Some(m), form, |form, team, existing| {
            form.start_from(&previous, team, existing);
            change(form, team, existing)
        })
    }

    /// The members of the new season as the form shows them.
    fn season_players(&self, team: &TeamEntry, members: &[SeasonMember]) -> Vec<SeasonPlayer> {
        let labels = self.language.labels();
        members
            .iter()
            .filter_map(|m| {
                team.find_player(m.player).map(|p| SeasonPlayer {
                    id: m.player,
                    name: p.name.clone(),
                    number: m.number,
                    role: m.role,
                    role_name: m.role.map(|r| r.friendly_name(labels).to_string()),
                    chosen: m.chosen,
                })
            })
            .collect()
    }

    /// An error of the new season, on the number of a player.
    fn season_error(&self, e: SeasonError) -> MobileError {
        MobileError::InvalidPlayer {
            field: PlayerField::Number,
            reason: e.message(self.language.labels()),
        }
    }

    /// The team, a player removed from the registry named as such.
    fn read_team(&self, id: &str) -> Result<TeamEntry, MobileError> {
        let id = parse_id(id)?;
        let labels = self.language.labels();
        let mut team = self
            .runtime
            .block_on(self.team_reader.read_single(&id))
            .map_err(|e| unreadable(&e, labels.unreadable_team))?;
        name_the_removed(&mut team, labels);
        Ok(team)
    }

    /// The match, a player removed from the registry named as such.
    fn read_match(&self, team_id: &str, match_id: &str) -> Result<MatchEntry, MobileError> {
        let team = self.read_team(team_id)?;
        let labels = self.language.labels();
        let mut m = self
            .runtime
            .block_on(self.match_reader.read_single(&team, match_id))
            .map_err(|e| unreadable(&e, labels.unreadable_match))?;
        name_the_removed(&mut m.team, labels);
        Ok(m)
    }

    /// The match of team A `team_id` as the team of the side sees it: B
    /// only of a match scouted on both sides (`SqliteMatchReader::read_of_b`).
    fn read_match_of(
        &self,
        team_id: &str,
        match_id: &str,
        side: MatchSide,
    ) -> Result<MatchEntry, MobileError> {
        let labels = self.language.labels();
        match side {
            MatchSide::A => self.read_match(team_id, match_id),
            MatchSide::B => {
                let team = self.read_team(team_id)?;
                let mut m = self
                    .match_reader
                    .read_of_b(&team, match_id)
                    .map_err(|e| unreadable(&e, labels.unreadable_match))?
                    .ok_or_else(|| MobileError::Failed {
                        reason: labels.one_side_only.to_string(),
                    })?;
                name_the_removed(&mut m.team, labels);
                Ok(m)
            }
        }
    }
}

/// What the user reads when a team or a match can't be read; the detail
/// (a file, a row, a field) goes to the log.
fn unreadable(error: &AppError, reason: &str) -> MobileError {
    log_error(&format!("{reason}: {error}"));
    MobileError::Failed {
        reason: reason.to_string(),
    }
}

/// The index of `position` (1 to 6) in the lineup; out of it for any other.
fn index(position: u8) -> usize {
    usize::from(position).wrapping_sub(1)
}

fn lineup_player(p: &PlayerEntry, labels: &Labels) -> LineupPlayer {
    LineupPlayer {
        id: p.id.to_string(),
        name: p.name.clone(),
        number: p.number,
        role: p.role,
        role_name: p.role.map(|r| r.friendly_name(labels).to_string()),
    }
}

/// A set of the match, by its number.
/// The events of the set, each with the score after it.
fn set_event_rows(
    m: &MatchEntry,
    set: &SetEntry,
    labels: &Labels,
) -> Result<Vec<SetEvent>, MobileError> {
    let scores = set.scores(m.max_substitutions)?;
    Ok(set
        .events
        .iter()
        .zip(scores)
        .enumerate()
        .map(|(i, (e, (score_us, score_them)))| SetEvent {
            index: u32::try_from(i).unwrap_or(u32::MAX),
            event: scouted_event(&m.team, e, labels),
            score_us,
            score_them,
            editable: !e.event_type.changes_lineup(),
        })
        .collect())
}

/// The touches of a set scouted on both sides, to correct them: the score
/// after each, from A's side.
fn touch_rows(
    m: &MatchEntry,
    team_b: &TeamEntry,
    touches: &scout4all_core::rally::Touches,
    labels: &Labels,
) -> Result<Vec<SetEvent>, MobileError> {
    touches
        .touches
        .iter()
        .enumerate()
        .map(|(i, (side, e))| {
            let (a, _) = scout4all_core::rally::replay(
                &touches.a,
                &touches.b,
                touches.touches.get(..=i).unwrap_or(&touches.touches),
                m.max_substitutions,
            )
            .map_err(|e| MobileError::Failed {
                reason: e.to_string(),
            })?;
            let (team, side_b) = match side {
                scout4all_core::domain::Side::A => (&m.team, false),
                scout4all_core::domain::Side::B => (team_b, true),
            };
            Ok(SetEvent {
                index: u32::try_from(i).unwrap_or(u32::MAX),
                event: ScoutedEvent {
                    side_b,
                    ..scouted_event(team, e, labels)
                },
                score_us: a.score_us,
                score_them: a.score_them,
                editable: !e.event_type.changes_lineup(),
            })
        })
        .collect()
}

/// For each event of B's side, set by set, the index of the event of A's
/// born from the same touch (`origins_of_b`).
pub type Origins = Vec<(u8, Vec<Option<usize>>)>;

/// What the profile keeps the event `index` of a side by (`EventRef`):
/// A's, itself; B's (`origins`), the event of A of the same touch, or, with
/// none, its own with the side B.
fn stored_ref(origins: Option<&Origins>, set_number: u8, index: usize) -> EventRef {
    origins.map_or(EventRef::of_a(set_number, index), |origins| {
        origins
            .iter()
            .find(|(set, _)| *set == set_number)
            .and_then(|(_, of_a)| of_a.get(index).copied().flatten())
            .map_or(EventRef::of_b(set_number, index), |a| {
                EventRef::of_a(set_number, a)
            })
    })
}

/// The events of the match the statistics counted for the pick, set by
/// set in order.
fn clips_of(m: &MatchEntry, pick: &ClipPick) -> Result<Vec<EventRef>, MobileError> {
    let filter = narrowed(&pick.filter)?;
    let picked = EventPick {
        event_type: pick.event,
        evals: (!pick.evals.is_empty()).then(|| EvalSet::of(&pick.evals)),
        zone: pick.zone.and_then(|z| ZoneEnum::try_from(z).ok()),
        target: pick.target,
        serve_start: pick
            .filter
            .serve_start
            .filter(|_| pick.event == EventTypeEnum::S),
        filter,
    };
    let mut sets: Vec<&SetEntry> = in_scope(&m.sets.iter().collect::<Vec<_>>(), pick.scope);
    sets.sort_by_key(|s| s.set_number);
    let mut refs = vec![];
    for set in sets {
        let keys = set
            .event_keys(m.max_substitutions)
            .map_err(|e| MobileError::Failed {
                reason: e.to_string(),
            })?;
        refs.extend(
            keys.iter()
                .enumerate()
                .filter(|(_, k)| k.as_ref().is_some_and(|k| picked.keeps(k)))
                .map(|(index, _)| EventRef::of_a(set.set_number, index)),
        );
    }
    Ok(refs)
}

fn find_set(m: &MatchEntry, set_number: u8) -> Result<&SetEntry, MobileError> {
    m.sets
        .iter()
        .find(|s| s.set_number == set_number)
        .ok_or_else(|| MobileError::Failed {
            reason: format!("no set {set_number} in {}", m.id),
        })
}

/// A day as `YYYY-MM-DD`.
fn parse_day(day: &str) -> Result<NaiveDate, MobileError> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|e| MobileError::Failed {
        reason: e.to_string(),
    })
}

/// A count for the app, which has no larger ones.
fn count_of(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn parse_id(id: &str) -> Result<Uuid, MobileError> {
    Uuid::parse_str(id).map_err(|e| MobileError::Failed {
        reason: e.to_string(),
    })
}

fn find_player(team: &TeamEntry, id: &str) -> Result<PlayerEntry, MobileError> {
    let id = parse_id(id)?;
    team.find_player(id)
        .cloned()
        .ok_or_else(|| MobileError::Failed {
            reason: format!("no player {id} in the team"),
        })
}

/// The players of the new season as the core keeps them.
fn season_members(players: &[SeasonPlayer]) -> Vec<SeasonMember> {
    players
        .iter()
        .map(|p| SeasonMember {
            player: p.id,
            number: p.number,
            role: p.role,
            chosen: p.chosen,
        })
        .collect()
}

fn players(team: &TeamEntry, labels: &Labels) -> Vec<Player> {
    let mut players: Vec<Player> = team
        .active_players()
        .into_iter()
        .map(|p| Player {
            id: p.id.to_string(),
            name: p.name.clone(),
            number: p.number,
            role: p.role.map(|r| r.friendly_name(labels).to_string()),
            initials: names::initials(&p.name),
            form: PlayerForm::of(p),
        })
        .collect();
    players.sort_by_key(|p| p.number);
    players
}

/// The players still in the team: the deleted ones stay for the old matches.
fn count(team: &TeamEntry) -> u32 {
    u32::try_from(team.active_players().len()).unwrap_or(u32::MAX)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::unreachable)]
mod tests {
    use super::*;
    use scout4all_core::{localization::labels, match_form::MatchInput, settings::TERMS_VERSION};

    #[test]
    fn an_empty_folder_has_no_teams() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        assert_eq!(app.teams().unwrap(), vec![]);
    }

    /// The data folder of the legacy fixtures of the core, as a version
    /// before the database left it.
    fn legacy_data(version: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../core/src/tests/fixtures/legacy")
            .join(version)
            .join("data")
    }

    fn copy_dir(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            match entry.file_type().unwrap().is_dir() {
                true => copy_dir(&entry.path(), &target),
                false => {
                    std::fs::copy(entry.path(), target).unwrap();
                }
            }
        }
    }

    /// What the lists show of every match, by team name: as the files
    /// tell it before and the database after (the ids aside).
    type Listed = std::collections::BTreeMap<String, Vec<(String, String, u8, u8, bool)>>;

    async fn listed_in_files(dir: &Path) -> Listed {
        use scout4all_core::legacy::folder::read_folder;
        let store = FsStore::new(dir);
        let mut listed = Listed::new();
        for team in read_folder(&store, Language::default().labels())
            .await
            .unwrap()
        {
            let team = team.unwrap();
            let mut matches: Vec<MatchEntry> =
                team.matches.into_iter().map(|m| m.unwrap()).collect();
            // the most recent first, as the lists
            matches.sort_by_key(|m| std::cmp::Reverse(m.date));
            listed.insert(
                team.team.name.clone(),
                matches
                    .iter()
                    .map(|m| {
                        let status = m.get_status().unwrap();
                        (
                            m.opponent.clone(),
                            m.date.to_rfc3339(),
                            status.us_wins,
                            status.them_wins,
                            status.match_finished,
                        )
                    })
                    .collect(),
            );
        }
        listed
    }

    fn listed_in_app(app: &Scout4All) -> Listed {
        app.teams()
            .unwrap()
            .into_iter()
            .map(|t| match t {
                TeamListing::Team { id, name, .. } => (
                    name,
                    app.matches(id)
                        .unwrap()
                        .into_iter()
                        .map(|m| match m {
                            MatchListing::Match {
                                opponent,
                                date,
                                won,
                                lost,
                                finished,
                                ..
                            } => (opponent, date, won, lost, finished),
                            other => panic!("{other:?}"),
                        })
                        .collect(),
                ),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    /// The day of the backups, as the migration names them.
    fn backup_of(dir: &tempfile::TempDir) -> PathBuf {
        let day = chrono::Local::now().date_naive();
        dir.path().join(format!("data-backup-{day}.zip"))
    }

    /// TODO/001 step 17: the app opens the database in the data folder and
    /// moves into it what a version before left there, a copy of the
    /// folder beside it first; nothing to tell when all moved, nothing to
    /// do at the next start.
    #[test]
    fn the_start_moves_the_folder_of_a_version_before_into_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        copy_dir(&legacy_data("v0.18.0"), &data);
        let expected = Runtime::new().unwrap().block_on(listed_in_files(&data));
        assert_eq!(expected.len(), 3);
        let app = Scout4All::new(data.to_string_lossy().into_owned(), "it".into()).unwrap();
        assert_eq!(app.startup_notice(), None);
        assert_eq!(listed_in_app(&app), expected);
        assert!(data.join(layout::DATABASE).exists());
        assert!(backup_of(&dir).exists());
        // only the database and the settings left: no folder
        let folders: Vec<_> = std::fs::read_dir(&data)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_dir())
            .collect();
        assert_eq!(folders, Vec::<PathBuf>::new());
        // the queue of the uploads moved with it
        assert!(!queued_of(&data).is_empty());
        drop(app);
        let again = Scout4All::new(data.to_string_lossy().into_owned(), "it".into()).unwrap();
        assert_eq!(again.startup_notice(), None);
        assert_eq!(listed_in_app(&again), expected);
        let backups = std::fs::read_dir(dir.path())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|x| x == "zip")
            })
            .count();
        assert_eq!(backups, 1);
    }

    fn queued_of(data: &Path) -> Vec<String> {
        let db = scout4all_core::db::open(&data.join(layout::DATABASE)).unwrap();
        let mut rows = db.prepare("SELECT match_id FROM upload_queue").unwrap();
        rows.query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    /// What can't be moved stays in the folder and is told once, in the
    /// language of the app, with where the copy is; the rest moves.
    #[test]
    fn what_stays_in_the_folder_is_told_once() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        copy_dir(&legacy_data("v0.18.0"), &data);
        // a set cut in two
        let broken = std::fs::read_dir(&data)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_dir())
            .flat_map(|team| std::fs::read_dir(team).unwrap().map(|e| e.unwrap().path()))
            .find(|p| p.join("set_1.json").exists())
            .unwrap();
        std::fs::write(broken.join("set_1.json"), "{\"serving").unwrap();
        let app = Scout4All::new(data.to_string_lossy().into_owned(), "it".into()).unwrap();
        let it = labels(LanguageEnum::It);
        let notice = app.startup_notice().unwrap();
        let lines: Vec<&str> = notice.lines().collect();
        assert_eq!(lines[0], it.migration_left);
        let folder = broken
            .strip_prefix(&data)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(lines[1].starts_with(&format!("- {folder}: ")), "{notice}");
        assert_eq!(
            lines[2],
            it.migration_backup
                .replace("{}", &backup_of(&dir).display().to_string())
        );
        assert_eq!(app.startup_notice(), None);
        assert!(broken.join("match.json").exists());
        assert_eq!(app.teams().unwrap().len(), 3);
        // the next start tells it again, without a new copy
        drop(app);
        let again = Scout4All::new(data.to_string_lossy().into_owned(), "it".into()).unwrap();
        assert_eq!(again.startup_notice().unwrap(), lines[..2].join("\n"));
    }

    /// No copy of the folder (where it goes can't be written): nothing
    /// moves, the app says why and opens all the same; the next start
    /// moves it.
    #[cfg(unix)]
    #[test]
    fn a_migration_that_cant_start_changes_nothing_and_is_told() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("parent");
        let data = parent.join("data");
        copy_dir(&legacy_data("v0.18.0"), &data);
        let expected = Runtime::new().unwrap().block_on(listed_in_files(&data));
        let mode = |m: u32| {
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(m)).unwrap();
        };
        mode(0o555);
        let app = Scout4All::new(data.to_string_lossy().into_owned(), "en".into());
        mode(0o755);
        let app = app.unwrap();
        let notice = app.startup_notice().unwrap();
        let en = labels(LanguageEnum::En);
        let (before, after) = en.migration_failed.split_once("{}").unwrap();
        assert!(
            notice.starts_with(before) && notice.ends_with(after),
            "{notice}"
        );
        assert_eq!(app.teams().unwrap(), vec![]);
        assert_eq!(
            Runtime::new().unwrap().block_on(listed_in_files(&data)),
            expected
        );
        drop(app);
        let again = Scout4All::new(data.to_string_lossy().into_owned(), "en".into()).unwrap();
        assert_eq!(again.startup_notice(), None);
        assert_eq!(listed_in_app(&again), expected);
    }

    /// A database saved by a newer version isn't opened: the app says to
    /// update it, the database stays as it is.
    #[test]
    fn a_database_of_a_newer_version_is_not_opened() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(layout::DATABASE);
        drop(scout4all_core::db::open(&file).unwrap());
        let newer = scout4all_core::db::schema_version() + 1;
        sql(&dir, &format!("PRAGMA user_version = {newer}"));
        let e = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into())
            .err()
            .unwrap();
        assert_eq!(reason_of(e), labels(LanguageEnum::It).io_database_newer);
    }

    /// The database that can't be opened is told in the language of the
    /// app being made, even when another app (another test running at the
    /// same time) changed the language of the errors meanwhile.
    #[test]
    fn the_database_not_opened_is_told_in_the_language_of_its_app() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(layout::DATABASE);
        drop(scout4all_core::db::open(&file).unwrap());
        let newer = scout4all_core::db::schema_version() + 1;
        sql(&dir, &format!("PRAGMA user_version = {newer}"));
        set_errors_language(Language::new(LanguageEnum::En));
        let it = Language::new(LanguageEnum::It);
        let e = open_database(&file, it.labels()).err().unwrap();
        assert_eq!(reason_of(e), labels(LanguageEnum::It).io_database_newer);
    }

    /// An app made by another test at the same time doesn't change the
    /// language of this test's errors.
    #[test]
    fn the_errors_of_a_test_stay_in_its_language() {
        let dir = tempfile::tempdir().unwrap();
        let _app = app(&dir, "it");
        std::thread::spawn(|| {
            let other = tempfile::tempdir().unwrap();
            drop(app(&other, "en"));
        })
        .join()
        .unwrap();
        let e = MobileError::from(AppError::IO(IOError::DatabaseNewer { found: 2, known: 1 }));
        assert_eq!(reason_of(e), labels(LanguageEnum::It).io_database_newer);
    }

    fn reason_of(e: MobileError) -> String {
        match e {
            MobileError::Failed { reason } => reason,
            other => format!("{other:?}"),
        }
    }

    /// The tutorial seen again the same day: another opponent, numbered,
    /// as the matches have UUIDs now and wouldn't clash.
    #[test]
    fn the_match_of_the_tutorial_seen_again_is_numbered() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let team = app.tutorial_team().unwrap();
        let date = DateTime::parse_from_rfc3339("2026-11-13T18:00:00+01:00").unwrap();
        let first = app.tutorial_match_form(team.clone(), date).unwrap();
        assert_eq!(first.opponent, "Brazil");
        app.save_match(team.clone(), None, first).unwrap();
        let second = app.tutorial_match_form(team.clone(), date).unwrap();
        assert_eq!(second.opponent, "Brazil 2");
        let next_day = date + chrono::Duration::days(1);
        assert_eq!(
            app.tutorial_match_form(team, next_day).unwrap().opponent,
            "Brazil"
        );
    }

    /// A team whose row the code can't read: listed as unreadable, the
    /// others as they are.
    #[test]
    fn a_team_that_cant_be_read_is_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "xx".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        // the schema doesn't check the spelling of the country, reading does
        sql(
            &dir,
            &format!("UPDATE teams SET country = 'it' WHERE id = '{team}'"),
        );
        assert_eq!(
            app.teams().unwrap(),
            vec![TeamListing::Unreadable { folder: team }]
        );
    }

    /// Changes the database of the app in `dir` behind its back, as a
    /// broken row would be.
    fn sql(dir: &tempfile::TempDir, sql: &str) {
        scout4all_core::db::open(&dir.path().join(layout::DATABASE))
            .unwrap()
            .execute_batch(sql)
            .unwrap();
    }

    /// Makes the match unreadable: a set 2 without the first, without
    /// lineup; the cache of the lists forgotten.
    fn break_match(dir: &tempfile::TempDir, match_id: &str) {
        sql(
            dir,
            &format!(
                "INSERT INTO sets (match_id, number, serving) VALUES ('{match_id}', 2, 'a');
                 DELETE FROM match_summary;"
            ),
        );
    }

    fn app(dir: &tempfile::TempDir, language: &str) -> Arc<Scout4All> {
        Scout4All::new(dir.path().to_string_lossy().into_owned(), language.into()).unwrap()
    }

    fn config(dir: &tempfile::TempDir) -> Value {
        from_slice(&std::fs::read(dir.path().join(CONFIG)).unwrap()).unwrap()
    }

    #[test]
    fn the_terms_are_seen_once_accepted() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        assert!(!app.terms_seen());
        app.accept_terms().unwrap();
        assert!(app.terms_seen());
        // the language too, which the TUI needs
        assert_eq!(
            config(&dir),
            serde_json::json!({
                "analytics_enabled": true,
                "terms_version": TERMS_VERSION,
                "language": "it",
            })
        );
    }

    #[test]
    fn accepting_the_terms_keeps_the_settings_of_the_tui() {
        let dir = tempfile::tempdir().unwrap();
        let tui = serde_json::json!({
            "language": "en",
            "scouting_keybindings": { "a": "Attack" },
            "analytics_enabled": false,
            "last_used_dir": "/home/coach",
            "terms_version": 0,
        });
        std::fs::write(dir.path().join(CONFIG), tui.to_string()).unwrap();
        let app = app(&dir, "it");
        assert!(!app.terms_seen());
        app.accept_terms().unwrap();
        let mut expected = tui;
        expected["terms_version"] = TERMS_VERSION.into();
        assert_eq!(config(&dir), expected);
    }

    /// Kept, they would show the terms at every start: accepting them sets
    /// the unreadable ones aside, as they were, and starts again from the
    /// defaults.
    #[test]
    fn unreadable_settings_are_set_aside_once_the_terms_are_accepted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(CONFIG), "{ not json").unwrap();
        let app = app(&dir, "it");
        assert!(!app.terms_seen());
        assert!(!app.analytics_enabled());
        app.accept_terms().unwrap();
        assert!(app.terms_seen());
        assert!(app.analytics_enabled());
        assert_eq!(
            std::fs::read_to_string(dir.path().join(UNREADABLE_CONFIG)).unwrap(),
            "{ not json"
        );
        // not a team
        assert_eq!(app.teams().unwrap(), vec![]);
    }

    #[test]
    fn a_new_installation_has_the_example_team_and_the_tutorial_to_show() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        assert!(app.add_example_team_if_new().unwrap());
        let teams = app.teams().unwrap();
        let id = match teams.as_slice() {
            [
                TeamListing::Team {
                    id,
                    name,
                    players: 14,
                    ..
                },
            ] if name == "Italia" => id.clone(),
            _ => unreachable!("the example team only: {teams:?}"),
        };
        // the terms still to accept, which keeps the tutorial
        assert!(!app.terms_seen());
        assert!(app.tutorial_pending());
        app.accept_terms().unwrap();
        assert!(app.tutorial_pending());
        assert_eq!(app.tutorial_team().unwrap(), id);
        // nothing more at the next start
        assert!(!app.add_example_team_if_new().unwrap());
        assert_eq!(app.teams().unwrap().len(), 1);
    }

    #[test]
    fn the_tutorial_seen_is_not_shown_again_and_keeps_its_team() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        app.add_example_team_if_new().unwrap();
        app.accept_terms().unwrap();
        let id = app.tutorial_team().unwrap();
        app.tutorial_seen().unwrap();
        assert!(!app.tutorial_pending());
        let config = config(&dir);
        assert_eq!(config.get("tutorial_pending"), None);
        assert_eq!(config["example_team"], serde_json::json!(id));
        assert_eq!(config["terms_version"], serde_json::json!(TERMS_VERSION));
        // seen again from the settings, with the same team
        assert_eq!(app.tutorial_team().unwrap(), id);
    }

    /// An update has the settings, or a team: neither the team nor the
    /// tutorial by itself.
    #[test]
    fn an_update_has_no_example_team_nor_the_tutorial() {
        let dir = tempfile::tempdir().unwrap();
        let settings = serde_json::json!({ "analytics_enabled": true, "terms_version": 1 });
        std::fs::write(dir.path().join(CONFIG), settings.to_string()).unwrap();
        let app = app(&dir, "it");
        assert!(!app.add_example_team_if_new().unwrap());
        assert_eq!(app.teams().unwrap(), vec![]);
        assert!(!app.tutorial_pending());
        app.accept_terms().unwrap();
        assert!(!app.tutorial_pending());
        assert_eq!(config(&dir).get("tutorial_pending"), None);
    }

    /// Seen again from the settings, the tutorial needs its players: a new
    /// example team when one was removed.
    #[test]
    fn the_tutorial_gets_a_new_team_when_its_players_changed() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        assert_eq!(app.teams().unwrap(), vec![]);
        // an update: none yet
        let first = app.tutorial_team().unwrap();
        assert_eq!(app.tutorial_team().unwrap(), first);
        let team = app.team(first.clone()).unwrap();
        let orro = team.players.iter().find(|p| p.number == 8).unwrap();
        app.remove_player(first.clone(), orro.id.clone()).unwrap();
        let second = app.tutorial_team().unwrap();
        assert_ne!(second, first);
        assert_eq!(app.team(second.clone()).unwrap().players.len(), 14);
        assert_eq!(app.teams().unwrap().len(), 2);
        assert_eq!(config(&dir)["example_team"], serde_json::json!(second));
    }

    #[test]
    fn the_terms_tell_about_the_statistics_in_the_language_of_the_app() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        let terms = app.terms();
        assert_eq!(terms.title, "termini di utilizzo");
        let it = labels(LanguageEnum::It);
        assert_eq!(
            terms.paragraphs,
            vec![
                it.terms_free,
                it.terms_license,
                it.terms_upload,
                it.terms_full
            ]
        );
        app.set_language("pl".into());
        assert_eq!(app.terms().title, "warunki korzystania");
        app.set_language("xx".into());
        assert_eq!(app.terms().title, "terms of use");
    }

    #[test]
    fn the_licenses_of_others_in_the_language_of_the_app_with_their_texts() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        let licenses = app.licenses();
        assert_eq!(licenses.title, "licenze di terze parti");
        assert_eq!(
            licenses
                .parts
                .iter()
                .map(|p| (p.name.as_str(), p.license.as_str(), p.purpose.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (
                    "SeetaFace Engine",
                    "BSD 2-Clause",
                    "trova il volto in una foto, per ritagliarla"
                ),
                (
                    "U-2-Net (U2NetP)",
                    "Apache License 2.0",
                    "toglie lo sfondo a una foto"
                ),
                (
                    "Source Sans 3",
                    "SIL Open Font License 1.1",
                    "il carattere dei report PDF"
                ),
                (
                    "Rust crates, Android libraries",
                    "MIT, Apache 2.0, …",
                    "le librerie con cui è fatta l'app"
                ),
            ]
        );
        // the texts whole, without the blank lines around them
        assert!(licenses.parts.iter().all(|p| p.text.len() > 1000));
        assert!(licenses.parts.iter().all(|p| p.text == p.text.trim()));
        app.set_language("ja".into());
        assert_eq!(app.licenses().title, "サードパーティのライセンス");
    }

    fn form(name: &str) -> TeamForm {
        TeamForm {
            name: name.into(),
            classification: Some(TeamClassificationEnum::Regional),
            gender: Some(GenderEnum::Women),
            country: CountryCode::known("IT"),
            year: "2026".into(),
        }
    }

    #[test]
    fn a_new_team_is_listed_and_can_be_edited() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let id = app.save_team(None, form(" Cortona ")).unwrap();
        let team = app.team(id.clone()).unwrap();
        assert_eq!(team.name, "Cortona");
        assert_eq!(team.gender.as_deref(), Some("femminile"));
        assert_eq!(team.form, form("Cortona"));

        let edited = app
            .save_team(Some(id.clone()), form("Cortona Volley"))
            .unwrap();
        assert_eq!(edited, id);
        assert_eq!(
            app.teams().unwrap(),
            vec![TeamListing::Team {
                id,
                name: "Cortona Volley".into(),
                year: 2026,
                players: 0,
                // created by hand
                pinned: true,
            }]
        );
    }

    /// The pinned teams at the top, each group by name; kept in
    /// `config.json` with the settings of the TUI.
    #[test]
    fn the_pinned_teams_come_first_and_keep_the_other_settings() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        app.accept_terms().unwrap();
        let ids: Vec<String> = ["Zeta", "alfa", "Beta"]
            .into_iter()
            .map(|name| app.save_team(None, form(name)).unwrap())
            .collect();
        let listed = |app: &Scout4All| -> Vec<(String, bool)> {
            app.teams()
                .unwrap()
                .into_iter()
                .filter_map(|t| match t {
                    TeamListing::Team { name, pinned, .. } => Some((name, pinned)),
                    TeamListing::Unreadable { .. } => None,
                })
                .collect()
        };
        let all = |pinned| {
            ["alfa", "Beta", "Zeta"]
                .map(|n| (n.to_string(), pinned))
                .to_vec()
        };
        assert_eq!(listed(&app), all(true));
        for id in &ids[1..] {
            app.set_pinned(id.clone(), false).unwrap();
        }
        assert_eq!(
            listed(&app),
            [("Zeta", true), ("alfa", false), ("Beta", false)]
                .map(|(n, p)| (n.to_string(), p))
                .to_vec()
        );
        app.set_pinned(ids[0].clone(), false).unwrap();
        assert_eq!(listed(&app), all(false));
        let config = config(&dir);
        assert_eq!(config.get("pinned_teams"), None, "{config}");
        assert!(config.get("language").is_some(), "{config}");
    }

    /// The opponent made from the match form starts unpinned, as the
    /// imported teams; the teams made by hand start pinned.
    #[test]
    fn the_opponent_made_in_the_match_form_is_not_pinned() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        app.accept_terms().unwrap();
        let team = app.save_team(None, form("Pisa")).unwrap();
        for n in 1..=7 {
            app.save_player(
                team.clone(),
                None,
                player(&format!("P{n}"), &n.to_string()),
                false,
            )
            .unwrap();
        }
        app.save_match(
            team,
            None,
            MatchForm {
                create_opponent: true,
                ..match_form("Arezzo", "2026-10-24T18:00:00+02:00", "8")
            },
        )
        .unwrap();
        let pins: Vec<(String, bool)> = app
            .teams()
            .unwrap()
            .into_iter()
            .filter_map(|t| match t {
                TeamListing::Team { name, pinned, .. } => Some((name, pinned)),
                TeamListing::Unreadable { .. } => None,
            })
            .collect();
        assert_eq!(
            pins,
            [("Pisa", true), ("Arezzo", false)]
                .map(|(n, p)| (n.to_string(), p))
                .to_vec()
        );
    }

    #[test]
    fn an_invalid_form_names_its_field() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let result = app.save_team(
            None,
            TeamForm {
                year: "26".into(),
                ..form("Cortona")
            },
        );
        match result {
            Err(MobileError::InvalidTeam { field, reason }) => {
                assert_eq!(field, TeamField::Year);
                assert_eq!(reason, "year must be a 4-digit number");
            }
            other => unreachable!("expected an invalid year, got {other:?}"),
        }
        assert_eq!(app.teams().unwrap(), vec![]);
    }

    #[test]
    fn the_country_is_guessed_once_and_offered_to_a_new_team() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        let app = Scout4All::new(path.clone(), "it".into()).unwrap();
        assert_eq!(app.new_team_form(2026).country, None);
        // no SIM, the network in France
        app.guess_default_country(vec!["".into(), "fr".into(), "it-IT".into()])
            .unwrap();
        assert_eq!(app.default_country().unwrap().name, "Francia");
        assert_eq!(app.new_team_form(2026).country, CountryCode::known("FR"));
        // guessed once: what the user chose stays
        app.set_default_country(CountryCode::known("JP").unwrap())
            .unwrap();
        app.guess_default_country(vec!["fr".into()]).unwrap();
        let config: Value = from_slice(&std::fs::read(dir.path().join(CONFIG)).unwrap()).unwrap();
        assert_eq!(config["country"], "JP");
        // the TUI reads the same config.json
        assert_eq!(config["language"], "it");
    }

    #[test]
    fn without_hints_the_country_of_the_language() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "pt".into()).unwrap();
        app.guess_default_country(vec![]).unwrap();
        assert_eq!(app.default_country().unwrap().name, "Brasil");
        let english = tempfile::tempdir().unwrap();
        let app =
            Scout4All::new(english.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        app.guess_default_country(vec!["".into()]).unwrap();
        assert_eq!(app.default_country(), None);
    }

    #[test]
    fn countries_by_code_or_name_in_the_language() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "pl".into()).unwrap();
        assert_eq!(app.countries(String::new()).len(), 249);
        let found = app.countries("wło".into());
        assert_eq!(found[0].name, "Włochy");
        assert_eq!(found[0].value.as_str(), "IT");
        assert_eq!(app.country_name(found[0].value), "Włochy");
        let id = app.save_team(None, form("Cortona")).unwrap();
        assert_eq!(app.team(id).unwrap().country.as_deref(), Some("Włochy"));
    }

    #[test]
    fn a_team_without_country_names_the_field() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let result = app.save_team(
            None,
            TeamForm {
                country: None,
                ..form("Cortona")
            },
        );
        match result {
            Err(MobileError::InvalidTeam { field, reason }) => {
                assert_eq!(field, TeamField::Country);
                assert_eq!(reason, "country is required");
            }
            other => unreachable!("expected a missing country, got {other:?}"),
        }
    }

    #[test]
    fn the_new_form_has_the_year_given() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        assert_eq!(app.new_team_form(2026).year, "2026");
        assert_eq!(
            app.classifications().len(),
            TeamClassificationEnum::ALL.len()
        );
        assert_eq!(app.genders()[0].name, "men");
    }

    fn player(name: &str, number: &str) -> PlayerForm {
        PlayerForm {
            name: name.into(),
            role: Some(RoleEnum::Setter),
            number: number.into(),
            birth_date: String::new(),
            height_cm: String::new(),
            notes: String::new(),
            nationality: None,
            dominant_hand: None,
            federation_id: String::new(),
            person: None,
        }
    }

    #[test]
    fn a_player_profile_lists_only_other_teams_of_the_same_season() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let first = app.save_team(None, form("First")).unwrap();
        let same_season = app.save_team(None, form("Second")).unwrap();
        let mut next_year_form = form("Next year");
        next_year_form.year = "2027".into();
        let next_year = app.save_team(None, next_year_form).unwrap();

        let player_id = app
            .save_player(first.clone(), None, player("Alex Player", "7"), false)
            .unwrap();
        for team in [&same_season, &next_year] {
            let mut form = player("Alex Player", "8");
            form.person = Some(parse_id(&player_id).unwrap());
            app.save_player(team.clone(), None, form, false).unwrap();
        }

        let profile = app.player_profile(first, player_id).unwrap();
        assert_eq!(profile.initials, "AP");
        assert_eq!(profile.age, None);
        assert_eq!(profile.matches_played, 0);
        assert_eq!(profile.matches_scouted, 0);
        assert_eq!(profile.sets_played, 0);
        assert_eq!(profile.points, 0);
        assert!(!profile.has_training);
        assert_eq!(profile.attendance_percent, None);
        assert_eq!(
            profile
                .other_teams
                .iter()
                .map(|team| team.id.as_str())
                .collect::<Vec<_>>(),
            vec![same_season.as_str()]
        );
    }

    /// A new player starts with the first number free in the team, from 1;
    /// none when they're all taken; a player removed frees theirs.
    #[test]
    fn a_new_player_starts_with_the_first_number_free() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        let number = || app.new_player_form(team.clone()).unwrap().number;
        assert_eq!(number(), "1");
        let save = |n: u8| {
            app.save_player(
                team.clone(),
                None,
                player(&format!("P{n}"), &n.to_string()),
                false,
            )
            .unwrap()
        };
        let one = save(1);
        save(2);
        save(4);
        assert_eq!(number(), "3");
        app.remove_player(team.clone(), one).unwrap();
        assert_eq!(number(), "1");
        (1..=99).filter(|n| ![2, 4].contains(n)).for_each(|n| {
            save(n);
        });
        assert_eq!(number(), "");
        assert_eq!(app.new_player_form(team).unwrap().name, "");
    }

    /// The registry through the app (TODO/001, step 22): a person of
    /// another team offered with their teams and added; the new season
    /// with the same persons; a person removed from the registry, named as
    /// a removed player on the score sheet of the matches played.
    #[test]
    fn the_registry_offers_adds_takes_to_a_new_season_and_removes() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let senior = app.save_team(None, form("Cortona")).unwrap();
        let ids: Vec<String> = (1..=7)
            .map(|n| {
                app.save_player(
                    senior.clone(),
                    None,
                    PlayerForm {
                        height_cm: "180".into(),
                        ..player(&format!("P{n}"), &n.to_string())
                    },
                    false,
                )
                .unwrap()
            })
            .collect();
        let under = app.save_team(None, form("Cortona U19")).unwrap();
        let offered = app.existing_players(under.clone()).unwrap();
        assert_eq!(offered.len(), 7);
        let p1 = &offered[0];
        assert_eq!(p1.label, "P1 (Cortona 2026)");
        assert_eq!(
            p1.form,
            PlayerForm {
                height_cm: "180".into(),
                person: Some(p1.id),
                ..player("P1", "1")
            }
        );
        let added = app
            .save_player(under.clone(), None, p1.form.clone(), false)
            .unwrap();
        assert_eq!(added, ids[0]);
        assert_eq!(app.existing_players(under.clone()).unwrap().len(), 6);
        assert_eq!(
            app.other_teams(under.clone(), added.clone()).unwrap(),
            ["Cortona (2026)".to_string()]
        );

        // the new season: P7 left, P2 with the 10
        let season = app.new_season(senior.clone()).unwrap();
        assert_eq!(season.form.year, "2027");
        assert_eq!(season.players.len(), 7);
        assert!(season.players.iter().all(|p| p.chosen));
        let mut players = season.players;
        players[6].chosen = false;
        let taken = app.correct_season_player(
            senior.clone(),
            players.clone(),
            players[1].id,
            "3".into(),
            None,
        );
        assert!(matches!(
            taken,
            Err(MobileError::InvalidPlayer {
                field: PlayerField::Number,
                ..
            })
        ));
        let players = app
            .correct_season_player(
                senior.clone(),
                players.clone(),
                players[1].id,
                "10".into(),
                Some(RoleEnum::Libero),
            )
            .unwrap();
        assert_eq!(
            players.last().map(|p| (p.number, p.role_name.as_deref())),
            Some((10, Some("libero")))
        );
        let new = app
            .save_new_season(senior.clone(), season.form, players)
            .unwrap();
        let details = app.team(new.clone()).unwrap();
        assert_eq!(details.year, 2027);
        let numbers: Vec<(String, u8)> = details
            .players
            .iter()
            .map(|p| (p.id.clone(), p.number))
            .collect();
        assert_eq!(numbers.len(), 6);
        assert!(numbers.contains(&(ids[1].clone(), 10)));
        assert!(!numbers.iter().any(|(id, _)| *id == ids[6]));
        // pinned, as a team created by hand
        assert!(app.teams().unwrap().iter().any(|t| matches!(
            t,
            TeamListing::Team { id, pinned: true, .. } if *id == new
        )));

        // a match played, then P3 removed from the registry: everywhere
        let m = app
            .save_match(
                senior.clone(),
                None,
                match_form("Arezzo", "2026-10-24T18:00:00+02:00", "8"),
            )
            .unwrap();
        app.delete_person(ids[2].clone()).unwrap();
        assert!(
            app.team(new)
                .unwrap()
                .players
                .iter()
                .all(|p| p.id != ids[2])
        );
        assert_eq!(app.team(senior.clone()).unwrap().players.len(), 6);
        let read = app.match_form(senior.clone(), m.clone()).unwrap();
        let roster = app.match_roster(senior, Some(m), read, None).unwrap();
        let gone = roster
            .players
            .iter()
            .find(|p| p.id.to_string() == ids[2])
            .map(|p| (p.name.as_str(), p.number));
        assert_eq!(gone, Some(("giocatore rimosso", 3)));
        assert!(app.delete_person("not an id".into()).is_err());
    }

    #[test]
    fn players_are_added_edited_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        let anna = app
            .save_player(team.clone(), None, player("Anna", "9"), false)
            .unwrap();
        app.save_player(team.clone(), None, player("Bea", "3"), false)
            .unwrap();
        app.save_player(
            team.clone(),
            Some(anna.clone()),
            player("Anna", "12"),
            false,
        )
        .unwrap();

        let players = app.team(team.clone()).unwrap().players;
        let listed: Vec<(&str, u8)> = players
            .iter()
            .map(|p| (p.name.as_str(), p.number))
            .collect();
        assert_eq!(listed, vec![("Bea", 3), ("Anna", 12)]);
        assert_eq!(players[1].id, anna);
        // the roles are neutral, in a women's team too
        assert_eq!(players[1].role.as_deref(), Some("palleggiatore"));
        assert_eq!(players[1].form, player("Anna", "12"));
        let roles = app.roles();
        let setter = roles.iter().find(|r| r.value == RoleEnum::Setter);
        assert_eq!(setter.map(|r| r.name.as_str()), Some("palleggiatore"));
        let men = app
            .save_team(
                None,
                TeamForm {
                    gender: Some(GenderEnum::Men),
                    ..form("Arezzo")
                },
            )
            .unwrap();
        app.save_player(men.clone(), None, player("Ugo", "1"), false)
            .unwrap();
        let ugo = &app.team(men.clone()).unwrap().players[0];
        assert_eq!(ugo.role.as_deref(), Some("palleggiatore"));

        app.remove_player(team.clone(), anna).unwrap();
        let players = app.team(team.clone()).unwrap().players;
        assert_eq!(players.len(), 1);
        // the number of a removed player is free again
        app.save_player(team, None, player("Carla", "12"), false)
            .unwrap();
    }

    #[test]
    fn matches_are_listed_the_most_recent_first() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team_id = app.save_team(None, form("Cortona")).unwrap();
        assert_eq!(app.matches(team_id.clone()).unwrap(), vec![]);

        let team = app.read_team(&team_id).unwrap();
        let mut ids = vec![];
        for (opponent, date, home) in [
            ("Arezzo", "2026-10-04T18:00:00+02:00", true),
            ("Siena", "2026-10-11T21:00:00+02:00", false),
            ("Lucca", "2026-10-18T21:00:00+02:00", false),
        ] {
            let date = DateTime::parse_from_rfc3339(date).unwrap();
            let m = app
                .runtime
                .block_on(app.match_writer.create(
                    &team,
                    MatchInput::against(opponent, date, home, 6, "", vec![]),
                ))
                .unwrap();
            ids.push(m.id);
        }
        break_match(&dir, &ids[2]);

        assert_eq!(
            app.matches(team_id).unwrap(),
            vec![
                MatchListing::Match {
                    id: ids[1].clone(),
                    opponent: "Siena".into(),
                    home: false,
                    date: "2026-10-11T21:00:00+02:00".into(),
                    won: 0,
                    lost: 0,
                    finished: false,
                    scouted_by: None,
                },
                MatchListing::Match {
                    id: ids[0].clone(),
                    opponent: "Arezzo".into(),
                    home: true,
                    date: "2026-10-04T18:00:00+02:00".into(),
                    won: 0,
                    lost: 0,
                    finished: false,
                    scouted_by: None,
                },
                MatchListing::Unreadable {
                    folder: ids[2].clone(),
                    reason: None,
                },
            ]
        );
    }

    /// A match removed, readable or not, leaves the list; a team removed
    /// leaves the list and the pins, the matches with it.
    #[test]
    fn teams_and_matches_are_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team_id = app.save_team(None, form("Cortona")).unwrap();
        let other = app.save_team(None, form("Arezzo")).unwrap();
        let team = app.read_team(&team_id).unwrap();
        let date = DateTime::parse_from_rfc3339("2026-10-04T18:00:00+02:00").unwrap();
        let create = |opponent: &str| {
            app.runtime
                .block_on(app.match_writer.create(
                    &team,
                    MatchInput::against(opponent, date, true, 6, "", vec![]),
                ))
                .unwrap()
        };
        let (m, broken) = (create("Siena"), create("Lucca"));
        break_match(&dir, &broken.id);
        assert_eq!(app.matches(team_id.clone()).unwrap().len(), 2);

        app.delete_match(team_id.clone(), m.id).unwrap();
        app.delete_match(team_id.clone(), broken.id).unwrap();
        assert_eq!(app.matches(team_id.clone()).unwrap(), vec![]);

        app.delete_team(team_id.clone()).unwrap();
        let ids: Vec<String> = app
            .teams()
            .unwrap()
            .into_iter()
            .filter_map(|t| match t {
                TeamListing::Team { id, .. } => Some(id),
                TeamListing::Unreadable { .. } => None,
            })
            .collect();
        assert_eq!(ids, vec![other.clone()]);
        let pins = config(&dir)["pinned_teams"].to_string();
        assert!(pins.contains(&other) && !pins.contains(&team_id), "{pins}");
    }

    fn match_form(opponent: &str, date: &str, max_substitutions: &str) -> MatchForm {
        MatchForm {
            opponent: opponent.into(),
            date: DateTime::parse_from_rfc3339(date).ok(),
            home: true,
            max_substitutions: max_substitutions.into(),
            ..MatchForm::default()
        }
    }

    /// The teams first, with their ids and years, then the names met that
    /// aren't a team; team B a team, read back for the form; the
    /// competition created once, then offered.
    #[test]
    fn team_b_and_the_competition_are_offered_and_saved() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        let livorno = app.save_team(None, form("Livorno")).unwrap();
        for n in 1..=7 {
            app.save_player(
                team.clone(),
                None,
                player(&format!("P{n}"), &n.to_string()),
                false,
            )
            .unwrap();
        }
        let date = "2026-10-24T18:00:00+02:00";
        for opponent in ["Arezzo", "livorno"] {
            app.save_match(team.clone(), None, match_form(opponent, date, "8"))
                .unwrap();
        }
        let livorno_id = parse_id(&livorno).unwrap();
        let offered = app.opponent_choices(team.clone()).unwrap();
        assert_eq!(
            offered
                .iter()
                .map(|o| (o.name.as_str(), o.team))
                .collect::<Vec<_>>(),
            vec![("Livorno", Some(livorno_id)), ("Arezzo", None)]
        );
        let m = app
            .save_match(
                team.clone(),
                None,
                MatchForm {
                    opponent_team: Some(livorno_id),
                    neutral: true,
                    competition: "Serie C".into(),
                    competition_format: Some(CompetitionFormat::Cup),
                    stage: Some(Stage::Playoff),
                    round: "Semifinale".into(),
                    ..match_form("Livorno", date, "8")
                },
            )
            .unwrap();
        let read = app.match_form(team.clone(), m.clone()).unwrap();
        let competitions = app.competitions().unwrap();
        assert_eq!(
            competitions
                .iter()
                .map(|c| (c.name.as_str(), c.format))
                .collect::<Vec<_>>(),
            vec![("Serie C", CompetitionFormat::Cup)]
        );
        assert_eq!(
            (
                read.opponent.as_str(),
                read.opponent_team,
                read.neutral,
                read.competition_id,
                read.stage,
                read.round.as_str()
            ),
            (
                "Livorno",
                Some(livorno_id),
                true,
                competitions.first().map(|c| c.id),
                Some(Stage::Playoff),
                "Semifinale"
            )
        );
        // saved as read: the same competition
        app.save_match(team.clone(), Some(m), read).unwrap();
        assert_eq!(app.competitions().unwrap().len(), 1);
        assert_eq!(app.stages().len(), 8);
        assert_eq!(
            app.competition_formats()
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Campionato", "Coppa", "Torneo"]
        );
    }

    /// The competitions in their list (TODO/001, step 23): counted with
    /// their matches, edited (a name taken refused on the name), removed
    /// leaving their matches without one.
    #[test]
    fn a_competition_is_listed_edited_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        for n in 1..=7 {
            app.save_player(
                team.clone(),
                None,
                player(&format!("P{n}"), &n.to_string()),
                false,
            )
            .unwrap();
        }
        let date = "2026-10-24T18:00:00+02:00";
        let mut matches = vec![];
        for (opponent, competition) in [("Arezzo", "Serie C"), ("Livorno", "Coppa")] {
            let form = MatchForm {
                competition: competition.into(),
                ..match_form(opponent, date, "8")
            };
            matches.push(app.save_match(team.clone(), None, form).unwrap());
        }
        let listed = app.competition_listings().unwrap();
        assert_eq!(
            listed
                .iter()
                .map(|c| (
                    c.name.as_str(),
                    c.season.as_deref(),
                    c.format.as_str(),
                    c.matches
                ))
                .collect::<Vec<_>>(),
            vec![
                ("Coppa", Some("2026/2027"), "Campionato", 1),
                ("Serie C", Some("2026/2027"), "Campionato", 1),
            ]
        );
        let (coppa, serie_c) = (listed[0].id, listed[1].id);

        let form = app.competition_form(coppa).unwrap();
        let taken = app.save_competition(
            coppa,
            CompetitionForm {
                name: "serie c".into(),
                ..form.clone()
            },
        );
        assert!(matches!(
            taken,
            Err(MobileError::InvalidCompetition { field: CompetitionField::Name, ref reason })
                if reason.contains("stagione")
        ));
        app.save_competition(
            coppa,
            CompetitionForm {
                name: "Coppa Toscana".into(),
                format: CompetitionFormat::Cup,
                ..form
            },
        )
        .unwrap();
        let edited = app.competition_form(coppa).unwrap();
        assert_eq!(
            (edited.name.as_str(), edited.format),
            ("Coppa Toscana", CompetitionFormat::Cup)
        );
        let read = app.match_form(team.clone(), matches[1].clone()).unwrap();
        assert_eq!(read.competition, "Coppa Toscana");

        app.delete_competition(serie_c).unwrap();
        assert_eq!(app.competition_listings().unwrap().len(), 1);
        assert!(app.competition_form(serie_c).is_err());
        assert!(app.delete_competition(serie_c).is_err());
        let read = app.match_form(team.clone(), matches[0].clone()).unwrap();
        assert_eq!((read.competition.as_str(), read.competition_id), ("", None));
        // the match stays, saved again as it is
        app.save_match(team, Some(matches[0].clone()), read)
            .unwrap();
    }

    /// Number and role corrected on the score sheet: a wrong one refused
    /// with the message of the roster, a good one shown, saved for the
    /// match and read back.
    #[test]
    fn a_number_of_the_score_sheet_is_corrected_and_saved() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        let ids: Vec<Uuid> = (1..=7)
            .map(|n| {
                let id = app
                    .save_player(
                        team.clone(),
                        None,
                        player(&format!("P{n}"), &n.to_string()),
                        false,
                    )
                    .unwrap();
                parse_id(&id).unwrap()
            })
            .collect();
        let two = ids[1];
        let new = match_form("Arezzo", "2026-10-24T18:00:00+02:00", "8");
        let taken = app.correct_roster_number(
            team.clone(),
            None,
            new.clone(),
            two,
            "3".into(),
            Some(RoleEnum::Setter),
        );
        match taken {
            Err(MobileError::InvalidMatch { field, reason }) => assert_eq!(
                (field, reason.as_str()),
                (
                    MatchField::Roster,
                    "un altro giocatore a referto ha questo numero"
                )
            ),
            other => unreachable!("expected the number in use, got {other:?}"),
        }
        let corrected = app
            .correct_roster_number(
                team.clone(),
                None,
                new,
                two,
                "12".into(),
                Some(RoleEnum::Libero),
            )
            .unwrap();
        let shown = corrected.players.last().unwrap();
        assert_eq!(
            (
                shown.id,
                shown.number,
                shown.role.as_deref(),
                shown.role_value
            ),
            (two, 12, Some("libero"), Some(RoleEnum::Libero))
        );
        let m = app.save_match(team.clone(), None, corrected.form).unwrap();
        let read = app.match_form(team.clone(), m.clone()).unwrap();
        let roster = app.match_roster(team.clone(), Some(m), read, None).unwrap();
        assert_eq!(
            roster.players.last().map(|p| (p.id, p.number)),
            Some((two, 12))
        );
        assert_eq!(app.team(team).unwrap().players[1].number, 2);
    }

    #[test]
    fn matches_are_created_and_edited() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        let players: Vec<Uuid> = (1..=7)
            .map(|n| {
                app.save_player(
                    team.clone(),
                    None,
                    player(&format!("P{n}"), &n.to_string()),
                    false,
                )
                .and_then(|id| parse_id(&id))
                .unwrap()
            })
            .collect();
        let date = "2026-10-24T18:00:00+02:00";
        let new = app.new_match_form(DateTime::parse_from_rfc3339(date).unwrap());
        assert_eq!(new.max_substitutions, "8");

        let id = app
            .save_match(team.clone(), None, match_form(" Arezzo ", date, "8"))
            .unwrap();
        // a new id, not the opponent and the day: two matches the same day
        // against the same opponent are two
        assert!(Uuid::parse_str(&id).is_ok(), "{id}");
        let edited = match_form("Arezzo Volley", date, "6");
        app.save_match(team.clone(), Some(id.clone()), edited.clone())
            .unwrap();
        // the roster of a new match: the players of the team
        assert_eq!(
            app.match_form(team.clone(), id.clone()).unwrap(),
            MatchForm {
                roster: Some(players),
                ..edited
            }
        );
        match app.matches(team).unwrap().as_slice() {
            [
                MatchListing::Match {
                    id: listed,
                    opponent,
                    ..
                },
            ] => {
                assert_eq!(listed, &id);
                assert_eq!(opponent, "Arezzo Volley");
            }
            other => unreachable!("expected the match, got {other:?}"),
        }
    }

    /// Over 14 players the roster is chosen: the core says who is on it,
    /// and refuses too many.
    #[test]
    fn a_match_of_a_large_team_chooses_its_roster() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        for n in 1..=15 {
            // a libero: more than 12 on the score sheet need one
            let role = Some(match n {
                7 => RoleEnum::Libero,
                _ => RoleEnum::Setter,
            });
            let player = PlayerForm {
                role,
                ..player(&format!("P{n}"), &n.to_string())
            };
            app.save_player(team.clone(), None, player, false).unwrap();
        }
        let date = "2026-10-24T18:00:00+02:00";
        let new = app
            .match_roster(team.clone(), None, match_form("Arezzo", date, "8"), None)
            .unwrap();
        assert_eq!(new.form.roster, None);
        assert_eq!(new.players.iter().filter(|p| p.chosen).count(), 15);
        match app.save_match(team.clone(), None, new.form.clone()) {
            Err(MobileError::InvalidMatch { field, .. }) => assert_eq!(field, MatchField::Roster),
            other => unreachable!("expected too many players, got {other:?}"),
        }
        // the fields are fine: the roster is the next step
        app.check_match_fields(team.clone(), None, new.form.clone())
            .unwrap();
        let first = new.players[0].id;
        let toggled = app
            .match_roster(team.clone(), None, new.form, Some(first))
            .unwrap();
        assert_eq!(
            toggled
                .players
                .iter()
                .map(|p| (p.number, p.chosen))
                .collect::<Vec<_>>(),
            (1..=15).map(|n| (n, n != 1)).collect::<Vec<_>>()
        );
        let id = app.save_match(team.clone(), None, toggled.form).unwrap();
        let edited = app.match_form(team.clone(), id.clone()).unwrap();
        let saved = app.match_roster(team, Some(id), edited, None).unwrap();
        assert_eq!(saved.players.iter().filter(|p| p.chosen).count(), 14);
    }

    #[test]
    fn an_invalid_match_names_its_field() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        match app.save_match(
            team.clone(),
            None,
            match_form("Arezzo", "2026-10-24T18:00:00+02:00", "0"),
        ) {
            Err(MobileError::InvalidMatch { field, reason }) => {
                assert_eq!(field, MatchField::MaxSubstitutions);
                assert_eq!(
                    reason,
                    "substitutions per set must be a number from 1 to 99"
                );
            }
            other => unreachable!("expected invalid substitutions, got {other:?}"),
        }
        assert_eq!(app.matches(team).unwrap(), vec![]);
    }

    #[test]
    fn a_set_starts_from_the_lineup_and_then_resumes() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        let ids: Vec<String> = (1..=7)
            .map(|n| {
                app.save_player(
                    team.clone(),
                    None,
                    player(&format!("P{n}"), &n.to_string()),
                    false,
                )
                .unwrap()
            })
            .collect();
        let date = "2026-10-24T18:00:00+02:00";
        let m = app
            .save_match(team.clone(), None, match_form("Arezzo", date, "8"))
            .unwrap();
        let details = app.match_details(team.clone(), m.clone()).unwrap();
        assert_eq!(details.next, NextSetKind::Start { set_number: 1 });
        assert_eq!(details.sets, vec![]);

        let start = app.set_start(team.clone(), m.clone()).unwrap();
        assert!(start.lineup().chooses_serving_team);
        // seven players: no fallback libero
        assert!(!start.lineup().allows_fallback_libero);
        start.choose_serving_team(TeamSideEnum::Them);
        for (position, id) in (1..=6).zip(&ids) {
            start.place(position, Some(id.clone())).unwrap();
        }
        assert_eq!(start.position_choices(1).len(), 2);
        assert_eq!(start.setter_choices().len(), 6);
        start.choose_setter(Some(ids[0].clone())).unwrap();
        assert!(!start.lineup().complete);
        match app.start_set(start.clone()) {
            Err(MobileError::Failed { reason }) => assert_eq!(reason, "libero is required"),
            other => unreachable!("expected a missing libero, got {other:?}"),
        }
        let liberos = start.libero_choices();
        assert_eq!(
            liberos.iter().map(|p| p.number).collect::<Vec<_>>(),
            vec![7]
        );
        start.choose_libero(Some(liberos[0].id.clone())).unwrap();
        assert!(start.lineup().complete);
        app.start_set(start).unwrap();

        let details = app.match_details(team.clone(), m.clone()).unwrap();
        assert_eq!(details.next, NextSetKind::Resume { set_number: 1 });
        assert_eq!(
            details.sets,
            vec![SetScore {
                number: 1,
                us: 0,
                them: 0
            }]
        );
        assert!(app.set_start(team, m).is_err());
    }

    /// A match between two teams (TODO/001, step 21): listed by both, B
    /// seeing it from its side (A the opponent, its court, the sets the
    /// other way), scouted by A, nothing to scout from B; the removal of
    /// a team counts the matches that stay to the other one.
    #[test]
    fn a_match_between_two_teams_is_seen_by_both() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let (team, _) = match_with_a_set(&app);
        let livorno = app.save_team(None, form("Livorno")).unwrap();
        let m = app
            .save_match(
                team.clone(),
                None,
                MatchForm {
                    opponent_team: Some(parse_id(&livorno).unwrap()),
                    home: false,
                    ..match_form("Livorno", "2026-10-25T18:00:00+02:00", "8")
                },
            )
            .unwrap();
        let scouting = started_set(&app, &team, &m);
        for _ in 0..25 {
            scouting.choose_eval(EvalEnum::Perfect).unwrap();
        }
        let of_a = app.match_details(team.clone(), m.clone()).unwrap();
        assert_eq!(
            (
                of_a.opponent.as_str(),
                of_a.home,
                of_a.won,
                of_a.lost,
                &of_a.next,
                &of_a.scouted_by
            ),
            (
                "Livorno",
                false,
                1,
                0,
                &NextSetKind::Start { set_number: 2 },
                &None
            )
        );
        let of_b = app.match_details(livorno.clone(), m.clone()).unwrap();
        assert_eq!(
            of_b,
            MatchDetails {
                opponent: "Cortona".into(),
                home: true,
                won: 0,
                lost: 1,
                sets: vec![SetScore {
                    number: 1,
                    us: 0,
                    them: 25
                }],
                next: NextSetKind::Over,
                scouted_by: Some(ScoutedBy {
                    team_id: team.clone(),
                    team_name: "Cortona".into()
                }),
                ..of_a
            }
        );
        match app.matches(livorno.clone()).unwrap().as_slice() {
            [
                MatchListing::Match {
                    id,
                    opponent,
                    home: true,
                    won: 0,
                    lost: 1,
                    finished: false,
                    scouted_by: Some(a),
                    ..
                },
            ] => assert_eq!((id, opponent.as_str(), a), (&m, "Cortona", &team)),
            other => unreachable!("{other:?}"),
        }
        // A's list: the match of the set and this one, its own
        let listed = app.matches(team.clone()).unwrap();
        assert!(
            listed.iter().all(|l| matches!(
                l,
                MatchListing::Match {
                    scouted_by: None,
                    ..
                }
            )),
            "{listed:?}"
        );
        assert_eq!(listed.len(), 2);
        // a match of Livorno not scouted by it: not in its statistics
        let analysis = app.team_analysis(
            livorno.clone(),
            AnalysisPeriod::LastMatches { count: 10 },
            None,
            EventTypeEnum::S,
            DetailFilter::default(),
        );
        assert!(analysis.is_ok_and(|a| a.matches.is_empty()));
        assert_eq!(
            app.team_removal(team.clone()).unwrap(),
            TeamRemovalCounts {
                sessions: 0,
                kept: 1,
                removed: 1
            }
        );
        assert_eq!(
            app.team_removal(livorno.clone()).unwrap(),
            TeamRemovalCounts {
                sessions: 0,
                kept: 1,
                removed: 0
            }
        );
    }

    /// The analysis and the clips of a team narrowed to a competition: the
    /// last matches are the last of the competition, the competitions
    /// offered are named, and none changes nothing.
    #[test]
    fn the_analysis_of_a_competition() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, first) = match_with_a_set(&app);
        let cup = |name: &str| MatchForm {
            competition: name.into(),
            competition_format: Some(CompetitionFormat::Cup),
            ..match_form("Siena", "2026-10-31T18:00:00+01:00", "8")
        };
        let second = app.save_match(team.clone(), None, cup("Coppa")).unwrap();
        for m in [&first, &second] {
            started_set(&app, &team, m)
                .choose_eval(EvalEnum::Perfect)
                .unwrap();
        }
        let coppa = app.competitions().unwrap().remove(0);
        let analysis = |competition, count| {
            app.team_analysis(
                team.clone(),
                AnalysisPeriod::LastMatches { count },
                competition,
                EventTypeEnum::S,
                DetailFilter::default(),
            )
            .unwrap()
        };
        let all = analysis(None, 2);
        assert_eq!(all.matches.len(), 2);
        assert_eq!(all.competition, None);
        assert_eq!(
            all.competitions
                .iter()
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["Coppa"]
        );
        // the last 2 of the cup are the only one of the cup
        let only = analysis(Some(coppa.id), 2);
        assert_eq!(only.competition, Some(coppa.id));
        assert_eq!(
            only.matches
                .iter()
                .map(|m| m.opponent.as_str())
                .collect::<Vec<_>>(),
            ["Siena"]
        );
        assert_eq!(only.summary.aces, 1);
        assert_eq!(all.summary.aces, 2);
        let clips = |competition| {
            app.team_clip_matches(
                team.clone(),
                AnalysisPeriod::LastMatches { count: 2 },
                competition,
                None,
                ClipPick {
                    scope: StatsScope::Match,
                    event: EventTypeEnum::S,
                    filter: DetailFilter::default(),
                    evals: vec![],
                    zone: None,
                    target: None,
                },
            )
            .unwrap()
            .len()
        };
        assert_eq!((clips(None), clips(Some(coppa.id))), (2, 1));
    }

    /// A team of seven, a match and its first set started, the team serving.
    fn match_with_a_set(app: &Scout4All) -> (String, String) {
        let team = app.save_team(None, form("Cortona")).unwrap();
        for n in 1..=7 {
            app.save_player(
                team.clone(),
                None,
                player(&format!("P{n}"), &n.to_string()),
                false,
            )
            .unwrap();
        }
        let m = app
            .save_match(
                team.clone(),
                None,
                match_form("Arezzo", "2026-10-24T18:00:00+02:00", "8"),
            )
            .unwrap();
        (team, m)
    }

    fn started_set(app: &Scout4All, team: &str, m: &str) -> Arc<Scouting> {
        let ids: Vec<String> = app
            .team(team.into())
            .unwrap()
            .players
            .into_iter()
            .map(|p| p.id)
            .collect();
        let start = app.set_start(team.into(), m.into()).unwrap();
        start.choose_serving_team(TeamSideEnum::Us);
        for (position, id) in (1..=6).zip(&ids) {
            start.place(position, Some(id.clone())).unwrap();
        }
        start.choose_setter(Some(ids[0].clone())).unwrap();
        start.choose_libero(Some(ids[6].clone())).unwrap();
        app.start_set(start).unwrap();
        app.scouting(team.into(), m.into()).unwrap()
    }

    /// A match against a team of seven scouted on both sides, its first set
    /// started, the team serving; the team, the match.
    fn set_started_on_both_sides(app: &Scout4All) -> (String, String) {
        let team = app.save_team(None, form("Cortona")).unwrap();
        let other = app.save_team(None, form("Livorno")).unwrap();
        for (t, prefix) in [(&team, "P"), (&other, "L")] {
            for n in 1..=7 {
                app.save_player(
                    t.clone(),
                    None,
                    player(&format!("{prefix}{n}"), &n.to_string()),
                    false,
                )
                .unwrap();
            }
        }
        let m = app
            .save_match(
                team.clone(),
                None,
                MatchForm {
                    opponent_team: Some(parse_id(&other).unwrap()),
                    both: true,
                    ..match_form("Livorno", "2026-10-24T18:00:00+02:00", "8")
                },
            )
            .unwrap();
        let ids = |t: &str| -> Vec<String> {
            app.team(t.into())
                .unwrap()
                .players
                .into_iter()
                .map(|p| p.id)
                .collect()
        };
        let place = |start: &SetStart, ids: &[String]| {
            for (position, id) in (1..=6).zip(ids) {
                start.place(position, Some(id.clone())).unwrap();
            }
            start.choose_setter(Some(ids[0].clone())).unwrap();
            start.choose_libero(Some(ids[6].clone())).unwrap();
        };
        let start = app.set_start(team.clone(), m.clone()).unwrap();
        start.choose_serving_team(TeamSideEnum::Us);
        place(&start, &ids(&team));
        let of_b = app
            .set_start_of_b(team.clone(), m.clone())
            .unwrap()
            .unwrap();
        place(&of_b, &ids(&other));
        app.start_set_both(start, of_b).unwrap();
        (team, m)
    }

    /// While scouting both sides, the statistics of B are of B's team; none
    /// scouting A only.
    #[test]
    fn the_statistics_of_b_while_scouting() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let (team, m) = set_started_on_both_sides(&app);
        let scouting = app.scouting(team.clone(), m).unwrap();
        scouting.choose_eval(EvalEnum::Perfect).unwrap();
        let filter = DetailFilter::default();
        let b = scouting
            .stats_of_b(StatsScope::Match, filter.clone())
            .unwrap()
            .unwrap();
        assert_eq!(
            (b.team_name.as_str(), b.opponent.as_str()),
            ("Livorno", "Cortona")
        );
        assert_eq!((b.sets.clone(), b.aces_against), (vec![1], 1));
        let detail = scouting
            .detail_of_b(StatsScope::Match, EventTypeEnum::R, filter.clone())
            .unwrap()
            .unwrap();
        assert!(detail.players.iter().all(|p| p.name.starts_with('L')));
        let (one, single) = match_with_a_set(&app);
        let alone = started_set(&app, &one, &single);
        assert_eq!(
            alone.stats_of_b(StatsScope::Match, filter.clone()).unwrap(),
            None
        );
        assert_eq!(
            alone
                .detail_of_b(StatsScope::Match, EventTypeEnum::S, filter)
                .unwrap(),
            None
        );
    }

    /// The set in progress of a match on both sides scouted again on both;
    /// B unreadable meanwhile (the match no longer on both sides, B's
    /// touches still there): told why, not "could not compute".
    #[test]
    fn a_set_on_both_sides_resumed_or_told_why_not() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let (team, m) = set_started_on_both_sides(&app);
        assert!(app.scouting(team.clone(), m.clone()).unwrap().state().both);
        sql(&dir, "UPDATE matches SET scouting = 'a';");
        let reason = app.scouting(team, m).err().map(reason_of);
        assert_eq!(
            reason.as_deref(),
            Some(labels(LanguageEnum::En).one_side_only)
        );
    }

    /// The digs of the detail of the defense: all of them with none, the
    /// ones that aren't covers, only the covers (our dig after our attack
    /// blocked back, `A!`).
    #[test]
    fn the_defense_by_digs() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        let player = |n: usize| {
            scouting
                .player_choices()
                .into_iter()
                .nth(n)
                .map(|p| p.id)
                .unwrap_or_default()
        };
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        for (event, eval) in [
            (EventTypeEnum::D, EvalEnum::Positive),
            (EventTypeEnum::A, EvalEnum::Exclamative),
            (EventTypeEnum::D, EvalEnum::Negative),
            (EventTypeEnum::A, EvalEnum::Perfect),
        ] {
            scouting.choose_event(event).unwrap();
            scouting.choose_player(player(1)).unwrap();
            scouting.choose_eval(eval).unwrap();
        }
        let total = |digs| {
            app.match_detail(
                team.clone(),
                m.clone(),
                MatchSide::A,
                StatsScope::Match,
                EventTypeEnum::D,
                DetailFilter {
                    digs,
                    ..DetailFilter::default()
                },
            )
            .unwrap()
            .fundamental
            .map_or(0, |f| f.total)
        };
        assert_eq!(total(None), 2);
        assert_eq!(total(Some(DigsEnum::All)), 2);
        assert_eq!(total(Some(DigsEnum::Defense)), 1);
        assert_eq!(total(Some(DigsEnum::Cover)), 1);
    }

    /// Who comes in has the initials, in place of a missing photo.
    #[test]
    fn the_replacements_have_the_initials() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        // seven on court with the libero: one on the bench
        for n in 1..=8 {
            app.save_player(
                team.clone(),
                None,
                player(&format!("P{n} Rossi"), &n.to_string()),
                false,
            )
            .unwrap();
        }
        let m = app
            .save_match(
                team.clone(),
                None,
                match_form("Arezzo", "2026-10-24T18:00:00+02:00", "8"),
            )
            .unwrap();
        let scouting = started_set(&app, &team, &m);
        scouting.choose_event(EventTypeEnum::R).unwrap();
        let out = scouting.player_choices().into_iter().nth(1).map(|p| p.id);
        scouting.choose_player(out.unwrap_or_default()).unwrap();
        let replacements = scouting.replacement_choices();
        assert!(!replacements.is_empty());
        for p in replacements {
            assert_eq!(p.initials, "PR");
        }
    }

    /// Who goes out: the one out for the libero too, though not on court,
    /// told apart; never the libero.
    #[test]
    fn the_one_out_for_the_libero_can_go_out() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        for n in 1..=8 {
            app.save_player(
                team.clone(),
                None,
                player(&format!("P{n} Rossi"), &n.to_string()),
                false,
            )
            .unwrap();
        }
        let m = app
            .save_match(
                team.clone(),
                None,
                match_form("Arezzo", "2026-10-24T18:00:00+02:00", "8"),
            )
            .unwrap();
        let scouting = started_set(&app, &team, &m);
        // the libero 7 on court for 6
        let court: Vec<u8> = scouting.state().court.iter().map(|p| p.number).collect();
        assert!(court.contains(&7) && !court.contains(&6));
        scouting.choose_event(EventTypeEnum::R).unwrap();
        let choices = scouting.player_choices();
        let numbers = |out: bool| -> Vec<u8> {
            choices
                .iter()
                .filter(|p| p.out_for_the_libero == out)
                .map(|p| p.number)
                .collect()
        };
        assert_eq!(numbers(true), vec![6]);
        assert_eq!(numbers(false), vec![1, 2, 3, 4, 5]);
        let out = choices.iter().find(|p| p.number == 6).map(|p| p.id.clone());
        scouting.choose_player(out.unwrap_or_default()).unwrap();
        assert!(
            scouting
                .replacement_choices()
                .iter()
                .all(|p| p.number == 8 && !p.out_for_the_libero)
        );
    }

    /// How many blocked: on our block from one on, the one chosen saved
    /// with the evaluation and shown on the event; none on the serve.
    #[test]
    fn how_many_blocked_is_told() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        assert!(scouting.available_blockers().is_empty());
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        scouting.choose_event(EventTypeEnum::B).unwrap();
        let blocker = scouting.player_choices().into_iter().next().unwrap().id;
        scouting.choose_player(blocker).unwrap();
        let names: Vec<String> = scouting
            .available_blockers()
            .into_iter()
            .map(|b| b.name)
            .collect();
        assert_eq!(
            names,
            vec!["muro a 1", "muro a 2", "muro a 3", "muro con buco"]
        );
        assert!(
            scouting
                .choose_blockers(Some(BlockersEnum::NoBlock))
                .is_err()
        );
        scouting.choose_blockers(Some(BlockersEnum::Two)).unwrap();
        let chosen: Vec<BlockersEnum> = scouting
            .available_blockers()
            .into_iter()
            .filter(|b| b.chosen)
            .map(|b| b.value)
            .collect();
        assert_eq!(chosen, vec![BlockersEnum::Two]);
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        let state = scouting.state();
        assert_eq!(state.events[0].blockers.as_deref(), Some("muro a 2"));
        assert_eq!(state.events[1].blockers, None);
    }

    /// Our block: one row, the zones at the net on our court, 4 3 2;
    /// touched with no subzone, again taken back; in the stats one row.
    #[test]
    fn the_block_target_is_at_the_net() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        scouting.choose_event(EventTypeEnum::B).unwrap();
        let blocker = scouting.player_choices().into_iter().next().unwrap().id;
        scouting.choose_player(blocker).unwrap();
        assert!(scouting.target_at_net() && scouting.target_ours());
        let labels = |s: &Scouting| -> Vec<Vec<String>> {
            s.target_court()
                .iter()
                .map(|row| row.iter().map(|c| c.label.clone()).collect())
                .collect()
        };
        assert_eq!(labels(&scouting), vec![vec!["4", "3", "2"]]);
        assert!(matches!(
            scouting.touch_target(3, "A".into()),
            Err(MobileError::Scouting { .. })
        ));
        scouting.touch_target(3, String::new()).unwrap();
        scouting.touch_target(3, String::new()).unwrap();
        assert_eq!(scouting.target_told(), None);
        scouting.touch_target(4, String::new()).unwrap();
        let chosen: Vec<String> = scouting
            .target_court()
            .into_iter()
            .flatten()
            .filter(|c| c.chosen)
            .map(|c| c.label)
            .collect();
        assert_eq!(chosen, vec!["4"]);
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        let state = scouting.state();
        let targets: Vec<Option<&str>> = state.events.iter().map(|e| e.target.as_deref()).collect();
        // the latest first
        assert_eq!(targets, vec![Some("4"), None]);
        assert!(!scouting.target_at_net());
    }

    /// Where our serve went: their court while it waits for its
    /// evaluation, the zone chosen or taken back, never required; where we
    /// received theirs: our court, turned round, once the receiver is told.
    #[test]
    fn the_serve_target_is_told_on_the_court() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        let court = scouting.target_court();
        let labels: Vec<Vec<&str>> = court
            .iter()
            .map(|row| row.iter().map(|c| c.label.as_str()).collect())
            .collect();
        assert_eq!(
            labels,
            vec![
                vec!["1A", "1D", "6A", "6D", "5A", "5D"],
                vec!["1B", "1C", "6B", "6C", "5B", "5C"],
                vec!["9A", "9D", "8A", "8D", "7A", "7D"],
                vec!["9B", "9C", "8B", "8C", "7B", "7C"],
                vec!["2A", "2D", "3A", "3D", "4A", "4D"],
                vec!["2B", "2C", "3B", "3C", "4B", "4C"],
            ]
        );
        assert!(court.iter().flatten().all(|c| !c.chosen));
        assert!(!scouting.target_ours());
        assert_eq!(scouting.target_told(), None);
        // 3D, then 4A next to it: on the line between 3 and 4
        scouting.touch_target(3, "D".into()).unwrap();
        assert_eq!(scouting.target_told().as_deref(), Some("3D"));
        scouting.touch_target(4, "A".into()).unwrap();
        let chosen: Vec<String> = scouting
            .target_court()
            .into_iter()
            .flatten()
            .filter(|c| c.chosen)
            .map(|c| c.label)
            .collect();
        assert_eq!(chosen, vec!["3D", "4A"]);
        assert_eq!(scouting.target_told().as_deref(), Some("3D-4A (4/3)"));
        scouting.choose_eval(EvalEnum::Error).unwrap();
        let state = scouting.state();
        let targets: Vec<Option<&str>> = state.events.iter().map(|e| e.target.as_deref()).collect();
        assert_eq!(targets, vec![Some("3D-4A (4/3)")]);
        // their serve: no court before the receiver
        scouting.choose_event(EventTypeEnum::P).unwrap();
        assert!(scouting.target_court().is_empty());
        assert!(matches!(
            scouting.touch_target(1, "A".into()),
            Err(MobileError::Scouting { .. })
        ));
        let receiver = scouting.player_choices().into_iter().next().unwrap().id;
        scouting.choose_player(receiver).unwrap();
        assert!(scouting.target_ours());
        let rows: Vec<String> = scouting
            .target_court()
            .iter()
            .map(|row| {
                row.iter()
                    .map(|c| c.label.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        assert_eq!(rows.first().map(String::as_str), Some("4C 4B 3C 3B 2C 2B"));
        assert_eq!(rows.last().map(String::as_str), Some("5D 5A 6D 6A 1D 1A"));
        scouting.touch_target(6, "A".into()).unwrap();
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        let state = scouting.state();
        assert_eq!(
            state.events.first().and_then(|e| e.target.as_deref()),
            Some("6A")
        );
    }

    #[test]
    fn a_set_in_progress_is_scouted_and_undone() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        // no set in progress yet
        assert!(app.scouting(team.clone(), m.clone()).is_err());
        let scouting = started_set(&app, &team, &m);
        let state = scouting.state();
        assert_eq!(
            (state.set_number, state.score_us, state.score_them),
            (1, 0, 0)
        );
        assert_eq!(state.serving_team, TeamSideEnum::Us);
        assert_eq!(state.court.len(), 6);
        assert!(state.court[0].setter);
        assert_eq!(state.rotation, "P1");
        // we serve: in the middle of the zones
        assert_eq!((state.court[0].x, state.court[0].y), (7.5, 6.0));
        assert_eq!(state.substitutions_left, 8);

        // we serve: the server is known, the serve waits for its evaluation
        assert_eq!(state.step, ScoutingStep::Eval);
        assert_eq!(state.event.map(|e| e.value), Some(EventTypeEnum::S));
        assert_eq!(state.player.map(|p| p.number), Some(1));
        assert!(state.court.iter().all(|p| !p.selectable));
        assert!(
            scouting
                .other_events()
                .iter()
                .any(|e| e.value == EventTypeEnum::OE && e.name == "errore avversario")
        );
        let serves = scouting.available_techniques();
        assert_eq!(serves.len(), 2);
        assert_eq!(serves.iter().filter(|t| t.chosen).count(), 1);
        scouting
            .choose_technique(TechniqueEnum::Serve(ServeTypeEnum::Spin))
            .unwrap();
        assert!(
            scouting
                .available_evals()
                .iter()
                .any(|e| e.value == EvalEnum::Perfect && e.symbol == "#" && e.name == "ace")
        );
        assert_eq!(
            scouting.choose_eval(EvalEnum::Perfect).unwrap(),
            Progress::Saved
        );
        let state = scouting.state();
        assert_eq!((state.score_us, state.score_them), (1, 0));
        // serving again
        assert_eq!(state.step, ScoutingStep::Eval);
        match state.events.as_slice() {
            [event] => {
                assert_eq!(event.event, EventTypeEnum::S);
                assert_eq!(event.eval.as_deref(), Some("ace"));
                assert_eq!(event.technique.as_deref(), Some("S"));
            }
            other => unreachable!("expected the ace, got {other:?}"),
        }
        // a choice that doesn't fit the step says why
        match scouting.choose_player_first(state.court[1].id.clone()) {
            Err(MobileError::Scouting { .. }) => {}
            other => unreachable!("expected a scouting error, got {other:?}"),
        }
        // the ace is saved: a new scouting of the set starts from it
        let resumed = app.scouting(team.clone(), m.clone()).unwrap();
        assert_eq!(resumed.state().score_us, 1);

        // they receive: the player who digs first, then the evaluation or
        // the event
        let outside_hitter = state.court[1].id.clone();
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        let state = scouting.state();
        assert_eq!((state.step, state.player), (ScoutingStep::Event, None));
        // the serve is made: the outside hitter in 2 plays in 4
        assert_eq!(state.court[3].id, outside_hitter);
        assert!(state.court[0].setter);
        let digger = state.court.iter().find(|p| p.selectable).unwrap();
        let events = scouting.player_events(digger.id.clone()).unwrap();
        assert!(events.iter().any(|e| e.value == EventTypeEnum::D));
        scouting.choose_player_first(digger.id.clone()).unwrap();
        let state = scouting.state();
        assert_eq!(state.player.map(|p| p.id), Some(digger.id.clone()));
        match events.as_slice() {
            [_] => assert_eq!(state.step, ScoutingStep::Eval),
            _ => assert_eq!(state.step, ScoutingStep::Event),
        }

        // undo goes back to the players, then removes the serve and resumes
        // it from its evaluation, then the ace
        scouting.undo().unwrap();
        let state = scouting.state();
        assert_eq!((state.step, state.player), (ScoutingStep::Event, None));
        scouting.undo().unwrap();
        assert_eq!(scouting.state().events.len(), 1);
        scouting.undo().unwrap();
        let state = scouting.state();
        assert_eq!((state.score_us, state.step), (0, ScoutingStep::Eval));
        assert!(
            scouting
                .available_techniques()
                .iter()
                .any(|t| t.chosen && t.value == TechniqueEnum::Serve(ServeTypeEnum::Spin))
        );
        assert_eq!(app.scouting(team, m).unwrap().state().events, vec![]);
    }

    /// A set this version doesn't replay, e.g. saved by a version whose
    /// scouting allowed an event no more: the match is unreadable, and
    /// tells why.
    #[test]
    fn a_match_whose_set_does_not_replay_tells_why() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        scouting.choose_eval(EvalEnum::Over).unwrap();
        // their freeball after our serve `/`: their reception came back
        sql(
            &dir,
            &format!(
                "INSERT INTO events (id, match_id, set_number, seq, side, timestamp, event_type)
                 VALUES ('{}', '{m}', 1, 1, 'a', '2026-10-24T18:00:00Z', 'OF');
                 DELETE FROM match_summary;",
                Uuid::new_v4()
            ),
        );
        let listed = app.matches(team).unwrap();
        assert_eq!(
            listed,
            vec![MatchListing::Unreadable {
                folder: m,
                reason: Some("set 1, row 2: OF after S/ isn't valid in this version".into()),
            }]
        );
    }

    /// Our serve `/`, their reception straight over: our block among the
    /// events of the players in the front row, only as the invasion (the
    /// block answers an attack, scouting rules v4), saved.
    #[test]
    fn our_block_on_their_reception_straight_over() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        scouting.choose_eval(EvalEnum::Over).unwrap();
        let state = scouting.state();
        let blockers: Vec<_> = state
            .court
            .iter()
            .filter(|p| {
                scouting
                    .player_events(p.id.clone())
                    .unwrap()
                    .iter()
                    .any(|e| e.value == EventTypeEnum::B)
            })
            .collect();
        assert_eq!(blockers.len(), 3);
        scouting
            .choose_player_first(blockers[0].id.clone())
            .unwrap();
        if scouting.state().step == ScoutingStep::Event {
            scouting.choose_event(EventTypeEnum::B).unwrap();
        }
        let evals: Vec<_> = scouting.available_evals().iter().map(|e| e.value).collect();
        assert_eq!(evals, vec![EvalEnum::Over]);
        assert!(scouting.choose_eval(EvalEnum::Perfect).is_err());
        assert_eq!(
            scouting.choose_eval(EvalEnum::Over).unwrap(),
            Progress::Saved
        );
        match scouting.state().events.as_slice() {
            [block, ..] => assert_eq!(block.event, EventTypeEnum::B),
            other => unreachable!("expected the block, got {other:?}"),
        }
    }

    /// Our serve `!`, their reception and attack, our dig: our freeball is
    /// a team event, among the other events and not the player's, saved
    /// with no player.
    #[test]
    fn our_freeball_is_a_team_event() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        assert!(
            scouting
                .available_evals()
                .iter()
                .any(|e| e.value == EvalEnum::Exclamative && e.symbol == "!")
        );
        scouting.choose_eval(EvalEnum::Exclamative).unwrap();
        let state = scouting.state();
        let digger = state.court.iter().find(|p| p.selectable).unwrap();
        scouting.choose_player_first(digger.id.clone()).unwrap();
        if scouting.state().step == ScoutingStep::Event {
            scouting.choose_event(EventTypeEnum::D).unwrap();
        }
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        let state = scouting.state();
        for player in state.court.iter().filter(|p| p.selectable) {
            let events = scouting.player_events(player.id.clone()).unwrap();
            assert!(events.iter().all(|e| e.value != EventTypeEnum::FB));
        }
        assert!(
            scouting
                .other_events()
                .iter()
                .any(|e| e.value == EventTypeEnum::FB)
        );
        assert_eq!(
            scouting.choose_event(EventTypeEnum::FB).unwrap(),
            Progress::Saved
        );
        // the latest first
        match scouting.state().events.as_slice() {
            [freeball, ..] => {
                assert_eq!(freeball.event, EventTypeEnum::FB);
                assert_eq!(freeball.player, None);
            }
            other => unreachable!("expected the freeball, got {other:?}"),
        }
    }

    /// Our fault, among the other events: the team can make it (e.g. a
    /// rotation fault), saved with no player; only the fault offers the
    /// team.
    #[test]
    fn our_fault_can_be_the_teams() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        assert!(!scouting.state().team_choice);
        assert!(scouting.choose_team().is_err());
        scouting.choose_event(EventTypeEnum::D).unwrap();
        let state = scouting.state();
        assert_eq!(state.step, ScoutingStep::Player);
        assert!(!state.team_choice);
        assert!(scouting.choose_team().is_err());
        scouting.undo().unwrap();
        assert!(
            scouting
                .other_events()
                .iter()
                .any(|e| e.value == EventTypeEnum::F)
        );
        scouting.choose_event(EventTypeEnum::F).unwrap();
        let state = scouting.state();
        assert_eq!(state.step, ScoutingStep::Player);
        assert!(state.team_choice);
        assert_eq!(scouting.choose_team().unwrap(), Progress::Saved);
        let state = scouting.state();
        assert!(!state.team_choice);
        assert_eq!(state.score_them, 1);
        // the latest first
        match state.events.as_slice() {
            [fault, ..] => {
                assert_eq!(fault.event, EventTypeEnum::F);
                assert_eq!(fault.player, None);
            }
            other => unreachable!("expected the fault, got {other:?}"),
        }
    }

    /// Before the serve the libero goes out, then back in: the event says
    /// which, saved at once; never within the rally.
    #[test]
    fn the_libero_goes_out_and_back_in() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        let name = |scouting: &Scouting| {
            scouting
                .other_events()
                .into_iter()
                .find(|e| e.value == EventTypeEnum::LR)
                .map(|e| e.name)
        };
        let en = labels(LanguageEnum::En);
        assert_eq!(name(&scouting).as_deref(), Some(en.libero_out));
        assert_eq!(
            scouting.choose_event(EventTypeEnum::LR).unwrap(),
            Progress::Saved
        );
        assert_eq!(name(&scouting).as_deref(), Some(en.libero_in));
        assert_eq!(
            scouting.choose_event(EventTypeEnum::LR).unwrap(),
            Progress::Saved
        );
        assert_eq!(name(&scouting).as_deref(), Some(en.libero_out));
        match scouting.state().events.as_slice() {
            [first, second] => {
                assert_eq!(
                    (first.event, second.event),
                    (EventTypeEnum::LR, EventTypeEnum::LR)
                );
                assert_eq!((first.player.clone(), second.player.clone()), (None, None));
            }
            other => unreachable!("expected the libero out and in, got {other:?}"),
        }
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        assert_eq!(name(&scouting), None);
        assert!(scouting.choose_event(EventTypeEnum::LR).is_err());
    }

    /// Before the serve, the libero for a player touched on court (the
    /// libero aside): saved with the player; on court for the middle
    /// blocker, the libero goes out at once and comes in for the player
    /// after a rally.
    #[test]
    fn the_libero_for_a_player_of_the_back_row() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        let ids: Vec<String> = scouting
            .state()
            .court
            .iter()
            .map(|p| p.id.clone())
            .collect();
        let libero = scouting
            .state()
            .court
            .iter()
            .find(|p| p.libero)
            .map(|p| p.id.clone())
            .expect("the libero on court");
        let outside_hitter = ids[4].clone();
        let en = labels(LanguageEnum::En);
        let name = scouting
            .other_events()
            .into_iter()
            .find(|e| e.value == EventTypeEnum::LF)
            .map(|e| e.name);
        assert_eq!(name.as_deref(), Some(en.libero_for));
        assert_eq!(
            scouting.choose_event(EventTypeEnum::LF).unwrap(),
            Progress::Choosing
        );
        let choices: Vec<String> = scouting
            .player_choices()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert!(choices.contains(&outside_hitter));
        assert!(!choices.contains(&libero));
        assert_eq!(
            scouting.choose_player(outside_hitter.clone()).unwrap(),
            Progress::Saved
        );
        let on_court = |scouting: &Scouting| -> Vec<String> {
            scouting.state().court.into_iter().map(|p| p.id).collect()
        };
        assert!(!on_court(&scouting).contains(&libero), "out at once");
        assert!(on_court(&scouting).contains(&outside_hitter));
        match scouting.state().events.as_slice() {
            [first] => {
                assert_eq!(first.event, EventTypeEnum::LF);
                assert_eq!(
                    first.player.as_ref().map(|p| p.id.clone()),
                    Some(outside_hitter.clone())
                );
            }
            other => unreachable!("expected the libero for, got {other:?}"),
        }
        // our ace (the serve waits for its evaluation): the libero for the
        // outside hitter
        scouting.choose_eval(EvalEnum::Perfect).unwrap();
        assert!(on_court(&scouting).contains(&libero));
        assert!(!on_court(&scouting).contains(&outside_hitter));
    }

    #[test]
    fn the_players_spread_to_receive() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        // our serve error: they serve, we receive in P1
        scouting.choose_eval(EvalEnum::Error).unwrap();
        let state = scouting.state();
        assert_eq!(
            (state.serving_team, state.rotation.as_str()),
            (TeamSideEnum::Them, "S1")
        );
        let spots: Vec<(f32, f32)> = state.court.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(spots, RECEPTION[0].to_vec());
        assert!(state.court[0].setter);
        let receivers: Vec<usize> = (1..=6).filter(|p| state.court[p - 1].receives).collect();
        assert_eq!(receivers, RECEIVERS[0].to_vec());
    }

    /// The inverted P1 is kept from event to event, until changed.
    #[test]
    fn the_inverted_p1_stays_until_changed() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        let state = scouting.state();
        assert!(!state.inverted_p1);
        // we serve in P1: no difference, not shown
        assert!(!state.inverted_p1_applies);
        scouting.set_inverted_p1(true);
        scouting.choose_eval(EvalEnum::Error).unwrap();
        let state = scouting.state();
        assert!(state.inverted_p1);
        // they serve, P1 in side out: shown
        assert!(state.inverted_p1_applies);
        scouting.set_inverted_p1(false);
        assert!(!scouting.state().inverted_p1);
        // their error: we rotate and serve, not shown again
        scouting.choose_event(EventTypeEnum::OE).unwrap();
        assert!(!scouting.state().inverted_p1_applies);
    }

    /// The reception is told before their serve only, 3, 4, 2.
    #[test]
    fn the_reception_is_told_before_their_serve() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        assert!(!scouting.state().reception_open);
        assert!(scouting.next_reception().is_err());
        scouting.choose_eval(EvalEnum::Error).unwrap();
        let state = scouting.state();
        assert_eq!((state.reception, state.reception_open), (3, true));
        scouting.next_reception().unwrap();
        assert_eq!(scouting.state().reception, 4);
        scouting.next_reception().unwrap();
        assert_eq!(scouting.state().reception, 2);
    }

    #[test]
    fn nobody_receives_once_the_serve_is_received() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        // we serve: the players are in their zones
        assert!(scouting.state().court.iter().all(|p| !p.receives));
    }

    #[test]
    fn the_receivers_spread_across_the_court_in_front_of_the_others() {
        for (rotation, (spots, receivers)) in RECEPTION.iter().zip(RECEIVERS).enumerate() {
            let spot = |position: usize| spots[position - 1];
            let mut across: Vec<f32> = receivers.iter().map(|p| spot(*p).0).collect();
            across.sort_by(f32::total_cmp);
            // one on the left, one in the middle, one on the right
            assert!(
                across.windows(2).all(|w| w[1] - w[0] >= 2.5),
                "P{}: {across:?}",
                rotation + 1
            );
            // the outside hitters and the libero, never the setter
            let setter = rotation + 1;
            assert!(!receivers.contains(&setter), "P{}", rotation + 1);
            let hitters = [(rotation + 1) % 6 + 1, (rotation + 4) % 6 + 1];
            assert!(
                hitters.iter().all(|h| receivers.contains(h)),
                "P{}",
                rotation + 1
            );
        }
    }

    #[test]
    fn the_reception_formations_respect_the_order_of_rotation() {
        for (rotation, spots) in RECEPTION.iter().enumerate() {
            let x = |position: usize| spots[position - 1].0;
            let y = |position: usize| spots[position - 1].1;
            // each front row player before the one behind, and from the left
            // 4 3 2 and 5 6 1
            assert!(
                y(4) < y(5) && y(3) < y(6) && y(2) < y(1),
                "P{}: {spots:?}",
                rotation + 1
            );
            assert!(
                x(4) < x(3) && x(3) < x(2) && x(5) < x(6) && x(6) < x(1),
                "P{}: {spots:?}",
                rotation + 1
            );
            assert!(
                spots
                    .iter()
                    .all(|(x, y)| (0.0..=9.0).contains(x) && (0.0..=9.0).contains(y)),
                "P{}: {spots:?}",
                rotation + 1
            );
        }
    }

    #[test]
    fn the_set_that_wins_the_match_has_its_result() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let mut result = None;
        for set in 1..=3 {
            let scouting = started_set(&app, &team, &m);
            // aces until the set is over
            while scouting.choose_eval(EvalEnum::Perfect).unwrap() != Progress::SetOver {}
            result = scouting.match_result().unwrap();
            assert_eq!(result.is_some(), set == 3, "set {set}");
        }
        let point = |number| SetScore {
            number,
            us: 25,
            them: 0,
        };
        assert_eq!(
            result,
            Some(MatchResult {
                won: 3,
                lost: 0,
                sets: vec![point(1), point(2), point(3)],
            })
        );
    }

    /// A match of `team` against `opponent` on `day` of October 2026.
    fn match_on(app: &Scout4All, team: &str, opponent: &str, day: u8) -> String {
        app.save_match(
            team.into(),
            None,
            match_form(opponent, &format!("2026-10-{day:02}T18:00:00+02:00"), "8"),
        )
        .unwrap()
    }

    /// Plays the sets of the match, all aces, until it's over or `sets` are.
    fn play(app: &Scout4All, team: &str, m: &str, sets: u8) {
        for _ in 0..sets {
            let scouting = started_set(app, team, m);
            while scouting.choose_eval(EvalEnum::Perfect).unwrap() != Progress::SetOver {}
        }
    }

    #[test]
    fn the_second_set_proposes_the_lineup_of_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let (team, m) = match_with_a_set(&app);
        play(&app, &team, &m, 1);
        let lineup = app.set_start(team.clone(), m).unwrap().lineup();
        let numbers = |players: &[Option<LineupPlayer>]| -> Vec<Option<u8>> {
            players
                .iter()
                .map(|p| p.as_ref().map(|p| p.number))
                .collect()
        };
        assert_eq!(lineup.set_number, 2);
        assert_eq!(lineup.serving_team, Some(TeamSideEnum::Them));
        assert_eq!(
            numbers(&lineup.positions),
            [1, 2, 3, 4, 5, 6].map(Some).to_vec()
        );
        assert_eq!(
            numbers(&[lineup.setter, lineup.libero]),
            vec![Some(1), Some(7)]
        );
        assert!(lineup.complete);
    }

    #[test]
    fn the_home_has_the_matches_of_every_team_not_over_and_the_last_over() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        // Arezzo on the 24th, not started
        let (cortona, _) = match_with_a_set(&app);
        let siena = app.save_team(None, form("Siena")).unwrap();
        for n in 1..=7 {
            app.save_player(
                siena.clone(),
                None,
                player(&format!("S{n}"), &n.to_string()),
                false,
            )
            .unwrap();
        }
        // six over, on the 1st to the 6th
        for day in 1..=6 {
            let m = match_on(&app, &cortona, &format!("Over {day}"), day);
            play(&app, &cortona, &m, 3);
        }
        // a set over, one in progress, one not started
        let between = match_on(&app, &siena, "Between", 20);
        play(&app, &siena, &between, 1);
        let resume = match_on(&app, &cortona, "Resume", 21);
        started_set(&app, &cortona, &resume);
        let home = app.home().unwrap();
        let opponents = |matches: &[HomeMatch]| -> Vec<String> {
            matches.iter().map(|m| m.opponent.clone()).collect()
        };
        assert_eq!(
            opponents(&home.in_progress),
            ["Arezzo", "Resume", "Between"]
        );
        assert_eq!(
            home.in_progress.iter().map(|m| &m.next).collect::<Vec<_>>(),
            [
                &NextSetKind::Start { set_number: 1 },
                &NextSetKind::Resume { set_number: 1 },
                &NextSetKind::Start { set_number: 2 },
            ]
        );
        assert_eq!(home.in_progress[2].team_name, "Siena");
        // the five most recent over, the oldest left out
        assert_eq!(
            opponents(&home.recent),
            ["Over 6", "Over 5", "Over 4", "Over 3", "Over 2"]
        );
        assert_eq!((home.recent[0].won, home.recent[0].lost), (3, 0));
        assert_eq!(home.recent[0].team_id, cortona);
        assert_eq!(home.teams.len(), 2);
    }

    #[test]
    fn the_matches_of_the_home_at_the_same_time_are_by_team() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        // more teams than the recent matches, each a match over on the 1st
        let mut teams = vec![];
        for t in 1..=RECENT_MATCHES + 1 {
            let team = app.save_team(None, form(&format!("Team {t}"))).unwrap();
            for n in 1..=7 {
                app.save_player(
                    team.clone(),
                    None,
                    player(&format!("P{n}"), &n.to_string()),
                    false,
                )
                .unwrap();
            }
            let m = match_on(&app, &team, "Same day", 1);
            play(&app, &team, &m, 3);
            teams.push(team);
        }
        teams.sort();
        let recent: Vec<String> = app
            .home()
            .unwrap()
            .recent
            .into_iter()
            .map(|m| m.team_id)
            .collect();
        assert_eq!(recent, teams[..RECENT_MATCHES]);
    }

    #[test]
    fn without_teams_the_home_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        assert_eq!(
            app.home().unwrap(),
            Home {
                in_progress: vec![],
                recent: vec![],
                teams: vec![],
            }
        );
    }

    #[test]
    fn the_point_that_wins_the_set_ends_it() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        // we serve: the serve waits for its evaluation
        let ace = || scouting.choose_eval(EvalEnum::Perfect).unwrap();
        for _ in 1..25 {
            assert_eq!(ace(), Progress::Saved);
        }
        assert_eq!(ace(), Progress::SetOver);
        assert_eq!(scouting.state().won, 1);

        let details = app.match_details(team.clone(), m.clone()).unwrap();
        assert_eq!(details.next, NextSetKind::Start { set_number: 2 });
        assert_eq!(
            details.sets,
            vec![SetScore {
                number: 1,
                us: 25,
                them: 0
            }]
        );
        assert!(app.scouting(team, m).is_err());
    }

    /// The statistics of each set and of the match, with their trend, as
    /// the sets are scouted.
    #[test]
    fn the_statistics_of_a_set_and_of_the_match() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        // set 1: 25 aces
        let scouting = started_set(&app, &team, &m);
        for _ in 0..25 {
            scouting.choose_eval(EvalEnum::Perfect).unwrap();
        }
        // set 2: a serve error, their error, a serve error
        let scouting = started_set(&app, &team, &m);
        assert_eq!(scouting.state().set_number, 2);
        scouting.choose_eval(EvalEnum::Error).unwrap();
        scouting.choose_event(EventTypeEnum::OE).unwrap();
        scouting.choose_eval(EvalEnum::Error).unwrap();

        let serve = |stats: &LiveStats| {
            stats
                .fundamentals
                .iter()
                .find(|f| f.event == EventTypeEnum::S)
                .map(|f| (f.name.clone(), f.total, f.efficiency, f.perfect, f.errors))
                .unwrap()
        };
        let phase = |p: &Option<PhaseStats>| p.as_ref().map(|p| (p.won, p.rallies));
        let line = |stats: &LiveStats, series: Series| {
            stats
                .trend
                .iter()
                .find(|t| t.series == series)
                .cloned()
                .unwrap()
        };

        let set = scouting
            .stats(StatsScope::Set { number: 2 }, DetailFilter::default())
            .unwrap();
        assert_eq!(set.scope, StatsScope::Set { number: 2 });
        assert_eq!(set.sets, vec![1, 2]);
        assert_eq!(
            set.fundamentals.iter().map(|f| f.event).collect::<Vec<_>>(),
            vec![
                EventTypeEnum::P,
                EventTypeEnum::A,
                EventTypeEnum::S,
                EventTypeEnum::B,
                EventTypeEnum::D
            ]
        );
        assert_eq!(serve(&set), ("battuta".into(), 2, Some(-100.0), 0, 2));
        assert_eq!((set.aces, set.opponent_errors), (0, 1));
        assert_eq!(set.unforced_errors, 2);
        assert_eq!(
            set.unforced_by_event,
            vec![
                ErrorCount {
                    event: EventTypeEnum::S,
                    count: 2
                },
                ErrorCount {
                    event: EventTypeEnum::A,
                    count: 0
                },
                ErrorCount {
                    event: EventTypeEnum::B,
                    count: 0
                },
                ErrorCount {
                    event: EventTypeEnum::F,
                    count: 0
                },
            ]
        );
        assert_eq!(set.forced_errors, 0);
        assert_eq!(phase(&set.side_out), Some((1, 1)));
        // the side out on their serve error: no possession of ours
        assert_eq!(
            set.side_out_possessions,
            ["1°", "2°", "3°+"].map(|name| PossessionStats {
                name: name.into(),
                total: 0,
                won: 0,
                percentage: None,
            })
        );
        assert_eq!(phase(&set.break_points), Some((0, 2)));
        assert!(set.distribution.is_empty());
        // a regional women's team: graded
        assert!(set.graded);
        let serve_grade = |stats: &LiveStats| {
            stats
                .fundamentals
                .iter()
                .find(|f| f.event == EventTypeEnum::S)
                .and_then(|f| f.grade)
        };
        assert_eq!(serve_grade(&set), Some(0.0));
        assert_eq!(set.side_out.as_ref().and_then(|p| p.grade), Some(1.0));
        assert_eq!(
            set.fundamentals
                .iter()
                .find(|f| f.event == EventTypeEnum::D)
                .map(|f| f.grade),
            Some(None)
        );

        // the trend: every set so far, each from its first rally
        assert_eq!(
            set.trend.iter().map(|t| t.series).collect::<Vec<_>>(),
            Series::ALL.to_vec()
        );
        let serves = line(&set, Series::Serve);
        assert_eq!(serves.name, "battuta");
        assert_eq!(
            serves.band,
            Some(GradeBand {
                low: 1.0,
                high: 25.0
            })
        );
        assert_eq!(
            serves.sets.iter().map(|s| s.set_number).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(serves.sets[0].values, vec![Some(100.0); 25]);
        assert_eq!(serves.sets[0].recent, vec![Some(100.0); 25]);
        assert_eq!(serves.sets[1].values, vec![Some(-100.0); 3]);
        // 0-1, 1-1, 1-2
        assert_eq!(serves.sets[1].progress, vec![0.5, 1.0, 2.0]);
        assert_eq!(
            serves.sets[0].progress,
            (1..=25).map(f64::from).collect::<Vec<_>>()
        );
        assert_eq!(serves.sets[1].recent, vec![Some(-100.0); 3]);
        // the errors of the serve rally by rally: the two of the second
        // set, unforced; none in the first
        assert_eq!(serves.sets[1].errors, vec![1, 0, 1]);
        assert_eq!(serves.sets[1].unforced, vec![1, 0, 1]);
        assert_eq!(serves.sets[0].errors, vec![0; 25]);
        // the points of each set, as on the score sheet
        let scored = |us, score| PointScored { us, score };
        assert_eq!(
            set.points.iter().map(|p| p.set_number).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            set.points[0].points,
            (1..=25).map(|n| scored(true, n)).collect::<Vec<_>>()
        );
        assert_eq!(
            set.points[1].points,
            vec![scored(false, 1), scored(true, 1), scored(false, 2)]
        );
        // the match: (25 aces - 2 errors) / 27 serves; the last 10: 8 aces
        // and 2 errors
        assert_eq!(serves.overall.map(|s| (s * 100.0).round()), Some(8519.0));
        assert_eq!(serves.overall_recent, Some(60.0));
        assert_eq!(
            line(&set, Series::SideOut).sets[1].values,
            vec![None, Some(100.0), Some(100.0)]
        );
        assert_eq!(line(&set, Series::SideOut).sets[0].values, vec![None; 25]);

        let first = scouting
            .stats(StatsScope::Set { number: 1 }, DetailFilter::default())
            .unwrap();
        assert_eq!(serve(&first), ("battuta".into(), 25, Some(100.0), 25, 0));
        assert_eq!(first.aces, 25);
        assert_eq!(phase(&first.break_points), Some((25, 25)));
        assert_eq!(first.side_out, None);
        assert_eq!(serve_grade(&first), Some(1.0));
        // the trend is the same whatever the scope
        assert_eq!(first.trend, set.trend);
        assert_eq!(first.points, set.points);

        // the match: the trend goes on across the sets
        let all = scouting
            .stats(StatsScope::Match, DetailFilter::default())
            .unwrap();
        assert_eq!(serve(&all).1, 27);
        assert_eq!(serve(&all).3, 25);
        assert_eq!(all.aces, 25);
        assert_eq!(phase(&all.break_points), Some((25, 27)));
        assert_eq!(all.trend, set.trend);

        // a set not played yet: nothing
        let none = scouting
            .stats(StatsScope::Set { number: 3 }, DetailFilter::default())
            .unwrap();
        assert!(
            none.fundamentals
                .iter()
                .all(|f| f.total == 0 && f.efficiency.is_none())
        );
        assert_eq!(none.trend, set.trend);
        assert_eq!(none.sets, vec![1, 2]);

        // from the page of the match, the same as while scouting
        let saved = |scope| {
            app.match_stats(
                team.clone(),
                m.clone(),
                MatchSide::A,
                scope,
                DetailFilter::default(),
            )
            .unwrap()
        };
        assert_eq!(saved(StatsScope::Set { number: 2 }), set);
        assert_eq!(saved(StatsScope::Set { number: 1 }), first);
        assert_eq!(saved(StatsScope::Match), all);
        assert_eq!(saved(StatsScope::Set { number: 3 }), none);
    }

    /// The serves by zone, of the ones whose zone was told: their court a
    /// row each, the nine zones, how many to each zone and the efficiency
    /// from three on; the receptions with the conflict zones too.
    #[test]
    fn the_serves_by_zone() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        // three serves to cells of 1, one without a zone
        for cell in [Some("A"), Some("C"), None, Some("A")] {
            if let Some(subzone) = cell {
                scouting.touch_target(1, subzone.into()).unwrap();
            }
            scouting.choose_eval(EvalEnum::Perfect).unwrap();
        }
        let serve = scouting
            .detail(StatsScope::Match, EventTypeEnum::S, DetailFilter::default())
            .unwrap()
            .fundamental
            .unwrap();
        assert_eq!((serve.total, serve.targets_told), (4, 3));
        assert_eq!(
            serve
                .targets
                .iter()
                .map(|row| row.len())
                .collect::<Vec<_>>(),
            vec![3, 3, 3]
        );
        assert_eq!(serve.on_conflicts, None);
        let z1 = serve
            .targets
            .iter()
            .flatten()
            .find(|t| t.label == "1")
            .unwrap();
        assert_eq!(
            (z1.count, z1.share, z1.efficiency),
            (3.0, 100.0, Some(100.0))
        );
        assert!(
            serve
                .targets
                .iter()
                .flatten()
                .filter(|t| t.label != "1")
                .all(|t| t.count == 0.0 && t.efficiency.is_none())
        );
        // none for another fundamental
        let reception = scouting
            .detail(StatsScope::Match, EventTypeEnum::P, DetailFilter::default())
            .unwrap()
            .fundamental
            .unwrap();
        assert!(reception.targets.is_empty());
        // a reception in 6A: our court, 4 at the net on the left
        scouting.touch_target(1, "A".into()).unwrap();
        scouting.choose_eval(EvalEnum::Error).unwrap();
        scouting.choose_event(EventTypeEnum::P).unwrap();
        let receiver = scouting.player_choices().into_iter().next().unwrap().id;
        scouting.choose_player(receiver).unwrap();
        scouting.touch_target(6, "A".into()).unwrap();
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        let reception = scouting
            .detail(StatsScope::Match, EventTypeEnum::P, DetailFilter::default())
            .unwrap()
            .fundamental
            .unwrap();
        assert_eq!((reception.total, reception.targets_told), (1, 1));
        let first: Vec<&str> = reception.targets[0]
            .iter()
            .map(|t| t.label.as_str())
            .collect();
        assert_eq!(first, vec!["4", "4/3", "3", "3/2", "2"]);
        let z6 = reception
            .targets
            .iter()
            .flatten()
            .find(|t| t.label == "6")
            .unwrap();
        assert_eq!((z6.count, z6.share), (1.0, 100.0));
        assert_eq!(reception.targets.len(), 3);
        assert_eq!(
            reception.on_conflicts,
            Some(ConflictStats {
                count: 0,
                share: 0.0,
                efficiency: None
            })
        );
    }

    /// Our attack by the zone it went to is shown as our serve: their
    /// court as our server sees it, 1 6 5 / 9 8 7 / 2 3 4.
    #[test]
    fn the_attacks_by_zone_as_the_serves() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        // our serve in the net, their serve received, then our attack to 9
        scouting.choose_eval(EvalEnum::Error).unwrap();
        scouting.choose_event(EventTypeEnum::P).unwrap();
        let player = scouting.player_choices().into_iter().next().unwrap().id;
        scouting.choose_player(player).unwrap();
        scouting.touch_target(6, "A".into()).unwrap();
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        scouting.choose_event(EventTypeEnum::A).unwrap();
        let attacker = scouting.player_choices().into_iter().next().unwrap().id;
        scouting.choose_player(attacker).unwrap();
        scouting.touch_target(9, "A".into()).unwrap();
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        let attack = scouting
            .detail(StatsScope::Match, EventTypeEnum::A, DetailFilter::default())
            .unwrap()
            .fundamental
            .unwrap();
        let rows: Vec<Vec<&str>> = attack
            .targets
            .iter()
            .map(|row| row.iter().map(|t| t.label.as_str()).collect())
            .collect();
        assert_eq!(
            rows,
            vec![
                vec!["1", "6", "5"],
                vec!["9", "8", "7"],
                vec!["2", "3", "4"]
            ]
        );
        let z9 = attack
            .targets
            .iter()
            .flatten()
            .find(|t| t.label == "9")
            .unwrap();
        assert_eq!((z9.count, z9.share), (1.0, 100.0));
    }

    /// Where our serve starts from: the five zones from the left as our
    /// server sees them, along with the evaluation of our serve only;
    /// remembered for the server; the serves by zone they started from,
    /// where they went only from the zone asked, and their clips.
    #[test]
    fn the_serves_by_where_they_started_from() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let scouting = started_set(&app, &team, &m);
        let labels = |s: &Scouting| {
            s.serve_starts()
                .iter()
                .map(|c| (c.label.clone(), c.chosen))
                .collect::<Vec<_>>()
        };
        let none: Vec<(String, bool)> = ["5", "7", "6", "9", "1"]
            .iter()
            .map(|l| (l.to_string(), false))
            .collect();
        assert_eq!(labels(&scouting), none);
        // from 1 to 1, from 1 to 5, then from 5 without a target
        scouting.touch_serve_start(ServeStartEnum::Z1).unwrap();
        scouting.touch_target(1, "A".into()).unwrap();
        scouting.choose_eval(EvalEnum::Perfect).unwrap();
        // remembered for the same server
        assert_eq!(
            scouting
                .serve_starts()
                .iter()
                .find(|c| c.chosen)
                .map(|c| c.value),
            Some(ServeStartEnum::Z1)
        );
        scouting.touch_target(5, "A".into()).unwrap();
        scouting.choose_eval(EvalEnum::Perfect).unwrap();
        scouting.touch_serve_start(ServeStartEnum::Z5).unwrap();
        scouting.choose_eval(EvalEnum::Perfect).unwrap();
        let detail = |serve_start| {
            scouting
                .detail(
                    StatsScope::Match,
                    EventTypeEnum::S,
                    DetailFilter {
                        serve_start,
                        ..DetailFilter::default()
                    },
                )
                .unwrap()
                .fundamental
                .unwrap()
        };
        let all = detail(None);
        assert_eq!(all.serve_starts_told, 3);
        let shares: Vec<(&str, u32)> = all
            .serve_starts
            .iter()
            .map(|z| (z.label.as_str(), z.count))
            .collect();
        assert_eq!(
            shares,
            vec![("5", 1), ("7", 0), ("6", 0), ("9", 0), ("1", 2)]
        );
        assert_eq!(all.targets_told, 2);
        // the paths on the whole court: from the right fifth of our end
        // line to 1 (their end line, on the left) and to 5 (on the right)
        // each end moved towards the other: off the middle
        let cm = |m: f64| (m * 100.0).round() as i64;
        let paths: Vec<(String, i64, i64, i64, i64)> = all
            .serve_paths
            .iter()
            .map(|p| {
                (
                    p.label.clone(),
                    cm(p.from_x),
                    cm(p.from_y),
                    cm(p.to_x),
                    cm(p.to_y),
                )
            })
            .collect();
        let mut sorted = paths.clone();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            sorted,
            vec![
                ("1 → 1".to_string(), 760, 1800, 200, 150),
                ("1 → 5".to_string(), 860, 1800, 800, 150),
            ]
        );
        // no errors yet
        assert_eq!(all.serve_starts.iter().map(|z| z.errors).sum::<u32>(), 0);
        let from_1 = detail(Some(ServeStartEnum::Z1));
        assert_eq!((from_1.total, from_1.targets_told), (3, 2));
        assert_eq!(from_1.serve_starts, all.serve_starts);
        let from_5 = detail(Some(ServeStartEnum::Z5));
        assert!(from_5.targets.is_empty() && from_5.serve_paths.is_empty());
        assert_eq!(from_1.serve_paths.len(), 2);
        // the event tells it, the last first
        let state = scouting.state();
        assert_eq!(
            state
                .events
                .iter()
                .map(|e| e.serve_start.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("5"), Some("1"), Some("1")]
        );
        // not for their serve: our reception
        scouting.choose_eval(EvalEnum::Error).unwrap();
        scouting.choose_event(EventTypeEnum::P).unwrap();
        let receiver = scouting.player_choices().into_iter().next().unwrap().id;
        scouting.choose_player(receiver).unwrap();
        assert!(scouting.serve_starts().is_empty());
        assert!(scouting.touch_serve_start(ServeStartEnum::Z1).is_err());
        // the clips from 1, to 1 from 1
        let pick = |serve_start, target| ClipPick {
            scope: StatsScope::Match,
            event: EventTypeEnum::S,
            filter: DetailFilter {
                serve_start,
                ..DetailFilter::default()
            },
            evals: vec![],
            zone: None,
            target,
        };
        let clips = |p| app.match_clips(&team, &m, &p).unwrap().len();
        assert_eq!(clips(pick(None, None)), 4);
        assert_eq!(clips(pick(Some(ServeStartEnum::Z1), None)), 2);
        assert_eq!(
            clips(pick(Some(ServeStartEnum::Z1), Some(CourtZoneEnum::Z5))),
            1
        );
        assert_eq!(
            clips(pick(Some(ServeStartEnum::Z5), Some(CourtZoneEnum::Z5))),
            0
        );
    }

    #[test]
    fn the_detail_of_a_fundamental() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        // set 1: 25 aces of the player in 1
        let scouting = started_set(&app, &team, &m);
        for _ in 0..25 {
            scouting.choose_eval(EvalEnum::Perfect).unwrap();
        }
        // set 2: a serve error, their error (we rotate), a serve error of
        // the next server
        let scouting = started_set(&app, &team, &m);
        scouting.choose_eval(EvalEnum::Error).unwrap();
        scouting.choose_event(EventTypeEnum::OE).unwrap();
        scouting.choose_eval(EvalEnum::Error).unwrap();
        let all = DetailFilter::default();
        let detail = |scope, event, filter: &DetailFilter| {
            scouting.detail(scope, event, filter.clone()).unwrap()
        };

        let serve = detail(StatsScope::Match, EventTypeEnum::S, &all);
        assert_eq!(
            serve
                .fundamentals
                .iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>(),
            vec![
                "serve",
                "reception",
                "defense",
                "block",
                "attack",
                "freeball"
            ]
        );
        assert_eq!(serve.sets, vec![1, 2]);
        assert_eq!(serve.freeballs, None);
        let f = serve.fundamental.clone().unwrap();
        assert_eq!(f.total, 27);
        assert_eq!(
            f.evals
                .iter()
                .map(|e| (e.symbol.as_str(), e.tone, e.count))
                .collect::<Vec<_>>(),
            vec![("#", Tone::Excellent, 25), ("=", Tone::Bad, 2)]
        );
        assert_eq!(f.efficiency, Some(23.0 / 27.0 * 100.0));
        assert_eq!((f.errors, f.unforced_errors, f.points), (2, 2, Some(25)));
        // 28 rallies, 26 won
        assert_eq!(
            f.phases_per_point.map(|p| (p.won, p.rallies)),
            Some((26, 28))
        );
        // the two servers, the one of the aces first
        let servers: Vec<(u8, u32, Option<f64>)> = f
            .players
            .iter()
            .map(|p| (p.player.number, p.total, p.efficiency))
            .collect();
        assert_eq!(
            servers,
            vec![(1, 26, Some(24.0 / 26.0 * 100.0)), (2, 1, Some(-100.0))]
        );
        assert_eq!(
            serve.players.iter().map(|p| p.number).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(serve.rotations.len(), 6);
        assert_eq!(serve.rotations[0].name, "S1");
        assert_eq!(serve.phases.len(), 2);

        // a player: the rallies have none
        let second = DetailFilter {
            players: vec![f.players[1].player.id.clone()],
            ..DetailFilter::default()
        };
        let theirs = detail(StatsScope::Match, EventTypeEnum::S, &second)
            .fundamental
            .unwrap();
        assert_eq!((theirs.total, theirs.phases_per_point), (1, None));
        // two players: the serves of both
        let both = DetailFilter {
            players: vec![
                f.players[0].player.id.clone(),
                f.players[1].player.id.clone(),
            ],
            ..DetailFilter::default()
        };
        let both = detail(StatsScope::Match, EventTypeEnum::S, &both)
            .fundamental
            .unwrap();
        assert_eq!((both.total, both.phases_per_point), (27, None));
        // to choose, only who has the fundamental: no reception, nobody;
        // the one chosen stays, to take the filter off
        let reception = detail(StatsScope::Match, EventTypeEnum::P, &all);
        assert!(reception.players.is_empty());
        let chosen = detail(StatsScope::Match, EventTypeEnum::P, &second);
        assert_eq!(
            chosen.players.iter().map(|p| p.number).collect::<Vec<_>>(),
            vec![2]
        );
        // a set
        let set = detail(StatsScope::Set { number: 2 }, EventTypeEnum::S, &all);
        assert_eq!(set.fundamental.unwrap().total, 2);
        // no serve in side out
        let side_out = DetailFilter {
            phase: Some(PhaseEnum::SideOut),
            ..DetailFilter::default()
        };
        let none = detail(StatsScope::Match, EventTypeEnum::S, &side_out)
            .fundamental
            .unwrap();
        assert_eq!((none.total, none.efficiency), (0, None));
        assert!(none.evals.is_empty() && none.players.is_empty());
        // the first rotation: the aces and the first error
        let first = DetailFilter {
            rotations: vec![0],
            ..DetailFilter::default()
        };
        assert_eq!(
            detail(StatsScope::Match, EventTypeEnum::S, &first)
                .fundamental
                .unwrap()
                .total,
            26
        );
        // two rotations: the serves of both
        let in_rotations = |rotations: Vec<u8>| {
            let filter = DetailFilter {
                rotations,
                ..DetailFilter::default()
            };
            detail(StatsScope::Match, EventTypeEnum::S, &filter)
                .fundamental
                .map_or(0, |f| f.total)
        };
        for other in 1..6 {
            assert_eq!(
                in_rotations(vec![0, other]),
                in_rotations(vec![0]) + in_rotations(vec![other])
            );
        }
        assert_eq!((1..6).map(|r| in_rotations(vec![r])).sum::<u32>(), 1);

        let freeballs = detail(StatsScope::Match, EventTypeEnum::FB, &all);
        assert_eq!(freeballs.fundamental, None);
        assert_eq!(
            freeballs.freeballs,
            Some(FreeballStats {
                given: 0,
                received: Some(0),
                exploited: None,
            })
        );
        assert!(
            app.match_detail(
                team.clone(),
                m.clone(),
                MatchSide::A,
                StatsScope::Match,
                EventTypeEnum::S,
                DetailFilter {
                    players: vec!["not an id".into()],
                    ..DetailFilter::default()
                },
            )
            .is_err()
        );

        // from the page of the match, the same as while scouting
        assert_eq!(
            app.match_detail(
                team.clone(),
                m.clone(),
                MatchSide::A,
                StatsScope::Match,
                EventTypeEnum::S,
                all
            )
            .unwrap(),
            serve
        );

        // the phases of each rotation: the 25 aces and the first error in
        // break in the first, their error in side out; the next serve
        // error in break in the rotation after
        let rotations = scouting
            .stats(StatsScope::Match, DetailFilter::default())
            .unwrap()
            .rotations;
        let won = |p: &Option<PhaseStats>| p.as_ref().map(|p| (p.won, p.rallies));
        let played: Vec<_> = rotations
            .iter()
            .filter(|r| r.side_out.is_some() || r.break_points.is_some())
            .map(|r| (r.name.as_str(), won(&r.side_out), won(&r.break_points)))
            .collect();
        assert_eq!(played.len(), 2);
        assert_eq!(played[0], ("S1", Some((1, 1)), Some((25, 26))));
        assert_eq!((played[1].1, played[1].2), (None, Some((0, 1))));
        // all the rallies of each rotation, and the points won minus lost
        let balance = |p: &Option<PhaseStats>| p.as_ref().map(|p| p.balance);
        let totals: Vec<_> = rotations
            .iter()
            .filter(|r| r.total.is_some())
            .map(|r| {
                (
                    won(&r.total),
                    balance(&r.total),
                    balance(&r.side_out),
                    balance(&r.break_points),
                )
            })
            .collect();
        assert_eq!(
            totals,
            vec![
                (Some((26, 27)), Some(25), Some(1), Some(24)),
                (Some((0, 1)), Some(-1), None, Some(-1)),
            ]
        );
        assert!(
            rotations
                .iter()
                .all(|r| r.total.as_ref().is_none_or(|t| t.grade.is_none()))
        );

        // the summary narrowed by the phase and the rotation agrees with
        // the detail of the fundamental, and says what it is narrowed to
        let serves = |live: &LiveStats| {
            live.fundamentals
                .iter()
                .find(|f| f.event == EventTypeEnum::S)
                .map(|f| f.total)
        };
        let filters = [
            DetailFilter {
                phase: Some(PhaseEnum::Break),
                ..DetailFilter::default()
            },
            DetailFilter {
                phase: Some(PhaseEnum::SideOut),
                ..DetailFilter::default()
            },
            DetailFilter {
                rotations: vec![0],
                ..DetailFilter::default()
            },
            DetailFilter {
                rotations: vec![1],
                phase: Some(PhaseEnum::Break),
                ..DetailFilter::default()
            },
        ];
        let everything = scouting
            .stats(StatsScope::Match, DetailFilter::default())
            .unwrap();
        assert_eq!(everything.filter, DetailFilter::default());
        assert_eq!(
            (
                everything.rotation_choices.len(),
                everything.phase_choices.len()
            ),
            (6, 2)
        );
        for filter in filters.clone() {
            let live = scouting.stats(StatsScope::Match, filter.clone()).unwrap();
            let detail = scouting
                .detail(StatsScope::Match, EventTypeEnum::S, filter.clone())
                .unwrap();
            assert_eq!(live.filter, filter);
            assert_eq!(
                serves(&live),
                detail.fundamental.as_ref().map(|f| f.total),
                "{filter:?}"
            );
        }
        let side_out = scouting
            .stats(StatsScope::Match, filters[1].clone())
            .unwrap();
        assert!(side_out.break_points.is_none() && side_out.side_out.is_some());
        let first = scouting
            .stats(StatsScope::Match, filters[2].clone())
            .unwrap();
        // the table of the rotations stays the one of every rotation
        assert_eq!(first.rotations, everything.rotations);
        assert!(serves(&first) < serves(&everything));
    }

    #[test]
    fn the_events_of_a_set_are_corrected() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        // two aces, a serve error, their error
        let scouting = started_set(&app, &team, &m);
        scouting.choose_eval(EvalEnum::Perfect).unwrap();
        scouting.choose_eval(EvalEnum::Perfect).unwrap();
        scouting.choose_eval(EvalEnum::Error).unwrap();
        scouting.choose_event(EventTypeEnum::OE).unwrap();

        let events = app.set_events(team.clone(), m.clone(), 1).unwrap();
        assert_eq!((events.set_number, events.opponent.as_str()), (1, "Arezzo"));
        let listed: Vec<(u32, EventTypeEnum, Option<String>, u8, u8, bool)> = events
            .events
            .iter()
            .map(|e| {
                (
                    e.index,
                    e.event.event,
                    e.event.symbol.clone(),
                    e.score_us,
                    e.score_them,
                    e.editable,
                )
            })
            .collect();
        let symbol = |s: &str| Some(s.to_string());
        assert_eq!(
            listed,
            vec![
                (0, EventTypeEnum::S, symbol("#"), 1, 0, true),
                (1, EventTypeEnum::S, symbol("#"), 2, 0, true),
                (2, EventTypeEnum::S, symbol("="), 2, 1, true),
                (3, EventTypeEnum::OE, None, 3, 1, true),
            ]
        );
        assert!(app.set_events(team.clone(), m.clone(), 2).is_err());

        // the second ace, at 1-0
        let correction = app.correction(team.clone(), m.clone(), 1, 1).unwrap();
        let state = correction.state();
        assert_eq!((state.index, state.score_us, state.score_them), (1, 1, 0));
        assert_eq!(
            state.fields,
            vec![EditField::EventType, EditField::Eval, EditField::Technique]
        );
        assert_eq!(state.eval, Some(EvalEnum::Perfect));
        assert_eq!(state.evals.len(), 6);
        // the serve type the scouting preselected
        let chosen: Vec<String> = state
            .techniques
            .iter()
            .filter(|t| t.chosen)
            .map(|t| t.code.clone())
            .collect();
        assert_eq!(chosen.len(), 1);
        assert_eq!(state.event.technique.as_ref(), chosen.first());
        assert!(
            state
                .event_types
                .iter()
                .any(|e| e.value == EventTypeEnum::S)
        );
        // a serve in play: the serve after it doesn't fit
        correction.choose_eval(EvalEnum::Positive);
        assert_eq!(correction.state().eval, Some(EvalEnum::Positive));
        let error = app.save_correction(correction.clone()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "the change doesn't fit the events after it"
        );
        // still an ace, the other serve type: saved
        let float = state
            .techniques
            .iter()
            .find(|t| !t.chosen)
            .map(|t| t.value)
            .unwrap();
        correction.choose_eval(EvalEnum::Perfect);
        correction.choose_technique(Some(float));
        assert!(
            correction
                .state()
                .techniques
                .iter()
                .any(|t| t.chosen && t.value == float)
        );
        app.save_correction(correction).unwrap();
        let saved = app.set_events(team.clone(), m.clone(), 1).unwrap();
        assert_eq!(
            saved.events[1].event.technique.as_deref(),
            Some(float.to_string().as_str())
        );
        assert_eq!(saved.events.len(), 4);

        assert!(app.correction(team.clone(), m.clone(), 1, 4).is_err());
        assert!(app.correction(team, m, 2, 0).is_err());
    }

    /// A set scouted on both sides: its touches listed with their side and
    /// the score from A's, a touch of B corrected with B's team and saved
    /// on both sides (B's statistics read it).
    #[test]
    fn the_touches_of_a_set_on_both_sides_are_corrected() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let choice = DvwChoice {
            opponent: DvwOpponent::NewTeam,
            ..dvw_choice(DvwSide::Home, None)
        };
        let imported = app.import_dvw(THREE_SETS.to_vec(), choice).unwrap();
        let (team, m) = (imported.team_id, imported.match_id);
        let events = app.set_events(team.clone(), m.clone(), 1).unwrap();
        assert!(events.both);
        assert!(events.events.iter().any(|e| e.event.side_b));
        assert!(events.events.iter().any(|e| !e.event.side_b));
        let last = events.events.last().unwrap();
        assert_eq!(last.score_us.max(last.score_them), 25);
        // a serve of B
        let serve = events
            .events
            .iter()
            .find(|e| e.event.side_b && e.event.event == EventTypeEnum::S)
            .unwrap();
        let correction = app
            .correction(team.clone(), m.clone(), 1, serve.index)
            .unwrap();
        let state = correction.state();
        assert_eq!(state.index, serve.index);
        assert!(state.event.side_b);
        assert!(state.event_types.iter().all(|e| e.side_b));
        let other = state
            .techniques
            .iter()
            .find(|t| !t.chosen)
            .map(|t| t.value)
            .unwrap();
        correction.choose_technique(Some(other));
        app.save_correction(correction).unwrap();
        let saved = app.set_events(team.clone(), m.clone(), 1).unwrap();
        assert!(saved.both);
        assert_eq!(saved.events.len(), events.events.len());
        let index = usize::try_from(serve.index).unwrap();
        assert_eq!(
            saved.events[index].event.technique.as_deref(),
            Some(other.to_string().as_str())
        );
        assert!(
            app.match_stats(
                team,
                m,
                MatchSide::B,
                StatsScope::Match,
                DetailFilter::default()
            )
            .is_ok()
        );
    }

    #[test]
    fn the_statistics_of_a_match_without_sets_are_empty() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let stats = app
            .match_stats(
                team,
                m,
                MatchSide::A,
                StatsScope::Match,
                DetailFilter::default(),
            )
            .unwrap();
        assert!(stats.sets.is_empty());
        assert!(stats.fundamentals.iter().all(|f| f.total == 0));
        assert!(stats.trend.iter().all(|t| t.sets.is_empty()));
    }

    /// Damaged files: what the user reads says so, in the language of the
    /// app; the detail goes to the log.
    #[test]
    fn a_damaged_team_or_match_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        let (team, m) = match_with_a_set(&app);
        let started = started_set(&app, &team, &m);
        drop(started);
        let reason = |e: MobileError| match e {
            MobileError::Failed { reason } => reason,
            other => format!("{other:?}"),
        };
        let it = labels(LanguageEnum::It);
        // the set without our lineup
        sql(
            &dir,
            &format!("DELETE FROM set_lineups WHERE match_id = '{m}'"),
        );
        assert_eq!(
            reason(app.scouting(team.clone(), m.clone()).err().unwrap()),
            it.unreadable_match
        );
        assert_eq!(
            reason(app.match_details(team.clone(), m.clone()).unwrap_err()),
            it.unreadable_match
        );
        // the schema doesn't check the spelling of the country, reading does
        sql(
            &dir,
            &format!("UPDATE teams SET country = 'it' WHERE id = '{team}'"),
        );
        assert_eq!(
            reason(app.team(team.clone()).unwrap_err()),
            it.unreadable_team
        );
        assert_eq!(
            reason(app.match_details(team, m).unwrap_err()),
            it.unreadable_team
        );
    }

    #[test]
    fn a_number_in_use_names_its_field() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        app.save_player(team.clone(), None, player("Anna", "9"), false)
            .unwrap();
        match app.save_player(team, None, player("Bea", "9"), false) {
            Err(MobileError::InvalidPlayer { field, reason }) => {
                assert_eq!(field, PlayerField::Number);
                assert_eq!(reason, "number already in use");
            }
            other => unreachable!("expected a number in use, got {other:?}"),
        }
    }

    #[test]
    fn a_namesake_is_saved_once_confirmed() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        app.save_player(team.clone(), None, player("Anna Rossi", "9"), false)
            .unwrap();
        match app.save_player(team.clone(), None, player("anna rossi", "10"), false) {
            Err(MobileError::Namesake { reason }) => {
                assert_eq!(reason, "Anna Rossi, number 9, is already in the team.");
            }
            other => unreachable!("expected a namesake, got {other:?}"),
        }
        assert_eq!(app.team(team.clone()).unwrap().players.len(), 1);
        // an error of the form comes first: the namesake once it's valid
        assert!(matches!(
            app.save_player(team.clone(), None, player("Anna Rossi", "9"), false),
            Err(MobileError::InvalidPlayer { .. })
        ));
        app.save_player(team.clone(), None, player("anna rossi", "10"), true)
            .unwrap();
        assert_eq!(app.team(team).unwrap().players.len(), 2);
    }

    /// The fixtures of the core: archives as exported up to v0.9.0.
    const OLD_TEAM: &[u8] =
        include_bytes!("../../core/src/tests/fixtures/45a589d5-004a-4a50-99f5-6d78fd052a3c.zip");
    const OLD_TEAM_FILE: &str = "45a589d5-004a-4a50-99f5-6d78fd052a3c.zip";
    const OLD_MATCH: &[u8] =
        include_bytes!("../../core/src/tests/fixtures/2025-09-20_club_arezzo.zip");

    fn reason<T: std::fmt::Debug>(result: Result<T, MobileError>) -> String {
        match result {
            Err(MobileError::Failed { reason }) => reason,
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[test]
    fn a_team_and_a_match_move_to_another_phone() {
        let from_dir = tempfile::tempdir().unwrap();
        let from = app(&from_dir, "it");
        let (team, m) = match_with_a_set(&from);
        started_set(&from, &team, &m);
        let team_zip = from.export_team(team.clone()).unwrap();
        assert_eq!(team_zip.file_name, format!("{team}.zip"));
        let match_zip = from.export_match(team.clone(), m.clone()).unwrap();
        assert_eq!(match_zip.file_name, format!("{m}.zip"));

        let to_dir = tempfile::tempdir().unwrap();
        let to = app(&to_dir, "it");
        // renamed on the way: the archive has the id
        let renamed = Some("Cortona (1).zip".to_string());
        assert_eq!(
            to.preview_archive(team_zip.bytes.clone(), renamed.clone(), None)
                .unwrap(),
            ArchivePreview::Team {
                name: "Cortona".into(),
                year: 2026,
                players: 7,
            }
        );
        assert_eq!(
            to.import_team(team_zip.bytes.clone(), renamed).unwrap(),
            ImportedArchive {
                id: team.clone(),
                summary: None
            }
        );
        // again: the same team, nothing to tell
        assert_eq!(
            to.import_team(team_zip.bytes, None).unwrap(),
            ImportedArchive {
                id: team.clone(),
                summary: None
            }
        );
        let it = labels(LanguageEnum::It);
        assert_eq!(
            to.preview_archive(match_zip.bytes.clone(), None, None)
                .unwrap(),
            ArchivePreview::Match {
                opponent: "Arezzo".into(),
                home: true,
                date: "2026-10-24T18:00:00+02:00".into(),
                won: 0,
                lost: 0,
                teams: vec![TeamChoice {
                    id: team.clone(),
                    name: "Cortona".into(),
                }],
            }
        );
        assert_eq!(
            to.import_match(match_zip.bytes.clone(), team.clone())
                .unwrap(),
            ImportedArchive {
                id: m.clone(),
                summary: None
            }
        );
        assert_eq!(
            to.match_details(team.clone(), m.clone()).unwrap(),
            from.match_details(team.clone(), m).unwrap()
        );
        assert_eq!(
            reason(to.import_match(match_zip.bytes, team)),
            it.match_already_exists
        );
    }

    #[test]
    fn the_archives_of_the_cli_up_to_v0_9_0_are_imported() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let en = labels(LanguageEnum::En);
        assert_eq!(
            reason(app.preview_archive(OLD_MATCH.to_vec(), None, None)),
            en.match_of_another_team
        );
        assert_eq!(
            reason(app.preview_archive(OLD_TEAM.to_vec(), None, None)),
            en.team_archive_without_id
        );
        let file_name = Some(OLD_TEAM_FILE.to_string());
        assert!(matches!(
            app.preview_archive(OLD_TEAM.to_vec(), file_name.clone(), None),
            Ok(ArchivePreview::Team { players: 8, .. })
        ));
        let team = app.import_team(OLD_TEAM.to_vec(), file_name).unwrap().id;
        let Ok(ArchivePreview::Match {
            teams, won, lost, ..
        }) = app.preview_archive(OLD_MATCH.to_vec(), Some("whatever.zip".into()), None)
        else {
            panic!("expected a match");
        };
        assert_eq!((teams.len(), won, lost), (1, 0, 0));
        // from the matches of another team
        let other = app.save_team(None, form("Other Club")).unwrap();
        assert_eq!(
            reason(app.preview_archive(OLD_MATCH.to_vec(), None, Some(other))),
            en.match_of_another_team
        );
        assert!(matches!(
            app.preview_archive(OLD_MATCH.to_vec(), None, Some(team.clone())),
            Ok(ArchivePreview::Match { .. })
        ));
        let m = app.import_match(OLD_MATCH.to_vec(), team.clone()).unwrap();
        assert_eq!(m.summary, None);
        let details = app.match_details(team.clone(), m.id).unwrap();
        assert_eq!(details.opponent, "Club Arezzo");
        assert_eq!(
            details.sets,
            vec![SetScore {
                number: 1,
                us: 3,
                them: 1
            }]
        );
        // there already: no team left to take it
        assert_eq!(
            reason(app.preview_archive(OLD_MATCH.to_vec(), None, Some(team))),
            en.match_already_exists
        );
    }

    #[test]
    fn a_file_that_is_not_an_archive_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        assert_eq!(
            reason(app.preview_archive(b"not a zip".to_vec(), None, None)),
            labels(LanguageEnum::It).invalid_archive
        );
    }

    /// A match won 3-0 with aces, the terms accepted or not: the matches in
    /// the upload queue, with their attempts.
    fn won_match(app: &Scout4All) -> (String, String) {
        let (team, m) = match_with_a_set(app);
        for _ in 1..=3 {
            let scouting = started_set(app, &team, &m);
            for _ in 1..=25 {
                scouting.choose_eval(EvalEnum::Perfect).unwrap();
            }
        }
        assert_eq!(
            app.match_details(team.clone(), m.clone()).unwrap().next,
            NextSetKind::Over
        );
        (team, m)
    }

    /// The upload queue in the database of the app in `dir`, in order:
    /// each match with its attempts.
    fn queued(dir: &tempfile::TempDir) -> Vec<(String, u64)> {
        let db = scout4all_core::db::open(&dir.path().join(layout::DATABASE)).unwrap();
        let mut rows = db
            .prepare("SELECT match_id, retry_count FROM upload_queue ORDER BY position")
            .unwrap();
        rows.query_map([], |row| Ok((row.get(0)?, row.get::<_, i64>(1)?)))
            .unwrap()
            .map(|row| row.map(|(id, tries)| (id, u64::try_from(tries).unwrap())))
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn a_finished_match_is_queued_for_the_statistics() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        app.accept_terms().unwrap();
        assert!(app.analytics_enabled());
        let (_, m) = won_match(&app);
        assert_eq!(queued(&dir), vec![(m, 0)]);
    }

    #[test]
    fn not_before_the_terms_are_accepted_nor_once_turned_off() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        won_match(&app);
        assert_eq!(queued(&dir), vec![]);

        let dir = tempfile::tempdir().unwrap();
        let app = self::app(&dir, "it");
        app.accept_terms().unwrap();
        app.set_analytics_enabled(false).unwrap();
        won_match(&app);
        assert_eq!(queued(&dir), vec![]);
    }

    #[test]
    fn the_choice_about_the_statistics_keeps_the_settings_of_the_tui() {
        let dir = tempfile::tempdir().unwrap();
        let tui = serde_json::json!({
            "language": "en",
            "scouting_keybindings": { "a": "Attack" },
            "analytics_enabled": true,
            "terms_version": TERMS_VERSION,
        });
        std::fs::write(dir.path().join(CONFIG), tui.to_string()).unwrap();
        let app = app(&dir, "it");
        app.set_analytics_enabled(false).unwrap();
        let mut expected = tui;
        expected["analytics_enabled"] = false.into();
        assert_eq!(config(&dir), expected);
        // read again when the app starts
        assert!(!self::app(&dir, "it").analytics_enabled());
    }

    #[test]
    fn the_seconds_of_the_video_keep_the_other_settings() {
        let dir = tempfile::tempdir().unwrap();
        let before = serde_json::json!({
            "language": "en",
            "scouting_keybindings": { "a": "Attack" },
            "analytics_enabled": true,
            "terms_version": TERMS_VERSION,
        });
        std::fs::write(dir.path().join(CONFIG), before.to_string()).unwrap();
        let app = app(&dir, "it");
        assert_eq!(app.video_seconds(), VideoSeconds::default());
        let saved = app
            .set_video_seconds(VideoSeconds {
                step_s: 10,
                lead_s: 99,
            })
            .unwrap();
        assert_eq!((saved.step_s, saved.lead_s), (10, 10));
        let mut expected = before;
        expected["step_s"] = 10.into();
        expected["lead_s"] = 10.into();
        assert_eq!(config(&dir), expected);
        // read again when the app starts
        assert_eq!(self::app(&dir, "it").video_seconds(), saved);
    }

    #[test]
    fn a_correction_of_a_finished_match_queues_it_again() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        app.accept_terms().unwrap();
        let (team, m) = won_match(&app);
        // sent in the meantime
        sql(&dir, "DELETE FROM upload_queue");
        let correction = app.correction(team, m.clone(), 1, 0).unwrap();
        app.save_correction(correction).unwrap();
        assert_eq!(queued(&dir), vec![(m, 0)]);
    }

    /// No network: the match stays in the queue, for the next attempt.
    #[test]
    fn a_failed_upload_is_tried_again_later() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        app.accept_terms().unwrap();
        let (_, m) = won_match(&app);
        // nothing listens there
        assert_eq!(
            app.upload_pending_to("http://127.0.0.1:9/sign"),
            Uploads::Retry
        );
        assert_eq!(queued(&dir), vec![(m, 1)]);
    }

    /// The API of the uploads, on a thread: `POST` answers with the signed
    /// URL, where `PUT` takes the archive. It fails while `down`.
    struct FakeApi {
        url: String,
        down: Arc<AtomicBool>,
        /// the archives put
        puts: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl FakeApi {
        fn start() -> FakeApi {
            use std::io::{BufRead, BufReader, Read, Write};
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let down = Arc::new(AtomicBool::new(false));
            let puts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let (is_down, put) = (down.clone(), puts.clone());
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let mut reader = BufReader::new(stream);
                    let mut head = vec![];
                    loop {
                        let mut line = String::new();
                        match reader.read_line(&mut line) {
                            Ok(n) if n > 0 && line != "\r\n" => head.push(line),
                            _ => break,
                        }
                    }
                    let length = head
                        .iter()
                        .filter_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                        })
                        .next()
                        .unwrap_or(0);
                    let mut body = vec![0; length];
                    let _ = reader.read_exact(&mut body);
                    let method = head
                        .first()
                        .and_then(|l| l.split(' ').next())
                        .unwrap_or("")
                        .to_string();
                    let status = match (is_down.load(Ordering::SeqCst), method.as_str()) {
                        (true, _) => "503 Service Unavailable".to_string(),
                        (false, "POST") => format!("201 Created\r\nLocation: http://{address}/put"),
                        (false, _) => {
                            put.fetch_add(1, Ordering::SeqCst);
                            "200 OK".to_string()
                        }
                    };
                    let _ = reader.get_mut().write_all(
                        format!(
                            "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        )
                        .as_bytes(),
                    );
                }
            });
            FakeApi {
                url: format!("http://{address}/sign"),
                down,
                puts,
            }
        }

        fn puts(&self) -> usize {
            self.puts.load(Ordering::SeqCst)
        }
    }

    /// The core sending in the background to `url`, not the API of the build.
    fn sending_to(app: &Arc<Scout4All>, url: String) {
        let core = Arc::downgrade(app);
        app.sender.start(Box::new(move || {
            core.upgrade()
                .map_or(Uploads::Done, |core| core.upload_pending_to(&url))
        }));
    }

    fn eventually(what: impl Fn() -> bool) -> bool {
        // true once seen: not asked again, the state may move on meanwhile
        // (a second attempt of the upload)
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            match (what(), std::time::Instant::now() < end) {
                (true, _) => return true,
                (false, false) => return false,
                (false, true) => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }
    }

    /// A match imported from DataVolley is sent right away, without waiting
    /// for the next start of the app.
    #[test]
    fn an_imported_match_is_sent_in_the_background() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        app.accept_terms().unwrap();
        let api = FakeApi::start();
        sending_to(&app, api.url.clone());
        assert!(eventually(|| !app.sender.running()));
        app.import_dvw(THREE_SETS.to_vec(), dvw_choice(DvwSide::Home, None))
            .unwrap();
        assert!(eventually(|| api.puts() == 1 && queued(&dir).is_empty()));
    }

    /// A finished match too, as the scouting queues it.
    #[test]
    fn a_finished_match_is_sent_in_the_background() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        app.accept_terms().unwrap();
        let api = FakeApi::start();
        sending_to(&app, api.url.clone());
        let _ = won_match(&app);
        assert!(eventually(|| api.puts() == 1 && queued(&dir).is_empty()));
    }

    /// The API down: the match stays queued, sent at the next attempt.
    #[test]
    fn an_error_of_the_api_keeps_the_match_for_the_next_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        app.accept_terms().unwrap();
        let api = FakeApi::start();
        api.down.store(true, Ordering::SeqCst);
        sending_to(&app, api.url.clone());
        // the first attempt over (nothing to send), or it may find the
        // match too, failing it twice in a row
        assert!(eventually(|| !app.sender.running()));
        let imported = app
            .import_dvw(THREE_SETS.to_vec(), dvw_choice(DvwSide::Home, None))
            .unwrap();
        assert!(eventually(
            || queued(&dir) == vec![(imported.match_id.clone(), 1)]
        ));
        // waiting for the backoff
        assert!(app.sender.running());
        api.down.store(false, Ordering::SeqCst);
        app.send_uploads();
        assert!(eventually(|| api.puts() == 1 && queued(&dir).is_empty()));
    }

    /// An import asks the sender once: a second request while the first
    /// attempt runs is a second attempt right after it, which with the API
    /// down fails the match twice in a row.
    #[test]
    fn an_import_asks_to_send_once() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        app.accept_terms().unwrap();
        let api = FakeApi::start();
        sending_to(&app, api.url.clone());
        assert_eq!(app.sender.requests(), 1);
        app.import_dvw(THREE_SETS.to_vec(), dvw_choice(DvwSide::Home, None))
            .unwrap();
        assert_eq!(app.sender.requests(), 2);
    }

    /// Before `send_in_background` (Android, with WorkManager) nothing is
    /// sent by the core.
    #[test]
    fn nothing_is_sent_in_the_background_unless_asked() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        app.accept_terms().unwrap();
        let (_, m) = won_match(&app);
        app.send_uploads();
        assert!(!app.sender.running());
        assert_eq!(queued(&dir), vec![(m, 0)]);
    }

    /// Three sets won by the home team ("Home", players 1 to 7, the libero
    /// 7) against "Visiting", on 2015-01-25: the fixture of the TUI.
    const THREE_SETS: &[u8] = include_bytes!("../../core/src/tests/fixtures/three_sets.dvw");

    fn dvw_choice(side: DvwSide, team_id: Option<String>) -> DvwChoice {
        DvwChoice {
            opponent: DvwOpponent::Name,
            side,
            day: "2015-01-25".into(),
            max_substitutions: 6,
            team_id,
            classification: Some(TeamClassificationEnum::Regional),
            gender: Some(GenderEnum::Women),
            country: CountryCode::known("SI"),
        }
    }

    /// A match imported on both sides (B a new team): its statistics from
    /// B's side too, with B's players and A's aces as aces against.
    #[test]
    fn the_statistics_of_b_of_a_match_on_both_sides() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let choice = DvwChoice {
            opponent: DvwOpponent::NewTeam,
            ..dvw_choice(DvwSide::Home, None)
        };
        let imported = app.import_dvw(THREE_SETS.to_vec(), choice).unwrap();
        let (team, m) = (imported.team_id, imported.match_id);
        assert!(
            app.match_details(team.clone(), m.clone())
                .unwrap()
                .both_sides
        );
        let stats = |side| {
            app.match_stats(
                team.clone(),
                m.clone(),
                side,
                StatsScope::Match,
                DetailFilter::default(),
            )
            .unwrap()
        };
        let (a, b) = (stats(MatchSide::A), stats(MatchSide::B));
        assert_eq!(b.sets, a.sets);
        assert_eq!((b.aces, b.aces_against), (a.aces_against, a.aces));
        assert_ne!(a, b);
        let players = |side| {
            app.match_detail(
                team.clone(),
                m.clone(),
                side,
                StatsScope::Match,
                EventTypeEnum::S,
                DetailFilter::default(),
            )
            .unwrap()
            .players
            .into_iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
        };
        let of_a: Vec<String> = app
            .team(team.clone())
            .unwrap()
            .players
            .into_iter()
            .map(|p| p.id)
            .collect();
        let (pa, pb) = (players(MatchSide::A), players(MatchSide::B));
        assert!(!pa.is_empty() && pa.iter().all(|p| of_a.contains(p)));
        assert!(!pb.is_empty() && pb.iter().all(|p| !of_a.contains(p)));
    }

    /// The report of a match on both sides is longer than the one of a
    /// match scouted on one: it has the opponent's pages.
    #[test]
    fn the_report_of_a_match_on_both_sides_has_the_opponent() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let report = |opponent| {
            let choice = DvwChoice {
                opponent,
                ..dvw_choice(DvwSide::Home, None)
            };
            let imported = app.import_dvw(THREE_SETS.to_vec(), choice).unwrap();
            app.match_pdf(imported.team_id, imported.match_id).unwrap()
        };
        let (one, both) = (report(DvwOpponent::Name), report(DvwOpponent::NewTeam));
        assert!(one.starts_with(b"%PDF") && both.starts_with(b"%PDF"));
        assert!(both.len() > one.len());
    }

    /// The analysis of the opponent over the matches scouted on both
    /// sides: its choices, its figures those of B in the match, a match on
    /// one side left out.
    #[test]
    fn the_analysis_of_an_opponent_over_matches_on_both_sides() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let imported = app
            .import_dvw(
                THREE_SETS.to_vec(),
                DvwChoice {
                    opponent: DvwOpponent::NewTeam,
                    ..dvw_choice(DvwSide::Home, None)
                },
            )
            .unwrap();
        let team = imported.team_id.clone();
        // a match against a name only: no side B to analyse
        app.import_dvw(THREE_SETS.to_vec(), dvw_choice(DvwSide::Visiting, None))
            .unwrap();
        let opponents = app.analysis_opponents(team.clone()).unwrap();
        assert_eq!(opponents.len(), 1);
        assert_eq!(opponents[0].matches, 1);
        let period = AnalysisPeriod::Between {
            from: "2015-01-01".into(),
            to: "2015-12-31".into(),
        };
        // the team over the same matches, to compare: the one on both sides
        let ours = app
            .analysis_against(
                team.clone(),
                opponents[0].id,
                period.clone(),
                None,
                EventTypeEnum::S,
                DetailFilter::default(),
            )
            .unwrap();
        assert_eq!(ours.matches.len(), 1);
        assert_eq!(ours.matches[0].id, imported.match_id);
        let a = app
            .match_stats(
                team.clone(),
                imported.match_id.clone(),
                MatchSide::A,
                StatsScope::Match,
                DetailFilter::default(),
            )
            .unwrap();
        assert_eq!(ours.summary.aces, a.aces);
        let of_b = app
            .opponent_analysis(
                team.clone(),
                opponents[0].id,
                period,
                None,
                EventTypeEnum::S,
                DetailFilter::default(),
            )
            .unwrap();
        assert_eq!(of_b.matches.len(), 1);
        assert_eq!(of_b.matches[0].id, imported.match_id);
        let b = app
            .match_stats(
                team,
                imported.match_id,
                MatchSide::B,
                StatsScope::Match,
                DetailFilter::default(),
            )
            .unwrap();
        assert_eq!(of_b.summary.aces, b.aces);
        assert_eq!(of_b.summary.aces_against, b.aces_against);
        // the score as B sees it: the sets A won are B's lost
        assert_eq!((of_b.matches[0].won, of_b.matches[0].lost), (0, 3));
    }

    /// B of a match scouted on A's side only: said so, no panic.
    #[test]
    fn no_statistics_of_b_of_a_match_on_one_side() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let imported = app
            .import_dvw(THREE_SETS.to_vec(), dvw_choice(DvwSide::Home, None))
            .unwrap();
        let (team, m) = (imported.team_id, imported.match_id);
        assert!(
            !app.match_details(team.clone(), m.clone())
                .unwrap()
                .both_sides
        );
        let read = app.match_stats(
            team,
            m,
            MatchSide::B,
            StatsScope::Match,
            DetailFilter::default(),
        );
        assert!(
            matches!(&read, Err(MobileError::Failed { reason }) if reason == "this match is scouted on one side only"),
            "{read:?}"
        );
    }

    #[test]
    fn a_datavolley_file_previewed() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let preview = app.preview_dvw(THREE_SETS.to_vec()).unwrap();
        assert_eq!(
            preview,
            DvwPreview {
                home: "Home".into(),
                visiting: "Visiting".into(),
                days: vec!["2015-01-25".into()],
                season: Some(2014),
                sets: (0..3)
                    .map(|_| DvwSetScore {
                        home: 25,
                        visiting: 0
                    })
                    .collect(),
            }
        );
        // day first, then month first
        let text = String::from_utf8_lossy(THREE_SETS).replacen("01/25/2015", "02/03/2015", 1);
        let preview = app.preview_dvw(text.into_bytes()).unwrap();
        assert_eq!(preview.days, vec!["2015-03-02", "2015-02-03"]);
        // the year of the season out of the names
        let text = String::from_utf8_lossy(THREE_SETS).replacen("H;Home;", "H;Home 2014;", 1);
        let preview = app.preview_dvw(text.into_bytes()).unwrap();
        assert_eq!(preview.home, "Home");
        assert_eq!(app.dvw_max_substitutions(Some("2025-12-31".into())), 6);
        assert_eq!(app.dvw_max_substitutions(Some("2026-01-01".into())), 8);
        assert_eq!(app.dvw_max_substitutions(None), 8);
    }

    /// The events behind the detail of each fundamental, to see in the
    /// video: as many as it counts, by evaluation, set and player too.
    #[test]
    fn the_clips_of_a_statistic() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let imported = app
            .import_dvw(THREE_SETS.to_vec(), dvw_choice(DvwSide::Home, None))
            .unwrap();
        let (team, m) = (imported.team_id, imported.match_id);
        let scopes = [StatsScope::Match, StatsScope::Set { number: 2 }];
        let mut seen = 0;
        for (scope, event) in scopes.into_iter().flat_map(|s| {
            use EventTypeEnum::*;
            [S, P, D, B, A].map(|e| (s, e))
        }) {
            let detail = app
                .match_detail(
                    team.clone(),
                    m.clone(),
                    MatchSide::A,
                    scope,
                    event,
                    DetailFilter::default(),
                )
                .unwrap();
            let Some(f) = detail.fundamental else {
                continue;
            };
            let pick = ClipPick {
                scope,
                event,
                filter: DetailFilter::default(),
                evals: vec![],
                zone: None,
                target: None,
            };
            let clips = app.match_clips(&team, &m, &pick).unwrap();
            assert_eq!(clips.len() as u32, f.total, "{event:?} in {scope:?}");
            seen += clips.len();
            if let StatsScope::Set { number } = scope {
                assert!(clips.iter().all(|c| c.set_number == number));
            }
            for e in &f.evals {
                let by_eval = ClipPick {
                    evals: vec![e.eval],
                    ..pick.clone()
                };
                let clips = app.match_clips(&team, &m, &by_eval).unwrap();
                assert_eq!(clips.len() as u32, e.count, "{event:?} {:?}", e.eval);
            }
            for p in &f.players {
                let filter = DetailFilter {
                    players: vec![p.player.id.clone()],
                    ..DetailFilter::default()
                };
                let by_player = ClipPick {
                    filter,
                    ..pick.clone()
                };
                let clips = app.match_clips(&team, &m, &by_player).unwrap();
                assert_eq!(clips.len() as u32, p.total, "{event:?} {}", p.player.name);
            }
        }
        assert!(seen > 0, "the match has events");
    }

    #[test]
    fn a_file_not_datavolley_is_said_in_the_language_of_the_app() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        let it = labels(LanguageEnum::It);
        let message = reason(app.preview_dvw(b"nothing here".to_vec()));
        assert_eq!(
            message,
            "il file non si legge come DataVolley (non è un file DataVolley)"
        );
        assert_eq!(
            message,
            it.dvw_unreadable.replace("{detail}", it.dvw_not_dvw)
        );
    }

    /// A new team: checked, then saved with its match, queued for the
    /// statistics as any finished match; the level, gender and country
    /// chosen.
    #[test]
    fn a_datavolley_team_imported_as_a_new_team() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let choice = dvw_choice(DvwSide::Home, None);
        assert!(
            app.dvw_similar_teams(THREE_SETS.to_vec(), DvwSide::Home, None)
                .unwrap()
                .is_empty()
        );
        let ready = app.plan_dvw(THREE_SETS.to_vec(), choice.clone()).unwrap();
        assert_eq!(
            ready,
            DvwReady {
                team: "Home".into(),
                sets: 3,
                new_players: 7,
                opponent: "Visiting".into(),
                opponent_new_players: None,
                match_numbers: 0,
                competition: Some("Test".into()),
                one_side: None,
            }
        );
        assert!(app.teams().unwrap().is_empty(), "checking saves nothing");
        let imported = app.import_dvw(THREE_SETS.to_vec(), choice).unwrap();
        let team = app.team(imported.team_id.clone()).unwrap();
        assert_eq!((team.name.as_str(), team.year), ("Home", 2014));
        assert_eq!(team.gender.as_deref(), Some("women"));
        assert_eq!(team.classification.as_deref(), Some("regional"));
        assert_eq!(
            team.form.classification,
            Some(TeamClassificationEnum::Regional)
        );
        assert_eq!(team.players.len(), 7);
        let matches = app.matches(imported.team_id).unwrap();
        assert!(
            matches!(
                &matches[..],
                [MatchListing::Match { id, opponent, home: true, won: 3, lost: 0, finished: true, .. }]
                    if *id == imported.match_id && opponent == "Visiting"
            ),
            "{matches:?}"
        );
    }

    /// The visiting team into a team of ours like it; then again: the match
    /// saved already, refused.
    #[test]
    fn a_datavolley_team_into_a_similar_team() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let first = app
            .import_dvw(THREE_SETS.to_vec(), dvw_choice(DvwSide::Visiting, None))
            .unwrap();
        let similar = app
            .dvw_similar_teams(THREE_SETS.to_vec(), DvwSide::Visiting, None)
            .unwrap();
        assert_eq!(
            similar,
            vec![DvwSimilarTeam {
                id: first.team_id.clone(),
                name: "Visiting".into(),
                year: 2014,
                same_name: true,
                same_players: 7,
                players: 7,
            }]
        );
        let choice = dvw_choice(DvwSide::Visiting, Some(first.team_id.clone()));
        // the match saved already: refused, to delete first
        let refused = "Visiting – Home is saved already, played the same day with the same sets: \
                       to import it again, delete it first";
        assert_eq!(
            reason(app.plan_dvw(THREE_SETS.to_vec(), choice.clone())),
            refused
        );
        assert_eq!(reason(app.import_dvw(THREE_SETS.to_vec(), choice)), refused);
        let matches = app.matches(first.team_id).unwrap();
        assert!(
            matches!(
                &matches[..],
                [MatchListing::Match {
                    home: false,
                    won: 0,
                    lost: 3,
                    ..
                }]
            ),
            "{matches:?}"
        );
    }

    /// Team B: the teams like the other team of the file proposed, team A
    /// left out, the team of the side imported before first; a new team
    /// with the players of the file, told in the check; the other side
    /// against the team of the first: the same match, refused.
    #[test]
    fn a_datavolley_match_against_a_team() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let home = DvwChoice {
            opponent: DvwOpponent::NewTeam,
            ..dvw_choice(DvwSide::Home, None)
        };
        let ready = app.plan_dvw(THREE_SETS.to_vec(), home.clone()).unwrap();
        assert_eq!(
            (ready.opponent.as_str(), ready.opponent_new_players),
            ("Visiting", Some(7))
        );
        assert!(app.teams().unwrap().is_empty(), "checking saves nothing");
        let first = app.import_dvw(THREE_SETS.to_vec(), home).unwrap();
        let visiting = app
            .dvw_similar_teams(THREE_SETS.to_vec(), DvwSide::Visiting, None)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect::<Vec<_>>();
        assert_eq!(visiting.len(), 1, "team B saved, like the visiting team");
        // the visiting side into team B of the first: team A first among
        // its opponents, itself left out
        let opponents = |team_id: Option<String>, first: Option<String>| {
            app.dvw_similar_opponents(THREE_SETS.to_vec(), DvwSide::Visiting, team_id, first)
                .unwrap()
                .into_iter()
                .map(|t| t.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            opponents(visiting.first().cloned(), Some(first.team_id.clone())),
            vec![first.team_id.clone()]
        );
        assert_eq!(
            opponents(Some(first.team_id.clone()), None),
            Vec::<String>::new()
        );
        let second = DvwChoice {
            opponent: DvwOpponent::Team {
                id: first.team_id.clone(),
            },
            ..dvw_choice(DvwSide::Visiting, visiting.first().cloned())
        };
        // the same match, on both sides already: refused
        assert_eq!(
            reason(app.plan_dvw(THREE_SETS.to_vec(), second)),
            "Home – Visiting is saved already, played the same day with the same sets: \
             to import it again, delete it first"
        );
        assert_eq!(app.teams().unwrap().len(), 2);
        // team A as B: refused, said
        let itself = DvwChoice {
            opponent: DvwOpponent::Team {
                id: first.team_id.clone(),
            },
            ..dvw_choice(DvwSide::Home, Some(first.team_id.clone()))
        };
        assert_eq!(
            reason(app.plan_dvw(THREE_SETS.to_vec(), itself)),
            "team B can't be team A"
        );
    }

    /// Team B a team whose side doesn't translate: the match only from
    /// A's side, the check telling why; against a name, nothing to tell.
    #[test]
    fn a_datavolley_match_on_one_side_tells_why() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        // the serve of B by a player not on the court
        let text = String::from_utf8(THREE_SETS.to_vec())
            .unwrap()
            .replacen("a11SM-", "a12SM-", 1);
        let bytes = text.into_bytes();
        let home = DvwChoice {
            opponent: DvwOpponent::NewTeam,
            ..dvw_choice(DvwSide::Home, None)
        };
        let ready = app.plan_dvw(bytes.clone(), home).unwrap();
        let told = ready.one_side.unwrap_or_default();
        assert!(
            told.starts_with(
                "only the side of Home, without the touches of Visiting: set 1, line "
            ),
            "{told}"
        );
        let name = app
            .plan_dvw(bytes, dvw_choice(DvwSide::Home, None))
            .unwrap();
        assert_eq!(name.one_side, None);
    }

    /// Asked from a team, the import offers it first: before the alike
    /// ones, even if not alike; once only.
    #[test]
    fn a_datavolley_import_from_a_team_offers_it_first() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let alike = app
            .import_dvw(THREE_SETS.to_vec(), dvw_choice(DvwSide::Visiting, None))
            .unwrap()
            .team_id;
        let ours = app.save_team(None, form("Cortona")).unwrap();
        let offered = |first: Option<String>| {
            app.dvw_similar_teams(THREE_SETS.to_vec(), DvwSide::Visiting, first)
                .unwrap()
                .into_iter()
                .map(|t| (t.id, t.same_name))
                .collect::<Vec<_>>()
        };
        assert_eq!(offered(None), vec![(alike.clone(), true)]);
        assert_eq!(
            offered(Some(ours.clone())),
            vec![(ours, false), (alike.clone(), true)]
        );
        assert_eq!(offered(Some(alike.clone())), vec![(alike, true)]);
    }

    /// A new team without its level, gender or country: the field said, as
    /// by the form of a team, in the language of the app; nothing saved.
    #[test]
    fn a_new_datavolley_team_needs_level_gender_and_country() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        let it = labels(LanguageEnum::It);
        let full = dvw_choice(DvwSide::Home, None);
        for (choice, field, message) in [
            (
                DvwChoice {
                    classification: None,
                    gender: None,
                    country: None,
                    ..full.clone()
                },
                TeamField::Classification,
                it.classification_is_required,
            ),
            (
                DvwChoice {
                    gender: None,
                    ..full.clone()
                },
                TeamField::Gender,
                it.gender_is_required,
            ),
            (
                DvwChoice {
                    country: None,
                    ..full.clone()
                },
                TeamField::Country,
                it.country_is_required,
            ),
        ] {
            for result in [
                app.plan_dvw(THREE_SETS.to_vec(), choice.clone())
                    .map(|_| ()),
                app.import_dvw(THREE_SETS.to_vec(), choice).map(|_| ()),
            ] {
                match result {
                    Err(MobileError::InvalidTeam { field: f, reason }) => {
                        assert_eq!((f, reason.as_str()), (field, message));
                    }
                    other => panic!("{field:?}: {other:?}"),
                }
            }
        }
        assert!(app.teams().unwrap().is_empty());
    }

    /// Why a side can't be imported, in the language of the app: nothing
    /// saved.
    #[test]
    fn a_datavolley_team_not_imported_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let text =
            String::from_utf8_lossy(THREE_SETS).replacen("True;;;;25-0;;", "True;;;;25-3;;", 1);
        let choice = dvw_choice(DvwSide::Home, None);
        let message = reason(app.plan_dvw(text.clone().into_bytes(), choice.clone()));
        assert!(
            message.contains("25-3 in the file, 25-0 from its rallies"),
            "{message}"
        );
        assert!(app.import_dvw(text.into_bytes(), choice).is_err());
        assert!(app.teams().unwrap().is_empty());
    }

    /// The analysis of a team: the matches of the period in order of date,
    /// each figure match by match with the moving average, the statistics
    /// together, the days played; a filter on the phase narrows them all.
    #[test]
    fn the_analysis_of_the_matches_of_a_period() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, first) = match_with_a_set(&app);
        let second = app
            .save_match(
                team.clone(),
                None,
                match_form("Siena", "2026-10-31T18:00:00+01:00", "8"),
            )
            .unwrap();
        // not played: never in the analysis
        app.save_match(
            team.clone(),
            None,
            match_form("Pisa", "2026-10-27T18:00:00+01:00", "8"),
        )
        .unwrap();
        let scouting = started_set(&app, &team, &first);
        for _ in 0..3 {
            scouting.choose_eval(EvalEnum::Perfect).unwrap();
        }
        let scouting = started_set(&app, &team, &second);
        scouting.choose_eval(EvalEnum::Perfect).unwrap();
        scouting.choose_eval(EvalEnum::Error).unwrap();
        let analysis = |period, filter| {
            app.team_analysis(team.clone(), period, None, EventTypeEnum::S, filter)
                .unwrap()
        };
        let last_two = analysis(
            AnalysisPeriod::LastMatches { count: 2 },
            DetailFilter::default(),
        );
        let opponents: Vec<&str> = last_two
            .matches
            .iter()
            .map(|m| m.opponent.as_str())
            .collect();
        assert_eq!(opponents, ["Arezzo", "Siena"]);
        assert_eq!(last_two.from.as_deref(), Some("2026-10-24"));
        assert_eq!(last_two.to.as_deref(), Some("2026-10-31"));
        assert_eq!(last_two.played_days, ["2026-10-24", "2026-10-31"]);
        let serve = last_two
            .trend
            .iter()
            .find(|t| t.series == Series::Serve)
            .unwrap();
        assert_eq!(serve.values, [Some(100.0), Some(0.0)]);
        assert_eq!(serve.moving, [Some(100.0), Some(60.0)]);
        assert_eq!(serve.touches, [3, 2]);
        assert_eq!(serve.overall, Some(60.0));
        assert_eq!(last_two.summary.aces, 4);
        let detail = last_two.detail.fundamental.unwrap();
        assert_eq!(detail.total, 5);
        // the days the other way round: the same period
        let between = analysis(
            AnalysisPeriod::Between {
                from: "2026-10-31".into(),
                to: "2026-10-24".into(),
            },
            DetailFilter::default(),
        );
        assert_eq!(between.matches.len(), 2);
        let only_last = analysis(
            AnalysisPeriod::Between {
                from: "2026-10-25".into(),
                to: "2026-10-31".into(),
            },
            DetailFilter::default(),
        );
        assert_eq!(only_last.summary.aces, 1);
        // the serves are in break: none in side out
        let side_out = analysis(
            AnalysisPeriod::LastMatches { count: 2 },
            DetailFilter {
                phase: Some(PhaseEnum::SideOut),
                ..DetailFilter::default()
            },
        );
        assert_eq!(side_out.summary.aces, 0);
        let serve = side_out
            .trend
            .iter()
            .find(|t| t.series == Series::Serve)
            .unwrap();
        assert_eq!(serve.values, [None, None]);
        assert!(
            app.team_analysis(
                team.clone(),
                AnalysisPeriod::Between {
                    from: "31/10/2026".into(),
                    to: "2026-10-24".into(),
                },
                None,
                EventTypeEnum::S,
                DetailFilter::default(),
            )
            .is_err()
        );
    }

    /// A touch of ours told event first: the event, the player, the
    /// evaluation.
    fn touch(scouting: &Scouting, event: EventTypeEnum, player: &str, eval: EvalEnum) {
        scouting.choose_event(event).unwrap();
        scouting.choose_player(player.into()).unwrap();
        scouting.choose_eval(eval).unwrap();
    }

    /// The players of the filter, as the app lists them: number and name.
    fn listed(players: &[ScoutingPlayer]) -> Vec<(u8, String)> {
        players.iter().map(|p| (p.number, p.name.clone())).collect()
    }

    /// The players of the filter of the analysis, by number.
    fn numbers(players: &[ScoutingPlayer]) -> Vec<u8> {
        players.iter().map(|p| p.number).collect()
    }

    /// The id of the player of the number on court.
    fn on_court(scouting: &Scouting, number: u8) -> String {
        scouting
            .state()
            .court
            .into_iter()
            .find(|p| p.number == number)
            .map(|p| p.id)
            .unwrap()
    }

    /// A match of a team of eight (8 never on court, a second libero), its
    /// set started as `started_set` (the libero 7 on court for 6): our
    /// serve `+` of 1 and the libero's dig, their error; our serve error;
    /// the libero's reception, their error; the serve `/` of 2 and the
    /// block of 4 (the middle blocker, never serving); a fault of 5. The
    /// team, the match.
    fn libero_match(app: &Scout4All) -> (String, String) {
        let (team, m) = match_with_a_set(app);
        app.save_player(team.clone(), None, player("P8", "8"), false)
            .unwrap();
        let scouting = started_set(app, &team, &m);
        let libero = on_court(&scouting, 7);
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        touch(&scouting, EventTypeEnum::D, &libero, EvalEnum::Positive);
        scouting.choose_event(EventTypeEnum::OE).unwrap();
        scouting.choose_eval(EvalEnum::Error).unwrap();
        touch(&scouting, EventTypeEnum::P, &libero, EvalEnum::Perfect);
        scouting.choose_event(EventTypeEnum::OE).unwrap();
        scouting.choose_eval(EvalEnum::Over).unwrap();
        touch(
            &scouting,
            EventTypeEnum::B,
            &on_court(&scouting, 4),
            EvalEnum::Over,
        );
        scouting.choose_event(EventTypeEnum::F).unwrap();
        scouting.choose_player(on_court(&scouting, 5)).unwrap();
        (team, m)
    }

    /// Another match of the team, a week later: our serve error of 1, the
    /// libero's reception `+`, their error.
    fn libero_receives(app: &Scout4All, team: &str, date: &str) {
        let m = app
            .save_match(team.into(), None, match_form("Siena", date, "8"))
            .unwrap();
        let scouting = started_set(app, team, &m);
        scouting.choose_eval(EvalEnum::Error).unwrap();
        let libero = scouting
            .state()
            .court
            .into_iter()
            .find(|p| p.libero)
            .map(|p| p.id)
            .unwrap();
        touch(&scouting, EventTypeEnum::P, &libero, EvalEnum::Positive);
        scouting.choose_event(EventTypeEnum::OE).unwrap();
    }

    /// With players chosen, the figures of the team (its rallies) aren't
    /// shown, in the analysis and in the statistics of a match: they'd be
    /// empty, a rally has no player.
    #[test]
    fn the_figures_of_the_team_are_not_of_a_player() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let imported = app
            .import_dvw(THREE_SETS.to_vec(), dvw_choice(DvwSide::Home, None))
            .unwrap();
        let team = imported.team_id.clone();
        let season = |filter| {
            app.team_analysis(
                team.clone(),
                AnalysisPeriod::Season,
                None,
                EventTypeEnum::S,
                filter,
            )
            .unwrap()
        };
        let series = |a: &TeamAnalysis| a.trend.iter().map(|t| t.series).collect::<Vec<_>>();
        let all = season(DetailFilter::default());
        assert!(all.summary.team_figures);
        assert!(all.summary.side_out.is_some());
        assert_eq!(series(&all), Series::ALL.to_vec());
        let server = all.detail.players[0].id.clone();
        let of_player = DetailFilter {
            players: vec![server],
            ..DetailFilter::default()
        };
        let narrowed = season(of_player.clone());
        assert!(!narrowed.summary.team_figures);
        // what they would show
        assert_eq!(narrowed.summary.side_out, None);
        assert_eq!(
            series(&narrowed),
            [
                Series::Reception,
                Series::Attack,
                Series::CounterAttack,
                Series::Serve
            ]
        );
        let m = imported.match_id;
        let of_match = |filter| {
            app.match_stats(
                team.clone(),
                m.clone(),
                MatchSide::A,
                StatsScope::Match,
                filter,
            )
            .unwrap()
        };
        assert!(of_match(DetailFilter::default()).team_figures);
        let narrowed = of_match(of_player);
        assert!(!narrowed.team_figures);
        // the trend of a match is of every rally, whatever the filter
        assert_eq!(narrowed.trend.len(), Series::ALL.len());
    }

    /// A team that was only B of a match scouted on both sides has it in
    /// its analysis and in the statistics of its players, which tell the
    /// same figures as the analysis narrowed to the player.
    #[test]
    fn the_statistics_of_a_player_of_side_b() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let team = app
            .import_dvw(
                THREE_SETS.to_vec(),
                DvwChoice {
                    opponent: DvwOpponent::NewTeam,
                    ..dvw_choice(DvwSide::Home, None)
                },
            )
            .unwrap()
            .team_id;
        let b = app.analysis_opponents(team.clone()).unwrap()[0].id;
        let season = |filter| {
            app.team_analysis(
                b.to_string(),
                AnalysisPeriod::Season,
                None,
                EventTypeEnum::S,
                filter,
            )
            .unwrap()
        };
        let all = season(DetailFilter::default());
        assert_eq!(all.matches.len(), 1);
        let from_a = app
            .opponent_analysis(
                team.clone(),
                b,
                AnalysisPeriod::Season,
                None,
                EventTypeEnum::S,
                DetailFilter::default(),
            )
            .unwrap();
        assert_eq!(all.matches, from_a.matches);
        assert_eq!(all.summary, from_a.summary);
        let server = all.detail.players[0].id.clone();
        let stats = app
            .player_statistics(b.to_string(), server.clone(), AnalysisPeriod::Season, None)
            .unwrap();
        let narrowed = season(DetailFilter {
            players: vec![server],
            ..DetailFilter::default()
        });
        let detail = narrowed.detail.fundamental.unwrap();
        let serve = &stats.fundamentals[0];
        assert_eq!(serve.event, EventTypeEnum::S);
        assert_eq!(serve.measure, PlayerMeasure::Efficiency);
        assert_eq!(serve.value, detail.efficiency);
        assert_eq!(serve.total, detail.total);
        assert_eq!(serve.points, detail.points);
        assert_eq!(serve.errors, detail.errors);
        let trend = narrowed
            .trend
            .iter()
            .find(|t| t.series == Series::Serve)
            .unwrap();
        assert_eq!(stats.trend.first().map(|t| &t.line), Some(trend));
        assert_eq!(stats.matches, narrowed.matches);
        assert!(stats.fundamentals.iter().all(|f| f.total > 0));
        let receiver = app
            .team_analysis(
                team.clone(),
                AnalysisPeriod::Season,
                None,
                EventTypeEnum::P,
                DetailFilter::default(),
            )
            .unwrap()
            .detail
            .players[0]
            .id
            .clone();
        let stats = app
            .player_statistics(team, receiver, AnalysisPeriod::Season, None)
            .unwrap();
        let reception = stats
            .fundamentals
            .iter()
            .find(|f| f.event == EventTypeEnum::P)
            .unwrap();
        let trend = stats
            .trend
            .iter()
            .find(|t| t.line.series == Series::Reception)
            .unwrap();
        assert_eq!(trend.measure, PlayerMeasure::Positivity);
        // one match: the box, the match and the overall are the same
        assert_eq!(trend.line.values, [reception.value]);
        assert_eq!(trend.line.moving, [reception.value]);
        assert_eq!(trend.line.overall, reception.value);
        assert_eq!(trend.line.band, None);
    }

    fn analysis_of(
        app: &Scout4All,
        team: &str,
        count: u32,
        event: EventTypeEnum,
        filter: DetailFilter,
    ) -> TeamAnalysis {
        app.team_analysis(
            team.into(),
            AnalysisPeriod::LastMatches { count },
            None,
            event,
            filter,
        )
        .unwrap()
    }

    /// The filter of the pages of the analysis but the detail lists every
    /// player with a touch of any fundamental, whatever the fundamental of
    /// the detail: the libero only receiving and digging, the middle
    /// blocker only blocking, the player only at fault; not the ones never
    /// touching the ball (the second libero, 3, 6 replaced by the libero).
    /// The detail keeps the players of its fundamental (TODO/000).
    #[test]
    fn the_filter_of_the_analysis_lists_every_player_with_a_touch() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let (team, _) = libero_match(&app);
        let all = DetailFilter::default();
        for (event, of_the_detail) in [
            (EventTypeEnum::S, vec![1, 2]),
            (EventTypeEnum::P, vec![7]),
            (EventTypeEnum::D, vec![7]),
            (EventTypeEnum::B, vec![4]),
            (EventTypeEnum::A, vec![]),
            (EventTypeEnum::FB, vec![]),
        ] {
            let a = analysis_of(&app, &team, 4, event, all.clone());
            assert_eq!(
                listed(&a.players),
                [(1, "P1"), (2, "P2"), (4, "P4"), (5, "P5"), (7, "P7")]
                    .map(|(n, name)| (n, name.to_string())),
                "{event:?}"
            );
            assert_eq!(numbers(&a.detail.players), of_the_detail, "{event:?}");
            // the detail's players are among them
            assert!(a.detail.players.iter().all(|p| a.players.contains(p)));
        }
    }

    /// The libero chosen in the filter of the analysis: the summary and the
    /// trend count their reception and dig only, as the role of libero
    /// does; with the rotation and the phase, the touches of those; with
    /// another player, the touches of both (TODO/000).
    #[test]
    fn the_figures_of_the_libero_chosen_are_their_touches() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let (team, _) = libero_match(&app);
        let all = analysis_of(&app, &team, 4, EventTypeEnum::S, DetailFilter::default());
        let id = |number: u8| {
            all.players
                .iter()
                .find(|p| p.number == number)
                .map(|p| p.id.clone())
                .unwrap()
        };
        let totals = |filter: DetailFilter| {
            let a = analysis_of(&app, &team, 4, EventTypeEnum::S, filter);
            let summary: Vec<(EventTypeEnum, u32)> = a
                .summary
                .fundamentals
                .iter()
                .map(|f| (f.event, f.total))
                .collect();
            let trend: Vec<(Series, Vec<u32>)> = a
                .trend
                .iter()
                .filter(|t| matches!(t.series, Series::Reception | Series::Serve))
                .map(|t| (t.series, t.touches.clone()))
                .collect();
            (summary, trend)
        };
        let libero = DetailFilter {
            players: vec![id(7)],
            ..DetailFilter::default()
        };
        use EventTypeEnum::{A, B, D, P, S};
        let (summary, trend) = totals(libero.clone());
        assert_eq!(summary, [(P, 1), (A, 0), (S, 0), (B, 0), (D, 1)]);
        assert_eq!(
            trend,
            [(Series::Reception, vec![1]), (Series::Serve, vec![0])]
        );
        // the role of libero: the same touches
        let role = DetailFilter {
            roles: vec![RoleEnum::Libero],
            ..DetailFilter::default()
        };
        assert_eq!(totals(role.clone()).0, summary);
        assert_eq!(
            totals(DetailFilter {
                players: vec![id(7)],
                ..role
            })
            .0,
            summary
        );
        // the reception in side out, the dig in break, both in S1
        let narrowed = |phase, rotations: Vec<u8>| {
            totals(DetailFilter {
                phase,
                rotations,
                ..libero.clone()
            })
            .0
        };
        assert_eq!(
            narrowed(Some(PhaseEnum::SideOut), vec![]),
            [(P, 1), (A, 0), (S, 0), (B, 0), (D, 0)]
        );
        assert_eq!(
            narrowed(Some(PhaseEnum::Break), vec![]),
            [(P, 0), (A, 0), (S, 0), (B, 0), (D, 1)]
        );
        assert_eq!(narrowed(None, vec![0]), summary);
        assert_eq!(
            narrowed(None, vec![1]),
            [(P, 0), (A, 0), (S, 0), (B, 0), (D, 0)]
        );
        // with the server of the first rallies: the touches of both
        let both = DetailFilter {
            players: vec![id(7), id(1)],
            ..DetailFilter::default()
        };
        assert_eq!(totals(both).0, [(P, 1), (A, 0), (S, 2), (B, 0), (D, 1)]);
    }

    /// A player chosen in the filter of the analysis with no touch (the
    /// second libero), or no player of the team at all: no error, the
    /// first listed to take the filter off, with figures of none; the
    /// other one left out (TODO/000).
    #[test]
    fn a_player_chosen_with_no_touch_stays_in_the_filter() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let (team, _) = libero_match(&app);
        let second = app
            .team(team.clone())
            .unwrap()
            .players
            .into_iter()
            .find(|p| p.number == 8)
            .unwrap()
            .id;
        let chosen = DetailFilter {
            players: vec![second.clone(), Uuid::new_v4().to_string()],
            ..DetailFilter::default()
        };
        for event in [EventTypeEnum::S, EventTypeEnum::P] {
            let a = analysis_of(&app, &team, 4, event, chosen.clone());
            assert_eq!(numbers(&a.players), [1, 2, 4, 5, 7, 8]);
            assert!(numbers(&a.detail.players).contains(&8));
            assert!(a.summary.fundamentals.iter().all(|f| f.total == 0));
            assert!(a.trend.iter().all(|t| t.touches.iter().all(|n| *n == 0)));
        }
        // a period with no match: only the one chosen
        let none = app
            .team_analysis(
                team.clone(),
                AnalysisPeriod::Between {
                    from: "2020-01-01".into(),
                    to: "2020-01-31".into(),
                },
                None,
                EventTypeEnum::S,
                chosen,
            )
            .unwrap();
        assert!(none.matches.is_empty());
        assert_eq!(numbers(&none.players), [8]);
    }

    /// No match, or no touch: nobody to choose, no panic.
    #[test]
    fn with_no_touch_the_filter_of_the_analysis_is_empty() {
        assert!(touched(&Stats::default()).is_empty());
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let (team, m) = match_with_a_set(&app);
        let all = DetailFilter::default();
        let a = analysis_of(&app, &team, 4, EventTypeEnum::S, all.clone());
        assert!(a.matches.is_empty() && a.players.is_empty());
        // a set started, no rally
        started_set(&app, &team, &m);
        let a = analysis_of(&app, &team, 4, EventTypeEnum::S, all);
        assert!(a.players.is_empty() && a.detail.players.is_empty());
    }

    /// Over several matches each player is listed once, by number; one
    /// removed from the team after the matches stays, with the name and
    /// number of then, beside the new player with the same number; the
    /// period lists the players of its matches only (TODO/000).
    #[test]
    fn the_filter_of_the_analysis_over_matches_and_removed_players() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let (team, _) = libero_match(&app);
        libero_receives(&app, &team, "2026-10-31T18:00:00+01:00");
        let all = DetailFilter::default();
        let both = analysis_of(&app, &team, 4, EventTypeEnum::S, all.clone());
        assert_eq!(numbers(&both.players), [1, 2, 4, 5, 7]);
        assert_eq!(numbers(&both.detail.players), [1, 2]);
        // the last match: the server and the libero
        let last = analysis_of(&app, &team, 1, EventTypeEnum::S, all.clone());
        assert_eq!(numbers(&last.players), [1, 7]);
        // the libero leaves, a new 7 is the libero of a third match
        let libero = both.players[4].id.clone();
        app.remove_player(team.clone(), libero.clone()).unwrap();
        app.save_player(team.clone(), None, player("Q7", "7"), false)
            .unwrap();
        libero_receives(&app, &team, "2026-11-07T18:00:00+01:00");
        let three = analysis_of(&app, &team, 4, EventTypeEnum::P, all.clone());
        let sevens: Vec<(String, &str)> = three
            .players
            .iter()
            .filter(|p| p.number == 7)
            .map(|p| (p.id.clone(), p.name.as_str()))
            .collect();
        assert_eq!(sevens.len(), 2, "{sevens:?}");
        assert!(sevens.contains(&(libero.clone(), "P7")));
        assert!(
            sevens
                .iter()
                .any(|(id, name)| *id != libero && *name == "Q7")
        );
        assert_eq!(numbers(&three.players), [1, 2, 4, 5, 7, 7]);
        assert_eq!(numbers(&three.detail.players), [7, 7]);
        // the last match: the new libero only
        let last = analysis_of(&app, &team, 1, EventTypeEnum::S, all);
        assert_eq!(listed(&last.players), [(1, "P1".into()), (7, "Q7".into())]);
        // the libero removed, chosen: their reception of the two matches
        let removed = analysis_of(
            &app,
            &team,
            4,
            EventTypeEnum::P,
            DetailFilter {
                players: vec![libero],
                ..DetailFilter::default()
            },
        );
        assert_eq!(removed.detail.fundamental.map(|f| f.total), Some(2));
    }

    /// Seen from an opponent scouted on both sides, the filter lists its
    /// players with a touch, never the team's; every fundamental's players
    /// among them; the team against the opponent lists its own (TODO/000).
    #[test]
    fn the_filter_of_the_analysis_of_an_opponent() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let imported = app
            .import_dvw(
                THREE_SETS.to_vec(),
                DvwChoice {
                    opponent: DvwOpponent::NewTeam,
                    ..dvw_choice(DvwSide::Home, None)
                },
            )
            .unwrap();
        let team = imported.team_id;
        let opponent = app.analysis_opponents(team.clone()).unwrap()[0].id;
        let ids = |players: &[ScoutingPlayer]| -> Vec<String> {
            players.iter().map(|p| p.id.clone()).collect()
        };
        let of_team =
            ids(&analysis_of(&app, &team, 4, EventTypeEnum::S, DetailFilter::default()).players);
        let period = AnalysisPeriod::LastMatches { count: 4 };
        let all = DetailFilter::default();
        let mut of_the_details: Vec<String> = vec![];
        for event in FUNDAMENTALS {
            let b = app
                .opponent_analysis(
                    team.clone(),
                    opponent,
                    period.clone(),
                    None,
                    event,
                    all.clone(),
                )
                .unwrap();
            assert!(!b.players.is_empty());
            assert!(ids(&b.players).iter().all(|p| !of_team.contains(p)));
            assert!(b.detail.players.iter().all(|p| b.players.contains(p)));
            of_the_details.extend(ids(&b.detail.players));
            let against = app
                .analysis_against(
                    team.clone(),
                    opponent,
                    period.clone(),
                    None,
                    event,
                    all.clone(),
                )
                .unwrap();
            assert_eq!(ids(&against.players), of_team);
        }
        // a player of the opponent touching the ball has a fundamental
        let b = app
            .opponent_analysis(team.clone(), opponent, period, None, EventTypeEnum::S, all)
            .unwrap();
        assert!(ids(&b.players).iter().all(|p| of_the_details.contains(p)));
    }

    /// A match imported from DataVolley on both sides, the libero 7
    /// receiving (DataVolley tells the libero's touches with them out of
    /// the lineup) and a second libero 8 never on court: the filter of the
    /// analysis lists the libero, not the second one, whatever the detail;
    /// chosen, the libero counts their reception (TODO/000).
    #[test]
    fn the_filter_of_the_analysis_lists_the_libero_of_datavolley() {
        let text = String::from_utf8_lossy(THREE_SETS)
            .replacen("*05RM+;", "*07RM+;", 1)
            .replacen(
                "0;7;7;*;;;;;P7;Surname7;Name7;;L;1;False;;;\n",
                "0;7;7;*;;;;;P7;Surname7;Name7;;L;1;False;;;\n0;8;8;;;;;;P8;Surname8;Name8;;L;1;False;;;\n",
                1,
            );
        assert!(text.contains("*07RM+;") && text.contains("P8;Surname8"));
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let imported = app
            .import_dvw(
                text.into_bytes(),
                DvwChoice {
                    opponent: DvwOpponent::NewTeam,
                    ..dvw_choice(DvwSide::Home, None)
                },
            )
            .unwrap();
        let team = imported.team_id;
        let numbers_of_team: Vec<u8> = app
            .team(team.clone())
            .unwrap()
            .players
            .iter()
            .map(|p| p.number)
            .collect();
        assert!(numbers_of_team.contains(&8), "{numbers_of_team:?}");
        let all = DetailFilter::default();
        for event in FUNDAMENTALS {
            let a = analysis_of(&app, &team, 4, event, all.clone());
            assert!(numbers(&a.players).contains(&7), "{event:?}");
            assert!(!numbers(&a.players).contains(&8), "{event:?}");
            assert_eq!(
                numbers(&a.detail.players).contains(&7),
                event == EventTypeEnum::P,
                "{event:?}"
            );
        }
        let libero = analysis_of(&app, &team, 4, EventTypeEnum::S, all)
            .players
            .into_iter()
            .find(|p| p.number == 7)
            .unwrap()
            .id;
        let chosen = analysis_of(
            &app,
            &team,
            4,
            EventTypeEnum::S,
            DetailFilter {
                players: vec![libero],
                ..DetailFilter::default()
            },
        );
        let totals: Vec<(EventTypeEnum, u32)> = chosen
            .summary
            .fundamentals
            .iter()
            .map(|f| (f.event, f.total))
            .collect();
        use EventTypeEnum::{A, B, D, P, S};
        assert_eq!(totals, [(P, 1), (A, 0), (S, 0), (B, 0), (D, 0)]);
    }
}
