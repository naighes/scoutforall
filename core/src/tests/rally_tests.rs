//! The projection of a set scouted on both sides: the stream of each side
//! from the one sequence of touches.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::domain::Side;
    use crate::rally::{Incoherence, Streams, admits, answer_evals, answers, project, replay};
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, PhaseEnum, TeamSideEnum},
        flags::EventFlags,
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
    };
    use EvalEnum::*;
    use EventTypeEnum::*;
    use chrono::{DateTime, TimeZone, Utc};
    use uuid::Uuid;

    // the n-th second of the set: the told events keep the time of the
    // touch telling them, exactly
    fn at(n: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 14, 18, 0, n)
            .single()
            .unwrap_or_default()
    }

    fn touch(
        side: Side,
        event_type: EventTypeEnum,
        eval: Option<EvalEnum>,
        n: u32,
    ) -> (Side, EventEntry) {
        let player = event_type.asks_player().then(Uuid::new_v4);
        (
            side,
            EventEntry {
                timestamp: at(n),
                event_type,
                player: player.filter(|_| event_type != F),
                eval,
                target_player: None,
                technique: None,
                flags: EventFlags::default(),
            },
        )
    }

    // the stream of the side as (event, evaluation, touch it comes from)
    fn stream(streams: &Streams, side: Side) -> Vec<(EventTypeEnum, Option<EvalEnum>, usize)> {
        streams
            .of(side)
            .iter()
            .map(|p| (p.entry.event_type, p.entry.eval, p.source))
            .collect()
    }

    #[test]
    fn each_side_keeps_its_touches_and_learns_the_point_it_lost() {
        let streams = project(&[
            touch(Side::A, S, Some(Positive), 0),
            touch(Side::B, P, Some(Negative), 1),
            touch(Side::B, A, Some(Perfect), 2),
        ]);
        assert_eq!(
            stream(&streams, Side::A),
            vec![(S, Some(Positive), 0), (OS, None, 2)]
        );
        assert_eq!(
            stream(&streams, Side::B),
            vec![(P, Some(Negative), 1), (A, Some(Perfect), 2)]
        );
    }

    #[test]
    fn the_other_side_of_the_same_point_needs_nothing_told() {
        // the ace, then the reception error
        let streams = project(&[
            touch(Side::A, S, Some(Perfect), 0),
            touch(Side::B, P, Some(Error), 1),
        ]);
        assert_eq!(stream(&streams, Side::A), vec![(S, Some(Perfect), 0)]);
        assert_eq!(stream(&streams, Side::B), vec![(P, Some(Error), 1)]);
    }

    #[test]
    fn an_ace_without_the_reception_is_their_point() {
        let streams = project(&[touch(Side::A, S, Some(Perfect), 0)]);
        assert_eq!(stream(&streams, Side::B), vec![(OS, None, 0)]);
    }

    #[test]
    fn an_error_is_the_other_side_s_opponent_error() {
        let streams = project(&[touch(Side::A, S, Some(Error), 0)]);
        assert_eq!(stream(&streams, Side::B), vec![(OE, None, 0)]);
    }

    #[test]
    fn a_fault_is_the_other_side_s_opponent_error() {
        let streams = project(&[touch(Side::B, F, None, 0)]);
        assert_eq!(stream(&streams, Side::A), vec![(OE, None, 0)]);
    }

    #[test]
    fn an_opponent_error_without_a_touch_is_the_other_side_s_fault() {
        let streams = project(&[touch(Side::A, OE, None, 0)]);
        assert_eq!(stream(&streams, Side::B), vec![(F, None, 0)]);
        assert_eq!(streams.b.first().and_then(|p| p.entry.player), None);
    }

    #[test]
    fn an_opponent_point_without_a_touch_is_the_other_side_s_opponent_error() {
        let streams = project(&[touch(Side::A, OS, None, 0)]);
        assert_eq!(stream(&streams, Side::B), vec![(OE, None, 0)]);
    }

    #[test]
    fn a_freeball_sent_is_the_freeball_the_other_side_receives() {
        let streams = project(&[
            touch(Side::A, S, Some(Positive), 0),
            touch(Side::B, P, Some(Negative), 1),
            touch(Side::B, FB, None, 2),
            touch(Side::A, A, Some(Perfect), 3),
        ]);
        assert_eq!(
            stream(&streams, Side::A),
            vec![(S, Some(Positive), 0), (OF, None, 2), (A, Some(Perfect), 3)]
        );
        assert_eq!(
            stream(&streams, Side::B),
            vec![(P, Some(Negative), 1), (FB, None, 2), (OS, None, 3)]
        );
        assert_eq!(streams.a.get(1).map(|p| p.entry.timestamp), Some(at(2)));
    }

    #[test]
    fn a_freeball_told_without_a_touch_is_the_other_side_s_freeball() {
        let streams = project(&[touch(Side::A, OF, None, 0)]);
        assert_eq!(stream(&streams, Side::B), vec![(FB, None, 0)]);
    }

    #[test]
    fn the_point_is_told_before_what_comes_after_it() {
        let streams = project(&[
            touch(Side::A, S, Some(Perfect), 0),
            touch(Side::B, R, None, 1),
            touch(Side::A, S, Some(Error), 2),
        ]);
        assert_eq!(
            stream(&streams, Side::B),
            vec![(OS, None, 0), (R, None, 1), (OE, None, 2)]
        );
        assert_eq!(streams.b.first().map(|p| p.entry.timestamp), Some(at(0)));
    }

    #[test]
    fn a_touch_with_another_winner_starts_the_next_rally() {
        // the ace told, then a point of theirs: the next rally
        let streams = project(&[
            touch(Side::A, S, Some(Perfect), 0),
            touch(Side::B, OE, None, 1),
        ]);
        assert_eq!(
            stream(&streams, Side::A),
            vec![(S, Some(Perfect), 0), (F, None, 1)]
        );
        assert_eq!(
            stream(&streams, Side::B),
            vec![(OS, None, 0), (OE, None, 1)]
        );
    }

    #[test]
    fn the_ace_of_the_winner_starts_the_next_rally() {
        // their attack error, then our ace: our point first, then the ace
        let streams = project(&[
            touch(Side::B, S, Some(Positive), 0),
            touch(Side::A, P, Some(Negative), 1),
            touch(Side::B, A, Some(Error), 2),
            touch(Side::A, S, Some(Perfect), 3),
        ]);
        assert_eq!(
            stream(&streams, Side::A),
            vec![(P, Some(Negative), 1), (OE, None, 2), (S, Some(Perfect), 3)]
        );
        assert_eq!(
            stream(&streams, Side::B),
            vec![(S, Some(Positive), 0), (A, Some(Error), 2), (OS, None, 3)]
        );
    }

    #[test]
    fn a_point_without_a_cause_is_the_other_side_s_the_other_way_round() {
        for (side, event, told) in [
            (Side::A, PW, PL),
            (Side::A, PL, PW),
            (Side::B, PW, PL),
            (Side::B, PL, PW),
        ] {
            let streams = project(&[touch(side, event, None, 0)]);
            assert_eq!(stream(&streams, side), vec![(event, None, 0)]);
            assert_eq!(stream(&streams, side.other()), vec![(told, None, 0)]);
        }
    }

    #[test]
    fn a_rally_being_played_tells_nothing_yet() {
        let streams = project(&[
            touch(Side::A, S, Some(Positive), 0),
            touch(Side::B, P, Some(Positive), 1),
        ]);
        assert_eq!(stream(&streams, Side::A), vec![(S, Some(Positive), 0)]);
        assert_eq!(stream(&streams, Side::B), vec![(P, Some(Positive), 1)]);
    }

    /// A team in rotation 0: the setter in 1, outside hitter 1 in 2,
    /// middle blocker 2 in 3, opposite in 4, outside hitter 2 in 5, middle
    /// blocker 1 in 6 (the libero for them).
    struct Team {
        positions: [Uuid; 6],
        libero: Uuid,
    }

    impl Team {
        fn new() -> Self {
            Team {
                positions: [(); 6].map(|_| Uuid::new_v4()),
                libero: Uuid::new_v4(),
            }
        }

        fn setter(&self) -> Uuid {
            self.positions[0]
        }

        fn outside_hitter(&self) -> Uuid {
            self.positions[1]
        }

        fn start(&self, serving_team: TeamSideEnum) -> SetEntry {
            SetEntry {
                set_number: 1,
                serving_team,
                initial_positions: self.positions,
                libero: self.libero,
                fallback_libero: None,
                setter: self.setter(),
                lineup_rules: Some(LineupRulesEnum::CURRENT),
                events: vec![],
            }
        }
    }

    fn by(
        side: Side,
        event_type: EventTypeEnum,
        player: Uuid,
        eval: EvalEnum,
        n: u32,
    ) -> (Side, EventEntry) {
        let (side, entry) = touch(side, event_type, Some(eval), n);
        (
            side,
            EventEntry {
                player: Some(player),
                ..entry
            },
        )
    }

    // A serving
    fn replayed(
        a: &Team,
        b: &Team,
        touches: &[(Side, EventEntry)],
    ) -> Result<(Snapshot, Snapshot), Incoherence> {
        replay(
            &a.start(TeamSideEnum::Us),
            &b.start(TeamSideEnum::Them),
            touches,
            DEFAULT_MAX_SUBSTITUTIONS,
        )
    }

    #[test]
    fn both_sides_replay_a_rally_and_agree_on_the_score() {
        let (a, b) = (Team::new(), Team::new());
        let replayed = replayed(
            &a,
            &b,
            &[
                by(Side::A, S, a.setter(), Positive, 0),
                by(Side::B, P, b.libero, Negative, 1),
                by(Side::B, A, b.outside_hitter(), Perfect, 2),
                // B serves now
                by(Side::B, S, b.outside_hitter(), Error, 3),
            ],
        );
        let scores = replayed.map(|(a, b)| {
            (
                (a.score_us, a.score_them),
                (b.score_us, b.score_them),
                a.current_lineup.get_current_phase(),
            )
        });
        assert_eq!(scores, Ok(((1, 1), (1, 1), PhaseEnum::Break)));
    }

    #[test]
    fn a_touch_not_following_in_its_side_is_refused_with_its_place() {
        let (a, b) = (Team::new(), Team::new());
        // B's reception, then its attack not scouted: A's kill can't be
        // B's opponent point right after a reception
        let refused = replayed(
            &a,
            &b,
            &[
                by(Side::A, S, a.setter(), Positive, 0),
                by(Side::B, P, b.libero, Positive, 1),
                by(Side::A, A, a.outside_hitter(), Perfect, 2),
            ],
        );
        assert!(
            matches!(
                refused,
                Err(Incoherence::Refused {
                    side: Side::B,
                    source: 2,
                    ..
                })
            ),
            "{refused:?}"
        );
    }

    #[test]
    fn a_player_of_the_other_team_is_refused() {
        let (a, b) = (Team::new(), Team::new());
        let refused = replayed(&a, &b, &[by(Side::A, S, b.setter(), Positive, 0)]);
        assert!(
            matches!(
                refused,
                Err(Incoherence::Refused {
                    side: Side::A,
                    source: 0,
                    ..
                })
            ),
            "{refused:?}"
        );
    }

    #[test]
    fn both_sides_must_start_with_the_same_serve() {
        let (a, b) = (Team::new(), Team::new());
        let refused = replay(
            &a.start(TeamSideEnum::Us),
            &b.start(TeamSideEnum::Us),
            &[],
            DEFAULT_MAX_SUBSTITUTIONS,
        );
        assert!(matches!(refused, Err(Incoherence::Serving)), "{refused:?}");
    }

    /// Whether the side can add the touch after the touches (A serving).
    fn admitted(
        a: &Team,
        b: &Team,
        touches: &[(Side, EventEntry)],
        next: (Side, EventEntry),
    ) -> bool {
        match replayed(a, b, touches) {
            Ok((sa, sb)) => admits(touches, &sa, &sb, next.0, &next.1),
            Err(e) => panic!("{e:?}"),
        }
    }

    #[test]
    fn after_a_serve_only_the_reception_of_the_other_side() {
        let (a, b) = (Team::new(), Team::new());
        let serve = [by(Side::A, S, a.setter(), Exclamative, 0)];
        assert!(admitted(
            &a,
            &b,
            &serve,
            by(Side::B, P, b.libero, Exclamative, 1)
        ));
        // the server's freeball before it: the reception missing
        assert!(!admitted(&a, &b, &serve, touch(Side::A, FB, None, 1)));
        assert!(!admitted(
            &a,
            &b,
            &serve,
            by(Side::B, A, b.outside_hitter(), Positive, 1)
        ));
    }

    #[test]
    fn a_serve_over_wants_the_reception_over() {
        // their reception came straight back: the server's half
        let (a, b) = (Team::new(), Team::new());
        let serve = [by(Side::A, S, a.setter(), Over, 0)];
        assert!(admitted(&a, &b, &serve, by(Side::B, P, b.libero, Over, 1)));
        assert!(!admitted(
            &a,
            &b,
            &serve,
            by(Side::B, P, b.libero, Positive, 1)
        ));
        assert!(!admitted(
            &a,
            &b,
            &serve,
            by(Side::B, P, b.libero, Error, 1)
        ));
        assert!(!admitted(
            &a,
            &b,
            &serve,
            by(Side::A, A, a.outside_hitter(), Positive, 1)
        ));
    }

    #[test]
    fn an_answer_closing_the_rally_needs_the_ball_on_its_side() {
        let (a, b) = (Team::new(), Team::new());
        let rally = [
            by(Side::A, S, a.setter(), Positive, 0),
            by(Side::B, P, b.libero, Negative, 1),
            // blocked and covered (A's dig going over, untold)
            by(Side::B, A, b.outside_hitter(), Exclamative, 2),
        ];
        // A's block touched, the ball back to B
        assert!(admitted(
            &a,
            &b,
            &rally,
            by(Side::A, B, a.outside_hitter(), Exclamative, 3)
        ));
        assert!(!admitted(
            &a,
            &b,
            &rally,
            by(Side::A, B, a.outside_hitter(), Perfect, 3)
        ));
        assert!(!admitted(
            &a,
            &b,
            &rally,
            by(Side::A, D, a.libero, Positive, 3)
        ));
        assert!(admitted(
            &a,
            &b,
            &rally,
            by(Side::B, A, b.outside_hitter(), Perfect, 3)
        ));
    }

    #[test]
    fn the_other_half_of_the_point_is_admitted() {
        let (a, b) = (Team::new(), Team::new());
        let ace = [by(Side::A, S, a.setter(), Perfect, 0)];
        assert!(admitted(&a, &b, &ace, by(Side::B, P, b.libero, Error, 1)));
    }

    /// The evaluations of the touch admitted after the touches, in the
    /// order of `EvalEnum::ALL`.
    fn admitted_evals(
        a: &Team,
        b: &Team,
        touches: &[(Side, EventEntry)],
        side: Side,
        event_type: EventTypeEnum,
        player: Uuid,
    ) -> Vec<EvalEnum> {
        let n = u32::try_from(touches.len()).unwrap();
        EvalEnum::ALL
            .into_iter()
            .filter(|eval| admitted(a, b, touches, by(side, event_type, player, *eval, n)))
            .collect()
    }

    /// The serve decides the reception (TODO/001, §4.2): one value, no
    /// choice, a choice only where the game leaves one.
    #[test]
    fn the_serve_decides_the_reception() {
        let (a, b) = (Team::new(), Team::new());
        let receptions = |serve: EvalEnum| {
            let serve = [by(Side::A, S, a.setter(), serve, 0)];
            admitted_evals(&a, &b, &serve, Side::B, P, b.libero)
        };
        assert_eq!(receptions(Perfect), vec![Error]);
        assert_eq!(receptions(Error), vec![]);
        assert_eq!(receptions(Over), vec![Over]);
        assert_eq!(receptions(Positive), vec![Negative]);
        assert_eq!(receptions(Exclamative), vec![Exclamative]);
        assert_eq!(receptions(Negative), vec![Perfect, Positive]);
    }

    /// The attack decides the dig or the block of the other side (TODO/001,
    /// §4.2): a kill is never blocked out (`/`), an attack error never a
    /// block point.
    #[test]
    fn the_attack_decides_the_dig_and_the_block() {
        let (a, b) = (Team::new(), Team::new());
        let answers = |attack: EvalEnum, answer: EventTypeEnum| {
            let rally = [
                by(Side::A, S, a.setter(), Positive, 0),
                by(Side::B, P, b.libero, Negative, 1),
                by(Side::B, A, b.outside_hitter(), attack, 2),
            ];
            let player = match answer {
                D => a.libero,
                _ => a.outside_hitter(),
            };
            admitted_evals(&a, &b, &rally, Side::A, answer, player)
        };
        assert_eq!(answers(Perfect, D), vec![Error]);
        assert_eq!(answers(Perfect, B), vec![Error]);
        assert_eq!(answers(Error, D), vec![]);
        assert_eq!(answers(Error, B), vec![]);
        assert_eq!(answers(Over, D), vec![]);
        assert_eq!(answers(Over, B), vec![Perfect]);
        // the attack `-` was dug easily: the dig `+` or `#`, never `-`
        assert_eq!(answers(Negative, D), vec![Perfect, Positive]);
        assert_eq!(answers(Negative, B), vec![Positive]);
        // the attack `+` opens the roads: the dig `-` or `/`, the block
        // `-`; the attack `!` was blocked and covered: the block `!`
        assert_eq!(answers(Positive, D), vec![Over, Negative]);
        assert_eq!(answers(Exclamative, D), vec![]);
        assert_eq!(answers(Positive, B), vec![Negative]);
        assert_eq!(answers(Exclamative, B), vec![Exclamative]);
    }

    /// After A's attack `+` the roads are open: B digs it (`-` or `/`),
    /// blocks it, or plays on without the dig; A plays again (B's dig `/`
    /// untold), never digs or receives without B's attack.
    #[test]
    fn after_an_attack_positive_both_sides_can_play() {
        let (a, b) = (Team::new(), Team::new());
        let rally = [
            by(Side::A, S, a.setter(), Positive, 0),
            by(Side::B, P, b.libero, Negative, 1),
            by(Side::B, A, b.outside_hitter(), Negative, 2),
            by(Side::A, D, a.libero, Positive, 3),
            by(Side::A, A, a.outside_hitter(), Positive, 4),
        ];
        assert_eq!(
            admitted_evals(&a, &b, &rally, Side::B, D, b.libero),
            vec![Over, Negative]
        );
        assert!(admitted(
            &a,
            &b,
            &rally,
            by(Side::B, A, b.outside_hitter(), Negative, 5)
        ));
        assert!(admitted(&a, &b, &rally, touch(Side::B, FB, None, 5)));
        assert!(admitted(
            &a,
            &b,
            &rally,
            by(Side::A, A, a.outside_hitter(), Perfect, 5)
        ));
        assert!(admitted(&a, &b, &rally, touch(Side::A, FB, None, 5)));
        assert!(!admitted(
            &a,
            &b,
            &rally,
            by(Side::A, D, a.libero, Positive, 5)
        ));
        assert!(!admitted(
            &a,
            &b,
            &rally,
            by(Side::A, B, a.outside_hitter(), Positive, 5)
        ));
    }

    /// The cover after the own attack `!`, or after the block `!` of the
    /// other side: `!` or `-`.
    #[test]
    fn the_cover_follows_the_own_attack_exclamative() {
        let (a, b) = (Team::new(), Team::new());
        let rally = [
            by(Side::A, S, a.setter(), Positive, 0),
            by(Side::B, P, b.libero, Negative, 1),
            by(Side::B, A, b.outside_hitter(), Exclamative, 2),
        ];
        assert_eq!(
            admitted_evals(&a, &b, &rally, Side::B, D, b.libero),
            vec![Exclamative, Negative]
        );
        let blocked = [
            rally.to_vec(),
            vec![by(Side::A, B, a.outside_hitter(), Exclamative, 3)],
        ]
        .concat();
        assert_eq!(
            admitted_evals(&a, &b, &blocked, Side::B, D, b.libero),
            vec![Exclamative, Negative]
        );
    }

    /// The block decides the touch after it (TODO/001, §4.2): after the
    /// block `!` the cover of the side that attacked answers it, `!` or
    /// `-`; after the block `+` or `-` the ball stays with the blockers,
    /// whose dig tells what the block left them, and the attackers have
    /// nothing to answer.
    #[test]
    fn the_block_decides_the_touch_after_it() {
        assert!(answers(D, B));
        assert_eq!(
            answer_evals(B, Some(Exclamative), D),
            &[Exclamative, Negative]
        );
        assert_eq!(answer_evals(B, Some(Positive), D), &[] as &[EvalEnum]);
        assert_eq!(answer_evals(B, Some(Negative), D), &[] as &[EvalEnum]);
        let (a, b) = (Team::new(), Team::new());
        let after = |block: EvalEnum, side: Side| {
            let rally = [
                by(Side::A, S, a.setter(), Positive, 0),
                by(Side::B, P, b.libero, Negative, 1),
                by(Side::B, A, b.outside_hitter(), Exclamative, 2),
                by(Side::A, B, a.outside_hitter(), block, 3),
            ];
            let player = match side {
                Side::A => a.libero,
                Side::B => b.libero,
            };
            admitted_evals(&a, &b, &rally, side, D, player)
        };
        assert_eq!(after(Exclamative, Side::B), vec![Exclamative, Negative]);
        assert_eq!(after(Exclamative, Side::A), vec![]);
        assert_eq!(after(Positive, Side::A), vec![Perfect, Positive]);
        assert_eq!(after(Positive, Side::B), vec![]);
        assert_eq!(after(Negative, Side::A), vec![Over, Negative]);
        assert_eq!(after(Negative, Side::B), vec![]);
    }

    /// The block `/` is an invasion (TODO/001, §4.2): a fault like the
    /// others, answering nothing, so the attack before it isn't saved.
    #[test]
    fn an_invasion_is_a_fault_without_the_attack() {
        let (a, b) = (Team::new(), Team::new());
        let rally = [
            by(Side::A, S, a.setter(), Positive, 0),
            by(Side::B, P, b.libero, Negative, 1),
        ];
        assert!(admitted(
            &a,
            &b,
            &rally,
            by(Side::A, B, a.outside_hitter(), Over, 2)
        ));
        let attacked = [
            rally.to_vec(),
            vec![by(Side::B, A, b.outside_hitter(), Negative, 2)],
        ]
        .concat();
        assert!(!admitted(
            &a,
            &b,
            &attacked,
            by(Side::A, B, a.outside_hitter(), Over, 3)
        ));
        // never on the half holding the ball
        assert!(!admitted(
            &a,
            &b,
            &rally,
            by(Side::B, B, b.outside_hitter(), Over, 2)
        ));
    }

    /// The block answers the other side's attack, as in DataVolley
    /// (TODO/001, §4.2): after its freeball, its reception or its dig
    /// straight over, only the invasion (`/`), a fault.
    #[test]
    fn the_block_answers_only_the_other_side_s_attack() {
        let (a, b) = (Team::new(), Team::new());
        let rallies = [
            // B's freeball: A blocks it
            (
                vec![
                    by(Side::A, S, a.setter(), Positive, 0),
                    by(Side::B, P, b.libero, Negative, 1),
                    touch(Side::B, FB, None, 2),
                ],
                Side::A,
            ),
            // B's reception straight back after A's serve `/`
            (
                vec![
                    by(Side::A, S, a.setter(), Over, 0),
                    by(Side::B, P, b.libero, Over, 1),
                ],
                Side::A,
            ),
            // A's dig straight over after B's attack `+`: B blocks it
            (
                vec![
                    by(Side::A, S, a.setter(), Positive, 0),
                    by(Side::B, P, b.libero, Negative, 1),
                    by(Side::B, A, b.outside_hitter(), Positive, 2),
                    by(Side::A, D, a.libero, Over, 3),
                ],
                Side::B,
            ),
        ];
        for (i, (rally, side)) in rallies.iter().enumerate() {
            let blocker = match side {
                Side::A => a.outside_hitter(),
                Side::B => b.outside_hitter(),
            };
            assert_eq!(
                admitted_evals(&a, &b, rally, *side, B, blocker),
                vec![Over],
                "{i}"
            );
        }
        // the other side's attack: blocked
        let attacked = [
            by(Side::A, S, a.setter(), Positive, 0),
            by(Side::B, P, b.libero, Negative, 1),
            by(Side::B, A, b.outside_hitter(), Negative, 2),
        ];
        assert_eq!(
            admitted_evals(&a, &b, &attacked, Side::A, B, a.outside_hitter()),
            vec![Positive]
        );
    }

    #[test]
    fn what_the_other_side_tells_is_never_scouted() {
        let (a, b) = (Team::new(), Team::new());
        let serve = [by(Side::A, S, a.setter(), Positive, 0)];
        for told in [OS, OE, OF] {
            assert!(
                !admitted(&a, &b, &serve, touch(Side::A, told, None, 1)),
                "{told}"
            );
        }
        assert!(admitted(&a, &b, &serve, touch(Side::A, F, None, 1)));
        assert!(admitted(&a, &b, &serve, touch(Side::B, F, None, 1)));
    }

    /// Our serve `/`, their reception came back and we let it fall: their
    /// point, nobody's, told by either side.
    #[test]
    fn the_ball_back_after_the_serve_over_falls_untouched() {
        let (a, b) = (Team::new(), Team::new());
        for told in [touch(Side::A, PL, None, 2), touch(Side::B, PW, None, 2)] {
            let replayed = replayed(
                &a,
                &b,
                &[
                    by(Side::A, S, a.setter(), Over, 0),
                    by(Side::B, P, b.libero, Over, 1),
                    told,
                ],
            );
            let scores = replayed.map(|(a, b)| {
                (
                    (a.score_us, a.score_them),
                    (b.score_us, b.score_them),
                    a.current_lineup.get_current_phase(),
                )
            });
            assert_eq!(scores, Ok(((0, 1), (1, 0), PhaseEnum::SideOut)));
        }
    }

    /// Always admitted, for either side: before the serve, with the serve
    /// waiting for its reception, the attack for its dig or block.
    #[test]
    fn the_point_without_a_cause_is_always_admitted() {
        let (a, b) = (Team::new(), Team::new());
        let moments = [
            vec![],
            vec![by(Side::A, S, a.setter(), Positive, 0)],
            vec![
                by(Side::A, S, a.setter(), Positive, 0),
                by(Side::B, P, b.libero, Negative, 1),
                by(Side::B, A, b.outside_hitter(), Positive, 2),
            ],
        ];
        for touches in moments {
            for side in [Side::A, Side::B] {
                for event in [PW, PL] {
                    assert!(
                        admitted(&a, &b, &touches, touch(side, event, None, 3)),
                        "{side:?} {event} after {} touches",
                        touches.len()
                    );
                }
            }
        }
    }
}
