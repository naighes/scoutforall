use crate::{
    domain::ScoutingMode,
    errors::AppError,
    match_form::{MatchInput, RosterNumber},
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

    /// The roster of team B of the match, a team: these players with
    /// these numbers and roles, in place of the saved one.
    async fn save_roster_of_b(
        &self,
        m: &MatchEntry,
        numbers: &[RosterNumber],
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

    /// The match, saved, scouted as `scouting` from now on: on both sides
    /// only against a team; scouting only A, the lineups and the touches
    /// of B go.
    async fn save_scouting(&self, m: &MatchEntry, scouting: ScoutingMode) -> Result<(), AppError>;

    /// Removes the match with its sets, for good: by its folder, so that
    /// a match that can't be read goes too.
    async fn remove(&self, team_id: &Uuid, match_id: &str) -> Result<(), AppError>;
}
