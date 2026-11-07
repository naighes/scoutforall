use crate::{
    errors::AppError,
    shapes::{
        listed::Listed,
        r#match::{MatchEntry, MatchStatus},
        team::TeamEntry,
    },
};
use async_trait::async_trait;

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
    #[allow(dead_code)]
    async fn read_single(&self, team: &TeamEntry, match_id: &str) -> Result<MatchEntry, AppError>;
    async fn exists(&self, team: &TeamEntry, match_id: &str) -> Result<bool, AppError>;
}
