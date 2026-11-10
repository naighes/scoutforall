//! The free space where the recordings go: what a match takes said before
//! recording, the recording stopped cleanly (its MP4 made, readable)
//! before the disk is full.

use std::path::Path;

/// Left free on the disk, for the system and the other apps.
pub const RESERVE: u64 = 500_000_000;

/// How long a match is taken to last, for the space it needs.
pub const MATCH_SECONDS: u64 = 2 * 3600;

/// The space free where `folder` is, or the nearest folder above it that
/// exists (made when recording starts); none if the system doesn't say.
pub fn free(folder: &Path) -> Option<u64> {
    folder
        .ancestors()
        .find(|p| p.is_dir())
        .and_then(|p| fs4::available_space(p).ok())
}

/// The bytes a match recorded at `bits_per_second` needs: twice the
/// recording, as its MP4 is made next to it before the raw files go, and
/// the reserve.
pub fn needed(bits_per_second: u64) -> u64 {
    (bits_per_second / 8)
        .saturating_mul(MATCH_SECONDS)
        .saturating_mul(2)
        .saturating_add(RESERVE)
}

/// Whether a recording of `written` bytes so far may go on with `free`
/// bytes left: its MP4 still fits, with the reserve. Not known, on.
pub fn may_go_on(free: Option<u64>, written: u64) -> bool {
    free.is_none_or(|f| f >= written.saturating_add(RESERVE))
}

/// What the free space allows, said before recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Room {
    /// not known: nothing said
    Unknown,
    /// a match fits
    Enough { free: u64 },
    /// it stops before a match ends
    Short { free: u64, needed: u64 },
    /// not even the reserve: not started
    Full { free: u64 },
}

impl Room {
    pub fn of(free: Option<u64>, bits_per_second: Option<u64>) -> Room {
        match (free, bits_per_second.map(needed)) {
            (None, _) => Room::Unknown,
            (Some(free), _) if !may_go_on(Some(free), 0) => Room::Full { free },
            (Some(free), Some(needed)) if free < needed => Room::Short { free, needed },
            (Some(free), _) => Room::Enough { free },
        }
    }

    pub fn can_record(self) -> bool {
        !matches!(self, Room::Full { .. })
    }
}

/// Bytes in GB, as the screen says them.
pub fn gb(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let gb = bytes as f64 / 1e9;
    format!("{gb:.1}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_match_needs() {
        // 720p: 3.5 Mbit/s for two hours is 3.15 GB, twice with its MP4
        assert_eq!(needed(3_500_000), 6_300_000_000 + RESERVE);
        assert_eq!(needed(u64::MAX), u64::MAX);
    }

    #[test]
    fn going_on_while_the_mp4_fits() {
        assert!(may_go_on(None, u64::MAX));
        assert!(may_go_on(Some(RESERVE + 1_000), 1_000));
        assert!(!may_go_on(Some(RESERVE + 999), 1_000));
        assert!(!may_go_on(Some(0), 0));
    }

    #[test]
    fn the_room_before_recording() {
        let need = needed(3_500_000);
        assert_eq!(Room::of(None, Some(3_500_000)), Room::Unknown);
        assert_eq!(
            Room::of(Some(need), Some(3_500_000)),
            Room::Enough { free: need }
        );
        assert_eq!(
            Room::of(Some(need - 1), Some(3_500_000)),
            Room::Short {
                free: need - 1,
                needed: need
            }
        );
        // the bitrate not known yet (a source on the network): only full
        assert_eq!(
            Room::of(Some(RESERVE), None),
            Room::Enough { free: RESERVE }
        );
        assert_eq!(
            Room::of(Some(RESERVE - 1), None),
            Room::Full { free: RESERVE - 1 }
        );
        assert!(!Room::Full { free: 0 }.can_record());
        assert!(Room::Short { free: 0, needed: 1 }.can_record());
        assert_eq!(gb(6_300_000_000), "6.3");
    }

    #[test]
    fn the_free_space_of_a_folder_not_made_yet() {
        let dir = tempfile::tempdir().unwrap();
        let free = free(&dir.path().join("rec").join("more"));
        assert!(free.is_some_and(|f| f > 0));
    }
}
