//! Sends the upload queue in the background, as WorkManager does on Android:
//! one attempt at a time, again with an exponential backoff while matches
//! are left, a new request while one runs done right after it
//! (`APPEND_OR_REPLACE`). The desktop app turns it on; Android keeps
//! WorkManager, which also runs with the app closed.

use crate::Uploads;
use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock},
    thread,
    time::Duration,
};

/// The wait before the first new attempt, doubled each time, as the backoff
/// of the `UploadWorker`.
pub const FIRST_RETRY: Duration = Duration::from_secs(60);
/// The longest wait between two attempts.
pub const LAST_RETRY: Duration = Duration::from_secs(60 * 60);

/// The wait after `failures` attempts in a row with matches left.
pub fn retry_delay(first: Duration, last: Duration, failures: u32) -> Duration {
    first
        .checked_mul(2u32.saturating_pow(failures.saturating_sub(1)))
        .map_or(last, |d| d.min(last))
}

type Attempt = Box<dyn Fn() -> Uploads + Send + Sync>;

#[derive(Default)]
struct State {
    /// a thread is sending, or waiting to try again
    running: bool,
    /// asked while it was running: one more attempt, right away
    requested: bool,
}

pub struct UploadSender {
    /// what an attempt does; none until the app turns the sender on
    attempt: OnceLock<Attempt>,
    state: Mutex<State>,
    wake: Condvar,
    first: Duration,
    last: Duration,
}

impl UploadSender {
    pub fn new(first: Duration, last: Duration) -> Arc<Self> {
        Arc::new(Self {
            attempt: OnceLock::new(),
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
            first,
            last,
        })
    }

    /// From now on each request sends; the first one is made now.
    pub fn start(self: &Arc<Self>, attempt: Attempt) {
        let _ = self.attempt.set(attempt);
        self.request();
    }

    /// Sends the queue on a thread, or right after the attempt running.
    /// Nothing before `start`: Android sends with WorkManager.
    pub fn request(self: &Arc<Self>) {
        if self.attempt.get().is_none() {
            return;
        }
        let mut state = self.state();
        match state.running {
            true => {
                state.requested = true;
                self.wake.notify_all();
            }
            false => {
                state.running = true;
                drop(state);
                let sender = self.clone();
                thread::spawn(move || sender.run());
            }
        }
    }

    /// Whether a thread is sending or waiting to try again.
    #[cfg(test)]
    pub fn running(&self) -> bool {
        self.state().running
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn run(&self) {
        let mut failures = 0u32;
        loop {
            self.state().requested = false;
            let left = self
                .attempt
                .get()
                .map_or(Uploads::Done, |attempt| attempt());
            failures = match left {
                Uploads::Done => 0,
                Uploads::Retry => failures.saturating_add(1),
            };
            let wait = (failures > 0).then(|| retry_delay(self.first, self.last, failures));
            let mut state = self.state();
            // no request and nothing left: the thread ends
            match (state.requested, wait) {
                (true, _) => {}
                (false, None) => {
                    state.running = false;
                    return;
                }
                (false, Some(wait)) => {
                    let _ = self
                        .wake
                        .wait_timeout_while(state, wait, |s| !s.requested)
                        .unwrap_or_else(|p| p.into_inner());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU32, Ordering},
        time::Instant,
    };

    fn wait_until(what: impl Fn() -> bool) -> bool {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            if what() {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        what()
    }

    #[test]
    fn the_backoff_doubles_from_a_minute_up_to_an_hour() {
        let delay = |n| retry_delay(FIRST_RETRY, LAST_RETRY, n).as_secs();
        assert_eq!(
            (1..=8).map(delay).collect::<Vec<_>>(),
            vec![60, 120, 240, 480, 960, 1920, 3600, 3600]
        );
        assert_eq!(delay(u32::MAX), 3600);
    }

    #[test]
    fn nothing_is_sent_before_the_start() {
        let sender = UploadSender::new(FIRST_RETRY, LAST_RETRY);
        sender.request();
        assert!(!sender.running());
    }

    #[test]
    fn matches_left_are_tried_again_until_the_queue_is_empty() {
        let sender = UploadSender::new(Duration::from_millis(10), Duration::from_millis(20));
        let attempts = Arc::new(AtomicU32::new(0));
        let counted = attempts.clone();
        sender.start(Box::new(move || {
            match counted.fetch_add(1, Ordering::SeqCst) {
                0..=2 => Uploads::Retry,
                _ => Uploads::Done,
            }
        }));
        assert!(wait_until(|| !sender.running()));
        assert_eq!(attempts.load(Ordering::SeqCst), 4);
    }

    /// Without network it waits for the backoff, not hammering.
    #[test]
    fn a_failure_waits_for_the_backoff() {
        let sender = UploadSender::new(Duration::from_secs(60), Duration::from_secs(60));
        let attempts = Arc::new(AtomicU32::new(0));
        let counted = attempts.clone();
        sender.start(Box::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Uploads::Retry
        }));
        assert!(wait_until(|| attempts.load(Ordering::SeqCst) == 1));
        thread::sleep(Duration::from_millis(100));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert!(sender.running());
        // a new match: tried at once, then the backoff again
        sender.request();
        assert!(wait_until(|| attempts.load(Ordering::SeqCst) == 2));
        thread::sleep(Duration::from_millis(100));
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    /// One at a time: a request while sending runs after it, not beside it.
    #[test]
    fn a_request_while_sending_runs_after_it() {
        let sender = UploadSender::new(FIRST_RETRY, LAST_RETRY);
        let attempts = Arc::new(AtomicU32::new(0));
        let inside = Arc::new(AtomicU32::new(0));
        let (counted, busy) = (attempts.clone(), inside.clone());
        sender.start(Box::new(move || {
            assert_eq!(busy.fetch_add(1, Ordering::SeqCst), 0);
            counted.fetch_add(1, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(50));
            busy.fetch_sub(1, Ordering::SeqCst);
            Uploads::Done
        }));
        assert!(wait_until(|| attempts.load(Ordering::SeqCst) == 1));
        sender.request();
        sender.request();
        assert!(wait_until(|| !sender.running()));
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        // done: the next request starts a new thread
        sender.request();
        assert!(wait_until(|| attempts.load(Ordering::SeqCst) == 3));
    }
}
