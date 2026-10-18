use crate::{
    errors::AppError,
    shapes::{listed::Listed, team::TeamEntry},
};
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait TeamReader {
    /// Every team folder: a folder that can't be loaded is unreadable, and
    /// doesn't hide the others.
    async fn read_all(&self) -> Result<Vec<Listed<TeamEntry>>, AppError>;
    async fn read_single(&self, team_id: &Uuid) -> Result<TeamEntry, AppError>;
    async fn exists(&self, team_id: &Uuid) -> Result<bool, AppError>;
}
