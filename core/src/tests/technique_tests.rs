//! The set received by the attacker and the serve, saved in the last column
//! of the events: which ones the scout chooses from, which one is chosen
//! first, how they're saved and read back, and how the snapshot checks them.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::errors::{AppError, SnapshotError};
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, RoleEnum, TeamSideEnum},
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        stats::Counter,
        technique::{ServeTypeEnum, SetTypeEnum, TechniqueEnum},
    };
    use EvalEnum::*;
    use EventTypeEnum::*;
    use RoleEnum::*;
    use ServeTypeEnum::*;
    // B is the block: the set is SetTypeEnum::B
    use SetTypeEnum::{
        C, Eight, Fast, Five, Four, High, Nine, One, Pipe, Quick, Seven, Six, Super, Three, Two,
    };
    use chrono::{TimeZone, Utc};
    use csv::{ReaderBuilder, WriterBuilder};
    use std::collections::HashMap;
    use std::str::FromStr;
    use uuid::Uuid;

    // --- the sets to choose from --------------------------------------------

    #[test]
    fn sets_available_by_role_and_row() {
        let cases = [
            (
                MiddleBlocker,
                false,
                vec![One, Two, Three, Four, C, SetTypeEnum::B, Seven, Fast],
            ),
            (
                MiddleBlocker,
                true,
                vec![One, Two, Three, Four, C, SetTypeEnum::B, Seven, Fast],
            ),
            (OutsideHitter, false, vec![High, Five, Super, Nine, Quick]),
            (OppositeHitter, false, vec![High, Six, Super, Quick]),
            (OutsideHitter, true, vec![High, Pipe]),
            (OppositeHitter, true, vec![High, Eight]),
            // the setter attacks on the second touch only, the libero never
            (Setter, false, vec![]),
            (Setter, true, vec![]),
            (Libero, false, vec![]),
            (Libero, true, vec![]),
        ];
        for (role, back_row, expected) in cases {
            assert_eq!(
                SetTypeEnum::available(role, back_row),
                expected,
                "{role:?} back row {back_row}"
            );
        }
    }

    /// Every set is available to someone, and every preselected set is among
    /// the available ones.
    #[test]
    fn every_set_is_available_and_preselections_are_among_them() {
        let roles = [MiddleBlocker, OutsideHitter, OppositeHitter, Setter, Libero];
        let combos: Vec<(RoleEnum, bool)> = roles
            .iter()
            .flat_map(|role| [(*role, false), (*role, true)])
            .collect();
        for set in SetTypeEnum::ALL {
            assert!(
                combos
                    .iter()
                    .any(|(role, back)| SetTypeEnum::available(*role, *back).contains(&set)),
                "{set:?} never available"
            );
        }
        let previous_touches: Vec<Option<(EventTypeEnum, Option<EvalEnum>)>> =
            std::iter::once(None)
                .chain(EventTypeEnum::ALL.iter().flat_map(|event| {
                    std::iter::once(None)
                        .chain(EvalEnum::ALL.iter().copied().map(Some))
                        .map(move |eval| Some((*event, eval)))
                }))
                .collect();
        for (role, back) in &combos {
            for previous in &previous_touches {
                let available = SetTypeEnum::available(*role, *back);
                match SetTypeEnum::preselected(*role, *back, *previous) {
                    Some(set) => assert!(
                        available.contains(&set),
                        "{set:?} for {role:?} back {back} after {previous:?}"
                    ),
                    // nothing to choose only when there's no set at all
                    None => assert!(available.is_empty(), "{role:?} back {back}"),
                }
            }
        }
    }

    // --- the set chosen first -------------------------------------------------

    #[test]
    fn preselected_set_of_the_middle_blockers() {
        for previous in [
            None,
            Some((P, Some(Perfect))),
            Some((P, Some(Negative))),
            Some((D, Some(Positive))),
            Some((S, Some(Over))),
            Some((A, Some(Exclamative))),
        ] {
            assert_eq!(
                SetTypeEnum::preselected(MiddleBlocker, false, previous),
                Some(One)
            );
            assert_eq!(
                SetTypeEnum::preselected(MiddleBlocker, true, previous),
                Some(One)
            );
        }
    }

    #[test]
    fn preselected_set_in_the_back_row() {
        for previous in [
            None,
            Some((P, Some(Perfect))),
            Some((P, Some(Positive))),
            Some((D, Some(Negative))),
            Some((S, Some(Over))),
            Some((B, Some(Positive))),
        ] {
            assert_eq!(
                SetTypeEnum::preselected(OutsideHitter, true, previous),
                Some(Pipe),
                "{previous:?}"
            );
            assert_eq!(
                SetTypeEnum::preselected(OppositeHitter, true, previous),
                Some(Eight),
                "{previous:?}"
            );
        }
    }

    /// In the front row the set follows the reception or the dig before.
    #[test]
    fn preselected_set_in_the_front_row_after_reception_or_dig() {
        let cases = [
            (Perfect, Super, Super),
            (Positive, Five, Six),
            (Negative, High, High),
            (Exclamative, High, High),
        ];
        for touch in [P, D] {
            for (eval, outside, opposite) in cases {
                assert_eq!(
                    SetTypeEnum::preselected(OutsideHitter, false, Some((touch, Some(eval)))),
                    Some(outside),
                    "outside hitter after {touch} {eval}"
                );
                assert_eq!(
                    SetTypeEnum::preselected(OppositeHitter, false, Some((touch, Some(eval)))),
                    Some(opposite),
                    "opposite after {touch} {eval}"
                );
            }
        }
    }

    /// After their reception coming straight back, their freeball, an
    /// attack of ours `+` or `!`, or a block `+`: the super.
    #[test]
    fn preselected_set_in_the_front_row_after_a_ball_coming_back() {
        for previous in [
            (S, Some(Over)),
            (A, Some(Positive)),
            (A, Some(Exclamative)),
            (B, Some(Positive)),
            (OF, None),
        ] {
            for role in [OutsideHitter, OppositeHitter] {
                assert_eq!(
                    SetTypeEnum::preselected(role, false, Some(previous)),
                    Some(Super),
                    "{role:?} after {previous:?}"
                );
            }
        }
    }

    /// A set is always chosen for the hitters: high when nothing else tells.
    #[test]
    fn preselected_set_in_the_front_row_otherwise() {
        for previous in [
            None,
            Some((A, Some(Negative))),
            Some((B, Some(Negative))),
            Some((S, Some(Positive))),
            // their first touch coming over after our freeball
            Some((FB, None)),
        ] {
            for role in [OutsideHitter, OppositeHitter] {
                assert_eq!(
                    SetTypeEnum::preselected(role, false, previous),
                    Some(High),
                    "{role:?} after {previous:?}"
                );
            }
        }
    }

    #[test]
    fn no_preselected_set_for_the_setter_and_the_libero() {
        for (role, back) in [(Setter, false), (Setter, true), (Libero, true)] {
            assert_eq!(
                SetTypeEnum::preselected(role, back, Some((P, Some(Perfect)))),
                None
            );
        }
    }

    // --- the serve chosen first -----------------------------------------------

    fn serve(player: Uuid, technique: Option<TechniqueEnum>) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type: S,
            player: Some(player),
            eval: Some(Positive),
            target_player: None,
            technique,
        }
    }

    #[test]
    fn preselected_serve_is_float_before_serving() {
        let server = Uuid::new_v4();
        assert_eq!(ServeTypeEnum::preselected(&server, [].iter()), Float);
        // others' serves don't count
        let events = [serve(Uuid::new_v4(), Some(TechniqueEnum::Serve(Spin)))];
        assert_eq!(ServeTypeEnum::preselected(&server, events.iter()), Float);
    }

    #[test]
    fn preselected_serve_is_the_last_of_the_server() {
        let (server, other) = (Uuid::new_v4(), Uuid::new_v4());
        let events = [
            serve(server, Some(TechniqueEnum::Serve(Float))),
            serve(server, Some(TechniqueEnum::Serve(Spin))),
            serve(other, Some(TechniqueEnum::Serve(Float))),
        ];
        assert_eq!(ServeTypeEnum::preselected(&server, events.iter()), Spin);
        assert_eq!(ServeTypeEnum::preselected(&other, events.iter()), Float);
    }

    /// The serves scouted before the technique existed tell nothing.
    #[test]
    fn preselected_serve_skips_the_serves_without_technique() {
        let server = Uuid::new_v4();
        let events = [
            serve(server, Some(TechniqueEnum::Serve(Spin))),
            serve(server, None),
        ];
        assert_eq!(ServeTypeEnum::preselected(&server, events.iter()), Spin);
        assert_eq!(
            ServeTypeEnum::preselected(&server, [serve(server, None)].iter()),
            Float
        );
    }

    // --- codes ----------------------------------------------------------------

    #[test]
    fn set_codes() {
        let codes: Vec<String> = SetTypeEnum::ALL.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            codes,
            [
                "1", "2", "3", "4", "C", "B", "7", "F", "A", "5", "6", "S", "9", "Q", "0", "8"
            ]
        );
        for set in SetTypeEnum::ALL {
            assert_eq!(SetTypeEnum::from_str(&set.to_string()).unwrap(), set);
            assert_eq!(
                SetTypeEnum::from_str(&set.to_string().to_lowercase()).unwrap(),
                set
            );
        }
        for code in ["", "X", "10", "AA", "#"] {
            assert!(SetTypeEnum::from_str(code).is_err(), "{code}");
        }
    }

    #[test]
    fn serve_codes() {
        assert_eq!(Float.to_string(), "F");
        assert_eq!(Spin.to_string(), "S");
        for serve in ServeTypeEnum::ALL {
            assert_eq!(ServeTypeEnum::from_str(&serve.to_string()).unwrap(), serve);
        }
        assert!(ServeTypeEnum::from_str("A").is_err());
    }

    /// The same code is a set for an attack and a serve for a serve.
    #[test]
    fn techniques_parsed_by_event() {
        let cases = [
            (A, "F", TechniqueEnum::Set(Fast)),
            (S, "F", TechniqueEnum::Serve(Float)),
            (A, "S", TechniqueEnum::Set(Super)),
            (S, "S", TechniqueEnum::Serve(Spin)),
            (A, "0", TechniqueEnum::Set(Pipe)),
            (A, "a", TechniqueEnum::Set(High)),
        ];
        for (event, code, expected) in cases {
            assert_eq!(TechniqueEnum::parse(event, code).unwrap(), expected);
        }
        // a set isn't a serve
        assert!(TechniqueEnum::parse(S, "5").is_err());
        // nor do the other events have one
        for event in [P, D, B, F, OS, OE, R, CL, CS] {
            assert!(TechniqueEnum::parse(event, "A").is_err(), "{event}");
        }
    }

    #[test]
    fn techniques_fit_their_events() {
        for event in EventTypeEnum::ALL {
            assert_eq!(TechniqueEnum::Set(High).fits(event), event == A, "{event}");
            assert_eq!(
                TechniqueEnum::Serve(Float).fits(event),
                event == S,
                "{event}"
            );
        }
    }

    #[test]
    fn technique_descriptions() {
        let labels = crate::localization::current_labels();
        assert_eq!(
            TechniqueEnum::Set(High).description(),
            format!("A ({})", labels.high_set)
        );
        assert_eq!(TechniqueEnum::Set(Five).description(), "5");
        assert_eq!(
            TechniqueEnum::Serve(Spin).description(),
            format!("S ({})", labels.spin_serve)
        );
    }

    // --- the events file ------------------------------------------------------

    fn event(
        event_type: EventTypeEnum,
        eval: Option<EvalEnum>,
        technique: Option<TechniqueEnum>,
    ) -> EventEntry {
        EventEntry {
            timestamp: Utc.with_ymd_and_hms(2026, 10, 19, 20, 0, 0).unwrap(),
            event_type,
            player: Some(Uuid::nil()),
            eval,
            target_player: None,
            technique,
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

    #[test]
    fn technique_saved_as_the_last_column() {
        let csv = to_csv(&[
            event(A, Some(Perfect), Some(TechniqueEnum::Set(Super))),
            event(S, Some(Positive), Some(TechniqueEnum::Serve(Float))),
            event(D, Some(Positive), None),
        ]);
        let nil = Uuid::nil();
        assert_eq!(
            csv,
            format!(
                "2026-10-19T20:00:00Z,A,{nil},#,,S\n\
                 2026-10-19T20:00:00Z,S,{nil},+,,F\n\
                 2026-10-19T20:00:00Z,D,{nil},+,,\n"
            )
        );
    }

    #[test]
    fn techniques_read_back() {
        let events = [
            event(A, Some(Perfect), Some(TechniqueEnum::Set(Fast))),
            event(S, Some(Positive), Some(TechniqueEnum::Serve(Float))),
            event(A, Some(Positive), Some(TechniqueEnum::Set(Super))),
            event(S, Some(Error), Some(TechniqueEnum::Serve(Spin))),
            event(B, Some(Positive), None),
        ];
        let read = from_csv(&to_csv(&events)).unwrap();
        let techniques: Vec<_> = read.iter().map(|e| e.technique).collect();
        let expected: Vec<_> = events.iter().map(|e| e.technique).collect();
        assert_eq!(techniques, expected);
    }

    /// Rows saved up to v0.4.3 have no technique column.
    #[test]
    fn rows_without_technique_read() {
        let nil = Uuid::nil();
        let read = from_csv(&format!(
            "2026-10-19T20:00:00Z,A,{nil},#,\n2026-10-19T20:00:00Z,A,{nil},#,,5\n"
        ))
        .unwrap();
        assert_eq!(read[0].technique, None);
        assert_eq!(read[1].technique, Some(TechniqueEnum::Set(Five)));
    }

    #[test]
    fn rows_with_an_invalid_technique_dont_read() {
        let nil = Uuid::nil();
        for row in [
            // no such set
            format!("2026-10-19T20:00:00Z,A,{nil},#,,X\n"),
            // a set for a serve
            format!("2026-10-19T20:00:00Z,S,{nil},#,,5\n"),
            // a technique for a dig
            format!("2026-10-19T20:00:00Z,D,{nil},#,,A\n"),
        ] {
            assert!(from_csv(&row).is_err(), "{row}");
        }
    }

    // --- the snapshot ---------------------------------------------------------

    struct Team {
        setter: Uuid,
        oh1: Uuid,
        mb2: Uuid,
        opposite: Uuid,
        oh2: Uuid,
        mb1: Uuid,
        libero: Uuid,
    }

    impl Team {
        fn new() -> Self {
            Team {
                setter: Uuid::new_v4(),
                oh1: Uuid::new_v4(),
                mb2: Uuid::new_v4(),
                opposite: Uuid::new_v4(),
                oh2: Uuid::new_v4(),
                mb1: Uuid::new_v4(),
                libero: Uuid::new_v4(),
            }
        }

        fn replay(
            &self,
            positions: [Uuid; 6],
            serving_team: TeamSideEnum,
            events: Vec<EventEntry>,
        ) -> Result<Snapshot, AppError> {
            SetEntry {
                set_number: 1,
                serving_team,
                initial_positions: positions,
                libero: self.libero,
                fallback_libero: None,
                setter: self.setter,
                lineup_rules: Some(LineupRulesEnum::CURRENT),
                events,
            }
            .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
        }

        /// Rotation 0, receiving: the setter in position 1, outside hitter 1,
        /// middle blocker 2 and the opposite in the front row, outside
        /// hitter 2 in the back row, the libero in for middle blocker 1.
        fn setter_in_the_back_row(&self, events: Vec<EventEntry>) -> Snapshot {
            self.replay(
                [
                    self.setter,
                    self.oh1,
                    self.mb2,
                    self.opposite,
                    self.oh2,
                    self.mb1,
                ],
                TeamSideEnum::Them,
                events,
            )
            .unwrap_or_else(|e| panic!("expected the set to replay: {e}"))
        }

        /// The setter in position 4, receiving: the opposite in position 1
        /// and outside hitter 1 in position 5 are in the back row.
        fn setter_in_the_front_row(&self, events: Vec<EventEntry>) -> Snapshot {
            self.replay(
                [
                    self.opposite,
                    self.oh2,
                    self.mb1,
                    self.setter,
                    self.oh1,
                    self.mb2,
                ],
                TeamSideEnum::Them,
                events,
            )
            .unwrap_or_else(|e| panic!("expected the set to replay: {e}"))
        }
    }

    fn act(
        event_type: EventTypeEnum,
        player: Uuid,
        eval: EvalEnum,
        technique: Option<TechniqueEnum>,
    ) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type,
            player: Some(player),
            eval: Some(eval),
            target_player: None,
            technique,
        }
    }

    fn opp(event_type: EventTypeEnum) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type,
            player: None,
            eval: None,
            target_player: None,
            technique: None,
        }
    }

    #[test]
    fn sets_available_on_court() {
        let t = Team::new();
        let s = t.setter_in_the_back_row(vec![act(P, t.libero, Positive, None)]);
        assert_eq!(
            s.available_set_types(&t.oh1),
            SetTypeEnum::available(OutsideHitter, false)
        );
        assert_eq!(
            s.available_set_types(&t.opposite),
            SetTypeEnum::available(OppositeHitter, false)
        );
        assert_eq!(
            s.available_set_types(&t.mb2),
            SetTypeEnum::available(MiddleBlocker, false)
        );
        assert_eq!(
            s.available_set_types(&t.oh2),
            SetTypeEnum::available(OutsideHitter, true)
        );
        assert!(s.available_set_types(&t.setter).is_empty());
        assert!(s.available_set_types(&t.libero).is_empty());
        // not on court
        assert!(s.available_set_types(&t.mb1).is_empty());
        assert!(s.available_set_types(&Uuid::new_v4()).is_empty());
        let s = t.setter_in_the_front_row(vec![act(P, t.libero, Positive, None)]);
        assert_eq!(
            s.available_set_types(&t.opposite),
            SetTypeEnum::available(OppositeHitter, true)
        );
        assert_eq!(
            s.available_set_types(&t.oh1),
            SetTypeEnum::available(OutsideHitter, true)
        );
        assert_eq!(
            s.available_set_types(&t.oh2),
            SetTypeEnum::available(OutsideHitter, false)
        );
        assert!(s.available_set_types(&t.setter).is_empty());
    }

    #[test]
    fn sets_preselected_on_court_after_the_reception() {
        let t = Team::new();
        let cases = [
            (Perfect, Super, Super),
            (Positive, Five, Six),
            (Exclamative, High, High),
            (Negative, High, High),
        ];
        for (eval, outside, opposite) in cases {
            let s = t.setter_in_the_back_row(vec![act(P, t.libero, eval, None)]);
            assert_eq!(s.preselected_set_type(&t.oh1), Some(outside), "{eval}");
            assert_eq!(
                s.preselected_set_type(&t.opposite),
                Some(opposite),
                "{eval}"
            );
            assert_eq!(s.preselected_set_type(&t.mb2), Some(One), "{eval}");
            assert_eq!(s.preselected_set_type(&t.oh2), Some(Pipe), "{eval}");
            assert_eq!(s.preselected_set_type(&t.setter), None, "{eval}");
            let s = t.setter_in_the_front_row(vec![act(P, t.libero, eval, None)]);
            assert_eq!(s.preselected_set_type(&t.opposite), Some(Eight), "{eval}");
        }
    }

    /// The touch telling the set is the last one: an attack covered (`!`)
    /// after a positive reception gives the super for the next attack.
    #[test]
    fn sets_preselected_on_court_after_an_attack_or_a_block() {
        let t = Team::new();
        let reception = act(P, t.libero, Positive, None);
        for second_touch in [Positive, Exclamative] {
            let s = t.setter_in_the_back_row(vec![
                reception.clone(),
                act(A, t.oh1, second_touch, Some(TechniqueEnum::Set(Five))),
            ]);
            assert_eq!(s.preselected_set_type(&t.oh1), Some(Super));
            assert_eq!(s.preselected_set_type(&t.opposite), Some(Super));
        }
        // our attack "-", their attack, our block "+"
        let s = t.setter_in_the_back_row(vec![
            reception,
            act(A, t.oh1, Negative, Some(TechniqueEnum::Set(Five))),
            act(B, t.mb2, Positive, None),
        ]);
        assert_eq!(s.preselected_set_type(&t.oh1), Some(Super));
        assert_eq!(s.preselected_set_type(&t.mb2), Some(One));
    }

    /// After our serve "/" (their reception coming straight back) we
    /// attack a free ball.
    #[test]
    fn sets_preselected_on_court_after_a_free_ball() {
        let t = Team::new();
        // we win the first rally: rotation 1, we serve
        let s = t.setter_in_the_back_row(vec![
            act(P, t.libero, Positive, None),
            act(A, t.oh1, Perfect, Some(TechniqueEnum::Set(Five))),
        ]);
        let server = s.current_lineup.get_serving_player().unwrap();
        let s = t.setter_in_the_back_row(vec![
            act(P, t.libero, Positive, None),
            act(A, t.oh1, Perfect, Some(TechniqueEnum::Set(Five))),
            act(S, server, Over, Some(TechniqueEnum::Serve(Float))),
        ]);
        let front_hitters: Vec<Uuid> = [t.oh2, t.opposite, t.oh1]
            .into_iter()
            .filter(|p| {
                s.current_lineup.find_position(p).is_some()
                    && !s.current_lineup.is_back_row_player(p)
            })
            .collect();
        assert!(!front_hitters.is_empty());
        for hitter in front_hitters {
            assert_eq!(s.preselected_set_type(&hitter), Some(Super));
        }
    }

    fn assert_invalid(result: Result<Snapshot, AppError>, reason: &str) {
        match result {
            Err(AppError::Snapshot(SnapshotError::InvalidEvent(error))) => {
                assert!(error.contains(reason), "{error}")
            }
            Err(e) => panic!("expected an invalid event ({reason}), got {e}"),
            Ok(_) => panic!("expected an invalid event ({reason})"),
        }
    }

    #[test]
    fn techniques_checked_by_the_snapshot() {
        let t = Team::new();
        let positions = [t.setter, t.oh1, t.mb2, t.opposite, t.oh2, t.mb1];
        let reception = act(P, t.libero, Positive, None);
        // a serve for an attack
        assert_invalid(
            t.replay(
                positions,
                TeamSideEnum::Them,
                vec![
                    reception.clone(),
                    act(A, t.oh1, Perfect, Some(TechniqueEnum::Serve(Float))),
                ],
            ),
            "technique",
        );
        // a set for a reception
        assert_invalid(
            t.replay(
                positions,
                TeamSideEnum::Them,
                vec![act(P, t.libero, Positive, Some(TechniqueEnum::Set(High)))],
            ),
            "technique",
        );
        // a set for a serve
        assert_invalid(
            t.replay(
                positions,
                TeamSideEnum::Us,
                vec![act(S, t.setter, Positive, Some(TechniqueEnum::Set(High)))],
            ),
            "technique",
        );
        // a technique for an opponent error
        assert_invalid(
            t.replay(
                positions,
                TeamSideEnum::Them,
                vec![EventEntry {
                    technique: Some(TechniqueEnum::Set(High)),
                    ..opp(OE)
                }],
            ),
            "technique",
        );
        // the right ones, or none as scouted up to v0.4.3
        for technique in [Some(TechniqueEnum::Set(Five)), None] {
            assert!(
                t.replay(
                    positions,
                    TeamSideEnum::Them,
                    vec![reception.clone(), act(A, t.oh1, Perfect, technique)],
                )
                .is_ok()
            );
        }
        for technique in [Some(TechniqueEnum::Serve(Spin)), None] {
            assert!(
                t.replay(
                    positions,
                    TeamSideEnum::Us,
                    vec![act(S, t.setter, Positive, technique)],
                )
                .is_ok()
            );
        }
    }

    /// For now the techniques are only recorded: score and statistics are
    /// the same with or without them.
    #[test]
    fn techniques_dont_change_the_replay() {
        let t = Team::new();
        let events = |with: bool| {
            let technique = |technique| with.then_some(technique);
            vec![
                act(P, t.libero, Positive, None),
                act(A, t.oh1, Positive, technique(TechniqueEnum::Set(Five))),
                act(A, t.opposite, Perfect, technique(TechniqueEnum::Set(Super))),
            ]
        };
        let (with, without) = (
            t.setter_in_the_back_row(events(true)),
            t.setter_in_the_back_row(events(false)),
        );
        assert_eq!(
            (with.score_us, with.score_them),
            (without.score_us, without.score_them)
        );
        fn counted<K: Clone + Eq + std::hash::Hash>(c: &Counter<K>) -> HashMap<K, u32> {
            c.iter(|_| true).map(|(k, v)| (k.clone(), v)).collect()
        }
        assert_eq!(counted(&with.stats.events), counted(&without.stats.events));
        assert_eq!(
            counted(&with.stats.attacks),
            counted(&without.stats.attacks)
        );
        assert_eq!(
            counted(&with.stats.rallies),
            counted(&without.stats.rallies)
        );
        assert_eq!(
            counted(&with.stats.first_rallies),
            counted(&without.stats.first_rallies)
        );
        assert_eq!(
            with.current_lineup.get_serving_player(),
            without.current_lineup.get_serving_player()
        );
    }
}
