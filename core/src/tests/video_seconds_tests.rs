//! The seconds of the video: kept in `config.json` along with the other
//! settings, the defaults when missing, each in its bounds.

#[cfg(test)]
mod tests {
    use crate::settings::{LEAD_SECONDS, STEP_SECONDS, VideoSeconds};

    #[test]
    fn a_config_from_before_them_reads_the_defaults() {
        let read: VideoSeconds = serde_json::from_str(r#"{"language":"it"}"#).unwrap();
        assert_eq!(read, VideoSeconds::default());
        assert_eq!((read.step_s, read.lead_s), (5, 2));
        assert_eq!((read.step_ms(), read.lead_ms()), (5000, 2000));
        // and left out
        assert_eq!(serde_json::to_string(&read).unwrap(), "{}");
    }

    #[test]
    fn the_seconds_saved_are_read_back() {
        let chosen = VideoSeconds {
            step_s: 10,
            lead_s: 0,
        };
        let json = serde_json::to_string(&chosen).unwrap();
        assert_eq!(serde_json::from_str::<VideoSeconds>(&json).unwrap(), chosen);
    }

    /// Written by hand: out of bounds in them, not a number the default,
    /// never an error (the other settings would be lost).
    #[test]
    fn unreadable_or_out_of_bounds_seconds_are_bounded() {
        for (json, expected) in [
            (r#"{"step_s":0,"lead_s":99}"#, (1, 10)),
            (r#"{"step_s":31,"lead_s":-3}"#, (30, 2)),
            (r#"{"step_s":"tanti","lead_s":null}"#, (5, 2)),
            (r#"{"step_s":99999999999,"lead_s":1.5}"#, (30, 2)),
        ] {
            let read: VideoSeconds = serde_json::from_str(json).unwrap();
            assert_eq!((read.step_s, read.lead_s), expected, "{json}");
        }
    }

    #[test]
    fn the_seconds_given_are_saved_in_their_bounds() {
        let given = VideoSeconds {
            step_s: 0,
            lead_s: 60,
        };
        let bounded = given.bounded();
        assert_eq!(
            (bounded.step_s, bounded.lead_s),
            (*STEP_SECONDS.start(), *LEAD_SECONDS.end())
        );
        assert_eq!(VideoSeconds::default().bounded(), VideoSeconds::default());
    }
}
