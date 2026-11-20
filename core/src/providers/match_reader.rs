use crate::{
    errors::AppError,
    shapes::{
        listed::Listed,
        r#match::{MatchEntry, MatchStatus, MatchSummary},
        team::TeamEntry,
    },
};
use async_trait::async_trait;
use chrono::NaiveDate;
use uuid::Uuid;

/// A match saved, as an import may tell it again (`MatchReader::played_on`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayedOn {
    pub id: String,
    pub team_a: Uuid,
    /// none when B is only a name
    pub team_b: Option<Uuid>,
    /// team A and B as the lists name them
    pub names: (String, String),
    /// the score of each set, from A's side
    pub scores: Vec<(u8, u8)>,
}

#[async_trait]
pub trait MatchReader {
    /// Every match folder of the team: a match whose files can't be loaded,
    /// or whose sets don't replay, is unreadable, and doesn't hide the others.
    async fn read_all(&self, team: &TeamEntry) -> Result<Vec<Listed<MatchEntry>>, AppError>;
    /// As `read_all`, each match with the status its sets were checked
    /// with: a list showing it replays each match once.
    async fn read_all_with_status(
        &self,
        team: &TeamEntry,
    ) -> Result<Vec<Listed<(MatchEntry, MatchStatus)>>, AppError>;
    /// These matches of the team, in the order given, read together.
    async fn read_some(&self, team: &TeamEntry, ids: &[String]) -> Vec<Listed<MatchEntry>>;
    /// What the lists show of each match, the most recent first, then the
    /// unreadable ones: from the cache of the database (see
    /// `sqlite::summary`).
    async fn read_summaries(&self, team: &TeamEntry)
    -> Result<Vec<Listed<MatchSummary>>, AppError>;
    #[allow(dead_code)]
    async fn read_single(&self, team: &TeamEntry, match_id: &str) -> Result<MatchEntry, AppError>;
    async fn exists(&self, team: &TeamEntry, match_id: &str) -> Result<bool, AppError>;
    /// The matches not removed started on `day` (in their own offset) in
    /// which one of `teams` plays, as A or as B; those whose sets don't
    /// replay left out.
    async fn played_on(&self, teams: &[Uuid], day: NaiveDate) -> Result<Vec<PlayedOn>, AppError>;
}
