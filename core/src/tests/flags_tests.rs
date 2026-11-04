//! The flags of an event, saved as one number in the last column: the
//! inverted P1 and the reception formation, the bits of later versions kept.

#[cfg(test)]
mod tests {
    use crate::shapes::{
        court_zone::{CourtCell, CourtSide, CourtTarget, CourtZoneEnum, Direction, SubzoneEnum},
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
        // bit 10 and a reception with both its bits
        let later = EventFlags::parse("1030").unwrap();
        assert_eq!(later.to_string(), "1030");
        assert_eq!(later.reception(), ReceptionSchemeEnum::Three);
        assert_eq!(later.target(), None);
        let changed = later
            .with_inverted_p1(true)
            .with_reception(ReceptionSchemeEnum::Four)
            .with_target(Some(CourtTarget::Zone(1)));
        assert_eq!(changed.to_string(), "1035");
        // a target this version doesn't know (code 127) is none, and kept
        let later = EventFlags::parse("1016").unwrap();
        assert_eq!(later.target(), None);
        assert_eq!(later.with_inverted_p1(true).to_string(), "1017");
        assert!(EventFlags::parse("x").is_err());
        assert!(EventFlags::parse("70000").is_err());
    }

    /// Every target in bits 3 to 9, apart from the other flags: the nine
    /// zones, the 36 cells, the 60 pairs side by side; none by default, and
    /// taken away. The zones keep the codes 1 to 9 of before the cells.
    #[test]
    fn the_target() {
        let base = EventFlags::default()
            .with_inverted_p1(true)
            .with_reception(ReceptionSchemeEnum::Two);
        let all = CourtTarget::all();
        assert_eq!(all.len(), 9 + 36 + 60);
        for (i, target) in all.into_iter().enumerate() {
            let flags = base.with_target(Some(target));
            assert_eq!(flags.target(), Some(target));
            assert!(flags.inverted_p1());
            assert_eq!(flags.reception(), ReceptionSchemeEnum::Two);
            assert_eq!(flags.to_string(), ((i + 1) * 8 + 5).to_string());
            assert_eq!(flags.with_target(None), base);
        }
        assert_eq!(EventFlags::default().target(), None);
        assert_eq!(
            EventFlags::parse("8").unwrap().target(),
            Some(CourtTarget::Zone(1))
        );
        assert_eq!(
            EventFlags::parse("72").unwrap().target(),
            Some(CourtTarget::Zone(9))
        );
    }

    fn cell(text: &str) -> CourtCell {
        let mut chars = text.chars();
        let zone = chars.next().and_then(|c| c.to_digit(10)).unwrap();
        let subzone = chars.next().and_then(SubzoneEnum::of).unwrap();
        CourtCell::new(u8::try_from(zone).unwrap(), subzone).unwrap()
    }

    /// The court as our server sees it, their end line at the top: zone 1
    /// at the top on the left, each zone in the subzones of DataVolley (A
    /// behind on the right of who plays there, then anticlockwise).
    #[test]
    fn the_cells_as_datavolley_tells_them() {
        let rows: Vec<String> = CourtCell::grid()
            .iter()
            .map(|row| {
                row.iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                "1A 1D 6A 6D 5A 5D",
                "1B 1C 6B 6C 5B 5C",
                "9A 9D 8A 8D 7A 7D",
                "9B 9C 8B 8C 7B 7C",
                "2A 2D 3A 3D 4A 4D",
                "2B 2C 3B 3C 4B 4C",
            ]
        );
        for row in CourtCell::grid() {
            for c in row {
                let (r, col) = c.place();
                assert_eq!(CourtCell::at(r, col), Some(c));
            }
        }
    }

    /// Our court, seen from the bench with the net at the top: theirs
    /// turned round, the same cells; the cursor moves as shown.
    #[test]
    fn our_court_is_theirs_turned_round() {
        let rows: Vec<String> = CourtCell::shown(CourtSide::Ours)
            .iter()
            .map(|row| {
                row.iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        assert_eq!(
            rows,
            vec![
                "4C 4B 3C 3B 2C 2B",
                "4D 4A 3D 3A 2D 2A",
                "7C 7B 8C 8B 9C 9B",
                "7D 7A 8D 8A 9D 9A",
                "5C 5B 6C 6B 1C 1B",
                "5D 5A 6D 6A 1D 1A",
            ]
        );
        assert_eq!(CourtCell::shown(CourtSide::Theirs), CourtCell::grid());
        let zones: Vec<Vec<CourtZoneEnum>> = CourtZoneEnum::shown(CourtSide::Ours);
        assert_eq!(
            zones.first().map(Vec::as_slice),
            Some(
                &[
                    CourtZoneEnum::Z4,
                    CourtZoneEnum::Z43,
                    CourtZoneEnum::Z3,
                    CourtZoneEnum::Z32,
                    CourtZoneEnum::Z2
                ][..]
            )
        );
        // 8D: right to 8A on ours, to 7A on theirs
        let at = cell("8D");
        assert_eq!(at.moved_on(CourtSide::Ours, Direction::Right), cell("8A"));
        assert_eq!(at.moved_on(CourtSide::Theirs, Direction::Right), cell("7A"));
        // up to 8C, over it on ours (under it on theirs)
        assert_eq!(at.moved_on(CourtSide::Ours, Direction::Up), cell("8C"));
        assert_eq!(at.moved_on(CourtSide::Theirs, Direction::Down), cell("8C"));
    }

    /// Two cells side by side across or down, not on a corner: of two
    /// zones they are on the line between them (a conflict zone), of one
    /// zone in it.
    #[test]
    fn two_cells_side_by_side() {
        let pair = |a: &str, b: &str| CourtTarget::pair(cell(a), cell(b));
        let zone = |a: &str, b: &str| pair(a, b).map(|t| t.court_zone());
        assert_eq!(zone("1D", "6A"), Some(CourtZoneEnum::Z61));
        assert_eq!(zone("6A", "1D"), Some(CourtZoneEnum::Z61));
        assert_eq!(zone("1A", "1D"), Some(CourtZoneEnum::Z1));
        // 1B with 9A under it, 1C on its right, 1A over it
        assert_eq!(zone("1B", "9A"), Some(CourtZoneEnum::Z19));
        assert_eq!(zone("1B", "1C"), Some(CourtZoneEnum::Z1));
        assert_eq!(zone("1B", "1A"), Some(CourtZoneEnum::Z1));
        // not 1D nor 9D, on its corners
        assert_eq!(pair("1B", "1D"), None);
        assert_eq!(pair("1B", "9D"), None);
        assert_eq!(pair("1A", "6A"), None);
        assert_eq!(pair("1A", "1A"), None);
        // every line between two zones has its pairs
        let lines: std::collections::HashSet<CourtZoneEnum> =
            CourtTarget::all().iter().map(|t| t.court_zone()).collect();
        assert_eq!(lines.len(), CourtZoneEnum::ALL.len());
        assert_eq!(
            pair("8C", "7B").map(|t| t.to_string()),
            Some("8C-7B".to_string())
        );
        assert_eq!(
            pair("7B", "8C").map(|t| t.to_string()),
            Some("8C-7B".to_string())
        );
        assert_eq!(
            pair("3A", "8B").map(|t| t.court_zone()),
            Some(CourtZoneEnum::Z38)
        );
    }

    /// Touching cells: one, then one side by side along with it; touched
    /// again, taken back; one elsewhere starts over.
    #[test]
    fn touching_the_cells() {
        let touch = |current: Option<CourtTarget>, c: &str| CourtTarget::touched(current, cell(c));
        let one = touch(None, "1D");
        assert_eq!(one, Some(CourtTarget::Cell(cell("1D"))));
        let two = touch(one, "6A");
        assert_eq!(two.map(|t| t.to_string()), Some("1D-6A".to_string()));
        assert_eq!(touch(two, "6A"), one);
        assert_eq!(touch(two, "1D"), Some(CourtTarget::Cell(cell("6A"))));
        assert_eq!(touch(two, "8A"), Some(CourtTarget::Cell(cell("8A"))));
        assert_eq!(touch(one, "1D"), None);
        // a corner isn't side by side: it starts over
        assert_eq!(touch(one, "6B"), Some(CourtTarget::Cell(cell("6B"))));
        // the whole zone of a file: a touch tells a cell
        assert_eq!(
            touch(Some(CourtTarget::Zone(1)), "1A"),
            Some(CourtTarget::Cell(cell("1A")))
        );
    }

    /// The cursor on the court: a cell at a time, still at the edges.
    #[test]
    fn the_cursor_moves_on_the_court() {
        use Direction::*;
        let path = |from: &str, moves: &[Direction]| {
            moves
                .iter()
                .fold(cell(from), |c, d| c.moved(*d))
                .to_string()
        };
        assert_eq!(path("8A", &[Up]), "6B");
        assert_eq!(path("8A", &[Left]), "9D");
        assert_eq!(path("1A", &[Up, Left]), "1A");
        assert_eq!(path("4C", &[Down, Right]), "4C");
        assert_eq!(path("1A", &[Right, Right, Down, Down]), "8A");
        assert_eq!(path("2B", &[Right; 5]), "4C");
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
