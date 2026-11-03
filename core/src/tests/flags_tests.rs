//! The flags of an event, saved as one number in the last column: the
//! inverted P1 and the reception formation, the bits of later versions kept.

#[cfg(test)]
mod tests {
    use crate::shapes::{
        court_zone::{CourtZoneEnum, Direction},
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
        // bit 8 and a reception with both its bits
        let later = EventFlags::parse("262").unwrap();
        assert_eq!(later.to_string(), "262");
        assert_eq!(later.reception(), ReceptionSchemeEnum::Three);
        assert_eq!(later.target(), None);
        let changed = later
            .with_inverted_p1(true)
            .with_reception(ReceptionSchemeEnum::Four)
            .with_target(Some(CourtZoneEnum::Z1));
        assert_eq!(changed.to_string(), "267");
        // a zone this version doesn't know (code 31) is none, and kept
        let later = EventFlags::parse("248").unwrap();
        assert_eq!(later.target(), None);
        assert_eq!(later.with_inverted_p1(true).to_string(), "249");
        assert!(EventFlags::parse("x").is_err());
        assert!(EventFlags::parse("70000").is_err());
    }

    /// Every zone in bits 3 to 7, apart from the other flags; none by
    /// default, and taken away.
    #[test]
    fn the_target_zone() {
        let base = EventFlags::default()
            .with_inverted_p1(true)
            .with_reception(ReceptionSchemeEnum::Two);
        for (i, zone) in CourtZoneEnum::ALL.into_iter().enumerate() {
            let flags = base.with_target(Some(zone));
            assert_eq!(flags.target(), Some(zone));
            assert!(flags.inverted_p1());
            assert_eq!(flags.reception(), ReceptionSchemeEnum::Two);
            assert_eq!(flags.to_string(), ((i + 1) * 8 + 5).to_string());
            assert_eq!(flags.with_target(None), base);
        }
        assert_eq!(EventFlags::default().target(), None);
        assert_eq!(
            EventFlags::parse("8").unwrap().target(),
            Some(CourtZoneEnum::Z1)
        );
        assert_eq!(
            EventFlags::parse("168").unwrap().target(),
            Some(CourtZoneEnum::Z19)
        );
    }

    /// The cursor on the court: along the rows, between the rows of five
    /// and the rows of three, still at the edges.
    #[test]
    fn the_cursor_moves_on_the_court() {
        use CourtZoneEnum::*;
        use Direction::*;
        let path =
            |from: CourtZoneEnum, moves: &[Direction]| moves.iter().fold(from, |z, d| z.moved(*d));
        assert_eq!(path(Z8, &[Up]), Z38);
        assert_eq!(path(Z8, &[Up, Up]), Z3);
        assert_eq!(path(Z8, &[Left]), Z78);
        assert_eq!(path(Z4, &[Down]), Z47);
        assert_eq!(path(Z43, &[Down]), Z47);
        assert_eq!(path(Z32, &[Down]), Z29);
        assert_eq!(path(Z47, &[Right]), Z38);
        assert_eq!(path(Z29, &[Down]), Z9);
        assert_eq!(path(Z68, &[Down]), Z6);
        assert_eq!(path(Z1, &[Up, Up, Up, Up]), Z2);
        assert_eq!(path(Z4, &[Up, Left]), Z4);
        assert_eq!(path(Z1, &[Down, Right]), Z1);
        assert_eq!(path(Z19, &[Right]), Z19);
        // every zone reachable from the center
        let mut seen = std::collections::HashSet::from([CourtZoneEnum::CENTER]);
        let mut todo = vec![CourtZoneEnum::CENTER];
        while let Some(z) = todo.pop() {
            for d in [Up, Down, Left, Right] {
                if seen.insert(z.moved(d)) {
                    todo.push(z.moved(d));
                }
            }
        }
        assert_eq!(seen.len(), CourtZoneEnum::ALL.len());
        assert_eq!(CourtZoneEnum::ALL.iter().filter(|z| z.is_zone()).count(), 9);
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
