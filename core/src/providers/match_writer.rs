use crate::{
    errors::AppError,
    shapes::{r#match::MatchEntry, team::TeamEntry},
};
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset};

#[async_trait]
pub trait MatchWriter {
    async fn create(
        &self,
        team: &TeamEntry,
        opponent: String,
        date: DateTime<FixedOffset>,
        home: bool,
        max_substitutions: u8,
    ) -> Result<MatchEntry, AppError>;

    async fn update(
        &self,
        m: &MatchEntry,
        opponent: String,
        date: DateTime<FixedOffset>,
        home: bool,
        max_substitutions: u8,
    ) -> Result<(), AppError>;
}
