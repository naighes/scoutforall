use crate::{
    analytics::queue_manager::QueueManager,
    errors::AppError,
    providers::{queue_reader::QueueReader, queue_writer::QueueWriter},
};
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

static GLOBAL_QUEUE_MANAGER: OnceLock<Arc<dyn GlobalQueueManager>> = OnceLock::new();

// whether the user allows uploads: the worker always runs and checks it at
// each tick, so the setting applies without a restart
static UPLOADS_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_uploads_enabled(enabled: bool) {
    UPLOADS_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn uploads_enabled() -> bool {
    UPLOADS_ENABLED.load(Ordering::Relaxed)
}

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
