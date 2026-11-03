//! The libero for a player of the scout's choice (`LF`), as the rules allow:
//! any player of the back row. When the player chosen is in the back row and
//! not serving, the libero plays in their place; otherwise in the place of
//! the middle blocker of the back row, not serving, as ever. Between two
//! libero replacements (in or out) a rally is completed: changing player,
//! the libero goes out at once and comes in for the other one after a
//! rally. The choice lasts until the next one; a middle blocker brings back
//! the default. Without `LF` (every set scouted before it) nothing changes.
//! Replayed through `Snapshot::add_event` like the scouting does.

#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::event_edit::{EventEdit, EventEditError};
    use crate::providers::store::set_reader::read_events;
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, PhaseEnum, RoleEnum, TeamSideEnum},
        lineup::Lineup,
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        stats::StatsFilter,
    };
    use crate::tests::EN;
    use EventTypeEnum::*;
    use chrono::Utc;
    use csv::WriterBuilder;
    use uuid::Uuid;

    /// The starting six in rotation 0 (the setter in position 1, the middle
    /// blocker 1 in position 6, out for the libero), the two liberos and two
    /// players on the bench.
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
        bench2: Uuid,
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
                bench2: Uuid::new_v4(),
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

        /// The players the libero can be chosen for, but the middle
        /// blockers.
        fn not_middles(&self) -> [Uuid; 4] {
            [self.setter, self.oh1, self.opposite, self.oh2]
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

    fn libero_for(player: Uuid) -> EventEntry {
        event(LF, Some(player), None)
    }

    fn substitution(replaced: Uuid, replacement: Uuid) -> EventEntry {
        EventEntry {
            target_player: Some(replacement),
            ..event(R, Some(replaced), None)
        }
    }

    /// A pseudo-random sequence, the same at every run.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self, bound: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) as usize) % bound.max(1)
        }
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
                .unwrap_or_else(|e| panic!("expected a valid event {event}: {e}"));
            self.set.events.push(event);
        }

        fn choose(&mut self, player: Uuid) {
            self.add(libero_for(player));
        }

        fn toggle(&mut self) {
            self.add(event(LR, None, None));
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

        fn lineup(&self) -> &Lineup {
            &self.snapshot.current_lineup
        }

        fn on_court(&self) -> [Uuid; 6] {
            self.lineup().get_positions()
        }

        fn base(&self) -> [Uuid; 6] {
            self.lineup().get_positions_without_libero()
        }

        fn libero_position(&self) -> Option<usize> {
            let libero = self.lineup().get_current_libero();
            self.on_court().iter().position(|id| *id == libero)
        }

        /// The player out for the libero, if on court.
        fn covered(&self) -> Option<Uuid> {
            self.libero_position()
                .and_then(|position| self.base().get(position).copied())
        }

        fn phase(&self) -> PhaseEnum {
            self.lineup().get_current_phase()
        }

        fn index_of(&self, player: Uuid) -> usize {
            self.base()
                .iter()
                .position(|id| *id == player)
                .expect("expected the player in the game")
        }

        /// The rules, told apart from the lineup: the libero never in the
        /// front row nor serving, six players, one out for it at most.
        fn assert_sound(&self, context: &str) {
            let court = self.on_court();
            let mut distinct = court.to_vec();
            distinct.sort();
            distinct.dedup();
            assert_eq!(distinct.len(), 6, "{context}: duplicates {court:?}");
            let idle = [
                self.base()
                    .iter()
                    .filter(|id| self.lineup().is_out_for_the_libero(id))
                    .count(),
                usize::from(self.libero_position().is_some()),
            ];
            assert_eq!(idle[0], idle[1], "{context}: out for the libero");
            if let Some(position) = self.libero_position() {
                assert!([0, 4, 5].contains(&position), "{context}: front row");
                assert!(
                    !(position == 0 && self.phase() == PhaseEnum::Break),
                    "{context}: the libero serving"
                );
                assert_eq!(
                    self.lineup()
                        .get_role(&self.lineup().get_current_libero())
                        .ok(),
                    Some(RoleEnum::Libero),
                    "{context}"
                );
            }
        }

        /// The set saved and read back, through the stored rows too: the
        /// same lineup.
        fn assert_replays(&self) {
            let replayed = Scout::new(self.set.clone());
            assert_eq!(replayed.on_court(), self.on_court());
            assert_eq!(replayed.base(), self.base());
            assert_eq!(
                replayed.lineup().get_libero_for(),
                self.lineup().get_libero_for()
            );
            let mut writer = WriterBuilder::new().has_headers(false).from_writer(vec![]);
            self.set
                .events
                .iter()
                .for_each(|e| writer.serialize(e).expect("expected a row"));
            let csv = writer.into_inner().expect("expected the rows");
            let read = read_events(&csv).expect("expected the events read back");
            let mut stored = self.set.clone();
            stored.events = read;
            let restored = Scout::new(stored);
            assert_eq!(restored.on_court(), self.on_court());
        }
    }

    /// Where the rules want the libero, told apart from the lineup: the
    /// player chosen in the back row and not serving; the player chosen in
    /// the front row (or none), the middle blocker of the back row, not
    /// serving; nobody while out (`LR`).
    fn wanted(scout: &Scout, chosen: Option<Uuid>, out: bool) -> Option<Uuid> {
        let base = scout.base();
        let serving = scout.phase() == PhaseEnum::Break;
        let back = |index: usize| matches!(index, 0 | 4 | 5);
        let free = |index: usize| matches!(index, 4 | 5) || (index == 0 && !serving);
        let setter = scout
            .lineup()
            .get_setter()
            .map(|s| scout.index_of(s))
            .expect("expected the setter");
        let middle = [2, 5]
            .iter()
            .map(|offset| (setter + offset) % 6)
            .find(|index| back(*index))
            .expect("expected a middle blocker in the back row");
        match (out, chosen.map(|c| scout.index_of(c))) {
            (true, _) => None,
            (false, Some(index)) if back(index) => chosen.filter(|_| free(index)),
            (false, _) => base.get(middle).copied().filter(|_| free(middle)),
        }
    }

    /// The libero as the rules want it, one replacement per dead ball: from
    /// the player covered before, out at once (a dead ball), in for the
    /// wanted one the dead ball after a rally.
    struct Expected {
        covered: Option<Uuid>,
        went_out: bool,
    }

    impl Expected {
        fn of(scout: &Scout) -> Self {
            Expected {
                covered: scout.covered(),
                went_out: false,
            }
        }

        /// A rally completed: the libero can come in again.
        fn rally(&mut self) {
            self.went_out = false;
        }

        fn dead_ball(&mut self, wanted: Option<Uuid>) {
            self.covered = match (self.covered, wanted, self.went_out) {
                (covered, wanted, _) if covered == wanted => covered,
                (Some(_), _, _) => {
                    self.went_out = true;
                    None
                }
                (None, Some(player), false) => Some(player),
                (None, _, _) => None,
            };
        }
    }

    /// Who scores the rally, by `rng`, the set never over: near the end,
    /// the team behind.
    fn we_win(scout: &Scout, rng: &mut Lcg) -> bool {
        let (us, them) = (scout.snapshot.score_us, scout.snapshot.score_them);
        match (us >= 23 && us > them, them >= 23 && them > us) {
            (true, _) => false,
            (_, true) => true,
            _ => rng.next(2) == 0,
        }
    }

    /// A rally of the set, by `rng`: a point to us or to them.
    fn rally(scout: &mut Scout, rng: &mut Lcg) {
        match we_win(scout, rng) {
            true => scout.we_score(),
            false => scout.they_score(),
        }
    }

    /// Every player the libero can be for, from every rotation, us or them
    /// serving first, chosen before the first serve: after each rally the
    /// libero is where the rules want it, never two replacements in a dead
    /// ball; each case happens; the set replays.
    #[test]
    fn the_libero_plays_where_the_rules_want_it() {
        let p = Players::new();
        let mut rng = Lcg(2026);
        let mut cases = [0usize; 4];
        for chosen in p.not_middles() {
            for rotation in 0..6 {
                for serving in [TeamSideEnum::Us, TeamSideEnum::Them] {
                    let mut scout = Scout::start(&p, rotation, serving);
                    let mut expected = Expected::of(&scout);
                    scout.choose(chosen);
                    expected.dead_ball(wanted(&scout, Some(chosen), false));
                    for n in 0..60 {
                        let context = format!(
                            "chosen {} rotation {rotation} {serving:?} rally {n}",
                            [p.setter, p.oh1, p.opposite, p.oh2]
                                .iter()
                                .position(|c| *c == chosen)
                                .unwrap_or_default()
                        );
                        assert_eq!(scout.covered(), expected.covered, "{context}");
                        scout.assert_sound(&context);
                        let before = scout.covered();
                        rally(&mut scout, &mut rng);
                        expected.rally();
                        let want = wanted(&scout, Some(chosen), false);
                        expected.dead_ball(want);
                        assert_eq!(scout.covered(), expected.covered, "{context}");
                        // the cases: the chosen one, the middle blocker,
                        // nobody (serving), waiting a rally to change
                        let case = match (scout.covered(), want, before) {
                            (Some(c), _, _) if c == chosen => 0,
                            (Some(_), _, _) => 1,
                            (None, Some(_), Some(_)) => 3,
                            (None, _, _) => 2,
                        };
                        cases[case] += 1;
                        // never from a player to another in a dead ball
                        assert!(
                            !matches!((before, scout.covered()), (Some(a), Some(b)) if a != b),
                            "{context}: two replacements in a dead ball"
                        );
                    }
                    scout.assert_replays();
                }
            }
        }
        assert!(cases.iter().all(|c| *c > 0), "{cases:?}");
    }

    /// A middle blocker chosen from the start: the same lineup as without
    /// any choice, rally by rally, with the libero out and in and the
    /// substitutions on the way.
    #[test]
    fn a_middle_blocker_chosen_is_the_default() {
        let p = Players::new();
        let mut rng = Lcg(7);
        for middle in [p.mb1, p.mb2] {
            for rotation in 0..6 {
                for serving in [TeamSideEnum::Us, TeamSideEnum::Them] {
                    let mut default = Scout::start(&p, rotation, serving);
                    let mut chosen = Scout::start(&p, rotation, serving);
                    chosen.choose(middle);
                    assert_eq!(chosen.lineup().get_libero_for(), None);
                    let mut last_toggle = 0;
                    for n in 0..80usize {
                        let context = format!("rotation {rotation} {serving:?} rally {n}");
                        assert_eq!(chosen.on_court(), default.on_court(), "{context}");
                        assert_eq!(chosen.base(), default.base(), "{context}");
                        assert_eq!(
                            chosen.lineup().get_current_rotation().ok(),
                            default.lineup().get_current_rotation().ok(),
                            "{context}"
                        );
                        for p in [p.setter, p.oh1, p.mb2, p.opposite, p.oh2, p.mb1] {
                            if chosen.on_court().contains(&p) {
                                assert_eq!(
                                    chosen.lineup().get_role(&p).ok(),
                                    default.lineup().get_role(&p).ok(),
                                    "{context}"
                                );
                            }
                        }
                        assert_eq!(
                            chosen.lineup().get_lineup_choices(),
                            default.lineup().get_lineup_choices(),
                            "{context}"
                        );
                        // the changes: the libero out or in (a rally after
                        // the last one), the outside hitter substituted and
                        // back
                        let change = match (n, rng.next(6), n - last_toggle > 1) {
                            (10, _, _) => Some(substitution(p.oh1, p.bench)),
                            (30, _, _) => Some(substitution(p.bench, p.oh1)),
                            (_, 0, true) => {
                                last_toggle = n;
                                Some(event(LR, None, None))
                            }
                            _ => None,
                        };
                        if let Some(change) = change {
                            default.add(change.clone());
                            chosen.add(change);
                        }
                        let outcome = we_win(&default, &mut rng);
                        for scout in [&mut default, &mut chosen] {
                            match outcome {
                                true => scout.we_score(),
                                false => scout.they_score(),
                            }
                        }
                    }
                    chosen.assert_replays();
                }
            }
        }
    }

    /// The player chosen in the back row, the libero off court: in their
    /// place at once, the middle blocker on court.
    #[test]
    fn chosen_in_the_back_row_the_libero_comes_in_at_once() {
        let p = Players::new();
        // rotation 1, us serving: the middle blocker 1 serves, the libero
        // off court; the outside hitter 2 in position 6
        let mut scout = Scout::start(&p, 1, TeamSideEnum::Us);
        assert_eq!(scout.covered(), None);
        assert_eq!(scout.index_of(p.oh2), 5);
        scout.choose(p.oh2);
        assert_eq!(scout.covered(), Some(p.oh2));
        assert_eq!(scout.on_court()[5], p.libero);
        assert!(scout.on_court().contains(&p.mb1));
        assert!(scout.lineup().is_out_for_the_libero(&p.oh2));
        assert_eq!(scout.lineup().get_libero_for(), Some(p.oh2));
        scout.assert_sound("chosen");
        scout.assert_replays();
    }

    /// The libero in for the middle blocker, the player chosen in the back
    /// row: out at once, in for them only after a rally.
    #[test]
    fn changing_player_the_libero_waits_a_rally() {
        let p = Players::new();
        // rotation 0, them serving: the libero for the middle blocker 1 in
        // position 6, the outside hitter 2 in position 5
        let mut scout = Scout::start(&p, 0, TeamSideEnum::Them);
        assert_eq!(scout.covered(), Some(p.mb1));
        scout.choose(p.oh2);
        assert_eq!(scout.covered(), None, "out at once");
        assert_eq!(scout.on_court()[5], p.mb1);
        // their serve error: we serve, the outside hitter 2 in 4 now
        scout.we_score();
        assert_eq!(scout.index_of(p.oh2), 3);
        // in the front row: the middle blocker again, a rally gone
        assert_eq!(scout.covered(), Some(p.mb1));
        // back to rotation 0 the long way: the choice still holds
        let mut scout = Scout::start(&p, 0, TeamSideEnum::Us);
        scout.choose(p.oh2);
        assert_eq!(scout.covered(), None);
        // our point: still serving, no rotation, a rally gone
        scout.we_score();
        assert_eq!(scout.covered(), Some(p.oh2));
        scout.assert_sound("after the rally");
        scout.assert_replays();
    }

    /// The player chosen going to the front row: the libero out at once,
    /// the player back in its place; the middle blocker of the back row
    /// replaced after a rally.
    #[test]
    fn the_player_chosen_to_the_front_row() {
        let p = Players::new();
        // rotation 1, them serving: the outside hitter 2 in position 6
        let mut scout = Scout::start(&p, 1, TeamSideEnum::Them);
        scout.choose(p.oh2);
        // the middle blocker 1 out for the libero in position 1: out, then
        // the libero for the outside hitter after a rally
        assert_eq!(scout.covered(), None);
        scout.they_score();
        assert_eq!(scout.covered(), Some(p.oh2));
        // side out: rotation 0, the outside hitter 2 in 5
        scout.we_score();
        assert_eq!(scout.covered(), Some(p.oh2));
        assert_eq!(scout.index_of(p.oh2), 4);
        // lost the serve, side out again: the outside hitter 2 to 4
        scout.they_score();
        scout.we_score();
        assert_eq!(scout.index_of(p.oh2), 3);
        assert_eq!(scout.covered(), None, "out at once");
        assert_eq!(scout.on_court()[3], p.oh2);
        // a rally: the middle blocker of the back row, the middle blocker
        // 1 in position 5
        scout.they_score();
        assert_eq!(scout.index_of(p.mb1), 4);
        assert_eq!(scout.covered(), Some(p.mb1));
        scout.assert_sound("middle blocker");
        scout.assert_replays();
    }

    /// The player chosen serving: the libero off court (neither for them
    /// nor for the middle blocker), in their place once the serve is lost.
    #[test]
    fn the_player_chosen_serves() {
        let p = Players::new();
        // rotation 1, them serving: the outside hitter 1 in position 2
        let mut scout = Scout::start(&p, 1, TeamSideEnum::Them);
        scout.choose(p.oh1);
        assert_eq!(scout.index_of(p.oh1), 2);
        assert_eq!(scout.covered(), Some(p.mb1));
        // side out: rotation 0, the outside hitter 1 in position 2 still;
        // twice more: rotation 5, then 4... until in position 1
        while scout.index_of(p.oh1) != 0 {
            scout.we_score();
            if scout.index_of(p.oh1) != 0 {
                scout.they_score();
            }
        }
        assert_eq!(scout.phase(), PhaseEnum::Break);
        assert_eq!(scout.lineup().get_serving_player(), Some(p.oh1));
        assert_eq!(scout.covered(), None, "serving");
        // points on serve: still off court
        scout.we_score();
        scout.we_score();
        assert_eq!(scout.covered(), None);
        // the serve lost: the libero in its place, in position 1
        scout.they_score();
        assert_eq!(scout.covered(), Some(p.oh1));
        assert_eq!(scout.on_court()[0], p.libero);
        scout.assert_sound("after the serve");
        scout.assert_replays();
    }

    /// A middle blocker chosen again: the default back, the libero for the
    /// middle blocker of the back row after a rally.
    #[test]
    fn a_middle_blocker_brings_back_the_default() {
        let p = Players::new();
        let mut scout = Scout::start(&p, 1, TeamSideEnum::Us);
        scout.choose(p.oh2);
        assert_eq!(scout.covered(), Some(p.oh2));
        scout.choose(p.mb2);
        assert_eq!(scout.lineup().get_libero_for(), None);
        assert_eq!(scout.covered(), None, "out at once");
        // our point: rotation 1 still, the middle blocker 1 serving
        scout.we_score();
        assert_eq!(scout.covered(), None);
        // the serve lost: the middle blocker 1 in position 1
        scout.they_score();
        assert_eq!(scout.covered(), Some(p.mb1));
        scout.assert_replays();
    }

    /// The choice lasts: back to the back row, the libero is for the same
    /// player again.
    #[test]
    fn the_choice_lasts_the_whole_set() {
        let p = Players::new();
        let mut scout = Scout::start(&p, 0, TeamSideEnum::Them);
        scout.choose(p.oh2);
        let mut seen_front = false;
        let mut back_again = false;
        for _ in 0..12 {
            scout.we_score();
            scout.they_score();
            let front = !matches!(scout.index_of(p.oh2), 0 | 4 | 5);
            seen_front |= front;
            back_again |= seen_front && scout.covered() == Some(p.oh2);
        }
        assert!(seen_front && back_again);
        assert_eq!(scout.lineup().get_libero_for(), Some(p.oh2));
        scout.assert_replays();
    }

    /// The player chosen substituted on court: the substitute is the one
    /// the libero is for.
    #[test]
    fn the_choice_follows_the_substitution_on_court() {
        let p = Players::new();
        // rotation 0, them serving: the outside hitter 1 in position 2
        let mut scout = Scout::start(&p, 0, TeamSideEnum::Them);
        scout.choose(p.oh1);
        scout.add(substitution(p.oh1, p.bench));
        assert_eq!(scout.lineup().get_libero_for(), Some(p.bench));
        scout.assert_replays();
    }

    /// The player chosen substituted while out for the libero: the libero
    /// stays, now for the substitute, who is the one chosen.
    #[test]
    fn the_choice_follows_the_substitution_out_for_the_libero() {
        let p = Players::new();
        let mut scout = Scout::start(&p, 1, TeamSideEnum::Us);
        scout.choose(p.oh2);
        assert_eq!(scout.covered(), Some(p.oh2));
        scout.add(substitution(p.oh2, p.bench));
        assert_eq!(scout.covered(), Some(p.bench));
        assert!(scout.lineup().is_out_for_the_libero(&p.bench));
        assert_eq!(scout.lineup().get_libero_for(), Some(p.bench));
        assert_eq!(scout.on_court()[5], p.libero);
        scout.assert_sound("substituted");
        scout.assert_replays();
    }

    /// The middle blocker substituted, the choice elsewhere: the libero
    /// is for the substitute when for the middle blocker.
    #[test]
    fn the_middle_blocker_substituted() {
        let p = Players::new();
        let mut scout = Scout::start(&p, 0, TeamSideEnum::Them);
        scout.choose(p.oh1);
        scout.add(substitution(p.mb1, p.bench2));
        assert_eq!(scout.covered(), Some(p.bench2));
        assert!(scout.lineup().is_out_for_the_libero(&p.bench2));
        scout.assert_replays();
    }

    /// The libero out (`LR`) once chosen: off court whoever it's for; back
    /// in, for the player chosen, after a rally if it went out in the same
    /// dead ball.
    #[test]
    fn out_and_in_once_chosen() {
        let p = Players::new();
        let mut scout = Scout::start(&p, 1, TeamSideEnum::Us);
        scout.choose(p.oh2);
        scout.toggle();
        assert!(scout.lineup().is_libero_out());
        assert_eq!(scout.covered(), None);
        // back in at once: not before a rally
        scout.toggle();
        assert!(!scout.lineup().is_libero_out());
        assert_eq!(scout.covered(), None);
        scout.we_score();
        assert_eq!(scout.covered(), Some(p.oh2));
        // out for a few rallies, then back in: at once
        scout.toggle();
        scout.we_score();
        scout.we_score();
        assert_eq!(scout.covered(), None);
        scout.toggle();
        assert_eq!(scout.covered(), Some(p.oh2));
        scout.assert_replays();
    }

    /// The other libero once chosen: in the place of the first one, for
    /// the same player; off court, off court still.
    #[test]
    fn the_other_libero_once_chosen() {
        let p = Players::new();
        let mut scout = Scout::start(&p, 1, TeamSideEnum::Us);
        scout.choose(p.oh2);
        scout.add(event(CL, None, None));
        assert_eq!(scout.lineup().get_current_libero(), p.fallback_libero);
        assert_eq!(scout.on_court()[5], p.fallback_libero);
        assert!(!scout.on_court().contains(&p.libero));
        assert_eq!(scout.covered(), Some(p.oh2));
        scout.toggle();
        scout.add(event(CL, None, None));
        assert_eq!(scout.covered(), None);
        assert!(!scout.on_court().contains(&p.libero));
        scout.assert_replays();
    }

    /// The libero for the setter, as the rules allow: the rotation still
    /// told by the setter, out for the libero.
    #[test]
    fn the_libero_for_the_setter() {
        let p = Players::new();
        // rotation 5, them serving: the setter in position 6
        let mut scout = Scout::start(&p, 5, TeamSideEnum::Them);
        scout.choose(p.setter);
        // out for the middle blocker, in for the setter after a rally
        scout.they_score();
        assert_eq!(scout.covered(), Some(p.setter));
        assert_eq!(scout.lineup().get_current_rotation().ok(), Some(5));
        assert_eq!(scout.lineup().get_setter(), Some(p.setter));
        assert!(!scout.on_court().contains(&p.setter));
        scout.assert_sound("setter");
        // side out: rotation 4, the setter in position 5, still out
        scout.we_score();
        assert_eq!(scout.lineup().get_current_rotation().ok(), Some(4));
        assert_eq!(scout.covered(), Some(p.setter));
        scout.assert_replays();
    }

    /// The roles: the libero as the libero wherever it plays, the middle
    /// blocker in the back row as a middle blocker (it attacks, it doesn't
    /// block); the choices name the player out for the libero in its role.
    #[test]
    fn the_roles_once_chosen() {
        let p = Players::new();
        let mut scout = Scout::start(&p, 1, TeamSideEnum::Us);
        scout.choose(p.oh2);
        let lineup = scout.lineup();
        assert_eq!(lineup.get_role(&p.libero).ok(), Some(RoleEnum::Libero));
        assert_eq!(lineup.get_role(&p.mb1).ok(), Some(RoleEnum::MiddleBlocker));
        assert_eq!(lineup.get_oh2(), Some(p.oh2));
        assert_eq!(lineup.get_mb1(), Some(p.mb1));
        let choices: Vec<Option<Uuid>> = lineup
            .get_lineup_choices()
            .into_iter()
            .map(|(_, (_, id))| id)
            .collect();
        assert_eq!(
            choices,
            vec![
                Some(p.setter),
                Some(p.oh1),
                Some(p.mb2),
                Some(p.opposite),
                Some(p.oh2),
                Some(p.mb1),
                Some(p.libero)
            ]
        );
        let s = &scout.snapshot;
        assert!(s.can_play(A, &p.mb1));
        assert!(!s.can_play(B, &p.mb1), "the back row doesn't block");
        assert!(!s.can_play(A, &p.oh2), "out for the libero");
        assert!(s.can_play(P, &p.libero));
        assert!(!s.can_play(A, &p.libero));
    }

    /// Before the serve only, a player in the game but the libero; named,
    /// with no evaluation.
    #[test]
    fn who_the_libero_can_be_for() {
        let p = Players::new();
        let mut scout = Scout::start(&p, 0, TeamSideEnum::Them);
        assert!(scout.options().contains(&LF));
        let s = &scout.snapshot;
        assert!(!s.can_play(LF, &p.libero));
        assert!(!s.can_play(LF, &p.bench));
        assert!(!s.can_play(LF, &p.fallback_libero));
        assert!(s.can_play(LF, &p.mb1), "out for the libero, in the game");
        for wrong in [
            event(LF, None, None),
            event(LF, Some(p.libero), None),
            event(LF, Some(p.bench), None),
            event(LF, Some(p.oh1), Some(EvalEnum::Perfect)),
        ] {
            assert!(scout.snapshot.clone().add_event(&wrong).is_err(), "{wrong}");
        }
        // within the rally, never
        scout.add(event(P, Some(p.oh1), Some(EvalEnum::Positive)));
        assert!(!scout.options().contains(&LF));
        assert!(
            scout
                .snapshot
                .clone()
                .add_event(&libero_for(p.oh2))
                .is_err()
        );
        // us serving, before the serve
        let scout = Scout::start(&p, 0, TeamSideEnum::Us);
        assert!(scout.options().contains(&LF));
    }

    /// Not a touch: no score, no serve changing hands, nothing in the
    /// statistics, not to correct; read and written as "LF".
    #[test]
    fn the_choice_is_no_touch() {
        let p = Players::new();
        let mut scout = Scout::start(&p, 0, TeamSideEnum::Us);
        scout.choose(p.oh2);
        scout.choose(p.oh1);
        let s = &scout.snapshot;
        assert_eq!((s.score_us, s.score_them), (0, 0));
        assert_eq!(s.get_serving_team(), Some(TeamSideEnum::Us));
        assert_eq!(s.stats.event_count(LF, None, &StatsFilter::default()), 0);
        assert_eq!(LF.ball_side_after(None), None);
        assert_eq!(LF.rally_winner(None), None);
        assert!(LF.requires_player() && LF.asks_player());
        assert!(!LF.requires_evaluation());
        assert!(matches!(
            EventEdit::new(&scout.set, 0, DEFAULT_MAX_SUBSTITUTIONS),
            Err(EventEditError::NotEditable)
        ));
        assert_eq!("LF".parse::<EventTypeEnum>().ok(), Some(LF));
        assert_eq!(LF.to_string(), "LF");
        assert_eq!(scout.snapshot.next_event_name(LF, EN), EN.libero_for);
        scout.assert_replays();
    }

    /// The sets without a choice: the libero for the middle blocker as
    /// ever, nobody chosen.
    #[test]
    fn without_a_choice_nothing_changes() {
        let p = Players::new();
        for rotation in 0..6 {
            let scout = Scout::start(&p, rotation, TeamSideEnum::Them);
            assert_eq!(scout.lineup().get_libero_for(), None);
            let covered = scout.covered().expect("the libero on court");
            assert!(covered == p.mb1 || covered == p.mb2);
        }
    }
}
