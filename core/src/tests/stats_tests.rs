//! The statistics: how the events of a set are counted (by replaying them,
//! as the app does), and how the counters make the numbers shown in the
//! match stats screen and in the PDF report. Each test pins a rule or an
//! assumption of `shapes/stats.rs` and `Snapshot::record_stats`.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::shapes::{
        enums::{
            ErrorTypeEnum, EvalEnum, EventTypeEnum, LineupRulesEnum, PhaseEnum, TeamSideEnum,
            ZoneEnum,
        },
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        stats::{AttackKey, EventKey, Metric, Stats, StatsFilter},
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
            let set = SetEntry {
                set_number: 1,
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
    /// counter-attacks are the ones not after a reception nor a serve.
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
        assert_eq!((keys[0].prev_type, keys[0].prev_eval), (P, Negative));
        assert!(!keys[0].is_counter_attack());
        // after their reception came straight back: not a counter-attack
        let free_ball = t.serving(vec![act(S, t.setter, Over), act(A, t.oh1, Perfect)]);
        let keys = attacks(&free_ball);
        assert_eq!(keys.len(), 1);
        assert!(!keys[0].is_counter_attack());
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
                .any(|k| (k.prev_type, k.prev_eval, k.eval) == (A, Exclamative, Perfect))
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
            // not a counter-attack: after the serve came back
            act(S, t.setter, Over),
            act(A, t.oh1, Error),
        ]);
        let conversion = |zone| snapshot.stats.counter_attack_conversion(zone, &all());
        assert_eq!(conversion(ZoneEnum::Two), Some((0.0, 1, 0)));
        assert_eq!(conversion(ZoneEnum::Four), Some((100.0, 1, 1)));
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
                prev_eval: Perfect,
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
        assert_eq!(options, vec![OE, F, A]);
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
        use crate::reporting::pdf::{render_match_pdf, render_match_typst};
        use crate::shapes::{r#match::MatchEntry, player::PlayerEntry, team::TeamEntry};
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
        };
        let typst = render_match_typst(&m).expect("expected the report source");
        for player in 1..=7 {
            assert!(typst.contains(&format!("player {}", player)));
        }
        render_match_pdf(&m).expect("expected the report");
    }
}
