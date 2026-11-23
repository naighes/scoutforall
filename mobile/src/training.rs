//! The training of a team for the apps (TODO/021): sessions, activities,
//! attendance, series, duplicates, cycles, the agenda, the totals and the
//! sheets. The dates travel as text: `YYYY-MM-DD` a day, `YYYY-MM-DDTHH:MM`
//! a moment (local, without offset, as the core keeps them).
use super::{MobileError, Scout4All, failed, parse_id};
use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use scout4all_core::{
    errors::AppError,
    providers::{
        sqlite::{persons::SqlitePersons, training::SqliteTraining, venues::SqliteVenues},
        training_agenda::TrainingAgenda,
        training_cycles::TrainingCycles,
        training_reader::TrainingReader,
        training_totals::TrainingTotals,
        training_writer::TrainingWriter,
    },
    reporting::{
        pdf::render_training_pdf,
        training::{Context, SessionOptions, microcycle_sheet, session_sheet},
    },
    training::{
        Activity, ActivityHit, ActivityStatus, AttendanceInput, AttendanceRow, AttendanceStatus,
        Move, SessionInput, SessionStatus, TrainingSession,
        agenda::{AgendaEvent, AgendaItem, MatchState, Period, busy_days, by_day},
        cycles::{
            Child, Cycle, CycleForm, Level, Span, Window, proposed_container, proposed_microcycle,
        },
        duplicate::{DuplicateCycleInput, DuplicateInput},
        form::{ActivityForm, ActivityFormError, SessionForm, SessionFormError},
        overlap::{Slot, SlotKind},
        series::{Band, SeriesEdit, SeriesInput, SeriesScope},
        totals::{AttendanceReport, Scope, Totals},
        venue::Venue,
    },
};
use uuid::Uuid;

const MOMENT: &str = "%Y-%m-%dT%H:%M";
const DAY: &str = "%Y-%m-%d";
/// The activities a search gives at most.
const ACTIVITY_HITS: usize = 20;

#[uniffi::remote(Enum)]
pub enum SessionStatus {
    Planned,
    Completed,
    Cancelled,
}

#[uniffi::remote(Enum)]
pub enum ActivityStatus {
    Planned,
    Completed,
    Skipped,
}

#[uniffi::remote(Enum)]
pub enum AttendanceStatus {
    Present,
    Absent,
    Late,
    Unavailable,
}

#[uniffi::remote(Enum)]
pub enum Move {
    Up,
    Down,
}

#[uniffi::remote(Enum)]
pub enum Level {
    Program,
    Mesocycle,
    Microcycle,
}

#[uniffi::remote(Enum)]
pub enum SeriesScope {
    This,
    ThisAndFollowing,
    All,
}

/// The field of a form of the training a message is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TrainingField {
    Start,
    Duration,
    ActualDuration,
    Name,
    Planned,
    Actual,
    Weeks,
    End,
    Range,
    Bands,
    /// the name of a gym
    VenueName,
    /// the latitude and longitude of a gym
    Coordinates,
    /// the link of the map of a gym
    MapLink,
}

/// A session, as saved.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SessionRecord {
    pub id: String,
    pub team_id: String,
    pub microcycle_id: Option<String>,
    pub series_id: Option<String>,
    pub start: String,
    pub duration_min: u16,
    pub actual_duration_min: Option<u16>,
    /// the gym, if any (it may be a removed one)
    pub venue_id: Option<String>,
    pub title: String,
    pub objectives: String,
    pub notes: String,
    pub final_remarks: String,
    pub status: SessionStatus,
    /// planned and over: to complete or cancel
    pub to_close: bool,
}

/// What the user typed for a session.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SessionDraft {
    pub start: Option<String>,
    pub duration: String,
    pub actual_duration: String,
    /// the id of the gym, if any
    pub venue: Option<String>,
    pub title: String,
    pub objectives: String,
    pub notes: String,
    pub final_remarks: String,
    pub microcycle: Option<String>,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ActivityRecord {
    pub id: String,
    pub session_id: String,
    pub position: u16,
    pub name: String,
    pub description: String,
    pub planned_min: Option<u16>,
    pub actual_min: Option<u16>,
    pub notes: String,
    pub status: ActivityStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ActivityDraft {
    pub name: String,
    pub description: String,
    pub planned: String,
    pub actual: String,
    pub notes: String,
    pub status: ActivityStatus,
}

/// An activity written before, to copy.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ActivityHitRecord {
    pub name: String,
    pub description: String,
    pub planned_min: Option<u16>,
    pub team_id: String,
    pub team_name: String,
    pub session_start: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AttendanceRowRecord {
    pub person_id: String,
    /// empty when the person was removed from the registry
    pub name: String,
    pub number: Option<u8>,
    pub status: AttendanceStatus,
    pub note: String,
    pub guest: bool,
    /// a guest who makes up for a session of their own team
    pub makeup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AttendanceSheetRecord {
    /// written at least once: otherwise the rows are the roster, all
    /// present, still to be saved
    pub recorded: bool,
    pub rows: Vec<AttendanceRowRecord>,
}

/// A player of another team who can be a guest, with where they play.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GuestChoice {
    pub person_id: String,
    pub name: String,
    /// the name and number in their teams
    pub teams: Vec<GuestTeam>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct GuestTeam {
    pub name: String,
    pub year: u16,
    pub number: u8,
}

/// A band of a series: a weekday (1 Monday … 7 Sunday), the time (`HH:MM`)
/// and the minutes.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SeriesBand {
    pub weekday: u8,
    pub time: String,
    pub duration_min: u16,
    /// the id of the gym of this band, if any
    pub venue: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SeriesDraft {
    pub from: String,
    pub to: String,
    pub bands: Vec<SeriesBand>,
}

/// A slot of the team: a session or a match.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SlotRecord {
    pub is_match: bool,
    pub start: String,
    pub minutes: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SeriesConflictRecord {
    pub start: String,
    pub minutes: u16,
    pub with: Vec<SlotRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SeriesResult {
    pub created: Vec<SessionRecord>,
    pub conflicts: Vec<SeriesConflictRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DuplicateDraft {
    pub start: String,
    pub microcycle_id: Option<String>,
    pub copy_objectives: bool,
    pub copy_activities: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DuplicateCycleDraft {
    pub shift_weeks: u16,
    pub copies: u16,
    pub copy_objectives: bool,
    pub copy_activities: bool,
}

/// A program, a mesocycle or a microcycle.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CycleRecord {
    pub id: String,
    pub team_id: String,
    pub level: Level,
    pub parent_id: Option<String>,
    pub title: String,
    /// the weeks of a mesocycle or a microcycle; none for a program
    pub weeks: Option<u16>,
    pub first_day: String,
    /// the last day, included
    pub last_day: String,
    /// the window as it reads: `from – to`
    pub window: String,
    pub objectives: String,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CycleDraft {
    pub title: String,
    pub start: Option<String>,
    /// for a mesocycle and a microcycle
    pub weeks: String,
    /// the last day, for a program
    pub end: Option<String>,
    pub objectives: String,
    pub notes: String,
    pub parent: Option<String>,
}

/// What a container holds, or can hold: a cycle or a session by its window.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChildRecord {
    pub id: String,
    pub first_day: String,
    pub last_day: String,
    pub window: String,
}

/// How long the agenda looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AgendaSpan {
    Day,
    /// Monday to Sunday
    Week,
    Month,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum AgendaItemRecord {
    Session {
        id: String,
        title: String,
        status: SessionStatus,
        to_close: bool,
    },
    Match {
        id: String,
        opponent: String,
        opponent_team: Option<String>,
        played: bool,
        /// the points sets of the team and of the opponent, once played
        ours: Option<u8>,
        theirs: Option<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AgendaEventRecord {
    pub team_id: String,
    pub team_name: String,
    pub start: String,
    pub minutes: u16,
    pub item: AgendaItemRecord,
    pub venue: Option<VenueRefRecord>,
    /// another team holds the same gym at the same time
    pub shared_venue: bool,
}

/// The gym of an event.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VenueRefRecord {
    pub id: String,
    pub name: String,
    /// removed: the sessions and matches keep it
    pub deleted: bool,
}

/// A day of the agenda, empty too.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AgendaDay {
    pub date: String,
    pub events: Vec<AgendaEventRecord>,
}

/// What the totals and the attendance count.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum TrainingScope {
    All,
    /// the sessions attached to the cycle
    Cycle {
        level: Level,
        id: String,
    },
    /// the sessions that start between the days, both included
    Dates {
        from: String,
        to: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TotalsRecord {
    pub planned: u32,
    pub completed: u32,
    pub cancelled: u32,
    pub planned_minutes: u32,
    pub actual_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlayerAttendanceRecord {
    pub person_id: String,
    pub name: String,
    pub number: Option<u8>,
    pub present: u32,
    pub late: u32,
    pub absent: u32,
    pub unavailable: u32,
    /// of the presences, the makeups that took the place of an absence
    pub makeups: u32,
    /// none without sessions to count
    pub percent: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AttendanceReportRecord {
    pub sessions: u32,
    pub players: Vec<PlayerAttendanceRecord>,
}

// the records of the core

pub(crate) fn moment(at: NaiveDateTime) -> String {
    at.format(MOMENT).to_string()
}

pub(crate) fn day(on: NaiveDate) -> String {
    on.format(DAY).to_string()
}

pub(crate) fn parse_moment(text: &str) -> Result<NaiveDateTime, MobileError> {
    NaiveDateTime::parse_from_str(text.trim(), MOMENT)
        .or_else(|_| NaiveDateTime::parse_from_str(text.trim(), "%Y-%m-%dT%H:%M:%S"))
        .map_err(|_| invalid(TrainingField::Start, text))
}

pub(crate) fn parse_day(text: &str, field: TrainingField) -> Result<NaiveDate, MobileError> {
    NaiveDate::parse_from_str(text.trim(), DAY).map_err(|_| invalid(field, text))
}

fn parse_time(text: &str) -> Result<NaiveTime, MobileError> {
    NaiveTime::parse_from_str(text.trim(), "%H:%M").map_err(|_| invalid(TrainingField::Bands, text))
}

/// The input of a series from what the user typed.
pub(crate) fn series_input(draft: &SeriesDraft) -> Result<SeriesInput, MobileError> {
    Ok(SeriesInput {
        from: parse_day(&draft.from, TrainingField::Range)?,
        to: parse_day(&draft.to, TrainingField::Range)?,
        bands: draft
            .bands
            .iter()
            .map(|b| {
                Ok(Band {
                    weekday: weekday(b.weekday)?,
                    time: parse_time(&b.time)?,
                    duration_min: b.duration_min,
                    venue_id: opt_id(&b.venue)?,
                })
            })
            .collect::<Result<Vec<_>, MobileError>>()?,
    })
}

fn invalid(field: TrainingField, text: &str) -> MobileError {
    MobileError::InvalidTraining {
        field,
        reason: format!("invalid value: {text}"),
    }
}

fn weekday(number: u8) -> Result<Weekday, MobileError> {
    match number {
        1 => Ok(Weekday::Mon),
        2 => Ok(Weekday::Tue),
        3 => Ok(Weekday::Wed),
        4 => Ok(Weekday::Thu),
        5 => Ok(Weekday::Fri),
        6 => Ok(Weekday::Sat),
        7 => Ok(Weekday::Sun),
        _ => Err(invalid(TrainingField::Bands, &number.to_string())),
    }
}

fn opt_id(id: &Option<String>) -> Result<Option<Uuid>, MobileError> {
    id.as_deref().map(parse_id).transpose()
}

fn ids(ids: &[String]) -> Result<Vec<Uuid>, MobileError> {
    ids.iter().map(|id| parse_id(id)).collect()
}

fn session_record(s: &TrainingSession, now: NaiveDateTime) -> SessionRecord {
    SessionRecord {
        id: s.id.to_string(),
        team_id: s.team_id.to_string(),
        microcycle_id: s.microcycle_id.map(|id| id.to_string()),
        series_id: s.series_id.map(|id| id.to_string()),
        start: moment(s.start),
        duration_min: s.duration_min,
        actual_duration_min: s.actual_duration_min,
        venue_id: s.venue_id.map(|id| id.to_string()),
        title: s.title.clone(),
        objectives: s.objectives.clone(),
        notes: s.notes.clone(),
        final_remarks: s.final_remarks.clone(),
        status: s.status,
        to_close: s.is_to_close(now),
    }
}

fn activity_record(a: &Activity) -> ActivityRecord {
    ActivityRecord {
        id: a.id.to_string(),
        session_id: a.session_id.to_string(),
        position: a.position,
        name: a.name.clone(),
        description: a.description.clone(),
        planned_min: a.planned_min,
        actual_min: a.actual_min,
        notes: a.notes.clone(),
        status: a.status,
    }
}

fn hit_record(h: &ActivityHit) -> ActivityHitRecord {
    ActivityHitRecord {
        name: h.name.clone(),
        description: h.description.clone(),
        planned_min: h.planned_min,
        team_id: h.team_id.to_string(),
        team_name: h.team_name.clone(),
        session_start: moment(h.session_start),
    }
}

fn attendance_record(r: &AttendanceRow) -> AttendanceRowRecord {
    AttendanceRowRecord {
        person_id: r.person_id.to_string(),
        name: r.name.clone(),
        number: r.number,
        status: r.status,
        note: r.note.clone(),
        guest: r.guest,
        makeup: r.makeup,
    }
}

fn slot_record(s: &Slot) -> SlotRecord {
    SlotRecord {
        is_match: s.kind == SlotKind::Match,
        start: moment(s.start),
        minutes: s.minutes,
    }
}

fn cycle_record(c: &Cycle) -> CycleRecord {
    CycleRecord {
        id: c.id.to_string(),
        team_id: c.team_id.to_string(),
        level: c.level,
        parent_id: c.parent_id.map(|id| id.to_string()),
        title: c.title.clone(),
        weeks: match c.span {
            Span::Weeks(weeks) => Some(weeks),
            Span::Until(_) => None,
        },
        first_day: day(c.window.start),
        last_day: day(c.window.last()),
        window: c.window.text(),
        objectives: c.objectives.clone(),
        notes: c.notes.clone(),
    }
}

fn child_record(c: &Child) -> ChildRecord {
    ChildRecord {
        id: c.id.to_string(),
        first_day: day(c.window.start),
        last_day: day(c.window.last()),
        window: c.window.text(),
    }
}

fn event_record(e: &AgendaEvent) -> AgendaEventRecord {
    AgendaEventRecord {
        team_id: e.team_id.to_string(),
        team_name: e.team_name.clone(),
        start: moment(e.start),
        minutes: e.minutes,
        item: match &e.item {
            AgendaItem::Session {
                id,
                title,
                status,
                to_close,
            } => AgendaItemRecord::Session {
                id: id.to_string(),
                title: title.clone(),
                status: *status,
                to_close: *to_close,
            },
            AgendaItem::Match {
                id,
                opponent,
                opponent_team,
                state,
            } => AgendaItemRecord::Match {
                id: id.to_string(),
                opponent: opponent.clone(),
                opponent_team: opponent_team.map(|t| t.to_string()),
                played: matches!(state, MatchState::Played(_)),
                ours: match state {
                    MatchState::Played(Some((ours, _))) => Some(*ours),
                    MatchState::Played(None) | MatchState::Scheduled => None,
                },
                theirs: match state {
                    MatchState::Played(Some((_, theirs))) => Some(*theirs),
                    MatchState::Played(None) | MatchState::Scheduled => None,
                },
            },
        },
        venue: e.venue.as_ref().map(|v| VenueRefRecord {
            id: v.id.to_string(),
            name: v.name.clone(),
            deleted: v.deleted,
        }),
        shared_venue: e.shared_venue,
    }
}

fn totals_record(t: &Totals) -> TotalsRecord {
    TotalsRecord {
        planned: t.planned,
        completed: t.completed,
        cancelled: t.cancelled,
        planned_minutes: t.planned_minutes,
        actual_minutes: t.actual_minutes,
    }
}

fn report_record(r: &AttendanceReport) -> AttendanceReportRecord {
    AttendanceReportRecord {
        sessions: r.sessions,
        players: r
            .players
            .iter()
            .map(|p| PlayerAttendanceRecord {
                person_id: p.person_id.to_string(),
                name: p.name.clone(),
                number: p.number,
                present: p.present,
                late: p.late,
                absent: p.absent,
                unavailable: p.unavailable,
                makeups: p.makeups,
                percent: p.percent(),
            })
            .collect(),
    }
}

fn session_form_field(e: SessionFormError) -> TrainingField {
    match e {
        SessionFormError::InvalidStart => TrainingField::Start,
        SessionFormError::InvalidDuration => TrainingField::Duration,
        SessionFormError::InvalidActualDuration => TrainingField::ActualDuration,
    }
}

fn activity_form_field(e: ActivityFormError) -> TrainingField {
    match e {
        ActivityFormError::EmptyName => TrainingField::Name,
        ActivityFormError::InvalidPlanned => TrainingField::Planned,
        ActivityFormError::InvalidActual => TrainingField::Actual,
    }
}

#[uniffi::export]
impl Scout4All {
    // ---- sessions

    /// The sessions of the team, by start.
    pub fn training_sessions(&self, team_id: String) -> Result<Vec<SessionRecord>, MobileError> {
        let now = Local::now().naive_local();
        let sessions = self
            .runtime
            .block_on(self.training().sessions(&parse_id(&team_id)?))?;
        Ok(sessions.iter().map(|s| session_record(s, now)).collect())
    }

    pub fn training_session(&self, session_id: String) -> Result<SessionRecord, MobileError> {
        let session = self
            .runtime
            .block_on(self.training().session(&parse_id(&session_id)?))?;
        Ok(session_record(&session, Local::now().naive_local()))
    }

    /// A form for a new session: the usual duration, planned.
    pub fn new_session_draft(&self) -> SessionDraft {
        session_draft_of(&SessionForm::default())
    }

    /// The form to edit the session.
    pub fn session_draft(&self, session_id: String) -> Result<SessionDraft, MobileError> {
        let session = self
            .runtime
            .block_on(self.training().session(&parse_id(&session_id)?))?;
        Ok(session_draft_of(&SessionForm::of(&session)))
    }

    /// Creates a session of the team (no `session_id`) or updates the one
    /// with it. A session that overlaps another or a match: `Overlap`.
    pub fn save_session(
        &self,
        team_id: String,
        session_id: Option<String>,
        draft: SessionDraft,
    ) -> Result<SessionRecord, MobileError> {
        let input = self.session_input(draft)?;
        let training = self.training();
        let session = match session_id {
            Some(id) => self
                .runtime
                .block_on(training.update_session(&parse_id(&id)?, input))?,
            None => self
                .runtime
                .block_on(training.create_session(&parse_id(&team_id)?, input))?,
        };
        Ok(session_record(&session, Local::now().naive_local()))
    }

    /// Whether saving the draft (or, with none, removing the session) asks
    /// "this, this and the following, the whole series": the session is in
    /// a series with other planned sessions, and the draft changes the time,
    /// the duration or the gym (what goes to the others).
    pub fn asks_series_scope(
        &self,
        session_id: String,
        draft: Option<SessionDraft>,
    ) -> Result<bool, MobileError> {
        let training = self.training();
        let session = self
            .runtime
            .block_on(training.session(&parse_id(&session_id)?))?;
        let others = match session.series_id {
            None => false,
            Some(series) => self
                .runtime
                .block_on(training.sessions(&session.team_id))?
                .iter()
                .any(|s| {
                    s.series_id == Some(series)
                        && s.id != session.id
                        && s.status == SessionStatus::Planned
                }),
        };
        match (others, draft) {
            (false, _) => Ok(false),
            (true, None) => Ok(true),
            (true, Some(draft)) => {
                let input = self.session_input(draft)?;
                Ok(
                    !SeriesEdit::between(&session, input.start, input.duration_min, input.venue_id)
                        .is_empty(),
                )
            }
        }
    }

    /// Saves the session, and its time, duration and gym to the other
    /// planned sessions of its series the scope reaches. An overlap stops
    /// everything (`Overlap`).
    pub fn save_session_in_series(
        &self,
        session_id: String,
        draft: SessionDraft,
        scope: SeriesScope,
    ) -> Result<SessionRecord, MobileError> {
        let input = self.session_input(draft)?;
        let written = self
            .runtime
            .block_on(self.training().update_session_in_series(
                &parse_id(&session_id)?,
                input,
                scope,
            ))?;
        let now = Local::now().naive_local();
        written
            .first()
            .map(|s| session_record(s, now))
            .ok_or_else(|| failed(format!("session {session_id}")))
    }

    /// Cancels the session, or reopens it (planned).
    pub fn set_session_status(
        &self,
        session_id: String,
        status: SessionStatus,
    ) -> Result<SessionRecord, MobileError> {
        let session = self.runtime.block_on(
            self.training()
                .set_session_status(&parse_id(&session_id)?, status),
        )?;
        Ok(session_record(&session, Local::now().naive_local()))
    }

    /// Completes the session and the planned activities chosen.
    pub fn complete_session(
        &self,
        session_id: String,
        activities: Vec<String>,
    ) -> Result<SessionRecord, MobileError> {
        let session = self.runtime.block_on(
            self.training()
                .complete_session(&parse_id(&session_id)?, ids(&activities)?),
        )?;
        Ok(session_record(&session, Local::now().naive_local()))
    }

    pub fn remove_session(&self, session_id: String) -> Result<(), MobileError> {
        Ok(self
            .runtime
            .block_on(self.training().remove_session(&parse_id(&session_id)?))?)
    }

    // ---- activities

    /// The activities of the session, in order.
    pub fn session_activities(
        &self,
        session_id: String,
    ) -> Result<Vec<ActivityRecord>, MobileError> {
        let activities = self
            .runtime
            .block_on(self.training().activities(&parse_id(&session_id)?))?;
        Ok(activities.iter().map(activity_record).collect())
    }

    pub fn new_activity_draft(&self) -> ActivityDraft {
        activity_draft_of(&ActivityForm::default())
    }

    /// The form to edit an activity of the session.
    pub fn activity_draft(
        &self,
        session_id: String,
        activity_id: String,
    ) -> Result<ActivityDraft, MobileError> {
        let id = parse_id(&activity_id)?;
        let activities = self
            .runtime
            .block_on(self.training().activities(&parse_id(&session_id)?))?;
        activities
            .iter()
            .find(|a| a.id == id)
            .map(|a| activity_draft_of(&ActivityForm::of(a)))
            .ok_or_else(|| failed(format!("activity {activity_id} not found")))
    }

    /// The form of a new activity that copies one found.
    pub fn activity_draft_copying(
        &self,
        hit: ActivityHitRecord,
    ) -> Result<ActivityDraft, MobileError> {
        Ok(activity_draft_of(&ActivityForm {
            name: hit.name,
            description: hit.description,
            planned: hit.planned_min.map(|m| m.to_string()).unwrap_or_default(),
            ..ActivityForm::default()
        }))
    }

    /// The activities written before that match `query`, those of the team
    /// first; only the team's when the setting says so.
    pub fn search_activities(
        &self,
        team_id: String,
        query: String,
    ) -> Result<Vec<ActivityHitRecord>, MobileError> {
        let only_team = self.activity_search_team_only();
        let hits = self.runtime.block_on(self.training().search_activities(
            &parse_id(&team_id)?,
            &query,
            only_team,
            ACTIVITY_HITS,
        ))?;
        Ok(hits.iter().map(hit_record).collect())
    }

    /// Adds an activity at the end of the session (no `activity_id`) or
    /// updates the one with it.
    pub fn save_activity(
        &self,
        session_id: String,
        activity_id: Option<String>,
        draft: ActivityDraft,
    ) -> Result<ActivityRecord, MobileError> {
        let input = ActivityForm {
            name: draft.name,
            description: draft.description,
            planned: draft.planned,
            actual: draft.actual,
            notes: draft.notes,
            status: draft.status,
        }
        .validate()
        .map_err(|e| MobileError::InvalidTraining {
            field: activity_form_field(e),
            reason: e.message(self.language.labels()),
        })?;
        let training = self.training();
        let activity = match activity_id {
            Some(id) => self
                .runtime
                .block_on(training.update_activity(&parse_id(&id)?, input))?,
            None => self
                .runtime
                .block_on(training.add_activity(&parse_id(&session_id)?, input))?,
        };
        Ok(activity_record(&activity))
    }

    pub fn remove_activity(&self, activity_id: String) -> Result<(), MobileError> {
        Ok(self
            .runtime
            .block_on(self.training().remove_activity(&parse_id(&activity_id)?))?)
    }

    pub fn move_activity(&self, activity_id: String, to: Move) -> Result<(), MobileError> {
        Ok(self
            .runtime
            .block_on(self.training().move_activity(&parse_id(&activity_id)?, to))?)
    }

    /// Whether the search of activities looks only at the open team.
    pub fn activity_search_team_only(&self) -> bool {
        self.read_config()
            .map(|config| config.activity_search.activity_search_team_only)
            .unwrap_or_default()
    }

    pub fn set_activity_search_team_only(&self, only_team: bool) -> Result<(), MobileError> {
        self.update_config(|config| super::AppConfig {
            activity_search: scout4all_core::settings::ActivitySearch {
                activity_search_team_only: only_team,
            },
            ..config
        })
    }

    // ---- attendance

    /// The attendance of the session: what was saved, or the active roster
    /// all present.
    pub fn session_attendance(
        &self,
        session_id: String,
    ) -> Result<AttendanceSheetRecord, MobileError> {
        let sheet = self
            .runtime
            .block_on(self.training().attendance(&parse_id(&session_id)?))?;
        Ok(AttendanceSheetRecord {
            recorded: sheet.recorded,
            rows: sheet.rows.iter().map(attendance_record).collect(),
        })
    }

    /// The players that joined the team after the attendance was written.
    pub fn missing_players(
        &self,
        session_id: String,
    ) -> Result<Vec<AttendanceRowRecord>, MobileError> {
        let rows = self
            .runtime
            .block_on(self.training().missing_players(&parse_id(&session_id)?))?;
        Ok(rows.iter().map(attendance_record).collect())
    }

    /// The people of the registry who can be guests of the team's sessions.
    pub fn guest_choices(&self, team_id: String) -> Result<Vec<GuestChoice>, MobileError> {
        let choices = SqlitePersons::new(self.db.clone()).choices(&parse_id(&team_id)?)?;
        Ok(choices
            .into_iter()
            .map(|c| GuestChoice {
                person_id: c.person.id.to_string(),
                name: c.person.name,
                teams: c
                    .teams
                    .into_iter()
                    .map(|t| GuestTeam {
                        name: t.name,
                        year: t.year,
                        number: t.number,
                    })
                    .collect(),
            })
            .collect())
    }

    /// Saves exactly these rows as the attendance of the session.
    pub fn save_attendance(
        &self,
        session_id: String,
        rows: Vec<AttendanceRowRecord>,
    ) -> Result<(), MobileError> {
        let rows = rows
            .into_iter()
            .map(|r| {
                Ok(AttendanceInput::from(&AttendanceRow {
                    person_id: parse_id(&r.person_id)?,
                    name: r.name,
                    number: r.number,
                    status: r.status,
                    note: r.note,
                    guest: r.guest,
                    makeup: r.makeup,
                }))
            })
            .collect::<Result<Vec<_>, MobileError>>()?;
        Ok(self.runtime.block_on(
            self.training()
                .save_attendance(&parse_id(&session_id)?, rows),
        )?)
    }

    // ---- series and duplicates

    /// Creates the sessions of the bands between the days, each draft a
    /// series of its own (an entry of the dialog). With a conflict
    /// and not `create_others` nothing is written and the conflicts are
    /// told; with it the others are created.
    pub fn create_series(
        &self,
        team_id: String,
        drafts: Vec<SeriesDraft>,
        create_others: bool,
    ) -> Result<SeriesResult, MobileError> {
        let inputs = drafts
            .iter()
            .map(series_input)
            .collect::<Result<Vec<_>, _>>()?;
        let outcome = self.runtime.block_on(self.training().create_series(
            &parse_id(&team_id)?,
            inputs,
            create_others,
        ))?;
        let now = Local::now().naive_local();
        Ok(SeriesResult {
            created: outcome
                .created
                .iter()
                .map(|s| session_record(s, now))
                .collect(),
            conflicts: outcome
                .conflicts
                .iter()
                .map(|c| SeriesConflictRecord {
                    start: moment(c.start),
                    minutes: c.minutes,
                    with: c.with.iter().map(slot_record).collect(),
                })
                .collect(),
        })
    }

    /// Removes the session and the other planned sessions of its series the
    /// scope reaches: how many.
    pub fn remove_series(
        &self,
        session_id: String,
        scope: SeriesScope,
    ) -> Result<u32, MobileError> {
        let removed = self.runtime.block_on(
            self.training()
                .remove_series(&parse_id(&session_id)?, scope),
        )?;
        Ok(u32::try_from(removed).unwrap_or(u32::MAX))
    }

    /// What duplicating the session proposes: a week later.
    pub fn proposed_duplicate(&self, session_id: String) -> Result<DuplicateDraft, MobileError> {
        let session = self
            .runtime
            .block_on(self.training().session(&parse_id(&session_id)?))?;
        let input = DuplicateInput::proposed(session.start, session.microcycle_id);
        Ok(DuplicateDraft {
            start: moment(input.start),
            microcycle_id: input.microcycle_id.map(|id| id.to_string()),
            copy_objectives: input.copy_objectives,
            copy_activities: input.copy_activities,
        })
    }

    pub fn duplicate_session(
        &self,
        session_id: String,
        draft: DuplicateDraft,
    ) -> Result<SessionRecord, MobileError> {
        let input = DuplicateInput {
            start: parse_moment(&draft.start)?,
            microcycle_id: opt_id(&draft.microcycle_id)?,
            copy_objectives: draft.copy_objectives,
            copy_activities: draft.copy_activities,
        };
        let session = self.runtime.block_on(
            self.training()
                .duplicate_session(&parse_id(&session_id)?, input),
        )?;
        Ok(session_record(&session, Local::now().naive_local()))
    }

    // ---- cycles

    /// The cycles of the level of the team, by start.
    pub fn cycles(&self, team_id: String, level: Level) -> Result<Vec<CycleRecord>, MobileError> {
        let cycles = self
            .runtime
            .block_on(self.training().cycles(&parse_id(&team_id)?, level))?;
        Ok(cycles.iter().map(cycle_record).collect())
    }

    pub fn cycle(&self, level: Level, id: String) -> Result<CycleRecord, MobileError> {
        let cycle = self
            .runtime
            .block_on(self.training().cycle(level, &parse_id(&id)?))?;
        Ok(cycle_record(&cycle))
    }

    pub fn new_cycle_draft(&self) -> CycleDraft {
        cycle_draft_of(&CycleForm::default())
    }

    /// The form to edit the cycle.
    pub fn cycle_draft(&self, level: Level, id: String) -> Result<CycleDraft, MobileError> {
        let cycle = self
            .runtime
            .block_on(self.training().cycle(level, &parse_id(&id)?))?;
        Ok(cycle_draft_of(&CycleForm::of(&cycle)))
    }

    /// Creates a cycle of the team (no `id`) or updates the one with it.
    /// `children` are the free ones to put in a new cycle (cycles of the
    /// level below, or sessions); `detach` lets an edit leave out the
    /// children that fall out of the new window (`ChildrenOutside` tells).
    pub fn save_cycle(
        &self,
        team_id: String,
        level: Level,
        id: Option<String>,
        draft: CycleDraft,
        children: Vec<String>,
        detach: bool,
    ) -> Result<CycleRecord, MobileError> {
        let form = CycleForm {
            title: draft.title,
            start: draft
                .start
                .as_deref()
                .map(|d| parse_day(d, TrainingField::Start))
                .transpose()?,
            weeks: draft.weeks,
            end: draft
                .end
                .as_deref()
                .map(|d| parse_day(d, TrainingField::End))
                .transpose()?,
            objectives: draft.objectives,
            notes: draft.notes,
            parent: opt_id(&draft.parent)?,
        };
        let input = form
            .validate(level)
            .map_err(|e| MobileError::InvalidTraining {
                field: match e {
                    scout4all_core::training::cycles::CycleFormError::InvalidStart => {
                        TrainingField::Start
                    }
                    scout4all_core::training::cycles::CycleFormError::InvalidWeeks => {
                        TrainingField::Weeks
                    }
                    scout4all_core::training::cycles::CycleFormError::InvalidEnd => {
                        TrainingField::End
                    }
                },
                reason: e.message(self.language.labels()),
            })?;
        let training = self.training();
        let cycle = match id {
            Some(id) => self.runtime.block_on(training.update_cycle(
                level,
                &parse_id(&id)?,
                input,
                detach,
            ))?,
            None => self.runtime.block_on(training.create_cycle(
                &parse_id(&team_id)?,
                level,
                input,
                ids(&children)?,
            ))?,
        };
        Ok(cycle_record(&cycle))
    }

    /// Removes the cycle, with what is under it or leaving it free.
    pub fn remove_cycle(
        &self,
        level: Level,
        id: String,
        with_children: bool,
    ) -> Result<(), MobileError> {
        Ok(self.runtime.block_on(self.training().remove_cycle(
            level,
            &parse_id(&id)?,
            with_children,
        ))?)
    }

    /// What is in the cycle: its cycles, or the sessions of a microcycle.
    pub fn cycle_children(
        &self,
        level: Level,
        id: String,
    ) -> Result<Vec<ChildRecord>, MobileError> {
        let children = self
            .runtime
            .block_on(self.training().children(level, &parse_id(&id)?))?;
        Ok(children.iter().map(child_record).collect())
    }

    /// What could be added to the cycle: free and inside its window.
    pub fn free_children(&self, level: Level, id: String) -> Result<Vec<ChildRecord>, MobileError> {
        let children = self
            .runtime
            .block_on(self.training().free_children(level, &parse_id(&id)?))?;
        Ok(children.iter().map(child_record).collect())
    }

    pub fn add_children(
        &self,
        level: Level,
        id: String,
        children: Vec<String>,
    ) -> Result<(), MobileError> {
        Ok(self.runtime.block_on(self.training().add_children(
            level,
            &parse_id(&id)?,
            ids(&children)?,
        ))?)
    }

    /// The cycles of the level that hold the day, to propose one to a
    /// session.
    pub fn cycles_containing(
        &self,
        team_id: String,
        level: Level,
        date: String,
    ) -> Result<Vec<CycleRecord>, MobileError> {
        let cycles = self.runtime.block_on(self.training().containing(
            &parse_id(&team_id)?,
            level,
            parse_day(&date, TrainingField::Start)?,
        ))?;
        Ok(cycles.iter().map(cycle_record).collect())
    }

    /// The window a microcycle of these sessions proposes: the Monday of
    /// the first, the weeks to the last.
    pub fn proposed_microcycle(
        &self,
        session_ids: Vec<String>,
    ) -> Result<Option<CycleDraft>, MobileError> {
        let training = self.training();
        let starts = ids(&session_ids)?
            .iter()
            .map(|id| self.runtime.block_on(training.session(id)).map(|s| s.start))
            .collect::<Result<Vec<_>, AppError>>()?;
        Ok(proposed_microcycle(&starts).map(|(start, span)| proposal(start, span)))
    }

    /// The window a mesocycle or a program of these cycles proposes.
    pub fn proposed_container(
        &self,
        level: Level,
        child_ids: Vec<String>,
    ) -> Result<Option<CycleDraft>, MobileError> {
        let training = self.training();
        let child_level = level
            .child()
            .ok_or_else(|| failed("a microcycle has no cycles"))?;
        let windows = ids(&child_ids)?
            .iter()
            .map(|id| {
                self.runtime
                    .block_on(training.cycle(child_level, id))
                    .map(|c| c.window)
            })
            .collect::<Result<Vec<Window>, AppError>>()?;
        Ok(proposed_container(level, &windows).map(|(start, span)| proposal(start, span)))
    }

    /// Copies the microcycle `copies` times, each `shift_weeks` later: the
    /// new ones.
    pub fn duplicate_microcycle(
        &self,
        id: String,
        draft: DuplicateCycleDraft,
    ) -> Result<Vec<CycleRecord>, MobileError> {
        let cycles = self.runtime.block_on(self.training().duplicate_microcycle(
            &parse_id(&id)?,
            DuplicateCycleInput {
                shift_weeks: draft.shift_weeks,
                copies: draft.copies,
                copy_objectives: draft.copy_objectives,
                copy_activities: draft.copy_activities,
            },
        ))?;
        Ok(cycles.iter().map(cycle_record).collect())
    }

    // ---- agenda and totals

    /// The days of the period that holds `date`, with the sessions and the
    /// matches of the team (all the teams without), by start.
    pub fn training_agenda(
        &self,
        team_id: Option<String>,
        span: AgendaSpan,
        date: String,
    ) -> Result<Vec<AgendaDay>, MobileError> {
        let date = parse_day(&date, TrainingField::Start)?;
        let period = match span {
            AgendaSpan::Day => Period::day(date),
            AgendaSpan::Week => Period::week(date),
            AgendaSpan::Month => Period::month(date),
        };
        let team = opt_id(&team_id)?;
        let events = self.runtime.block_on(self.training().agenda(
            team.as_ref(),
            period,
            Local::now().naive_local(),
        ))?;
        Ok(by_day(&period, &events)
            .iter()
            .map(|(date, events)| AgendaDay {
                date: day(*date),
                events: events.iter().map(event_record).collect(),
            })
            .collect())
    }

    /// The days between `from` and `to` (both included, either open) that
    /// have sessions or matches of the team (all the teams without), by
    /// start: the list of the calendar, the empty days left out.
    pub fn training_agenda_between(
        &self,
        team_id: Option<String>,
        from: Option<String>,
        to: Option<String>,
    ) -> Result<Vec<AgendaDay>, MobileError> {
        // an open end: dates no session or match can be before or after
        let first = NaiveDate::from_ymd_opt(1900, 1, 1).unwrap_or(NaiveDate::MIN);
        let last = NaiveDate::from_ymd_opt(9999, 12, 30).unwrap_or(NaiveDate::MAX);
        let from = from
            .map(|d| parse_day(&d, TrainingField::Start))
            .transpose()?
            .unwrap_or(first);
        let to = to
            .map(|d| parse_day(&d, TrainingField::Start))
            .transpose()?
            .unwrap_or(last);
        let period = Period {
            from,
            to: to.succ_opt().unwrap_or(to),
        };
        let team = opt_id(&team_id)?;
        let events = self.runtime.block_on(self.training().agenda(
            team.as_ref(),
            period,
            Local::now().naive_local(),
        ))?;
        Ok(busy_days(&events)
            .iter()
            .map(|(date, events)| AgendaDay {
                date: day(*date),
                events: events.iter().map(event_record).collect(),
            })
            .collect())
    }

    pub fn training_totals(
        &self,
        team_id: String,
        scope: TrainingScope,
    ) -> Result<TotalsRecord, MobileError> {
        let totals = self.runtime.block_on(
            self.training()
                .totals(&parse_id(&team_id)?, scope_of(&scope)?),
        )?;
        Ok(totals_record(&totals))
    }

    /// The attendance of each player over the completed sessions of the
    /// scope, with the percentage.
    pub fn attendance_report(
        &self,
        team_id: String,
        scope: TrainingScope,
    ) -> Result<AttendanceReportRecord, MobileError> {
        let report = self.runtime.block_on(
            self.training()
                .attendance_report(&parse_id(&team_id)?, scope_of(&scope)?),
        )?;
        Ok(report_record(&report))
    }

    // ---- archive and sheets

    /// The archive of the training of the team: cycles, sessions,
    /// activities and attendance.
    pub fn export_training(&self, team_id: String) -> Result<super::ExportedArchive, MobileError> {
        use scout4all_core::providers::sqlite::archive::{self as archives, Subject};
        let id = parse_id(&team_id)?;
        self.read_team(&team_id)?;
        let bytes = archives::export(&self.db, Subject::Training(id))?;
        Ok(super::ExportedArchive {
            file_name: format!("{id}-training.zip"),
            bytes,
        })
    }

    /// The sheet of a session as a PDF: with who took part and with the
    /// notes of the session, if asked.
    pub fn session_pdf(
        &self,
        session_id: String,
        participants: bool,
        notes: bool,
    ) -> Result<Vec<u8>, MobileError> {
        let training = self.training();
        let session = self
            .runtime
            .block_on(training.session(&parse_id(&session_id)?))?;
        let team = self.read_team(&session.team_id.to_string())?;
        let activities = self.runtime.block_on(training.activities(&session.id))?;
        let sheet = self.runtime.block_on(training.attendance(&session.id))?;
        let venues = self.venues_of(std::slice::from_ref(&session))?;
        let sheet = session_sheet(
            self.language.labels(),
            &Context {
                team: &team.name,
                year: team.year,
                printed: Local::now().naive_local(),
                venues: &venues,
            },
            &session,
            &activities,
            sheet.recorded.then_some(sheet.rows.as_slice()),
            SessionOptions {
                participants,
                notes,
            },
        );
        Ok(render_training_pdf(&sheet)?)
    }

    /// The plan of a microcycle as a PDF.
    pub fn microcycle_pdf(&self, id: String) -> Result<Vec<u8>, MobileError> {
        let training = self.training();
        let cycle = self
            .runtime
            .block_on(training.cycle(Level::Microcycle, &parse_id(&id)?))?;
        let team = self.read_team(&cycle.team_id.to_string())?;
        let sessions = self
            .runtime
            .block_on(training.children(Level::Microcycle, &cycle.id))?
            .iter()
            .map(|child| {
                let session = self.runtime.block_on(training.session(&child.id))?;
                let activities = self.runtime.block_on(training.activities(&session.id))?;
                Ok((session, activities))
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        let venues = self.venues_of(
            &sessions
                .iter()
                .map(|(session, _)| session.clone())
                .collect::<Vec<_>>(),
        )?;
        let sheet = microcycle_sheet(
            self.language.labels(),
            &Context {
                team: &team.name,
                year: team.year,
                printed: Local::now().naive_local(),
                venues: &venues,
            },
            &cycle,
            &sessions,
        );
        Ok(render_training_pdf(&sheet)?)
    }
}

impl Scout4All {
    /// The draft checked, as the provider wants it.
    fn session_input(&self, draft: SessionDraft) -> Result<SessionInput, MobileError> {
        let form = SessionForm {
            start: draft.start.as_deref().map(parse_moment).transpose()?,
            duration: draft.duration,
            actual_duration: draft.actual_duration,
            venue: opt_id(&draft.venue)?,
            title: draft.title,
            objectives: draft.objectives,
            notes: draft.notes,
            final_remarks: draft.final_remarks,
            microcycle: opt_id(&draft.microcycle)?,
            status: draft.status,
        };
        form.validate().map_err(|e| MobileError::InvalidTraining {
            field: session_form_field(e),
            reason: e.message(self.language.labels()),
        })
    }

    /// The provider of the training, over the database of the app.
    fn training(&self) -> SqliteTraining {
        SqliteTraining::new(self.db.clone())
    }

    /// The gyms the sessions hold, removed or not: what a sheet names.
    fn venues_of(&self, sessions: &[TrainingSession]) -> Result<Vec<Venue>, AppError> {
        let venues = SqliteVenues::new(self.db.clone());
        sessions
            .iter()
            .filter_map(|s| s.venue_id)
            .collect::<std::collections::BTreeSet<_>>()
            .iter()
            .map(|id| venues.venue(id))
            .collect()
    }
}

fn session_draft_of(form: &SessionForm) -> SessionDraft {
    SessionDraft {
        start: form.start.map(moment),
        duration: form.duration.clone(),
        actual_duration: form.actual_duration.clone(),
        venue: form.venue.map(|id| id.to_string()),
        title: form.title.clone(),
        objectives: form.objectives.clone(),
        notes: form.notes.clone(),
        final_remarks: form.final_remarks.clone(),
        microcycle: form.microcycle.map(|id| id.to_string()),
        status: form.status,
    }
}

fn activity_draft_of(form: &ActivityForm) -> ActivityDraft {
    ActivityDraft {
        name: form.name.clone(),
        description: form.description.clone(),
        planned: form.planned.clone(),
        actual: form.actual.clone(),
        notes: form.notes.clone(),
        status: form.status,
    }
}

fn cycle_draft_of(form: &CycleForm) -> CycleDraft {
    CycleDraft {
        title: form.title.clone(),
        start: form.start.map(day),
        weeks: form.weeks.clone(),
        end: form.end.map(day),
        objectives: form.objectives.clone(),
        notes: form.notes.clone(),
        parent: form.parent.map(|id| id.to_string()),
    }
}

/// A draft with only the window a proposal gives.
fn proposal(start: NaiveDate, span: Span) -> CycleDraft {
    CycleDraft {
        start: Some(day(start)),
        weeks: match span {
            Span::Weeks(weeks) => weeks.to_string(),
            Span::Until(_) => String::new(),
        },
        end: match span {
            Span::Until(last) => Some(day(last)),
            Span::Weeks(_) => None,
        },
        ..cycle_draft_of(&CycleForm::default())
    }
}

fn scope_of(scope: &TrainingScope) -> Result<Scope, MobileError> {
    match scope {
        TrainingScope::All => Ok(Scope::All),
        TrainingScope::Cycle { level, id } => Ok(Scope::Cycle(*level, parse_id(id)?)),
        TrainingScope::Dates { from, to } => {
            let from = parse_day(from, TrainingField::Range)?;
            let to = parse_day(to, TrainingField::Range)?;
            Window::of(Level::Program, from, Span::Until(to))
                .map(Scope::Dates)
                .ok_or_else(|| invalid(TrainingField::Range, &format!("{from} – {to}")))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::unreachable)]
mod tests {
    use super::{super::ArchivePreview, *};
    use scout4all_core::{
        player_form::PlayerForm,
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, RoleEnum, TeamClassificationEnum},
        },
        team_form::TeamForm,
    };
    use std::sync::Arc;

    fn app(dir: &tempfile::TempDir) -> Arc<Scout4All> {
        app_in(dir)
    }

    fn app_in(dir: &tempfile::TempDir) -> Arc<Scout4All> {
        Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap()
    }

    fn team(app: &Scout4All, name: &str) -> String {
        app.save_team(
            None,
            TeamForm {
                name: name.into(),
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Women),
                country: CountryCode::known("IT"),
                year: "2026".into(),
            },
        )
        .unwrap()
    }

    fn player(app: &Scout4All, team: &str, name: &str, number: u8) {
        app.save_player(
            team.into(),
            None,
            PlayerForm {
                name: name.into(),
                role: Some(RoleEnum::Setter),
                number: number.to_string(),
                birth_date: String::new(),
                height_cm: String::new(),
                notes: String::new(),
                person: None,
            },
            false,
        )
        .unwrap();
    }

    fn draft(app: &Scout4All, start: &str) -> SessionDraft {
        SessionDraft {
            start: Some(start.into()),
            title: "Ricezione".into(),
            ..app.new_session_draft()
        }
    }

    fn session(app: &Scout4All, team: &str, start: &str) -> SessionRecord {
        app.save_session(team.into(), None, draft(app, start))
            .unwrap()
    }

    /// A gym of the database, by its id.
    fn gym(app: &Scout4All, name: &str) -> String {
        use scout4all_core::training::venue::VenueInput;
        SqliteVenues::new(app.db.clone())
            .create(VenueInput::named(name).unwrap())
            .unwrap()
            .id
            .to_string()
    }

    #[test]
    fn a_session_is_saved_listed_and_edited() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        let first = session(&app, &team, "2030-03-04T18:30");
        // two hours, the usual duration
        assert_eq!(first.duration_min, 120);
        assert_eq!(first.status, SessionStatus::Planned);
        assert!(!first.to_close);
        let gym = gym(&app, "Palestra");
        let mut edit = app.session_draft(first.id.clone()).unwrap();
        edit.venue = Some(gym.clone());
        let saved = app
            .save_session(team.clone(), Some(first.id.clone()), edit)
            .unwrap();
        assert_eq!(saved.id, first.id);
        assert_eq!(saved.venue_id, Some(gym));
        assert_eq!(app.training_sessions(team).unwrap(), vec![saved]);
    }

    #[test]
    fn a_session_in_the_past_is_to_close() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        let old = session(&app, &team, "2020-03-04T18:30");
        assert!(old.to_close);
        let done = app.complete_session(old.id.clone(), vec![]).unwrap();
        assert_eq!(
            (done.status, done.to_close),
            (SessionStatus::Completed, false)
        );
    }

    #[test]
    fn a_bad_form_names_its_field() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        let bad = SessionDraft {
            duration: "0".into(),
            ..draft(&app, "2030-03-04T18:30")
        };
        assert!(matches!(
            app.save_session(team.clone(), None, bad),
            Err(MobileError::InvalidTraining {
                field: TrainingField::Duration,
                ..
            })
        ));
        let none = SessionDraft {
            start: None,
            ..draft(&app, "")
        };
        assert!(matches!(
            app.save_session(team.clone(), None, none),
            Err(MobileError::InvalidTraining {
                field: TrainingField::Start,
                ..
            })
        ));
        let nameless = ActivityDraft {
            name: " ".into(),
            ..app.new_activity_draft()
        };
        let s = session(&app, &team, "2030-03-04T18:30");
        assert!(matches!(
            app.save_activity(s.id, None, nameless),
            Err(MobileError::InvalidTraining {
                field: TrainingField::Name,
                ..
            })
        ));
    }

    #[test]
    fn an_overlap_names_what_it_overlaps() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        session(&app, &team, "2030-03-04T18:30");
        let Err(MobileError::Overlap { reason }) =
            app.save_session(team, None, draft(&app, "2030-03-04T19:00"))
        else {
            unreachable!("not an overlap");
        };
        assert!(reason.contains("2030-03-04 18:30"), "{reason}");
    }

    #[test]
    fn the_activities_are_added_moved_and_searched() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        let s = session(&app, &team, "2030-03-04T18:30");
        let add = |name: &str| {
            app.save_activity(
                s.id.clone(),
                None,
                ActivityDraft {
                    name: name.into(),
                    planned: "15".into(),
                    ..app.new_activity_draft()
                },
            )
            .unwrap()
        };
        let (a, b) = (add("Palleggio"), add("Battuta"));
        app.move_activity(b.id.clone(), Move::Up).unwrap();
        let names: Vec<_> = app
            .session_activities(s.id.clone())
            .unwrap()
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(names, ["Battuta", "Palleggio"]);
        let hits = app.search_activities(team.clone(), "pall".into()).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            app.activity_draft_copying(hits[0].clone()).unwrap().planned,
            "15"
        );
        let mut edit = app.activity_draft(s.id.clone(), a.id.clone()).unwrap();
        edit.status = ActivityStatus::Completed;
        assert_eq!(
            app.save_activity(s.id.clone(), Some(a.id.clone()), edit)
                .unwrap()
                .status,
            ActivityStatus::Completed
        );
        app.remove_activity(a.id).unwrap();
        assert_eq!(app.session_activities(s.id).unwrap().len(), 1);
    }

    #[test]
    fn the_search_can_look_at_the_open_team_only() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let (a, b) = (team(&app, "Cortona"), team(&app, "Arezzo"));
        let s = session(&app, &a, "2030-03-04T18:30");
        app.save_activity(
            s.id,
            None,
            ActivityDraft {
                name: "Palleggio".into(),
                ..app.new_activity_draft()
            },
        )
        .unwrap();
        assert!(!app.activity_search_team_only());
        assert_eq!(
            app.search_activities(b.clone(), "pall".into())
                .unwrap()
                .len(),
            1
        );
        app.set_activity_search_team_only(true).unwrap();
        assert!(app.activity_search_team_only());
        assert!(app.search_activities(b, "pall".into()).unwrap().is_empty());
        // kept across a restart, with the other settings
        let again = app_in(&dir);
        assert!(again.activity_search_team_only());
    }

    #[test]
    fn the_attendance_starts_from_the_roster() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        player(&app, &team, "Anna", 1);
        player(&app, &team, "Bea", 2);
        let s = session(&app, &team, "2030-03-04T18:30");
        let sheet = app.session_attendance(s.id.clone()).unwrap();
        assert!(!sheet.recorded);
        assert_eq!(sheet.rows.len(), 2);
        let mut rows = sheet.rows;
        rows[1].status = AttendanceStatus::Absent;
        rows[1].note = "  influenza ".into();
        app.save_attendance(s.id.clone(), rows).unwrap();
        let saved = app.session_attendance(s.id.clone()).unwrap();
        assert!(saved.recorded);
        assert_eq!(saved.rows[1].status, AttendanceStatus::Absent);
        assert_eq!(saved.rows[1].note, "influenza");
        assert!(app.missing_players(s.id).unwrap().is_empty());
        assert!(app.guest_choices(team).unwrap().is_empty());
    }

    #[test]
    fn a_series_tells_its_conflicts_and_is_changed_together() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        let (palestra, campo) = (gym(&app, "Palestra"), gym(&app, "Campo"));
        // Mondays and Wednesdays of two weeks
        let series = SeriesDraft {
            from: "2030-03-04".into(),
            to: "2030-03-15".into(),
            bands: [(1, "18:30"), (3, "20:00")]
                .map(|(weekday, time)| SeriesBand {
                    weekday,
                    time: time.into(),
                    duration_min: 90,
                    venue: Some(palestra.clone()),
                })
                .into(),
        };
        session(&app, &team, "2030-03-11T18:00");
        let blocked = app
            .create_series(team.clone(), vec![series.clone()], false)
            .unwrap();
        assert!(blocked.created.is_empty());
        assert_eq!(blocked.conflicts.len(), 1);
        assert_eq!(blocked.conflicts[0].start, "2030-03-11T18:30");
        let made = app.create_series(team.clone(), vec![series], true).unwrap();
        assert_eq!(made.created.len(), 3);
        let first = made.created[0].clone();
        // a change of gym and minutes asks where it goes; a title doesn't
        let changed = SessionDraft {
            venue: Some(campo.clone()),
            duration: "60".into(),
            ..app.session_draft(first.id.clone()).unwrap()
        };
        assert!(
            app.asks_series_scope(first.id.clone(), Some(changed.clone()))
                .unwrap()
        );
        let titled = SessionDraft {
            title: "Battuta".into(),
            ..app.session_draft(first.id.clone()).unwrap()
        };
        assert!(
            !app.asks_series_scope(first.id.clone(), Some(titled))
                .unwrap()
        );
        assert!(app.asks_series_scope(first.id.clone(), None).unwrap());
        app.save_session_in_series(first.id.clone(), changed, SeriesScope::All)
            .unwrap();
        assert!(
            app.training_sessions(team.clone())
                .unwrap()
                .iter()
                .filter(|s| s.series_id.is_some())
                .all(|s| s.venue_id.as_ref() == Some(&campo) && s.duration_min == 60)
        );
        assert_eq!(app.remove_series(first.id, SeriesScope::All).unwrap(), 3);
        assert_eq!(app.training_sessions(team.clone()).unwrap().len(), 1);
        // a session alone never asks
        let alone = app.training_sessions(team).unwrap()[0].id.clone();
        assert!(!app.asks_series_scope(alone, None).unwrap());
    }

    /// Each entry of the dialog is a series: Thursday moves with Thursdays.
    #[test]
    fn the_entries_of_a_series_are_series_of_their_own() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        let entry = |from: &str, weekday| SeriesDraft {
            from: from.into(),
            to: "2030-03-17".into(),
            bands: vec![SeriesBand {
                weekday,
                time: "20:30".into(),
                duration_min: 120,
                venue: None,
            }],
        };
        let made = app
            .create_series(
                team.clone(),
                vec![entry("2030-03-04", 1), entry("2030-03-07", 4)],
                false,
            )
            .unwrap();
        assert_eq!(made.created.len(), 4);
        let thursday = made.created[1].clone();
        assert_eq!(thursday.start, "2030-03-07T20:30");
        let earlier = SessionDraft {
            start: Some("2030-03-07T19:30".into()),
            ..app.session_draft(thursday.id.clone()).unwrap()
        };
        app.save_session_in_series(thursday.id, earlier, SeriesScope::All)
            .unwrap();
        let starts: Vec<String> = app
            .training_sessions(team)
            .unwrap()
            .into_iter()
            .map(|s| s.start)
            .collect();
        assert_eq!(
            starts,
            [
                "2030-03-04T20:30",
                "2030-03-07T19:30",
                "2030-03-11T20:30",
                "2030-03-14T19:30"
            ]
        );
    }

    #[test]
    fn a_session_is_duplicated_a_week_later() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        let s = session(&app, &team, "2030-03-04T18:30");
        let proposed = app.proposed_duplicate(s.id.clone()).unwrap();
        assert_eq!(proposed.start, "2030-03-11T18:30");
        let copy = app
            .duplicate_session(s.id.clone(), proposed.clone())
            .unwrap();
        assert_ne!(copy.id, s.id);
        assert!(matches!(
            app.duplicate_session(s.id, proposed),
            Err(MobileError::Overlap { .. })
        ));
    }

    #[test]
    fn the_cycles_hold_the_sessions_and_tell_what_falls_out() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        let s = session(&app, &team, "2030-03-06T18:30");
        let proposal = app
            .proposed_microcycle(vec![s.id.clone()])
            .unwrap()
            .unwrap();
        assert_eq!(proposal.start.as_deref(), Some("2030-03-04"));
        let micro = app
            .save_cycle(
                team.clone(),
                Level::Microcycle,
                None,
                CycleDraft {
                    title: "Settimana 1".into(),
                    ..proposal
                },
                vec![s.id.clone()],
                false,
            )
            .unwrap();
        assert_eq!(
            (micro.weeks, micro.last_day.as_str()),
            (Some(1), "2030-03-10")
        );
        assert_eq!(
            app.session_draft(s.id.clone()).unwrap().microcycle,
            Some(micro.id.clone())
        );
        assert_eq!(
            app.cycles_containing(team.clone(), Level::Microcycle, "2030-03-08".into())
                .unwrap(),
            vec![micro.clone()]
        );
        // a window that leaves the session out: told, then detached
        let narrow = CycleDraft {
            start: Some("2030-03-11".into()),
            ..app
                .cycle_draft(Level::Microcycle, micro.id.clone())
                .unwrap()
        };
        assert!(matches!(
            app.save_cycle(
                team.clone(),
                Level::Microcycle,
                Some(micro.id.clone()),
                narrow.clone(),
                vec![],
                false
            ),
            Err(MobileError::ChildrenOutside { .. })
        ));
        app.save_cycle(
            team.clone(),
            Level::Microcycle,
            Some(micro.id.clone()),
            narrow,
            vec![],
            true,
        )
        .unwrap();
        assert!(
            app.cycle_children(Level::Microcycle, micro.id.clone())
                .unwrap()
                .is_empty()
        );
        let copies = app
            .duplicate_microcycle(
                micro.id.clone(),
                DuplicateCycleDraft {
                    shift_weeks: 1,
                    copies: 2,
                    copy_objectives: true,
                    copy_activities: true,
                },
            )
            .unwrap();
        assert_eq!(copies.len(), 2);
        assert_eq!(
            app.cycles(team.clone(), Level::Microcycle).unwrap().len(),
            3
        );
        // two cycles of a level can't overlap
        let clash = CycleDraft {
            title: "Dopo".into(),
            start: Some("2030-03-12".into()),
            weeks: "1".into(),
            ..app.new_cycle_draft()
        };
        assert!(matches!(
            app.save_cycle(team, Level::Microcycle, None, clash, vec![], false),
            Err(MobileError::Overlap { .. })
        ));
        app.remove_cycle(Level::Microcycle, micro.id, false)
            .unwrap();
    }

    #[test]
    fn the_agenda_has_every_day_of_the_period() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        let s = session(&app, &team, "2030-03-06T18:30");
        let week = app
            .training_agenda(Some(team.clone()), AgendaSpan::Week, "2030-03-08".into())
            .unwrap();
        assert_eq!(week.len(), 7);
        assert_eq!(week[0].date, "2030-03-04");
        let busy: Vec<_> = week.iter().filter(|d| !d.events.is_empty()).collect();
        assert_eq!(busy.len(), 1);
        assert!(matches!(
            &busy[0].events[0].item,
            AgendaItemRecord::Session { id, .. } if *id == s.id
        ));
        let today = app
            .training_agenda(None, AgendaSpan::Day, "2030-03-06".into())
            .unwrap();
        assert_eq!((today.len(), today[0].events.len()), (1, 1));
        assert_eq!(
            app.training_agenda(None, AgendaSpan::Month, "2030-03-06".into())
                .unwrap()
                .len(),
            31
        );
    }

    /// The list of the calendar: only the days with something, in time
    /// order, between the days given (both included) or over any date.
    #[test]
    fn the_agenda_between_two_days_has_only_the_busy_ones() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        let first = session(&app, &team, "2030-03-06T18:30");
        let second = session(&app, &team, "2030-03-06T20:30");
        let later = session(&app, &team, "2031-01-10T18:30");
        let ids = |days: &[AgendaDay]| -> Vec<(String, Vec<String>)> {
            days.iter()
                .map(|d| {
                    let ids = d
                        .events
                        .iter()
                        .filter_map(|e| match &e.item {
                            AgendaItemRecord::Session { id, .. } => Some(id.clone()),
                            AgendaItemRecord::Match { .. } => None,
                        })
                        .collect();
                    (d.date.clone(), ids)
                })
                .collect()
        };
        let any = app
            .training_agenda_between(Some(team.clone()), None, None)
            .unwrap();
        assert_eq!(
            ids(&any),
            vec![
                ("2030-03-06".to_string(), vec![first.id.clone(), second.id]),
                ("2031-01-10".to_string(), vec![later.id]),
            ]
        );
        let march = app
            .training_agenda_between(
                Some(team),
                Some("2030-03-01".into()),
                Some("2030-03-06".into()),
            )
            .unwrap();
        assert_eq!(march.len(), 1);
        assert_eq!(march[0].events.len(), 2);
        assert!(matches!(
            &march[0].events[0].item,
            AgendaItemRecord::Session { id, .. } if *id == first.id
        ));
    }

    #[test]
    fn the_totals_and_the_attendance_count_the_completed_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        player(&app, &team, "Anna", 1);
        player(&app, &team, "Bea", 2);
        let done = session(&app, &team, "2020-03-04T18:30");
        let planned = session(&app, &team, "2030-03-04T18:30");
        let mut rows = app.session_attendance(done.id.clone()).unwrap().rows;
        rows[1].status = AttendanceStatus::Absent;
        app.save_attendance(done.id.clone(), rows).unwrap();
        app.complete_session(done.id, vec![]).unwrap();
        app.set_session_status(planned.id, SessionStatus::Cancelled)
            .unwrap();
        assert_eq!(
            app.training_totals(team.clone(), TrainingScope::All)
                .unwrap(),
            TotalsRecord {
                planned: 0,
                completed: 1,
                cancelled: 1,
                planned_minutes: 120,
                actual_minutes: 120,
            }
        );
        let report = app
            .attendance_report(
                team.clone(),
                TrainingScope::Dates {
                    from: "2020-01-01".into(),
                    to: "2020-12-31".into(),
                },
            )
            .unwrap();
        assert_eq!(report.sessions, 1);
        assert_eq!(
            report.players.iter().map(|p| p.percent).collect::<Vec<_>>(),
            [Some(100), Some(0)]
        );
        assert!(matches!(
            app.training_totals(
                team,
                TrainingScope::Dates {
                    from: "2020-02-01".into(),
                    to: "2020-01-01".into()
                }
            ),
            Err(MobileError::InvalidTraining {
                field: TrainingField::Range,
                ..
            })
        ));
    }

    #[test]
    fn the_training_moves_to_another_phone() {
        let from_dir = tempfile::tempdir().unwrap();
        let from = app(&from_dir);
        let team = team(&from, "Cortona");
        let s = session(&from, &team, "2030-03-04T18:30");
        from.save_activity(
            s.id.clone(),
            None,
            ActivityDraft {
                name: "Palleggio".into(),
                ..from.new_activity_draft()
            },
        )
        .unwrap();
        assert_eq!(from.team_removal(team.clone()).unwrap().sessions, 1);
        let zip = from.export_training(team.clone()).unwrap();
        let to_dir = tempfile::tempdir().unwrap();
        let to = app(&to_dir);
        // the team first
        assert!(to.import_team(zip.bytes.clone(), None).is_err());
        let team_zip = from.export_team(team.clone()).unwrap();
        to.import_team(team_zip.bytes, None).unwrap();
        assert_eq!(
            to.preview_archive(zip.bytes.clone(), None, None).unwrap(),
            ArchivePreview::Training {
                name: "Cortona".into(),
                year: 2026,
                sessions: 1,
                cycles: 0,
            }
        );
        to.import_team(zip.bytes, None).unwrap();
        assert_eq!(to.training_sessions(team.clone()).unwrap()[0].id, s.id);
        assert_eq!(to.session_activities(s.id).unwrap().len(), 1);
    }

    #[test]
    fn the_sheets_are_pdfs() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        let s = session(&app, &team, "2030-03-04T18:30");
        let pdf = app.session_pdf(s.id.clone(), true, true).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        let micro = app
            .save_cycle(
                team,
                Level::Microcycle,
                None,
                CycleDraft {
                    title: "Settimana 1".into(),
                    start: Some("2030-03-04".into()),
                    weeks: "1".into(),
                    ..app.new_cycle_draft()
                },
                vec![s.id],
                false,
            )
            .unwrap();
        assert!(app.microcycle_pdf(micro.id).unwrap().starts_with(b"%PDF"));
    }
}
