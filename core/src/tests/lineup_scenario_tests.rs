//! Scouting scenarios around the lineup: the libero going in and out, setter
//! changes and substitutions, replayed through `Snapshot::add_event` like the
//! scouting screen does.

#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, PhaseEnum, RoleEnum, TeamSideEnum},
        flags::EventFlags,
        player::PlayerEntry,
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        team::TeamEntry,
    };
    use chrono::Utc;
    use uuid::Uuid;

    /// Starting lineup in rotation 0: setter in position 1.
    struct Players {
        setter: Uuid,
        oh1: Uuid,
        mb2: Uuid,
        opposite: Uuid,
        oh2: Uuid,
        mb1: Uuid,
        libero: Uuid,
        fallback_libero: Uuid,
        bench: [Uuid; 4],
    }

    impl Players {
        fn new() -> Self {
            Players {
                setter: Uuid::new_v4(),
                oh1: Uuid::new_v4(),
                mb2: Uuid::new_v4(),
                opposite: Uuid::new_v4(),
                oh2: Uuid::new_v4(),
                mb1: Uuid::new_v4(),
                libero: Uuid::new_v4(),
                fallback_libero: Uuid::new_v4(),
                bench: [
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                ],
            }
        }

        /// The lineup with the setter in position `rotation + 1`.
        fn positions(&self, rotation: usize) -> [Uuid; 6] {
            let mut positions = [
                self.setter,
                self.oh1,
                self.mb2,
                self.opposite,
                self.oh2,
                self.mb1,
            ];
            positions.rotate_right(rotation);
            positions
        }

        fn team(&self) -> TeamEntry {
            let mut ids = vec![
                (self.setter, RoleEnum::Setter),
                (self.oh1, RoleEnum::OutsideHitter),
                (self.mb2, RoleEnum::MiddleBlocker),
                (self.opposite, RoleEnum::OppositeHitter),
                (self.oh2, RoleEnum::OutsideHitter),
                (self.mb1, RoleEnum::MiddleBlocker),
                (self.libero, RoleEnum::Libero),
                (self.fallback_libero, RoleEnum::Libero),
            ];
            ids.extend(self.bench.iter().map(|id| (*id, RoleEnum::OutsideHitter)));
            TeamEntry {
                name: "Volley Club".to_owned(),
                players: ids
                    .into_iter()
                    .enumerate()
                    .map(|(i, (id, role))| PlayerEntry {
                        id,
                        name: format!("player {}", i + 1),
                        role: Some(role),
                        number: i as u8 + 1,
                        deleted: false,
                        birth_date: None,
                        height_cm: None,
                        notes: String::new(),
                    })
                    .collect(),
                ..TeamEntry::default()
            }
        }

        fn set(&self, rotation: usize, serving_team: TeamSideEnum) -> SetEntry {
            SetEntry::new(
                1,
                serving_team,
                self.positions(rotation),
                self.libero,
                Some(self.fallback_libero),
                self.setter,
            )
            .expect("expected a valid set")
        }
    }

    fn event(event_type: EventTypeEnum, player: Option<Uuid>, target: Option<Uuid>) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type,
            player,
            eval: None,
            target_player: target,
            technique: None,
            flags: Default::default(),
        }
    }

    struct Scout {
        snapshot: Snapshot,
        events: Vec<EventEntry>,
    }

    impl Scout {
        fn new(set: &SetEntry) -> Self {
            let snapshot = set
                .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
                .expect("expected a snapshot");
            Scout {
                snapshot,
                events: set.events.clone(),
            }
        }

        fn options(&self) -> Vec<EventTypeEnum> {
            self.snapshot.get_available_options().to_vec()
        }

        fn add(&mut self, event: EventEntry) {
            assert!(
                self.options().contains(&event.event_type),
                "{:?} not available among {:?}",
                event.event_type,
                self.options()
            );
            self.snapshot
                .add_event(&event)
                .expect("expected a valid event");
            self.events.push(event);
        }

        /// Opponent error: we score, and rotate if we were receiving.
        fn we_score(&mut self) {
            self.add(event(EventTypeEnum::OE, None, None));
        }

        /// They score, the serve goes to them: a serve error when we serve,
        /// an opponent point otherwise.
        fn they_score(&mut self) {
            if self.phase() == PhaseEnum::Break {
                let server = self.snapshot.current_lineup.get_serving_player();
                self.add(EventEntry {
                    eval: Some(EvalEnum::Error),
                    ..event(EventTypeEnum::S, server, None)
                });
            } else {
                self.add(event(EventTypeEnum::OS, None, None));
            }
        }

        fn substitute(&mut self, replaced: Uuid, replacement: Uuid) {
            self.add(event(EventTypeEnum::R, Some(replaced), Some(replacement)));
        }

        fn on_court(&self) -> Vec<Uuid> {
            (0..6)
                .map(|i| self.snapshot.current_lineup.get(i).expect("a player"))
                .collect()
        }

        fn rotation(&self) -> u8 {
            self.snapshot
                .current_lineup
                .get_current_rotation()
                .expect("expected a rotation")
        }

        fn phase(&self) -> PhaseEnum {
            self.snapshot.current_lineup.get_current_phase()
        }

        /// The libero plays only in the back row and never serves; the
        /// middle blockers never play together with the libero in the back
        /// row, and nobody is on court twice.
        fn assert_libero_rules(&self, libero: Uuid, middles: [Uuid; 2], context: &str) {
            let court = self.on_court();
            let mut distinct = court.clone();
            distinct.sort();
            distinct.dedup();
            assert_eq!(distinct.len(), 6, "{context}: duplicate players {court:?}");
            if let Some(pos) = court.iter().position(|p| *p == libero) {
                assert!(
                    [0, 4, 5].contains(&pos),
                    "{context}: libero in the front row (position {})",
                    pos + 1
                );
                assert!(
                    !(pos == 0 && self.phase() == PhaseEnum::Break),
                    "{context}: libero serving"
                );
                let middles_on_court = middles.iter().filter(|m| court.contains(m)).count();
                assert_eq!(
                    middles_on_court, 1,
                    "{context}: libero on court with {middles_on_court} middle blockers"
                );
            } else {
                for m in middles {
                    assert!(
                        court.contains(&m),
                        "{context}: libero and a middle blocker both off court"
                    );
                }
            }
        }
    }

    /// Plays two full turns of rotations from every starting rotation, both
    /// serving and receiving, checking the libero rules after each rally.
    #[test]
    fn libero_rules_hold_from_any_starting_rotation() {
        for rotation in 0..6 {
            for serving in [TeamSideEnum::Us, TeamSideEnum::Them] {
                let p = Players::new();
                let mut scout = Scout::new(&p.set(rotation, serving));
                let context = format!("start rotation {}, {:?} serving", rotation + 1, serving);
                scout.assert_libero_rules(p.libero, [p.mb1, p.mb2], &context);
                for rally in 0..24 {
                    // side out, then a point on serve, then lose the serve
                    match rally % 3 {
                        0 | 1 => scout.we_score(),
                        _ => scout.they_score(),
                    }
                    let context = format!("{context}, rally {}", rally + 1);
                    scout.assert_libero_rules(p.libero, [p.mb1, p.mb2], &context);
                }
            }
        }
    }

    /// Serving first with the middle blocker serving, the libero isn't on
    /// court yet: it goes in for the server on the side out, and must leave
    /// the court when it would rotate to the front row.
    #[test]
    fn libero_leaves_the_front_row_when_the_set_starts_with_a_middle_serving() {
        let p = Players::new();
        // setter in position 2: middle blocker 1 serves
        let mut scout = Scout::new(&p.set(1, TeamSideEnum::Us));
        assert_eq!(scout.on_court()[0], p.mb1);
        assert_eq!(scout.snapshot.current_lineup.find_position(&p.libero), None);
        // the serve is lost: the libero goes in for the server
        scout.they_score();
        assert_eq!(scout.on_court()[0], p.libero);
        // three side outs: the libero would rotate to position 4
        for _ in 0..2 {
            scout.we_score();
            scout.they_score();
        }
        scout.we_score();
        assert_eq!(scout.rotation(), 4);
        assert_eq!(scout.phase(), PhaseEnum::Break);
        assert_eq!(scout.snapshot.current_lineup.find_position(&p.libero), None);
        assert_eq!(scout.on_court()[0], p.mb2, "the other middle serves");
        assert_eq!(scout.on_court()[3], p.mb1, "the middle is back in front");
    }

    /// The middle blocker who replaces the libero when it leaves the court
    /// was substituted: the substitute takes the court, not the player who
    /// left it.
    #[test]
    fn substituted_middle_blocker_replaces_the_libero() {
        let p = Players::new();
        let sub = p.bench[0];
        let mut scout = Scout::new(&p.set(0, TeamSideEnum::Them));
        assert_eq!(scout.on_court()[5], p.libero);
        // middle blocker 2 is in the front row
        scout.substitute(p.mb2, sub);
        assert_eq!(scout.on_court()[2], sub);
        // rotate until the libero would go to the front row
        scout.we_score();
        scout.they_score();
        scout.we_score();
        assert_eq!(scout.rotation(), 4);
        assert_eq!(scout.phase(), PhaseEnum::Break);
        assert_eq!(scout.on_court()[0], sub, "the substitute serves");
        assert_eq!(scout.on_court()[3], p.mb1);
        assert!(
            !scout.on_court().contains(&p.mb2),
            "the substituted middle is back on court: {:?}",
            scout.on_court()
        );
        scout.assert_libero_rules(p.libero, [p.mb1, sub], "after the substitution");
    }

    /// A middle blocker substituted while serving is the one the libero
    /// replaces on the side out.
    #[test]
    fn libero_replaces_the_substituted_server() {
        let p = Players::new();
        let sub = p.bench[0];
        let mut scout = Scout::new(&p.set(1, TeamSideEnum::Us));
        scout.substitute(p.mb1, sub);
        assert_eq!(scout.on_court()[0], sub);
        scout.they_score();
        assert_eq!(scout.on_court()[0], p.libero);
        assert_eq!(scout.snapshot.current_lineup.get_mb1(), Some(sub));
        // until the substitute is back on court in place of the libero
        for _ in 0..2 {
            scout.we_score();
            scout.they_score();
        }
        scout.we_score();
        assert_eq!(scout.rotation(), 4);
        assert_eq!(scout.on_court()[3], sub);
        scout.assert_libero_rules(p.libero, [sub, p.mb2], "around the court");
    }

    #[test]
    fn libero_change_on_court() {
        let p = Players::new();
        let mut scout = Scout::new(&p.set(0, TeamSideEnum::Them));
        scout.we_score();
        scout.add(event(EventTypeEnum::CL, None, None));
        assert_eq!(scout.on_court()[4], p.fallback_libero);
        assert!(!scout.on_court().contains(&p.libero));
        assert_eq!(
            scout.snapshot.current_lineup.get_current_libero(),
            p.fallback_libero
        );
        // and back
        scout.add(event(EventTypeEnum::CL, None, None));
        assert_eq!(scout.on_court()[4], p.libero);
        assert!(!scout.on_court().contains(&p.fallback_libero));
    }

    /// The libero is changed while the middle serves: the new libero goes
    /// in on the side out.
    #[test]
    fn libero_change_off_court() {
        let p = Players::new();
        let mut scout = Scout::new(&p.set(1, TeamSideEnum::Us));
        scout.add(event(EventTypeEnum::CL, None, None));
        assert!(!scout.on_court().contains(&p.libero));
        assert!(!scout.on_court().contains(&p.fallback_libero));
        scout.they_score();
        assert_eq!(scout.on_court()[0], p.fallback_libero);
        scout.assert_libero_rules(p.fallback_libero, [p.mb1, p.mb2], "after the change");
    }

    /// Both liberos played: both are among the players of the set, as the
    /// report lists them.
    #[test]
    fn both_liberos_are_involved_after_a_libero_change() {
        let p = Players::new();
        let mut scout = Scout::new(&p.set(0, TeamSideEnum::Them));
        scout.we_score();
        scout.add(event(EventTypeEnum::CL, None, None));
        let involved = scout.snapshot.current_lineup.get_involved_players();
        assert!(involved.contains(&p.libero), "first libero missing");
        assert!(
            involved.contains(&p.fallback_libero),
            "second libero missing"
        );
    }

    #[test]
    fn libero_is_not_involved_without_a_change() {
        let p = Players::new();
        let scout = Scout::new(&p.set(0, TeamSideEnum::Them));
        let involved = scout.snapshot.current_lineup.get_involved_players();
        assert!(involved.contains(&p.libero));
        assert!(!involved.contains(&p.fallback_libero));
    }

    /// Neither libero can enter as a regular substitute.
    #[test]
    fn liberos_are_not_replacements() {
        let p = Players::new();
        let team = p.team();
        let mut scout = Scout::new(&p.set(0, TeamSideEnum::Them));
        let ids = |scout: &Scout| -> Vec<Uuid> {
            scout
                .snapshot
                .current_lineup
                .get_available_replacements(&team, p.oh1)
                .iter()
                .map(|(_, player)| player.id)
                .collect()
        };
        assert_eq!(ids(&scout), p.bench.to_vec());
        scout.we_score();
        scout.add(event(EventTypeEnum::CL, None, None));
        assert_eq!(ids(&scout), p.bench.to_vec());
    }

    /// Players removed from the team can't enter the court.
    #[test]
    fn deleted_players_are_not_replacements() {
        let p = Players::new();
        let mut team = p.team();
        team.players
            .iter_mut()
            .find(|player| player.id == p.bench[1])
            .expect("a bench player")
            .deleted = true;
        let scout = Scout::new(&p.set(0, TeamSideEnum::Them));
        let replacements: Vec<Uuid> = scout
            .snapshot
            .current_lineup
            .get_available_replacements(&team, p.oh1)
            .iter()
            .map(|(_, player)| player.id)
            .collect();
        assert_eq!(replacements, vec![p.bench[0], p.bench[2], p.bench[3]]);
    }

    /// Changing the setter to the opposite moves the roles, not the players.
    #[test]
    fn setter_change_to_the_opposite() {
        let p = Players::new();
        let mut scout = Scout::new(&p.set(0, TeamSideEnum::Them));
        scout.add(event(EventTypeEnum::CS, Some(p.opposite), None));
        let lineup = &scout.snapshot.current_lineup;
        assert_eq!(lineup.get_setter(), Some(p.opposite));
        assert_eq!(lineup.get_opposite(), Some(p.setter));
        assert_eq!(lineup.get_role(&p.opposite).unwrap(), RoleEnum::Setter);
        assert_eq!(
            lineup.get_role(&p.setter).unwrap(),
            RoleEnum::OppositeHitter
        );
        assert_eq!(scout.rotation(), 3);
        assert_eq!(lineup.get_mb1(), Some(p.mb2));
        assert_eq!(lineup.get_mb2(), Some(p.mb1));
        // the libero keeps following the middle blockers
        for rally in 0..18 {
            match rally % 3 {
                0 | 1 => scout.we_score(),
                _ => scout.they_score(),
            }
            scout.assert_libero_rules(p.libero, [p.mb1, p.mb2], &format!("rally {}", rally + 1));
        }
    }

    /// The setter is substituted: the substitute is the new setter.
    #[test]
    fn setter_substitution_moves_the_setter_role() {
        let p = Players::new();
        let sub = p.bench[0];
        let mut scout = Scout::new(&p.set(0, TeamSideEnum::Them));
        scout.substitute(p.setter, sub);
        let lineup = &scout.snapshot.current_lineup;
        assert_eq!(lineup.get_setter(), Some(sub));
        assert_eq!(lineup.get_role(&sub).unwrap(), RoleEnum::Setter);
        assert_eq!(scout.rotation(), 0);
        // and back, closing the change
        scout.substitute(sub, p.setter);
        assert_eq!(scout.snapshot.current_lineup.get_setter(), Some(p.setter));
        assert_eq!(scout.snapshot.current_lineup.get_substitutions().len(), 2);
    }

    /// Double change: setter and opposite out for a new opposite and a new
    /// setter, then the setter change tells who's setting.
    #[test]
    fn double_change() {
        let p = Players::new();
        let (new_opposite, new_setter) = (p.bench[0], p.bench[1]);
        let mut scout = Scout::new(&p.set(0, TeamSideEnum::Them));
        scout.substitute(p.setter, new_opposite);
        scout.substitute(p.opposite, new_setter);
        scout.add(event(EventTypeEnum::CS, Some(new_setter), None));
        let lineup = &scout.snapshot.current_lineup;
        assert_eq!(lineup.get_setter(), Some(new_setter));
        assert_eq!(lineup.get_opposite(), Some(new_opposite));
        assert_eq!(scout.rotation(), 3);
        // back to the starters
        scout.substitute(new_opposite, p.setter);
        scout.substitute(new_setter, p.opposite);
        scout.add(event(EventTypeEnum::CS, Some(p.setter), None));
        let lineup = &scout.snapshot.current_lineup;
        assert_eq!(lineup.get_setter(), Some(p.setter));
        assert_eq!(lineup.get_opposite(), Some(p.opposite));
        assert_eq!(scout.rotation(), 0);
        assert_eq!(lineup.get_substitutions().len(), 4);
        scout.assert_libero_rules(p.libero, [p.mb1, p.mb2], "after the double change");
    }

    /// The scouting screen recomputes the snapshot from the saved events on
    /// undo: substitutions, setter and libero changes replay the same.
    #[test]
    fn replayed_events_give_the_same_lineup() {
        let p = Players::new();
        let mut set = p.set(1, TeamSideEnum::Us);
        let mut scout = Scout::new(&set);
        scout.substitute(p.mb1, p.bench[0]);
        scout.they_score();
        scout.add(event(EventTypeEnum::CL, None, None));
        scout.we_score();
        scout.substitute(p.oh1, p.bench[1]);
        scout.add(event(EventTypeEnum::CS, Some(p.opposite), None));
        scout.they_score();
        scout.we_score();
        set.events = scout.events.clone();
        let replayed = Scout::new(&set);
        assert_eq!(replayed.on_court(), scout.on_court());
        assert_eq!(replayed.options(), scout.options());
        assert_eq!(replayed.rotation(), scout.rotation());
        assert_eq!(
            replayed.snapshot.current_lineup.get_current_libero(),
            p.fallback_libero
        );
    }

    /// A substitution before the first serve, then the match is opened
    /// again (or an event undone): the events before the serve are still
    /// available.
    #[test]
    fn substitution_before_the_first_serve_survives_a_reload() {
        let p = Players::new();
        let mut set = p.set(0, TeamSideEnum::Them);
        let mut scout = Scout::new(&set);
        let before = scout.options().clone();
        scout.substitute(p.oh1, p.bench[0]);
        assert_eq!(scout.options(), before);
        set.events = scout.events.clone();
        let reloaded = Scout::new(&set);
        assert_eq!(reloaded.options(), before);
        assert_eq!(reloaded.on_court(), scout.on_court());
    }

    /// Setter and libero changes are available from the first serve, like
    /// substitutions.
    #[test]
    fn changes_available_before_the_first_serve() {
        let p = Players::new();
        for serving in [TeamSideEnum::Us, TeamSideEnum::Them] {
            let scout = Scout::new(&p.set(0, serving));
            for e in [EventTypeEnum::R, EventTypeEnum::CL, EventTypeEnum::CS] {
                assert!(
                    scout.options().contains(&e),
                    "{e:?} with {serving:?} serving"
                );
            }
        }
        let without_second_libero = SetEntry::new(
            1,
            TeamSideEnum::Us,
            p.positions(0),
            p.libero,
            None,
            p.setter,
        )
        .expect("expected a valid set");
        let scout = Scout::new(&without_second_libero);
        assert!(!scout.options().contains(&EventTypeEnum::CL));
    }

    #[test]
    fn serve_error_and_point_on_serve() {
        let p = Players::new();
        let mut scout = Scout::new(&p.set(0, TeamSideEnum::Us));
        let serve = |eval| EventEntry {
            eval: Some(eval),
            ..event(EventTypeEnum::S, Some(p.setter), None)
        };
        scout.add(serve(EvalEnum::Perfect));
        assert_eq!(scout.phase(), PhaseEnum::Break);
        assert_eq!(scout.rotation(), 0);
        scout.add(serve(EvalEnum::Error));
        assert_eq!(scout.phase(), PhaseEnum::SideOut);
        assert_eq!(scout.rotation(), 0);
        scout.assert_libero_rules(p.libero, [p.mb1, p.mb2], "after the serve");
    }

    /// Our serve, evaluated: the rally goes on after a positive one.
    fn serve(scout: &mut Scout, eval: EvalEnum) {
        let server = scout.snapshot.current_lineup.get_serving_player();
        scout.add(EventEntry {
            eval: Some(eval),
            ..event(EventTypeEnum::S, server, None)
        });
    }

    #[test]
    fn players_play_in_their_positions_after_our_serve() {
        // setter in 5 (P5): C2 1, O 2, S2 3, C1 4, P 5, S1 6, the middle
        // blocker serving
        let p = Players::new();
        let mut scout = Scout::new(&p.set(4, TeamSideEnum::Us));
        let rotation = [p.mb2, p.opposite, p.oh2, p.mb1, p.setter, p.oh1];
        assert_eq!(scout.snapshot.get_playing_positions(), rotation);
        serve(&mut scout, EvalEnum::Positive);
        assert_eq!(
            scout.snapshot.get_playing_positions(),
            [p.setter, p.opposite, p.mb1, p.oh2, p.mb2, p.oh1]
        );
        // the point: the next serve, in the order of rotation again
        scout.we_score();
        assert_eq!(scout.snapshot.get_playing_positions(), rotation);
    }

    #[test]
    fn break_positions_from_any_rotation() {
        let p = Players::new();
        for rotation in 0..6 {
            let mut scout = Scout::new(&p.set(rotation, TeamSideEnum::Us));
            serve(&mut scout, EvalEnum::Positive);
            let positions = scout.snapshot.get_playing_positions();
            let front = &positions[1..4];
            let back = [positions[0], positions[4], positions[5]];
            let context = format!("rotation {}: {positions:?}", rotation + 1);
            assert!([p.setter, p.opposite].contains(&positions[1]), "{context}");
            assert!([p.mb1, p.mb2].contains(&positions[2]), "{context}");
            assert!([p.oh1, p.oh2].contains(&positions[3]), "{context}");
            assert!([p.setter, p.opposite].contains(&positions[0]), "{context}");
            assert!(
                [p.mb1, p.mb2, p.libero].contains(&positions[4]),
                "{context}"
            );
            assert!([p.oh1, p.oh2].contains(&positions[5]), "{context}");
            // nobody changes row
            let on_court = scout.on_court();
            assert!(
                front.iter().all(|id| on_court[1..4].contains(id)),
                "{context}"
            );
            assert!(
                back.iter().all(|id| !on_court[1..4].contains(id)),
                "{context}"
            );
        }
    }

    /// Their serve, received: the rally goes on after a positive one.
    fn receive(scout: &mut Scout) {
        let receiver = scout.snapshot.current_lineup.get_oh1();
        scout.add(EventEntry {
            eval: Some(EvalEnum::Positive),
            ..event(EventTypeEnum::P, receiver, None)
        });
    }

    #[test]
    fn players_play_in_their_positions_after_the_reception() {
        // setter in 5 (P5), the libero in 1 for C2
        let p = Players::new();
        let mut scout = Scout::new(&p.set(4, TeamSideEnum::Them));
        let rotation = [p.libero, p.opposite, p.oh2, p.mb1, p.setter, p.oh1];
        assert_eq!(scout.snapshot.get_playing_positions(), rotation);
        receive(&mut scout);
        assert_eq!(
            scout.snapshot.get_playing_positions(),
            [p.setter, p.opposite, p.mb1, p.oh2, p.libero, p.oh1]
        );
        // our attack error: the next reception, in the order of rotation
        // again
        scout.add(EventEntry {
            eval: Some(EvalEnum::Error),
            ..event(EventTypeEnum::A, Some(p.oh2), None)
        });
        assert_eq!(scout.snapshot.get_playing_positions(), rotation);
    }

    #[test]
    fn the_canonical_p1_attacks_from_where_it_receives() {
        let p = Players::new();
        let mut scout = Scout::new(&p.set(0, TeamSideEnum::Them));
        receive(&mut scout);
        // the opposite in 4 and S1 in 2, as they received
        assert_eq!(
            scout.snapshot.get_playing_positions(),
            [p.setter, p.oh1, p.mb2, p.opposite, p.libero, p.oh2]
        );
        // in break the opposite plays in 2 again
        let mut scout = Scout::new(&p.set(0, TeamSideEnum::Us));
        serve(&mut scout, EvalEnum::Positive);
        assert_eq!(
            scout.snapshot.get_playing_positions(),
            [p.setter, p.opposite, p.mb2, p.oh1, p.libero, p.oh2]
        );
    }

    /// With the P1 inverted the opposite plays in 2 also in side out.
    #[test]
    fn the_inverted_p1_attacks_as_in_break() {
        let p = Players::new();
        let mut scout = Scout::new(&p.set(0, TeamSideEnum::Them));
        let receiver = scout.snapshot.current_lineup.get_oh1();
        scout.add(EventEntry {
            eval: Some(EvalEnum::Positive),
            flags: EventFlags::default().with_inverted_p1(true),
            ..event(EventTypeEnum::P, receiver, None)
        });
        assert_eq!(
            scout.snapshot.get_playing_positions(),
            [p.setter, p.opposite, p.mb2, p.oh1, p.libero, p.oh2]
        );
    }

    #[test]
    fn rally_positions_after_the_reception_from_any_rotation_but_p1() {
        let p = Players::new();
        for rotation in 1..6 {
            let mut scout = Scout::new(&p.set(rotation, TeamSideEnum::Them));
            receive(&mut scout);
            let positions = scout.snapshot.get_playing_positions();
            let context = format!("rotation {}: {positions:?}", rotation + 1);
            assert!([p.setter, p.opposite].contains(&positions[0]), "{context}");
            assert!([p.setter, p.opposite].contains(&positions[1]), "{context}");
            assert!([p.mb1, p.mb2].contains(&positions[2]), "{context}");
            assert!([p.oh1, p.oh2].contains(&positions[3]), "{context}");
            assert_eq!(positions[4], p.libero, "{context}");
            assert!([p.oh1, p.oh2].contains(&positions[5]), "{context}");
        }
    }

    // --- the player out for the libero ------------------------------------
    //
    // While the libero plays in place of a middle blocker (or of whoever
    // took their place), that player is out of the court but still in the
    // game: they can be substituted, the libero staying on court and the
    // substitute taking their place for when the libero leaves. The libero
    // is never substituted, and the player out for the libero can't come in
    // for anyone else.

    impl Scout {
        /// The event refused, the snapshot unchanged: why.
        fn refused(&mut self, event: EventEntry) -> String {
            let (court, out) = (self.on_court(), self.out_for_the_libero());
            let reason = match self.snapshot.add_event(&event) {
                Ok(()) => panic!("expected {event} to be refused"),
                Err(e) => e.to_string(),
            };
            assert_eq!(self.on_court(), court, "{reason}");
            assert_eq!(self.out_for_the_libero(), out, "{reason}");
            reason
        }

        fn refused_substitution(&mut self, replaced: Uuid, replacement: Uuid) -> String {
            self.refused(event(EventTypeEnum::R, Some(replaced), Some(replacement)))
        }

        /// The player the libero plays for, if it's on court.
        fn out_for_the_libero(&self) -> Option<Uuid> {
            let lineup = &self.snapshot.current_lineup;
            let libero = lineup.get_current_libero();
            lineup.find_position(&libero).map(|_| {
                lineup
                    .get_involved_players()
                    .into_iter()
                    .find(|p| lineup.is_out_for_the_libero(p))
                    .expect("a player out for the libero")
            })
        }

        /// The same set saved and read back: the same court, and the same
        /// player out for the libero.
        fn assert_replays(&self, p: &Players, rotation: usize, serving: TeamSideEnum) {
            let set = SetEntry {
                events: self.events.clone(),
                ..p.set(rotation, serving)
            };
            let replayed = Scout::new(&set);
            assert_eq!(replayed.on_court(), self.on_court());
            assert_eq!(replayed.out_for_the_libero(), self.out_for_the_libero());
            assert_eq!(
                replayed
                    .snapshot
                    .current_lineup
                    .get_positions_without_libero(),
                self.snapshot.current_lineup.get_positions_without_libero()
            );
        }
    }

    /// Middle blocker 1 serves first (setter in position 2): substituted
    /// for the serve, the substitute loses it and the libero goes in for
    /// them, then the middle blocker comes back for the substitute, the
    /// libero on court. The most common substitution of all.
    #[test]
    fn the_substitute_for_the_serve_goes_back_while_the_libero_plays() {
        let p = Players::new();
        let sub = p.bench[0];
        let mut scout = Scout::new(&p.set(1, TeamSideEnum::Us));
        scout.substitute(p.mb1, sub);
        assert_eq!(scout.on_court()[0], sub, "the substitute serves");
        scout.they_score();
        assert_eq!(scout.on_court()[0], p.libero);
        assert_eq!(scout.out_for_the_libero(), Some(sub));
        // the middle blocker back, the libero still on court
        scout.substitute(sub, p.mb1);
        assert_eq!(scout.on_court()[0], p.libero);
        assert_eq!(scout.out_for_the_libero(), Some(p.mb1));
        assert!(!scout.snapshot.current_lineup.is_in_the_game(&sub));
        assert_eq!(scout.snapshot.current_lineup.get_mb1(), Some(p.mb1));
        assert_eq!(scout.snapshot.current_lineup.get_substitutions().len(), 2);
        scout.assert_replays(&p, 1, TeamSideEnum::Us);
        // around the court: the middle blocker goes back in front for the
        // libero, never the substitute
        for rally in 0..18 {
            match rally % 3 {
                0 | 1 => scout.we_score(),
                _ => scout.they_score(),
            }
            let context = format!("rally {}", rally + 1);
            scout.assert_libero_rules(p.libero, [p.mb1, p.mb2], &context);
            assert!(!scout.on_court().contains(&sub), "{context}");
        }
        scout.assert_replays(&p, 1, TeamSideEnum::Us);
        // the change is closed: neither plays again for someone else
        assert!(
            scout
                .refused_substitution(p.mb1, p.bench[1])
                .contains("already replaced")
        );
    }

    /// A starting middle blocker out for the libero substituted by a bench
    /// player: the libero stays, the substitute takes the court when the
    /// libero leaves it, and only the middle blocker can come back for them.
    #[test]
    fn the_middle_blocker_out_for_the_libero_is_substituted() {
        let p = Players::new();
        let sub = p.bench[0];
        let mut scout = Scout::new(&p.set(0, TeamSideEnum::Them));
        assert_eq!(scout.out_for_the_libero(), Some(p.mb1));
        let court = scout.on_court();
        scout.substitute(p.mb1, sub);
        assert_eq!(scout.on_court(), court, "the libero stays on court");
        assert_eq!(scout.out_for_the_libero(), Some(sub));
        assert_eq!(scout.snapshot.current_lineup.get_mb1(), Some(sub));
        assert!(!scout.snapshot.current_lineup.is_in_the_game(&p.mb1));
        scout.assert_replays(&p, 0, TeamSideEnum::Them);
        // until the libero would go to the front row: the other middle
        // blocker serves, the substitute back in the front row
        scout.we_score();
        scout.they_score();
        scout.we_score();
        assert_eq!(scout.rotation(), 4);
        assert_eq!(scout.phase(), PhaseEnum::Break);
        assert_eq!(scout.on_court()[0], p.mb2);
        assert_eq!(
            scout.on_court()[3],
            sub,
            "the substitute in place of the libero"
        );
        assert!(!scout.on_court().contains(&p.mb1));
        assert!(!scout.on_court().contains(&p.libero));
        scout.assert_libero_rules(p.libero, [sub, p.mb2], "after the side out");
        // closed change: only the middle blocker for the substitute
        assert!(
            scout
                .refused_substitution(sub, p.bench[1])
                .contains("can be only replaced by")
        );
        scout.substitute(sub, p.mb1);
        assert_eq!(scout.on_court()[3], p.mb1);
        scout.assert_replays(&p, 0, TeamSideEnum::Them);
    }

    /// The libero is never substituted, on court or not, nor comes in as a
    /// substitute; and the player out for the libero doesn't come in for
    /// someone else, already being in the game.
    #[test]
    fn the_libero_is_never_substituted() {
        let p = Players::new();
        let mut scout = Scout::new(&p.set(0, TeamSideEnum::Them));
        assert!(scout.on_court().contains(&p.libero));
        assert!(
            scout
                .refused_substitution(p.libero, p.bench[0])
                .contains("cannot replace the libero")
        );
        // the other libero isn't in the game
        assert!(
            scout
                .refused_substitution(p.fallback_libero, p.bench[0])
                .contains("can't play it")
        );
        for libero in [p.libero, p.fallback_libero] {
            assert!(
                scout
                    .refused_substitution(p.oh1, libero)
                    .contains("can't come in")
            );
        }
        // the middle blocker the libero plays for
        assert!(
            scout
                .refused_substitution(p.oh1, p.mb1)
                .contains("can't come in")
        );
        // the substitute out for the libero neither
        scout.substitute(p.mb1, p.bench[0]);
        assert!(
            scout
                .refused_substitution(p.oh1, p.bench[0])
                .contains("can't come in")
        );
        // the libero changed: still never substituted
        scout.we_score();
        scout.they_score();
        scout.add(event(EventTypeEnum::CL, None, None));
        assert!(scout.on_court().contains(&p.fallback_libero));
        assert_eq!(scout.out_for_the_libero(), Some(p.bench[0]));
        assert!(
            scout
                .refused_substitution(p.fallback_libero, p.bench[1])
                .contains("cannot replace the libero")
        );
        // and the libero not on court not in the game
        assert!(
            scout
                .refused_substitution(p.libero, p.bench[1])
                .contains("can't play it")
        );
    }

    /// The substitution of the player out for the libero counts as any
    /// other: up to the maximum.
    #[test]
    fn the_player_out_for_the_libero_counts_among_the_substitutions() {
        let p = Players::new();
        let set = p.set(0, TeamSideEnum::Them);
        let snapshot = set.compute_snapshot(1).expect("a snapshot");
        let mut scout = Scout {
            snapshot,
            events: vec![],
        };
        scout.substitute(p.mb1, p.bench[0]);
        assert!(
            scout
                .refused_substitution(p.oh1, p.bench[1])
                .contains("max number of substitutions")
        );
        assert!(
            scout
                .refused_substitution(p.bench[0], p.mb1)
                .contains("max number of substitutions")
        );
    }

    /// The scouting offers the player out for the libero among the ones who
    /// can leave, by their role, and the bench (or the one who left, once
    /// substituted) to come in.
    #[test]
    fn the_scouting_offers_the_player_out_for_the_libero() {
        let p = Players::new();
        let team = p.team();
        let sub = p.bench[0];
        let mut scout = Scout::new(&p.set(1, TeamSideEnum::Us));
        let leaving = |scout: &Scout| -> Vec<Uuid> {
            scout
                .snapshot
                .current_lineup
                .get_replaceable_lineup_choices()
                .into_iter()
                .filter_map(|(_, (_, id))| id)
                .collect()
        };
        let coming_in = |scout: &Scout, replaced: Uuid| -> Vec<Uuid> {
            scout
                .snapshot
                .current_lineup
                .get_available_replacements(&team, replaced)
                .iter()
                .map(|(_, player)| player.id)
                .collect()
        };
        scout.substitute(p.mb1, sub);
        scout.they_score();
        // the substitute, out for the libero, can leave; the libero can't
        assert!(leaving(&scout).contains(&sub), "{:?}", leaving(&scout));
        assert!(!leaving(&scout).contains(&p.libero));
        // only for the middle blocker
        assert_eq!(coming_in(&scout, sub), vec![p.mb1]);
        // and every player offered can play the substitution
        for player in leaving(&scout) {
            assert!(
                scout.snapshot.can_play(EventTypeEnum::R, &player),
                "{player}"
            );
        }
        scout.substitute(sub, p.mb1);
        // the change closed: neither offered again
        assert!(!leaving(&scout).contains(&p.mb1));
        assert_eq!(coming_in(&scout, p.mb1), Vec::<Uuid>::new());
        // a middle blocker out for the libero never substituted: the bench
        let mut scout = Scout::new(&p.set(0, TeamSideEnum::Them));
        assert!(leaving(&scout).contains(&p.mb1));
        assert_eq!(coming_in(&scout, p.mb1), p.bench.to_vec());
        scout.substitute(p.mb1, sub);
        assert_eq!(coming_in(&scout, sub), vec![p.mb1]);
    }
}
