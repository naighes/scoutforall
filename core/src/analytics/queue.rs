use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PendingUpload {
    pub team_id: Uuid,
    pub match_id: String,
    pub queued_at: i64, // timestamp
    pub retry_count: u8,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct UploadQueue {
    pending: Vec<PendingUpload>,
}

impl UploadQueue {
    // add match to upload queue
    pub fn enqueue(&mut self, team_id: Uuid, match_id: String) {
        // avoid duplicates
        if self.pending.iter().any(|p| p.match_id == match_id) {
            return;
        }
        self.pending.push(PendingUpload {
            team_id,
            match_id,
            queued_at: chrono::Utc::now().timestamp(),
            retry_count: 0,
        });
    }

    // get next item to process
    pub fn peek(&self) -> Option<&PendingUpload> {
        self.pending.first()
    }

    // remove successfully uploaded match
    pub fn dequeue(&mut self, match_id: &str) {
        self.pending.retain(|p| p.match_id != match_id);
    }

    // increment retry counter for failed upload
    pub fn mark_retry(&mut self, match_id: &str) {
        if let Some(item) = self.pending.iter_mut().find(|p| p.match_id == match_id) {
            item.retry_count += 1;
        }
    }

    // check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}
