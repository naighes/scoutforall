//! The point without a cause, ours (`PW`) or theirs (`PL`): nobody's,
//! always possible, as DataVolley's green code `$$&` (TODO/001, §4.2). It
//! counts in the score and in its own count, nowhere else.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::shapes::{
        enums::{ErrorTypeEnum, EvalEnum, EventTypeEnum, LineupRulesEnum, PhaseEnum, TeamSideEnum},
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        stats::StatsFilter,
    };
    use EvalEnum::*;
    use EventTypeEnum::*;
    use chrono::Utc;
    use uuid::Uuid;

    /// The setter in position 1, outside hitter in 2.
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

        fn replay(&self, serving_team: TeamSideEnum, events: Vec<EventEntry>) -> Snapshot {
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
            .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
            .expect("expected the set to replay")
        }

        fn serve(&self, eval: EvalEnum) -> EventEntry {
            ev(S, Some(self.setter()), Some(eval))
        }

        fn attack(&self, eval: EvalEnum) -> EventEntry {
            ev(A, Some(self.outside_hitter()), Some(eval))
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
            flags: Default::default(),
        }
    }

    fn team(event_type: EventTypeEnum) -> EventEntry {
        ev(event_type, None, None)
    }

    #[test]
    fn it_is_offered_in_every_moment_of_the_set() {
        let t = Team::new();
        let moments = [
            (TeamSideEnum::Us, vec![]),
            (TeamSideEnum::Them, vec![]),
            (TeamSideEnum::Us, vec![t.serve(Positive)]),
            // their reception came back
            (TeamSideEnum::Us, vec![t.serve(Over)]),
            (TeamSideEnum::Us, vec![t.serve(Positive), team(OF)]),
            (TeamSideEnum::Us, vec![t.serve(Over), team(FB)]),
            (TeamSideEnum::Us, vec![t.serve(Over), t.attack(Positive)]),
            (TeamSideEnum::Us, vec![t.serve(Over), t.attack(Negative)]),
            // our cover after the block
            (TeamSideEnum::Us, vec![t.serve(Over), t.attack(Exclamative)]),
        ];
        for (serving, events) in moments {
            let snapshot = t.replay(serving, events.clone());
            let options = snapshot.get_available_options();
            let kinds: Vec<_> = events.iter().map(|e| (e.event_type, e.eval)).collect();
            assert!(
                options.contains(&PW) && options.contains(&PL),
                "{serving:?} {kinds:?}: {options:?}"
            );
        }
    }

    #[test]
    fn ours_wins_the_serve_and_theirs_loses_it() {
        let t = Team::new();
        let won = t.replay(TeamSideEnum::Them, vec![team(PW)]);
        assert_eq!((won.score_us, won.score_them), (1, 0));
        assert_eq!(won.current_lineup.get_current_phase(), PhaseEnum::Break);
        // the side-out rotates, as on their error
        let rotation = |s: &Snapshot| s.current_lineup.get_current_rotation().ok();
        let on_their_error = t.replay(TeamSideEnum::Them, vec![team(OE)]);
        let start = t.replay(TeamSideEnum::Them, vec![]);
        assert_eq!(rotation(&won), rotation(&on_their_error));
        assert_ne!(rotation(&won), rotation(&start));
        let lost = t.replay(TeamSideEnum::Us, vec![t.serve(Positive), team(PL)]);
        assert_eq!((lost.score_us, lost.score_them), (0, 1));
        assert_eq!(lost.current_lineup.get_current_phase(), PhaseEnum::SideOut);
        assert_eq!(lost.points, vec![TeamSideEnum::Them]);
    }

    /// Our serve `/`, their reception came back and we let it fall: their
    /// point, nobody's fault.
    #[test]
    fn the_ball_back_after_the_serve_over_falls_untouched() {
        let t = Team::new();
        let snapshot = t.replay(TeamSideEnum::Us, vec![t.serve(Over), team(PL)]);
        assert_eq!((snapshot.score_us, snapshot.score_them), (0, 1));
    }

    #[test]
    fn only_the_score_and_its_own_count() {
        let t = Team::new();
        let snapshot = t.replay(
            TeamSideEnum::Us,
            vec![t.serve(Positive), team(PW), t.serve(Over), team(PL)],
        );
        let stats = &snapshot.stats;
        let all = StatsFilter::default();
        assert_eq!(
            (
                stats.event_count(PW, None, &all),
                stats.event_count(PL, None, &all)
            ),
            (1, 1)
        );
        assert_eq!(stats.event_count(OE, None, &all), 0);
        assert_eq!(stats.event_count(OS, None, &all), 0);
        assert_eq!(stats.total_errors(None, &all), 0);
        assert_eq!(stats.total_errors(Some(ErrorTypeEnum::Unforced), &all), 0);
        assert_eq!(stats.points(&all), 0);
        assert_eq!((snapshot.score_us, snapshot.score_them), (1, 1));
    }
}
