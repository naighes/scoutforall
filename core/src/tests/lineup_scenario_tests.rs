//! Scouting scenarios around the lineup: the libero going in and out, setter
//! changes and substitutions, replayed through `Snapshot::add_event` like the
//! scouting screen does.

#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, PhaseEnum, RoleEnum, TeamSideEnum},
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
}
