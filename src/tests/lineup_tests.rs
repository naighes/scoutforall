#[cfg(test)]
mod tests {
    use crate::shapes::{
        enums::{RoleEnum, TeamSideEnum},
        set::SetEntry,
        snapshot::Snapshot,
    };
    use uuid::Uuid;

    fn make_empty_snapshot(
        set_number: u8,
        serving_team: TeamSideEnum,
        lineup: [Uuid; 6],
        setter: Uuid,
        libero: Uuid,
    ) -> Snapshot {
        let set = SetEntry::new(set_number, serving_team, lineup, libero, None, setter)
            .expect("expected a valid set");
        Snapshot::new(&set).expect("expected a valid snapshot")
    }

    #[test]
    fn get_role_rotation_0() {
        let setter = Uuid::new_v4();
        let lineup: [Uuid; 6] = [
            setter,
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];
        let libero = Uuid::new_v4();
        let snapshot = make_empty_snapshot(1, TeamSideEnum::Them, lineup, setter, libero);
        assert_eq!(
            snapshot
                .current_lineup
                .get_role(&setter)
                .expect("expected a role"),
            RoleEnum::Setter
        );
        assert_eq!(
            snapshot
                .current_lineup
                .get_role(&snapshot.current_lineup.get(1).expect("expected a player"))
                .expect("expected a role"),
            RoleEnum::OutsideHitter
        );
        assert_eq!(
            snapshot
                .current_lineup
                .get_role(&snapshot.current_lineup.get(2).expect("expected a player"))
                .expect("expected a role"),
            RoleEnum::MiddleBlocker
        );
        assert_eq!(
            snapshot
                .current_lineup
                .get_role(&snapshot.current_lineup.get(3).expect("expected a player"))
                .expect("expected a role"),
            RoleEnum::OppositeHitter
        );
        assert_eq!(
            snapshot
                .current_lineup
                .get_role(&snapshot.current_lineup.get(4).expect("expected a player"))
                .expect("expected a role"),
            RoleEnum::OutsideHitter
        );
        assert_eq!(
            snapshot
                .current_lineup
                .get_role(&snapshot.current_lineup.get(5).expect("expected a player"))
                .expect("expected a role"),
            RoleEnum::Libero
        );
    }

    #[test]
    fn get_role_rotation_5() {
        let setter = Uuid::new_v4();
        let lineup: [Uuid; 6] = [
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            setter,
        ];
        let setter = lineup[5];
        let libero = Uuid::new_v4();
        let snapshot = make_empty_snapshot(1, TeamSideEnum::Them, lineup, setter, libero);
        assert_eq!(
            snapshot
                .current_lineup
                .get_role(&setter)
                .expect("expected a role"),
            RoleEnum::Setter
        );
        assert_eq!(
            snapshot
                .current_lineup
                .get_role(&snapshot.current_lineup.get(0).expect("expected a player"))
                .expect("expected a role"),
            RoleEnum::OutsideHitter
        );
        assert_eq!(
            snapshot
                .current_lineup
                .get_role(&snapshot.current_lineup.get(1).expect("expected a player"))
                .expect("expected a role"),
            RoleEnum::MiddleBlocker
        );
        assert_eq!(
            snapshot
                .current_lineup
                .get_role(&snapshot.current_lineup.get(2).expect("expected a player"))
                .expect("expected a role"),
            RoleEnum::OppositeHitter
        );
        assert_eq!(
            snapshot
                .current_lineup
                .get_role(&snapshot.current_lineup.get(3).expect("expected a player"))
                .expect("expected a role"),
            RoleEnum::OutsideHitter
        );
        assert_eq!(
            snapshot
                .current_lineup
                .get_role(&snapshot.current_lineup.get(4).expect("expected a player"))
                .expect("expected a role"),
            RoleEnum::Libero
        );
    }

    #[test]
    fn back_row_player_rotation_0() {
        let setter = Uuid::new_v4();
        let oh1 = Uuid::new_v4();
        let mb2 = Uuid::new_v4();
        let opposite = Uuid::new_v4();
        let oh2 = Uuid::new_v4();
        let mb1 = Uuid::new_v4();
        let libero = Uuid::new_v4();
        let lineup: [Uuid; 6] = [
            setter,   // br
            oh1,      // fr
            mb2,      // fr
            opposite, // fr
            oh2,      // br
            mb1,      // br
        ];
        let snapshot = make_empty_snapshot(1, TeamSideEnum::Them, lineup, setter, libero);
        assert!(snapshot.current_lineup.is_back_row_player(&setter));
        assert!(snapshot.current_lineup.is_back_row_player(&libero));
        assert!(snapshot.current_lineup.is_back_row_player(&oh2));
        assert!(!snapshot.current_lineup.is_back_row_player(&opposite));
        assert!(!snapshot.current_lineup.is_back_row_player(&mb2));
    }

    #[test]
    fn back_row_player_rotation_5() {
        let setter = Uuid::new_v4();
        let oh1 = Uuid::new_v4();
        let mb2 = Uuid::new_v4();
        let opposite = Uuid::new_v4();
        let oh2 = Uuid::new_v4();
        let mb1 = Uuid::new_v4();
        let libero = Uuid::new_v4();
        let lineup: [Uuid; 6] = [
            oh1,      // br
            mb2,      // fr
            opposite, // fr
            oh2,      // fr
            mb1,      // br
            setter,   // br
        ];
        let snapshot = make_empty_snapshot(1, TeamSideEnum::Them, lineup, setter, libero);
        assert!(snapshot.current_lineup.is_back_row_player(&setter));
        assert!(snapshot.current_lineup.is_back_row_player(&libero));
        assert!(!snapshot.current_lineup.is_back_row_player(&oh2));
        assert!(!snapshot.current_lineup.is_back_row_player(&opposite));
        assert!(!snapshot.current_lineup.is_back_row_player(&mb2));
        assert!(snapshot.current_lineup.is_back_row_player(&oh1));
    }
}
