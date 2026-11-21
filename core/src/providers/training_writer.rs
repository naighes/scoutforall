use crate::{
    errors::AppError,
    training::{
        Activity, ActivityInput, AttendanceInput, Move, SessionInput, SessionStatus,
        TrainingSession,
        duplicate::DuplicateInput,
        series::{SeriesEdit, SeriesInput, SeriesOutcome, SeriesScope},
    },
};
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait TrainingWriter {
    /// A new session of the team, in no series.
    async fn create_session(
        &self,
        team_id: &Uuid,
        input: SessionInput,
    ) -> Result<TrainingSession, AppError>;

    /// The session as the input says; its team and series stay.
    async fn update_session(
        &self,
        session_id: &Uuid,
        input: SessionInput,
    ) -> Result<TrainingSession, AppError>;

    /// Repeats the bands between the dates: real sessions with a `series_id`
    /// in common, planned, with no objectives, activities or attendance.
    /// The ones that overlap a session or a match in programma (or an
    /// earlier one of the series) are reported as conflicts: with
    /// `create_others` false any conflict stops everything and nothing is
    /// written; with true the others are created.
    async fn create_series(
        &self,
        team_id: &Uuid,
        input: SeriesInput,
        create_others: bool,
    ) -> Result<SeriesOutcome, AppError>;

    /// Changes location, time or duration of the planned sessions `scope`
    /// reaches from `session_id` (the date never changes). A change that
    /// overlaps something stops the whole group: nothing is written.
    async fn update_series(
        &self,
        session_id: &Uuid,
        scope: SeriesScope,
        edit: SeriesEdit,
    ) -> Result<Vec<TrainingSession>, AppError>;

    /// Removes (D7) the planned sessions `scope` reaches from `session_id`;
    /// how many.
    async fn remove_series(&self, session_id: &Uuid, scope: SeriesScope)
    -> Result<usize, AppError>;

    /// A new session of the same team on another date: planned, in no
    /// series, with the title, location and planned duration of the
    /// original, and its objectives and planned activities when the input
    /// says so. It overlaps nothing, or nothing is written.
    async fn duplicate_session(
        &self,
        session_id: &Uuid,
        input: DuplicateInput,
    ) -> Result<TrainingSession, AppError>;

    /// Only the status of the session: cancel it, or reopen it as planned.
    /// Its activities stay as they are.
    async fn set_session_status(
        &self,
        session_id: &Uuid,
        status: SessionStatus,
    ) -> Result<TrainingSession, AppError>;

    /// Completes the session and marks completed those of `activities` that
    /// are planned (the others stay as they are). An activity that is not
    /// of the session stops everything: nothing is written.
    async fn complete_session(
        &self,
        session_id: &Uuid,
        activities: Vec<Uuid>,
    ) -> Result<TrainingSession, AppError>;

    /// Marks the session removed (D7): its activities and attendance are no
    /// longer read.
    async fn remove_session(&self, session_id: &Uuid) -> Result<(), AppError>;

    /// A new activity at the end of the session.
    async fn add_activity(
        &self,
        session_id: &Uuid,
        input: ActivityInput,
    ) -> Result<Activity, AppError>;

    /// The activity as the input says; its session and place stay.
    async fn update_activity(
        &self,
        activity_id: &Uuid,
        input: ActivityInput,
    ) -> Result<Activity, AppError>;

    /// Marks the activity removed (D7); the others keep their order.
    async fn remove_activity(&self, activity_id: &Uuid) -> Result<(), AppError>;

    /// The activity swaps place with the one before or after it; the first
    /// going up and the last going down stay.
    async fn move_activity(&self, activity_id: &Uuid, to: Move) -> Result<(), AppError>;

    /// The attendance of the session is exactly `rows`: the others it had
    /// are removed (D7). A person twice, a removed one or a guest of the
    /// team itself are refused.
    async fn save_attendance(
        &self,
        session_id: &Uuid,
        rows: Vec<AttendanceInput>,
    ) -> Result<(), AppError>;
}
