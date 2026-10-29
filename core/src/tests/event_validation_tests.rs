//! The events are validated by the snapshot, not by the scouting screen: a
//! set written by hand (or imported) breaking a rule doesn't replay, as the
//! scouting wouldn't have let it be scouted. Up to v0.4.1 only the scouting
//! checked the order of the events, their evaluation and their players.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::errors::{AppError, SnapshotError};
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, TeamSideEnum},
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
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
        fallback_libero: Uuid,
        subs: [Uuid; 3],
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
                fallback_libero: Uuid::new_v4(),
                subs: [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()],
            }
        }

        /// Rotation 0: the setter in position 1, outside hitter 2 in
        /// position 5 (back row), the libero in 6 for middle blocker 1.
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
                fallback_libero: Some(self.fallback_libero),
                setter: self.setter,
                lineup_rules: Some(LineupRulesEnum::CURRENT),
                events,
            }
        }

        fn replay(
            &self,
            serving_team: TeamSideEnum,
            events: Vec<EventEntry>,
        ) -> Result<Snapshot, AppError> {
            self.set(serving_team, events)
                .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
        }

        /// Them serving: we're in side out.
        fn receiving(&self, events: Vec<EventEntry>) -> Result<Snapshot, AppError> {
            self.replay(TeamSideEnum::Them, events)
        }

        /// Us serving: we're in break.
        fn serving(&self, events: Vec<EventEntry>) -> Result<Snapshot, AppError> {
            self.replay(TeamSideEnum::Us, events)
        }

        fn players(&self) -> Vec<Uuid> {
            let mut players = vec![
                self.setter,
                self.oh1,
                self.mb2,
                self.opposite,
                self.oh2,
                self.mb1,
                self.libero,
                self.fallback_libero,
            ];
            players.extend(self.subs);
            players
        }
    }

    fn ev(
        event_type: EventTypeEnum,
        player: Option<Uuid>,
        eval: Option<EvalEnum>,
        target_player: Option<Uuid>,
    ) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type,
            player,
            eval,
            target_player,
            technique: None,
            flags: Default::default(),
        }
    }

    fn act(event_type: EventTypeEnum, player: Uuid, eval: EvalEnum) -> EventEntry {
        ev(event_type, Some(player), Some(eval), None)
    }

    fn opp(event_type: EventTypeEnum) -> EventEntry {
        ev(event_type, None, None, None)
    }

    fn sub(out: Uuid, r#in: Uuid) -> EventEntry {
        ev(R, Some(out), None, Some(r#in))
    }

    /// The set replays up to the last event, which is rejected as invalid.
    fn assert_invalid(result: Result<Snapshot, AppError>, reason: &str) {
        match result {
            Err(AppError::Snapshot(SnapshotError::InvalidEvent(error))) => {
                assert!(error.contains(reason), "{error}")
            }
            Err(e) => panic!("expected an invalid event ({reason}), got {e}"),
            Ok(_) => panic!("expected an invalid event ({reason})"),
        }
    }

    fn assert_valid(result: Result<Snapshot, AppError>) -> Snapshot {
        result.unwrap_or_else(|e| panic!("expected the set to replay: {e}"))
    }

    // --- the order of the events ------------------------------------------

    /// Before their serve, we can't attack.
    #[test]
    fn event_not_available() {
        let t = Team::new();
        assert_invalid(t.receiving(vec![act(A, t.oh1, Perfect)]), "not available");
        assert_valid(t.receiving(vec![act(P, t.libero, Perfect), act(A, t.oh1, Perfect)]));
    }

    /// When we serve, they can't score an ace.
    #[test]
    fn their_ace_while_we_serve() {
        let t = Team::new();
        assert_invalid(t.serving(vec![opp(OS)]), "not available");
        assert_invalid(t.receiving(vec![opp(OE), opp(OS)]), "not available");
        assert_valid(t.receiving(vec![opp(OS)]));
    }

    /// After a reception the serve can't come again, and a substitution must
    /// wait for the end of the rally.
    #[test]
    fn events_out_of_the_rally() {
        let t = Team::new();
        let reception = act(P, t.libero, Positive);
        assert_invalid(
            t.receiving(vec![reception.clone(), act(S, t.setter, Positive)]),
            "not available",
        );
        assert_invalid(
            t.receiving(vec![reception.clone(), sub(t.oh1, t.subs[0])]),
            "not available",
        );
        assert_invalid(
            t.receiving(vec![
                reception.clone(),
                ev(CS, Some(t.opposite), None, None),
            ]),
            "not available",
        );
        assert_invalid(t.receiving(vec![reception, opp(CL)]), "not available");
    }

    /// The substitutions and the libero and setter changes keep the events
    /// available before them.
    #[test]
    fn changes_keep_the_available_events() {
        let t = Team::new();
        let snapshot = assert_valid(t.receiving(vec![]));
        let before = snapshot.get_available_options().to_vec();
        let snapshot = assert_valid(t.receiving(vec![
            sub(t.oh1, t.subs[0]),
            opp(CL),
            ev(CS, Some(t.opposite), None, None),
        ]));
        assert_eq!(snapshot.get_available_options(), before);
    }

    /// Once a team wins the set, nothing can follow.
    #[test]
    fn nothing_after_the_end_of_the_set() {
        let t = Team::new();
        let won: Vec<EventEntry> = (0..25).map(|_| opp(OE)).collect();
        let snapshot = assert_valid(t.serving(won.clone()));
        assert_eq!(snapshot.score_us, 25);
        let mut after = won.clone();
        after.push(opp(OE));
        assert_invalid(t.serving(after), "the set is over");
        let mut after = won;
        after.push(sub(t.oh1, t.subs[0]));
        assert_invalid(t.serving(after), "the set is over");
    }

    /// At 24-24 the set goes on: two points ahead are needed.
    #[test]
    fn the_set_goes_on_at_24_all() {
        let t = Team::new();
        // their serve error, then our fault on our serve
        let mut events = vec![];
        for _ in 0..24 {
            events.push(opp(OE));
            events.push(ev(F, Some(t.opposite), None, None));
        }
        events.push(opp(OE));
        let snapshot = assert_valid(t.receiving(events.clone()));
        assert_eq!((snapshot.score_us, snapshot.score_them), (25, 24));
        events.push(opp(OE));
        let snapshot = assert_valid(t.receiving(events.clone()));
        assert_eq!((snapshot.score_us, snapshot.score_them), (26, 24));
        events.push(opp(OE));
        assert_invalid(t.receiving(events), "the set is over");
    }

    // --- the evaluation ---------------------------------------------------

    #[test]
    fn missing_evaluation() {
        let t = Team::new();
        assert_invalid(
            t.serving(vec![ev(S, Some(t.setter), None, None)]),
            "missing evaluation",
        );
        assert_invalid(
            t.receiving(vec![ev(P, Some(t.oh1), None, None)]),
            "missing evaluation",
        );
        assert_invalid(
            t.receiving(vec![
                act(P, t.libero, Positive),
                ev(A, Some(t.oh1), None, None),
            ]),
            "missing evaluation",
        );
    }

    /// Serve and block have no `!`.
    #[test]
    fn evaluation_not_allowed() {
        let t = Team::new();
        assert_invalid(
            t.serving(vec![act(S, t.setter, Exclamative)]),
            "evaluation ! not allowed",
        );
        assert_invalid(
            t.serving(vec![act(S, t.setter, Negative), act(B, t.oh1, Exclamative)]),
            "evaluation ! not allowed",
        );
        assert_valid(t.receiving(vec![act(P, t.libero, Exclamative)]));
    }

    #[test]
    fn unexpected_evaluation() {
        let t = Team::new();
        assert_invalid(
            t.receiving(vec![ev(OE, None, Some(Perfect), None)]),
            "unexpected evaluation",
        );
        assert_invalid(
            t.receiving(vec![ev(F, Some(t.oh1), Some(Error), None)]),
            "unexpected evaluation",
        );
        assert_invalid(
            t.receiving(vec![ev(R, Some(t.oh1), Some(Error), Some(t.subs[0]))]),
            "unexpected evaluation",
        );
    }

    // --- the players ------------------------------------------------------

    #[test]
    fn missing_player() {
        let t = Team::new();
        for event in [
            ev(P, None, Some(Perfect), None),
            ev(F, None, None, None),
            ev(CS, None, None, None),
            ev(R, None, None, Some(t.subs[0])),
        ] {
            assert_invalid(t.receiving(vec![event]), "missing player");
        }
    }

    #[test]
    fn unexpected_player() {
        let t = Team::new();
        assert_invalid(
            t.receiving(vec![ev(OE, Some(t.oh1), None, None)]),
            "unexpected player",
        );
        assert_invalid(
            t.receiving(vec![ev(OS, Some(t.oh1), None, None)]),
            "unexpected player",
        );
        assert_invalid(
            t.receiving(vec![ev(CL, Some(t.libero), None, None)]),
            "unexpected player",
        );
    }

    /// Only a player on court plays: not a substitute, not the libero
    /// resting, not someone out of the team.
    #[test]
    fn player_not_on_court() {
        let t = Team::new();
        for player in [t.subs[0], t.fallback_libero, Uuid::new_v4()] {
            assert_invalid(t.receiving(vec![act(P, player, Perfect)]), "can't play it");
            assert_invalid(
                t.receiving(vec![ev(F, Some(player), None, None)]),
                "can't play it",
            );
            assert_invalid(
                t.receiving(vec![ev(CS, Some(player), None, None)]),
                "can't play it",
            );
        }
        // middle blocker 1 is resting: the libero plays for them
        assert_invalid(t.receiving(vec![act(P, t.mb1, Perfect)]), "can't play it");
    }

    /// The libero receives, digs and makes faults, but doesn't attack,
    /// block or become the setter.
    #[test]
    fn what_the_libero_plays() {
        let t = Team::new();
        let reception = act(P, t.libero, Positive);
        assert_valid(t.receiving(vec![reception.clone()]));
        assert_valid(t.receiving(vec![ev(F, Some(t.libero), None, None)]));
        assert_valid(t.serving(vec![act(S, t.setter, Negative), act(D, t.libero, Positive)]));
        assert_invalid(
            t.receiving(vec![reception, act(A, t.libero, Perfect)]),
            "can't play it",
        );
        assert_invalid(
            t.serving(vec![act(S, t.setter, Negative), act(B, t.libero, Perfect)]),
            "can't play it",
        );
        assert_invalid(
            t.receiving(vec![ev(CS, Some(t.libero), None, None)]),
            "can't play it",
        );
    }

    /// Only the front row blocks; anyone on court but the libero attacks.
    #[test]
    fn back_row_player_does_not_block() {
        let t = Team::new();
        let serve = act(S, t.setter, Negative);
        for front in [t.oh1, t.mb2, t.opposite] {
            assert_valid(t.serving(vec![serve.clone(), act(B, front, Perfect)]));
        }
        for back in [t.setter, t.oh2] {
            assert_invalid(
                t.serving(vec![serve.clone(), act(B, back, Perfect)]),
                "can't play it",
            );
            assert_valid(t.serving(vec![
                serve.clone(),
                act(D, t.libero, Positive),
                act(A, back, Perfect),
            ]));
        }
    }

    /// The serve is of the player in position 1, after each rotation too.
    #[test]
    fn serve_of_the_player_in_position_1() {
        let t = Team::new();
        assert_valid(t.serving(vec![act(S, t.setter, Positive)]));
        assert_invalid(t.serving(vec![act(S, t.oh1, Positive)]), "can't play it");
        // side out won: we rotate, outside hitter 1 serves
        let side_out = vec![opp(OE)];
        let mut events = side_out.clone();
        events.push(act(S, t.setter, Positive));
        assert_invalid(t.receiving(events), "can't play it");
        let mut events = side_out;
        events.push(act(S, t.oh1, Positive));
        assert_valid(t.receiving(events));
    }

    // --- the substitutions ------------------------------------------------

    #[test]
    fn substitution_needs_who_comes_in() {
        let t = Team::new();
        assert_invalid(
            t.receiving(vec![ev(R, Some(t.oh1), None, None)]),
            "missing replacement",
        );
        assert_valid(t.receiving(vec![sub(t.oh1, t.subs[0])]));
    }

    /// Who comes in isn't on court, and isn't a libero.
    #[test]
    fn who_comes_in() {
        let t = Team::new();
        for r#in in [t.opposite, t.libero, t.fallback_libero] {
            assert_invalid(t.receiving(vec![sub(t.oh1, r#in)]), "can't come in");
        }
    }

    /// Who goes out is on court.
    #[test]
    fn who_goes_out() {
        let t = Team::new();
        assert_invalid(
            t.receiving(vec![sub(t.subs[1], t.subs[0])]),
            "can't play it",
        );
        assert_invalid(t.receiving(vec![sub(t.mb1, t.subs[0])]), "can't play it");
    }

    #[test]
    fn unexpected_replacement() {
        let t = Team::new();
        assert_invalid(
            t.receiving(vec![ev(P, Some(t.libero), Some(Perfect), Some(t.subs[0]))]),
            "unexpected replacement",
        );
        assert_invalid(
            t.receiving(vec![ev(CS, Some(t.opposite), None, Some(t.oh1))]),
            "unexpected replacement",
        );
    }

    /// The rules of the substitutions are still the ones of the lineup.
    #[test]
    fn substitution_rules_of_the_lineup() {
        let t = Team::new();
        let result = t.receiving(vec![sub(t.oh1, t.subs[0]), sub(t.subs[0], t.subs[1])]);
        assert!(
            matches!(
                result,
                Err(AppError::Snapshot(SnapshotError::LineupError(_)))
            ),
            "{:?}",
            result.err()
        );
    }

    // --- an invalid event changes nothing ---------------------------------

    struct Seen {
        score: (u8, u8),
        options: Vec<EventTypeEnum>,
        serving: Option<Uuid>,
        rotation: u8,
        events: u32,
        last_event: Option<EventTypeEnum>,
    }

    fn seen(snapshot: &Snapshot) -> Seen {
        Seen {
            score: (snapshot.score_us, snapshot.score_them),
            options: snapshot.get_available_options().to_vec(),
            serving: snapshot.current_lineup.get_serving_player(),
            rotation: snapshot.current_lineup.get_current_rotation().unwrap(),
            events: snapshot.stats.events.count(|_| true),
            last_event: snapshot.last_event.as_ref().map(|e| e.event_type),
        }
    }

    fn assert_unchanged(before: &Seen, snapshot: &Snapshot) {
        let after = seen(snapshot);
        assert_eq!(before.score, after.score);
        assert_eq!(before.options, after.options);
        assert_eq!(before.serving, after.serving);
        assert_eq!(before.rotation, after.rotation);
        assert_eq!(before.events, after.events);
        assert_eq!(before.last_event, after.last_event);
    }

    /// Neither an invalid event nor a substitution the lineup refuses
    /// changes the snapshot: the scouting can go on from it.
    #[test]
    fn invalid_event_changes_nothing() {
        let t = Team::new();
        let mut snapshot = assert_valid(t.receiving(vec![sub(t.oh1, t.subs[0])]));
        let before = seen(&snapshot);
        for event in [
            act(A, t.oh1, Perfect),
            act(P, t.subs[1], Perfect),
            ev(OE, None, Some(Perfect), None),
            // the lineup refuses it: the substitute can only go out for oh1
            sub(t.subs[0], t.subs[1]),
        ] {
            assert!(snapshot.add_event(&event).is_err(), "{event}");
            assert_unchanged(&before, &snapshot);
        }
        snapshot
            .add_event(&opp(OE))
            .expect("expected their error to apply");
        assert_eq!(snapshot.score_us, 1);
    }

    // --- what the scouting offers is valid --------------------------------

    /// A tiny deterministic generator: the tests don't depend on a crate.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self, n: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 33) as usize) % n
        }
    }

    /// The events the scouting would offer: the available ones, each with
    /// the players who can play it and the evaluations it allows.
    fn offered(t: &Team, snapshot: &Snapshot) -> Vec<EventEntry> {
        let lineup = &snapshot.current_lineup;
        let can_come_in = |p: &Uuid| {
            lineup.find_position(p).is_none()
                && *p != lineup.get_current_libero()
                && Some(*p) != lineup.get_fallback_libero()
        };
        snapshot
            .get_available_options()
            .iter()
            .flat_map(|&event_type| {
                let players: Vec<Option<Uuid>> = match event_type.requires_player() {
                    true => t
                        .players()
                        .into_iter()
                        .filter(|p| snapshot.can_play(event_type, p))
                        .map(Some)
                        .collect(),
                    false => vec![None],
                };
                let evals: Vec<Option<EvalEnum>> = match event_type.requires_evaluation() {
                    true => event_type.available_evals().into_iter().map(Some).collect(),
                    false => vec![None],
                };
                let targets: Vec<Option<Uuid>> = match event_type {
                    R => t
                        .players()
                        .into_iter()
                        .filter(can_come_in)
                        .map(Some)
                        .collect(),
                    _ => vec![None],
                };
                players
                    .into_iter()
                    .flat_map(move |player| {
                        let targets = targets.clone();
                        evals.clone().into_iter().flat_map(move |eval| {
                            targets
                                .clone()
                                .into_iter()
                                .map(move |target| ev(event_type, player, eval, target))
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Random sets scouted the way the scouting allows: every event offered
    /// is valid (only the lineup can refuse a substitution, changing
    /// nothing), and the saved set replays to the same snapshot.
    #[test]
    fn scouted_sets_replay() {
        let mut rng = Lcg(2026);
        for round in 0..200 {
            let t = Team::new();
            let serving_team = match round % 2 {
                0 => TeamSideEnum::Us,
                _ => TeamSideEnum::Them,
            };
            let mut snapshot = assert_valid(t.replay(serving_team, vec![]));
            let mut events = vec![];
            while snapshot.get_set_winner(1).is_none() {
                let offered = offered(&t, &snapshot);
                assert!(!offered.is_empty(), "nothing to scout after {events:?}");
                // the changes are rare, as in a real set
                let rally: Vec<&EventEntry> = offered
                    .iter()
                    .filter(|e| !matches!(e.event_type, R | CL | CS))
                    .collect();
                let event = match (rng.next(10), rally.is_empty()) {
                    (0, _) | (_, true) => offered[rng.next(offered.len())].clone(),
                    _ => rally[rng.next(rally.len())].clone(),
                };
                let before = seen(&snapshot);
                match snapshot.add_event(&event) {
                    Ok(()) => events.push(event),
                    Err(AppError::Snapshot(SnapshotError::LineupError(_)))
                        if event.event_type == R =>
                    {
                        assert_unchanged(&before, &snapshot)
                    }
                    Err(e) => panic!("offered {event} refused: {e}"),
                }
            }
            let replayed = assert_valid(t.replay(serving_team, events.clone()));
            assert_eq!(
                (replayed.score_us, replayed.score_them),
                (snapshot.score_us, snapshot.score_them)
            );
            assert_eq!(
                replayed.current_lineup.get_serving_player(),
                snapshot.current_lineup.get_serving_player()
            );
            // and nothing can follow
            events.push(opp(OE));
            assert_invalid(t.replay(serving_team, events), "the set is over");
        }
    }
}
