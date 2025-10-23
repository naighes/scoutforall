use crate::{
    analytics::queue::UploadQueue,
    errors::AppError,
    providers::{queue_reader::QueueReader, queue_writer::QueueWriter},
};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

// thread-safe queue manager to prevent race conditions
pub struct QueueManager<
    QR: QueueReader + Send + Sync + 'static,
    QW: QueueWriter + Send + Sync + 'static,
> {
    queue_reader: Arc<QR>,
    queue_writer: Arc<QW>,
    lock: Arc<Mutex<()>>,
}

impl<QR: QueueReader + Send + Sync + 'static, QW: QueueWriter + Send + Sync + 'static>
    QueueManager<QR, QW>
{
    pub fn new(queue_reader: Arc<QR>, queue_writer: Arc<QW>) -> Self {
        Self {
            queue_reader,
            queue_writer,
            lock: Arc::new(Mutex::new(())),
        }
    }

    // enqueue a match for upload (thread-safe)
    pub async fn enqueue(&self, team_id: Uuid, match_id: String) -> Result<(), AppError> {
        let _ = self.lock.lock().await;
        let mut queue = self.queue_reader.load().await?;
        queue.enqueue(team_id, match_id);
        self.queue_writer.save(&queue).await
    }

    // load queue (thread-safe)
    pub async fn load(&self) -> Result<UploadQueue, AppError> {
        let _ = self.lock.lock().await;
        self.queue_reader.load().await
    }

    // remove successfully uploaded match (thread-safe)
    pub async fn dequeue(&self, match_id: &str) -> Result<(), AppError> {
        let _ = self.lock.lock().await;
        let mut queue = self.queue_reader.load().await?;
        queue.dequeue(match_id);
        self.queue_writer.save(&queue).await
    }

    // mark retry for failed upload (thread-safe)
    pub async fn mark_retry(&self, match_id: &str) -> Result<(), AppError> {
        let _ = self.lock.lock().await;
        let mut queue = self.queue_reader.load().await?;
        queue.mark_retry(match_id);
        self.queue_writer.save(&queue).await
    }
}
