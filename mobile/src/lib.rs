//! The core of scout4all for the native apps: UniFFI generates the Kotlin
//! and Swift bindings of what's exported here.
//!
//! The API is blocking for now: the app calls it off the main thread
//! (`Dispatchers.IO` on Android) and a tokio runtime here drives the async
//! providers of the core. The final app must never block the UI: the plan is
//! to move to the `async fn` of UniFFI.

use chrono::{DateTime, FixedOffset, Local, NaiveDate};
use scout4all_core::{
    analytics::{
        finished::{FinishedMatches, QueuedFinishedMatches},
        queue_manager::QueueManager,
        upload::{UploadStep, upload_next, upload_url},
    },
    archive::{self, Archive, file_team_id, match_to_import, read_archive, team_to_import},
    benchmark::{Measure, band, grade},
    detail::{Breakdown, FUNDAMENTALS, Tone, by_player, by_rotation},
    dvw::{
        self,
        import::{
            MatchPlan, TeamChoice as DvwTeamChoice, TeamIssue, TeamPlan, default_max_substitutions,
            match_date, match_exists, match_plan, save_import, season_year, similar_teams,
            team_plan,
        },
        model::{DvwFile, MatchDate as DvwDate, Side},
    },
    errors::{AppError, IOError},
    event_edit::{EditField, EventEdit},
    example,
    localization::{Labels, Language},
    logging::logger::{init_logger, log_error},
    match_form::{MatchForm, MatchFormError},
    overview::{Errors, Fundamental, Overview, distribution, stats_of},
    player_form::{PlayerForm, PlayerFormError, removal},
    providers::{
        match_reader::MatchReader,
        match_writer::MatchWriter,
        set_writer::SetWriter,
        store::{
            match_reader::StoreMatchReader, match_writer::StoreMatchWriter,
            queue_reader::StoreQueueReader, queue_writer::StoreQueueWriter,
            set_reader::StoreSetReader, set_writer::StoreSetWriter, team_reader::StoreTeamReader,
            team_writer::StoreTeamWriter,
        },
        team_reader::TeamReader,
        team_writer::TeamWriter,
    },
    scouting::{ChoiceOrder, PlayerChoice, Progress, ScoutingError, ScoutingSession, ScoutingStep},
    set_start::{self, MIN_PLAYERS_FOR_FALLBACK_LIBERO, NextSet, SetStartForm, next_set},
    settings::{Consent, PinnedTeams, TeamDefaults},
    shapes::{
        country::CountryCode,
        court_zone::{CourtCell, CourtSide, CourtTarget, CourtZoneEnum, SubzoneEnum, TargetKind},
        enums::{
            EvalEnum, EventTypeEnum, FriendlyName, GenderEnum, LanguageEnum, PhaseEnum, RoleEnum,
            TeamClassificationEnum, TeamSideEnum, ZoneEnum,
        },
        flags::BlockersEnum,
        r#match::MatchEntry,
        player::PlayerEntry,
        set::SetEntry,
        snapshot::{EventEntry, Formation},
        stats::{AttackSummary, Metric, Stats, StatsFilter},
        team::TeamEntry,
        technique::{ServeTypeEnum, SetTypeEnum, TechniqueEnum},
    },
    storage::{
        Store, StoreEntry,
        fs::FsStore,
        layout::{self, CONFIG, UNREADABLE_CONFIG},
    },
    team_form::{TeamForm, TeamFormError},
    trend::{Replay, Series, TrendPoint},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, from_slice, to_value, to_vec_pretty};
use std::{
    path::PathBuf,
    str::FromStr,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::runtime::{Builder, Runtime};
use uuid::Uuid;

uniffi::setup_scaffolding!();

// the types of the core the app sees as they are

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
pub enum Tone {
    Excellent,
    Good,
    Fair,
    Poor,
    Bad,
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
    /// a field of the team form, which the app shows next to it
    #[error("{reason}")]
    InvalidTeam { field: TeamField, reason: String },
    /// a field of the player form
    #[error("{reason}")]
    InvalidPlayer { field: PlayerField, reason: String },
    /// a field of the match form
    #[error("{reason}")]
    InvalidMatch { field: MatchField, reason: String },
    /// a choice of the scout that doesn't apply, or an event not saved
    #[error("{reason}")]
    Scouting { reason: String },
}

impl From<AppError> for MobileError {
    fn from(e: AppError) -> Self {
        // the messages of the core are in the labels of the domain, for the
        // user as they are
        let reason = match e {
            AppError::IO(IOError::Msg(message)) => message,
            other => other.to_string(),
        };
        MobileError::Failed { reason }
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
            MatchFormError::EmptyOpponent => MatchField::Opponent,
            MatchFormError::InvalidDate => MatchField::Date,
            MatchFormError::InvalidMaxSubstitutions | MatchFormError::MaxSubstitutionsBelowUsed => {
                MatchField::MaxSubstitutions
            }
            MatchFormError::RosterTooSmall
            | MatchFormError::RosterTooLarge
            | MatchFormError::RosterWithoutPlayed
            | MatchFormError::RosterWithoutLibero => MatchField::Roster,
        }
    }
}

/// A role as the form offers it.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct RoleChoice {
    pub value: RoleEnum,
    pub name: String,
}

/// A player that can be on the score sheet of a match, and whether it is.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct RosterPlayer {
    pub id: Uuid,
    pub name: String,
    pub number: u8,
    pub role: Option<String>,
    pub chosen: bool,
    /// played in a set of the match: on the score sheet for good
    pub played: bool,
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
    /// what the form starts from, to edit the player
    pub form: PlayerForm,
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

/// An archive to share or to save: the zip of the CLI, and the name the
/// CLI gives to its file.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct ExportedArchive {
    pub file_name: String,
    pub bytes: Vec<u8>,
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
}

/// The import checked, ready to save.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct DvwReady {
    pub team: String,
    pub sets: u32,
    pub new_players: u32,
    /// The team has the match already: saved only when the user confirms.
    pub saved_already: bool,
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
            chooses_serving_team: form.chooses_serving_team(),
            serving_team: form.serving_team,
            positions: form.positions.iter().map(|id| player(*id)).collect(),
            setter: player(form.setter),
            libero: player(form.libero),
            fallback_libero: player(form.fallback_libero),
            allows_fallback_libero: self.team.active_players().len()
                >= MIN_PLAYERS_FOR_FALLBACK_LIBERO,
            complete: form.clone().validate(&self.team).is_ok(),
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
}

/// An event as the scout chooses it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct EventChoice {
    pub value: EventTypeEnum,
    pub name: String,
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
}

/// The sets the statistics are of: one of the match, or all of them so
/// far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
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
    /// the six rotations, in order, played or not
    pub rotations: Vec<RotationStats>,
}

/// The phases won in a rotation.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct RotationStats {
    /// 0 to 5, as `DetailFilter::rotation`
    pub rotation: u8,
    /// P1 to P6 in Italian, S1 to S6 in English
    pub name: String,
    pub side_out: Option<PhaseStats>,
    pub break_points: Option<PhaseStats>,
}

/// What the detail of a fundamental is narrowed to: none keeps them all.
#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct DetailFilter {
    pub phase: Option<PhaseEnum>,
    /// 0 to 5, the setter in position 1 to 6
    pub rotation: Option<u8>,
    /// the id of a player
    pub player: Option<String>,
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
    /// the players with an event in the match so far, by their number
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
        let team = &current_match.team;
        let (won, lost) = current_match
            .get_status()
            .map_or((0, 0), |s| (s.us_wins, s.them_wins));
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
        let mut events: Vec<&EventEntry> = session.set().events.iter().collect();
        events.sort_by_key(|e| e.timestamp);
        ScoutingState {
            set_number: session.set().set_number,
            team: team.name.clone(),
            opponent: current_match.opponent.clone(),
            home: current_match.home,
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
            events: events
                .into_iter()
                .rev()
                .map(|e| self.scouted_event(team, e))
                .collect(),
            ball: snapshot.get_ball_side(),
            last_touch: snapshot
                .get_last_touch()
                .map(|(e, _)| self.scouted_event(team, e)),
            inverted_p1: session.inverted_p1(),
            inverted_p1_applies: session.inverted_p1_applies(),
            reception: session.reception().players(),
            reception_open: session.can_tell_reception(),
            team_choice: session.team_choice(),
        }
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
    /// faults, the changes of the lineup.
    pub fn other_events(&self) -> Vec<EventChoice> {
        let session = self.session();
        session
            .other_events()
            .into_iter()
            .map(|e| self.event_choice(&session, e))
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
    /// with their trend rally by rally.
    pub fn stats(&self, scope: StatsScope) -> Result<LiveStats, MobileError> {
        let session = self.session();
        let current_match = session.current_match();
        live_stats(
            &session.sets(),
            scope,
            current_match.max_substitutions,
            &current_match.team,
            self.labels,
        )
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
}

/// The statistics of a set of the match, or of all of them, with their
/// trend rally by rally: `all` are the sets of the match so far, in order.
fn live_stats(
    all: &[&SetEntry],
    scope: StatsScope,
    max_substitutions: u8,
    team: &TeamEntry,
    labels: &Labels,
) -> Result<LiveStats, MobileError> {
    let stats = stats_of(&in_scope(all, scope), max_substitutions)?;
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
    let trend = trend_lines(all, max_substitutions, team, labels)?;
    let o = Overview::of(&stats);
    let fundamental = |event: EventTypeEnum, f: Fundamental| FundamentalStats {
        event,
        name: event.friendly_name(labels).to_string(),
        total: f.total,
        efficiency: f.efficiency,
        perfect: f.perfect,
        errors: f.errors,
        grade: Measure::of(event).and_then(|m| level(m, f.efficiency)),
    };
    let phase = |p: Option<(f64, u32, u32)>, measure| {
        p.map(|(percentage, won, rallies)| PhaseStats {
            percentage,
            won,
            rallies,
            grade: level(measure, Some(percentage)),
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
        distribution(&stats, counter_attack)
            .into_iter()
            .map(|z| ZoneStats {
                zone: zone_number(z.zone),
                share: z.share,
                attacks: attack_stats(z.attacks),
            })
            .collect()
    };
    Ok(LiveStats {
        scope,
        sets: all.iter().map(|s| s.set_number).collect(),
        fundamentals: vec![
            fundamental(EventTypeEnum::P, o.reception),
            fundamental(EventTypeEnum::A, o.attack),
            fundamental(EventTypeEnum::S, o.serve),
            fundamental(EventTypeEnum::B, o.block),
            fundamental(EventTypeEnum::D, o.dig),
        ],
        side_out: phase(o.side_out, Measure::SideOut),
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
        break_points: phase(o.break_points, Measure::BreakPoint),
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
        trend,
        rotations: by_rotation(&stats)
            .into_iter()
            .map(|r| RotationStats {
                rotation: r.rotation,
                name: rotation_name(r.rotation, labels),
                side_out: phase(r.side_out, Measure::SideOut),
                break_points: phase(r.break_points, Measure::BreakPoint),
            })
            .collect(),
    })
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
    let player = filter.player.as_deref().map(parse_id).transpose()?;
    let narrowed = StatsFilter::new(filter.phase, filter.rotation, player);
    let stats = stats_of(&in_scope(all, scope), max_substitutions)?;
    let level = |value: Option<f64>| {
        Measure::of(event)
            .zip(value)
            .and_then(|(m, v)| grade(team.classification, team.gender, m, v))
    };
    let player_of = |id: Uuid| team.find_player(id).map(|p| scouting_player(p, labels));
    let (fundamental, freeballs) = match Breakdown::of(&stats, event, &narrowed) {
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
                players: by_player(&stats, event, &narrowed)
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
    let mut players: Vec<ScoutingPlayer> = involved(&stats_of(all, max_substitutions)?)
        .into_iter()
        .filter_map(player_of)
        .collect();
    players.sort_by_key(|p| p.number);
    let fundamental_name = |e: EventTypeEnum| match e {
        EventTypeEnum::FB => labels.freeballs.to_string(),
        _ => e.friendly_name(labels).to_string(),
    };
    Ok(DetailStats {
        scope,
        sets: all.iter().map(|s| s.set_number).collect(),
        event,
        filter,
        fundamentals: FUNDAMENTALS
            .iter()
            .map(|e| EventChoice {
                value: *e,
                name: fundamental_name(*e),
            })
            .collect(),
        players,
        rotations: (0..6)
            .map(|r| RotationChoice {
                value: r,
                name: rotation_name(r, labels),
            })
            .collect(),
        phases: [PhaseEnum::SideOut, PhaseEnum::Break]
            .into_iter()
            .map(|p| PhaseChoice {
                value: p,
                name: p.friendly_name(labels).to_string(),
            })
            .collect(),
        fundamental,
        freeballs,
    })
}

/// The players with an event of ours in the statistics, by their id.
fn involved(stats: &Stats) -> Vec<Uuid> {
    let mut players: Vec<Uuid> = stats
        .events
        .iter(|k| k.player.is_some())
        .filter_map(|(k, _)| k.player)
        .collect();
    players.sort_unstable();
    players.dedup();
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
    }
}

fn scouting_player(p: &PlayerEntry, labels: &Labels) -> ScoutingPlayer {
    ScoutingPlayer {
        id: p.id.to_string(),
        name: p.name.clone(),
        number: p.number,
        role: p
            .role
            .map_or_else(String::new, |r| r.friendly_name(labels).to_string()),
    }
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
) -> Result<Vec<TrendLine>, MobileError> {
    let match_replay = Replay::new(sets, max_substitutions)?;
    let set_replays = sets
        .iter()
        .map(|set| Replay::new(&[set], max_substitutions).map(|r| (set.set_number, r)))
        .collect::<Result<Vec<_>, _>>()?;
    let values = |points: Vec<TrendPoint<Option<f64>>>| {
        points.into_iter().map(|p| p.value).collect::<Vec<_>>()
    };
    let last = |points: Vec<TrendPoint<Option<f64>>>| points.last().and_then(|p| p.value);
    let level = |series: Series, value: Option<f64>| {
        value.and_then(|v| grade(team.classification, team.gender, series.measure(), v))
    };
    Ok(Series::ALL
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
                    })
                    .collect(),
                overall,
                overall_recent,
                overall_grade: level(*series, overall),
                overall_recent_grade: level(*series, overall_recent),
            }
        })
        .collect())
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
            .filter_map(|c| session.current_match().team.find_player(c.id))
            .map(|p| self.scouting_player(p))
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
        }
    }

    fn scouted_event(&self, team: &TeamEntry, e: &EventEntry) -> ScoutedEvent {
        scouted_event(team, e, self.labels)
    }

    fn scouting_player(&self, p: &PlayerEntry) -> ScoutingPlayer {
        scouting_player(p, self.labels)
    }
}

/// Correcting an event of a set (see `event_edit` in the core): saved
/// with `Scout4All::save_correction`.
#[derive(uniffi::Object)]
pub struct Correction {
    current_match: MatchEntry,
    edit: Mutex<EventEdit>,
    labels: &'static Labels,
}

#[uniffi::export]
impl Correction {
    pub fn state(&self) -> CorrectionState {
        let edit = self.edit();
        let labels = self.labels;
        let team = &self.current_match.team;
        let event = edit.event();
        let before = edit.before();
        CorrectionState {
            index: u32::try_from(edit.index()).unwrap_or(u32::MAX),
            event: scouted_event(team, event, labels),
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
            fields: edit.fields(),
            event_types: edit
                .event_types()
                .into_iter()
                .map(|e| EventChoice {
                    value: e,
                    name: e.friendly_name(labels).to_string(),
                })
                .collect(),
            evals: edit
                .evals()
                .into_iter()
                .map(|eval| eval_choice(eval, event.event_type, labels))
                .collect(),
            eval: event.eval,
            techniques: edit
                .techniques()
                .into_iter()
                .flatten()
                .map(|t| technique_choice(t, event.technique == Some(t), labels))
                .collect(),
        }
    }

    pub fn choose_event_type(&self, event: EventTypeEnum) {
        self.edit().choose_event_type(event);
    }

    pub fn choose_eval(&self, eval: EvalEnum) {
        self.edit().choose_eval(eval);
    }

    /// A set or serve, or none.
    pub fn choose_technique(&self, technique: Option<TechniqueEnum>) {
        self.edit().choose_technique(technique);
    }
}

impl Correction {
    fn edit(&self) -> MutexGuard<'_, EventEdit> {
        self.edit
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The data of the app, in a folder with the same layout as `~/.scout4all`.
#[derive(uniffi::Object)]
pub struct Scout4All {
    /// shared with the sets being scouted
    runtime: Arc<Runtime>,
    language: Language,
    team_reader: StoreTeamReader,
    team_writer: StoreTeamWriter,
    match_reader: StoreMatchReader,
    match_writer: StoreMatchWriter,
    set_writer: Arc<dyn SetWriter + Send + Sync>,
    /// for `config.json`
    store: Arc<dyn Store>,
    /// the matches to upload for the statistics
    queue: Arc<UploadQueueManager>,
    /// whether the user allows the statistics, as in `config.json`
    uploads_enabled: Arc<AtomicBool>,
}

type UploadQueueManager = QueueManager<StoreQueueReader, StoreQueueWriter>;

#[uniffi::export]
impl Scout4All {
    /// The data in `data_dir`; `language` is a code like `"it"`, English when
    /// it isn't supported.
    #[uniffi::constructor]
    pub fn new(data_dir: String, language: String) -> Result<Arc<Self>, MobileError> {
        let data_dir = PathBuf::from(data_dir);
        let language = Language::new(LanguageEnum::from_str(&language).unwrap_or(LanguageEnum::En));
        let store: Arc<dyn Store> = Arc::new(FsStore::new(&data_dir));
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| MobileError::Failed {
                reason: e.to_string(),
            })?;
        let app = Self {
            runtime: Arc::new(runtime),
            queue: Arc::new(QueueManager::new(
                Arc::new(StoreQueueReader::new(store.clone())),
                Arc::new(StoreQueueWriter::new(store.clone())),
            )),
            uploads_enabled: Arc::new(AtomicBool::new(false)),
            team_reader: StoreTeamReader::new(store.clone(), language.clone()),
            team_writer: StoreTeamWriter::new(store.clone()),
            match_writer: StoreMatchWriter::new(store.clone()),
            set_writer: Arc::new(StoreSetWriter::new(store.clone(), language.clone())),
            match_reader: StoreMatchReader::new(
                store.clone(),
                Arc::new(StoreSetReader::new(store.clone(), language.clone())),
                language.clone(),
            ),
            store,
            language,
        };
        // unreadable settings: nothing sent until the terms are accepted
        let enabled = app.read_config().is_ok_and(|c| c.consent.uploads_enabled());
        app.uploads_enabled.store(enabled, Ordering::Relaxed);
        Ok(Arc::new(app))
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

    /// The matches of the team, the most recent first, then the unreadable
    /// ones.
    pub fn matches(&self, team_id: String) -> Result<Vec<MatchListing>, MobileError> {
        let team = self.read_team(&team_id)?;
        Ok(self
            .runtime
            .block_on(self.match_reader.read_all(&team))?
            .into_iter()
            .map(|listed| match listed.map(|m| (m.get_status(), m)) {
                Ok((Ok(status), m)) => MatchListing::Match {
                    date: m.date.to_rfc3339(),
                    id: m.id,
                    opponent: m.opponent,
                    home: m.home,
                    won: status.us_wins,
                    lost: status.them_wins,
                    finished: status.match_finished,
                },
                Ok((Err(_), m)) => MatchListing::Unreadable {
                    reason: m
                        .replay_issue()
                        .map(|issue| issue.describe(self.language.labels())),
                    folder: m.id,
                },
                Err(unreadable) => MatchListing::Unreadable {
                    folder: unreadable.id,
                    reason: unreadable.reason,
                },
            })
            .collect())
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
            let read = self.runtime.block_on(self.match_reader.read_all(&team))?;
            matches.extend(read.into_iter().flatten().filter_map(|m| {
                let status = m.get_status().ok()?;
                Some((
                    m.date,
                    HomeMatch {
                        team_id: team.id.to_string(),
                        team_name: team.name.clone(),
                        date: m.date.to_rfc3339(),
                        id: m.id,
                        opponent: m.opponent,
                        home: m.home,
                        won: status.us_wins,
                        lost: status.them_wins,
                        next: next_kind(&status, &team),
                    },
                ))
            }));
        }
        matches.sort_by(|(a, _), (b, _)| b.cmp(a));
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

    /// A match of the team, with its sets.
    pub fn match_details(
        &self,
        team_id: String,
        match_id: String,
    ) -> Result<MatchDetails, MobileError> {
        let m = self.read_match(&team_id, &match_id)?;
        let status = m.get_status()?;
        let sets = set_scores(&m)?;
        let next = next_kind(&status, &m.team);
        Ok(MatchDetails {
            date: m.date.to_rfc3339(),
            id: m.id,
            opponent: m.opponent,
            home: m.home,
            won: status.us_wins,
            lost: status.them_wins,
            finished: status.match_finished,
            sets,
            next,
            notes: m.notes,
        })
    }

    /// The statistics of a set of the match, or of all of them, as while
    /// scouting.
    pub fn match_stats(
        &self,
        team_id: String,
        match_id: String,
        scope: StatsScope,
    ) -> Result<LiveStats, MobileError> {
        let m = self.read_match(&team_id, &match_id)?;
        let mut sets: Vec<&SetEntry> = m.sets.iter().collect();
        sets.sort_by_key(|s| s.set_number);
        live_stats(
            &sets,
            scope,
            m.max_substitutions,
            &m.team,
            self.language.labels(),
        )
    }

    /// A fundamental of a set of the match, or of all of them, narrowed by
    /// the filter, as while scouting.
    pub fn match_detail(
        &self,
        team_id: String,
        match_id: String,
        scope: StatsScope,
        event: EventTypeEnum,
        filter: DetailFilter,
    ) -> Result<DetailStats, MobileError> {
        let m = self.read_match(&team_id, &match_id)?;
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

    /// The events of a set of the match, to correct them.
    pub fn set_events(
        &self,
        team_id: String,
        match_id: String,
        set_number: u8,
    ) -> Result<SetEvents, MobileError> {
        let m = self.read_match(&team_id, &match_id)?;
        let labels = self.language.labels();
        let set = find_set(&m, set_number)?;
        let scores = set.scores(m.max_substitutions)?;
        Ok(SetEvents {
            set_number,
            team: m.team.name.clone(),
            opponent: m.opponent.clone(),
            events: set
                .events
                .iter()
                .zip(scores)
                .enumerate()
                .map(|(i, (e, (score_us, score_them)))| SetEvent {
                    index: u32::try_from(i).unwrap_or(u32::MAX),
                    event: scouted_event(&m.team, e, labels),
                    score_us,
                    score_them,
                    editable: !matches!(
                        e.event_type,
                        EventTypeEnum::R
                            | EventTypeEnum::CL
                            | EventTypeEnum::LR
                            | EventTypeEnum::LF
                            | EventTypeEnum::CS
                    ),
                })
                .collect(),
        })
    }

    /// Starts correcting an event of a set of the match.
    pub fn correction(
        &self,
        team_id: String,
        match_id: String,
        set_number: u8,
        index: u32,
    ) -> Result<Arc<Correction>, MobileError> {
        let m = self.read_match(&team_id, &match_id)?;
        let labels = self.language.labels();
        let edit = EventEdit::new(
            find_set(&m, set_number)?,
            usize::try_from(index).unwrap_or(usize::MAX),
            m.max_substitutions,
        )
        .map_err(|e| MobileError::Failed {
            reason: e.message(labels).to_string(),
        })?;
        Ok(Arc::new(Correction {
            current_match: m,
            edit: Mutex::new(edit),
            labels,
        }))
    }

    /// Saves the set with the event corrected, if the events after it still
    /// fit and the set ends as it did.
    pub fn save_correction(&self, correction: Arc<Correction>) -> Result<(), MobileError> {
        let labels = self.language.labels();
        let set = correction.edit().apply().map_err(|e| MobileError::Failed {
            reason: e.message(labels).to_string(),
        })?;
        self.runtime
            .block_on(self.set_writer.update(&correction.current_match, &set))?;
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
                labels: self.language.labels(),
            })),
            NextSet::Resume(_) | NextSet::Over => Err(MobileError::Failed {
                reason: format!("no set to start in {match_id}"),
            }),
        }
    }

    /// Scouts the set in progress of the match.
    pub fn scouting(
        &self,
        team_id: String,
        match_id: String,
    ) -> Result<Arc<Scouting>, MobileError> {
        let m = self.read_match(&team_id, &match_id)?;
        match next_set(&m.get_status()?, &m.team) {
            NextSet::Resume(set) => set_start::resume(m, set, self.set_writer.clone())
                .map(|session| {
                    Arc::new(Scouting {
                        runtime: self.runtime.clone(),
                        session: Mutex::new(
                            session
                                .with_order(ChoiceOrder::PlayerFirst)
                                .with_finished(self.finished_matches()),
                        ),
                        labels: self.language.labels(),
                    })
                })
                .map_err(|e| MobileError::Failed {
                    reason: e.message(self.language.labels()),
                }),
            NextSet::Start(_) | NextSet::Over => Err(MobileError::Failed {
                reason: format!("no set in progress in {match_id}"),
            }),
        }
    }

    /// Saves the set that `set_start` starts.
    pub fn start_set(&self, set_start: Arc<SetStart>) -> Result<(), MobileError> {
        let m = self.read_match(&set_start.team.id.to_string(), &set_start.match_id)?;
        let failed = |e: set_start::SetStartError| MobileError::Failed {
            reason: e.message(self.language.labels()),
        };
        let input = set_start.form().clone().validate(&m.team).map_err(failed)?;
        self.runtime
            .block_on(set_start::start(m, input, self.set_writer.clone()))
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
        let folder = layout::team_folder(&parse_id(&team_id)?);
        let taken: Vec<String> = self
            .runtime
            .block_on(self.store.list(&folder))?
            .into_iter()
            .filter_map(|entry| match entry {
                StoreEntry::Folder(name) => Some(name),
                StoreEntry::Blob(_) => None,
            })
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
        // the team as it is, also with the players not on the roster
        let team = self.read_team(&team_id)?;
        let existing = match_id
            .map(|id| self.read_match(&team_id, &id))
            .transpose()?;
        let mut form = form;
        if let Some(player) = toggled {
            form.toggle(player, &team, existing.as_ref());
        }
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
            })
            .collect();
        Ok(MatchRoster { form, players })
    }

    /// The form of a match of the team, to edit it.
    pub fn match_form(&self, team_id: String, match_id: String) -> Result<MatchForm, MobileError> {
        Ok(MatchForm::of(&self.read_match(&team_id, &match_id)?))
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
                .block_on(self.match_writer.update(
                    &m,
                    input.opponent,
                    input.date,
                    input.home,
                    input.max_substitutions,
                    input.notes,
                    input.roster,
                ))
                .map(|()| m.id),
            None => self
                .runtime
                .block_on(self.match_writer.create(
                    &team,
                    input.opponent,
                    input.date,
                    input.home,
                    input.max_substitutions,
                    input.notes,
                    input.roster,
                ))
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

    /// Adds a player to the team (no `player_id`) or updates the one with
    /// `player_id`; returns its id.
    pub fn save_player(
        &self,
        team_id: String,
        player_id: Option<String>,
        form: PlayerForm,
    ) -> Result<String, MobileError> {
        let mut team = self.read_team(&team_id)?;
        let existing = player_id.map(|id| find_player(&team, &id)).transpose()?;
        let input = form
            .validate(&team, existing.as_ref(), Local::now().date_naive())
            .map_err(|e| MobileError::InvalidPlayer {
                field: e.into(),
                reason: e.message(self.language.labels()),
            })?;
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
            .block_on(self.team_writer.save_player(removal(&player), &mut team))?;
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

    /// The archive of the team, as the CLI exports it.
    pub fn export_team(&self, team_id: String) -> Result<ExportedArchive, MobileError> {
        let id = parse_id(&team_id)?;
        let bytes = self
            .runtime
            .block_on(archive::export_team(&*self.store, &id))?;
        Ok(ExportedArchive {
            file_name: format!("{id}.zip"),
            bytes,
        })
    }

    /// The archive of a match of the team, as the CLI exports it.
    pub fn export_match(
        &self,
        team_id: String,
        match_id: String,
    ) -> Result<ExportedArchive, MobileError> {
        let id = parse_id(&team_id)?;
        let bytes = self
            .runtime
            .block_on(archive::export_match(&*self.store, &id, &match_id))?;
        Ok(ExportedArchive {
            file_name: format!("{match_id}.zip"),
            bytes,
        })
    }

    /// What the archive holds, unless it can't be imported: e.g. a team
    /// already there, or a match of none of the teams, or not of `team_id`
    /// when it's imported into that team. `file_name` is the name of the
    /// file it comes from, when known: the archives exported up to v0.9.0
    /// have the id of the team only there.
    pub fn preview_archive(
        &self,
        bytes: Vec<u8>,
        file_name: Option<String>,
        team_id: Option<String>,
    ) -> Result<ArchivePreview, MobileError> {
        let labels = self.language.labels();
        match read_archive(&bytes, labels)? {
            Archive::Team { .. } => {
                let file_id = file_name.as_deref().and_then(file_team_id);
                let team = self.runtime.block_on(team_to_import(
                    &bytes,
                    file_id,
                    &self.team_reader,
                    labels,
                ))?;
                Ok(ArchivePreview::Team {
                    players: count(&team),
                    name: team.name,
                    year: team.year,
                })
            }
            Archive::Match(_) => {
                let teams = match team_id {
                    Some(id) => vec![self.read_team(&id)?],
                    None => self.readable_teams()?,
                };
                let (m, found) = self.runtime.block_on(match_to_import(
                    &bytes,
                    &teams,
                    &self.match_reader,
                    labels,
                ))?;
                let status = m.get_status()?;
                let mut choices: Vec<TeamChoice> = found
                    .into_iter()
                    .map(|t| TeamChoice {
                        id: t.id.to_string(),
                        name: t.name.clone(),
                    })
                    .collect();
                choices.sort_by_key(|t| t.name.to_lowercase());
                Ok(ArchivePreview::Match {
                    date: m.date.to_rfc3339(),
                    opponent: m.opponent,
                    home: m.home,
                    won: status.us_wins,
                    lost: status.them_wins,
                    teams: choices,
                })
            }
        }
    }

    /// Adds the team of the archive; its id.
    pub fn import_team(
        &self,
        bytes: Vec<u8>,
        file_name: Option<String>,
    ) -> Result<String, MobileError> {
        let file_id = file_name.as_deref().and_then(file_team_id);
        let team = self.runtime.block_on(archive::import_team(
            &bytes,
            file_id,
            &self.team_reader,
            &self.team_writer,
            self.language.labels(),
        ))?;
        Ok(team.id.to_string())
    }

    /// What the DataVolley file holds.
    pub fn preview_dvw(&self, bytes: Vec<u8>) -> Result<DvwPreview, MobileError> {
        let file = self.parse_dvw(&bytes)?;
        let day = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
        Ok(DvwPreview {
            home: file.home.name.trim().to_string(),
            visiting: file.visiting.name.trim().to_string(),
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

    /// Our teams like the one of the side, the most alike first.
    pub fn dvw_similar_teams(
        &self,
        bytes: Vec<u8>,
        side: DvwSide,
    ) -> Result<Vec<DvwSimilarTeam>, MobileError> {
        let file = self.parse_dvw(&bytes)?;
        let teams = self.readable_teams()?;
        Ok(similar_teams(&file, side.into(), &teams)
            .into_iter()
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

    /// The import as chosen, checked: what it saves, or why it can't.
    pub fn plan_dvw(&self, bytes: Vec<u8>, choice: DvwChoice) -> Result<DvwReady, MobileError> {
        let file = self.parse_dvw(&bytes)?;
        let (team, plan) = self.dvw_plans(&file, &choice)?;
        let saved_already = self
            .runtime
            .block_on(match_exists(&plan, &self.match_reader))?;
        Ok(DvwReady {
            team: team.team.name.clone(),
            sets: count_of(plan.entry.sets.len()),
            new_players: count_of(team.new_players.len()),
            saved_already,
        })
    }

    /// Saves the import as chosen; a match saved already only when
    /// `overwrite`.
    pub fn import_dvw(
        &self,
        bytes: Vec<u8>,
        choice: DvwChoice,
        overwrite: bool,
    ) -> Result<DvwImported, MobileError> {
        let file = self.parse_dvw(&bytes)?;
        let (team, plan) = self.dvw_plans(&file, &choice)?;
        let m = self.runtime.block_on(save_import(
            &team,
            &plan,
            overwrite,
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

    /// Adds the match of the archive to the team; its id.
    pub fn import_match(&self, bytes: Vec<u8>, team_id: String) -> Result<String, MobileError> {
        let team = self.read_team(&team_id)?;
        let m = self.runtime.block_on(archive::import_match(
            &bytes,
            &team,
            &self.match_reader,
            &self.match_writer,
            &*self.set_writer,
            self.language.labels(),
        ))?;
        Ok(m.id)
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
        match (self.uploads_enabled.load(Ordering::Relaxed), upload_url()) {
            (true, Some(url)) => self.upload_pending_to(&url),
            _ => Uploads::Done,
        }
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

fn failed(e: impl std::fmt::Display) -> MobileError {
    MobileError::Failed {
        reason: e.to_string(),
    }
}

impl Scout4All {
    /// Where the scouting sends a finished match: the queue, if the user
    /// allows the statistics.
    fn finished_matches(&self) -> Arc<dyn FinishedMatches> {
        Arc::new(QueuedFinishedMatches {
            queue: self.queue.clone(),
            enabled: self.uploads_enabled.clone(),
        })
    }

    fn upload_pending_to(&self, url: &str) -> Uploads {
        loop {
            let step = self.runtime.block_on(upload_next(
                &*self.store,
                &self.team_reader,
                &self.match_reader,
                &self.queue,
                url,
            ));
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
    ) -> Result<(TeamPlan, MatchPlan), MobileError> {
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
        let date = match_date(file, day, *Local::now().offset());
        let plan = match_plan(file, side, &team, date, choice.max_substitutions)
            .map_err(|e| failed(e.describe(labels)))?;
        Ok((team, plan))
    }

    fn readable_teams(&self) -> Result<Vec<TeamEntry>, MobileError> {
        Ok(self
            .runtime
            .block_on(self.team_reader.read_all())?
            .into_iter()
            .filter_map(Result::ok)
            .collect())
    }

    fn read_team(&self, id: &str) -> Result<TeamEntry, MobileError> {
        let id = parse_id(id)?;
        self.runtime
            .block_on(self.team_reader.read_single(&id))
            .map_err(|e| unreadable(&e, self.language.labels().unreadable_team))
    }

    fn read_match(&self, team_id: &str, match_id: &str) -> Result<MatchEntry, MobileError> {
        let team = self.read_team(team_id)?;
        self.runtime
            .block_on(self.match_reader.read_single(&team, match_id))
            .map_err(|e| unreadable(&e, self.language.labels().unreadable_match))
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

fn players(team: &TeamEntry, labels: &Labels) -> Vec<Player> {
    let mut players: Vec<Player> = team
        .active_players()
        .into_iter()
        .map(|p| Player {
            id: p.id.to_string(),
            name: p.name.clone(),
            number: p.number,
            role: p.role.map(|r| r.friendly_name(labels).to_string()),
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
    use scout4all_core::{localization::labels, settings::TERMS_VERSION};

    #[test]
    fn an_empty_folder_has_no_teams() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        assert_eq!(app.teams().unwrap(), vec![]);
    }

    #[test]
    fn a_team_folder_that_is_not_an_id_is_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("not-a-team")).unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "xx".into()).unwrap();
        assert_eq!(
            app.teams().unwrap(),
            vec![TeamListing::Unreadable {
                folder: "not-a-team".into()
            }]
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
        }
    }

    #[test]
    fn players_are_added_edited_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        let anna = app
            .save_player(team.clone(), None, player("Anna", "9"))
            .unwrap();
        app.save_player(team.clone(), None, player("Bea", "3"))
            .unwrap();
        app.save_player(team.clone(), Some(anna.clone()), player("Anna", "12"))
            .unwrap();

        let players = app.team(team.clone()).unwrap().players;
        let listed: Vec<(&str, u8)> = players
            .iter()
            .map(|p| (p.name.as_str(), p.number))
            .collect();
        assert_eq!(listed, vec![("Bea", 3), ("Anna", 12)]);
        assert_eq!(players[1].id, anna);
        assert_eq!(players[1].role.as_deref(), Some("palleggiatore"));
        assert_eq!(players[1].form, player("Anna", "12"));

        app.remove_player(team.clone(), anna).unwrap();
        let players = app.team(team.clone()).unwrap().players;
        assert_eq!(players.len(), 1);
        // the number of a removed player is free again
        app.save_player(team, None, player("Carla", "12")).unwrap();
    }

    #[test]
    fn matches_are_listed_the_most_recent_first() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team_id = app.save_team(None, form("Cortona")).unwrap();
        assert_eq!(app.matches(team_id.clone()).unwrap(), vec![]);

        let team = app.read_team(&team_id).unwrap();
        let writer = StoreMatchWriter::new(Arc::new(FsStore::new(dir.path())));
        for (opponent, date, home) in [
            ("Arezzo", "2026-10-04T18:00:00+02:00", true),
            ("Siena", "2026-10-11T21:00:00+02:00", false),
        ] {
            let date = DateTime::parse_from_rfc3339(date).unwrap();
            app.runtime
                .block_on(writer.create(
                    &team,
                    opponent.into(),
                    date,
                    home,
                    6,
                    String::new(),
                    vec![],
                ))
                .unwrap();
        }
        std::fs::create_dir(dir.path().join(&team_id).join("not-a-match")).unwrap();

        assert_eq!(
            app.matches(team_id).unwrap(),
            vec![
                MatchListing::Match {
                    id: "2026-10-11_siena".into(),
                    opponent: "Siena".into(),
                    home: false,
                    date: "2026-10-11T21:00:00+02:00".into(),
                    won: 0,
                    lost: 0,
                    finished: false,
                },
                MatchListing::Match {
                    id: "2026-10-04_arezzo".into(),
                    opponent: "Arezzo".into(),
                    home: true,
                    date: "2026-10-04T18:00:00+02:00".into(),
                    won: 0,
                    lost: 0,
                    finished: false,
                },
                MatchListing::Unreadable {
                    folder: "not-a-match".into(),
                    reason: None,
                },
            ]
        );
    }

    fn match_form(opponent: &str, date: &str, max_substitutions: &str) -> MatchForm {
        MatchForm {
            opponent: opponent.into(),
            date: DateTime::parse_from_rfc3339(date).ok(),
            home: true,
            max_substitutions: max_substitutions.into(),
            notes: String::new(),
            roster: None,
        }
    }

    #[test]
    fn matches_are_created_and_edited() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
        let players: Vec<Uuid> = (1..=7)
            .map(|n| {
                app.save_player(team.clone(), None, player(&format!("P{n}"), &n.to_string()))
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
        assert_eq!(id, "2026-10-24_arezzo");
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
            app.save_player(team.clone(), None, player).unwrap();
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
                app.save_player(team.clone(), None, player(&format!("P{n}"), &n.to_string()))
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

    /// A team of seven, a match and its first set started, the team serving.
    fn match_with_a_set(app: &Scout4All) -> (String, String) {
        let team = app.save_team(None, form("Cortona")).unwrap();
        for n in 1..=7 {
            app.save_player(team.clone(), None, player(&format!("P{n}"), &n.to_string()))
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
        let set = dir.path().join(&team).join(&m).join("set_1.csv");
        let mut rows = std::fs::read_to_string(&set).unwrap();
        rows.push_str("2026-10-24T18:00:00Z,OF,,,\n");
        std::fs::write(&set, rows).unwrap();
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
    /// events of the players in the front row, saved with its evaluation.
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
        assert_eq!(
            scouting.choose_eval(EvalEnum::Perfect).unwrap(),
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

        let set = scouting.stats(StatsScope::Set { number: 2 }).unwrap();
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
        // the match: (25 aces - 2 errors) / 27 serves; the last 10: 8 aces
        // and 2 errors
        assert_eq!(serves.overall.map(|s| (s * 100.0).round()), Some(8519.0));
        assert_eq!(serves.overall_recent, Some(60.0));
        assert_eq!(
            line(&set, Series::SideOut).sets[1].values,
            vec![None, Some(100.0), Some(100.0)]
        );
        assert_eq!(line(&set, Series::SideOut).sets[0].values, vec![None; 25]);

        let first = scouting.stats(StatsScope::Set { number: 1 }).unwrap();
        assert_eq!(serve(&first), ("battuta".into(), 25, Some(100.0), 25, 0));
        assert_eq!(first.aces, 25);
        assert_eq!(phase(&first.break_points), Some((25, 25)));
        assert_eq!(first.side_out, None);
        assert_eq!(serve_grade(&first), Some(1.0));
        // the trend is the same whatever the scope
        assert_eq!(first.trend, set.trend);

        // the match: the trend goes on across the sets
        let all = scouting.stats(StatsScope::Match).unwrap();
        assert_eq!(serve(&all).1, 27);
        assert_eq!(serve(&all).3, 25);
        assert_eq!(all.aces, 25);
        assert_eq!(phase(&all.break_points), Some((25, 27)));
        assert_eq!(all.trend, set.trend);

        // a set not played yet: nothing
        let none = scouting.stats(StatsScope::Set { number: 3 }).unwrap();
        assert!(
            none.fundamentals
                .iter()
                .all(|f| f.total == 0 && f.efficiency.is_none())
        );
        assert_eq!(none.trend, set.trend);
        assert_eq!(none.sets, vec![1, 2]);

        // from the page of the match, the same as while scouting
        let saved = |scope| app.match_stats(team.clone(), m.clone(), scope).unwrap();
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
            player: Some(f.players[1].player.id.clone()),
            ..DetailFilter::default()
        };
        let theirs = detail(StatsScope::Match, EventTypeEnum::S, &second)
            .fundamental
            .unwrap();
        assert_eq!((theirs.total, theirs.phases_per_point), (1, None));
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
            rotation: Some(0),
            ..DetailFilter::default()
        };
        assert_eq!(
            detail(StatsScope::Match, EventTypeEnum::S, &first)
                .fundamental
                .unwrap()
                .total,
            26
        );

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
                StatsScope::Match,
                EventTypeEnum::S,
                DetailFilter {
                    player: Some("not an id".into()),
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
        let rotations = scouting.stats(StatsScope::Match).unwrap().rotations;
        let won = |p: &Option<PhaseStats>| p.as_ref().map(|p| (p.won, p.rallies));
        let played: Vec<_> = rotations
            .iter()
            .filter(|r| r.side_out.is_some() || r.break_points.is_some())
            .map(|r| (r.name.as_str(), won(&r.side_out), won(&r.break_points)))
            .collect();
        assert_eq!(played.len(), 2);
        assert_eq!(played[0], ("S1", Some((1, 1)), Some((25, 26))));
        assert_eq!((played[1].1, played[1].2), (None, Some((0, 1))));
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

    #[test]
    fn the_statistics_of_a_match_without_sets_are_empty() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "en".into()).unwrap();
        let (team, m) = match_with_a_set(&app);
        let stats = app.match_stats(team, m, StatsScope::Match).unwrap();
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
        let folder = dir.path().join(&team);
        let match_folder = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.is_dir())
            .unwrap();
        let it = labels(LanguageEnum::It);
        std::fs::write(match_folder.join("set_1.json"), "").unwrap();
        assert_eq!(
            reason(app.scouting(team.clone(), m.clone()).err().unwrap()),
            it.unreadable_match
        );
        std::fs::write(match_folder.join("match.json"), "{ not json").unwrap();
        assert_eq!(
            reason(app.match_details(team.clone(), m.clone()).unwrap_err()),
            it.unreadable_match
        );
        std::fs::write(folder.join("team.json"), "{}").unwrap();
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
        app.save_player(team.clone(), None, player("Anna", "9"))
            .unwrap();
        match app.save_player(team, None, player("Bea", "9")) {
            Err(MobileError::InvalidPlayer { field, reason }) => {
                assert_eq!(field, PlayerField::Number);
                assert_eq!(reason, "number already in use");
            }
            other => unreachable!("expected a number in use, got {other:?}"),
        }
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
            team
        );
        let it = labels(LanguageEnum::It);
        assert_eq!(
            reason(to.preview_archive(team_zip.bytes, None, None)),
            it.team_already_exists
        );
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
            m
        );
        assert_eq!(
            to.match_details(team.clone(), m.clone()).unwrap(),
            from.match_details(team, m).unwrap()
        );
        assert_eq!(
            reason(to.preview_archive(match_zip.bytes, None, None)),
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
        let team = app.import_team(OLD_TEAM.to_vec(), file_name).unwrap();
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
        let details = app.match_details(team, m).unwrap();
        assert_eq!(details.opponent, "Club Arezzo");
        assert_eq!(
            details.sets,
            vec![SetScore {
                number: 1,
                us: 3,
                them: 1
            }]
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

    fn queued(dir: &tempfile::TempDir) -> Vec<(String, u64)> {
        std::fs::read(dir.path().join("upload_queue.json"))
            .map(|bytes| {
                from_slice::<Value>(&bytes).unwrap()["pending"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| {
                        (
                            p["match_id"].as_str().unwrap().to_string(),
                            p["retry_count"].as_u64().unwrap(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
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
    fn a_correction_of_a_finished_match_queues_it_again() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "it");
        app.accept_terms().unwrap();
        let (team, m) = won_match(&app);
        // sent in the meantime
        std::fs::remove_file(dir.path().join("upload_queue.json")).unwrap();
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

    /// Three sets won by the home team ("Home", players 1 to 7, the libero
    /// 7) against "Visiting", on 2015-01-25: the fixture of the TUI.
    const THREE_SETS: &[u8] = include_bytes!("../../src/tests/fixtures/three_sets.dvw");

    fn dvw_choice(side: DvwSide, team_id: Option<String>) -> DvwChoice {
        DvwChoice {
            side,
            day: "2015-01-25".into(),
            max_substitutions: 6,
            team_id,
            classification: Some(TeamClassificationEnum::Regional),
            gender: Some(GenderEnum::Women),
            country: CountryCode::known("SI"),
        }
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
        assert_eq!(app.dvw_max_substitutions(Some("2025-12-31".into())), 6);
        assert_eq!(app.dvw_max_substitutions(Some("2026-01-01".into())), 8);
        assert_eq!(app.dvw_max_substitutions(None), 8);
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
            app.dvw_similar_teams(THREE_SETS.to_vec(), DvwSide::Home)
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
                saved_already: false,
            }
        );
        assert!(app.teams().unwrap().is_empty(), "checking saves nothing");
        let imported = app.import_dvw(THREE_SETS.to_vec(), choice, false).unwrap();
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
    /// saved already, overwritten only when confirmed.
    #[test]
    fn a_datavolley_team_into_a_similar_team() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir, "en");
        let first = app
            .import_dvw(
                THREE_SETS.to_vec(),
                dvw_choice(DvwSide::Visiting, None),
                false,
            )
            .unwrap();
        let similar = app
            .dvw_similar_teams(THREE_SETS.to_vec(), DvwSide::Visiting)
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
        let ready = app.plan_dvw(THREE_SETS.to_vec(), choice.clone()).unwrap();
        assert_eq!((ready.new_players, ready.saved_already), (0, true));
        // not overwritten unless confirmed
        reason(app.import_dvw(THREE_SETS.to_vec(), choice.clone(), false));
        let again = app.import_dvw(THREE_SETS.to_vec(), choice, true).unwrap();
        assert_eq!(again, first);
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
                app.import_dvw(THREE_SETS.to_vec(), choice, false)
                    .map(|_| ()),
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
        assert!(app.import_dvw(text.into_bytes(), choice, false).is_err());
        assert!(app.teams().unwrap().is_empty());
    }
}
