mod tests {
    use crate::{
        errors::{AppError, SnapshotError},
        shapes::{
            enums::{
                ErrorTypeEnum, EvalEnum, EventTypeEnum, PhaseEnum, RoleEnum, TeamSideEnum, ZoneEnum,
            },
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
            snapshot
                .current_lineup
                .get_current_rotation()
                .expect("expected a rotation"),
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
                snapshot
                    .current_lineup
                    .get(libero as usize)
                    .expect("expected a player"),
                snapshot.current_lineup.get_current_libero(),
                "wrong libero"
            );
            assert_eq!(
                snapshot
                    .current_lineup
                    .get_role(
                        &snapshot
                            .current_lineup
                            .get(((libero + 3) % 6) as usize)
                            .expect("expected a player")
                    )
                    .expect("expected a role"),
                RoleEnum::MiddleBlocker,
                "wrong middle-blocker"
            );
        }
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
    fn snapshot_1_computing() {
        let setter: Uuid =
            Uuid::parse_str("00cece40-cdc0-4c54-a624-52556a3f9131").expect("should not throw");
        let oh1: Uuid =
            Uuid::parse_str("015f2daa-693e-4820-9708-9027b65f29d7").expect("should not throw");
        let mb2: Uuid =
            Uuid::parse_str("02eb5e86-cf5e-4a6d-ac53-29beff182dd8").expect("should not throw");
        let opposite: Uuid =
            Uuid::parse_str("03ee29ec-f62c-406a-9670-e2791d84f5b3").expect("should not throw");
        let oh2: Uuid =
            Uuid::parse_str("0462a0e7-d7d7-4fa7-bd7d-8b5144506d50").expect("should not throw");
        let mb1: Uuid =
            Uuid::parse_str("05887877-d823-4270-a8c8-42f901541d5c").expect("should not throw");
        let libero: Uuid =
            Uuid::parse_str("0686d9e5-ed37-4758-8b11-52df14406008").expect("should not throw");
        let setter_replacement: Uuid =
            Uuid::parse_str("079a79c8-ab17-401e-bc34-9ea68dd578b4").expect("should not throw");
        let some_other_replacement: Uuid =
            Uuid::parse_str("e1f518fd-a730-4d5e-bb78-e97daefdf929").expect("should not throw");
        let some_other_player: Uuid =
            Uuid::parse_str("9dac1242-72a5-48f6-bb61-680d2be65dcb").expect("should not throw");
        let fallback_libero: Uuid =
            Uuid::parse_str("ab4bded1-c5a4-416d-82d0-66324e2dc2cd").expect("should not throw");
        let positions: [Uuid; 6] = [setter, oh1, mb2, opposite, oh2, mb1];

        let set = SetEntry {
            set_number: 1,
            serving_team: TeamSideEnum::Us,
            initial_positions: positions,
            libero,
            fallback_libero: Some(fallback_libero),
            setter,
            events: vec![EventEntry {
                event_type: EventTypeEnum::S,
                player: Some(setter),
                target_player: None,
                eval: Some(EvalEnum::Perfect),
                timestamp: Utc::now(),
            }],
        };
        let (mut snapshot, mut availeble_options) = set
            .compute_snapshot()
            .expect("expected successful computation");
        assert_eq!(snapshot.score_us, 1);
        assert_eq!(snapshot.score_them, 0);
        assert_eq!(snapshot.get_serving_team(), Some(TeamSideEnum::Us));
        assert_eq!(
            snapshot.current_lineup.get_current_phase(),
            PhaseEnum::Break
        );
        assert_eq!(
            snapshot
                .current_lineup
                .get_current_rotation()
                .expect("expected a rotation"),
            0
        );
        assert_eq!(
            snapshot.current_lineup.get(0).expect("expected a player"),
            setter
        );
        assert_eq!(
            snapshot.current_lineup.get(3).expect("expected a player"),
            opposite
        );

        assert_eq!(
            snapshot.current_lineup.get(5).expect("expected a player"),
            libero
        );
        assert_eq!(
            snapshot
                .current_lineup
                .get_mb1()
                .expect("expected a player"),
            mb1
        );
        assert_eq!(
            snapshot
                .current_lineup
                .get_mb2()
                .expect("expected a player"),
            mb2
        );
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
                        0, // rotation
                        PhaseEnum::SideOut,
                        1,       // score_us
                        1,       // score_them
                        None,    // serving_team
                        Some(5), // libero_position
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
                        || {
                            assert_eq!(
                                1,
                                snapshot
                                    .stats
                                    .distribution
                                    .query(
                                        Some(PhaseEnum::SideOut),
                                        Some(0),
                                        Some(ZoneEnum::Four),
                                        Some(EvalEnum::Perfect),
                                        Some(EvalEnum::Perfect)
                                    )
                                    .map(|(_, v)| *v)
                                    .sum::<u32>()
                            );
                        },
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
                        || {
                            assert_eq!(
                                snapshot.current_lineup.get_setter(),
                                Some(setter_replacement)
                            );
                        },
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
                                snapshot.current_lineup.find_position(
                                    &snapshot
                                        .current_lineup
                                        .get_mb2()
                                        .expect("expected a player")
                                ),
                                Some(0)
                            );
                            assert_eq!(
                                snapshot.current_lineup.find_position(
                                    &snapshot
                                        .current_lineup
                                        .get_mb1()
                                        .expect("expected a player")
                                ),
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
                        || {
                            assert_eq!(
                                1,
                                snapshot
                                    .stats
                                    .distribution
                                    .query(
                                        Some(PhaseEnum::Break),
                                        Some(4),
                                        Some(ZoneEnum::Four),
                                        Some(EvalEnum::Over),
                                        Some(EvalEnum::Perfect)
                                    )
                                    .map(|(_, v)| *v)
                                    .sum::<u32>()
                            );
                        },
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
                        || {
                            assert_eq!(
                                1,
                                snapshot
                                    .stats
                                    .distribution
                                    .query(
                                        Some(PhaseEnum::SideOut),
                                        Some(4),
                                        Some(ZoneEnum::Eight),
                                        Some(EvalEnum::Positive),
                                        Some(EvalEnum::Negative)
                                    )
                                    .map(|(_, v)| *v)
                                    .sum::<u32>()
                            );
                        },
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
            (
                EventEntry {
                    event_type: EventTypeEnum::R,
                    eval: None,
                    target_player: Some(setter),
                    player: Some(setter_replacement),
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
                        || {
                            assert_eq!(snapshot.current_lineup.get_setter(), Some(setter));
                        },
                    );
                }),
            ),
            (
                EventEntry {
                    event_type: EventTypeEnum::CL,
                    eval: None,
                    target_player: None,
                    player: None,
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
                        || {
                            assert_eq!(
                                snapshot.current_lineup.get_current_libero(),
                                fallback_libero
                            );
                        },
                    );
                }),
            ),
        ];

        for (event, assert) in list {
            availeble_options = snapshot
                .add_event(&event, availeble_options)
                .expect("expected a successful computation");
            assert(&snapshot);
        }

        let setter_already_replaced = snapshot.add_event(
            &EventEntry {
                event_type: EventTypeEnum::R,
                eval: None,
                player: Some(setter),                    // out
                target_player: Some(setter_replacement), // in
                timestamp: Utc::now(),
            },
            vec![EventTypeEnum::R],
        );
        assert!(match setter_already_replaced {
            Err(AppError::Snapshot(SnapshotError::LineupError(error))) => {
                error == format!("player {:?} was already replaced", setter)
            }
            _ => false,
        });

        let setter_replacement_already_used = snapshot.add_event(
            &EventEntry {
                event_type: EventTypeEnum::R,
                eval: None,
                player: Some(oh1),                       // out
                target_player: Some(setter_replacement), // in
                timestamp: Utc::now(),
            },
            vec![EventTypeEnum::R],
        );
        assert!(match setter_replacement_already_used {
            Err(AppError::Snapshot(SnapshotError::LineupError(error))) => {
                error == format!("player {:?} was already a replacement", setter_replacement)
            }
            _ => false,
        });

        let libero_cannot_be_replaced = snapshot.add_event(
            &EventEntry {
                event_type: EventTypeEnum::R,
                eval: None,
                player: Some(fallback_libero),               // out
                target_player: Some(some_other_replacement), // in
                timestamp: Utc::now(),
            },
            vec![EventTypeEnum::R],
        );
        assert!(match libero_cannot_be_replaced {
            Err(AppError::Snapshot(SnapshotError::LineupError(error))) => {
                error == "cannot replace the libero player"
            }
            _ => false,
        });

        snapshot
            .add_event(
                &EventEntry {
                    event_type: EventTypeEnum::R,
                    eval: None,
                    player: Some(oh1),                           // out
                    target_player: Some(some_other_replacement), // in
                    timestamp: Utc::now(),
                },
                vec![EventTypeEnum::R],
            )
            .expect("should not throw");

        let unclosed_change_error = snapshot.add_event(
            &EventEntry {
                event_type: EventTypeEnum::R,
                eval: None,
                player: Some(some_other_replacement),   // out
                target_player: Some(some_other_player), // in
                timestamp: Utc::now(),
            },
            vec![EventTypeEnum::R],
        );
        assert!(match unclosed_change_error {
            Err(AppError::Snapshot(SnapshotError::LineupError(error))) => {
                error
                    == format!(
                        "player {:?} can be only replaced by player {:?}",
                        some_other_replacement, oh1,
                    )
            }
            _ => false,
        });

        snapshot
            .add_event(
                &EventEntry {
                    event_type: EventTypeEnum::R,
                    eval: None,
                    player: Some(some_other_replacement), // out
                    target_player: Some(oh1),             // in
                    timestamp: Utc::now(),
                },
                vec![EventTypeEnum::R],
            )
            .expect("should not throw");

        // TODO: final assertion for snapshot stats
    }
}
