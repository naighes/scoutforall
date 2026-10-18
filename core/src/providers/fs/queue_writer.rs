use crate::{
    analytics::queue::UploadQueue,
    errors::{AppError, IOError},
    providers::queue_writer::QueueWriter,
};
use async_trait::async_trait;
use serde_json::to_string_pretty;
use std::path::{Path, PathBuf};
use tokio::fs::{create_dir_all, write};

pub struct FileSystemQueueWriter(PathBuf);

impl FileSystemQueueWriter {
    pub fn new(path: &Path) -> Self {
        Self(path.to_path_buf())
    }
}

#[async_trait]
impl QueueWriter for FileSystemQueueWriter {
    async fn save(&self, queue: &UploadQueue) -> Result<(), AppError> {
        let contents = to_string_pretty(queue).map_err(|e| AppError::IO(IOError::from(e)))?;
        if let Some(parent) = self.0.parent() {
            create_dir_all(parent)
                .await
                .map_err(|e| AppError::IO(IOError::from(e)))?;
        }
        write(&self.0, contents)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))
    }
}
