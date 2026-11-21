use crate::{
    errors::AppError,
    training::{SessionInput, TrainingSession},
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
}
