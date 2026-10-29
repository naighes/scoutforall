//! The flags of an event, saved as one number in the last column: the
//! inverted P1 and the reception formation, the bits of later versions kept.

#[cfg(test)]
mod tests {
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum},
        flags::{EventFlags, ReceptionSchemeEnum},
        snapshot::EventEntry,
    };
    use chrono::DateTime;
    use csv::{ReaderBuilder, WriterBuilder};
    use uuid::Uuid;

    fn event(flags: EventFlags) -> EventEntry {
        EventEntry {
            timestamp: DateTime::parse_from_rfc3339("2026-10-29T20:00:00Z")
                .unwrap()
                .to_utc(),
            event_type: EventTypeEnum::P,
            player: Some(Uuid::nil()),
            eval: Some(EvalEnum::Positive),
            target_player: None,
            technique: None,
            flags,
        }
    }

    fn to_csv(events: &[EventEntry]) -> String {
        let mut writer = WriterBuilder::new().has_headers(false).from_writer(vec![]);
        for e in events {
            writer.serialize(e).unwrap();
        }
        String::from_utf8(writer.into_inner().unwrap()).unwrap()
    }

    fn from_csv(csv: &str) -> Result<Vec<EventEntry>, csv::Error> {
        ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_reader(csv.as_bytes())
            .deserialize()
            .collect()
    }

    /// None by default: the canonical P1, the reception with three.
    #[test]
    fn no_flags_by_default() {
        let flags = EventFlags::default();
        assert!(!flags.inverted_p1());
        assert_eq!(flags.reception(), ReceptionSchemeEnum::Three);
        assert_eq!(flags.to_string(), "");
    }

    /// The P1 and the reception are set apart, each leaving the other as
    /// it was.
    #[test]
    fn each_flag_on_its_own() {
        let flags = EventFlags::default()
            .with_inverted_p1(true)
            .with_reception(ReceptionSchemeEnum::Two);
        assert!(flags.inverted_p1());
        assert_eq!(flags.reception(), ReceptionSchemeEnum::Two);
        assert_eq!(flags.to_string(), "5");
        let flags = flags.with_reception(ReceptionSchemeEnum::Four);
        assert_eq!(
            (flags.inverted_p1(), flags.to_string().as_str()),
            (true, "3")
        );
        let flags = flags.with_inverted_p1(false);
        assert_eq!(flags.reception(), ReceptionSchemeEnum::Four);
        assert_eq!(flags.to_string(), "2");
        for scheme in ReceptionSchemeEnum::ALL {
            assert_eq!(
                EventFlags::default().with_reception(scheme).reception(),
                scheme
            );
        }
    }

    /// The bits of a later version stay when this one changes its own; a
    /// reception told in a way it doesn't know is three.
    #[test]
    fn unknown_bits_are_kept() {
        let later = EventFlags::parse("0").unwrap();
        assert_eq!(later, EventFlags::default());
        let later = EventFlags::parse("70").unwrap();
        assert_eq!(later.to_string(), "70");
        assert_eq!(later.reception(), ReceptionSchemeEnum::Three);
        let changed = later
            .with_inverted_p1(true)
            .with_reception(ReceptionSchemeEnum::Four);
        assert_eq!(changed.to_string(), "67");
        assert!(EventFlags::parse("x").is_err());
        assert!(EventFlags::parse("70000").is_err());
    }

    /// Saved last, after the technique, and read back; a row of v0.10.1 or
    /// older, without the column, has none.
    #[test]
    fn saved_as_the_last_column() {
        let flags = EventFlags::default().with_reception(ReceptionSchemeEnum::Four);
        let events = [event(flags), event(EventFlags::default())];
        let csv = to_csv(&events);
        let nil = Uuid::nil();
        assert_eq!(
            csv,
            format!(
                "2026-10-29T20:00:00Z,P,{nil},+,,,2\n\
                 2026-10-29T20:00:00Z,P,{nil},+,,,\n"
            )
        );
        let read = from_csv(&csv).unwrap();
        assert_eq!(
            read.iter().map(|e| e.flags).collect::<Vec<_>>(),
            [flags, EventFlags::default()]
        );
        let old = from_csv(&format!(
            "2026-10-29T20:00:00Z,P,{nil},+,,\n2026-10-29T20:00:00Z,P,{nil},+,\n"
        ))
        .unwrap();
        assert!(old.iter().all(|e| e.flags == EventFlags::default()));
        assert!(from_csv(&format!("2026-10-29T20:00:00Z,P,{nil},+,,,x\n")).is_err());
    }
}
