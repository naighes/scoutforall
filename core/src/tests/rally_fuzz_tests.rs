//! The projection of a set scouted on both sides, fuzzed: whole sets of
//! random touches of either side, the ones `rally::admits` lets in. The
//! property: an admitted touch valid in the stream of its side never
//! breaks the other's, and the two sides always agree on the score.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::domain::Side;
    use crate::rally::{Incoherence, admits, replay};
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, TeamSideEnum},
        flags::EventFlags,
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
    };
    use EventTypeEnum::*;
    use chrono::{Duration, TimeZone, Utc};
    use std::collections::BTreeSet;
    use uuid::Uuid;

    /// xorshift64*: the same sets on every run, no dependency.
    struct Rng(u64);

    impl Rng {
        fn new(seed: u64) -> Self {
            Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
        }

        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % (n.max(1) as u64)) as usize
        }

        fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
            items.get(self.below(items.len())).copied()
        }

        /// One time out of `n`.
        fn one_in(&mut self, n: usize) -> bool {
            self.below(n) == 0
        }
    }

    /// Six on court in rotation 0, the libero, the second libero, three
    /// on the bench.
    struct Team {
        positions: [Uuid; 6],
        libero: Uuid,
        fallback_libero: Uuid,
        bench: [Uuid; 3],
    }

    impl Team {
        fn new() -> Self {
            Team {
                positions: [(); 6].map(|_| Uuid::new_v4()),
                libero: Uuid::new_v4(),
                fallback_libero: Uuid::new_v4(),
                bench: [(); 3].map(|_| Uuid::new_v4()),
            }
        }

        fn everyone(&self) -> Vec<Uuid> {
            self.positions
                .iter()
                .chain([self.libero, self.fallback_libero].iter())
                .chain(self.bench.iter())
                .copied()
                .collect()
        }

        fn start(&self, serving_team: TeamSideEnum) -> SetEntry {
            SetEntry {
                set_number: 1,
                serving_team,
                initial_positions: self.positions,
                libero: self.libero,
                fallback_libero: Some(self.fallback_libero),
                setter: self.positions[0],
                lineup_rules: Some(LineupRulesEnum::CURRENT),
                events: vec![],
            }
        }
    }

    struct Match {
        a: Team,
        b: Team,
        a_serves: bool,
    }

    impl Match {
        fn replay(
            &self,
            touches: &[(Side, EventEntry)],
        ) -> Result<(Snapshot, Snapshot), Incoherence> {
            let (a, b) = match self.a_serves {
                true => (TeamSideEnum::Us, TeamSideEnum::Them),
                false => (TeamSideEnum::Them, TeamSideEnum::Us),
            };
            replay(
                &self.a.start(a),
                &self.b.start(b),
                touches,
                DEFAULT_MAX_SUBSTITUTIONS,
            )
        }

        fn team(&self, side: Side) -> &Team {
            match side {
                Side::A => &self.a,
                Side::B => &self.b,
            }
        }
    }

    /// A touch the side can try now: one of the events its stream offers
    /// (not the ones the other side's touches tell), a player able to
    /// play it, an evaluation the event can have now.  Substitutions,
    /// faults, libero and setter changes now and then, as in a real set;
    /// the new setter the diagonal only, as the scouting offers it (the
    /// libero follows the setter).
    fn propose(rng: &mut Rng, snapshot: &Snapshot, team: &Team, n: usize) -> Option<EventEntry> {
        let offered: Vec<EventTypeEnum> = snapshot
            .get_available_options()
            .iter()
            .copied()
            .filter(|e| !matches!(e, OS | OE | OF))
            // the point without a cause, offered always, is rare
            .filter(|e| match e {
                PW | PL => rng.one_in(40),
                R | F | CL | LR | LF | CS => rng.one_in(8),
                _ => true,
            })
            .collect();
        let event_type = rng.pick(&offered)?;
        let players: Vec<Uuid> = team
            .everyone()
            .into_iter()
            .filter(|p| snapshot.can_play(event_type, p))
            .filter(|p| {
                event_type != CS || snapshot.current_lineup.get_setter_diagonal() == Some(*p)
            })
            .collect();
        let player = match (event_type, event_type.requires_player()) {
            (F, _) => rng.pick(&players).filter(|_| rng.one_in(2)),
            (_, true) => Some(rng.pick(&players)?),
            (_, false) => None,
        };
        Some(EventEntry {
            timestamp: Utc.with_ymd_and_hms(2026, 11, 14, 18, 0, 0).single()?
                + Duration::seconds(n as i64),
            event_type,
            player,
            eval: rng.pick(&snapshot.available_evals(event_type)),
            target_player: (event_type == R).then(|| rng.pick(&team.bench)).flatten(),
            technique: None,
            flags: EventFlags::default(),
        })
    }

    /// The last touches as codes, e.g. `B:S/ A:F`: for the failures.
    fn codes(touches: &[(Side, EventEntry)]) -> String {
        touches
            .iter()
            .skip(touches.len().saturating_sub(12))
            .map(|(side, e)| {
                let eval = e.eval.map(|v| v.to_string()).unwrap_or_default();
                format!("{side:?}:{}{eval}", e.event_type)
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The touches missing in the set, where the ball was in play: each
    /// touch of the ball followed by one not completing it, as `A+ > same
    /// A` (A's attack `+`, then A's attack again: B's dig untold). Complete
    /// after a serve is the reception of the other side; after an attack
    /// the dig or the block of the other side; after a block `+` or `-` the
    /// dig of the blockers, after a block `!` the cover of the attackers;
    /// after any other touch, a touch of the ball (attack, dig, block,
    /// freeball) of the side it left the ball to. A fault, a point without
    /// a cause, the block `/` (an invasion) and the touches after a point
    /// miss nothing.
    fn holes(touches: &[(Side, EventEntry)]) -> BTreeSet<String> {
        use EvalEnum::*;
        let ball: Vec<&(Side, EventEntry)> = touches
            .iter()
            .filter(|(_, e)| !matches!(e.event_type, R | CL | LR | LF | CS))
            .collect();
        ball.windows(2)
            .filter_map(|pair| {
                let [(side, prev), (next_side, next)] = pair else {
                    return None;
                };
                let held = prev.event_type.ball_side_after(prev.eval)?;
                let same = side == next_side;
                let to_ball_side = same == (held == TeamSideEnum::Us);
                let complete = match (prev.event_type, prev.eval, same, next.event_type) {
                    (_, _, _, F | PW | PL) => true,
                    (_, _, _, B) if next.eval == Some(Over) => true,
                    (S, _, other, P) | (A, _, other, D | B) => !other,
                    (B, Some(Positive | Negative), same, D) => same,
                    (B, Some(Exclamative), other, D) => !other,
                    (S | A | B, _, _, _) => false,
                    (_, _, _, A | D | B | FB) => to_ball_side,
                    _ => false,
                };
                let eval = prev.eval.map(|v| v.to_string()).unwrap_or_default();
                let who = if same { "same" } else { "other" };
                (!complete)
                    .then(|| format!("{}{eval} > {who} {}", prev.event_type, next.event_type))
            })
            .collect()
    }

    /// A whole set (or 400 touches), every touch checked on both sides,
    /// the score of both sides after every touch; the touches.
    fn play_set(seed: u64) -> Vec<(Side, EventEntry)> {
        let mut rng = Rng::new(seed);
        let game = Match {
            a: Team::new(),
            b: Team::new(),
            a_serves: rng.one_in(2),
        };
        let mut touches: Vec<(Side, EventEntry)> = vec![];
        let mut replayed = game.replay(&touches);
        for _ in 0..20000 {
            let (a, b) = match &replayed {
                Ok(snapshots) => snapshots,
                Err(e) => panic!("seed {seed}: {e:?} after {}", codes(&touches)),
            };
            // the two sides tell every point at the same touch: the same
            // points in the same order, not only the same score at the end
            assert_eq!(
                (a.score_us, a.score_them),
                (b.score_them, b.score_us),
                "seed {seed}: {}",
                codes(&touches)
            );
            if a.get_set_winner(1).is_some() || touches.len() >= 400 {
                return touches;
            }
            // either side tries, the rule of the touches decides
            let side = match rng.one_in(2) {
                true => Side::A,
                false => Side::B,
            };
            let snapshot = match side {
                Side::A => a,
                Side::B => b,
            };
            let Some(entry) = propose(&mut rng, snapshot, game.team(side), touches.len()) else {
                continue;
            };
            if !admits(&touches, a, b, side, &entry) {
                continue;
            }
            let mut next = touches.clone();
            next.push((side, entry));
            let source = touches.len();
            match game.replay(&next) {
                Ok(snapshots) => {
                    touches = next;
                    replayed = Ok(snapshots);
                }
                // not valid in its own stream (e.g. a substitution the
                // rules refuse): the scouting wouldn't offer it
                Err(Incoherence::Refused {
                    side: refused,
                    source: s,
                    ..
                }) if refused == side && s == source => {}
                Err(e) => panic!("seed {seed}: {e:?}, touch {source}: {}", codes(&next)),
            }
        }
        touches
    }

    #[test]
    fn random_sets_replay_on_both_sides() {
        let sets: Vec<Vec<(Side, EventEntry)>> = (0..200).map(play_set).collect();
        // the sets get played to their end, with every kind of touch, of
        // both sides
        let unfinished = sets.iter().filter(|touches| touches.len() >= 400).count();
        assert!(unfinished < 5, "{unfinished} sets not finished");
        let played: std::collections::HashSet<(Side, EventTypeEnum)> = sets
            .iter()
            .flatten()
            .map(|(side, e)| (*side, e.event_type))
            .collect();
        // the touches that can go untold (TODO/001, §4): the dig or the
        // block of the attack, the dig `/` after the attack `+` (the
        // roads open), the touch after the block, the block `!` and the
        // cover after the attack `!`; any other hole is a new one
        let found: BTreeSet<String> = sets.iter().flat_map(|touches| holes(touches)).collect();
        let expected: BTreeSet<String> = [
            "A- > other A",
            "A- > other FB",
            "A+ > other A",
            "A+ > other FB",
            "A+ > same A",
            "A+ > same FB",
            "A! > same D",
            "A! > same A",
            "A! > same FB",
            "B+ > same A",
            "B+ > same FB",
            "B- > same A",
            "B- > same FB",
            "B! > other A",
            "B! > other FB",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(found, expected);
        for side in Side::ALL {
            for event_type in [S, P, A, D, B, F, FB, PW, PL, R, CL, LR, LF, CS] {
                assert!(
                    played.contains(&(side, event_type)),
                    "{side:?} {event_type}"
                );
            }
        }
    }
}
