use crate::{
    errors::AppError,
    shapes::{r#match::MatchEntry, team::TeamEntry},
};
use async_trait::async_trait;

#[async_trait]
pub trait MatchReader {
    async fn read_all(&self, team: &TeamEntry) -> Result<Vec<MatchEntry>, AppError>;
    #[allow(dead_code)]
    async fn read_single(&self, team: &TeamEntry, match_id: &str) -> Result<MatchEntry, AppError>;
    async fn exists(&self, team: &TeamEntry, match_id: &str) -> Result<bool, AppError>;
}
