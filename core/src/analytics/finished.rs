//! Where a finished match goes for the statistics: the upload queue, if the
//! user allows them. The TUI uses the global queue and flag; an app with
//! more than one data folder in a process (the tests of `mobile`) gives
//! each scouting its own.

use crate::{
    analytics::{
        global::{enqueue_match_for_upload, uploads_enabled},
        queue_manager::QueueManager,
    },
    providers::{queue_reader::QueueReader, queue_writer::QueueWriter},
};
use async_trait::async_trait;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

#[async_trait]
pub trait FinishedMatches: Send + Sync {
    /// The match is over: queued for the statistics, if the user allows
    /// them. The match stays saved anyway: an upload not queued is lost.
    async fn finished(&self, team_id: Uuid, match_id: String);
}

/// The global queue and flag, of the TUI.
pub struct GlobalFinishedMatches;

#[async_trait]
impl FinishedMatches for GlobalFinishedMatches {
    async fn finished(&self, team_id: Uuid, match_id: String) {
        // the global flag: the settings may change while scouting
        if uploads_enabled() {
            let _ = enqueue_match_for_upload(team_id, match_id).await;
        }
    }
}

/// A queue of its own, and the choice of the user, which may change while
/// scouting.
pub struct QueuedFinishedMatches<
    QR: QueueReader + Send + Sync + 'static,
    QW: QueueWriter + Send + Sync + 'static,
> {
    pub queue: Arc<QueueManager<QR, QW>>,
    pub enabled: Arc<AtomicBool>,
}

#[async_trait]
impl<QR: QueueReader + Send + Sync + 'static, QW: QueueWriter + Send + Sync + 'static>
    FinishedMatches for QueuedFinishedMatches<QR, QW>
{
    async fn finished(&self, team_id: Uuid, match_id: String) {
        if self.enabled.load(Ordering::Relaxed) {
            let _ = self.queue.enqueue(team_id, match_id).await;
        }
    }
}
