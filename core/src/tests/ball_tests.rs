//! Where the ball is while a rally is played: the last touch, and the side
//! of the net it leaves the ball on.
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

        fn replay(&self, serving_team: TeamSideEnum, events: Vec<EventEntry>) -> Snapshot {
            SetEntry {
                set_number: 1,
                serving_team,
                initial_positions: [
                    self.setter,
                    self.oh1,
                    self.mb2,
                    self.opposite,
                    self.oh2,
                    self.mb1,
                ],
                libero: self.libero,
                fallback_libero: None,
                setter: self.setter,
                lineup_rules: Some(LineupRulesEnum::CURRENT),
                events,
            }
            .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
            .expect("expected the set to replay")
        }
    }

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

    fn act(event_type: EventTypeEnum, player: Uuid, eval: EvalEnum) -> EventEntry {
        ev(event_type, Some(player), Some(eval))
    }

    /// The side of the ball, and the last touch: event, player, evaluation.
    fn ball(
        snapshot: &Snapshot,
    ) -> (
        TeamSideEnum,
        Option<(EventTypeEnum, Option<Uuid>, Option<EvalEnum>)>,
    ) {
        (
            snapshot.get_ball_side(),
            snapshot
                .get_last_touch()
                .map(|(e, _)| (e.event_type, e.player, e.eval)),
        )
    }

    #[test]
    fn before_the_serve_the_ball_is_with_the_team_serving() {
        let t = Team::new();
        assert_eq!(ball(&t.replay(Us, vec![])), (Us, None));
        assert_eq!(ball(&t.replay(Them, vec![])), (Them, None));
    }

    #[test]
    fn after_a_point_the_ball_is_with_the_team_serving_next() {
        let t = Team::new();
        let events = vec![act(P, t.oh1, Positive), act(A, t.oh1, Perfect)];
        assert_eq!(ball(&t.replay(Them, events)), (Us, None));
        let events = vec![act(S, t.setter, Error)];
        assert_eq!(ball(&t.replay(Us, events)), (Them, None));
    }

    #[test]
    fn our_serve_goes_over_unless_it_comes_straight_back() {
        let t = Team::new();
        assert_eq!(
            ball(&t.replay(Us, vec![act(S, t.setter, Positive)])),
            (Them, Some((S, Some(t.setter), Some(Positive))))
        );
        assert_eq!(
            ball(&t.replay(Us, vec![act(S, t.setter, Over)])),
            (Us, Some((S, Some(t.setter), Some(Over))))
        );
    }

    #[test]
    fn a_reception_or_a_dig_keeps_the_ball_unless_it_goes_straight_over() {
        let t = Team::new();
        for eval in [Perfect, Positive, Exclamative, Negative] {
            assert_eq!(
                ball(&t.replay(Them, vec![act(P, t.oh1, eval)])),
                (Us, Some((P, Some(t.oh1), Some(eval))))
            );
            let events = vec![act(S, t.setter, Positive), act(D, t.libero, eval)];
            assert_eq!(
                ball(&t.replay(Us, events)),
                (Us, Some((D, Some(t.libero), Some(eval))))
            );
        }
        assert_eq!(
            ball(&t.replay(Them, vec![act(P, t.oh1, Over)])),
            (Them, Some((P, Some(t.oh1), Some(Over))))
        );
    }

    #[test]
    fn an_attack_comes_back_unless_defended() {
        let t = Team::new();
        for (eval, side) in [(Positive, Us), (Exclamative, Us), (Negative, Them)] {
            let events = vec![act(P, t.oh1, Positive), act(A, t.oh1, eval)];
            assert_eq!(
                ball(&t.replay(Them, events)),
                (side, Some((A, Some(t.oh1), Some(eval))))
            );
        }
    }

    #[test]
    fn a_block_keeps_the_ball_when_positive() {
        let t = Team::new();
        for (eval, side) in [(Positive, Us), (Negative, Them)] {
            let events = vec![act(S, t.setter, Positive), act(B, t.mb2, eval)];
            assert_eq!(
                ball(&t.replay(Us, events)),
                (side, Some((B, Some(t.mb2), Some(eval))))
            );
        }
    }

    #[test]
    fn a_freeball_crosses_the_net() {
        let t = Team::new();
        let events = vec![act(P, t.oh1, Positive), ev(FB, Some(t.oh1), None)];
        assert_eq!(
            ball(&t.replay(Them, events)),
            (Them, Some((FB, Some(t.oh1), None)))
        );
        let events = vec![act(S, t.setter, Positive), ev(OF, None, None)];
        assert_eq!(ball(&t.replay(Us, events)), (Us, Some((OF, None, None))));
    }
}
