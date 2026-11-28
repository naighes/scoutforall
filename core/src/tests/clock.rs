//! The time the tests give the core: one place, so that it can be stopped.

use crate::clock::UtcNow;
use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use std::{
    sync::{Arc, OnceLock},
    time::Instant,
};

/// Where the time of the tests starts: noon (UTC) of Thursday 26 November
/// 2026, the same day in every zone the tests run in (`scripts/test.sh`).
pub(crate) fn start() -> DateTime<Utc> {
    NaiveDate::from_ymd_opt(2026, 11, 26)
        .and_then(|day| day.and_hms_opt(12, 0, 0))
        .map(|noon| noon.and_utc())
        .unwrap_or_default()
}

/// Now, in UTC: from [`start`], going on as a stopwatch since the first
/// test asked. The day never changes under a test, whatever day it is
/// run, and two moments still come one after another.
pub(crate) fn now() -> DateTime<Utc> {
    static STARTED: OnceLock<Instant> = OnceLock::new();
    let gone = STARTED.get_or_init(Instant::now).elapsed();
    start() + TimeDelta::from_std(gone).unwrap_or_default()
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
