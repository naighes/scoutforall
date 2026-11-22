use crate::{
    errors::AppError,
    training::totals::{AttendanceReport, Scope, Totals},
};
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait TrainingTotals {
    /// The sessions of the team in the scope by status, and the minutes
    /// planned against the done. A removed session counts for nobody; a
    /// cycle of another team gives nothing.
    async fn totals(&self, team_id: &Uuid, scope: Scope) -> Result<Totals, AppError>;

    /// The attendance of the players in the scope: only the completed
    /// sessions with the attendance taken count; guests never.
    async fn attendance_report(
        &self,
        team_id: &Uuid,
        scope: Scope,
    ) -> Result<AttendanceReport, AppError>;
}
