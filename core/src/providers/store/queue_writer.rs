use crate::{
    analytics::queue::UploadQueue,
    errors::AppError,
    providers::{queue_writer::QueueWriter, store::write_json},
    storage::{Store, layout},
};
use async_trait::async_trait;
use std::sync::Arc;

pub struct StoreQueueWriter(Arc<dyn Store>);

impl StoreQueueWriter {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self(store)
    }
}

#[async_trait]
impl QueueWriter for StoreQueueWriter {
    async fn save(&self, queue: &UploadQueue) -> Result<(), AppError> {
        write_json(&*self.0, layout::UPLOAD_QUEUE, queue).await
    }
}
