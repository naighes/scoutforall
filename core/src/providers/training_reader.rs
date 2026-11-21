use crate::{errors::AppError, training::TrainingSession};
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait TrainingReader {
    /// The sessions of a team not removed, by start.
    async fn sessions(&self, team_id: &Uuid) -> Result<Vec<TrainingSession>, AppError>;

    /// The session, or an error when it's removed or there's none.
    async fn session(&self, session_id: &Uuid) -> Result<TrainingSession, AppError>;

    /// The places the team trained in, the one of the latest session first:
    /// what the form of a session proposes.
    async fn locations(&self, team_id: &Uuid) -> Result<Vec<String>, AppError>;
}
