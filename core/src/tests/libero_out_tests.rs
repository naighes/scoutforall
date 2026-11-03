//! The libero out by the scout's choice (`LR`): the middle blocker stays on
//! court in the back row until the libero comes back in, and neither change
//! happens by itself in between. Replayed through `Snapshot::add_event` like
//! the scouting screen does.

#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::event_edit::{EventEdit, EventEditError};
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, PhaseEnum, RoleEnum, TeamSideEnum},
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        stats::StatsFilter,
    };
    use crate::tests::EN;
    use EventTypeEnum::*;
    use chrono::Utc;
    use uuid::Uuid;

    /// The starting six in rotation 0 (the setter in position 1), the two
    /// liberos and a player on the bench.
    struct Players {
        setter: Uuid,
        oh1: Uuid,
        mb2: Uuid,
        opposite: Uuid,
        oh2: Uuid,
        mb1: Uuid,
        libero: Uuid,
        fallback_libero: Uuid,
        bench: Uuid,
    }

    impl Players {
        fn new() -> Self {
            Players {
                setter: Uuid::new_v4(),
                oh1: Uuid::new_v4(),
                mb2: Uuid::new_v4(),
                opposite: Uuid::new_v4(),
                oh2: Uuid::new_v4(),
                mb1: Uuid::new_v4(),
                libero: Uuid::new_v4(),
                fallback_libero: Uuid::new_v4(),
                bench: Uuid::new_v4(),
            }
        }

        /// The setter in position `rotation + 1`.
        fn set(&self, rotation: usize, serving_team: TeamSideEnum) -> SetEntry {
            let mut positions = [
                self.setter,
                self.oh1,
                self.mb2,
                self.opposite,
                self.oh2,
                self.mb1,
            ];
            positions.rotate_right(rotation);
            SetEntry::new(
                1,
                serving_team,
                positions,
                self.libero,
                Some(self.fallback_libero),
                self.setter,
            )
            .expect("expected a valid set")
        }
    }

    fn event(
        event_type: EventTypeEnum,
        player: Option<Uuid>,
        eval: Option<EvalEnum>,
    ) -> EventEntry {
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

    fn libero_replacement() -> EventEntry {
        event(LR, None, None)
    }

    struct Scout {
        set: SetEntry,
        snapshot: Snapshot,
    }

    impl Scout {
        fn new(set: SetEntry) -> Self {
            let snapshot = set
                .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
                .expect("expected a snapshot");
            Scout { set, snapshot }
        }

        fn start(p: &Players, rotation: usize, serving: TeamSideEnum) -> Self {
            Scout::new(p.set(rotation, serving))
        }

        fn options(&self) -> Vec<EventTypeEnum> {
            self.snapshot.get_available_options().to_vec()
        }

        fn add(&mut self, event: EventEntry) {
            assert!(
                self.options().contains(&event.event_type),
                "{:?} not available among {:?}",
                event.event_type,
                self.options()
            );
            self.snapshot
                .add_event(&event)
                .expect("expected a valid event");
            self.set.events.push(event);
        }

        /// The libero out, or back in.
        fn toggle(&mut self) {
            self.add(libero_replacement());
        }

        fn we_score(&mut self) {
            self.add(event(OE, None, None));
        }

        /// They score: a serve error when we serve, their point otherwise.
        fn they_score(&mut self) {
            match self.phase() {
                PhaseEnum::Break => {
                    let server = self.lineup().get_serving_player();
                    self.add(event(S, server, Some(EvalEnum::Error)));
                }
                PhaseEnum::SideOut => self.add(event(OS, None, None)),
            }
        }

        fn lineup(&self) -> &crate::shapes::lineup::Lineup {
            &self.snapshot.current_lineup
        }

        fn on_court(&self) -> Vec<Uuid> {
            self.lineup().get_positions().to_vec()
        }

        fn libero_on_court(&self) -> bool {
            let libero = self.lineup().get_current_libero();
            self.on_court().contains(&libero)
        }

        fn rotation(&self) -> u8 {
            self.lineup()
                .get_current_rotation()
                .expect("expected a rotation")
        }

        fn phase(&self) -> PhaseEnum {
            self.lineup().get_current_phase()
        }

        /// A middle blocker serves: the libero can't be on court.
        fn middle_serves(&self) -> bool {
            self.phase() == PhaseEnum::Break && matches!(self.rotation(), 1 | 4)
        }

        /// The libero out: both middle blockers on court, nobody out for the
        /// libero. In: the libero rules, as ever (back row, never serving,
        /// for one middle blocker).
        fn assert_lineup(&self, p: &Players, out: bool, context: &str) {
            let court = self.on_court();
            let mut distinct = court.clone();
            distinct.sort();
            distinct.dedup();
            assert_eq!(distinct.len(), 6, "{context}: duplicate players {court:?}");
            assert_eq!(self.lineup().is_libero_out(), out, "{context}");
            let middles = [p.mb1, p.mb2].iter().filter(|m| court.contains(m)).count();
            let out_for_the_libero = [p.mb1, p.mb2]
                .into_iter()
                .filter(|m| self.lineup().is_out_for_the_libero(m))
                .count();
            match (out, self.libero_on_court()) {
                (true, true) => panic!("{context}: the libero out, but on court"),
                (true, false) | (false, false) => {
                    assert_eq!(middles, 2, "{context}: a middle blocker off court");
                    assert_eq!(out_for_the_libero, 0, "{context}");
                    assert!(
                        out || self.middle_serves(),
                        "{context}: the libero off court"
                    );
                }
                (false, true) => {
                    let position = court
                        .iter()
                        .position(|id| *id == p.libero)
                        .expect("the libero");
                    assert!([0, 4, 5].contains(&position), "{context}: front row");
                    assert!(!self.middle_serves(), "{context}: the libero serving");
                    assert_eq!(middles, 1, "{context}: {middles} middle blockers");
                    assert_eq!(out_for_the_libero, 1, "{context}");
                }
            }
        }

        /// The set saved and read back: the same lineup, the libero out or
        /// in as it was.
        fn assert_replays(&self) {
            let replayed = Scout::new(self.set.clone());
            assert_eq!(replayed.on_court(), self.on_court());
            assert_eq!(
                replayed.lineup().is_libero_out(),
                self.lineup().is_libero_out()
            );
            assert_eq!(
                replayed.lineup().get_positions_without_libero(),
                self.lineup().get_positions_without_libero()
            );
        }
    }

    /// The rally: a side out, a point on serve, then the serve lost.
    fn play(scout: &mut Scout, rally: usize) {
        match rally % 3 {
            0 | 1 => scout.we_score(),
            _ => scout.they_score(),
        }
    }

    /// Before any serve, in break and in side out: never within the rally.
    #[test]
    fn the_libero_goes_out_or_in_before_the_serve() {
        let p = Players::new();
        for serving in [TeamSideEnum::Us, TeamSideEnum::Them] {
            let mut scout = Scout::start(&p, 0, serving);
            for _ in 0..3 {
                assert!(scout.options().contains(&LR), "{serving:?}");
                scout.toggle();
                assert!(scout.options().contains(&LR), "{serving:?}");
            }
        }
        let mut scout = Scout::start(&p, 0, TeamSideEnum::Us);
        scout.add(event(S, Some(p.setter), Some(EvalEnum::Positive)));
        assert!(!scout.options().contains(&LR));
        let refused = scout.snapshot.add_event(&libero_replacement());
        assert!(refused.is_err());
        // with a player or an evaluation, never
        let mut scout = Scout::start(&p, 0, TeamSideEnum::Us);
        for wrong in [
            event(LR, Some(p.libero), None),
            event(LR, None, Some(EvalEnum::Perfect)),
        ] {
            assert!(scout.snapshot.add_event(&wrong).is_err(), "{wrong}");
            assert!(!scout.lineup().is_libero_out());
        }
    }

    /// Out: the middle blocker it replaced back in its place, then a middle
    /// blocker in the back row, playing as one: receiving, digging,
    /// attacking from behind, but never blocking. The libero plays nothing.
    #[test]
    fn out_the_middle_blocker_plays_in_the_back_row() {
        let p = Players::new();
        // the setter in position 1: middle blocker 1 in 6, for the libero
        let mut scout = Scout::start(&p, 0, TeamSideEnum::Them);
        assert_eq!(scout.on_court()[5], p.libero);
        scout.toggle();
        assert_eq!(scout.on_court()[5], p.mb1);
        assert!(!scout.libero_on_court());
        let lineup = scout.lineup();
        assert_eq!(lineup.get_role(&p.mb1).ok(), Some(RoleEnum::MiddleBlocker));
        assert_eq!(lineup.get_mb1(), Some(p.mb1));
        assert_eq!(lineup.get_mb2(), Some(p.mb2));
        assert!(!lineup.is_out_for_the_libero(&p.mb1));
        for event_type in [P, D, A, F] {
            assert!(scout.snapshot.can_play(event_type, &p.mb1), "{event_type}");
            assert!(
                !scout.snapshot.can_play(event_type, &p.libero),
                "{event_type}"
            );
        }
        assert!(!scout.snapshot.can_play(B, &p.mb1));
        // their serve received by the middle blocker, who attacks
        scout.add(event(P, Some(p.mb1), Some(EvalEnum::Positive)));
        scout.add(event(A, Some(p.mb1), Some(EvalEnum::Perfect)));
        assert_eq!((scout.snapshot.score_us, scout.snapshot.score_them), (1, 0));
        assert_eq!(
            scout
                .snapshot
                .stats
                .event_count(P, None, &StatsFilter::new(None, None, Some(p.mb1))),
            1
        );
    }

    /// Out, the libero never comes back by itself, nor the middle blocker
    /// goes out: two turns of rotations from every starting rotation.
    #[test]
    fn out_the_libero_stays_out() {
        for rotation in 0..6 {
            for serving in [TeamSideEnum::Us, TeamSideEnum::Them] {
                let p = Players::new();
                let mut scout = Scout::start(&p, rotation, serving);
                scout.toggle();
                let context = format!("rotation {}, {serving:?} serving", rotation + 1);
                scout.assert_lineup(&p, true, &context);
                for rally in 0..24 {
                    play(&mut scout, rally);
                    scout.assert_lineup(&p, true, &format!("{context}, rally {rally}"));
                }
                scout.assert_replays();
            }
        }
    }

    /// Back in: at once for the middle blocker in the back row, unless they
    /// serve; then the libero rules as ever, for two turns of rotations.
    #[test]
    fn back_in_the_libero_plays_as_ever() {
        for rotation in 0..6 {
            for serving in [TeamSideEnum::Us, TeamSideEnum::Them] {
                for rallies_out in 0..6 {
                    let p = Players::new();
                    let mut scout = Scout::start(&p, rotation, serving);
                    let context = format!(
                        "rotation {}, {serving:?} serving, {rallies_out} rallies out",
                        rotation + 1
                    );
                    scout.toggle();
                    (0..rallies_out).for_each(|rally| play(&mut scout, rally));
                    let court = scout.on_court();
                    scout.toggle();
                    scout.assert_lineup(&p, false, &context);
                    match scout.middle_serves() {
                        // nothing changes until they lose the serve
                        true => assert_eq!(scout.on_court(), court, "{context}"),
                        false => assert!(scout.libero_on_court(), "{context}"),
                    }
                    for rally in 0..24 {
                        play(&mut scout, rally);
                        scout.assert_lineup(&p, false, &format!("{context}, rally {rally}"));
                    }
                    scout.assert_replays();
                }
            }
        }
    }

    /// Back in while the middle blocker serves: the libero goes in for
    /// them once they lose the serve.
    #[test]
    fn back_in_while_the_middle_blocker_serves() {
        let p = Players::new();
        // the setter in position 2: middle blocker 1 serves
        let mut scout = Scout::start(&p, 1, TeamSideEnum::Us);
        assert_eq!(scout.on_court()[0], p.mb1);
        scout.toggle();
        scout.toggle();
        assert_eq!(scout.on_court()[0], p.mb1);
        scout.we_score();
        assert_eq!(scout.on_court()[0], p.mb1);
        scout.they_score();
        assert_eq!(scout.on_court()[0], p.libero);
        assert!(scout.lineup().is_out_for_the_libero(&p.mb1));
    }

    /// Out while the middle blocker serves: once they lose the serve, they
    /// stay on court.
    #[test]
    fn out_while_the_middle_blocker_serves() {
        let p = Players::new();
        let mut scout = Scout::start(&p, 1, TeamSideEnum::Us);
        scout.toggle();
        assert_eq!(scout.on_court()[0], p.mb1);
        scout.they_score();
        assert_eq!(scout.on_court()[0], p.mb1);
        assert!(!scout.libero_on_court());
        scout.assert_lineup(&p, true, "the serve lost");
    }

    /// In and out at random before every rally, from every starting
    /// rotation: the lineup always as the choice says, and the same once
    /// read back.
    #[test]
    fn in_and_out_at_any_time() {
        // a small linear congruential generator: the same sequence each run
        let mut seed: u32 = 7;
        let mut next = move || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            (seed >> 16) % 4
        };
        for rotation in 0..6 {
            for serving in [TeamSideEnum::Us, TeamSideEnum::Them] {
                let p = Players::new();
                let mut scout = Scout::start(&p, rotation, serving);
                let mut out = false;
                for rally in 0..36 {
                    let context = format!(
                        "rotation {}, {serving:?} serving, rally {rally}",
                        rotation + 1
                    );
                    // the libero out or back in, once or (back and forth)
                    // twice
                    for _ in 0..next().saturating_sub(1) {
                        scout.toggle();
                        out = !out;
                        scout.assert_lineup(&p, out, &context);
                    }
                    play(&mut scout, rally);
                    scout.assert_lineup(&p, out, &context);
                }
                scout.assert_replays();
            }
        }
    }

    /// A middle blocker on court instead of the libero is substituted as
    /// any player on court; back in, the libero goes in for the
    /// substitute.
    #[test]
    fn out_the_middle_blocker_is_substituted_on_court() {
        let p = Players::new();
        let mut scout = Scout::start(&p, 0, TeamSideEnum::Them);
        scout.toggle();
        scout.add(event(R, Some(p.mb1), None).with_target(p.bench));
        assert_eq!(scout.on_court()[5], p.bench);
        assert_eq!(scout.lineup().get_substitutions().len(), 1);
        scout.toggle();
        assert_eq!(scout.on_court()[5], p.libero);
        assert!(scout.lineup().is_out_for_the_libero(&p.bench));
        scout.assert_replays();
    }

    /// Changing libero while out: the other libero doesn't come in until
    /// back in.
    #[test]
    fn the_libero_changes_while_out() {
        let p = Players::new();
        let mut scout = Scout::start(&p, 0, TeamSideEnum::Them);
        scout.toggle();
        scout.add(event(CL, None, None));
        assert_eq!(scout.lineup().get_current_libero(), p.fallback_libero);
        assert!(!scout.libero_on_court());
        assert_eq!(scout.on_court()[5], p.mb1);
        scout.toggle();
        assert_eq!(scout.on_court()[5], p.fallback_libero);
        scout.assert_replays();
    }

    /// Not a touch: no score, no serve changing hands, nothing in the
    /// statistics; and not to correct, as the other changes of the lineup.
    #[test]
    fn the_libero_out_is_no_touch() {
        let p = Players::new();
        let mut scout = Scout::start(&p, 0, TeamSideEnum::Us);
        scout.toggle();
        scout.toggle();
        scout.toggle();
        let s = &scout.snapshot;
        assert_eq!((s.score_us, s.score_them), (0, 0));
        assert_eq!(s.get_serving_team(), Some(TeamSideEnum::Us));
        assert_eq!(s.stats.event_count(LR, None, &StatsFilter::default()), 0);
        assert_eq!(LR.ball_side_after(None), None);
        assert_eq!(LR.rally_winner(None), None);
        assert!(!LR.asks_player() && !LR.accepts_player());
        assert!(!LR.requires_evaluation());
        assert!(matches!(
            EventEdit::new(&scout.set, 0, DEFAULT_MAX_SUBSTITUTIONS),
            Err(EventEditError::NotEditable)
        ));
        assert_eq!("LR".parse::<EventTypeEnum>().ok(), Some(LR));
        assert_eq!(LR.to_string(), "LR");
    }

    /// To choose it, the libero replacement says what it does: out while
    /// the libero plays (or would), in while out.
    #[test]
    fn the_libero_replacement_says_what_it_does() {
        let p = Players::new();
        let mut scout = Scout::start(&p, 0, TeamSideEnum::Us);
        let name = |scout: &Scout| scout.snapshot.next_event_name(LR, EN);
        assert_eq!(name(&scout), EN.libero_out);
        scout.toggle();
        assert_eq!(name(&scout), EN.libero_in);
        scout.toggle();
        assert_eq!(name(&scout), EN.libero_out);
        // the middle blocker serving: still out, to choose
        let scout = Scout::start(&p, 1, TeamSideEnum::Us);
        assert!(!scout.libero_on_court());
        assert_eq!(name(&scout), EN.libero_out);
        assert_eq!(scout.snapshot.next_event_name(CL, EN), EN.change_libero);
    }

    trait WithTarget {
        fn with_target(self, target: Uuid) -> Self;
    }

    impl WithTarget for EventEntry {
        fn with_target(self, target: Uuid) -> Self {
            EventEntry {
                target_player: Some(target),
                ..self
            }
        }
    }
}
