use crate::{
    errors::AppError,
    training::{Activity, ActivityInput, Move, SessionInput, TrainingSession},
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
}
