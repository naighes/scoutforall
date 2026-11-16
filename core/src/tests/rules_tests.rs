//! The rules of the rally for a team scouted on its own (TODO/001, §4.2):
//! where each touch leaves the ball, what can follow it, and the
//! evaluations a touch can have after the one before it. The two modes,
//! one team and both, follow the same rules.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, TeamSideEnum},
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
    };
    use EvalEnum::*;
    use EventTypeEnum::*;
    use TeamSideEnum::*;
    use chrono::Utc;
    use uuid::Uuid;

    /// Rotation 0 (setter in position 1): outside hitter 1 in 2, middle
    /// blocker 2 in 3, opposite in 4, outside hitter 2 in 5, the libero in
    /// 6 for middle blocker 1.
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

        fn oh1(&self) -> Uuid {
            self.positions[1]
        }

        fn mb2(&self) -> Uuid {
            self.positions[2]
        }

        fn set(&self, serving_team: TeamSideEnum, events: Vec<EventEntry>) -> SetEntry {
            SetEntry {
                set_number: 1,
                serving_team,
                initial_positions: self.positions,
                libero: self.libero,
                fallback_libero: None,
                setter: self.setter(),
                lineup_rules: Some(LineupRulesEnum::CURRENT),
                events,
            }
        }

        fn replay(&self, serving_team: TeamSideEnum, events: Vec<EventEntry>) -> Snapshot {
            self.set(serving_team, events)
                .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
                .expect("expected the set to replay")
        }

        /// The player playing the event in the tests: the libero digs,
        /// the middle blocker blocks, the outside hitter does the rest.
        fn player(&self, event_type: EventTypeEnum) -> Uuid {
            match event_type {
                D => self.libero,
                B => self.mb2(),
                S => self.setter(),
                _ => self.oh1(),
            }
        }

        fn act(&self, event_type: EventTypeEnum, eval: EvalEnum) -> EventEntry {
            EventEntry {
                timestamp: Utc::now(),
                event_type,
                player: Some(self.player(event_type)),
                eval: Some(eval),
                target_player: None,
                technique: None,
                flags: Default::default(),
            }
        }

        /// Our serve `+`, then the events: the ball on their side.
        fn after_our_serve(&self, events: &[(EventTypeEnum, EvalEnum)]) -> Snapshot {
            let events = std::iter::once((S, Positive))
                .chain(events.iter().copied())
                .map(|(t, e)| self.act(t, e))
                .collect();
            self.replay(Us, events)
        }

        /// Our reception `+`, then the events: the ball on our side.
        fn after_our_reception(&self, events: &[(EventTypeEnum, EvalEnum)]) -> Snapshot {
            let events = std::iter::once((P, Positive))
                .chain(events.iter().copied())
                .map(|(t, e)| self.act(t, e))
                .collect();
            self.replay(Them, events)
        }
    }

    fn options(snapshot: &Snapshot) -> Vec<EventTypeEnum> {
        snapshot.get_available_options().to_vec()
    }

    /// What follows a touch giving them the ball to attack.
    const THEIRS: [EventTypeEnum; 8] = [OS, OE, F, A, D, B, FB, OF];

    #[test]
    fn a_block_can_be_exclamative() {
        assert_eq!(
            B.available_evals(),
            vec![Perfect, Positive, Exclamative, Over, Negative, Error]
        );
    }

    /// Our block `!`: the ball goes back to them, who attacked, however they
    /// rebuild.
    #[test]
    fn after_our_block_exclamative_the_ball_goes_back_to_them() {
        let t = Team::new();
        let snapshot = t.after_our_serve(&[(B, Exclamative)]);
        assert_eq!(snapshot.get_ball_side(), Them);
        assert_eq!(options(&snapshot), THEIRS);
        assert_eq!(snapshot.score_us + snapshot.score_them, 0);
    }

    /// Our block `+` or `-`: the ball stays with us, the blockers, who
    /// counterattack (`+`) or struggle (`-`); our dig after the block is
    /// optional, never their freeball.
    #[test]
    fn after_our_block_positive_or_negative_the_ball_stays_with_us() {
        let t = Team::new();
        for eval in [Positive, Negative] {
            let snapshot = t.after_our_serve(&[(B, eval)]);
            assert_eq!(snapshot.get_ball_side(), Us, "{eval}");
            assert_eq!(options(&snapshot), vec![OE, F, A, D, FB], "{eval}");
        }
    }

    /// The dig after our block tells what the block left us: `+` or `#`
    /// after a block `+`, `-` or `/` after a block `-`.
    #[test]
    fn the_dig_after_our_block_follows_the_block() {
        let t = Team::new();
        let after = |block: EvalEnum| t.after_our_serve(&[(B, block)]).available_evals(D);
        assert_eq!(after(Positive), vec![Perfect, Positive]);
        assert_eq!(after(Negative), vec![Over, Negative]);
    }

    #[test]
    fn a_dig_the_block_does_not_admit_is_refused() {
        let t = Team::new();
        for (block, dig) in [
            (Positive, Negative),
            (Positive, Over),
            (Positive, Error),
            (Positive, Exclamative),
            (Negative, Positive),
            (Negative, Perfect),
            (Negative, Error),
            (Negative, Exclamative),
        ] {
            let events = vec![t.act(S, Positive), t.act(B, block), t.act(D, dig)];
            assert!(
                t.set(Us, events)
                    .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
                    .is_err(),
                "{block} then {dig}"
            );
        }
    }

    /// The dig after our block: then where the dig leaves the ball, as
    /// after any other.
    #[test]
    fn after_the_dig_after_our_block_as_after_a_dig() {
        let t = Team::new();
        let kept = t.after_our_serve(&[(B, Positive), (D, Positive)]);
        assert_eq!(kept.get_ball_side(), Us);
        assert_eq!(options(&kept), vec![OE, F, A, FB]);
        let over = t.after_our_serve(&[(B, Negative), (D, Over)]);
        assert_eq!(over.get_ball_side(), Them);
        assert_eq!(options(&over), THEIRS);
    }

    /// The other evaluations of the block end the rally, as before: `#`
    /// our point, `=` (out) and `/` (invasion) theirs.
    #[test]
    fn a_block_point_out_or_invasion_ends_the_rally() {
        let t = Team::new();
        for (eval, score) in [(Perfect, (1, 0)), (Error, (0, 1)), (Over, (0, 1))] {
            let snapshot = t.after_our_serve(&[(B, eval)]);
            assert_eq!((snapshot.score_us, snapshot.score_them), score, "{eval}");
        }
    }

    /// The block after our reception: the ball on our side, theirs the
    /// block `!` sends back.
    #[test]
    fn the_block_after_their_attack_on_our_reception() {
        let t = Team::new();
        // our attack defended, their attack, our block
        let snapshot = t.after_our_reception(&[(A, Negative), (B, Exclamative)]);
        assert_eq!(snapshot.get_ball_side(), Them);
        assert_eq!(options(&snapshot), THEIRS);
    }

    /// Our attack `+`: defended with difficulty, they don't counterattack.
    /// Open roads: the ball can be theirs (their dig `-`, then they attack
    /// or send a freeball) or back to us (their dig `/`), the touch after
    /// tells: the same choices as after our attack `-`.
    #[test]
    fn after_our_attack_positive_the_roads_are_open() {
        let t = Team::new();
        let snapshot = t.after_our_reception(&[(A, Positive)]);
        assert_eq!(snapshot.get_ball_side(), Them);
        assert_eq!(options(&snapshot), THEIRS);
        // back to us: we play again
        let again = t.after_our_reception(&[(A, Positive), (A, Perfect)]);
        assert_eq!(again.score_us, 1);
        // theirs: our dig of their attack
        let dug = t.after_our_reception(&[(A, Positive), (D, Positive)]);
        assert_eq!(dug.get_ball_side(), Us);
    }

    /// Our attack `!`, blocked and covered: our cover after it is optional,
    /// `!` (we rebuild) or `-` (we struggle).
    #[test]
    fn after_our_attack_exclamative_our_cover_is_optional() {
        let t = Team::new();
        let snapshot = t.after_our_reception(&[(A, Exclamative)]);
        assert_eq!(snapshot.get_ball_side(), Us);
        assert_eq!(options(&snapshot), vec![OE, F, A, D, FB]);
        assert_eq!(snapshot.available_evals(D), vec![Exclamative, Negative]);
        for cover in [Exclamative, Negative] {
            let covered = t.after_our_reception(&[(A, Exclamative), (D, cover)]);
            assert_eq!(covered.get_ball_side(), Us, "{cover}");
            assert_eq!(options(&covered), vec![OE, F, A, FB], "{cover}");
        }
        for refused in [Perfect, Positive, Over, Error] {
            let events = vec![t.act(P, Positive), t.act(A, Exclamative), t.act(D, refused)];
            assert!(
                t.set(Them, events)
                    .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
                    .is_err(),
                "{refused}"
            );
        }
    }

    /// The dig `!` is only a cover: never the dig of their attack, nor the
    /// one after our block.
    #[test]
    fn a_dig_exclamative_is_only_a_cover() {
        let t = Team::new();
        let dig = t.after_our_serve(&[]).available_evals(D);
        assert_eq!(dig, vec![Perfect, Positive, Over, Negative, Error]);
        let events = vec![t.act(S, Positive), t.act(D, Exclamative)];
        assert!(
            t.set(Us, events)
                .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
                .is_err()
        );
    }

    // --- the translation of older rules (`domain::rules`) ---------------

    use crate::domain::{Side, rules::translate};
    use crate::shapes::enums::ScoutingRulesEnum;

    fn ev(event_type: EventTypeEnum, eval: Option<EvalEnum>) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type,
            player: None,
            eval,
            target_player: None,
            technique: None,
            flags: Default::default(),
        }
    }

    fn told(
        rules: Option<ScoutingRulesEnum>,
        touches: &[(Side, EventTypeEnum, Option<EvalEnum>)],
    ) -> Vec<(Side, EventTypeEnum, Option<EvalEnum>)> {
        let touches: Vec<(Side, EventEntry)> =
            touches.iter().map(|(s, t, e)| (*s, ev(*t, *e))).collect();
        translate(rules, &touches)
            .into_iter()
            .map(|(s, e)| (s, e.event_type, e.eval))
            .collect()
    }

    /// v1: the block `-` let the ball go back to them, `!` now; the block
    /// `+` before their freeball too, also with a change of the lineup
    /// between them; the rest as saved.
    #[test]
    fn the_blocks_of_v1_are_told_with_the_current_rules() {
        use Side::A as Us_;
        let v1 = [
            (Us_, B, Some(Negative)),
            (Us_, D, Some(Positive)),
            (Us_, B, Some(Positive)),
            (Us_, OF, None),
            (Us_, B, Some(Positive)),
            (Us_, R, None),
            (Us_, OF, None),
            (Us_, B, Some(Positive)),
            (Us_, A, Some(Perfect)),
            (Us_, B, Some(Perfect)),
            (Us_, B, Some(Error)),
            (Us_, B, Some(Over)),
            (Us_, A, Some(Negative)),
            (Us_, D, Some(Negative)),
        ];
        let expected = [
            (Us_, B, Some(Exclamative)),
            (Us_, D, Some(Positive)),
            (Us_, B, Some(Exclamative)),
            (Us_, OF, None),
            (Us_, B, Some(Exclamative)),
            (Us_, R, None),
            (Us_, OF, None),
            (Us_, B, Some(Positive)),
            (Us_, A, Some(Perfect)),
            (Us_, B, Some(Perfect)),
            (Us_, B, Some(Error)),
            (Us_, B, Some(Over)),
            (Us_, A, Some(Negative)),
            (Us_, D, Some(Negative)),
        ];
        assert_eq!(told(Some(ScoutingRulesEnum::V1), &v1), expected);
        // saved without rules: v1
        assert_eq!(told(None, &v1), expected);
    }

    /// The current rules: as saved.
    #[test]
    fn the_current_rules_are_told_as_saved() {
        let saved = [
            (Side::A, B, Some(Negative)),
            (Side::A, D, Some(Negative)),
            (Side::A, B, Some(Positive)),
            (Side::A, OF, None),
        ];
        assert_eq!(told(Some(ScoutingRulesEnum::V3), &saved), saved);
        assert_eq!(ScoutingRulesEnum::CURRENT, ScoutingRulesEnum::V3);
    }

    /// v2: the dig `!` was a dig of their attack with few options left,
    /// `+` now (`!` is only a cover); the attack `+` (we played again) as
    /// saved, one of the roads it opens now. From v1 both translations.
    #[test]
    fn the_digs_of_v2_are_told_with_the_current_rules() {
        let v2 = [
            (Side::A, D, Some(Exclamative)),
            (Side::B, D, Some(Exclamative)),
            (Side::A, A, Some(Positive)),
            (Side::A, A, Some(Exclamative)),
            (Side::A, P, Some(Exclamative)),
            (Side::A, D, Some(Negative)),
        ];
        let expected = [
            (Side::A, D, Some(Positive)),
            (Side::B, D, Some(Positive)),
            (Side::A, A, Some(Positive)),
            (Side::A, A, Some(Exclamative)),
            (Side::A, P, Some(Exclamative)),
            (Side::A, D, Some(Negative)),
        ];
        assert_eq!(told(Some(ScoutingRulesEnum::V2), &v2), expected);
        let v1 = [
            (Side::A, B, Some(Negative)),
            (Side::A, D, Some(Exclamative)),
        ];
        assert_eq!(
            told(None, &v1),
            [
                (Side::A, B, Some(Exclamative)),
                (Side::A, D, Some(Positive))
            ]
        );
    }

    /// A set of v2 with the dig `!` replays with the current rules.
    #[test]
    fn a_set_of_v2_replays_with_the_current_rules() {
        let t = Team::new();
        let saved = [
            t.act(S, Positive),
            t.act(D, Exclamative),
            t.act(A, Positive),
            t.act(A, Perfect),
        ];
        let touches: Vec<(Side, EventEntry)> = saved.into_iter().map(|e| (Side::A, e)).collect();
        let events = translate(Some(ScoutingRulesEnum::V2), &touches)
            .into_iter()
            .map(|(_, e)| e)
            .collect();
        let snapshot = t.replay(Us, events);
        assert_eq!((snapshot.score_us, snapshot.score_them), (1, 0));
    }

    /// Both sides: the freeball after the block is the blocking side's.
    #[test]
    fn the_freeball_after_the_block_is_of_the_side_of_the_block() {
        let saved = [
            (Side::A, B, Some(Positive)),
            (Side::B, OF, None),
            (Side::A, OF, None),
        ];
        assert_eq!(
            told(Some(ScoutingRulesEnum::V1), &saved),
            [
                (Side::A, B, Some(Exclamative)),
                (Side::B, OF, None),
                (Side::A, OF, None),
            ]
        );
        let other = [(Side::A, B, Some(Positive)), (Side::B, OF, None)];
        assert_eq!(told(Some(ScoutingRulesEnum::V1), &other), other);
    }
}
