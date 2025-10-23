use crate::{
    analytics::queue_manager::QueueManager,
    errors::AppError,
    providers::{queue_reader::QueueReader, queue_writer::QueueWriter},
};
use once_cell::sync::OnceCell;
use std::sync::Arc;
use uuid::Uuid;

static GLOBAL_QUEUE_MANAGER: OnceCell<Arc<dyn GlobalQueueManager>> = OnceCell::new();

#[async_trait::async_trait]
pub trait GlobalQueueManager: Send + Sync {
    async fn enqueue(&self, team_id: Uuid, match_id: String) -> Result<(), AppError>;
}

struct QueueManagerWrapper<
    QR: QueueReader + Send + Sync + 'static,
    QW: QueueWriter + Send + Sync + 'static,
> {
    inner: Arc<QueueManager<QR, QW>>,
}

#[async_trait::async_trait]
impl<QR: QueueReader + Send + Sync + 'static, QW: QueueWriter + Send + Sync + 'static>
    GlobalQueueManager for QueueManagerWrapper<QR, QW>
{
    async fn enqueue(&self, team_id: Uuid, match_id: String) -> Result<(), AppError> {
        self.inner.enqueue(team_id, match_id).await
    }
}

// initialize the global queue manager
pub fn init_global_queue_manager<
    QR: QueueReader + Send + Sync + 'static,
    QW: QueueWriter + Send + Sync + 'static,
>(
    queue_manager: Arc<QueueManager<QR, QW>>,
) -> Result<(), String> {
    let wrapper = QueueManagerWrapper {
        inner: queue_manager,
    };
    GLOBAL_QUEUE_MANAGER
        .set(Arc::new(wrapper))
        .map_err(|_| "Global queue manager already initialized".to_string())
}

// enqueue a match for upload (thread-safe, uses global queue manager)
pub async fn enqueue_match_for_upload(team_id: Uuid, match_id: String) -> Result<(), AppError> {
    match GLOBAL_QUEUE_MANAGER.get() {
        Some(manager) => manager.enqueue(team_id, match_id).await,
        None => Err(AppError::IO(crate::errors::IOError::Msg(
            "Queue manager not initialized".to_string(),
        ))),
    }
}
