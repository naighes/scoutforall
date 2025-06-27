mod tests {
    use crate::{
        ops::compute_snapshot,
        shapes::{
            enums::{ErrorTypeEnum, EvalEnum, EventTypeEnum, PhaseEnum, RoleEnum, TeamSideEnum},
            set::SetEntry,
            snapshot::{EventEntry, Snapshot},
        },
    };
    use chrono::Utc;
    use uuid::Uuid;

    fn assert_snapshot<F>(
        snapshot: &Snapshot,
        rotation: u8,
        phase: PhaseEnum,
        score_us: u8,
        score_them: u8,
        serving_team: Option<TeamSideEnum>,
        libero_position: Option<u8>,
        possessions: u32,
        attacks: u32,
        errors: u32,
        unforced_errors: u32,
        counter_attacks: u32,
        opponent_errors: u32,
        post: F,
    ) where
        F: FnOnce(),
    {
        assert_eq!(
            snapshot.current_lineup.get_current_rotation(),
            rotation,
            "wrong rotation"
        );
        assert_eq!(
            snapshot.current_lineup.get_current_phase(),
            phase,
            "wrong phase"
        );
        assert_eq!(snapshot.score_us, score_us, "wrong score_us");
        assert_eq!(snapshot.score_them, score_them, "wrong score_them");
        assert_eq!(
            snapshot.get_serving_team(),
            serving_team,
            "wrong serving_team"
        );
        if let Some(libero) = libero_position {
            assert_eq!(
                snapshot.current_lineup.get(libero),
                snapshot.current_lineup.get_current_libero(),
                "wrong libero"
            );
            assert_eq!(
                snapshot
                    .current_lineup
                    .get_role(&snapshot.current_lineup.get((libero + 3) % 6)),
                RoleEnum::MiddleBlocker,
                "wrong middle-blocker"
            );
        }
        // TODO: test scenario with no libero
        assert_eq!(
            snapshot.stats.possessions.0.values().sum::<u32>(),
            possessions,
            "wrong possessions count"
        );
        assert_eq!(
            snapshot.stats.attack.0.values().sum::<u32>(),
            attacks,
            "wrong attacks count"
        );
        assert_eq!(
            snapshot.stats.errors.0.values().sum::<u32>(),
            errors,
            "wrong errors count"
        );
        assert_eq!(
            snapshot.stats.counter_attack.0.values().sum::<u32>(),
            counter_attacks,
            "wrong counter_attacks count"
        );
        assert_eq!(
            snapshot
                .stats
                .errors
                .query(None, None, None, Some(ErrorTypeEnum::Unforced))
                .map(|(_, v)| *v)
                .sum::<u32>(),
            unforced_errors,
            "wrong unforced_errors count"
        );
        assert_eq!(
            snapshot.stats.opponent_errors.0.values().sum::<u32>(),
            opponent_errors,
            "wrong opponent_errors count"
        );

        post();
    }

    #[test]
    fn snapshot_computing() {
        let setter: Uuid = Uuid::new_v4();
        let oh1: Uuid = Uuid::new_v4();
        let mb2: Uuid = Uuid::new_v4();
        let opposite: Uuid = Uuid::new_v4();
        let oh2: Uuid = Uuid::new_v4();
        let mb1: Uuid = Uuid::new_v4();
        let libero: Uuid = Uuid::new_v4();
        let setter_replacement: Uuid = Uuid::new_v4();
        let positions: [Uuid; 6] = [setter, oh1, mb2, opposite, oh2, mb1];

        let set = SetEntry {
            set_number: 1,
            serving_team: TeamSideEnum::Us,
            initial_positions: positions,
            libero: libero,
            setter,
            events: vec![EventEntry {
                event_type: EventTypeEnum::S,
                player: Some(setter),
                target_player: None,
                eval: Some(EvalEnum::Perfect),
                timestamp: Utc::now(),
            }],
        };
        let (mut snapshot, mut availeble_options) =
            compute_snapshot(&set).expect("expected successful computation");
        assert_eq!(snapshot.score_us, 1);
        assert_eq!(snapshot.score_them, 0);
        assert_eq!(snapshot.get_serving_team(), Some(TeamSideEnum::Us));
        assert_eq!(
            snapshot.current_lineup.get_current_phase(),
            PhaseEnum::Break
        );
        assert_eq!(snapshot.current_lineup.get_current_rotation(), 0);
        assert_eq!(snapshot.current_lineup.get(0), setter);
        assert_eq!(snapshot.current_lineup.get(3), opposite);

        assert_eq!(snapshot.current_lineup.get(5), libero);
        assert_eq!(snapshot.current_lineup.get_mb1(), mb1);
        assert_eq!(snapshot.current_lineup.get_mb2(), mb2);
        assert_eq!(
            &snapshot.last_event.clone().expect("error").event_type,
            &EventTypeEnum::S
        );

        let list: Vec<(EventEntry, Box<dyn Fn(&Snapshot)>)> = vec![
            (
                EventEntry {
                    event_type: EventTypeEnum::S,
                    eval: Some(EvalEnum::Error),
                    player: Some(setter),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        0,
                        PhaseEnum::SideOut,
                        1,
                        1,
                        Some(TeamSideEnum::Them),
                        Some(5),
                        2,
                        0,
                        1,
                        1,
                        0,
                        0,
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::P,
                    eval: Some(EvalEnum::Perfect),
                    player: Some(oh1),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        0,
                        PhaseEnum::SideOut,
                        1,
                        1,
                        None,
                        Some(5),
                        3,
                        0,
                        1,
                        1,
                        0,
                        0,
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::A,
                    eval: Some(EvalEnum::Perfect),
                    player: Some(opposite),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        5,
                        PhaseEnum::Break,
                        2,
                        1,
                        Some(TeamSideEnum::Us),
                        Some(4),
                        3,
                        1,
                        1,
                        1,
                        0,
                        0,
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::S,
                    eval: Some(EvalEnum::Positive),
                    player: Some(oh1),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        5,
                        PhaseEnum::Break,
                        2,
                        1,
                        None,
                        Some(4),
                        4,
                        1,
                        1,
                        1,
                        0,
                        0,
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::OE,
                    eval: None,
                    player: None,
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        5,
                        PhaseEnum::Break,
                        3,
                        1,
                        Some(TeamSideEnum::Us),
                        Some(4),
                        4,
                        1,
                        1,
                        1,
                        0,
                        1,
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::R,
                    eval: None,
                    target_player: Some(setter_replacement),
                    player: Some(setter),
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        5, // rotation
                        PhaseEnum::Break,
                        3,                      // score_us
                        1,                      // score_them
                        Some(TeamSideEnum::Us), // serving_team
                        Some(4),                // libero_position
                        4,                      // possessions
                        1,                      // attacks
                        1,                      // errors
                        1,                      // unforced_errors
                        0,                      // counter_attacks
                        1,                      // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::S,
                    player: Some(oh1),
                    target_player: None,
                    eval: Some(EvalEnum::Negative),
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        5, // rotation
                        PhaseEnum::Break,
                        3,       // score_us
                        1,       // score_them
                        None,    // serving_team
                        Some(4), // libero_position
                        5,       // possessions
                        1,       // attacks
                        1,       // errors
                        1,       // unforced_errors
                        0,       // counter_attacks
                        1,       // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::B,
                    eval: Some(EvalEnum::Negative),
                    player: Some(mb2),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        5, // rotation
                        PhaseEnum::Break,
                        3,       // score_us
                        1,       // score_them
                        None,    // serving_team
                        Some(4), // libero_position
                        5,       // possessions
                        1,       // attacks
                        1,       // errors
                        1,       // unforced_errors
                        0,       // counter_attacks
                        1,       //opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::OS,
                    eval: None,
                    player: None,
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        5, // rotation
                        PhaseEnum::SideOut,
                        3,                        // score_us
                        2,                        // score_them
                        Some(TeamSideEnum::Them), // serving_team
                        Some(4),                  // libero_position
                        5,                        // possessions
                        1,                        // attacks
                        1,                        // errors
                        1,                        // unforced_errors
                        0,                        // counter_attacks
                        1,                        // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::P,
                    eval: Some(EvalEnum::Perfect),
                    player: Some(libero),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        5, // rotation
                        PhaseEnum::SideOut,
                        3,       // score_us
                        2,       // score_them
                        None,    // serving_team
                        Some(4), // libero_position
                        6,       // possessions
                        1,       // attacks
                        1,       // errors
                        1,       // unforced_errors
                        0,       // counter_attacks
                        1,       // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::F,
                    eval: None,
                    player: Some(setter_replacement),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        5, // rotation
                        PhaseEnum::SideOut,
                        3,                        // score_us
                        3,                        // score_them
                        Some(TeamSideEnum::Them), // serving_team
                        Some(4),                  // libero_position
                        6,                        // possessions
                        1,                        // attacks
                        2,                        // errors
                        2,                        // unforced_errors
                        0,                        // counter_attacks
                        1,                        // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::OE,
                    eval: None,
                    player: None,
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        4, // rotation
                        PhaseEnum::Break,
                        4,                      // score_us
                        3,                      // score_them
                        Some(TeamSideEnum::Us), // serving_team
                        None,                   // libero_position
                        6,                      // possessions
                        1,                      // attacks
                        2,                      // errors
                        2,                      // unforced_errors
                        0,                      // counter_attacks
                        2,                      // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::S,
                    eval: Some(EvalEnum::Over),
                    player: Some(mb2),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        4, // rotation
                        PhaseEnum::Break,
                        4,    // score_us
                        3,    // score_them
                        None, // serving_team
                        None, // libero_position
                        7,    // possessions
                        1,    // attacks
                        2,    // errors
                        2,    // unforced_errors
                        0,    // counter_attacks
                        2,    // opponent_errors
                        || {
                            assert_eq!(
                                snapshot
                                    .current_lineup
                                    .find_position(&snapshot.current_lineup.get_mb2()),
                                Some(0)
                            );
                            assert_eq!(
                                snapshot
                                    .current_lineup
                                    .find_position(&snapshot.current_lineup.get_mb1()),
                                Some(3)
                            );
                        },
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::A,
                    eval: Some(EvalEnum::Perfect),
                    player: Some(oh2),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        4, // rotation
                        PhaseEnum::Break,
                        5,                      // score_us
                        3,                      // score_them
                        Some(TeamSideEnum::Us), // serving_team
                        None,                   // libero_position
                        7,                      // possessions
                        2,                      // attacks
                        2,                      // errors
                        2,                      // unforced_errors
                        0,                      // counter_attacks
                        2,                      // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::S,
                    eval: Some(EvalEnum::Error),
                    player: Some(oh2),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        4, // rotation
                        PhaseEnum::SideOut,
                        5,                        // score_us
                        4,                        // score_them
                        Some(TeamSideEnum::Them), // serving_team
                        Some(0),                  // libero_position
                        8,                        // possessions
                        2,                        // attacks
                        3,                        // errors
                        3,                        // unforced_errors
                        0,                        // counter_attacks
                        2,                        // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::P,
                    eval: Some(EvalEnum::Positive),
                    player: Some(libero),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        4, // rotation
                        PhaseEnum::SideOut,
                        5,       // score_us
                        4,       // score_them
                        None,    // serving_team
                        Some(0), // libero_position
                        9,       // possessions
                        2,       // attacks
                        3,       // errors
                        3,       // unforced_errors
                        0,       // counter_attacks
                        2,       // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::A,
                    eval: Some(EvalEnum::Negative),
                    player: Some(oh1),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        4, // rotation
                        PhaseEnum::SideOut,
                        5,       // score_us
                        4,       // score_them
                        None,    // serving_team
                        Some(0), // libero_position
                        9,       // possessions
                        3,       // attacks
                        3,       // errors
                        3,       // unforced_errors
                        0,       // counter_attacks
                        2,       // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::D,
                    eval: Some(EvalEnum::Positive),
                    player: Some(libero),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        4, // rotation
                        PhaseEnum::SideOut,
                        5,       // score_us
                        4,       // score_them
                        None,    // serving_team
                        Some(0), // libero_position
                        10,      // possessions
                        3,       // attacks
                        3,       // errors
                        3,       // unforced_errors
                        0,       // counter_attacks
                        2,       // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::A,
                    eval: Some(EvalEnum::Perfect),
                    player: Some(opposite),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        3, // rotation
                        PhaseEnum::Break,
                        6,                      // score_us
                        4,                      // score_them
                        Some(TeamSideEnum::Us), // serving_team
                        Some(5),                // libero_position
                        10,                     // possessions
                        4,                      // attacks
                        3,                      // errors
                        3,                      // unforced_errors
                        1,                      // counter_attacks
                        2,                      // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::S,
                    eval: Some(EvalEnum::Positive),
                    player: Some(opposite),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        3, // rotation
                        PhaseEnum::Break,
                        6,       // score_us
                        4,       // score_them
                        None,    // serving_team
                        Some(5), // libero_position
                        11,      // possessions
                        4,       // attacks
                        3,       // errors
                        3,       // unforced_errors
                        1,       // counter_attacks
                        2,       // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::B,
                    eval: Some(EvalEnum::Error),
                    player: Some(mb1),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        3, // rotation
                        PhaseEnum::SideOut,
                        6,                        // score_us
                        5,                        // score_them
                        Some(TeamSideEnum::Them), // serving_team
                        Some(5),                  // libero_position
                        11,                       // possessions
                        4,                        // attacks
                        4,                        // errors
                        3,                        // unforced_errors
                        1,                        // counter_attacks
                        2,                        // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::P,
                    eval: Some(EvalEnum::Perfect),
                    player: Some(libero),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        3, // rotation
                        PhaseEnum::SideOut,
                        6,       // score_us
                        5,       // score_them
                        None,    // serving_team
                        Some(5), // libero_position
                        12,      // possessions
                        4,       // attacks
                        4,       // errors
                        3,       // unforced_errors
                        1,       // counter_attacks
                        2,       // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::A,
                    eval: Some(EvalEnum::Over),
                    player: Some(opposite),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        3, // rotation
                        PhaseEnum::SideOut,
                        6,                        // score_us
                        6,                        // score_them
                        Some(TeamSideEnum::Them), // serving_team
                        Some(5),                  // libero_position
                        12,                       // possessions
                        5,                        // attacks
                        5,                        // errors
                        3,                        // unforced_errors
                        1,                        // counter_attacks
                        2,                        // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::OE,
                    eval: None,
                    player: None,
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        2, // rotation
                        PhaseEnum::Break,
                        7,                      // score_us
                        6,                      // score_them
                        Some(TeamSideEnum::Us), // serving_team
                        Some(4),                // libero_position
                        12,                     // possessions
                        5,                      // attacks
                        5,                      // errors
                        3,                      // unforced_errors
                        1,                      // counter_attacks
                        3,                      // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::S,
                    eval: Some(EvalEnum::Negative),
                    player: Some(oh2),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        2, // rotation
                        PhaseEnum::Break,
                        7,       // score_us
                        6,       // score_them
                        None,    // serving_team
                        Some(4), // libero_position
                        13,      // possessions
                        5,       // attacks
                        5,       // errors
                        3,       // unforced_errors
                        1,       // counter_attacks
                        3,       // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::D,
                    eval: Some(EvalEnum::Positive),
                    player: Some(libero),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        2, // rotation
                        PhaseEnum::Break,
                        7,       // score_us
                        6,       // score_them
                        None,    // serving_team
                        Some(4), // libero_position
                        14,      // possessions
                        5,       // attacks
                        5,       // errors
                        3,       // unforced_errors
                        1,       // counter_attacks
                        3,       // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::A,
                    eval: Some(EvalEnum::Error),
                    player: Some(oh1),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        2, // rotation
                        PhaseEnum::SideOut,
                        7,                        // score_us
                        7,                        // score_them
                        Some(TeamSideEnum::Them), // serving_team
                        Some(4),                  // libero_position
                        14,                       // possessions
                        6,                        // attacks
                        6,                        // errors
                        4,                        // unforced_errors
                        2,                        // counter_attacks
                        3,                        // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::OE,
                    eval: None,
                    player: None,
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        1, // rotation
                        PhaseEnum::Break,
                        8,                      // score_us
                        7,                      // score_them
                        Some(TeamSideEnum::Us), // serving_team
                        None,                   // libero_position
                        14,                     // possessions
                        6,                      // attacks
                        6,                      // errors
                        4,                      // unforced_errors
                        2,                      // counter_attacks
                        4,                      // opponent_errors
                        || {},
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::S,
                    eval: Some(EvalEnum::Error),
                    player: Some(mb1),
                    target_player: None,
                    timestamp: Utc::now(),
                },
                Box::new(|snapshot: &Snapshot| {
                    assert_snapshot(
                        snapshot,
                        1, // rotation
                        PhaseEnum::SideOut,
                        8,                        // score_us
                        8,                        // score_them
                        Some(TeamSideEnum::Them), // serving_team
                        Some(0),                  // libero_position
                        15,                       // possessions
                        6,                        // attacks
                        7,                        // errors
                        5,                        // unforced_errors
                        2,                        // counter_attacks
                        4,                        // opponent_errors
                        || {},
                    );
                }),
            ),
        ];

        for (event, assert) in list {
            availeble_options = snapshot
                .compute_event(&event, availeble_options)
                .expect("expected a successful computation");
            assert(&snapshot);
        }

        // TODO: final assertion for snapshot stats
    }
}
