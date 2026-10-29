use crate::{
    errors::AppError,
    shapes::{r#match::MatchEntry, team::TeamEntry},
};
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset};
use uuid::Uuid;

#[async_trait]
pub trait MatchWriter {
    async fn create(
        &self,
        team: &TeamEntry,
        opponent: String,
        date: DateTime<FixedOffset>,
        home: bool,
        max_substitutions: u8,
        notes: String,
        roster: Vec<Uuid>,
    ) -> Result<MatchEntry, AppError>;

    async fn update(
        &self,
        m: &MatchEntry,
        opponent: String,
        date: DateTime<FixedOffset>,
        home: bool,
        max_substitutions: u8,
        notes: String,
        roster: Vec<Uuid>,
    ) -> Result<(), AppError>;
}
