mod tests {
    use crate::shapes::{
        enums::{GenderEnum, RoleEnum, TeamClassificationEnum, TeamSideEnum},
        player::PlayerEntry,
        set::SetEntry,
        snapshot::Snapshot,
        team::TeamEntry,
    };
    use uuid::Uuid;

    #[test]
    fn valid_options_beginning_of_set() {
        let setter: Uuid = Uuid::new_v4();
        let oh1: Uuid = Uuid::new_v4();
        let mb2: Uuid = Uuid::new_v4();
        let opposite: Uuid = Uuid::new_v4();
        let oh2: Uuid = Uuid::new_v4();
        let mb1: Uuid = Uuid::new_v4();
        let libero: Uuid = Uuid::new_v4();
        let positions: [Uuid; 6] = [setter, oh1, mb2, opposite, oh2, mb1];
        let set = SetEntry::new(1, TeamSideEnum::Us, positions, libero, None, setter)
            .expect("expected a valid set");
        let snapshot: Snapshot = Snapshot::new(&set).expect("expected a valid snapshot");
        let options = snapshot.current_lineup.get_replaceable_lineup_choices();
        assert_eq!(
            options[0].1 .1,
            Some(setter),
            "first option should be setter"
        );
        assert_eq!(options[1].1 .1, Some(oh1), "second option should be oh1");
        assert_eq!(options[2].1 .1, Some(mb2), "third option should be mb2");
        assert_eq!(
            options[3].1 .1,
            Some(opposite),
            "fourth option should be opposite"
        );
        assert_eq!(options[4].1 .1, Some(oh2), "fifth option should be oh2");
        assert_eq!(options[5].1 .1, Some(mb1), "sixth option should be mb1");
        assert_eq!(options.len(), 6);
    }

    #[test]
    fn valid_options_closed_change() {
        let setter: Uuid = Uuid::new_v4();
        let oh1: Uuid = Uuid::new_v4();
        let mb2: Uuid = Uuid::new_v4();
        let opposite: Uuid = Uuid::new_v4();
        let oh2: Uuid = Uuid::new_v4();
        let mb1: Uuid = Uuid::new_v4();
        let libero: Uuid = Uuid::new_v4();
        let setter_replacement: Uuid = Uuid::new_v4();
        let positions: [Uuid; 6] = [setter, oh1, mb2, opposite, oh2, mb1];
        let set = SetEntry::new(1, TeamSideEnum::Us, positions, libero, None, setter)
            .expect("expected a valid set");
        let mut snapshot: Snapshot = Snapshot::new(&set).expect("expected a valid snapshot");
        snapshot
            .current_lineup
            .add_substitution(&setter, &setter_replacement)
            .expect("no errors expected");
        snapshot
            .current_lineup
            .add_substitution(&setter_replacement, &setter)
            .expect("no errors expected");
        let options = snapshot.current_lineup.get_replaceable_lineup_choices();
        assert_eq!(options[0].1 .1, Some(oh1), "first option should be oh1");
        assert_eq!(options[1].1 .1, Some(mb2), "second option should be mb2");
        assert_eq!(
            options[2].1 .1,
            Some(opposite),
            "third option should be opposite"
        );
        assert_eq!(options[3].1 .1, Some(oh2), "fourth option should be oh2");
        assert_eq!(options[4].1 .1, Some(mb1), "fifth option should be mb1");
        assert_eq!(options.len(), 5);
    }

    #[test]
    fn valid_options_non_closed_change() {
        let setter: Uuid = Uuid::new_v4();
        let oh1: Uuid = Uuid::new_v4();
        let mb2: Uuid = Uuid::new_v4();
        let opposite: Uuid = Uuid::new_v4();
        let oh2: Uuid = Uuid::new_v4();
        let mb1: Uuid = Uuid::new_v4();
        let libero: Uuid = Uuid::new_v4();
        let setter_replacement: Uuid = Uuid::new_v4();
        let positions: [Uuid; 6] = [setter, oh1, mb2, opposite, oh2, mb1];
        let set = SetEntry::new(1, TeamSideEnum::Us, positions, libero, None, setter)
            .expect("expected a valid set");
        let mut snapshot: Snapshot = Snapshot::new(&set).expect("expected a valid snapshot");
        snapshot
            .current_lineup
            .add_substitution(&setter, &setter_replacement)
            .expect("no errors expected");
        let options = snapshot.current_lineup.get_replaceable_lineup_choices();
        assert_eq!(
            options[0].1 .1,
            Some(setter_replacement),
            "first option should be setter_replacement"
        );
        assert_eq!(options[1].1 .1, Some(oh1), "second option should be oh1");
        assert_eq!(options[2].1 .1, Some(mb2), "third option should be mb2");
        assert_eq!(
            options[3].1 .1,
            Some(opposite),
            "fourth option should be opposite"
        );
        assert_eq!(options[4].1 .1, Some(oh2), "fifth option should be oh2");
        assert_eq!(options[5].1 .1, Some(mb1), "sixth option should be mb1");
        assert_eq!(options.len(), 6);
    }

    fn make_test_team() -> (TeamEntry, Uuid, Uuid) {
        let setter: Uuid = Uuid::new_v4();
        let oh1: Uuid = Uuid::new_v4();
        let mb2: Uuid = Uuid::new_v4();
        let opposite: Uuid = Uuid::new_v4();
        let oh2: Uuid = Uuid::new_v4();
        let mb1: Uuid = Uuid::new_v4();
        let libero: Uuid = Uuid::new_v4();
        let setter_replacement: Uuid = Uuid::new_v4();
        let some_replacement: Uuid = Uuid::new_v4();

        let team: TeamEntry = TeamEntry {
            id: Uuid::new_v4(),
            name: "My Team".to_owned(),
            classification: Some(TeamClassificationEnum::HighNational),
            gender: Some(GenderEnum::Men),
            players: vec![
                PlayerEntry {
                    id: setter,
                    name: "Ron Gilbert".to_owned(),
                    role: RoleEnum::Setter,
                    number: 1,
                },
                PlayerEntry {
                    id: oh1,
                    name: "David Crane".to_owned(),
                    role: RoleEnum::OutsideHitter,
                    number: 2,
                },
                PlayerEntry {
                    id: mb2,
                    name: "Éric Chahi".to_owned(),
                    role: RoleEnum::MiddleBlocker,
                    number: 3,
                },
                PlayerEntry {
                    id: opposite,
                    name: "David Braben".to_owned(),
                    role: RoleEnum::OppositeHitter,
                    number: 4,
                },
                PlayerEntry {
                    id: oh2,
                    name: "John Carmack".to_owned(),
                    role: RoleEnum::OutsideHitter,
                    number: 5,
                },
                PlayerEntry {
                    id: mb1,
                    name: "Eric Matthews".to_owned(),
                    role: RoleEnum::MiddleBlocker,
                    number: 6,
                },
                PlayerEntry {
                    id: libero,
                    name: "Dino Dini".to_owned(),
                    role: RoleEnum::Libero,
                    number: 7,
                },
                PlayerEntry {
                    id: setter_replacement,
                    name: "Manfred Trenz".to_owned(),
                    role: RoleEnum::Setter,
                    number: 8,
                },
                PlayerEntry {
                    id: some_replacement,
                    name: "Jordan Mechner".to_owned(),
                    role: RoleEnum::OppositeHitter,
                    number: 9,
                },
            ],
            year: 2024,
        };

        (team, setter, setter_replacement)
    }

    #[test]
    fn available_subs_beginning_of_set() {
        let (team, setter, setter_replacement) = make_test_team();
        let libero = team.players[6].id;
        let positions: [Uuid; 6] = [
            setter,
            team.players[1].id,
            team.players[2].id,
            team.players[3].id,
            team.players[4].id,
            team.players[5].id,
        ];
        let set = SetEntry::new(1, TeamSideEnum::Us, positions, libero, None, setter)
            .expect("expected a valid set");
        let snapshot: Snapshot = Snapshot::new(&set).expect("expected a valid snapshot");
        let options = snapshot
            .current_lineup
            .get_available_replacements(&team, setter);
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].1.id, setter_replacement);
    }

    #[test]
    fn available_subs_forced_replacement() {
        let (team, setter, setter_replacement) = make_test_team();
        let libero = team.players[6].id;
        let positions: [Uuid; 6] = [
            setter,
            team.players[1].id,
            team.players[2].id,
            team.players[3].id,
            team.players[4].id,
            team.players[5].id,
        ];
        let set = SetEntry::new(1, TeamSideEnum::Us, positions, libero, None, setter)
            .expect("expected a valid set");
        let mut snapshot: Snapshot = Snapshot::new(&set).expect("expected a valid snapshot");
        snapshot
            .current_lineup
            .add_substitution(&setter, &setter_replacement)
            .expect("no errors expected");
        let options = snapshot
            .current_lineup
            .get_available_replacements(&team, setter_replacement);
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].1.id, setter);
    }

    #[test]
    fn available_subs_closed_change() {
        let (team, setter, setter_replacement) = make_test_team();
        let libero = team.players[6].id;
        let positions: [Uuid; 6] = [
            setter,
            team.players[1].id,
            team.players[2].id,
            team.players[3].id,
            team.players[4].id,
            team.players[5].id,
        ];
        let set = SetEntry::new(1, TeamSideEnum::Us, positions, libero, None, setter)
            .expect("expected a valid set");
        let mut snapshot: Snapshot = Snapshot::new(&set).expect("expected a valid snapshot");
        snapshot
            .current_lineup
            .add_substitution(&setter, &setter_replacement)
            .expect("no errors expected");
        snapshot
            .current_lineup
            .add_substitution(&setter_replacement, &setter)
            .expect("no errors expected");
        assert_eq!(snapshot.current_lineup.get_substitutions().len(), 2);
        let options = snapshot
            .current_lineup
            .get_available_replacements(&team, setter);
        assert_eq!(options.len(), 0);
    }

    #[test]
    fn available_subs_replacement_already_used() {
        let (team, setter, setter_replacement) = make_test_team();
        let positions: [Uuid; 6] = [
            setter,
            team.players[1].id,
            team.players[2].id,
            team.players[3].id,
            team.players[4].id,
            team.players[5].id,
        ];
        let set = SetEntry::new(
            1,
            TeamSideEnum::Us,
            positions,
            team.players[6].id,
            None,
            setter,
        )
        .expect("expected a valid set");
        let mut snapshot: Snapshot = Snapshot::new(&set).expect("expected a valid snapshot");
        snapshot
            .current_lineup
            .add_substitution(&team.players[3].id, &setter_replacement)
            .expect("no errors expected");
        snapshot
            .current_lineup
            .add_substitution(&setter_replacement, &team.players[3].id)
            .expect("no errors expected");
        let options = snapshot
            .current_lineup
            .get_available_replacements(&team, setter);
        assert!(!options.iter().any(|p| p.1.id == setter_replacement));
    }
}
