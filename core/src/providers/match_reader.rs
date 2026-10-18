use crate::{
    errors::AppError,
    shapes::{listed::Listed, r#match::MatchEntry, team::TeamEntry},
};
use async_trait::async_trait;

#[async_trait]
pub trait MatchReader {
    /// Every match folder of the team: a match whose files can't be loaded,
    /// or whose sets don't replay, is unreadable, and doesn't hide the others.
    async fn read_all(&self, team: &TeamEntry) -> Result<Vec<Listed<MatchEntry>>, AppError>;
    #[allow(dead_code)]
    async fn read_single(&self, team: &TeamEntry, match_id: &str) -> Result<MatchEntry, AppError>;
    async fn exists(&self, team: &TeamEntry, match_id: &str) -> Result<bool, AppError>;
}
