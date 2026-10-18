use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PendingUpload {
    pub team_id: Uuid,
    pub match_id: String,
    pub queued_at: i64, // timestamp
    pub retry_count: u8,
}

impl PendingUpload {
    // the match id is unique only within its team
    fn is(&self, team_id: &Uuid, match_id: &str) -> bool {
        self.team_id == *team_id && self.match_id == match_id
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct UploadQueue {
    pending: Vec<PendingUpload>,
}

impl UploadQueue {
    // add match to upload queue
    pub fn enqueue(&mut self, team_id: Uuid, match_id: String) {
        // avoid duplicates
        if self.pending.iter().any(|p| p.is(&team_id, &match_id)) {
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
    pub fn dequeue(&mut self, team_id: &Uuid, match_id: &str) {
        self.pending.retain(|p| !p.is(team_id, match_id));
    }

    // increment retry counter for failed upload and move it to the back of the
    // queue: the worker only uploads the first match, which must not hold up
    // the others when it keeps failing
    pub fn mark_retry(&mut self, team_id: &Uuid, match_id: &str) {
        if let Some(index) = self.pending.iter().position(|p| p.is(team_id, match_id)) {
            let mut item = self.pending.remove(index);
            // saved with the queue, it keeps growing across runs
            item.retry_count = item.retry_count.saturating_add(1);
            self.pending.push(item);
        }
    }

    // check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}
