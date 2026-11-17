//! The covers: the dig of the attackers after their attack blocked back
//! (our `A!`, their block `!`), told apart from the other digs by the
//! touch before (TODO/001, §4). The defense counts both, as DataVolley;
//! the digs asked (`DigsEnum`) narrow only the defense.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, TeamSideEnum},
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        stats::{DigsEnum, EvalSet, EventPick, Metric, StatsFilter},
    };
    use EvalEnum::*;
    use EventTypeEnum::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn act(event_type: EventTypeEnum, player: Uuid, eval: EvalEnum) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type,
            player: Some(player),
            eval: Some(eval),
            target_player: None,
            technique: None,
            flags: Default::default(),
        }
    }

    /// Us serving, with the setter in position 1: two rallies, each won by
    /// our attack. The first: our dig `+`, our attack blocked back (`!`)
    /// covered with difficulty (`-`), again blocked back and covered well
    /// (`!`). The second: our block `+` keeps the ball and our dig after it
    /// (`+`) is defense, not cover.
    fn replay() -> Snapshot {
        let p: Vec<Uuid> = (0..7).map(|_| Uuid::new_v4()).collect();
        let (setter, oh1, mb2, opposite, oh2) = (p[0], p[1], p[2], p[3], p[4]);
        let events = vec![
            act(S, setter, Positive),
            act(D, oh2, Positive),
            act(A, oh1, Exclamative),
            act(D, opposite, Negative),
            act(A, oh1, Exclamative),
            act(D, oh2, Exclamative),
            act(A, opposite, Perfect),
            act(S, setter, Positive),
            act(B, mb2, Positive),
            act(D, oh2, Positive),
            act(A, oh1, Perfect),
        ];
        SetEntry {
            set_number: 1,
            serving_team: TeamSideEnum::Us,
            initial_positions: [p[0], p[1], p[2], p[3], p[4], p[5]],
            libero: p[6],
            fallback_libero: None,
            setter,
            lineup_rules: Some(LineupRulesEnum::CURRENT),
            events,
        }
        .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
        .expect("expected the set to replay")
    }

    fn digs(digs: DigsEnum) -> StatsFilter {
        StatsFilter::default().with_digs(digs)
    }

    #[test]
    fn the_defense_counts_the_covers_too() {
        let s = replay();
        let count = |d, eval| s.stats.event_count(D, eval, &digs(d));
        assert_eq!(count(DigsEnum::All, None), 4);
        assert_eq!(count(DigsEnum::Defense, None), 2);
        assert_eq!(count(DigsEnum::Cover, None), 2);
        // `-` is of both: the touch before tells the cover
        assert_eq!(count(DigsEnum::Cover, Some(Negative)), 1);
        assert_eq!(count(DigsEnum::Cover, Some(Exclamative)), 1);
        assert_eq!(count(DigsEnum::Defense, Some(Positive)), 2);
        assert_eq!(count(DigsEnum::Defense, Some(Exclamative)), 0);
        // the default is all of them
        assert_eq!(s.stats.event_count(D, None, &StatsFilter::default()), 4);
    }

    #[test]
    fn positiveness_and_efficiency_on_each() {
        let s = replay();
        let pct = |d, metric| s.stats.metric_percentage(D, metric, &digs(d));
        // `#` and `+` good, `/` and `=` bad: `!` and `-` neither
        assert_eq!(pct(DigsEnum::All, Metric::Positive), Some(50.0));
        assert_eq!(pct(DigsEnum::Defense, Metric::Positive), Some(100.0));
        assert_eq!(pct(DigsEnum::Cover, Metric::Efficiency), Some(0.0));
    }

    #[test]
    fn only_the_defense_is_narrowed() {
        let s = replay();
        for d in DigsEnum::ALL {
            assert_eq!(s.stats.event_count(A, None, &digs(d)), 4);
            assert_eq!(s.stats.event_count(S, None, &digs(d)), 2);
            assert_eq!(s.stats.event_count(B, None, &digs(d)), 1);
        }
    }

    #[test]
    fn the_actions_picked_are_the_digs_asked() {
        let s = replay();
        let picked = |d, evals| {
            let pick = EventPick {
                event_type: D,
                evals,
                zone: None,
                target: None,
                serve_start: None,
                filter: digs(d),
            };
            s.stats.events.count(|k| pick.keeps(k))
        };
        assert_eq!(picked(DigsEnum::All, None), 4);
        assert_eq!(picked(DigsEnum::Cover, None), 2);
        assert_eq!(picked(DigsEnum::Defense, Some(EvalSet::of(&[Negative]))), 0);
        assert_eq!(picked(DigsEnum::Cover, Some(EvalSet::of(&[Negative]))), 1);
    }
}
