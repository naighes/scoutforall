// use scoutforall::r#match::{
//     compute_snapshot, Eval, EventType, MatchEvent, Phase, SetInfo, TeamSide,
// };
// use std::collections::HashSet;
// use uuid::Uuid;

// fn sample_event(event_type: EventType, eval: Option<Eval>, player: Option<Uuid>) -> MatchEvent {
//     MatchEvent {
//         timestamp: "2025-06-20T10:00:00Z".parse().expect("invalid datetime"),
//         rotation: 0,
//         event_type,
//         eval,
//         player,
//     }
// }

// #[test]
// fn two_aces_in_a_row() {
//     let mut set_info: SetInfo = SetInfo {
//         set_number: 1,
//         // serving
//         serving_team: Some(TeamSide::Us),
//         positions: [
//             Uuid::new_v4(), // S in 1
//             Uuid::new_v4(), // OH1 in 2
//             Uuid::new_v4(), // MB2 in 3
//             Uuid::new_v4(), // OP in 4
//             Uuid::new_v4(), // OH2 in 5
//             Uuid::new_v4(), // MB1 in 6 (Libero)
//         ],
//         libero: Uuid::new_v4(),
//         setter: Uuid::new_v4(),
//     };
//     // Setter in 1
//     set_info.setter = set_info.positions[0];

//     let events = vec![
//         sample_event(
//             EventType::S,
//             Some(Eval::Perfect),
//             Some(set_info.positions[0]),
//         ),
//         sample_event(
//             EventType::S,
//             Some(Eval::Perfect),
//             Some(set_info.positions[0]),
//         ),
//     ];

//     let (snapshot, options) = compute_snapshot(&events, &set_info);
//     assert_eq!(snapshot.phase, Phase::Break);
//     assert_eq!(snapshot.score_us, 2);
//     assert_eq!(snapshot.score_them, 0);
//     assert_eq!(snapshot.rotation, 0);
//     assert_eq!(options, vec![EventType::F]);
//     let expected_options: HashSet<_> = [EventType::F].into_iter().collect();
//     let actual_options: HashSet<_> = options.into_iter().collect();
//     assert_eq!(expected_options, actual_options);
// }

// #[test]
// fn negative_serve() {
//     let mut set_info: SetInfo = SetInfo {
//         set_number: 1,
//         // serving
//         serving_team: Some(TeamSide::Us),
//         positions: [
//             Uuid::new_v4(), // S in 1
//             Uuid::new_v4(), // OH1 in 2
//             Uuid::new_v4(), // MB2 in 3
//             Uuid::new_v4(), // OP in 4
//             Uuid::new_v4(), // OH2 in 5
//             Uuid::new_v4(), // MB1 in 6 (Libero)
//         ],
//         libero: Uuid::new_v4(),
//         setter: Uuid::new_v4(),
//     };
//     // Setter in 1
//     set_info.setter = set_info.positions[0];

//     let events = vec![
//         sample_event(
//             EventType::S,
//             Some(Eval::Perfect),
//             Some(set_info.positions[0]),
//         ),
//         sample_event(
//             EventType::S,
//             Some(Eval::Perfect),
//             Some(set_info.positions[0]),
//         ),
//         sample_event(
//             EventType::S,
//             Some(Eval::Negative),
//             Some(set_info.positions[0]),
//         ),
//     ];

//     let (snapshot, options) = compute_snapshot(&events, &set_info);
//     assert_eq!(snapshot.phase, Phase::Break);
//     assert_eq!(snapshot.score_us, 2);
//     assert_eq!(snapshot.score_them, 0);
//     assert_eq!(snapshot.rotation, 0);
//     let expected_options: HashSet<_> = [EventType::B, EventType::D, EventType::OS, EventType::OE]
//         .into_iter()
//         .collect();
//     let actual_options: HashSet<_> = options.into_iter().collect();
//     assert_eq!(expected_options, actual_options);
// }

// #[test]
// fn opponent_score_from_break() {
//     let mut set_info: SetInfo = SetInfo {
//         set_number: 1,
//         // serving
//         serving_team: Some(TeamSide::Us),
//         positions: [
//             Uuid::new_v4(), // S in 1
//             Uuid::new_v4(), // OH1 in 2
//             Uuid::new_v4(), // MB2 in 3
//             Uuid::new_v4(), // OP in 4
//             Uuid::new_v4(), // OH2 in 5
//             Uuid::new_v4(), // MB1 in 6 (Libero)
//         ],
//         libero: Uuid::new_v4(),
//         setter: Uuid::new_v4(),
//     };
//     // Setter in 1
//     set_info.setter = set_info.positions[0];

//     let events = vec![
//         sample_event(
//             EventType::S,
//             Some(Eval::Perfect),
//             Some(set_info.positions[0]),
//         ),
//         sample_event(
//             EventType::S,
//             Some(Eval::Perfect),
//             Some(set_info.positions[0]),
//         ),
//         sample_event(
//             EventType::S,
//             Some(Eval::Negative),
//             Some(set_info.positions[0]),
//         ),
//         sample_event(EventType::OS, None, None),
//     ];

//     let (snapshot, options) = compute_snapshot(&events, &set_info);
//     assert_eq!(snapshot.phase, Phase::SideOut);
//     assert_eq!(snapshot.score_us, 2);
//     assert_eq!(snapshot.score_them, 1);
//     assert_eq!(snapshot.rotation, 0);
//     let expected_options: HashSet<_> = [EventType::P, EventType::F, EventType::OS, EventType::OE]
//         .into_iter()
//         .collect();
//     let actual_options: HashSet<_> = options.into_iter().collect();
//     assert_eq!(expected_options, actual_options);
// }

// #[test]
// fn opponent_error_from_break() {
//     let mut set_info: SetInfo = SetInfo {
//         set_number: 1,
//         // serving
//         serving_team: Some(TeamSide::Us),
//         positions: [
//             Uuid::new_v4(), // S in 1
//             Uuid::new_v4(), // OH1 in 2
//             Uuid::new_v4(), // MB2 in 3
//             Uuid::new_v4(), // OP in 4
//             Uuid::new_v4(), // OH2 in 5
//             Uuid::new_v4(), // MB1 in 6 (Libero)
//         ],
//         libero: Uuid::new_v4(),
//         setter: Uuid::new_v4(),
//     };
//     // Setter in 1
//     set_info.setter = set_info.positions[0];

//     let events = vec![
//         sample_event(
//             EventType::S,
//             Some(Eval::Perfect),
//             Some(set_info.positions[0]),
//         ),
//         sample_event(
//             EventType::S,
//             Some(Eval::Perfect),
//             Some(set_info.positions[0]),
//         ),
//         sample_event(
//             EventType::S,
//             Some(Eval::Negative),
//             Some(set_info.positions[0]),
//         ),
//         sample_event(EventType::OE, None, None),
//     ];

//     let (snapshot, options) = compute_snapshot(&events, &set_info);
//     assert_eq!(snapshot.phase, Phase::Break);
//     assert_eq!(snapshot.score_us, 3);
//     assert_eq!(snapshot.score_them, 0);
//     assert_eq!(snapshot.rotation, 0);
//     let expected_options: HashSet<_> = [EventType::F].into_iter().collect();
//     let actual_options: HashSet<_> = options.into_iter().collect();
//     assert_eq!(expected_options, actual_options);
// }

// #[test]
// fn opponent_serve_error() {
//     let mut set_info: SetInfo = SetInfo {
//         set_number: 1,
//         // serving
//         serving_team: Some(TeamSide::Them),
//         positions: [
//             Uuid::new_v4(), // S in 1
//             Uuid::new_v4(), // OH1 in 2
//             Uuid::new_v4(), // MB2 in 3
//             Uuid::new_v4(), // OP in 4
//             Uuid::new_v4(), // OH2 in 5
//             Uuid::new_v4(), // MB1 in 6 (Libero)
//         ],
//         libero: Uuid::new_v4(),
//         setter: Uuid::new_v4(),
//     };
//     // Setter in 1
//     set_info.setter = set_info.positions[0];

//     let events = vec![sample_event(EventType::OE, None, None)];

//     let (snapshot, options) = compute_snapshot(&events, &set_info);
//     assert_eq!(snapshot.score_us, 1);
//     assert_eq!(snapshot.score_them, 0);
//     assert_eq!(snapshot.phase, Phase::Break);
//     assert_eq!(snapshot.rotation, 5);
//     let expected_options: HashSet<_> = [EventType::F].into_iter().collect();
//     let actual_options: HashSet<_> = options.into_iter().collect();
//     assert_eq!(expected_options, actual_options);
// }

// #[test]
// fn long_action() {
//     let mut set_info: SetInfo = SetInfo {
//         set_number: 1,
//         // serving
//         serving_team: Some(TeamSide::Them),
//         positions: [
//             Uuid::new_v4(), // S in 1
//             Uuid::new_v4(), // OH1 in 2
//             Uuid::new_v4(), // MB2 in 3
//             Uuid::new_v4(), // OP in 4
//             Uuid::new_v4(), // OH2 in 5
//             Uuid::new_v4(), // MB1 in 6 (Libero)
//         ],
//         libero: Uuid::new_v4(),
//         setter: Uuid::new_v4(),
//     };
//     // Setter in 1
//     set_info.setter = set_info.positions[0];

//     let events = vec![
//         // S1 sideout
//         sample_event(EventType::OS, None, None),
//         sample_event(
//             EventType::P,
//             Some(Eval::Perfect),
//             Some(set_info.positions[1]),
//         ), // OH1 pass
//         sample_event(EventType::A, Some(Eval::Error), Some(set_info.positions[2])), // MB2 attack error
//         sample_event(
//             EventType::P,
//             Some(Eval::Positive),
//             Some(set_info.positions[5]),
//         ), // MB1 pass
//         sample_event(
//             EventType::A,
//             Some(Eval::Perfect),
//             Some(set_info.positions[1]),
//         ), // OH1 attack score
//         // S6 break
//         sample_event(
//             EventType::S,
//             Some(Eval::Perfect),
//             Some(set_info.positions[2]),
//         ), // OH1 serve score
//         sample_event(
//             EventType::S,
//             Some(Eval::Perfect),
//             Some(set_info.positions[2]),
//         ), // OH1 serve score
//         sample_event(EventType::S, Some(Eval::Error), Some(set_info.positions[2])), // OH1 serve error
//         // S6 sideout
//         sample_event(
//             EventType::P,
//             Some(Eval::Negative),
//             Some(set_info.positions[1]),
//         ), // OH1 pass
//         sample_event(
//             EventType::A,
//             Some(Eval::Perfect),
//             Some(set_info.positions[3]),
//         ), // OP attack score
//         // S5 break
//         sample_event(
//             EventType::S,
//             Some(Eval::Negative),
//             Some(set_info.positions[2]),
//         ), // MB2 serve
//         sample_event(
//             EventType::B,
//             Some(Eval::Perfect),
//             Some(set_info.positions[5]),
//         ), // MB2 block score
//         sample_event(
//             EventType::S,
//             Some(Eval::Negative),
//             Some(set_info.positions[2]),
//         ), // MB2 serve
//         sample_event(EventType::OS, None, None),
//     ];

//     let (snapshot, _options) = compute_snapshot(&events, &set_info);
//     assert_eq!(snapshot.score_us, 5);
//     assert_eq!(snapshot.score_them, 4);
//     assert_eq!(snapshot.phase, Phase::SideOut);
//     assert_eq!(snapshot.rotation, 4);
// }

// get_serving_player

use scoutforall::ops::{
    get_mb1, get_mb2, get_oh1, get_oh2, get_opposite, get_serving_player, get_setter, SetEntry,
    TeamSideEnum,
};
use uuid::Uuid;

#[test]
fn initial_p2_target_p4() {
    let s: SetEntry = SetEntry {
        set_number: 1,
        serving_team: TeamSideEnum::Them,
        positions: [
            Uuid::parse_str("bc19a25b-1ab0-4ade-9298-7c67cd61cda1").unwrap(),
            Uuid::parse_str("7485a1c4-0032-4db7-993a-1d5667f0ebce").unwrap(), // setter
            Uuid::parse_str("a026a9e5-4bdc-4559-a21a-b62fe3c8027a").unwrap(),
            Uuid::parse_str("8c91b74b-06a9-416c-8f86-2f75aa37e343").unwrap(),
            Uuid::parse_str("45598587-0df7-4627-b63e-e337272182e2").unwrap(),
            Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(),
        ],
        libero: Uuid::parse_str("2b604e66-3006-4b11-943e-d6d580b9bad9").unwrap(),
        setter: Uuid::parse_str("7485a1c4-0032-4db7-993a-1d5667f0ebce").unwrap(), // P2
        events: Vec::new(),
    };
    let result = get_serving_player(&s, 3); // P4
    assert_eq!(result.to_string(), "45598587-0df7-4627-b63e-e337272182e2");
}

#[test]
fn initial_p2_target_p6() {
    let s: SetEntry = SetEntry {
        set_number: 1,
        serving_team: TeamSideEnum::Them,
        positions: [
            Uuid::parse_str("bc19a25b-1ab0-4ade-9298-7c67cd61cda1").unwrap(),
            Uuid::parse_str("7485a1c4-0032-4db7-993a-1d5667f0ebce").unwrap(),
            Uuid::parse_str("a026a9e5-4bdc-4559-a21a-b62fe3c8027a").unwrap(),
            Uuid::parse_str("8c91b74b-06a9-416c-8f86-2f75aa37e343").unwrap(),
            Uuid::parse_str("45598587-0df7-4627-b63e-e337272182e2").unwrap(),
            Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // setter
        ],
        libero: Uuid::parse_str("2b604e66-3006-4b11-943e-d6d580b9bad9").unwrap(),
        setter: Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // P6
        events: Vec::new(),
    };
    let result = get_serving_player(&s, 5); // P6
    assert_eq!(result.to_string(), "bc19a25b-1ab0-4ade-9298-7c67cd61cda1");
}

#[test]
fn initial_p4_target_p6() {
    let s: SetEntry = SetEntry {
        set_number: 1,
        serving_team: TeamSideEnum::Them,
        positions: [
            Uuid::parse_str("bc19a25b-1ab0-4ade-9298-7c67cd61cda1").unwrap(),
            Uuid::parse_str("7485a1c4-0032-4db7-993a-1d5667f0ebce").unwrap(),
            Uuid::parse_str("a026a9e5-4bdc-4559-a21a-b62fe3c8027a").unwrap(),
            Uuid::parse_str("8c91b74b-06a9-416c-8f86-2f75aa37e343").unwrap(),
            Uuid::parse_str("45598587-0df7-4627-b63e-e337272182e2").unwrap(),
            Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // setter
        ],
        libero: Uuid::parse_str("2b604e66-3006-4b11-943e-d6d580b9bad9").unwrap(),
        setter: Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // P6
        events: Vec::new(),
    };
    let result = get_serving_player(&s, 5); // P6
    assert_eq!(result.to_string(), "bc19a25b-1ab0-4ade-9298-7c67cd61cda1");
}

#[test]
fn initial_p5_target_p4() {
    let s: SetEntry = SetEntry {
        set_number: 1,
        serving_team: TeamSideEnum::Them,
        positions: [
            Uuid::parse_str("bc19a25b-1ab0-4ade-9298-7c67cd61cda1").unwrap(),
            Uuid::parse_str("7485a1c4-0032-4db7-993a-1d5667f0ebce").unwrap(),
            Uuid::parse_str("a026a9e5-4bdc-4559-a21a-b62fe3c8027a").unwrap(),
            Uuid::parse_str("8c91b74b-06a9-416c-8f86-2f75aa37e343").unwrap(),
            Uuid::parse_str("45598587-0df7-4627-b63e-e337272182e2").unwrap(), // setter
            Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(),
        ],
        libero: Uuid::parse_str("2b604e66-3006-4b11-943e-d6d580b9bad9").unwrap(),
        setter: Uuid::parse_str("45598587-0df7-4627-b63e-e337272182e2").unwrap(), // P5
        events: Vec::new(),
    };
    let result = get_serving_player(&s, 3); // P4
    assert_eq!(result.to_string(), "7485a1c4-0032-4db7-993a-1d5667f0ebce");
}

#[test]
fn initial_p1_target_p5() {
    let s: SetEntry = SetEntry {
        set_number: 1,
        serving_team: TeamSideEnum::Them,
        positions: [
            Uuid::parse_str("bc19a25b-1ab0-4ade-9298-7c67cd61cda1").unwrap(), // setter
            Uuid::parse_str("7485a1c4-0032-4db7-993a-1d5667f0ebce").unwrap(),
            Uuid::parse_str("a026a9e5-4bdc-4559-a21a-b62fe3c8027a").unwrap(),
            Uuid::parse_str("8c91b74b-06a9-416c-8f86-2f75aa37e343").unwrap(),
            Uuid::parse_str("45598587-0df7-4627-b63e-e337272182e2").unwrap(),
            Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(),
        ],
        libero: Uuid::parse_str("2b604e66-3006-4b11-943e-d6d580b9bad9").unwrap(),
        setter: Uuid::parse_str("bc19a25b-1ab0-4ade-9298-7c67cd61cda1").unwrap(), // P1
        events: Vec::new(),
    };
    let result = get_serving_player(&s, 4); // P5
    assert_eq!(result.to_string(), "a026a9e5-4bdc-4559-a21a-b62fe3c8027a");
}

#[test]
fn setter_position() {
    let s: SetEntry = SetEntry {
        set_number: 1,
        serving_team: TeamSideEnum::Them,
        positions: [
            Uuid::parse_str("bc19a25b-1ab0-4ade-9298-7c67cd61cda1").unwrap(),
            Uuid::parse_str("7485a1c4-0032-4db7-993a-1d5667f0ebce").unwrap(),
            Uuid::parse_str("a026a9e5-4bdc-4559-a21a-b62fe3c8027a").unwrap(),
            Uuid::parse_str("8c91b74b-06a9-416c-8f86-2f75aa37e343").unwrap(),
            Uuid::parse_str("45598587-0df7-4627-b63e-e337272182e2").unwrap(),
            Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // setter
        ],
        libero: Uuid::parse_str("2b604e66-3006-4b11-943e-d6d580b9bad9").unwrap(),
        setter: Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // P6
        events: Vec::new(),
    };
    let result = get_setter(&s);
    assert_eq!(result.to_string(), "e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a");
}

#[test]
fn oh1_position() {
    let s: SetEntry = SetEntry {
        set_number: 1,
        serving_team: TeamSideEnum::Them,
        positions: [
            Uuid::parse_str("bc19a25b-1ab0-4ade-9298-7c67cd61cda1").unwrap(),
            Uuid::parse_str("7485a1c4-0032-4db7-993a-1d5667f0ebce").unwrap(),
            Uuid::parse_str("a026a9e5-4bdc-4559-a21a-b62fe3c8027a").unwrap(),
            Uuid::parse_str("8c91b74b-06a9-416c-8f86-2f75aa37e343").unwrap(),
            Uuid::parse_str("45598587-0df7-4627-b63e-e337272182e2").unwrap(),
            Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // setter
        ],
        libero: Uuid::parse_str("2b604e66-3006-4b11-943e-d6d580b9bad9").unwrap(),
        setter: Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // P6
        events: Vec::new(),
    };
    let result = get_oh1(&s);
    assert_eq!(result.to_string(), "bc19a25b-1ab0-4ade-9298-7c67cd61cda1");
}

#[test]
fn oh2_position() {
    let s: SetEntry = SetEntry {
        set_number: 1,
        serving_team: TeamSideEnum::Them,
        positions: [
            Uuid::parse_str("bc19a25b-1ab0-4ade-9298-7c67cd61cda1").unwrap(),
            Uuid::parse_str("7485a1c4-0032-4db7-993a-1d5667f0ebce").unwrap(),
            Uuid::parse_str("a026a9e5-4bdc-4559-a21a-b62fe3c8027a").unwrap(),
            Uuid::parse_str("8c91b74b-06a9-416c-8f86-2f75aa37e343").unwrap(),
            Uuid::parse_str("45598587-0df7-4627-b63e-e337272182e2").unwrap(),
            Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // setter
        ],
        libero: Uuid::parse_str("2b604e66-3006-4b11-943e-d6d580b9bad9").unwrap(),
        setter: Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // P6
        events: Vec::new(),
    };
    let result = get_oh2(&s);
    assert_eq!(result.to_string(), "8c91b74b-06a9-416c-8f86-2f75aa37e343");
}

#[test]
fn opposite_position() {
    let s: SetEntry = SetEntry {
        set_number: 1,
        serving_team: TeamSideEnum::Them,
        positions: [
            Uuid::parse_str("bc19a25b-1ab0-4ade-9298-7c67cd61cda1").unwrap(),
            Uuid::parse_str("7485a1c4-0032-4db7-993a-1d5667f0ebce").unwrap(),
            Uuid::parse_str("a026a9e5-4bdc-4559-a21a-b62fe3c8027a").unwrap(),
            Uuid::parse_str("8c91b74b-06a9-416c-8f86-2f75aa37e343").unwrap(),
            Uuid::parse_str("45598587-0df7-4627-b63e-e337272182e2").unwrap(),
            Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // setter
        ],
        libero: Uuid::parse_str("2b604e66-3006-4b11-943e-d6d580b9bad9").unwrap(),
        setter: Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // P6
        events: Vec::new(),
    };
    let result = get_opposite(&s);
    assert_eq!(result.to_string(), "a026a9e5-4bdc-4559-a21a-b62fe3c8027a");
}

#[test]
fn mb1_position() {
    let s: SetEntry = SetEntry {
        set_number: 1,
        serving_team: TeamSideEnum::Them,
        positions: [
            Uuid::parse_str("bc19a25b-1ab0-4ade-9298-7c67cd61cda1").unwrap(),
            Uuid::parse_str("7485a1c4-0032-4db7-993a-1d5667f0ebce").unwrap(),
            Uuid::parse_str("a026a9e5-4bdc-4559-a21a-b62fe3c8027a").unwrap(),
            Uuid::parse_str("8c91b74b-06a9-416c-8f86-2f75aa37e343").unwrap(),
            Uuid::parse_str("45598587-0df7-4627-b63e-e337272182e2").unwrap(),
            Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // setter
        ],
        libero: Uuid::parse_str("2b604e66-3006-4b11-943e-d6d580b9bad9").unwrap(),
        setter: Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // P6
        events: Vec::new(),
    };
    let result = get_mb1(&s);
    assert_eq!(result.to_string(), "45598587-0df7-4627-b63e-e337272182e2");
}

#[test]
fn mb2_position() {
    let s: SetEntry = SetEntry {
        set_number: 1,
        serving_team: TeamSideEnum::Them,
        positions: [
            Uuid::parse_str("bc19a25b-1ab0-4ade-9298-7c67cd61cda1").unwrap(),
            Uuid::parse_str("7485a1c4-0032-4db7-993a-1d5667f0ebce").unwrap(),
            Uuid::parse_str("a026a9e5-4bdc-4559-a21a-b62fe3c8027a").unwrap(),
            Uuid::parse_str("8c91b74b-06a9-416c-8f86-2f75aa37e343").unwrap(),
            Uuid::parse_str("45598587-0df7-4627-b63e-e337272182e2").unwrap(),
            Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // setter
        ],
        libero: Uuid::parse_str("2b604e66-3006-4b11-943e-d6d580b9bad9").unwrap(),
        setter: Uuid::parse_str("e08f5e9d-6ce8-442a-89d8-9ddaa1eba54a").unwrap(), // P6
        events: Vec::new(),
    };
    let result = get_mb2(&s);
    assert_eq!(result.to_string(), "7485a1c4-0032-4db7-993a-1d5667f0ebce");
}
