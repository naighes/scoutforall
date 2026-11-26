//! The time the tests give the core: one place, so that it can be stopped.

use crate::clock::UtcNow;
use chrono::{DateTime, NaiveDate, Utc};
use std::sync::Arc;

/// Now, in UTC.
pub(crate) fn now() -> DateTime<Utc> {
    Utc::now()
}

/// What a scouting asks for the time of each touch.
pub(crate) fn utc_now() -> UtcNow {
    Arc::new(now)
}

/// Today, for the names of the backups.
pub(crate) fn today() -> NaiveDate {
    now().date_naive()
}

/// This year, for a team.
pub(crate) fn this_year() -> u16 {
    u16::try_from(chrono::Datelike::year(&now())).unwrap_or_default()
}
