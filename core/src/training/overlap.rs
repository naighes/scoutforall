//! A team can't be in two places at once: its sessions don't overlap each
//! other, nor a match in programma (TODO/021 §2). Pure: the provider reads
//! the slots, this says which overlap.
//!
//! A slot is `[start, start + minutes)`: slots that touch don't overlap, so
//! a double session on the same day is fine. The times are local, as the
//! user wrote them (the clock time of a match, not converted).

use chrono::{Duration, NaiveDateTime};

/// What a match in programma lasts when it doesn't say, in minutes.
pub const DEFAULT_MATCH_MINUTES: u16 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    Session,
    Match,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Slot {
    pub kind: SlotKind,
    pub start: NaiveDateTime,
    pub minutes: u16,
}

impl Slot {
    pub fn session(start: NaiveDateTime, minutes: u16) -> Self {
        Self {
            kind: SlotKind::Session,
            start,
            minutes,
        }
    }

    /// A match in programma; none minutes is the usual duration.
    pub fn match_of(start: NaiveDateTime, minutes: Option<u16>) -> Self {
        Self {
            kind: SlotKind::Match,
            start,
            minutes: minutes.unwrap_or(DEFAULT_MATCH_MINUTES),
        }
    }

    fn end(&self) -> NaiveDateTime {
        self.start
            .checked_add_signed(Duration::minutes(i64::from(self.minutes)))
            .unwrap_or(NaiveDateTime::MAX)
    }

    fn overlaps(&self, other: &Slot) -> bool {
        self.start < other.end() && other.start < self.end()
    }
}

/// The slots of `others` that overlap `slot`, by start.
pub fn clashes(slot: &Slot, others: &[Slot]) -> Vec<Slot> {
    let mut found: Vec<Slot> = others
        .iter()
        .filter(|other| slot.overlaps(other))
        .copied()
        .collect();
    found.sort_by_key(|s| (s.start, s.minutes));
    found
}
