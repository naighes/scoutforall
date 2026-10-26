//! The statistics: how the events of a set are counted (by replaying them,
//! as the app does), and how the counters make the numbers shown in the
//! match stats screen and in the PDF report. Each test pins a rule or an
//! assumption of `shapes/stats.rs` and `Snapshot::record_stats`.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::detail::{
        Breakdown, Detail, EvalShare, FUNDAMENTALS, Freeballs, PlayerLine, RotationLine, Tone,
        ZoneDetail, by_player, by_rotation, evaluations,
    };
    use crate::overview::{Errors, Fundamental, Overview, distribution, stats_of as sets_stats};
    use crate::shapes::{
        enums::{
            ErrorTypeEnum, EvalEnum, EventTypeEnum, LineupRulesEnum, PhaseEnum, TeamSideEnum,
            ZoneEnum,
        },
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        stats::{AttackKey, AttackSummary, EventKey, Metric, Stats, StatsFilter},
    };
    use crate::trend::{RECENT_TOUCHES, Replay, Series, trend};
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

        /// Rotation 0 (setter in position 1): outside hitter 1 in 2, middle
        /// blocker 2 in 3, opposite in 4, outside hitter 2 in 5, the libero
        /// in 6 for middle blocker 1.
        fn rotation_0(&self) -> [Uuid; 6] {
            [
                self.setter,
                self.oh1,
                self.mb2,
                self.opposite,
                self.oh2,
                self.mb1,
            ]
        }

        /// Rotation 3 (setter in position 4): the setter in the front row,
        /// the opposite in position 1.
        fn rotation_3(&self) -> [Uuid; 6] {
            [
                self.opposite,
                self.oh2,
                self.mb1,
                self.setter,
                self.oh1,
                self.mb2,
            ]
        }

        fn replay(
            &self,
            serving_team: TeamSideEnum,
            positions: [Uuid; 6],
            events: Vec<EventEntry>,
        ) -> Snapshot {
            self.replay_set(1, serving_team, positions, events)
        }

        fn replay_set(
            &self,
            set_number: u8,
            serving_team: TeamSideEnum,
            positions: [Uuid; 6],
            events: Vec<EventEntry>,
        ) -> Snapshot {
            let set = SetEntry {
                set_number,
                serving_team,
                initial_positions: positions,
                libero: self.libero,
                fallback_libero: None,
                setter: self.setter,
                lineup_rules: Some(LineupRulesEnum::CURRENT),
                events,
            };
            set.compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
                .expect("expected the set to replay")
        }

        /// A set from rotation 0, with them serving (side out).
        fn receiving(&self, events: Vec<EventEntry>) -> Snapshot {
            self.replay(TeamSideEnum::Them, self.rotation_0(), events)
        }

        /// A set from rotation 0, with us serving (break).
        fn serving(&self, events: Vec<EventEntry>) -> Snapshot {
            self.replay(TeamSideEnum::Us, self.rotation_0(), events)
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

    /// An action of ours, with its evaluation.
    fn act(event_type: EventTypeEnum, player: Uuid, eval: EvalEnum) -> EventEntry {
        ev(event_type, Some(player), Some(eval))
    }

    /// An event of theirs (`OS`, `OE`).
    fn opp(event_type: EventTypeEnum) -> EventEntry {
        ev(event_type, None, None)
    }

    fn all() -> StatsFilter {
        StatsFilter::default()
    }

    /// Stats holding these events, all in side out, rotation 0, with no
    /// player: to test the formulas on the counters alone.
    fn stats_of(events: &[(EventTypeEnum, Option<EvalEnum>)]) -> Stats {
        let mut stats = Stats::new();
        for (event_type, eval) in events {
            stats.events.add(EventKey {
                event_type: *event_type,
                phase: PhaseEnum::SideOut,
                rotation: 0,
                player: None,
                zone: None,
                eval: *eval,
            });
        }
        stats
    }

    // --- the rallies -----------------------------------------------------

    /// Who wins the rally each event ends; the rally goes on otherwise.
    #[test]
    fn rally_winner_by_event() {
        let us = Some(TeamSideEnum::Us);
        let them = Some(TeamSideEnum::Them);
        let cases = [
            (OE, None, us),
            (OS, None, them),
            (F, None, them),
            (S, Some(Perfect), us),
            (S, Some(Error), them),
            (S, Some(Over), None),
            (S, Some(Positive), None),
            (S, Some(Negative), None),
            (A, Some(Perfect), us),
            (A, Some(Error), them),
            (A, Some(Over), them),
            (A, Some(Positive), None),
            // blocked, but covered: the rally goes on on our side
            (A, Some(Exclamative), None),
            (A, Some(Negative), None),
            (B, Some(Perfect), us),
            (B, Some(Error), them),
            (B, Some(Over), them),
            (B, Some(Positive), None),
            (B, Some(Negative), None),
            // an overpass isn't a point: the rally goes on on their side
            (P, Some(Over), None),
            (D, Some(Over), None),
            (P, Some(Error), them),
            (D, Some(Error), them),
            (P, Some(Exclamative), None),
            (D, Some(Exclamative), None),
            (R, None, None),
            (CL, None, None),
            (CS, None, None),
        ];
        for (event_type, eval, winner) in cases {
            assert_eq!(
                event_type.rally_winner(eval),
                winner,
                "{:?} {:?}",
                event_type,
                eval
            );
        }
    }

    /// The serving team is the one serving the set at its start, then the
    /// one winning the last rally, and none while a rally is played.
    #[test]
    fn serving_team_follows_the_rallies() {
        let t = Team::new();
        assert_eq!(t.serving(vec![]).get_serving_team(), Some(TeamSideEnum::Us));
        assert_eq!(
            t.receiving(vec![]).get_serving_team(),
            Some(TeamSideEnum::Them)
        );
        let rally = t.receiving(vec![act(P, t.libero, Perfect)]);
        assert_eq!(rally.get_serving_team(), None);
        let won = t.receiving(vec![act(P, t.libero, Perfect), act(A, t.oh1, Perfect)]);
        assert_eq!(won.get_serving_team(), Some(TeamSideEnum::Us));
        let lost = t.serving(vec![act(S, t.setter, Error)]);
        assert_eq!(lost.get_serving_team(), Some(TeamSideEnum::Them));
    }

    /// A rally counts in the phase and rotation it was played in, not in
    /// the ones the lineup moves to after it.
    #[test]
    fn rallies_count_in_the_phase_and_rotation_they_were_played_in() {
        let t = Team::new();
        let snapshot = t.receiving(vec![act(P, t.libero, Perfect), act(A, t.oh1, Perfect)]);
        // the side out moved the lineup to break, in the next rotation
        assert_eq!(
            snapshot.current_lineup.get_current_phase(),
            PhaseEnum::Break
        );
        assert_ne!(snapshot.current_lineup.get_current_rotation().unwrap(), 0);
        let side_out = StatsFilter::new(Some(PhaseEnum::SideOut), Some(0), None);
        assert_eq!(
            snapshot.stats.phases_per_point(&side_out),
            Some((1.0, 1, 1))
        );
        assert_eq!(snapshot.stats.points(&side_out), 1);
        let attack = StatsFilter::new(Some(PhaseEnum::SideOut), Some(0), Some(t.oh1));
        assert_eq!(snapshot.stats.event_count(A, None, &attack), 1);
        let break_ = StatsFilter::new(Some(PhaseEnum::Break), None, None);
        assert_eq!(snapshot.stats.phases_per_point(&break_), None);
        assert_eq!(snapshot.stats.points(&break_), 0);
    }

    /// The rallies per point won: in break the serves per break point, in
    /// side out the receptions per side out; none with no point won.
    #[test]
    fn phases_per_point() {
        let t = Team::new();
        let snapshot = t.serving(vec![
            // lost: they side out
            act(S, t.setter, Negative),
            act(B, t.mb2, Error),
            // won: their serve error, then we serve again
            opp(OE),
            // won: an ace, outside hitter 1 serving after the rotation
            act(S, t.oh1, Perfect),
            // lost: a serve error
            act(S, t.oh1, Error),
        ]);
        let break_ = StatsFilter::new(Some(PhaseEnum::Break), None, None);
        let side_out = StatsFilter::new(Some(PhaseEnum::SideOut), None, None);
        // 3 serves (one per break rally), 1 break point
        assert_eq!(snapshot.stats.phases_per_point(&break_), Some((3.0, 1, 3)));
        // 1 side out rally, won
        assert_eq!(
            snapshot.stats.phases_per_point(&side_out),
            Some((1.0, 1, 1))
        );
        assert_eq!(snapshot.stats.phases_per_point(&all()), Some((2.0, 2, 4)));
    }

    // --- the events -------------------------------------------------------

    /// Every event of the rally is counted, the opponent ones too; the
    /// substitutions and the libero and setter changes aren't.
    #[test]
    fn every_rally_event_is_counted() {
        let t = Team::new();
        let snapshot = t.receiving(vec![
            opp(OE),
            act(S, t.oh1, Error),
            opp(OS),
            act(P, t.libero, Perfect),
            act(A, t.opposite, Perfect),
            // middle blocker 2 serving after the rotation
            act(S, t.mb2, Negative),
            act(D, t.oh2, Positive),
            ev(F, Some(t.mb2), None),
        ]);
        let count = |event_type| snapshot.stats.event_count(event_type, None, &all());
        for event_type in [OE, S, OS, P, A, D, F] {
            assert!(count(event_type) > 0, "{:?}", event_type);
        }
        assert_eq!(count(S), 2);
        assert_eq!(snapshot.stats.events.count(|_| true), 8);
    }

    /// An attack is counted even when the role of the attacker tells no
    /// zone (here, the setter in the back row): it used to be dropped, with
    /// its point, from every statistic.
    #[test]
    fn an_attack_with_no_zone_is_counted() {
        let t = Team::new();
        let snapshot = t.receiving(vec![act(P, t.libero, Perfect), act(A, t.setter, Perfect)]);
        let setter = StatsFilter::new(None, None, Some(t.setter));
        assert_eq!(snapshot.stats.event_count(A, Some(Perfect), &setter), 1);
        assert_eq!(snapshot.stats.points(&setter), 1);
        assert_eq!(
            snapshot
                .stats
                .metric_percentage(A, Metric::Efficiency, &setter),
            Some(100.0)
        );
        // ...but it has no zone for the distribution
        assert_eq!(snapshot.stats.attacks.count(|_| true), 0);
    }

    /// The zone of an attack is told by the role and the row of the
    /// attacker (see `Snapshot::get_attack_zone`).
    #[test]
    fn attack_zone_by_role() {
        let t = Team::new();
        let zone = |snapshot: &Snapshot, player: Uuid| {
            snapshot
                .stats
                .events
                .iter(|k| k.event_type == A && k.player == Some(player))
                .map(|(k, _)| k.zone)
                .next()
                .expect("expected the attack")
        };
        // rotation 0, side out: the hitters attack from the side they stand on
        let side_out =
            |player| t.receiving(vec![act(P, t.libero, Perfect), act(A, player, Negative)]);
        assert_eq!(zone(&side_out(t.oh1), t.oh1), Some(ZoneEnum::Two));
        assert_eq!(
            zone(&side_out(t.opposite), t.opposite),
            Some(ZoneEnum::Four)
        );
        assert_eq!(zone(&side_out(t.mb2), t.mb2), Some(ZoneEnum::Three));
        assert_eq!(zone(&side_out(t.oh2), t.oh2), Some(ZoneEnum::Eight));
        assert_eq!(zone(&side_out(t.setter), t.setter), None);
        // rotation 0, break: they switch, outside hitter in 4, opposite in 2
        let break_ = |player| {
            t.serving(vec![
                act(S, t.setter, Negative),
                act(D, t.oh2, Positive),
                act(A, player, Negative),
            ])
        };
        assert_eq!(zone(&break_(t.oh1), t.oh1), Some(ZoneEnum::Four));
        assert_eq!(zone(&break_(t.opposite), t.opposite), Some(ZoneEnum::Two));
        // rotation 3: the setter in the front row, the opposite in the back
        let rotation_3 = |player| {
            t.replay(
                TeamSideEnum::Them,
                t.rotation_3(),
                vec![act(P, t.oh1, Perfect), act(A, player, Negative)],
            )
        };
        assert_eq!(zone(&rotation_3(t.setter), t.setter), Some(ZoneEnum::Three));
        assert_eq!(
            zone(&rotation_3(t.opposite), t.opposite),
            Some(ZoneEnum::Nine)
        );
        assert_eq!(zone(&rotation_3(t.oh2), t.oh2), Some(ZoneEnum::Four));
        assert_eq!(zone(&rotation_3(t.oh1), t.oh1), Some(ZoneEnum::Eight));
    }

    /// The share of an evaluation is 0% (not "no data") when there are
    /// events but none with that evaluation, and `None` with no events.
    #[test]
    fn event_percentage() {
        let stats = stats_of(&[(P, Some(Positive)), (P, Some(Negative)), (P, Some(Perfect))]);
        let pct = |eval| stats.event_percentage(P, eval, &all());
        assert_eq!(pct(Positive).map(|v| v.round()), Some(33.0));
        assert_eq!(pct(Error), Some(0.0));
        assert_eq!(stats.event_percentage(A, Perfect, &all()), None);
    }

    /// +1 for each good evaluation, -1 for each bad one (efficiency only),
    /// over all the events of the fundamental.
    #[test]
    fn metric_scores() {
        let score = |event_type, eval, metric| {
            stats_of(&[(event_type, Some(eval))])
                .metric_percentage(event_type, metric, &all())
                .unwrap() as i32
                / 100
        };
        // (event, eval, positive, efficiency)
        let cases = [
            // serve: ace, putting them in trouble, their reception coming
            // straight back are good; a serve error is bad
            (S, Perfect, 1, 1),
            (S, Positive, 1, 1),
            (S, Over, 1, 1),
            (S, Negative, 0, 0),
            (S, Error, 0, -1),
            // reception: perfect and good passes are good; an error or an
            // overpass is bad
            (P, Perfect, 1, 1),
            (P, Positive, 1, 1),
            (P, Exclamative, 0, 0),
            (P, Negative, 0, 0),
            (P, Over, 0, -1),
            (P, Error, 0, -1),
            // attack: a kill; positiveness also counts an attack they
            // couldn't counter and one blocked but covered; an error or
            // being blocked is bad
            (A, Perfect, 1, 1),
            (A, Positive, 1, 0),
            (A, Exclamative, 1, 0),
            (A, Negative, 0, 0),
            (A, Over, 0, -1),
            (A, Error, 0, -1),
            // dig: as the reception; an overpass gives them the ball (it
            // used to count as good in the efficiency)
            (D, Perfect, 1, 1),
            (D, Positive, 1, 1),
            (D, Exclamative, 0, 0),
            (D, Negative, 0, 0),
            (D, Over, 0, -1),
            (D, Error, 0, -1),
            // block: a block point or a touch keeping the ball; a block out
            // or an invasion is bad
            (B, Perfect, 1, 1),
            (B, Positive, 1, 1),
            (B, Negative, 0, 0),
            (B, Over, 0, -1),
            (B, Error, 0, -1),
        ];
        for (event_type, eval, positive, efficiency) in cases {
            assert_eq!(
                score(event_type, eval, Metric::Positive),
                positive,
                "positive {:?} {:?}",
                event_type,
                eval
            );
            assert_eq!(
                score(event_type, eval, Metric::Efficiency),
                efficiency,
                "efficiency {:?} {:?}",
                event_type,
                eval
            );
        }
    }

    #[test]
    fn metric_percentage_averages_the_scores() {
        let stats = stats_of(&[
            (A, Some(Perfect)),
            (A, Some(Perfect)),
            (A, Some(Positive)),
            (A, Some(Error)),
        ]);
        // (2 - 1) / 4
        assert_eq!(
            stats.metric_percentage(A, Metric::Efficiency, &all()),
            Some(25.0)
        );
        // 3 / 4
        assert_eq!(
            stats.metric_percentage(A, Metric::Positive, &all()),
            Some(75.0)
        );
        assert_eq!(stats.metric_percentage(S, Metric::Positive, &all()), None);
    }

    // --- errors and points ------------------------------------------------

    /// Which evaluation is an error, forced or not (see
    /// `EventTypeEnum::error_type`).
    #[test]
    fn errors_by_type() {
        let forced = Some(ErrorTypeEnum::Forced);
        let unforced = Some(ErrorTypeEnum::Unforced);
        let cases = [
            (S, Some(Error), unforced),
            (A, Some(Error), unforced),
            (A, Some(Over), forced),
            (B, Some(Over), unforced),
            (B, Some(Error), forced),
            (P, Some(Error), forced),
            (D, Some(Error), forced),
            (F, None, unforced),
            // an overpass and a serve coming back aren't errors
            (P, Some(Over), None),
            (D, Some(Over), None),
            (S, Some(Over), None),
            (A, Some(Negative), None),
            // blocked, but covered: the ball is still ours
            (A, Some(Exclamative), None),
        ];
        for (event_type, eval, error_type) in cases {
            let stats = stats_of(&[(event_type, eval)]);
            let expected = |et| u32::from(error_type == Some(et));
            assert_eq!(
                stats.errors(event_type, None, &all()),
                u32::from(error_type.is_some())
            );
            assert_eq!(
                stats.errors(event_type, Some(ErrorTypeEnum::Forced), &all()),
                expected(ErrorTypeEnum::Forced)
            );
            assert_eq!(
                stats.errors(event_type, Some(ErrorTypeEnum::Unforced), &all()),
                expected(ErrorTypeEnum::Unforced)
            );
        }
    }

    /// The faults are unforced errors in every total: they used to count
    /// only among the forced ones, and never in the points minus errors.
    #[test]
    fn total_errors_include_the_faults() {
        let stats = stats_of(&[(F, None), (F, None), (P, Some(Error)), (S, Some(Error))]);
        assert_eq!(stats.total_errors(None, &all()), 4);
        assert_eq!(stats.total_errors(Some(ErrorTypeEnum::Unforced), &all()), 3);
        assert_eq!(stats.total_errors(Some(ErrorTypeEnum::Forced), &all()), 1);
    }

    /// Our points are the aces, the kills and the block points: the points
    /// they give away aren't ours.
    #[test]
    fn points() {
        let stats = stats_of(&[
            (S, Some(Perfect)),
            (A, Some(Perfect)),
            (B, Some(Perfect)),
            (OE, None),
            (A, Some(Positive)),
            (A, Some(Exclamative)),
            (P, Some(Perfect)),
            (D, Some(Perfect)),
        ]);
        assert_eq!(stats.points(&all()), 3);
    }

    /// The points minus the errors: shown when either is there (it used to
    /// be shown only when both were).
    #[test]
    fn won_minus_lost() {
        assert_eq!(stats_of(&[]).won_minus_lost(&all()), None);
        assert_eq!(
            stats_of(&[(A, Some(Positive))]).won_minus_lost(&all()),
            None
        );
        assert_eq!(
            stats_of(&[(A, Some(Perfect))]).won_minus_lost(&all()),
            Some(1)
        );
        assert_eq!(stats_of(&[(F, None)]).won_minus_lost(&all()), Some(-1));
        let stats = stats_of(&[
            (A, Some(Perfect)),
            (S, Some(Perfect)),
            (A, Some(Over)),
            (F, None),
            (F, None),
        ]);
        assert_eq!(stats.won_minus_lost(&all()), Some(-1));
    }

    // --- the filters ------------------------------------------------------

    /// A player filter keeps the events of the player only: the opponent
    /// events have no player.
    #[test]
    fn filters() {
        let t = Team::new();
        let snapshot = t.receiving(vec![
            opp(OE),
            act(S, t.oh1, Negative),
            act(D, t.oh2, Positive),
            act(A, t.oh1, Perfect),
        ]);
        let stats = &snapshot.stats;
        let player = |p| StatsFilter::new(None, None, Some(p));
        assert_eq!(stats.event_count(OE, None, &all()), 1);
        assert_eq!(stats.event_count(OE, None, &player(t.oh1)), 0);
        assert_eq!(stats.event_count(S, None, &player(t.oh1)), 1);
        assert_eq!(stats.event_count(S, None, &player(t.oh2)), 0);
        assert_eq!(stats.points(&player(t.oh1)), 1);
        // the serve and what follows are in break, in the rotation after
        // the side out
        let rotation = |r| StatsFilter::new(Some(PhaseEnum::Break), Some(r), None);
        let r = snapshot.current_lineup.get_current_rotation().unwrap();
        assert_eq!(stats.event_count(S, None, &rotation(r)), 1);
        assert_eq!(stats.event_count(S, None, &rotation(0)), 0);
        assert_eq!(
            stats.event_count(
                S,
                None,
                &StatsFilter::new(Some(PhaseEnum::SideOut), None, None)
            ),
            0
        );
    }

    /// The stats of a match are the sum of the ones of its sets.
    #[test]
    fn merge_sums_the_sets() {
        let t = Team::new();
        let first = t.serving(vec![act(S, t.setter, Perfect)]);
        let second = t.serving(vec![act(S, t.setter, Perfect), act(S, t.setter, Error)]);
        let mut stats = Stats::new();
        stats.merge(&first.stats);
        stats.merge(&second.stats);
        assert_eq!(stats.event_count(S, None, &all()), 3);
        assert_eq!(stats.event_count(S, Some(Perfect), &all()), 2);
        assert_eq!(stats.phases_per_point(&all()), Some((1.5, 2, 3)));
    }

    // --- the attacks: distribution and counter-attacks --------------------

    /// The attacks of the distribution are the ones built by us; the
    /// counter-attacks are the ones not after our reception.
    #[test]
    fn attacks_built_by_us() {
        let t = Team::new();
        let attacks = |snapshot: &Snapshot| {
            let mut keys: Vec<AttackKey> = snapshot
                .stats
                .attacks
                .iter(|_| true)
                .map(|(k, _)| k.clone())
                .collect();
            keys.sort_by_key(|k| format!("{:?}", k));
            keys
        };
        // after our reception: built by us, not a counter-attack
        let side_out = t.receiving(vec![act(P, t.libero, Negative), act(A, t.oh1, Error)]);
        let keys = attacks(&side_out);
        assert_eq!(keys.len(), 1);
        assert_eq!((keys[0].prev_type, keys[0].prev_eval), (P, Some(Negative)));
        assert!(!keys[0].is_counter_attack());
        // after their reception came straight back: a counter-attack
        let free_ball = t.serving(vec![act(S, t.setter, Over), act(A, t.oh1, Perfect)]);
        let keys = attacks(&free_ball);
        assert_eq!(keys.len(), 1);
        assert!(keys[0].is_counter_attack());
        // after a dig, and again after our attack they couldn't counter
        let transition = t.serving(vec![
            act(S, t.setter, Negative),
            act(D, t.oh2, Positive),
            act(A, t.opposite, Positive),
            act(A, t.oh1, Perfect),
        ]);
        let keys = attacks(&transition);
        assert_eq!(keys.len(), 2);
        assert!(keys.iter().all(|k| k.is_counter_attack()));
        // after our attack blocked but covered: we attack again
        let covered = t.serving(vec![
            act(S, t.setter, Negative),
            act(D, t.oh2, Positive),
            act(A, t.opposite, Exclamative),
            act(A, t.oh1, Perfect),
        ]);
        let keys = attacks(&covered);
        assert_eq!(keys.len(), 2);
        assert!(keys.iter().all(|k| k.is_counter_attack()));
        assert!(
            keys.iter()
                .any(|k| (k.prev_type, k.prev_eval, k.eval) == (A, Some(Exclamative), Perfect))
        );
        // after our overpass the ball is theirs: an attack then isn't built
        // by us (but it's still an attack)
        let overpass = t.serving(vec![
            act(S, t.setter, Negative),
            act(D, t.oh2, Over),
            act(A, t.oh1, Perfect),
        ]);
        assert!(attacks(&overpass).is_empty());
        assert_eq!(overpass.stats.event_count(A, None, &all()), 1);
        // after a reception error the rally is over: no attack
        let error = t.receiving(vec![act(P, t.libero, Error)]);
        assert!(attacks(&error).is_empty());
    }

    /// Their reception coming straight back is played again by the serving
    /// team: its attack is a counter-attack, as the one after a dig.
    #[test]
    fn attack_after_a_serve_coming_back_is_a_counter_attack() {
        let t = Team::new();
        let snapshot = t.serving(vec![act(S, t.setter, Over), act(A, t.oh1, Perfect)]);
        assert_eq!(snapshot.stats.attacks.count(|k| k.is_counter_attack()), 1);
        // the attack after our reception is not
        let snapshot = t.receiving(vec![act(P, t.libero, Perfect), act(A, t.oh1, Perfect)]);
        assert_eq!(snapshot.stats.attacks.count(|k| k.is_counter_attack()), 0);
    }

    /// A team ahead at 8, 16 and 21 in a set to 25; at 5 and 10 in the fifth
    /// set, to 15. A team behind or tied doesn't make a partial.
    #[test]
    fn partials_by_set() {
        let t = Team::new();
        let aces = |set_number, n| {
            t.replay_set(
                set_number,
                TeamSideEnum::Us,
                t.rotation_0(),
                (0..n).map(|_| act(S, t.setter, Perfect)).collect(),
            )
            .partials
        };
        assert_eq!(aces(1, 22), vec![(8, 0), (16, 0), (21, 0)]);
        assert_eq!(aces(5, 14), vec![(5, 0), (10, 0)]);
        assert_eq!(aces(4, 10), vec![(8, 0)]);
        assert_eq!(aces(5, 4), vec![]);
    }

    #[test]
    fn counter_attack_conversion_by_zone() {
        let t = Team::new();
        let snapshot = t.serving(vec![
            act(S, t.setter, Negative),
            act(D, t.oh2, Positive),
            // opposite in break: zone 2
            act(A, t.opposite, Positive),
            // outside hitter in break: zone 4
            act(A, t.oh1, Perfect),
            // a counter-attack too: after the serve came back
            act(S, t.setter, Over),
            act(A, t.oh1, Error),
        ]);
        let conversion = |zone| snapshot.stats.counter_attack_conversion(zone, &all());
        assert_eq!(conversion(ZoneEnum::Two), Some((0.0, 1, 0)));
        assert_eq!(conversion(ZoneEnum::Four), Some((50.0, 2, 1)));
        assert_eq!(conversion(ZoneEnum::Three), None);
    }

    /// The share of the attacks set to each zone, and of the kills there.
    #[test]
    fn zone_distribution() {
        let mut stats = Stats::new();
        let player = Uuid::new_v4();
        let mut attack = |zone, eval| {
            stats.attacks.add(AttackKey {
                phase: PhaseEnum::SideOut,
                rotation: 0,
                player,
                zone,
                eval,
                prev_type: P,
                prev_eval: Some(Perfect),
            })
        };
        attack(ZoneEnum::Four, Perfect);
        attack(ZoneEnum::Four, Error);
        attack(ZoneEnum::Four, Positive);
        attack(ZoneEnum::Two, Perfect);
        let distribution = |zone| stats.zone_distribution(zone, &all());
        assert_eq!(
            distribution(ZoneEnum::Four).map(|(s, k)| (s, k.round())),
            Some((75.0, 33.0))
        );
        assert_eq!(distribution(ZoneEnum::Two), Some((25.0, 100.0)));
        assert_eq!(distribution(ZoneEnum::Three), None);
        let break_ = StatsFilter::new(Some(PhaseEnum::Break), None, None);
        assert_eq!(stats.zone_distribution(ZoneEnum::Four, &break_), None);
    }

    /// The attacks from each zone: all of them, or only the ones on our
    /// reception or in counter-attack.
    #[test]
    fn zone_attacks() {
        let mut stats = Stats::new();
        let player = Uuid::new_v4();
        let mut attack = |zone, eval, prev_type| {
            stats.attacks.add(AttackKey {
                phase: PhaseEnum::SideOut,
                rotation: 0,
                player,
                zone,
                eval,
                prev_type,
                prev_eval: Some(Positive),
            })
        };
        attack(ZoneEnum::Four, Perfect, P);
        attack(ZoneEnum::Four, Error, P);
        attack(ZoneEnum::Four, Over, D);
        attack(ZoneEnum::Four, Positive, D);
        attack(ZoneEnum::Two, Perfect, D);
        let four = |counter_attack| stats.zone_attacks(ZoneEnum::Four, counter_attack, &all());
        let summary = |total, errors, blocked, kills| {
            Some(AttackSummary {
                total,
                errors,
                blocked,
                kills,
            })
        };
        assert_eq!(four(None), summary(4, 1, 1, 1));
        assert_eq!(four(None).map(|s| s.efficiency()), Some(-25.0));
        assert_eq!(four(Some(false)), summary(2, 1, 0, 1));
        assert_eq!(four(Some(true)), summary(2, 0, 1, 0));
        assert_eq!(stats.zone_attacks(ZoneEnum::Three, None, &all()), None);
    }

    // --- the trend ----------------------------------------------------------

    /// The statistics are sampled after every rally, across the sets: the
    /// events in the middle of a rally don't make a point of the trend.
    #[test]
    fn trend_samples_every_rally_across_the_sets() {
        let t = Team::new();
        let set = |set_number, events| SetEntry {
            set_number,
            serving_team: TeamSideEnum::Us,
            initial_positions: t.rotation_0(),
            libero: t.libero,
            fallback_libero: None,
            setter: t.setter,
            lineup_rules: Some(LineupRulesEnum::CURRENT),
            events,
        };
        let first = set(
            1,
            vec![
                act(S, t.setter, Perfect),
                act(S, t.setter, Negative),
                act(D, t.oh2, Positive),
                act(A, t.oh1, Perfect),
            ],
        );
        let second = set(2, vec![act(S, t.setter, Error)]);
        let points = trend(&[&first, &second], DEFAULT_MAX_SUBSTITUTIONS, |stats| {
            stats.metric_percentage(S, Metric::Efficiency, &all())
        })
        .expect("expected the sets to replay");
        let samples: Vec<_> = points
            .iter()
            .map(|p| (p.set_number, p.score_us, p.score_them, p.value))
            .collect();
        assert_eq!(
            samples,
            vec![
                (1, 1, 0, Some(100.0)),
                (1, 2, 0, Some(50.0)),
                (2, 0, 1, Some(0.0)),
            ]
        );
    }

    // --- the overview -------------------------------------------------------

    /// Every event with every evaluation it can have, once: each error is
    /// counted as forced or unforced in the fundamental it was made in,
    /// the aces both ways, the errors and points of the opponent.
    #[test]
    fn overview_counts_every_event_once() {
        let events: Vec<(EventTypeEnum, Option<EvalEnum>)> = EventTypeEnum::ALL
            .iter()
            .filter(|et| !matches!(et, R | CL | CS))
            .flat_map(|et| match et.requires_evaluation() {
                true => et
                    .available_evals()
                    .into_iter()
                    .map(|e| (*et, Some(e)))
                    .collect(),
                false => vec![(*et, None)],
            })
            .collect();
        let overview = Overview::of(&stats_of(&events));
        assert_eq!(
            overview.unforced_errors,
            Errors {
                total: 4,
                // a serve error, an attack error, a block invasion, a fault
                by_event: vec![(S, 1), (A, 1), (B, 1), (F, 1)],
            }
        );
        assert_eq!(
            overview.forced_errors,
            Errors {
                total: 4,
                // an attack blocked, a block out, an ace received, a dig
                // lost
                by_event: vec![(A, 1), (B, 1), (P, 1), (D, 1)],
            }
        );
        assert_eq!(overview.aces, 1);
        assert_eq!(overview.aces_against, 1);
        assert_eq!(overview.opponent_errors, 1);
        assert_eq!(overview.opponent_points, 1);
        // each fundamental once with each evaluation it can have
        let fundamental = |f: Fundamental| (f.total, f.perfect, f.errors);
        assert_eq!(fundamental(overview.serve), (5, 1, 1));
        assert_eq!(fundamental(overview.reception), (6, 1, 1));
        assert_eq!(fundamental(overview.attack), (6, 1, 2));
        assert_eq!(fundamental(overview.dig), (6, 1, 1));
        assert_eq!(fundamental(overview.block), (5, 1, 2));
    }

    /// With no events: nothing to show, every count 0, the errors still by
    /// fundamental.
    #[test]
    fn overview_of_nothing() {
        let overview = Overview::of(&Stats::new());
        let none = Fundamental {
            total: 0,
            efficiency: None,
            perfect: 0,
            errors: 0,
        };
        assert_eq!(overview.reception, none);
        assert_eq!(overview.attack, none);
        assert_eq!(overview.serve, none);
        assert_eq!(overview.block, none);
        assert_eq!(overview.dig, none);
        assert_eq!(overview.side_out, None);
        assert_eq!(overview.break_points, None);
        assert_eq!(overview.first_attack, None);
        assert_eq!(overview.counter_attack, None);
        assert_eq!(
            overview.unforced_errors,
            Errors {
                total: 0,
                by_event: vec![(S, 0), (A, 0), (B, 0), (F, 0)],
            }
        );
        assert_eq!(
            overview.forced_errors,
            Errors {
                total: 0,
                by_event: vec![(A, 0), (B, 0), (P, 0), (D, 0)],
            }
        );
        assert_eq!(
            (
                overview.aces,
                overview.aces_against,
                overview.opponent_errors,
                overview.opponent_points
            ),
            (0, 0, 0, 0)
        );
        assert!(distribution(&Stats::new(), None).is_empty());
    }

    /// A set from rotation 0, them serving, played as the app scouts it.
    fn side_out_set(t: &Team) -> Vec<EventEntry> {
        vec![
            // 1-0: their serve received, the outside hitter 1 kills from 2
            // (rotation 0 in side out); we rotate, and serve
            act(P, t.oh2, Perfect),
            act(A, t.oh1, Perfect),
            // 1-1: the outside hitter 1 serves in the net
            act(S, t.oh1, Error),
            // 1-2: their ace
            act(P, t.oh1, Error),
            // 1-3: the opposite (front row, 2) blocked
            act(P, t.oh1, Positive),
            act(A, t.opposite, Over),
            // 1-4: the outside hitter 2 (front row, 4) out
            act(P, t.oh1, Positive),
            act(A, t.oh2, Error),
            // 1-5: their ace, untouched
            opp(OS),
            // 2-5: the outside hitter 2 can't put it away, the opposite
            // kills in counter-attack
            act(P, t.oh1, Negative),
            act(A, t.oh2, Positive),
            act(A, t.opposite, Perfect),
            // 3-5: their error
            opp(OE),
            // 3-6: our fault
            act(F, t.oh1, Error),
        ]
    }

    /// Their ace is our reception `=`, or their point right after their
    /// serve, before we touch the ball: not their point later in the rally,
    /// nor one while we serve.
    #[test]
    fn their_aces_touched_or_not() {
        let t = Team::new();
        let snapshot = t.receiving(vec![
            // 0-1: our fault before their serve, not an ace
            ev(F, Some(t.oh1), None),
            // 0-2: untouched
            opp(OS),
            // 0-3: their point after our attack
            act(P, t.oh2, Positive),
            act(A, t.oh1, Negative),
            opp(OS),
            // 0-4: touched
            act(P, t.oh2, Error),
            // 1-4: we side out, then serve; 1-5: their point in the rally
            act(P, t.oh2, Positive),
            act(A, t.oh1, Perfect),
            act(S, t.oh1, Negative),
            opp(OS),
            // 1-6: untouched again
            opp(OS),
            // 2-6: their serve error, not an ace
            opp(OE),
        ]);
        assert_eq!((snapshot.score_us, snapshot.score_them), (2, 6));
        assert_eq!(snapshot.stats.untouched_aces(&all()), 2);
        let overview = Overview::of(&snapshot.stats);
        assert_eq!(overview.aces_against, 3);
        assert_eq!(overview.opponent_points, 2);
        assert_eq!(overview.opponent_errors, 1);
        // each in the rotation it was served in
        let rotations: u32 = (0..6)
            .map(|r| {
                snapshot
                    .stats
                    .untouched_aces(&StatsFilter::new(None, Some(r), None))
            })
            .sum();
        assert_eq!(rotations, 2);
        let player = StatsFilter::new(None, None, Some(t.oh2));
        assert_eq!(snapshot.stats.untouched_aces(&player), 0);
    }

    /// The overview of a set played: every number counted by hand.
    #[test]
    fn overview_of_a_set() {
        let t = Team::new();
        let mut events = side_out_set(&t);
        // a fault has no evaluation
        if let Some(fault) = events.last_mut() {
            fault.eval = None;
        }
        let snapshot = t.receiving(events);
        assert_eq!((snapshot.score_us, snapshot.score_them), (3, 6));
        let overview = Overview::of(&snapshot.stats);
        // receptions # = + + -: (1 + 2 - 1) / 5
        assert_eq!(
            overview.reception,
            Fundamental {
                total: 5,
                efficiency: Some(40.0),
                perfect: 1,
                errors: 1,
            }
        );
        // attacks # / = + #: (2 - 2) / 5
        assert_eq!(
            overview.attack,
            Fundamental {
                total: 5,
                efficiency: Some(0.0),
                perfect: 2,
                errors: 2,
            }
        );
        assert_eq!(
            overview.serve,
            Fundamental {
                total: 1,
                efficiency: Some(-100.0),
                perfect: 0,
                errors: 1,
            }
        );
        assert_eq!(overview.block.total, 0);
        // side out: won the first and the counter-attack, lost the two
        // aces, the block and the attack out
        assert_eq!(overview.side_out.map(|(_, w, r)| (w, r)), Some((2, 6)));
        // break: lost the serve and the fault, won their error
        assert_eq!(overview.break_points.map(|(_, w, r)| (w, r)), Some((1, 3)));
        assert_eq!(
            overview.first_attack,
            Some(AttackSummary {
                total: 4,
                errors: 1,
                blocked: 1,
                kills: 1,
            })
        );
        assert_eq!(
            overview.counter_attack,
            Some(AttackSummary {
                total: 1,
                errors: 0,
                blocked: 0,
                kills: 1,
            })
        );
        assert_eq!(overview.unforced_errors.total, 3);
        assert_eq!(
            overview.unforced_errors.by_event,
            vec![(S, 1), (A, 1), (B, 0), (F, 1)]
        );
        assert_eq!(overview.forced_errors.total, 2);
        assert_eq!(
            overview.forced_errors.by_event,
            vec![(A, 1), (B, 0), (P, 1), (D, 0)]
        );
        assert_eq!(overview.aces, 0);
        // the reception `=` and the untouched one
        assert_eq!(overview.aces_against, 2);
        assert_eq!(overview.opponent_errors, 1);
        assert_eq!(overview.opponent_points, 0);
    }

    // --- the detail -------------------------------------------------------

    /// `side_out_set` with the fault as the app saves it, with no
    /// evaluation.
    fn side_out_stats(t: &Team) -> Stats {
        let mut events = side_out_set(t);
        if let Some(fault) = events.last_mut() {
            fault.eval = None;
        }
        t.receiving(events).stats
    }

    fn detail(stats: &Stats, event_type: EventTypeEnum, filter: &StatsFilter) -> Detail {
        match Breakdown::of(stats, event_type, filter) {
            Breakdown::Fundamental(d) => d,
            Breakdown::Freeballs(f) => panic!("expected a fundamental, got {f:?}"),
        }
    }

    fn share(eval: EvalEnum, tone: Tone, count: u32, share: f64) -> EvalShare {
        EvalShare {
            eval,
            tone,
            count,
            share,
        }
    }

    /// Every evaluation a fundamental can have is listed once, and only
    /// those: the freeballs and the other events have none.
    #[test]
    fn the_evaluations_of_each_fundamental() {
        for event_type in FUNDAMENTALS {
            let mut listed: Vec<EvalEnum> =
                evaluations(event_type).iter().map(|(e, _)| *e).collect();
            let mut available = match event_type.requires_evaluation() {
                true => event_type.available_evals(),
                false => vec![],
            };
            listed.sort_by_key(|e| e.to_string());
            available.sort_by_key(|e| e.to_string());
            assert_eq!(listed, available, "{event_type:?}");
        }
        // the best first, the worst last
        let tones = |event_type| {
            evaluations(event_type)
                .iter()
                .map(|(e, t)| format!("{e}{t:?}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(tones(S), "#Excellent /Excellent +Good -Poor =Bad");
        assert_eq!(tones(P), "#Excellent +Good !Fair -Poor /Poor =Bad");
        assert_eq!(tones(D), tones(P));
        assert_eq!(tones(B), "#Excellent +Good -Poor =Bad /Bad");
        assert_eq!(tones(A), "#Excellent +Good !Good -Poor /Bad =Bad");
        assert!(evaluations(FB).is_empty());
    }

    /// The attacks of the set: # / = + #, the opposite and the outside
    /// hitter 1 from 2, the outside hitter 2 from 4.
    #[test]
    fn detail_of_the_attack() {
        let t = Team::new();
        let stats = side_out_stats(&t);
        assert_eq!(
            detail(&stats, A, &all()),
            Detail {
                total: 5,
                evals: vec![
                    share(Perfect, Tone::Excellent, 2, 40.0),
                    share(Positive, Tone::Good, 1, 20.0),
                    share(Over, Tone::Bad, 1, 20.0),
                    share(Error, Tone::Bad, 1, 20.0),
                ],
                // (2 - 2) / 5; # + good, / = bad
                efficiency: Some(0.0),
                // # + ! over 5
                positiveness: Some(60.0),
                // blocked (forced), out (unforced)
                errors: 2,
                unforced_errors: 1,
                // 9 rallies, 3 won
                phases_per_point: Some((3.0, 3, 9)),
                faults: 1,
                points: Some(2),
                zones: vec![
                    ZoneDetail {
                        zone: ZoneEnum::Four,
                        share: 40.0,
                        conversion: 0.0,
                    },
                    ZoneDetail {
                        zone: ZoneEnum::Two,
                        share: 60.0,
                        conversion: 2.0 / 3.0 * 100.0,
                    },
                ],
            }
        );
    }

    /// A player: only the events, the faults and the zones of the player;
    /// the rallies have no player.
    #[test]
    fn detail_of_a_player() {
        let t = Team::new();
        let stats = side_out_stats(&t);
        assert_eq!(
            detail(&stats, A, &StatsFilter::new(None, None, Some(t.opposite))),
            Detail {
                total: 2,
                evals: vec![
                    share(Perfect, Tone::Excellent, 1, 50.0),
                    share(Over, Tone::Bad, 1, 50.0),
                ],
                efficiency: Some(0.0),
                positiveness: Some(50.0),
                errors: 1,
                unforced_errors: 0,
                phases_per_point: None,
                faults: 0,
                points: Some(1),
                zones: vec![ZoneDetail {
                    zone: ZoneEnum::Two,
                    share: 100.0,
                    conversion: 50.0,
                }],
            }
        );
        // the fault of the outside hitter 1
        assert_eq!(
            detail(&stats, S, &StatsFilter::new(None, None, Some(t.oh1))).faults,
            1
        );
    }

    /// The receptions in side out: # = + + -; the fault, in break, is left
    /// out, and so are the rallies in break.
    #[test]
    fn detail_of_the_reception_in_a_phase() {
        let t = Team::new();
        let stats = side_out_stats(&t);
        let side_out = StatsFilter::new(Some(PhaseEnum::SideOut), None, None);
        assert_eq!(
            detail(&stats, P, &side_out),
            Detail {
                total: 5,
                evals: vec![
                    share(Perfect, Tone::Excellent, 1, 20.0),
                    share(Positive, Tone::Good, 2, 40.0),
                    share(Negative, Tone::Poor, 1, 20.0),
                    share(Error, Tone::Bad, 1, 20.0),
                ],
                efficiency: Some(40.0),
                positiveness: Some(60.0),
                // their ace, forced
                errors: 1,
                unforced_errors: 0,
                phases_per_point: Some((3.0, 2, 6)),
                faults: 0,
                // a reception scores no point
                points: None,
                zones: vec![],
            }
        );
        let break_points = StatsFilter::new(Some(PhaseEnum::Break), None, None);
        let none = detail(&stats, P, &break_points);
        assert_eq!(
            (none.total, none.efficiency, none.positiveness),
            (0, None, None)
        );
        assert!(none.evals.is_empty());
        assert_eq!(none.faults, 1);
        assert_eq!(none.phases_per_point, Some((3.0, 1, 3)));
    }

    /// Their freeball after ours, exploited with a kill: with a player, only
    /// the ones given.
    #[test]
    fn detail_of_the_freeballs() {
        let t = Team::new();
        let stats = t
            .receiving(vec![
                act(P, t.oh2, Positive),
                ev(FB, Some(t.setter), None),
                opp(OF),
                act(A, t.oh1, Perfect),
            ])
            .stats;
        let freeballs = |filter| match Breakdown::of(&stats, FB, &filter) {
            Breakdown::Freeballs(f) => f,
            Breakdown::Fundamental(d) => panic!("expected the freeballs, got {d:?}"),
        };
        assert_eq!(
            freeballs(all()),
            Freeballs {
                given: 1,
                received: Some(1),
                exploited: Some((100.0, 1, 1)),
            }
        );
        let player = |p| StatsFilter::new(None, None, Some(p));
        assert_eq!(
            freeballs(player(t.setter)),
            Freeballs {
                given: 1,
                received: None,
                exploited: None,
            }
        );
        assert_eq!(freeballs(player(t.oh1)).given, 0);
        assert!(by_player(&stats, FB, &all()).is_empty());
    }

    /// The players compared: the most involved first, then the most
    /// efficient; only the ones who played the fundamental in the phase.
    #[test]
    fn players_compared() {
        let t = Team::new();
        let stats = side_out_stats(&t);
        let line = |player, total, efficiency, positiveness, points, errors| PlayerLine {
            player,
            total,
            efficiency,
            positiveness,
            points,
            errors,
        };
        assert_eq!(
            by_player(&stats, A, &all()),
            vec![
                // / #
                line(t.opposite, 2, Some(0.0), Some(50.0), Some(1), 1),
                // = +
                line(t.oh2, 2, Some(-50.0), Some(50.0), Some(0), 1),
                // #
                line(t.oh1, 1, Some(100.0), Some(100.0), Some(1), 0),
            ]
        );
        assert_eq!(
            by_player(&stats, P, &all()),
            vec![
                // = + + -: (2 - 1) / 4
                line(t.oh1, 4, Some(25.0), Some(50.0), None, 1),
                line(t.oh2, 1, Some(100.0), Some(100.0), None, 0),
            ]
        );
        // no reception in break; the filter of a player is ignored
        let break_points = StatsFilter::new(Some(PhaseEnum::Break), None, None);
        assert!(by_player(&stats, P, &break_points).is_empty());
        assert_eq!(
            by_player(&stats, A, &StatsFilter::new(None, None, Some(t.oh1))).len(),
            3
        );
    }

    /// The set from rotation 0: the side out at once, then we serve in the
    /// next rotation and lose it, win one side out of five there, and
    /// serve in the one after: their error, our fault.
    #[test]
    fn phases_by_rotation() {
        let t = Team::new();
        let stats = side_out_stats(&t);
        let lines = by_rotation(&stats);
        assert_eq!(
            lines.iter().map(|l| l.rotation).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4, 5]
        );
        let played: Vec<RotationLine> = lines
            .into_iter()
            .filter(|l| l.side_out.is_some() || l.break_points.is_some())
            .collect();
        let won = |p: Option<(f64, u32, u32)>| p.map(|(_, w, r)| (w, r));
        let mut phases: Vec<_> = played
            .iter()
            .map(|l| (won(l.side_out), won(l.break_points)))
            .collect();
        phases.sort();
        assert_eq!(
            phases,
            vec![
                (None, Some((1, 2))),
                (Some((1, 1)), None),
                (Some((1, 5)), Some((0, 1))),
            ]
        );
        assert_eq!(played[0].rotation, 0);
        assert_eq!(won(played[0].side_out), Some((1, 1)));
        // every rally once
        let (side_out, break_points) = played.iter().fold((0, 0), |(s, b), l| {
            (
                s + l.side_out.map_or(0, |(_, _, r)| r),
                b + l.break_points.map_or(0, |(_, _, r)| r),
            )
        });
        assert_eq!((side_out, break_points), (6, 3));
    }

    /// The distribution of the set: the share of each zone and its
    /// efficiency, all the attacks or only the first or the counter ones.
    #[test]
    fn distribution_of_a_set() {
        let t = Team::new();
        let mut events = side_out_set(&t);
        if let Some(fault) = events.last_mut() {
            fault.eval = None;
        }
        let stats = t.receiving(events).stats;
        let zones = |counter_attack| {
            distribution(&stats, counter_attack)
                .into_iter()
                .map(|z| {
                    (
                        z.zone,
                        z.share.round(),
                        z.attacks.total,
                        z.attacks.efficiency().round(),
                    )
                })
                .collect::<Vec<_>>()
        };
        // 4: = +, (0 - 1) / 2; 2: # / #, (2 - 1) / 3
        assert_eq!(
            zones(None),
            vec![
                (ZoneEnum::Four, 40.0, 2, -50.0),
                (ZoneEnum::Two, 60.0, 3, 33.0)
            ]
        );
        assert_eq!(
            zones(Some(false)),
            vec![
                (ZoneEnum::Four, 50.0, 2, -50.0),
                (ZoneEnum::Two, 50.0, 2, 0.0)
            ]
        );
        assert_eq!(zones(Some(true)), vec![(ZoneEnum::Two, 100.0, 1, 100.0)]);
        let shares: f64 = distribution(&stats, None).iter().map(|z| z.share).sum();
        assert!((shares - 100.0).abs() < 1e-9);
    }

    /// An attack with no zone (the setter in the back row) counts in the
    /// attack, not in the distribution.
    #[test]
    fn distribution_leaves_out_the_attacks_with_no_zone() {
        let t = Team::new();
        let stats = t
            .receiving(vec![act(P, t.oh2, Positive), act(A, t.setter, Perfect)])
            .stats;
        assert_eq!(Overview::of(&stats).attack.total, 1);
        assert_eq!(Overview::of(&stats).attack.perfect, 1);
        assert!(distribution(&stats, None).is_empty());
    }

    /// Serving: our ace, a block point, a block invasion.
    #[test]
    fn overview_of_the_serve_and_the_block() {
        let t = Team::new();
        let snapshot = t.serving(vec![
            act(S, t.setter, Perfect),
            act(S, t.setter, Negative),
            act(B, t.opposite, Perfect),
            act(S, t.setter, Positive),
            act(B, t.mb2, Over),
        ]);
        assert_eq!((snapshot.score_us, snapshot.score_them), (2, 1));
        let overview = Overview::of(&snapshot.stats);
        // serves # - +: (2 - 0) / 3
        assert_eq!(overview.serve.total, 3);
        assert_eq!(overview.serve.efficiency.map(f64::round), Some(67.0));
        assert_eq!(overview.aces, 1);
        // blocks # /: (1 - 1) / 2
        assert_eq!(
            overview.block,
            Fundamental {
                total: 2,
                efficiency: Some(0.0),
                perfect: 1,
                errors: 1,
            }
        );
        assert_eq!(
            overview.unforced_errors.by_event,
            vec![(S, 0), (A, 0), (B, 1), (F, 0)]
        );
        assert_eq!(overview.forced_errors.total, 0);
        assert_eq!(overview.break_points.map(|(_, w, r)| (w, r)), Some((2, 3)));
        assert_eq!(overview.side_out, None);
    }

    /// The trend ends where the overview of the set is: its last point is
    /// the statistics of the whole set, one point per rally.
    #[test]
    fn trend_ends_at_the_overview_of_the_set() {
        let t = Team::new();
        let mut events = side_out_set(&t);
        if let Some(fault) = events.last_mut() {
            fault.eval = None;
        }
        let set = SetEntry {
            set_number: 1,
            serving_team: TeamSideEnum::Them,
            initial_positions: t.rotation_0(),
            libero: t.libero,
            fallback_libero: None,
            setter: t.setter,
            lineup_rules: Some(LineupRulesEnum::CURRENT),
            events,
        };
        let snapshot = set
            .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
            .expect("expected the set to replay");
        let points = trend(&[&set], DEFAULT_MAX_SUBSTITUTIONS, Overview::of)
            .expect("expected the set to replay");
        assert_eq!(points.len(), snapshot.points.len());
        assert_eq!(
            points.last().map(|p| &p.value),
            Some(&Overview::of(&snapshot.stats))
        );
        // the reception after each rally: #, then the serve (no
        // reception), then = + + (the untouched ace: none) -, then none
        let receptions: Vec<Option<f64>> = points
            .iter()
            .map(|p| p.value.reception.efficiency.map(f64::round))
            .collect();
        assert_eq!(
            receptions,
            vec![
                Some(100.0),
                Some(100.0),
                Some(0.0),
                Some(33.0),
                Some(50.0),
                Some(50.0),
                Some(40.0),
                Some(40.0),
                Some(40.0),
            ]
        );
        // the score after each rally
        let scores: Vec<(u8, u8)> = points.iter().map(|p| (p.score_us, p.score_them)).collect();
        assert_eq!(
            scores,
            vec![
                (1, 0),
                (1, 1),
                (1, 2),
                (1, 3),
                (1, 4),
                (1, 5),
                (2, 5),
                (3, 5),
                (3, 6)
            ]
        );
    }

    /// The statistics since earlier ones of the same sets are the ones of
    /// the events in between.
    #[test]
    fn stats_since_earlier_ones() {
        let t = Team::new();
        let first = t.serving(vec![act(S, t.setter, Perfect), act(S, t.setter, Error)]);
        let later = t.serving(vec![
            act(S, t.setter, Perfect),
            act(S, t.setter, Error),
            act(P, t.oh2, Positive),
            act(A, t.oh1, Perfect),
        ]);
        let since = later.stats.since(&first.stats);
        assert_eq!(since.event_count(S, None, &all()), 0);
        assert_eq!(since.event_count(P, None, &all()), 1);
        assert_eq!(since.event_count(A, Some(Perfect), &all()), 1);
        assert_eq!(
            since.attack_summary(false, &all()).map(|a| a.total),
            Some(1)
        );
        assert_eq!(
            since.rallies_won(&all()).map(|(_, w, r)| (w, r)),
            Some((1, 1))
        );
        // since nothing: all of them; since themselves: none
        assert_eq!(
            later
                .stats
                .since(&Stats::new())
                .event_count(S, None, &all()),
            2
        );
        assert_eq!(later.stats.since(&later.stats).rallies_won(&all()), None);
    }

    fn side_out_entry(t: &Team, set_number: u8) -> SetEntry {
        let mut events = side_out_set(t);
        if let Some(fault) = events.last_mut() {
            fault.eval = None;
        }
        SetEntry {
            set_number,
            serving_team: TeamSideEnum::Them,
            initial_positions: t.rotation_0(),
            libero: t.libero,
            fallback_libero: None,
            setter: t.setter,
            lineup_rules: Some(LineupRulesEnum::CURRENT),
            events,
        }
    }

    fn reception(stats: &Stats) -> Option<f64> {
        stats.metric_percentage(P, Metric::Efficiency, &all())
    }

    fn receptions(stats: &Stats) -> u32 {
        stats.event_count(P, None, &all())
    }

    /// The last 2 receptions after each rally: # = + + - in the set.
    #[test]
    fn trend_of_the_last_touches() {
        let t = Team::new();
        let set = side_out_entry(&t, 1);
        let replay =
            Replay::new(&[&set], DEFAULT_MAX_SUBSTITUTIONS).expect("expected the set to replay");
        let last_two: Vec<Option<f64>> = replay
            .window(2, receptions, reception)
            .into_iter()
            .map(|p| p.value)
            .collect();
        assert_eq!(
            last_two,
            vec![
                // #, then the serve error: still #
                Some(100.0),
                Some(100.0),
                // # =
                Some(0.0),
                // = +
                Some(0.0),
                // + +, then the untouched ace
                Some(100.0),
                Some(100.0),
                // + -, then their error and our fault
                Some(50.0),
                Some(50.0),
                Some(50.0),
            ]
        );
        // a window larger than the touches: all of them
        let all_of_them = replay.window(10, receptions, reception);
        assert_eq!(all_of_them, replay.cumulative(reception));
        assert_eq!(
            replay.cumulative(reception),
            trend(&[&set], DEFAULT_MAX_SUBSTITUTIONS, reception)
                .expect("expected the set to replay")
        );
    }

    /// The window goes on across the sets: the last 2 receptions of set 2
    /// start with the last one of set 1.
    #[test]
    fn the_last_touches_across_the_sets() {
        let t = Team::new();
        let first = side_out_entry(&t, 1);
        let second = side_out_entry(&t, 2);
        let replay = Replay::new(&[&first, &second], DEFAULT_MAX_SUBSTITUTIONS)
            .expect("expected the sets to replay");
        let points = replay.window(2, receptions, reception);
        assert_eq!(points.len(), 18);
        // the first rally of set 2: - (set 1) and # (set 2)
        let first_of_second = points.iter().find(|p| p.set_number == 2).map(|p| p.value);
        assert_eq!(first_of_second, Some(Some(50.0)));
        // the last: as in set 1
        assert_eq!(points.last().map(|p| p.value), Some(Some(50.0)));
        // the window is always 2 receptions, once there are 2
        let sizes: Vec<u32> = replay
            .window(2, receptions, receptions)
            .into_iter()
            .map(|p| p.value)
            .collect();
        assert_eq!(&sizes[..2], &[1, 1]);
        assert!(sizes[2..].iter().all(|n| *n == 2));
    }

    /// Each figure of the trend on the set played: its value, its touches
    /// and its band.
    #[test]
    fn the_figures_of_the_trend() {
        use crate::benchmark::Measure;
        let t = Team::new();
        let set = side_out_entry(&t, 1);
        let stats = set
            .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
            .expect("expected the set to replay")
            .stats;
        let figure = |s: Series| (s.value(&stats).map(f64::round), s.touches(&stats));
        // receptions # = + + -: (1 + 2 - 1) / 5
        assert_eq!(figure(Series::Reception), (Some(40.0), 5));
        // attacks # / = + #: (2 - 2) / 5
        assert_eq!(figure(Series::Attack), (Some(0.0), 5));
        // one counter-attack, a kill
        assert_eq!(figure(Series::CounterAttack), (Some(100.0), 1));
        assert_eq!(figure(Series::Serve), (Some(-100.0), 1));
        // side out 2 of 6, break point 1 of 3
        assert_eq!(figure(Series::SideOut), (Some(33.0), 6));
        assert_eq!(figure(Series::BreakPoint), (Some(33.0), 3));
        let measures: Vec<Measure> = Series::ALL.iter().map(|s| s.measure()).collect();
        assert_eq!(
            measures,
            vec![
                Measure::Reception,
                Measure::Attack,
                Measure::Attack,
                Measure::Serve,
                Measure::SideOut,
                Measure::BreakPoint
            ]
        );
        // nothing yet: no value, no touches
        for series in Series::ALL {
            assert_eq!(
                (series.value(&Stats::new()), series.touches(&Stats::new())),
                (None, 0)
            );
        }
    }

    /// A figure after every rally: so far, or of its last 10 touches.
    #[test]
    fn the_series_so_far_and_recent() {
        let t = Team::new();
        let sets: Vec<SetEntry> = (1..=3).map(|n| side_out_entry(&t, n)).collect();
        let refs: Vec<&SetEntry> = sets.iter().collect();
        let replay =
            Replay::new(&refs, DEFAULT_MAX_SUBSTITUTIONS).expect("expected the sets to replay");
        let so_far = replay.series(Series::Reception, false);
        assert_eq!(so_far, replay.cumulative(|s| Series::Reception.value(s)));
        let recent = replay.series(Series::Reception, true);
        assert_eq!(
            recent,
            replay.window(RECENT_TOUCHES, receptions, |s| Series::Reception.value(s))
        );
        // 15 receptions in three sets: the last 10 are the last two sets,
        // as the whole match, the same
        assert_eq!(
            recent.last().map(|p| p.value),
            so_far.last().map(|p| p.value)
        );
        // after set 2: the last 10 are sets 1 and 2 entirely, # = + + - twice
        let end_of_two = recent.iter().rfind(|p| p.set_number <= 2);
        assert_eq!(end_of_two.map(|p| p.value), Some(Some(40.0)));
        // early in set 3: the last 10 drop set 1's first reception (#)
        let third: Vec<Option<f64>> = recent
            .iter()
            .filter(|p| p.set_number == 3)
            .map(|p| p.value.map(f64::round))
            .collect();
        // 1st rally: = + + - (set 1) # = + + - (set 2) # (set 3): 40%
        assert_eq!(third.first(), Some(&Some(40.0)));
    }

    /// How far the set had gone after each rally: the points of the team
    /// ahead, the rallies at the same points spread up to them.
    #[test]
    fn the_progress_of_the_set() {
        let t = Team::new();
        let set = side_out_entry(&t, 1);
        let replay =
            Replay::new(&[&set], DEFAULT_MAX_SUBSTITUTIONS).expect("expected the set to replay");
        // 1-0 1-1 1-2 1-3 1-4 1-5 2-5 3-5 3-6: the team ahead at 1 twice,
        // then 2 3 4 5 (three times: 1-5, 2-5, 3-5) 6
        assert_eq!(
            replay.progress(),
            vec![
                0.5,
                1.0,
                2.0,
                3.0,
                4.0,
                5.0 - 2.0 / 3.0,
                5.0 - 1.0 / 3.0,
                5.0,
                6.0
            ]
        );
        // each set from 0 again
        let second = side_out_entry(&t, 2);
        let both = Replay::new(&[&set, &second], DEFAULT_MAX_SUBSTITUTIONS)
            .expect("expected the sets to replay");
        let progress = both.progress();
        assert_eq!(progress.len(), 18);
        assert_eq!(&progress[..9], &progress[9..]);
    }

    /// The sets together count each event of each set once: two sets,
    /// twice the figures of one.
    #[test]
    fn the_stats_of_sets_together() {
        let t = Team::new();
        let (first, second) = (side_out_entry(&t, 1), side_out_entry(&t, 2));
        let one =
            sets_stats(&[&first], DEFAULT_MAX_SUBSTITUTIONS).expect("expected the set to replay");
        let both = sets_stats(&[&first, &second], DEFAULT_MAX_SUBSTITUTIONS)
            .expect("expected the sets to replay");
        let (o1, o2) = (Overview::of(&one), Overview::of(&both));
        assert_eq!(o1.reception.total * 2, o2.reception.total);
        assert_eq!(o1.attack.perfect * 2, o2.attack.perfect);
        assert_eq!(o1.reception.efficiency, o2.reception.efficiency);
        assert_eq!(
            o1.side_out.map(|(_, won, rallies)| (won * 2, rallies * 2)),
            o2.side_out.map(|(_, won, rallies)| (won, rallies))
        );
        // no sets, no figures
        let none = sets_stats(&[], DEFAULT_MAX_SUBSTITUTIONS).expect("expected nothing to replay");
        assert_eq!(Overview::of(&none).reception.total, 0);
    }

    // --- the side out on first rally --------------------------------------

    /// How each first rally of a side out ends, and how it counts.
    #[test]
    fn first_rally_outcomes() {
        let t = Team::new();
        // (events, reception, won, lost, blocked, errors, unforced errors)
        let cases = [
            // their serve error
            (vec![opp(OE)], None, 1, 0, 0, 0, 0),
            // their ace
            (vec![opp(OS)], None, 0, 1, 0, 0, 0),
            // our fault before the reception
            (vec![ev(F, Some(t.oh1), None)], None, 0, 1, 0, 1, 1),
            // a reception error: lost, and a forced error (it used to count
            // as neither)
            (vec![act(P, t.libero, Error)], Some(Error), 0, 1, 0, 1, 0),
            // an overpass: the first attack is lost, the rally isn't
            (vec![act(P, t.libero, Over)], Some(Over), 0, 0, 0, 0, 0),
            // a kill
            (
                vec![act(P, t.libero, Perfect), act(A, t.oh1, Perfect)],
                Some(Perfect),
                1,
                0,
                0,
                0,
                0,
            ),
            // an attack they counter: not a point (it used to count as one)
            (
                vec![act(P, t.libero, Perfect), act(A, t.oh1, Positive)],
                Some(Perfect),
                0,
                0,
                0,
                0,
                0,
            ),
            // blocked, but covered: not lost, the rally goes on
            (
                vec![act(P, t.libero, Perfect), act(A, t.oh1, Exclamative)],
                Some(Perfect),
                0,
                0,
                0,
                0,
                0,
            ),
            (
                vec![act(P, t.libero, Positive), act(A, t.oh1, Negative)],
                Some(Positive),
                0,
                0,
                0,
                0,
                0,
            ),
            // blocked
            (
                vec![act(P, t.libero, Positive), act(A, t.oh1, Over)],
                Some(Positive),
                0,
                1,
                1,
                1,
                0,
            ),
            // attack out
            (
                vec![act(P, t.libero, Exclamative), act(A, t.oh1, Error)],
                Some(Exclamative),
                0,
                1,
                0,
                1,
                1,
            ),
            // their error after our reception
            (
                vec![act(P, t.libero, Negative), opp(OE)],
                Some(Negative),
                1,
                0,
                0,
                0,
                0,
            ),
            // our fault after the reception
            (
                vec![act(P, t.libero, Perfect), ev(F, Some(t.oh1), None)],
                Some(Perfect),
                0,
                1,
                0,
                1,
                1,
            ),
        ];
        for (i, (events, reception, won, lost, blocked, errors, unforced)) in
            cases.into_iter().enumerate()
        {
            let snapshot = t.receiving(events);
            let summary = snapshot
                .stats
                .first_rallies(None, &all())
                .unwrap_or_else(|| panic!("case {}: expected a first rally", i));
            assert_eq!(summary.total, 1, "case {}", i);
            assert_eq!(summary.won, won, "case {}: won", i);
            assert_eq!(summary.lost, lost, "case {}: lost", i);
            assert_eq!(summary.blocked, blocked, "case {}: blocked", i);
            assert_eq!(summary.errors, errors, "case {}: errors", i);
            assert_eq!(summary.unforced_errors, unforced, "case {}: unforced", i);
            let by_reception = snapshot.stats.first_rallies(reception, &all());
            match reception {
                Some(_) => assert_eq!(by_reception, Some(summary), "case {}", i),
                // with no reception, the rally only counts among all of them
                None => assert!(
                    EvalEnum::ALL
                        .iter()
                        .all(|e| snapshot.stats.first_rallies(Some(*e), &all()).is_none()),
                    "case {}",
                    i
                ),
            }
        }
    }

    /// Only the first rally of a side out counts: not the rallies we serve,
    /// nor what follows the first attack.
    #[test]
    fn first_rally_only_in_side_out() {
        let t = Team::new();
        let serving = t.serving(vec![
            act(S, t.setter, Negative),
            act(D, t.oh2, Positive),
            act(A, t.oh1, Perfect),
            opp(OE),
            act(S, t.setter, Error),
        ]);
        assert_eq!(serving.stats.first_rallies(None, &all()), None);
        let long_rally = t.receiving(vec![
            act(P, t.libero, Perfect),
            act(A, t.oh1, Positive),
            act(A, t.opposite, Perfect),
        ]);
        let summary = long_rally.stats.first_rallies(None, &all()).unwrap();
        // the first attack was countered: the second one's kill doesn't count
        assert_eq!((summary.total, summary.won), (1, 0));
    }

    #[test]
    fn first_rally_rates() {
        let t = Team::new();
        let snapshot = t.receiving(vec![
            // won
            act(P, t.libero, Perfect),
            act(A, t.oh1, Perfect),
            // we serve, they side out
            act(S, t.oh1, Error),
            // won, middle blocker 2 serving after the rotation
            opp(OE),
            act(S, t.mb2, Error),
            // lost
            act(P, t.libero, Error),
            // nothing
            act(P, t.libero, Positive),
            act(A, t.oh1, Positive),
        ]);
        let summary = snapshot.stats.first_rallies(None, &all()).unwrap();
        assert_eq!((summary.total, summary.won, summary.lost), (4, 2, 1));
        assert_eq!(summary.won_percentage(), 50.0);
        // (2 - 1) / 4
        assert_eq!(summary.efficiency(), 25.0);
        // the first rallies have no player
        let player = StatsFilter::new(None, None, Some(t.oh1));
        assert_eq!(snapshot.stats.first_rallies(None, &player), None);
    }

    /// An attack blocked but covered keeps the ball on our side: the score
    /// and the serving team don't change, and the attacks after it count.
    #[test]
    fn attack_covered_keeps_the_rally() {
        let t = Team::new();
        let snapshot = t.serving(vec![
            act(S, t.setter, Negative),
            act(D, t.oh2, Positive),
            act(A, t.opposite, Exclamative),
        ]);
        assert_eq!((snapshot.score_us, snapshot.score_them), (0, 0));
        assert_eq!(snapshot.get_serving_team(), None);
        let set = SetEntry {
            set_number: 1,
            serving_team: TeamSideEnum::Us,
            initial_positions: t.rotation_0(),
            libero: t.libero,
            fallback_libero: None,
            setter: t.setter,
            lineup_rules: Some(LineupRulesEnum::CURRENT),
            events: vec![
                act(S, t.setter, Negative),
                act(D, t.oh2, Positive),
                act(A, t.opposite, Exclamative),
            ],
        };
        let snapshot = set.compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS).unwrap();
        let options = snapshot.get_available_options();
        assert_eq!(options, vec![OE, F, A, FB]);
        let snapshot = t.serving(vec![
            act(S, t.setter, Negative),
            act(D, t.oh2, Positive),
            act(A, t.opposite, Exclamative),
            act(A, t.opposite, Exclamative),
            act(A, t.oh1, Perfect),
        ]);
        assert_eq!((snapshot.score_us, snapshot.score_them), (1, 0));
        let stats = &snapshot.stats;
        assert_eq!(stats.event_count(A, None, &all()), 3);
        assert_eq!(stats.event_count(A, Some(Exclamative), &all()), 2);
        assert_eq!(
            stats.metric_percentage(A, Metric::Positive, &all()),
            Some(100.0)
        );
        assert_eq!(
            stats
                .metric_percentage(A, Metric::Efficiency, &all())
                .map(|v| v.round()),
            Some(33.0)
        );
    }

    /// Every evaluation an event can be given leads to the events that can
    /// follow it: none leaves the scouting with nothing to choose.
    #[test]
    fn every_evaluation_has_a_follow_up() {
        let t = Team::new();
        // the events before each action, so that it's available
        let before = |event_type| match event_type {
            S => vec![],
            P => vec![opp(OE), act(S, t.setter, Error)],
            A | D | B => vec![act(S, t.setter, Negative)],
            _ => unreachable!(),
        };
        for event_type in [S, P, A, D, B] {
            for eval in event_type.available_evals() {
                let mut events = before(event_type);
                // the attacker after a serve: a dig first
                if event_type == A {
                    events.push(act(D, t.oh2, Positive));
                }
                // the setter serves first
                let player = if event_type == S { t.setter } else { t.oh1 };
                events.push(act(event_type, player, eval));
                let set = SetEntry {
                    set_number: 1,
                    serving_team: TeamSideEnum::Us,
                    initial_positions: t.rotation_0(),
                    libero: t.libero,
                    fallback_libero: None,
                    setter: t.setter,
                    lineup_rules: Some(LineupRulesEnum::CURRENT),
                    events,
                };
                let snapshot = set.compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS).unwrap();
                let options = snapshot.get_available_options();
                assert!(!options.is_empty(), "{:?} {:?}", event_type, eval);
            }
        }
    }

    // --- the report -------------------------------------------------------

    /// The PDF report of a match builds, with every table of statistics.
    #[test]
    fn match_report_renders() {
        use crate::reporting::model::{Cell, build_report};
        use crate::reporting::pdf::render_report_pdf;
        use crate::shapes::{r#match::MatchEntry, player::PlayerEntry, team::TeamEntry};
        use crate::tests::EN;
        let t = Team::new();
        let players = [t.setter, t.oh1, t.mb2, t.opposite, t.oh2, t.mb1, t.libero]
            .iter()
            .enumerate()
            .map(|(i, id)| PlayerEntry {
                id: *id,
                name: format!("player {}", i + 1),
                number: i as u8 + 1,
                ..PlayerEntry::default()
            })
            .collect();
        let set = SetEntry {
            set_number: 1,
            serving_team: TeamSideEnum::Them,
            initial_positions: t.rotation_0(),
            libero: t.libero,
            fallback_libero: None,
            setter: t.setter,
            lineup_rules: Some(LineupRulesEnum::CURRENT),
            events: vec![
                act(P, t.libero, Perfect),
                act(A, t.oh1, Perfect),
                act(S, t.oh1, Negative),
                act(D, t.oh2, Positive),
                act(A, t.opposite, Over),
                act(P, t.libero, Error),
                ev(F, Some(t.mb2), None),
            ],
        };
        let m = MatchEntry {
            opponent: "them".to_string(),
            date: Utc::now().fixed_offset(),
            id: "match".to_string(),
            team: TeamEntry {
                name: "us".to_string(),
                players,
                ..TeamEntry::default()
            },
            home: true,
            max_substitutions: DEFAULT_MAX_SUBSTITUTIONS,
            sets: vec![set],
            notes: String::new(),
        };
        let report = build_report(&m, EN).expect("expected the report data");
        let names: Vec<&Cell> = report.players.rows.iter().map(|r| &r.lead[1]).collect();
        for player in 1..=7 {
            assert!(names.contains(&&Cell::Text {
                text: format!("player {}", player)
            }));
        }
        render_report_pdf(&report).expect("expected the report");
    }

    /// The report shows the opponent errors in the statistics tables, by
    /// rotation and in the whole match; a player row shows none, since they
    /// have no player.
    #[test]
    fn report_shows_the_opponent_errors() {
        use crate::reporting::model::{Cell, StatsRow, build_report};
        use crate::shapes::{r#match::MatchEntry, player::PlayerEntry, team::TeamEntry};
        use crate::tests::EN;
        let t = Team::new();
        let players = [t.setter, t.oh1, t.mb2, t.opposite, t.oh2, t.mb1, t.libero]
            .iter()
            .enumerate()
            .map(|(i, id)| PlayerEntry {
                id: *id,
                name: format!("player {}", i + 1),
                number: i as u8 + 1,
                ..PlayerEntry::default()
            })
            .collect();
        // an error in side out, setter in 1, then one in break, setter in 6
        let set = SetEntry {
            set_number: 1,
            serving_team: TeamSideEnum::Them,
            initial_positions: t.rotation_0(),
            libero: t.libero,
            fallback_libero: None,
            setter: t.setter,
            lineup_rules: Some(LineupRulesEnum::CURRENT),
            events: vec![act(P, t.libero, Positive), opp(OE), opp(OE)],
        };
        let m = MatchEntry {
            opponent: "them".to_string(),
            date: Utc::now().fixed_offset(),
            id: "match".to_string(),
            team: TeamEntry {
                name: "us".to_string(),
                players,
                ..TeamEntry::default()
            },
            home: true,
            max_substitutions: DEFAULT_MAX_SUBSTITUTIONS,
            sets: vec![set],
            notes: String::new(),
        };
        let report = build_report(&m, EN).expect("expected the report data");
        let labels = EN;
        // the opponent errors, the last statistic of the row with this label
        let opponent_errors = |rows: &[StatsRow], column: usize, label: &str| {
            rows.iter()
                .find(|r| {
                    r.lead[column]
                        == Cell::Text {
                            text: label.to_string(),
                        }
                })
                .and_then(|r| r.values.last().cloned())
                .unwrap_or_else(|| panic!("expected the row {label}"))
        };
        let rotation = |r: u8| format!("{}{}", labels.setter_prefix, r);
        let rotations = &report.rotations.rows;
        // the player rows: the number, then the name
        assert_eq!(opponent_errors(&report.players.rows, 1, "player 7"), "-");
        assert_eq!(opponent_errors(&report.players.rows, 1, labels.total), "2");
        assert_eq!(opponent_errors(rotations, 0, &rotation(1)), "1");
        assert_eq!(opponent_errors(rotations, 0, &rotation(6)), "1");
        assert_eq!(opponent_errors(rotations, 0, &rotation(2)), "-");
        assert_eq!(opponent_errors(rotations, 0, labels.total), "2");
    }
}
