use crate::{analytics::queue::UploadQueue, errors::AppError};
use async_trait::async_trait;

#[async_trait]
pub trait QueueReader {
    async fn load(&self) -> Result<UploadQueue, AppError>;
}
