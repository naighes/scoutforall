//! Editing an event of a set already scouted: the set replays with the
//! event in place of the one scouted, and the edit applies only if every
//! event still fits and the set ends as it did.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::errors::{AppError, MatchError, SnapshotError};
    use crate::event_edit::{EditField, EventEdit, EventEditError};
    use crate::localization::labels;
    use crate::shapes::enums::LanguageEnum;
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
    fn the_score_after_each_event() {
        let team = Team::new();
        assert_eq!(
            side_out(&team).scores(DEFAULT_MAX_SUBSTITUTIONS).unwrap(),
            vec![(0, 0), (1, 0), (1, 0)]
        );
        let mut set = side_out(&team);
        set.events.push(event(OS, None, None));
        set.events.push(event(OE, None, None));
        assert_eq!(
            set.scores(DEFAULT_MAX_SUBSTITUTIONS).unwrap()[3..],
            [(1, 1), (2, 1)]
        );
        // a set that doesn't replay
        set.events
            .insert(0, event(S, Some(team.setter()), Some(Perfect)));
        assert!(set.scores(DEFAULT_MAX_SUBSTITUTIONS).is_err());
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

    // --- correcting an event (`EventEdit`) -------------------------------

    fn edit(set: &SetEntry, index: usize) -> EventEdit {
        EventEdit::new(set, index, DEFAULT_MAX_SUBSTITUTIONS).unwrap()
    }

    #[test]
    fn substitutions_and_changes_cannot_be_corrected() {
        let team = Team::new();
        for event_type in [R, CL, CS] {
            let set = team.set(
                TeamSideEnum::Us,
                vec![event(event_type, Some(team.setter()), None)],
            );
            assert_eq!(
                EventEdit::new(&set, 0, DEFAULT_MAX_SUBSTITUTIONS).err(),
                Some(EventEditError::NotEditable),
                "{event_type:?}"
            );
        }
        let set = side_out(&Team::new());
        assert_eq!(
            EventEdit::new(&set, 3, DEFAULT_MAX_SUBSTITUTIONS).err(),
            Some(EventEditError::Unreadable)
        );
    }

    /// The reception: before it, the set at 0-0 with them serving.
    #[test]
    fn what_an_event_can_become() {
        let team = Team::new();
        let set = side_out(&team);
        let reception = edit(&set, 0);
        assert_eq!(reception.index(), 0);
        assert_eq!(
            (reception.before().score_us, reception.before().score_them),
            (0, 0)
        );
        assert_eq!(
            reception.fields(),
            vec![EditField::EventType, EditField::Eval]
        );
        let types = reception.event_types();
        assert!(types.contains(&P), "{types:?}");
        // never a change of the lineup, always played by a player
        assert!(
            types
                .iter()
                .all(|t| !matches!(t, R | CL | CS) && t.requires_player()),
            "{types:?}"
        );
        assert_eq!(reception.evals(), P.available_evals());
        assert_eq!(reception.techniques(), vec![None]);
        // the serve, with its serve types
        let serve = edit(&set, 2);
        assert_eq!(
            serve.fields(),
            vec![EditField::EventType, EditField::Eval, EditField::Technique]
        );
        assert_eq!(
            serve.techniques(),
            std::iter::once(None)
                .chain(
                    ServeTypeEnum::ALL
                        .into_iter()
                        .map(|t| Some(TechniqueEnum::Serve(t)))
                )
                .collect::<Vec<_>>()
        );
        // the attack, with the sets the outside hitter can receive
        let attack = edit(&set, 1);
        assert!(attack.techniques().len() > 1);
        assert!(
            attack
                .techniques()
                .iter()
                .flatten()
                .all(|t| matches!(t, TechniqueEnum::Set(_)))
        );
    }

    /// Changing the type keeps what the new one allows; what isn't among
    /// the choices is ignored.
    #[test]
    fn choosing_keeps_what_fits() {
        let team = Team::new();
        let set = side_out(&team);
        let mut serve = edit(&set, 2);
        let float = Some(TechniqueEnum::Serve(ServeTypeEnum::ALL[0]));
        serve.choose_technique(float);
        serve.choose_eval(Over);
        assert_eq!(
            (serve.event().eval, serve.event().technique),
            (Some(Over), float)
        );
        // an evaluation of the attack only, and a set: ignored
        serve.choose_eval(Exclamative);
        assert_eq!(serve.event().eval, Some(Over));
        let set_type = edit(&set, 1).techniques().into_iter().flatten().next();
        serve.choose_technique(set_type);
        assert_eq!(serve.event().technique, float);
        // still a serve: the evaluation and the serve type stay
        serve.choose_event_type(S);
        assert_eq!(
            (serve.event().eval, serve.event().technique),
            (Some(Over), float)
        );
        // a fault has no evaluation nor serve type
        assert!(serve.event_types().contains(&F));
        serve.choose_event_type(F);
        assert_eq!(serve.event().event_type, F);
        assert_eq!((serve.event().eval, serve.event().technique), (None, None));
        assert_eq!(serve.fields(), vec![EditField::EventType]);
        // back to a serve: its first evaluation
        serve.choose_event_type(S);
        assert_eq!(serve.event().eval, Some(Perfect));
        // a reception can't come before our serve: ignored
        serve.choose_event_type(P);
        assert_eq!(serve.event().event_type, S);
        // after our reception: an attack, a freeball, a fault
        let mut attack = edit(&set, 1);
        assert_eq!(attack.event_types(), vec![F, A, FB]);
        // the freeball has no evaluation nor set
        attack.choose_technique(attack.techniques().into_iter().flatten().next());
        assert!(attack.event().technique.is_some());
        attack.choose_event_type(FB);
        assert_eq!(
            (attack.event().eval, attack.event().technique),
            (None, None)
        );
    }

    #[test]
    fn a_correction_applies_if_it_fits() {
        let team = Team::new();
        let set = side_out(&team);
        let mut reception = edit(&set, 0);
        reception.choose_eval(Positive);
        let edited = reception.apply().unwrap();
        assert_eq!(edited.events[0].eval, Some(Positive));
        assert_eq!(edited.events.len(), 3);
        // the attack out: they serve next, not us
        let mut attack = edit(&set, 1);
        attack.choose_eval(Error);
        assert_eq!(attack.apply().err(), Some(EventEditError::DoesNotFit));
        // the last ace becomes a serve in play: the set isn't over
        let events: Vec<EventEntry> = (0..24)
            .map(|_| event(OE, None, None))
            .chain([event(S, Some(team.setter()), Some(Perfect))])
            .collect();
        let won = team.set(TeamSideEnum::Us, events);
        let mut ace = edit(&won, 24);
        ace.choose_eval(Positive);
        assert_eq!(ace.apply().err(), Some(EventEditError::ChangesTheResult));
    }

    #[test]
    fn every_error_has_a_message() {
        use EventEditError::*;
        for language in LanguageEnum::ALL {
            let messages: Vec<&str> = [NotEditable, DoesNotFit, ChangesTheResult, Unreadable]
                .iter()
                .map(|e| e.message(labels(language)))
                .collect();
            assert!(messages.iter().all(|m| !m.is_empty()), "{language:?}");
            let mut distinct = messages.clone();
            distinct.sort_unstable();
            distinct.dedup();
            assert_eq!(distinct.len(), 4, "{language:?}");
        }
    }
}
