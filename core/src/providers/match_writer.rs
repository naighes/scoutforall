use crate::{
    errors::AppError,
    shapes::{
        r#match::{MatchEntry, MatchSource},
        team::TeamEntry,
    },
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

    /// The match, saved, coming from `source`, with the times of its
    /// events in the video when `video_clock`: a match is created as
    /// scouted here, on the clock.
    async fn save_source(
        &self,
        m: &MatchEntry,
        source: MatchSource,
        video_clock: bool,
    ) -> Result<MatchEntry, AppError>;
}
