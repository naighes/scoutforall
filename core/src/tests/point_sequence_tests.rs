//! The sequence of the points of a set: the team scoring each one, with the
//! score it reaches, in the snapshot and in the report.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::reporting::model::{Point, build_report};
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, TeamSideEnum},
        r#match::MatchEntry,
        set::SetEntry,
        snapshot::EventEntry,
        team::TeamEntry,
    };
    use crate::tests::EN;
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
            flags: Default::default(),
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

    /// The replay of the trend has the same points, each set from 0-0;
    /// the errors of a figure rally by rally, all or the unforced ones:
    /// the serve error is both, of the serve and of the break point (where
    /// it was made), none of the side out.
    #[test]
    fn the_replay_has_the_points_and_the_errors_of_each_rally() {
        use crate::trend::{Replay, Series};
        let first = set();
        let second = SetEntry {
            set_number: 2,
            ..set()
        };
        let replay =
            Replay::new(&[&first, &second], DEFAULT_MAX_SUBSTITUTIONS).expect("the sets replay");
        let one = [(Us, 1), (Them, 1), (Them, 2), (Us, 2)];
        assert_eq!(replay.point_sequence(), [one, one].concat());
        assert_eq!(replay.set(2).point_sequence(), one);
        for unforced in [false, true] {
            assert_eq!(
                replay.errors(Series::Serve, unforced),
                [0, 1, 0, 0, 0, 1, 0, 0]
            );
            assert_eq!(
                replay.set(2).errors(Series::BreakPoint, unforced),
                [0, 1, 0, 0]
            );
            assert_eq!(replay.errors(Series::SideOut, unforced), [0; 8]);
            assert_eq!(replay.errors(Series::Reception, unforced), [0; 8]);
        }
    }

    /// An ace received: an error of the reception and of the side out,
    /// forced, so none of them unforced.
    #[test]
    fn an_ace_received_is_an_error_not_unforced() {
        use crate::trend::{Replay, Series};
        let mut received = set();
        received.serving_team = Them;
        let player = received.initial_positions[1];
        received.events = vec![ev(P, Some(player), Some(EvalEnum::Error))];
        let replay = Replay::new(&[&received], DEFAULT_MAX_SUBSTITUTIONS).expect("the set replays");
        assert_eq!(replay.point_sequence(), [(Them, 1)]);
        assert_eq!(replay.errors(Series::Reception, false), [1]);
        assert_eq!(replay.errors(Series::SideOut, false), [1]);
        assert_eq!(replay.errors(Series::Reception, true), [0]);
        assert_eq!(replay.errors(Series::SideOut, true), [0]);
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

    /// The home team first (them, playing away), each point in the row of
    /// the team scoring it.
    #[test]
    fn report_shows_the_point_sequence() {
        let m = MatchEntry {
            source: Default::default(),
            video_clock: false,
            roster: vec![],
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
            notes: String::new(),
        };
        let sequence = build_report(&m, EN)
            .expect("expected the report")
            .point_sequence;
        assert_eq!(sequence.teams, ["them", "us"]);
        assert_eq!(sequence.sets.len(), 1);
        assert_eq!(sequence.sets[0].label, "set 1");
        let point = |team, us, score| Point { team, us, score };
        assert_eq!(
            sequence.sets[0].points,
            [
                point(1, true, 1),
                point(0, false, 1),
                point(0, false, 2),
                point(1, true, 2),
            ]
        );
    }
}
