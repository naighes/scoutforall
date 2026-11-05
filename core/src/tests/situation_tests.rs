//! The statistics narrowed to a situation of the game (see
//! `SituationFilter`): our reception in the rally, the touch before, the
//! set of the attack, the role, the reception formation, the inverted P1.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, PhaseEnum, RoleEnum, TeamSideEnum},
        flags::{EventFlags, ReceptionSchemeEnum},
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        stats::{EvalSet, SetTypeSet, SituationFilter, StatsFilter},
        technique::{SetTypeEnum, TechniqueEnum},
    };
    use EvalEnum::*;
    use EventTypeEnum::*;
    use chrono::Utc;
    use uuid::Uuid;

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

        /// Receiving in P1, the libero in for middle blocker 1.
        fn replay(&self, events: Vec<EventEntry>) -> Snapshot {
            SetEntry {
                set_number: 1,
                serving_team: TeamSideEnum::Them,
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
            .unwrap_or_else(|e| panic!("expected the set to replay: {e}"))
        }

        /// Three rallies:
        /// 1. side out in P1, P1 inverted: reception `#`, kill of middle
        ///    blocker 2 on a 7;
        /// 2. break: our serve, their point;
        /// 3. side out in P6, received by four: reception `-`, attack `-` of
        ///    the opposite on a high set, dig `+`, kill of outside hitter 2
        ///    on a high set.
        fn three_rallies(&self) -> Snapshot {
            let inverted = EventFlags::default().with_inverted_p1(true);
            let by_four = EventFlags::default().with_reception(ReceptionSchemeEnum::Four);
            self.replay(vec![
                touch(P, self.libero, Perfect, None, inverted),
                touch(A, self.mb2, Perfect, Some(SetTypeEnum::Seven), inverted),
                touch(S, self.oh1, Positive, None, EventFlags::default()),
                theirs(OS),
                touch(P, self.libero, Negative, None, by_four),
                touch(
                    A,
                    self.opposite,
                    Negative,
                    Some(SetTypeEnum::High),
                    EventFlags::default(),
                ),
                touch(D, self.libero, Positive, None, EventFlags::default()),
                touch(
                    A,
                    self.oh2,
                    Perfect,
                    Some(SetTypeEnum::High),
                    EventFlags::default(),
                ),
            ])
        }
    }

    fn touch(
        event_type: EventTypeEnum,
        player: Uuid,
        eval: EvalEnum,
        set: Option<SetTypeEnum>,
        flags: EventFlags,
    ) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type,
            player: Some(player),
            eval: Some(eval),
            target_player: None,
            technique: set.map(TechniqueEnum::Set),
            flags,
        }
    }

    fn theirs(event_type: EventTypeEnum) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type,
            player: None,
            eval: None,
            target_player: None,
            technique: None,
            flags: Default::default(),
        }
    }

    fn situation(situation: SituationFilter) -> StatsFilter {
        StatsFilter::default().with_situation(situation)
    }

    fn attacks(s: &Snapshot, counter_attack: bool, filter: &StatsFilter) -> u32 {
        s.stats
            .attack_summary(counter_attack, filter)
            .map_or(0, |a| a.total)
    }

    #[test]
    fn no_situation_keeps_everything() {
        let s = Team::new().three_rallies();
        let all = StatsFilter::default();
        assert_eq!(attacks(&s, false, &all), 2);
        assert_eq!(attacks(&s, true, &all), 1);
        assert_eq!(s.stats.rallies_won(&all).map(|r| r.2), Some(3));
    }

    /// "How did my attackers do in counter-attack when the reception was
    /// negative?"
    #[test]
    fn by_the_reception_of_the_rally() {
        let s = Team::new().three_rallies();
        let negative = situation(SituationFilter {
            reception: Some(EvalSet::NEGATIVE),
            ..Default::default()
        });
        assert_eq!(attacks(&s, false, &negative), 1);
        assert_eq!(attacks(&s, true, &negative), 1);
        assert_eq!(
            s.stats.attack_summary(true, &negative).map(|a| a.kills),
            Some(1)
        );
        let positive = situation(SituationFilter {
            reception: Some(EvalSet::POSITIVE),
            ..Default::default()
        });
        assert_eq!(attacks(&s, false, &positive), 1);
        assert_eq!(attacks(&s, true, &positive), 0);
        // the break rally had no reception of ours
        assert_eq!(s.stats.rallies_won(&negative), Some((100.0, 1, 1)));
        assert_eq!(s.stats.rallies_won(&positive), Some((100.0, 1, 1)));
        // the reception itself is in its situation
        assert_eq!(s.stats.event_count(P, None, &negative), 1);
        // so are the first rallies
        assert_eq!(
            s.stats.first_rallies(None, &positive).map(|f| f.total),
            Some(1)
        );
    }

    #[test]
    fn by_the_touch_before() {
        let s = Team::new().three_rallies();
        let after_good_dig = situation(SituationFilter {
            previous: Some(EvalSet::of(&[Positive])),
            ..Default::default()
        });
        assert_eq!(attacks(&s, true, &after_good_dig), 1);
        assert_eq!(attacks(&s, false, &after_good_dig), 0);
        // the dig after our attack `-`... in the same rally
        let after_negative = situation(SituationFilter {
            previous: Some(EvalSet::of(&[Negative])),
            ..Default::default()
        });
        assert_eq!(s.stats.event_count(D, None, &after_negative), 1);
        // the rallies have no touch
        assert_eq!(s.stats.rallies_won(&after_good_dig), None);
        // the reception after their serve has none of ours before: the
        // serve of the rally before isn't
        assert_eq!(s.stats.event_count(P, None, &after_negative), 0);
    }

    /// "How did my middle blockers do on a 7, a C or a fast?"
    #[test]
    fn by_the_set_and_the_role() {
        let t = Team::new();
        let s = t.three_rallies();
        let quick = situation(SituationFilter {
            set_types: Some(SetTypeSet::of(&[
                SetTypeEnum::Seven,
                SetTypeEnum::C,
                SetTypeEnum::Fast,
            ])),
            ..Default::default()
        });
        assert_eq!(attacks(&s, false, &quick), 1);
        assert_eq!(s.stats.event_count(A, Some(Perfect), &quick), 1);
        let middles = situation(SituationFilter {
            role: Some(RoleEnum::MiddleBlocker),
            ..Default::default()
        });
        assert_eq!(
            attacks(&s, false, &middles) + attacks(&s, true, &middles),
            1
        );
        let high = situation(SituationFilter {
            set_types: Some(SetTypeSet::of(&[SetTypeEnum::High])),
            ..Default::default()
        });
        assert_eq!(attacks(&s, false, &high) + attacks(&s, true, &high), 2);
        // a player filter and a situation together
        let oh2_high = StatsFilter::new(None, None, Some(t.oh2)).with_situation(SituationFilter {
            set_types: Some(SetTypeSet::of(&[SetTypeEnum::High])),
            ..Default::default()
        });
        assert_eq!(attacks(&s, true, &oh2_high), 1);
    }

    #[test]
    fn by_the_reception_formation_and_the_inverted_p1() {
        let s = Team::new().three_rallies();
        let by_four = situation(SituationFilter {
            scheme: Some(ReceptionSchemeEnum::Four),
            ..Default::default()
        });
        assert_eq!(s.stats.rallies_won(&by_four), Some((100.0, 1, 1)));
        assert_eq!(s.stats.event_count(A, None, &by_four), 2);
        let by_three = situation(SituationFilter {
            scheme: Some(ReceptionSchemeEnum::Three),
            ..Default::default()
        });
        assert_eq!(s.stats.rallies_won(&by_three), Some((100.0, 1, 1)));
        let inverted = situation(SituationFilter {
            inverted_p1: Some(true),
            ..Default::default()
        });
        assert_eq!(attacks(&s, false, &inverted), 1);
        assert_eq!(
            s.stats.rallies_won(&StatsFilter {
                phase: Some(PhaseEnum::SideOut),
                ..inverted
            }),
            Some((100.0, 1, 1))
        );
        // only in P1 in side out
        let canonical = situation(SituationFilter {
            inverted_p1: Some(false),
            ..Default::default()
        });
        assert_eq!(s.stats.rallies_won(&canonical), None);
    }
}
