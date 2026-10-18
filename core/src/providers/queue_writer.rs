use crate::{analytics::queue::UploadQueue, errors::AppError};
use async_trait::async_trait;

#[async_trait]
pub trait QueueWriter {
    async fn save(&self, queue: &UploadQueue) -> Result<(), AppError>;
}
