//! The sequence of the points of a set: the team scoring each one, with the
//! score it reaches, in the snapshot and in the report.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::localization::current_labels;
    use crate::reporting::pdf::{render_match_pdf, render_match_typst};
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, TeamSideEnum},
        r#match::MatchEntry,
        set::SetEntry,
        snapshot::EventEntry,
        team::TeamEntry,
    };
    use EventTypeEnum::*;
    use TeamSideEnum::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn ev(event_type: EventTypeEnum, player: Option<Uuid>, eval: Option<EvalEnum>) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type,
            player,
            eval,
            target_player: None,
            technique: None,
        }
    }

    /// We serve with the first player: our ace, our serve error, their
    /// point, their error.
    fn set() -> SetEntry {
        let players: [Uuid; 6] = std::array::from_fn(|_| Uuid::new_v4());
        SetEntry {
            set_number: 1,
            serving_team: Us,
            initial_positions: players,
            libero: Uuid::new_v4(),
            fallback_libero: None,
            setter: players[0],
            lineup_rules: Some(LineupRulesEnum::CURRENT),
            events: vec![
                ev(S, Some(players[0]), Some(EvalEnum::Perfect)),
                ev(S, Some(players[0]), Some(EvalEnum::Error)),
                ev(OS, None, None),
                ev(OE, None, None),
            ],
        }
    }

    #[test]
    fn the_points_in_order_with_the_score_reached() {
        let snapshot = set()
            .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
            .expect("expected the set to replay");
        assert_eq!(snapshot.points, [Us, Them, Them, Us]);
        assert_eq!(
            snapshot.point_sequence(),
            [(Us, 1), (Them, 1), (Them, 2), (Us, 2)]
        );
    }

    #[test]
    fn no_points_before_the_first_rally() {
        let mut set = set();
        set.events.clear();
        let snapshot = set
            .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
            .expect("expected the set to replay");
        assert!(snapshot.point_sequence().is_empty());
    }

    /// A row per team, the home team on top (them, playing away), a column
    /// per point with the score in the row of the team scoring it.
    #[test]
    fn report_shows_the_point_sequence() {
        let m = MatchEntry {
            opponent: "them".to_string(),
            date: Utc::now().fixed_offset(),
            id: "match".to_string(),
            team: TeamEntry {
                name: "us".to_string(),
                ..TeamEntry::default()
            },
            home: false,
            max_substitutions: DEFAULT_MAX_SUBSTITUTIONS,
            sets: vec![set()],
        };
        let typst = render_match_typst(&m).expect("expected the report source");
        let section = &typst[typst
            .find(&format!("\"{}\"", current_labels().point_sequence))
            .expect("expected the point sequence")..];
        let texts: Vec<&str> = section
            .split("text(\"")
            .skip(1)
            .map(|t| &t[..t.find('"').unwrap()])
            .collect();
        assert_eq!(texts[0], "set 1");
        let rows: Vec<&[&str]> = texts[1..].chunks(41).collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][..5], ["them", "", "1", "2", ""]);
        assert_eq!(rows[1][..5], ["us", "1", "", "", "2"]);
        assert!(rows.iter().all(|r| r[5..].iter().all(|t| t.is_empty())));
        render_match_pdf(&m).expect("expected the report");
    }
}
