//! Our fault (`F`): of the player making it (e.g. a setter's double, a net
//! touch), or of the team, no player (e.g. a rotation fault). How it
//! replays, how the statistics count it and how it can be corrected.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::event_edit::EventEdit;
    use crate::shapes::{
        enums::{ErrorTypeEnum, EvalEnum, EventTypeEnum, LineupRulesEnum, TeamSideEnum},
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        stats::StatsFilter,
    };
    use EvalEnum::*;
    use EventTypeEnum::*;
    use chrono::Utc;
    use uuid::Uuid;

    /// The setter in position 1, the libero for the middle blocker in 6.
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

        fn serve(&self, eval: EvalEnum) -> EventEntry {
            ev(S, Some(self.setter()), Some(eval))
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

    /// The team's fault.
    fn team_fault() -> EventEntry {
        ev(F, None, None)
    }

    fn fault_of(player: Uuid) -> EventEntry {
        ev(F, Some(player), None)
    }

    fn all() -> StatsFilter {
        StatsFilter::default()
    }

    fn by(player: Uuid) -> StatsFilter {
        StatsFilter::new(None, None, Some(player))
    }

    /// Scouting asks the player of a fault, but it can be none; the other
    /// events asking one require it.
    #[test]
    fn the_player_of_a_fault_is_asked_not_required() {
        assert!(F.asks_player());
        assert!(!F.requires_player());
        assert!(F.accepts_player());
        for event_type in [A, B, P, D, R, S, CS] {
            assert!(event_type.asks_player(), "{event_type}");
            assert!(event_type.requires_player(), "{event_type}");
        }
        for event_type in [OS, OE, FB, OF, CL] {
            assert!(!event_type.asks_player(), "{event_type}");
        }
        assert_eq!(F.error_type(None), Some(ErrorTypeEnum::Unforced));
    }

    /// The team's fault, as the player's, gives them the point and the
    /// serve: in break, while we receive, and within a rally.
    #[test]
    fn the_teams_fault_gives_them_the_point() {
        let t = Team::new();
        for fault in [team_fault(), fault_of(t.setter())] {
            // in break: they serve next, the lineup as it was
            let s = t.replay(TeamSideEnum::Us, vec![fault.clone()]);
            assert_eq!((s.score_us, s.score_them), (0, 1));
            assert_eq!(s.get_serving_team(), Some(TeamSideEnum::Them));
            // in side out: they keep serving
            let s = t.replay(TeamSideEnum::Them, vec![fault.clone()]);
            assert_eq!((s.score_us, s.score_them), (0, 1));
            assert_eq!(s.get_serving_team(), Some(TeamSideEnum::Them));
            // within the rally, after their reception
            let s = t.replay(TeamSideEnum::Us, vec![t.serve(Positive), fault]);
            assert_eq!((s.score_us, s.score_them), (0, 1));
        }
    }

    /// A fault names a player on court only.
    #[test]
    fn the_player_of_a_fault_is_on_court() {
        let t = Team::new();
        let refused = |player: Uuid| {
            t.set(TeamSideEnum::Us, vec![fault_of(player)])
                .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
                .is_err()
        };
        assert!(refused(Uuid::new_v4()));
        assert!(!refused(t.positions[1]));
    }

    /// The team's faults count among the errors of the team, of no player;
    /// the player's among hers too.
    #[test]
    fn the_teams_fault_counts_for_the_team_only() {
        let t = Team::new();
        let setter = t.setter();
        let s = t.replay(
            TeamSideEnum::Them,
            vec![team_fault(), fault_of(setter), team_fault()],
        );
        assert_eq!(s.score_them, 3);
        assert_eq!(s.stats.event_count(F, None, &all()), 3);
        assert_eq!(s.stats.errors(F, None, &all()), 3);
        assert_eq!(s.stats.errors(F, Some(ErrorTypeEnum::Unforced), &all()), 3);
        assert_eq!(s.stats.total_errors(None, &all()), 3);
        assert_eq!(s.stats.event_count(F, None, &by(setter)), 1);
        assert_eq!(s.stats.total_errors(None, &by(setter)), 1);
        for player in t.positions.iter().skip(1).chain([&t.libero]) {
            assert_eq!(s.stats.event_count(F, None, &by(*player)), 0);
        }
    }

    /// Correcting a fault: the player's can become another event of hers,
    /// the team's stays a fault (no event of a player without her).
    #[test]
    fn the_teams_fault_is_corrected_as_a_team_event() {
        let t = Team::new();
        // after their reception: the ball theirs to attack
        let edit = |fault: EventEntry| {
            let set = t.set(TeamSideEnum::Us, vec![t.serve(Positive), fault]);
            EventEdit::new(&set, 1, DEFAULT_MAX_SUBSTITUTIONS)
                .expect("expected the fault to be editable")
                .event_types()
        };
        assert_eq!(edit(team_fault()), vec![F]);
        // the outside hitter in the front row
        let players = edit(fault_of(t.positions[1]));
        assert!(players.contains(&F), "{players:?}");
        assert!(players.contains(&B), "{players:?}");
        assert!(!players.contains(&OS), "{players:?}");
    }
}
