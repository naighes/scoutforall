//! Editing an event of a set already scouted: the set replays with the
//! event in place of the one scouted, and the edit applies only if every
//! event still fits and the set ends as it did.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::errors::{AppError, MatchError, SnapshotError};
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, TeamSideEnum},
        set::SetEntry,
        snapshot::EventEntry,
        technique::{ServeTypeEnum, TechniqueEnum},
    };
    use EvalEnum::*;
    use EventTypeEnum::*;
    use chrono::Utc;
    use uuid::Uuid;

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
        }
    }

    /// We receive and score: a reception, then a winning attack, then our
    /// serve.
    fn side_out(team: &Team) -> SetEntry {
        let oh = Some(team.outside_hitter());
        team.set(
            TeamSideEnum::Them,
            vec![
                event(P, oh, Some(Perfect)),
                event(A, oh, Some(Perfect)),
                event(S, Some(team.positions[1]), Some(Negative)),
            ],
        )
    }

    #[test]
    fn snapshot_before_an_event_replays_the_ones_before() {
        let team = Team::new();
        let set = side_out(&team);
        let before = |i| set.snapshot_before(i, DEFAULT_MAX_SUBSTITUTIONS).unwrap();
        assert_eq!((before(0).score_us, before(0).score_them), (0, 0));
        assert!(before(0).get_available_options().contains(&P));
        assert_eq!((before(2).score_us, before(2).score_them), (1, 0));
        assert!(before(2).get_available_options().contains(&S));
    }

    #[test]
    fn edit_fitting_the_events_after_it_applies() {
        let team = Team::new();
        let set = side_out(&team);
        let mut reception = set.events[0].clone();
        reception.eval = Some(Positive);
        let edited = set
            .with_event(0, reception, DEFAULT_MAX_SUBSTITUTIONS)
            .unwrap();
        assert_eq!(edited.events[0].eval, Some(Positive));
        assert_eq!(edited.events[1..].len(), 2);
        assert_eq!(edited.events[0].timestamp, set.events[0].timestamp);
    }

    #[test]
    fn edit_breaking_the_events_after_it_is_rejected() {
        let team = Team::new();
        let set = side_out(&team);
        // the attack out: they serve next, not us
        let mut attack = set.events[1].clone();
        attack.eval = Some(Error);
        let result = set.with_event(1, attack, DEFAULT_MAX_SUBSTITUTIONS);
        assert!(
            matches!(
                result,
                Err(AppError::Snapshot(SnapshotError::InvalidEvent(_)))
            ),
            "{result:?}"
        );
    }

    #[test]
    fn edit_changing_how_the_set_ends_is_rejected() {
        let team = Team::new();
        // 24 opponent errors, then our ace: 25-0
        let events: Vec<EventEntry> = (0..24)
            .map(|_| event(OE, None, None))
            .chain([event(S, Some(team.setter()), Some(Perfect))])
            .collect();
        let set = team.set(TeamSideEnum::Us, events);
        let mut serve = set.events[24].clone();
        // the rally goes on: the set isn't over anymore
        serve.eval = Some(Positive);
        let result = set.with_event(24, serve, DEFAULT_MAX_SUBSTITUTIONS);
        assert!(
            matches!(
                result,
                Err(AppError::Match(MatchError::SetOutcomeChanged(_)))
            ),
            "{result:?}"
        );
        // still an ace, with its serve type
        let mut serve = set.events[24].clone();
        serve.technique = Some(TechniqueEnum::Serve(ServeTypeEnum::ALL[0]));
        assert!(set.with_event(24, serve, DEFAULT_MAX_SUBSTITUTIONS).is_ok());
    }

    #[test]
    fn edited_set_saved_without_rules_gets_the_ones_it_replays_with() {
        let team = Team::new();
        let set = SetEntry {
            lineup_rules: None,
            ..side_out(&team)
        };
        let mut reception = set.events[0].clone();
        reception.eval = Some(Positive);
        let edited = set
            .with_event(0, reception, DEFAULT_MAX_SUBSTITUTIONS)
            .unwrap();
        assert_eq!(
            edited.lineup_rules,
            Some(set.find_lineup_rules(DEFAULT_MAX_SUBSTITUTIONS).unwrap())
        );
    }
}
