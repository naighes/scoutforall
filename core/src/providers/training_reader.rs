use crate::{
    errors::AppError,
    training::{Activity, ActivityHit, TrainingSession},
};
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

    /// The activities of the session not removed, in order. Empty when the
    /// session is removed.
    async fn activities(&self, session_id: &Uuid) -> Result<Vec<Activity>, AppError>;

    /// The activities written before whose name holds `query` (case
    /// aside; all when empty), the first `limit`: the ones of `team_id` in
    /// front, then by the latest session. The same name and description
    /// appear once. With `only_team` those of the team only.
    async fn search_activities(
        &self,
        team_id: &Uuid,
        query: &str,
        only_team: bool,
        limit: usize,
    ) -> Result<Vec<ActivityHit>, AppError>;
}
