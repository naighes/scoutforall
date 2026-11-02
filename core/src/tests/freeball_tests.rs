//! The freeballs: our player sending them an easy ball (`FB`), and them
//! sending us one (`OF`), on a second or third touch. Where they can come
//! in the rally, what can follow them, how they replay (score, rotations,
//! libero) and how the statistics count them.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::errors::{AppError, SnapshotError};
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, PhaseEnum, TeamSideEnum, ZoneEnum},
        set::{ReplayIssue, SetEntry},
        snapshot::{EventEntry, Snapshot},
        stats::StatsFilter,
    };
    use EvalEnum::*;
    use EventTypeEnum::*;
    use chrono::Utc;
    use std::str::FromStr;
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

        fn players(&self) -> [Uuid; 7] {
            [
                self.setter,
                self.oh1,
                self.mb2,
                self.opposite,
                self.oh2,
                self.mb1,
                self.libero,
            ]
        }

        /// Rotation 0 (setter in position 1): outside hitter 1 in 2, middle
        /// blocker 2 in 3, opposite in 4, outside hitter 2 in 5, the libero
        /// in 6 for middle blocker 1.
        fn set(&self, serving_team: TeamSideEnum, events: Vec<EventEntry>) -> SetEntry {
            SetEntry {
                set_number: 1,
                serving_team,
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
        }

        fn replay(&self, serving_team: TeamSideEnum, events: Vec<EventEntry>) -> Snapshot {
            self.set(serving_team, events)
                .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
                .expect("expected the set to replay")
        }

        /// With us serving (break).
        fn serving(&self, events: Vec<EventEntry>) -> Snapshot {
            self.replay(TeamSideEnum::Us, events)
        }

        /// With them serving (side out).
        fn receiving(&self, events: Vec<EventEntry>) -> Snapshot {
            self.replay(TeamSideEnum::Them, events)
        }

        /// Why the set doesn't replay.
        fn rejected(&self, serving_team: TeamSideEnum, events: Vec<EventEntry>) -> String {
            match self
                .set(serving_team, events)
                .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
            {
                Err(AppError::Snapshot(SnapshotError::InvalidEvent(reason))) => reason,
                Err(e) => panic!("unexpected error: {e}"),
                Ok(_) => panic!("expected the set not to replay"),
            }
        }

        /// The serve of the setter, in position 1.
        fn serve(&self, eval: EvalEnum) -> EventEntry {
            act(S, self.setter, eval)
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

    /// An action of ours, with its evaluation.
    fn act(event_type: EventTypeEnum, player: Uuid, eval: EvalEnum) -> EventEntry {
        ev(event_type, Some(player), Some(eval))
    }

    /// Our freeball, a team event.
    fn fb() -> EventEntry {
        ev(FB, None, None)
    }

    /// Our freeball naming its player, as scouted before it became a team
    /// event.
    fn fb_named(player: Uuid) -> EventEntry {
        ev(FB, Some(player), None)
    }

    /// An event of theirs: their freeball, their point or error.
    fn opp(event_type: EventTypeEnum) -> EventEntry {
        ev(event_type, None, None)
    }

    fn all() -> StatsFilter {
        StatsFilter::default()
    }

    fn by(player: Uuid) -> StatsFilter {
        StatsFilter::new(None, None, Some(player))
    }

    // --- the event types --------------------------------------------------------

    #[test]
    fn codes_and_names() {
        assert_eq!(FB.to_string(), "FB");
        assert_eq!(OF.to_string(), "OF");
        assert_eq!(EventTypeEnum::from_str("FB").unwrap(), FB);
        assert_eq!(EventTypeEnum::from_str("of").unwrap(), OF);
        // team events: no player (ours named it once, still accepted);
        // neither has an evaluation nor wins the rally
        assert!(!FB.requires_player());
        assert!(FB.accepts_player());
        assert!(!OF.requires_player());
        assert!(!OF.accepts_player());
        for event_type in [FB, OF] {
            assert!(!event_type.requires_evaluation());
            assert!(event_type.available_evals().is_empty());
            assert!(!event_type.provides_direct_points());
            assert_eq!(event_type.rally_winner(None), None);
            assert_eq!(event_type.error_type(None), None);
            assert!(EventTypeEnum::ALL.contains(&event_type));
        }
    }

    // --- what can come next ----------------------------------------------------

    /// The whole table of the events available within a rally: our
    /// freeball wherever the ball is on our side to play, theirs wherever
    /// it's on their side. Everything else as before them.
    #[test]
    fn available_events_within_the_rally() {
        let t = Team::new();
        let ball_ours_to_play = vec![OE, F, A, FB];
        // we play it again, after their freeball or not
        let ball_ours_or_their_freeball = vec![OE, F, A, FB, OF];
        // sent over without an attack: they attack, or send it back easy
        let ball_theirs = vec![OS, OE, F, A, D, B, FB, OF];
        // after their reception or dig they attack (our dig or block, or
        // our attack or freeball after our dig not scouted) or send a
        // freeball: the evaluation tells their reception or dig
        let ball_theirs_received_or_dug = vec![OS, OE, F, A, D, B, FB, OF];
        let cases: Vec<(&str, Snapshot, Vec<EventTypeEnum>)> = vec![
            // our serve: they receive and attack
            (
                "S+",
                t.serving(vec![t.serve(Positive)]),
                ball_theirs_received_or_dug.clone(),
            ),
            (
                "S!",
                t.serving(vec![t.serve(Exclamative)]),
                ball_theirs_received_or_dug.clone(),
            ),
            (
                "S-",
                t.serving(vec![t.serve(Negative)]),
                ball_theirs_received_or_dug.clone(),
            ),
            // their reception comes back: our first touch isn't scouted
            // or our block as it crosses the net, as on their freeball
            ("S/", t.serving(vec![t.serve(Over)]), vec![OE, A, F, B, FB]),
            // our dig keeping the ball
            (
                "D#",
                t.serving(vec![t.serve(Negative), act(D, t.libero, Perfect)]),
                ball_ours_to_play.clone(),
            ),
            (
                "D+",
                t.serving(vec![t.serve(Negative), act(D, t.libero, Positive)]),
                ball_ours_to_play.clone(),
            ),
            (
                "D!",
                t.serving(vec![t.serve(Negative), act(D, t.libero, Exclamative)]),
                ball_ours_to_play.clone(),
            ),
            (
                "D-",
                t.serving(vec![t.serve(Negative), act(D, t.libero, Negative)]),
                ball_ours_to_play.clone(),
            ),
            // our dig going over: not a freeball, a first touch
            (
                "D/",
                t.serving(vec![t.serve(Negative), act(D, t.libero, Over)]),
                ball_theirs.clone(),
            ),
            // our reception keeping the ball
            (
                "P#",
                t.receiving(vec![act(P, t.libero, Perfect)]),
                ball_ours_to_play.clone(),
            ),
            (
                "P+",
                t.receiving(vec![act(P, t.libero, Positive)]),
                ball_ours_to_play.clone(),
            ),
            (
                "P!",
                t.receiving(vec![act(P, t.libero, Exclamative)]),
                ball_ours_to_play.clone(),
            ),
            (
                "P-",
                t.receiving(vec![act(P, t.libero, Negative)]),
                ball_ours_to_play.clone(),
            ),
            (
                "P/",
                t.receiving(vec![act(P, t.libero, Over)]),
                ball_theirs.clone(),
            ),
            // our attack: they couldn't counter it, it was covered, or they dug it
            (
                "A+",
                t.receiving(vec![act(P, t.libero, Perfect), act(A, t.oh1, Positive)]),
                ball_ours_or_their_freeball.clone(),
            ),
            (
                "A!",
                t.receiving(vec![act(P, t.libero, Perfect), act(A, t.oh1, Exclamative)]),
                ball_ours_to_play.clone(),
            ),
            (
                "A-",
                t.receiving(vec![act(P, t.libero, Perfect), act(A, t.oh1, Negative)]),
                ball_theirs_received_or_dug.clone(),
            ),
            // our block
            (
                "B+",
                t.serving(vec![t.serve(Negative), act(B, t.oh1, Positive)]),
                ball_ours_or_their_freeball.clone(),
            ),
            (
                "B-",
                t.serving(vec![t.serve(Negative), act(B, t.oh1, Negative)]),
                ball_theirs_received_or_dug.clone(),
            ),
            // our freeball: they play it, and may send one back
            (
                "FB",
                t.receiving(vec![act(P, t.libero, Negative), fb()]),
                ball_theirs.clone(),
            ),
            // their freeball: no dig of ours, but the same as after one;
            // their point if it falls untouched
            (
                "OF",
                t.serving(vec![t.serve(Negative), act(B, t.oh1, Positive), opp(OF)]),
                vec![OS, OE, F, A, B, FB],
            ),
        ];
        for (case, snapshot, expected) in cases {
            assert_eq!(snapshot.get_available_options(), expected, "after {case}");
        }
    }

    /// Our block on a ball crossing the net without their attack: their
    /// reception straight over (our serve `/`) or their freeball. Each
    /// evaluation tells what comes next, as after their attack: our point,
    /// the ball ours, theirs to counter, their point.
    #[test]
    fn our_block_on_a_ball_not_attacked() {
        let t = Team::new();
        let before: [Vec<EventEntry>; 2] = [
            vec![t.serve(Over)],
            vec![t.serve(Negative), act(B, t.oh1, Positive), opp(OF)],
        ];
        for events in before {
            let block = |eval| t.serving([events.clone(), vec![act(B, t.oh1, eval)]].concat());
            let won = block(Perfect);
            assert_eq!(won.score_us, 1, "{events:?}");
            assert_eq!(
                block(Positive).get_available_options(),
                vec![OE, F, A, FB, OF]
            );
            assert_eq!(
                block(Negative).get_available_options(),
                vec![OS, OE, F, A, D, B, FB, OF]
            );
            assert_eq!(block(Over).score_them, 1, "{events:?}");
            assert_eq!(block(Error).score_them, 1, "{events:?}");
            // in the back row, not on court: no block
            for player in [t.setter, t.mb1] {
                let reason = t.rejected(
                    TeamSideEnum::Us,
                    [events.clone(), vec![act(B, player, Positive)]].concat(),
                );
                assert!(reason.contains("can't play it"), "{reason}");
            }
        }
    }

    /// Our block on their freeball itself: exploited when we win the rally
    /// (our block point too); on an attack after their freeball, it isn't.
    #[test]
    fn our_block_on_their_freeball_is_no_attack_of_theirs() {
        let t = Team::new();
        let freeball = vec![t.serve(Negative), act(B, t.oh1, Positive), opp(OF)];
        let point = t.serving([freeball.clone(), vec![act(B, t.oh1, Perfect)]].concat());
        assert_eq!(exploited(&point), vec![true]);
        let kept = t.serving(
            [
                freeball.clone(),
                vec![act(B, t.oh1, Positive), act(A, t.opposite, Perfect)],
            ]
            .concat(),
        );
        assert_eq!(exploited(&kept), vec![true]);
        let countered = t.serving(
            [
                freeball.clone(),
                vec![act(B, t.oh1, Negative), act(B, t.oh1, Perfect)],
            ]
            .concat(),
        );
        assert_eq!(exploited(&countered), vec![false]);
        let attacked = t.serving(
            [
                freeball,
                vec![act(A, t.opposite, Negative), act(B, t.oh1, Perfect)],
            ]
            .concat(),
        );
        assert_eq!(exploited(&attacked), vec![false]);
    }

    /// Before a serve neither freeball: the rally hasn't started.
    #[test]
    fn no_freeball_before_the_serve() {
        let t = Team::new();
        let rally_starts = [
            t.serving(vec![]),
            t.receiving(vec![]),
            t.serving(vec![t.serve(Perfect)]),
            t.serving(vec![
                t.serve(Negative),
                act(B, t.oh1, Positive),
                opp(OF),
                opp(OS),
            ]),
            t.serving(vec![
                t.serve(Negative),
                act(B, t.oh1, Positive),
                opp(OF),
                act(A, t.oh1, Perfect),
            ]),
        ];
        for snapshot in rally_starts {
            let options = snapshot.get_available_options();
            assert!(
                !options.contains(&FB) && !options.contains(&OF),
                "{options:?}"
            );
        }
    }

    // --- the validation of a saved set -----------------------------------------

    #[test]
    fn freeball_where_it_cant_be_is_rejected() {
        let t = Team::new();
        // ours only before the serve: wherever the ball is theirs, after
        // their attack and our dig not scouted
        let cases = [
            // the ball on our side: they can't send it
            (TeamSideEnum::Us, vec![t.serve(Over), opp(OF)]),
            (
                TeamSideEnum::Them,
                vec![act(P, t.libero, Positive), opp(OF)],
            ),
            (
                TeamSideEnum::Us,
                vec![t.serve(Negative), act(B, t.oh1, Positive), opp(OF), opp(OF)],
            ),
            (
                TeamSideEnum::Them,
                vec![
                    act(P, t.libero, Positive),
                    act(A, t.oh1, Exclamative),
                    opp(OF),
                ],
            ),
            // and no dig of ours on their freeball
            (
                TeamSideEnum::Us,
                vec![
                    t.serve(Negative),
                    act(B, t.oh1, Positive),
                    opp(OF),
                    act(D, t.libero, Positive),
                ],
            ),
            // before the serve
            (TeamSideEnum::Us, vec![fb()]),
            (TeamSideEnum::Them, vec![opp(OF)]),
        ];
        for (serving, events) in cases {
            let last = events.last().unwrap().event_type;
            let reason = t.rejected(serving, events);
            assert!(reason.contains("not available"), "{last}: {reason}");
        }
    }

    /// Our freeball has no evaluation, and the player only as scouted
    /// before (one on court); theirs neither.
    #[test]
    fn freeball_with_an_evaluation_or_a_player_off_court_is_rejected() {
        let t = Team::new();
        let reception = act(P, t.libero, Positive);
        let cases = [
            (ev(FB, None, Some(Positive)), "unexpected evaluation"),
            (
                ev(FB, Some(t.setter), Some(Positive)),
                "unexpected evaluation",
            ),
            (fb_named(Uuid::new_v4()), "can't play it"),
        ];
        for (event, expected) in cases {
            let reason = t.rejected(TeamSideEnum::Them, vec![reception.clone(), event]);
            assert!(reason.contains(expected), "{reason}");
        }
        let block = vec![t.serve(Negative), act(B, t.oh1, Positive)];
        let cases = [
            (ev(OF, Some(t.oh1), None), "unexpected player"),
            (ev(OF, None, Some(Positive)), "unexpected evaluation"),
        ];
        for (event, expected) in cases {
            let reason = t.rejected(TeamSideEnum::Us, [block.clone(), vec![event]].concat());
            assert!(reason.contains(expected), "{reason}");
        }
    }

    /// Their freeball after our attack they dug, or after our serve `+`,
    /// as scouted up to v0.11 and refused from v0.12: valid again, the
    /// evaluation telling their reception or dig, the freeball what they
    /// made of it; exploited when we win the rally.
    #[test]
    fn their_freeball_after_their_reception_or_dig_replays() {
        let t = Team::new();
        let dug = t.set(
            TeamSideEnum::Them,
            vec![
                act(P, t.libero, Positive),
                act(A, t.oh1, Negative),
                opp(OF),
                act(A, t.opposite, Perfect),
            ],
        );
        assert_eq!(dug.replay_issue(DEFAULT_MAX_SUBSTITUTIONS), None);
        for eval in [Positive, Exclamative, Negative] {
            let served = t.serving(vec![t.serve(eval), opp(OF), act(A, t.oh1, Perfect)]);
            assert_eq!(served.stats.event_count(OF, None, &all()), 1, "S{eval:?}");
            assert_eq!(served.stats.event_count(S, None, &all()), 1, "S{eval:?}");
            assert_eq!(exploited(&served), vec![true], "S{eval:?}");
            let attacked = t.serving(vec![
                t.serve(eval),
                opp(OF),
                act(A, t.oh1, Negative),
                act(D, t.libero, Positive),
            ]);
            assert_eq!(exploited(&attacked), vec![false], "S{eval:?}");
        }
        let blocked = t.serving(vec![
            t.serve(Negative),
            act(B, t.oh1, Negative),
            opp(OF),
            act(A, t.oh1, Perfect),
        ]);
        assert_eq!(blocked.stats.event_count(OF, None, &all()), 1);
        assert_eq!(exploited(&blocked), vec![true]);
    }

    /// Their freeball where the ball came straight back (our serve `/`) or
    /// is ours to play: refused while scouting, told replaying a set.
    #[test]
    fn their_freeball_where_it_cant_be_is_told() {
        let t = Team::new();
        let served = t.set(TeamSideEnum::Us, vec![t.serve(Over), opp(OF)]);
        let issue = served.replay_issue(DEFAULT_MAX_SUBSTITUTIONS).unwrap();
        assert_eq!(
            issue,
            ReplayIssue {
                set_number: 1,
                row: 2,
                event: "OF".into(),
                previous: Some("S/".into()),
            }
        );
        assert_eq!(
            issue.describe(crate::tests::EN),
            "set 1, row 2: OF after S/ isn't valid in this version"
        );
        let received = t.set(TeamSideEnum::Them, vec![act(P, t.libero, Perfect), opp(OF)]);
        let issue = received.replay_issue(DEFAULT_MAX_SUBSTITUTIONS).unwrap();
        assert_eq!((issue.row, issue.previous), (2, Some("P#".into())));
    }

    /// A freeball scouted before it became a team event names its player:
    /// it still replays, anyone on court, the libero too, and the
    /// statistics ignore the player.
    #[test]
    fn freeball_naming_its_player_still_replays() {
        let t = Team::new();
        for player in [t.setter, t.oh1, t.mb2, t.opposite, t.oh2, t.libero] {
            let snapshot = t.receiving(vec![act(P, t.oh2, Positive), fb_named(player)]);
            assert_eq!(snapshot.last_event.as_ref().unwrap().event_type, FB);
            assert_eq!(snapshot.stats.event_count(FB, None, &all()), 1);
            assert_eq!(snapshot.stats.event_count(FB, None, &by(player)), 0);
        }
        // middle blocker 1 is on the bench for the libero
        let reason = t.rejected(
            TeamSideEnum::Them,
            vec![act(P, t.oh2, Positive), fb_named(t.mb1)],
        );
        assert!(reason.contains("can't play it"), "{reason}");
    }

    /// The same rallies replay alike with our freeball naming its player or
    /// not: the score, the rotations and the statistics.
    #[test]
    fn freeball_naming_its_player_replays_as_a_team_one() {
        let t = Team::new();
        let rally = |freeball: EventEntry| {
            vec![
                act(P, t.oh2, Positive),
                freeball,
                act(D, t.libero, Positive),
                act(A, t.oh1, Perfect),
            ]
        };
        let team = t.receiving(rally(fb()));
        let named = t.receiving(rally(fb_named(t.setter)));
        assert_eq!(state(&t, &team), state(&t, &named));
        let counts = |s: &Snapshot| {
            EventTypeEnum::ALL
                .iter()
                .map(|e| s.stats.event_count(*e, None, &all()))
                .collect::<Vec<_>>()
        };
        assert_eq!(counts(&team), counts(&named));
        assert_eq!(
            team.stats.attacks.iter(|_| true).count(),
            named.stats.attacks.iter(|_| true).count()
        );
    }

    // --- the replay ------------------------------------------------------------

    /// Where every player is, and what's next.
    fn state(t: &Team, s: &Snapshot) -> (u8, u8, u8, PhaseEnum, Vec<Option<usize>>) {
        (
            s.score_us,
            s.score_them,
            s.current_lineup.get_current_rotation().unwrap(),
            s.current_lineup.get_current_phase(),
            t.players()
                .iter()
                .map(|p| s.current_lineup.find_position(p))
                .collect(),
        )
    }

    /// Freeballs back and forth don't end the rally: the score, the
    /// rotations and the libero go as without them.
    #[test]
    fn freeballs_back_and_forth_replay_like_any_rally() {
        let t = Team::new();
        let rallies = |freeballs: bool| {
            let middle = |plain: Vec<EventEntry>, with: Vec<EventEntry>| match freeballs {
                true => with,
                false => plain,
            };
            [
                // they serve: our reception, our freeball, theirs, our
                // kill; we side out and rotate
                vec![act(P, t.libero, Positive)],
                middle(
                    vec![act(A, t.oh1, Negative), act(D, t.libero, Positive)],
                    vec![fb(), opp(OF)],
                ),
                vec![act(A, t.opposite, Perfect)],
                // we serve: their freeball, ours, and their point
                vec![act(S, t.oh1, Negative)],
                middle(
                    vec![act(D, t.libero, Positive), act(A, t.mb2, Negative)],
                    vec![act(D, t.libero, Over), opp(OF), fb(), opp(OF), fb()],
                ),
                vec![opp(OS)],
                // they serve again: their error after our freeball
                vec![act(P, t.libero, Perfect)],
                middle(vec![act(A, t.oh1, Negative)], vec![fb()]),
                vec![opp(OE)],
            ]
            .concat()
        };
        let plain = t.receiving(rallies(false));
        let with_freeballs = t.receiving(rallies(true));
        assert_eq!(state(&t, &with_freeballs), state(&t, &plain));
        assert_eq!((with_freeballs.score_us, with_freeballs.score_them), (2, 1));
        assert_eq!(
            with_freeballs.get_serving_team(),
            Some(TeamSideEnum::Us),
            "their error: we serve"
        );
        // the rallies counted: three, the same
        let rallies = |s: &Snapshot| s.stats.rallies.count(|_| true);
        assert_eq!(rallies(&with_freeballs), 3);
        assert_eq!(rallies(&plain), 3);
    }

    /// Their freeball falling on our court untouched: their point.
    #[test]
    fn their_freeball_untouched_is_their_point() {
        let t = Team::new();
        let snapshot = t.serving(vec![
            t.serve(Positive),
            act(B, t.oh1, Positive),
            opp(OF),
            opp(OS),
        ]);
        assert_eq!((snapshot.score_us, snapshot.score_them), (0, 1));
        assert_eq!(snapshot.get_serving_team(), Some(TeamSideEnum::Them));
    }

    /// Our freeball out or into the net: a fault of ours.
    #[test]
    fn our_freeball_out_is_our_fault() {
        let t = Team::new();
        let snapshot = t.receiving(vec![
            act(P, t.libero, Positive),
            fb(),
            ev(F, Some(t.setter), None),
        ]);
        assert_eq!((snapshot.score_us, snapshot.score_them), (0, 1));
    }

    // --- the statistics -----------------------------------------------------------

    /// The freeballs we send and the ones we get, counted as events: ours
    /// by the player sending them.
    #[test]
    fn freeballs_are_counted() {
        let t = Team::new();
        let snapshot = t.serving(vec![
            t.serve(Negative),
            act(B, t.oh1, Positive),
            opp(OF),
            fb(),
            opp(OF),
            fb(),
            act(D, t.libero, Positive),
            fb(),
            opp(OS),
        ]);
        let stats = &snapshot.stats;
        assert_eq!(stats.event_count(FB, None, &all()), 3);
        assert_eq!(stats.event_count(OF, None, &all()), 2);
        // team events: none by player
        for player in t.players() {
            assert_eq!(stats.event_count(FB, None, &by(player)), 0);
            assert_eq!(stats.event_count(OF, None, &by(player)), 0);
        }
        // in the phase and rotation they were played in
        let in_break = StatsFilter::new(Some(PhaseEnum::Break), Some(0), None);
        assert_eq!(stats.event_count(FB, None, &in_break), 3);
        assert_eq!(stats.event_count(OF, None, &in_break), 2);
    }

    /// Their freeball isn't a dig of ours: the digs, the errors and the
    /// points stay as they were.
    #[test]
    fn freeballs_leave_the_other_numbers_alone() {
        let t = Team::new();
        let snapshot = t.serving(vec![
            t.serve(Negative),
            act(B, t.oh1, Positive),
            opp(OF),
            fb(),
            opp(OF),
            act(A, t.oh1, Perfect),
        ]);
        let stats = &snapshot.stats;
        assert_eq!(stats.event_count(D, None, &all()), 0);
        assert_eq!(stats.total_errors(None, &all()), 0);
        assert_eq!(stats.points(&all()), 1);
        assert_eq!(stats.won_minus_lost(&all()), Some(1));
    }

    /// The attack on their freeball is built by us: a counter-attack,
    /// whatever the phase, in the distribution and the counter-attack
    /// conversion.
    #[test]
    fn attack_on_their_freeball_is_a_counter_attack() {
        let t = Team::new();
        let cases = [
            // after our serve
            (
                t.serving(vec![
                    t.serve(Negative),
                    act(B, t.oh1, Positive),
                    opp(OF),
                    act(A, t.oh1, Perfect),
                ]),
                PhaseEnum::Break,
            ),
            // after their reception came back and we sent a freeball
            (
                t.serving(vec![t.serve(Over), fb(), opp(OF), act(A, t.oh1, Perfect)]),
                PhaseEnum::Break,
            ),
            // in side out, after they dug our attack
            (
                t.receiving(vec![
                    act(P, t.libero, Perfect),
                    act(A, t.opposite, Positive),
                    opp(OF),
                    act(A, t.oh1, Perfect),
                ]),
                PhaseEnum::SideOut,
            ),
            // after our reception and dig went over
            (
                t.receiving(vec![
                    act(P, t.libero, Over),
                    opp(OF),
                    act(A, t.oh1, Perfect),
                ]),
                PhaseEnum::SideOut,
            ),
        ];
        for (snapshot, phase) in cases {
            let attacks: Vec<_> = snapshot
                .stats
                .attacks
                .iter(|k| k.player == t.oh1)
                .map(|(k, _)| k.clone())
                .collect();
            assert_eq!(attacks.len(), 1, "{attacks:?}");
            let attack = &attacks[0];
            assert_eq!((attack.prev_type, attack.prev_eval), (OF, None));
            assert_eq!(attack.phase, phase);
            assert!(attack.is_counter_attack());
            assert!(attack.is_freeball_attack());
            assert_eq!(
                snapshot
                    .stats
                    .counter_attack_conversion(attack.zone, &by(t.oh1)),
                Some((100.0, 1, 1))
            );
            assert!(
                snapshot
                    .stats
                    .zone_distribution(attack.zone, &all())
                    .is_some()
            );
        }
    }

    /// The counter-attacks on a freeball count with the ones on a dig.
    #[test]
    fn counter_attack_conversion_counts_the_freeballs() {
        let t = Team::new();
        // rotation 0, break: outside hitter 1 in the front row, from 4. We
        // dig, they dig our attack and send us a freeball, our kill (the
        // scenario of TODO/test_freeball_attack_scores.md)
        let snapshot = t.serving(vec![
            t.serve(Negative),
            act(D, t.libero, Positive),
            act(A, t.oh1, Positive),
            opp(OF),
            act(A, t.oh1, Perfect),
        ]);
        assert_eq!((snapshot.score_us, snapshot.score_them), (1, 0));
        assert_eq!(snapshot.get_serving_team(), Some(TeamSideEnum::Us));
        let stats = &snapshot.stats;
        assert_eq!(
            stats.counter_attack_conversion(ZoneEnum::Four, &all()),
            Some((50.0, 2, 1))
        );
        let freeball_attacks = stats.attacks.count(|k| k.is_freeball_attack());
        assert_eq!(freeball_attacks, 1);
        // the attack on the dig isn't on a freeball
        let on_the_dig = stats
            .attacks
            .count(|k| k.prev_type == D && !k.is_freeball_attack());
        assert_eq!(on_the_dig, 1);
    }

    /// After our freeball the ball is theirs: an attack of ours coming
    /// straight after it (their attack and our dig not scouted) is built by
    /// us all the same, a counter-attack. The dig of their attack counts.
    #[test]
    fn after_our_freeball_as_after_our_attack_they_dug() {
        let t = Team::new();
        let snapshot = t.receiving(vec![
            act(P, t.libero, Negative),
            fb(),
            act(A, t.oh1, Perfect),
        ]);
        let attacks: Vec<_> = snapshot.stats.attacks.iter(|_| true).collect();
        assert_eq!(attacks.len(), 1);
        assert_eq!(attacks[0].0.prev_type, FB);
        assert!(attacks[0].0.is_counter_attack());
        assert_eq!(snapshot.stats.event_count(A, Some(Perfect), &all()), 1);
        let snapshot = t.receiving(vec![
            act(P, t.libero, Negative),
            fb(),
            act(D, t.libero, Positive),
            act(A, t.oh1, Perfect),
        ]);
        assert_eq!(
            snapshot.stats.event_count(D, Some(Positive), &all()),
            1,
            "the dig of their attack"
        );
        let attacks: Vec<_> = snapshot.stats.attacks.iter(|_| true).collect();
        assert_eq!(attacks.len(), 1);
        assert_eq!(attacks[0].0.prev_type, D);
    }

    /// Our freeball after a reception keeping the ball: the first rally
    /// ends there, our first attack lost (as with an overpass). It counts
    /// as neither won nor lost.
    #[test]
    fn freeball_after_the_reception_ends_the_first_rally() {
        let t = Team::new();
        for eval in [Perfect, Positive, Exclamative, Negative] {
            let snapshot = t.receiving(vec![
                act(P, t.libero, eval),
                fb(),
                opp(OF),
                act(A, t.oh1, Perfect),
            ]);
            let summary = snapshot.stats.first_rallies(None, &all()).unwrap();
            assert_eq!(
                (summary.total, summary.won, summary.lost, summary.errors),
                (1, 0, 0, 0),
                "after P{eval}"
            );
            let keys: Vec<_> = snapshot.stats.first_rallies.iter(|_| true).collect();
            assert_eq!(keys.len(), 1);
            assert_eq!(
                (keys[0].0.reception_eval, keys[0].0.ending),
                (Some(eval), FB)
            );
        }
        // the attack after their freeball, later in the rally, isn't
        // the first one
        let snapshot = t.receiving(vec![
            act(P, t.libero, Over),
            opp(OF),
            act(A, t.oh1, Perfect),
        ]);
        let keys: Vec<_> = snapshot.stats.first_rallies.iter(|_| true).collect();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].0.ending, P);
    }

    /// Their freeball in break doesn't touch the first rallies, which are
    /// of the side out.
    #[test]
    fn freeballs_in_break_are_not_first_rallies() {
        let t = Team::new();
        let snapshot = t.serving(vec![
            t.serve(Negative),
            act(B, t.oh1, Positive),
            opp(OF),
            fb(),
            opp(OS),
        ]);
        assert!(snapshot.stats.first_rallies(None, &all()).is_none());
    }

    // --- the freeballs we exploited -------------------------------------------------

    /// Whether we exploited their freeball, for each one in the rally.
    fn exploited(snapshot: &Snapshot) -> Vec<bool> {
        let mut keys: Vec<_> = snapshot
            .stats
            .freeballs
            .iter(|_| true)
            .flat_map(|(k, v)| std::iter::repeat_n(k.exploited, v as usize))
            .collect();
        keys.sort();
        keys
    }

    /// Their freeball is exploited when we win the rally before they attack
    /// again, whoever scores the point.
    #[test]
    fn freeball_exploited_when_we_win_before_they_attack_again() {
        let t = Team::new();
        let cases = [
            ("our kill", vec![act(A, t.oh1, Perfect)]),
            ("their error", vec![opp(OE)]),
            (
                "their error after digging our attack",
                vec![act(A, t.oh1, Negative), opp(OE)],
            ),
            (
                "our attack they couldn't counter, then our kill",
                vec![act(A, t.oh1, Positive), act(A, t.oh1, Perfect)],
            ),
            (
                "our attack blocked but covered, then our kill",
                vec![act(A, t.oh1, Exclamative), act(A, t.oh1, Perfect)],
            ),
            (
                "our attack on their overpass",
                vec![act(A, t.oh1, Negative), act(A, t.mb2, Perfect)],
            ),
            ("our freeball back, their error", vec![fb(), opp(OE)]),
        ];
        for (case, after) in cases {
            let events = [
                vec![t.serve(Negative), act(B, t.oh1, Positive), opp(OF)],
                after,
            ]
            .concat();
            let snapshot = t.serving(events);
            assert_eq!(snapshot.score_us, 1, "{case}");
            assert_eq!(exploited(&snapshot), vec![true], "{case}");
            assert_eq!(
                snapshot.stats.freeball_exploitation(&all()),
                Some((100.0, 1, 1)),
                "{case}"
            );
        }
    }

    /// Their freeball isn't exploited when they win the rally, or when they
    /// attack again (our dig or block comes next), even if we win it later.
    #[test]
    fn freeball_not_exploited_when_they_win_or_attack_again() {
        let t = Team::new();
        let cases = [
            ("untouched, their point", vec![opp(OS)]),
            ("our fault", vec![ev(F, Some(t.oh1), None)]),
            ("our attack error", vec![act(A, t.oh1, Error)]),
            ("our attack blocked", vec![act(A, t.oh1, Over)]),
            (
                "they dig and attack again, we win later",
                vec![
                    act(A, t.oh1, Negative),
                    act(D, t.libero, Positive),
                    act(A, t.oh1, Perfect),
                ],
            ),
            (
                "they dig and attack again, our block point",
                vec![act(A, t.oh1, Negative), act(B, t.mb2, Perfect)],
            ),
            (
                "they dig and attack again, our dig error",
                vec![act(A, t.oh1, Negative), act(D, t.libero, Error)],
            ),
        ];
        for (case, after) in cases {
            let events = [
                vec![t.serve(Negative), act(B, t.oh1, Positive), opp(OF)],
                after,
            ]
            .concat();
            let snapshot = t.serving(events);
            assert_eq!(exploited(&snapshot), vec![false], "{case}");
            assert_eq!(
                snapshot.stats.freeball_exploitation(&all()),
                Some((0.0, 1, 0)),
                "{case}"
            );
        }
    }

    /// Another freeball of theirs in the same rally: the first one wasn't
    /// exploited, the second one counts on its own.
    #[test]
    fn freeball_followed_by_another_one() {
        let t = Team::new();
        let snapshot = t.serving(vec![
            t.serve(Negative),
            act(B, t.oh1, Positive),
            opp(OF),
            act(A, t.oh1, Positive),
            opp(OF),
            act(A, t.oh1, Perfect),
        ]);
        assert_eq!(exploited(&snapshot), vec![false, true]);
        assert_eq!(
            snapshot.stats.freeball_exploitation(&all()),
            Some((50.0, 2, 1))
        );
        assert_eq!(snapshot.stats.event_count(OF, None, &all()), 2);
    }

    /// The freeballs exploited, by the phase and rotation they were played
    /// in; they have no player. A freeball in the rally being played isn't
    /// counted yet.
    #[test]
    fn freeball_exploitation_filters() {
        let t = Team::new();
        let snapshot = t.receiving(vec![
            act(P, t.libero, Over),
            opp(OF),
            act(A, t.oh1, Perfect),
            // break: outside hitter 1 serves, in the new rotation
            act(S, t.oh1, Negative),
            act(D, t.libero, Over),
            opp(OF),
        ]);
        let stats = &snapshot.stats;
        let side_out = StatsFilter::new(Some(PhaseEnum::SideOut), Some(0), None);
        assert_eq!(stats.freeball_exploitation(&side_out), Some((100.0, 1, 1)));
        assert_eq!(
            stats.freeball_exploitation(&StatsFilter::new(Some(PhaseEnum::Break), None, None)),
            None,
            "the rally isn't over"
        );
        assert_eq!(stats.freeball_exploitation(&by(t.oh1)), None);
        assert_eq!(stats.freeball_exploitation(&all()), Some((100.0, 1, 1)));
        assert_eq!(stats.event_count(OF, None, &all()), 2);
    }

    /// The freeballs of the sets add up in the match.
    #[test]
    fn freeball_exploitation_merges() {
        let t = Team::new();
        let won = t.serving(vec![
            t.serve(Negative),
            act(B, t.oh1, Positive),
            opp(OF),
            act(A, t.oh1, Perfect),
        ]);
        let lost = t.serving(vec![
            t.serve(Negative),
            act(B, t.oh1, Positive),
            opp(OF),
            opp(OS),
        ]);
        let mut stats = won.stats.clone();
        stats.merge(&lost.stats);
        assert_eq!(stats.freeball_exploitation(&all()), Some((50.0, 2, 1)));
    }

    /// The report shows the freeballs: the ones we gave in the statistics
    /// tables (by player too), and a table with the ones received and
    /// exploited, in all the rallies and by phase.
    #[test]
    fn report_shows_the_freeballs() {
        use crate::reporting::model::build_report;
        use crate::shapes::{r#match::MatchEntry, player::PlayerEntry, team::TeamEntry};
        use crate::tests::EN;
        let t = Team::new();
        let players = t
            .players()
            .iter()
            .enumerate()
            .map(|(i, id)| PlayerEntry {
                id: *id,
                name: format!("player {}", i + 1),
                number: i as u8 + 1,
                ..PlayerEntry::default()
            })
            .collect();
        // break: their freeball exploited, our freeball, their freeball
        // not exploited (they attack again)
        let set = t.set(
            TeamSideEnum::Us,
            vec![
                t.serve(Negative),
                act(B, t.oh1, Positive),
                opp(OF),
                act(A, t.oh1, Perfect),
                t.serve(Negative),
                act(D, t.libero, Positive),
                fb(),
                opp(OF),
                act(A, t.oh1, Negative),
                act(D, t.libero, Positive),
                act(A, t.oh1, Perfect),
            ],
        );
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
            roster: vec![],
            max_substitutions: DEFAULT_MAX_SUBSTITUTIONS,
            sets: vec![set],
            notes: String::new(),
        };
        let labels = EN;
        // a row for all the rallies, then one per phase
        let rows = build_report(&m, EN)
            .expect("expected the report")
            .freeballs
            .rows;
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], [labels.global, "1", "2", "1", "50"]);
        assert_eq!(rows[1], [labels.sideout, "-", "-", "-", "-"]);
        assert_eq!(rows[2], ["break", "1", "2", "1", "50"]);
    }
}
