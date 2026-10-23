//! The checks of the player form, shared by every user interface.

#[cfg(test)]
mod tests {
    use crate::{
        player_form::{PlayerForm, PlayerFormError, removal},
        providers::team_writer::PlayerInput,
        shapes::{enums::RoleEnum, player::PlayerEntry, team::TeamEntry},
        tests::EN,
    };

    fn player(name: &str, number: u8, deleted: bool) -> PlayerEntry {
        PlayerEntry {
            name: name.into(),
            role: Some(RoleEnum::Setter),
            number,
            deleted,
            ..Default::default()
        }
    }

    fn team() -> TeamEntry {
        TeamEntry {
            name: "Cortona".into(),
            year: 2026,
            players: vec![player("Anna", 7, false), player("Bea", 9, true)],
            ..Default::default()
        }
    }

    fn form(number: &str) -> PlayerForm {
        PlayerForm {
            name: " Carla ".into(),
            role: Some(RoleEnum::Libero),
            number: number.into(),
        }
    }

    fn error(form: PlayerForm) -> PlayerFormError {
        match form.validate(&team(), None) {
            Err(e) => e,
            Ok(_) => unreachable!("the form should not be valid"),
        }
    }

    #[test]
    fn a_valid_form_adds_a_player_with_the_name_trimmed() {
        match form("12").validate(&team(), None) {
            Ok(PlayerInput::New {
                name,
                role: RoleEnum::Libero,
                number: 12,
            }) => assert_eq!(name, "Carla"),
            _ => unreachable!("expected a new player"),
        }
    }

    #[test]
    fn editing_keeps_the_id_and_may_keep_the_number() {
        let team = team();
        let anna = &team.players[0];
        match form("7").validate(&team, Some(anna)) {
            Ok(PlayerInput::Existing(updated)) => {
                assert_eq!(updated.id, anna.id);
                assert_eq!(updated.name, "Carla");
                assert_eq!(updated.role, Some(RoleEnum::Libero));
                assert_eq!(updated.number, 7);
            }
            _ => unreachable!("expected the existing player"),
        }
    }

    #[test]
    fn the_number_of_another_active_player_is_in_use() {
        assert_eq!(error(form("7")), PlayerFormError::NumberInUse);
        // a removed player gives back the number
        assert!(form("9").validate(&team(), None).is_ok());
    }

    #[test]
    fn the_number_is_from_0_to_99() {
        for number in ["", "100", "-1", "7a", "７"] {
            assert_eq!(
                error(form(number)),
                PlayerFormError::InvalidNumber,
                "{number:?}"
            );
        }
        for number in ["0", "99", " 5 "] {
            assert!(form(number).validate(&team(), None).is_ok(), "{number:?}");
        }
    }

    #[test]
    fn name_and_role_are_required_in_the_order_of_the_fields() {
        assert_eq!(
            error(PlayerForm {
                name: "  ".into(),
                role: None,
                number: String::new(),
            }),
            PlayerFormError::EmptyName
        );
        assert_eq!(
            error(PlayerForm {
                role: None,
                ..form("")
            }),
            PlayerFormError::MissingRole
        );
    }

    #[test]
    fn the_form_of_a_player_validates_back_to_it() {
        let anna = player("Anna", 7, false);
        assert_eq!(
            PlayerForm::of(&anna),
            PlayerForm {
                name: "Anna".into(),
                role: Some(RoleEnum::Setter),
                number: "7".into(),
            }
        );
    }

    #[test]
    fn a_removed_player_is_kept_but_deleted() {
        let anna = player("Anna", 7, false);
        match removal(&anna) {
            PlayerInput::Existing(removed) => {
                assert_eq!(removed.id, anna.id);
                assert!(removed.deleted);
            }
            PlayerInput::New { .. } => unreachable!("expected the existing player"),
        }
    }

    #[test]
    fn the_errors_are_in_the_domain_labels() {
        assert_eq!(
            PlayerFormError::NumberInUse.message(EN),
            "number already in use"
        );
        assert_eq!(PlayerFormError::MissingRole.message(EN), "role is required");
    }
}
