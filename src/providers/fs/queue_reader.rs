use crate::{
    analytics::queue::UploadQueue,
    errors::{AppError, IOError},
    providers::queue_reader::QueueReader,
};
use async_trait::async_trait;
use serde_json::from_str;
use std::path::{Path, PathBuf};
use tokio::fs::read_to_string;

pub struct FileSystemQueueReader(PathBuf);

impl FileSystemQueueReader {
    pub fn new(path: &Path) -> Self {
        Self(path.to_path_buf())
    }
}

#[async_trait]
impl QueueReader for FileSystemQueueReader {
    async fn load(&self) -> Result<UploadQueue, AppError> {
        if !self.0.exists() {
            Ok(UploadQueue::default())
        } else {
            let contents = read_to_string(&self.0)
                .await
                .map_err(|e| AppError::IO(IOError::from(e)))?;
            from_str(&contents).map_err(|e| AppError::IO(IOError::from(e)))
        }
    }
}
