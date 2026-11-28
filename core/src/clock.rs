//! The core never reads the time: it takes it as an argument, or, where
//! the moments are stamped as they happen (the touches of a scouting),
//! the app gives it what to ask ([`UtcNow`]), its own clock.

use {
    chrono::{DateTime, NaiveDateTime, Utc},
    std::sync::Arc,
};

/// What time it is in UTC, by the clock of the app.
pub type UtcNow = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

/// What time it is on the wall where the app runs, by its clock.
pub type WallNow = Arc<dyn Fn() -> NaiveDateTime + Send + Sync>;
