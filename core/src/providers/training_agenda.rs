use crate::{
    errors::AppError,
    training::agenda::{AgendaEvent, Period},
};
use async_trait::async_trait;
use chrono::NaiveDateTime;
use uuid::Uuid;

#[async_trait]
pub trait TrainingAgenda {
    /// The sessions and the matches of the team in the period, by start; of
    /// all the teams when none (a match between two of them once, under A).
    /// `now` marks the sessions "to close".
    async fn agenda(
        &self,
        team_id: Option<&Uuid>,
        period: Period,
        now: NaiveDateTime,
    ) -> Result<Vec<AgendaEvent>, AppError>;
}
