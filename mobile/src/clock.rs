//! What time it is, asked of a clock instead of the system: the app's
//! own, or a fixed one in the tests (Rust and the apps alike, since the
//! apps can make one of their own and give it to [`crate::Scout4All`]).
//!
//! The practice of the codebase: `core` takes the time as an argument,
//! the edges (`mobile`, the desktop, Android) ask a [`Clock`]; nothing
//! reads `Local::now()` / `Utc::now()` / `LocalDate.now()` by itself.

use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, Offset, Utc};
use scout4all_core::clock::{UtcNow, WallNow};
use std::{sync::Arc, time::SystemTime};

/// What time it is.
#[uniffi::export(with_foreign)]
pub trait Clock: Send + Sync {
    /// the time on the wall where the app runs
    fn now(&self) -> NaiveDateTime;
    /// the same moment, anywhere
    fn utc_now(&self) -> SystemTime;
}

// the time on the wall, as text for the apps (`2026-11-26T10:15:30`):
// what `java.time.LocalDateTime` writes and reads, seconds optional.
uniffi::custom_type!(NaiveDateTime, String, {
    remote,
    lower: |time| time.format("%Y-%m-%dT%H:%M:%S%.f").to_string(),
    try_lift: |time| Ok(wall_time(&time)?),
});

/// The time on the wall as `java.time.LocalDateTime` writes it: to the
/// minute, the second, or a fraction of it.
fn wall_time(text: &str) -> Result<NaiveDateTime, chrono::ParseError> {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M"))
}

/// The clock of the system.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> NaiveDateTime {
        Local::now().naive_local()
    }

    fn utc_now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// A clock stopped at a moment, for the tests.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock {
    pub now: NaiveDateTime,
    pub utc_now: DateTime<Utc>,
}

impl FixedClock {
    /// Stopped at `now` on the wall, `offset` from UTC.
    pub fn at(now: NaiveDateTime, offset: FixedOffset) -> FixedClock {
        FixedClock {
            now,
            utc_now: now.and_utc() - offset,
        }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> NaiveDateTime {
        self.now
    }

    fn utc_now(&self) -> SystemTime {
        self.utc_now.into()
    }
}

/// What's worked out of the time a clock gives.
pub trait ClockExt {
    fn today(&self) -> NaiveDate;
    /// the offset from UTC of where the app runs: the wall and UTC apart,
    /// to the minute (a fixed clock is a zone of its own)
    fn offset(&self) -> FixedOffset;
    /// now, with the offset of where the app runs
    fn zoned_now(&self) -> DateTime<FixedOffset>;
    /// now in UTC
    fn utc(&self) -> DateTime<Utc>;
}

impl<C: Clock + ?Sized> ClockExt for C {
    fn today(&self) -> NaiveDate {
        self.now().date()
    }

    fn offset(&self) -> FixedOffset {
        let utc = DateTime::<Utc>::from(self.utc_now()).naive_utc();
        let seconds = (self.now() - utc).num_seconds();
        // the two asked a moment apart
        let minutes = (seconds + 30).div_euclid(60);
        i32::try_from(minutes * 60)
            .ok()
            .and_then(FixedOffset::east_opt)
            .unwrap_or(Utc.fix())
    }

    fn zoned_now(&self) -> DateTime<FixedOffset> {
        self.utc().with_timezone(&self.offset())
    }

    fn utc(&self) -> DateTime<Utc> {
        DateTime::<Utc>::from(self.utc_now())
    }
}

/// What the core asks for the time in UTC, by `clock`.
pub fn utc_now_of(clock: Arc<dyn Clock>) -> UtcNow {
    Arc::new(move || clock.utc())
}

/// What the core asks for the time on the wall, by `clock`.
pub fn wall_now_of(clock: Arc<dyn Clock>) -> WallNow {
    Arc::new(move || clock.now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wall_time_as_java_writes_it() {
        for text in [
            "2026-11-26T10:15",
            "2026-11-26T10:15:30",
            "2026-11-26T10:15:30.120",
            "2026-11-26T10:15:30.123456",
            "2026-11-26T10:15:30.123456789",
        ] {
            assert!(wall_time(text).is_ok(), "{text}: {:?}", wall_time(text));
        }
    }

    #[test]
    fn a_fixed_clock_tells_the_same_moment_on_the_wall_and_in_utc() {
        let now = NaiveDate::from_ymd_opt(2026, 3, 9)
            .and_then(|d| d.and_hms_opt(0, 30, 0))
            .unwrap();
        let rome = FixedOffset::east_opt(3600).unwrap();
        let clock = FixedClock::at(now, rome);
        assert_eq!((clock.today(), clock.offset()), (now.date(), rome));
        assert_eq!(clock.zoned_now().to_rfc3339(), "2026-03-09T00:30:00+01:00");
        assert_eq!(
            SystemClock.offset(),
            Local::now().offset().fix(),
            "the system's zone"
        );
        // in UTC it's still the day before
        assert_eq!(
            DateTime::<Utc>::from(clock.utc_now()).to_rfc3339(),
            "2026-03-08T23:30:00+00:00"
        );
    }
}
