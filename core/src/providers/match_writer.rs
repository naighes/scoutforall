use crate::{
    errors::AppError,
    match_form::MatchInput,
    shapes::{
        r#match::{MatchEntry, MatchSource},
        team::TeamEntry,
    },
};
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait MatchWriter {
    /// A new match of team A `team`: team B and the competition created
    /// when new.
    async fn create(&self, team: &TeamEntry, input: MatchInput) -> Result<MatchEntry, AppError>;

    async fn update(&self, m: &MatchEntry, input: MatchInput) -> Result<(), AppError>;

    /// The match saved again with the opponent, date, home, substitutions,
    /// notes and roster of `m`, as the matches were before the database:
    /// what isn't in `m` (team B a team, a neutral court, the competition)
    /// stays.
    async fn replace_details(&self, m: &MatchEntry) -> Result<(), AppError>;

    /// The match, saved, coming from `source`, with the times of its
    /// events in the video when `video_clock`: a match is created as
    /// scouted here, on the clock.
    async fn save_source(
        &self,
        m: &MatchEntry,
        source: MatchSource,
        video_clock: bool,
    ) -> Result<MatchEntry, AppError>;

    /// Removes the match with its sets, for good: by its folder, so that
    /// a match that can't be read goes too.
    async fn remove(&self, team_id: &Uuid, match_id: &str) -> Result<(), AppError>;
}
