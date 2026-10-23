//! The core of scout4all for the native apps: UniFFI generates the Kotlin
//! and Swift bindings of what's exported here.
//!
//! The API is blocking for now: the app calls it off the main thread
//! (`Dispatchers.IO` on Android) and a tokio runtime here drives the async
//! providers of the core. The final app must never block the UI: the plan is
//! to move to the `async fn` of UniFFI.

use chrono::{DateTime, FixedOffset};
use scout4all_core::{
    errors::AppError,
    localization::{Labels, Language},
    logging::logger::init_logger,
    match_form::{MatchForm, MatchFormError},
    player_form::{PlayerForm, PlayerFormError, removal},
    providers::{
        match_reader::MatchReader,
        match_writer::MatchWriter,
        set_writer::SetWriter,
        store::{
            match_reader::StoreMatchReader, match_writer::StoreMatchWriter,
            set_reader::StoreSetReader, set_writer::StoreSetWriter, team_reader::StoreTeamReader,
            team_writer::StoreTeamWriter,
        },
        team_reader::TeamReader,
        team_writer::TeamWriter,
    },
    scouting::{ChoiceOrder, PlayerChoice, Progress, ScoutingError, ScoutingSession, ScoutingStep},
    set_start::{self, MIN_PLAYERS_FOR_FALLBACK_LIBERO, NextSet, SetStartForm, next_set},
    shapes::{
        enums::{
            EvalEnum, EventTypeEnum, FriendlyName, GenderEnum, LanguageEnum, PhaseEnum, RoleEnum,
            TeamClassificationEnum, TeamSideEnum,
        },
        r#match::MatchEntry,
        player::PlayerEntry,
        snapshot::EventEntry,
        team::TeamEntry,
        technique::{ServeTypeEnum, SetTypeEnum, TechniqueEnum},
    },
    storage::{Store, fs::FsStore},
    team_form::{TeamForm, TeamFormError},
};
use std::{
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex, MutexGuard},
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
}

#[uniffi::remote(Record)]
pub struct TeamForm {
    pub name: String,
    pub classification: Option<TeamClassificationEnum>,
    pub gender: Option<GenderEnum>,
    pub year: String,
}

#[uniffi::remote(Record)]
pub struct MatchForm {
    pub opponent: String,
    pub date: Option<MatchDate>,
    pub home: bool,
    pub max_substitutions: String,
}

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
        MobileError::Failed {
            reason: e.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TeamField {
    Name,
    Classification,
    Gender,
    Year,
}

impl From<TeamFormError> for TeamField {
    fn from(e: TeamFormError) -> Self {
        match e {
            TeamFormError::EmptyName => TeamField::Name,
            TeamFormError::MissingClassification => TeamField::Classification,
            TeamFormError::MissingGender => TeamField::Gender,
            TeamFormError::InvalidYear => TeamField::Year,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PlayerField {
    Name,
    Role,
    Number,
}

impl From<PlayerFormError> for PlayerField {
    fn from(e: PlayerFormError) -> Self {
        match e {
            PlayerFormError::EmptyName => PlayerField::Name,
            PlayerFormError::MissingRole => PlayerField::Role,
            PlayerFormError::InvalidNumber | PlayerFormError::NumberInUse => PlayerField::Number,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MatchField {
    Opponent,
    Date,
    MaxSubstitutions,
}

impl From<MatchFormError> for MatchField {
    fn from(e: MatchFormError) -> Self {
        match e {
            MatchFormError::EmptyOpponent => MatchField::Opponent,
            MatchFormError::InvalidDate => MatchField::Date,
            MatchFormError::InvalidMaxSubstitutions | MatchFormError::MaxSubstitutionsBelowUsed => {
                MatchField::MaxSubstitutions
            }
        }
    }
}

/// A role as the form offers it.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct RoleChoice {
    pub value: RoleEnum,
    pub name: String,
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
    },
}

/// The points of a set, the team's first.
#[derive(Debug, PartialEq, uniffi::Record)]
pub struct SetScore {
    pub number: u8,
    pub us: u8,
    pub them: u8,
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
}

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
    pub name: String,
    pub description: Option<String>,
}

/// A set or serve to save with the evaluation.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TechniqueChoice {
    pub value: TechniqueEnum,
    /// e.g. "A (high)" or "F (float)"
    pub name: String,
    /// what the evaluation saves now, chosen or preselected
    pub chosen: bool,
}

/// An event saved in the set.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ScoutedEvent {
    pub event: EventTypeEnum,
    pub name: String,
    pub player: Option<ScoutingPlayer>,
    pub eval: Option<String>,
    /// e.g. "A" or "F"
    pub technique: Option<String>,
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
    /// the players in positions 1 to 6
    pub court: Vec<CourtPlayer>,
    pub substitutions_left: u8,
    pub step: ScoutingStep,
    /// the event being chosen, and its player once chosen
    pub event: Option<EventChoice>,
    pub player: Option<ScoutingPlayer>,
    /// the events of the set, the last first
    pub events: Vec<ScoutedEvent>,
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
        let court = (0..6)
            .filter_map(|position| {
                lineup
                    .get(position)
                    .and_then(|id| team.find_player(id))
                    .map(|p| CourtPlayer {
                        id: p.id.to_string(),
                        name: p.name.clone(),
                        number: p.number,
                        setter: lineup.has_setter_at_pos(position),
                        libero: lineup.has_libero_at_pos(position),
                        selectable: selectable(&p.id),
                    })
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
            court,
            substitutions_left: lineup
                .get_max_substitutions()
                .saturating_sub(u8::try_from(lineup.get_substitutions().len()).unwrap_or(u8::MAX)),
            step: session.step(),
            event: session.event().map(|e| self.event_choice(e)),
            player: session
                .player()
                .and_then(|id| team.find_player(id))
                .map(|p| self.scouting_player(p)),
            events: events
                .into_iter()
                .rev()
                .map(|e| ScoutedEvent {
                    event: e.event_type,
                    name: e.event_type.friendly_name(self.labels).to_string(),
                    player: e
                        .player
                        .and_then(|id| team.find_player(id))
                        .map(|p| self.scouting_player(p)),
                    eval: e.eval.map(|v| v.friendly_name(e.event_type, self.labels)),
                    technique: e.technique.map(|t| t.to_string()),
                })
                .collect(),
        }
    }

    /// The events that can follow.
    pub fn available_events(&self) -> Vec<EventChoice> {
        self.session()
            .available_events()
            .iter()
            .map(|e| self.event_choice(*e))
            .collect()
    }

    /// The events the player can play now, before an event is chosen.
    pub fn player_events(&self, player: String) -> Result<Vec<EventChoice>, MobileError> {
        let player = parse_id(&player)?;
        Ok(self
            .session()
            .player_events(&player)
            .into_iter()
            .map(|e| self.event_choice(e))
            .collect())
    }

    /// The events that come before their player: the opponent's, the
    /// faults, the changes of the lineup.
    pub fn other_events(&self) -> Vec<EventChoice> {
        self.session()
            .other_events()
            .into_iter()
            .map(|e| self.event_choice(e))
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
                    .map(|eval| EvalChoice {
                        value: eval,
                        name: eval.friendly_name(event, self.labels),
                        description: eval.friendly_description(event, self.labels),
                    })
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
            .map(|t| TechniqueChoice {
                value: t,
                name: t.description(self.labels),
                chosen: session.technique() == Some(t),
            })
            .collect()
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

    pub fn choose_replacement(&self, player: String) -> Result<Progress, MobileError> {
        let player = parse_id(&player)?;
        let mut session = self.session();
        self.scouted(self.runtime.block_on(session.choose_replacement(player)))
    }

    pub fn choose_technique(&self, technique: TechniqueEnum) -> Result<(), MobileError> {
        let chosen = self.session().choose_technique(technique);
        self.scouted(chosen)
    }

    pub fn choose_eval(&self, eval: EvalEnum) -> Result<Progress, MobileError> {
        let mut session = self.session();
        self.scouted(self.runtime.block_on(session.choose_eval(eval)))
    }

    /// Goes back a choice, or removes the last saved event.
    pub fn undo(&self) -> Result<(), MobileError> {
        let mut session = self.session();
        self.scouted(self.runtime.block_on(session.undo()))
    }
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

    fn event_choice(&self, event: EventTypeEnum) -> EventChoice {
        EventChoice {
            value: event,
            name: event.friendly_name(self.labels).to_string(),
        }
    }

    fn scouting_player(&self, p: &PlayerEntry) -> ScoutingPlayer {
        ScoutingPlayer {
            id: p.id.to_string(),
            name: p.name.clone(),
            number: p.number,
            role: p
                .role
                .map_or_else(String::new, |r| r.friendly_name(self.labels).to_string()),
        }
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
}

#[uniffi::export]
impl Scout4All {
    /// The data in `data_dir`; `language` is a code like `"it"`, English when
    /// it isn't supported.
    #[uniffi::constructor]
    pub fn new(data_dir: String, language: String) -> Result<Arc<Self>, MobileError> {
        let data_dir = PathBuf::from(data_dir);
        init_logger(data_dir.join("scout4all.log"));
        let language = Language::new(LanguageEnum::from_str(&language).unwrap_or(LanguageEnum::En));
        let store: Arc<dyn Store> = Arc::new(FsStore::new(&data_dir));
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| MobileError::Failed {
                reason: e.to_string(),
            })?;
        Ok(Arc::new(Self {
            runtime: Arc::new(runtime),
            team_reader: StoreTeamReader::new(store.clone(), language.clone()),
            team_writer: StoreTeamWriter::new(store.clone()),
            match_writer: StoreMatchWriter::new(store.clone()),
            set_writer: Arc::new(StoreSetWriter::new(store.clone(), language.clone())),
            match_reader: StoreMatchReader::new(
                store.clone(),
                Arc::new(StoreSetReader::new(store, language.clone())),
                language.clone(),
            ),
            language,
        }))
    }

    /// The teams, sorted by name.
    pub fn teams(&self) -> Result<Vec<TeamListing>, MobileError> {
        let mut teams: Vec<TeamListing> = self
            .runtime
            .block_on(self.team_reader.read_all())?
            .into_iter()
            .map(|listed| match listed {
                Ok(team) => TeamListing::Team {
                    id: team.id.to_string(),
                    players: count(&team),
                    name: team.name,
                    year: team.year,
                },
                Err(unreadable) => TeamListing::Unreadable {
                    folder: unreadable.id,
                },
            })
            .collect();
        teams.sort_by_key(|t| match t {
            TeamListing::Team { name, .. } => name.to_lowercase(),
            TeamListing::Unreadable { folder } => folder.to_lowercase(),
        });
        Ok(teams)
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
            players: players(&team, labels),
            form: TeamForm::of(&team),
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
                Ok((Err(_), m)) => MatchListing::Unreadable { folder: m.id },
                Err(unreadable) => MatchListing::Unreadable {
                    folder: unreadable.id,
                },
            })
            .collect())
    }

    /// A match of the team, with its sets.
    pub fn match_details(
        &self,
        team_id: String,
        match_id: String,
    ) -> Result<MatchDetails, MobileError> {
        let m = self.read_match(&team_id, &match_id)?;
        let status = m.get_status()?;
        let mut numbers: Vec<u8> = m.sets.iter().map(|s| s.set_number).collect();
        numbers.sort_unstable();
        let sets = numbers
            .into_iter()
            .map(|number| {
                m.get_set_snapshot(number).map(|snapshot| SetScore {
                    number,
                    us: snapshot.as_ref().map_or(0, |s| s.score_us),
                    them: snapshot.as_ref().map_or(0, |s| s.score_them),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let next = match next_set(&status) {
            NextSet::Start(form) => NextSetKind::Start {
                set_number: form.set_number,
            },
            NextSet::Resume(set) => NextSetKind::Resume {
                set_number: set.set_number,
            },
            NextSet::Over => NextSetKind::Over,
        };
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
        })
    }

    /// The start of the next set of the match, to choose who serves and the
    /// lineup.
    pub fn set_start(
        &self,
        team_id: String,
        match_id: String,
    ) -> Result<Arc<SetStart>, MobileError> {
        let m = self.read_match(&team_id, &match_id)?;
        match next_set(&m.get_status()?) {
            NextSet::Start(form) => Ok(Arc::new(SetStart {
                team: m.team,
                match_id: m.id,
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
        match next_set(&m.get_status()?) {
            NextSet::Resume(set) => set_start::resume(m, set, self.set_writer.clone())
                .map(|session| {
                    Arc::new(Scouting {
                        runtime: self.runtime.clone(),
                        session: Mutex::new(session.with_order(ChoiceOrder::PlayerFirst)),
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
        let input = form
            .validate(existing.as_ref())
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
                ))
                .map(|m| m.id),
        };
        Ok(saved?)
    }

    /// An empty form for a new team, with the year given.
    pub fn new_team_form(&self, year: u16) -> TeamForm {
        TeamForm {
            name: String::new(),
            classification: None,
            gender: None,
            year: year.to_string(),
        }
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
        let input =
            form.validate(&team, existing.as_ref())
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
        Ok(team.id.to_string())
    }
}

impl Scout4All {
    fn read_team(&self, id: &str) -> Result<TeamEntry, MobileError> {
        let id = parse_id(id)?;
        Ok(self.runtime.block_on(self.team_reader.read_single(&id))?)
    }

    fn read_match(&self, team_id: &str, match_id: &str) -> Result<MatchEntry, MobileError> {
        let team = self.read_team(team_id)?;
        Ok(self
            .runtime
            .block_on(self.match_reader.read_single(&team, match_id))?)
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

    fn form(name: &str) -> TeamForm {
        TeamForm {
            name: name.into(),
            classification: Some(TeamClassificationEnum::Regional),
            gender: Some(GenderEnum::Women),
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
            }]
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
                .block_on(writer.create(&team, opponent.into(), date, home, 6))
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
                    folder: "not-a-match".into()
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
        }
    }

    #[test]
    fn matches_are_created_and_edited() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = app.save_team(None, form("Cortona")).unwrap();
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
        assert_eq!(app.match_form(team.clone(), id.clone()).unwrap(), edited);
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
                .any(|e| e.value == EvalEnum::Perfect && e.name == "ace")
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
        scouting.choose_eval(EvalEnum::Positive).unwrap();
        let state = scouting.state();
        assert_eq!((state.step, state.player), (ScoutingStep::Event, None));
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
}
