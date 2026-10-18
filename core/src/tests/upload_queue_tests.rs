//! The analytics upload queue. The worker only ever uploads the first match
//! in the queue: a match that keeps failing must not hold up the others.

#[cfg(test)]
mod tests {
    use crate::analytics::{queue::UploadQueue, upload::without_token};
    use uuid::Uuid;

    const TEAM: Uuid = Uuid::from_u128(1);

    fn queue_of(match_ids: &[&str]) -> UploadQueue {
        match_ids
            .iter()
            .fold(UploadQueue::default(), |mut queue, match_id| {
                queue.enqueue(TEAM, match_id.to_string());
                queue
            })
    }

    fn head(queue: &UploadQueue) -> Option<(&str, u8)> {
        queue.peek().map(|p| (p.match_id.as_str(), p.retry_count))
    }

    #[test]
    fn failed_upload_moves_to_the_back() {
        let mut queue = queue_of(&["a", "b", "c"]);
        queue.mark_retry(&TEAM, "a");
        assert_eq!(head(&queue), Some(("b", 0)));
        queue.dequeue(&TEAM, "b");
        queue.dequeue(&TEAM, "c");
        assert_eq!(head(&queue), Some(("a", 1)));
    }

    #[test]
    fn always_failing_upload_does_not_block_the_queue() {
        let mut queue = queue_of(&["broken", "ok"]);
        // what the worker does on each tick: retry the head, dequeue on success
        (0..4).for_each(|_| match head(&queue) {
            Some(("broken", _)) => queue.mark_retry(&TEAM, "broken"),
            Some((match_id, _)) => {
                let match_id = match_id.to_string();
                queue.dequeue(&TEAM, &match_id)
            }
            None => {}
        });
        assert_eq!(head(&queue), Some(("broken", 3)));
    }

    #[test]
    fn retry_of_unknown_match_changes_nothing() {
        let mut queue = queue_of(&["a", "b"]);
        queue.mark_retry(&TEAM, "z");
        assert_eq!(head(&queue), Some(("a", 0)));
    }

    #[test]
    fn retry_count_saturates() {
        // the counter is saved with the queue and keeps growing across runs
        let mut queue: UploadQueue = serde_json::from_str(
            r#"{"pending":[{"team_id":"00000000-0000-0000-0000-000000000001","match_id":"a","queued_at":0,"retry_count":255}]}"#,
        )
        .expect("valid queue");
        queue.mark_retry(&TEAM, "a");
        assert_eq!(head(&queue), Some(("a", u8::MAX)));
    }

    #[test]
    fn same_match_id_of_two_teams_are_two_uploads() {
        // the match id is date and opponent: unique only within a team
        let (first, second) = (Uuid::new_v4(), Uuid::new_v4());
        let mut queue = UploadQueue::default();
        queue.enqueue(first, "2026-10-18_rivals".to_string());
        queue.enqueue(second, "2026-10-18_rivals".to_string());
        queue.mark_retry(&first, "2026-10-18_rivals");
        let team_of_head = |queue: &UploadQueue| queue.peek().map(|p| p.team_id);
        assert_eq!(team_of_head(&queue), Some(second));
        queue.dequeue(&second, "2026-10-18_rivals");
        assert_eq!(team_of_head(&queue), Some(first));
    }

    #[test]
    fn signed_url_token_stays_out_of_the_log() {
        let url = "https://x.supabase.co/storage/v1/object/upload/sign/bucket/a.zip?token=secret";
        assert_eq!(
            without_token(url),
            "https://x.supabase.co/storage/v1/object/upload/sign/bucket/a.zip"
        );
        assert_eq!(without_token("https://x/a.zip"), "https://x/a.zip");
    }
}
