use crate::{
    analytics::queue::UploadQueue,
    errors::{AppError, IOError},
    providers::queue_reader::QueueReader,
    storage::{Store, layout},
};
use async_trait::async_trait;
use serde_json::from_slice;
use std::sync::Arc;

pub struct StoreQueueReader(Arc<dyn Store>);

impl StoreQueueReader {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self(store)
    }
}

#[async_trait]
impl QueueReader for StoreQueueReader {
    async fn load(&self) -> Result<UploadQueue, AppError> {
        match self.0.read(layout::UPLOAD_QUEUE).await? {
            None => Ok(UploadQueue::default()),
            Some(contents) => from_slice(&contents).map_err(|e| AppError::IO(IOError::from(e))),
        }
    }
}
