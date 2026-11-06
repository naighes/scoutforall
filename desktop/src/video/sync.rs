//! The sync by hand: the event chosen tied to a moment of the video, the
//! others from it; the time written as the clock of the player shows it.

/// The milliseconds of a time in the video: `m:ss`, `h:mm:ss`, with tenths
/// or more (`6:18.5`), or seconds alone (`378`, `378.5`); a comma for the
/// point too.
pub fn parse_time(written: &str) -> Option<u64> {
    let written = written.trim().replace(',', ".");
    let parts: Vec<&str> = written.split(':').collect();
    let (whole, seconds) = match parts.split_last()? {
        (seconds, whole) if whole.len() <= 2 => (whole, *seconds),
        _ => return None,
    };
    let seconds: f64 = match seconds {
        s if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.') => {
            s.parse().ok()?
        }
        _ => return None,
    };
    // past the first part, the seconds and the minutes under 60
    let bounded = whole.is_empty() || seconds < 60.0;
    let units = whole
        .iter()
        .map(
            |p| match p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() {
                true => p.parse::<u64>().ok(),
                false => None,
            },
        )
        .collect::<Option<Vec<u64>>>()?;
    let minutes_bounded = match units.as_slice() {
        [_, minutes] => *minutes < 60,
        _ => true,
    };
    let whole_seconds = units.iter().fold(0u64, |total, u| total * 60 + u) * 60;
    match bounded && minutes_bounded && seconds.is_finite() {
        true => Some(whole_seconds * 1000 + (seconds * 1000.0).round() as u64),
        false => None,
    }
}

/// The time to write in the field: as the clock, with the tenths.
pub fn written(ms: u64) -> String {
    let tenths = ms / 100 % 10;
    format!("{}.{tenths}", super::clock(ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_times_written() {
        assert_eq!(parse_time("6:18"), Some(378_000));
        assert_eq!(parse_time(" 6:18.5 "), Some(378_500));
        assert_eq!(parse_time("6:18,25"), Some(378_250));
        assert_eq!(parse_time("1:34:46"), Some(5_686_000));
        assert_eq!(parse_time("378"), Some(378_000));
        assert_eq!(parse_time("378.5"), Some(378_500));
        assert_eq!(parse_time("0:00"), Some(0));
    }

    #[test]
    fn not_times() {
        for wrong in [
            "", "6:", ":18", "6:60", "1:60:00", "a:18", "6:1x", "1:2:3:4", "-5", "6:18.5.1",
        ] {
            assert_eq!(parse_time(wrong), None, "{wrong:?}");
        }
    }

    #[test]
    fn the_time_in_the_field() {
        assert_eq!(written(378_560), "6:18.5");
        assert_eq!(parse_time(&written(378_560)), Some(378_500));
    }
}
