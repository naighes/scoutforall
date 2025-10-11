use crate::{errors::AppError, shapes::team::TeamEntry};
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait TeamReader {
    async fn read_all(&self) -> Result<Vec<TeamEntry>, AppError>;
    async fn read_single(&self, team_id: &Uuid) -> Result<TeamEntry, AppError>;
    async fn exists(&self, team_id: &Uuid) -> Result<bool, AppError>;
}
