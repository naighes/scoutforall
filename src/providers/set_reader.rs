use crate::{
    errors::AppError,
    shapes::{r#match::MatchEntry, set::SetEntry},
};
use async_trait::async_trait;

#[async_trait]
pub trait SetReader: Send + Sync {
    async fn read_all(&self, m: &MatchEntry) -> Result<Vec<SetEntry>, AppError>;
}
