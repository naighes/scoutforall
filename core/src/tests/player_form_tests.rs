//! The checks of the player form, shared by every user interface.

#[cfg(test)]
mod tests {
    use crate::{
        player_form::{PlayerForm, PlayerFormError, removal},
        providers::team_writer::{PlayerDetails, PlayerInput},
        shapes::{enums::RoleEnum, player::PlayerEntry, team::TeamEntry},
        tests::EN,
    };
    use chrono::NaiveDate;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 26).unwrap()
    }

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
            ..Default::default()
        }
    }

    fn error(form: PlayerForm) -> PlayerFormError {
        match form.validate(&team(), None, today()) {
            Err(e) => e,
            Ok(_) => unreachable!("the form should not be valid"),
        }
    }

    #[test]
    fn a_valid_form_adds_a_player_with_the_name_trimmed() {
        match form("12").validate(&team(), None, today()) {
            Ok(PlayerInput::New {
                name,
                role: RoleEnum::Libero,
                number: 12,
                details,
            }) => {
                assert_eq!(name, "Carla");
                assert_eq!(details, PlayerDetails::default());
            }
            _ => unreachable!("expected a new player"),
        }
    }

    #[test]
    fn editing_keeps_the_id_and_may_keep_the_number() {
        let team = team();
        let anna = &team.players[0];
        match form("7").validate(&team, Some(anna), today()) {
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
        assert!(form("9").validate(&team(), None, today()).is_ok());
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
            assert!(
                form(number).validate(&team(), None, today()).is_ok(),
                "{number:?}"
            );
        }
    }

    #[test]
    fn name_and_role_are_required_in_the_order_of_the_fields() {
        assert_eq!(
            error(PlayerForm {
                name: "  ".into(),
                role: None,
                ..Default::default()
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
                ..Default::default()
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

    fn with(birth_date: &str, height_cm: &str) -> PlayerForm {
        PlayerForm {
            birth_date: birth_date.into(),
            height_cm: height_cm.into(),
            ..form("12")
        }
    }

    fn details(form: PlayerForm) -> PlayerDetails {
        match form.validate(&team(), None, today()) {
            Ok(PlayerInput::New { details, .. }) => details,
            _ => unreachable!("expected a new player"),
        }
    }

    #[test]
    fn birth_date_height_and_notes_are_optional() {
        assert_eq!(details(with(" ", " ")), PlayerDetails::default());
        assert_eq!(
            details(PlayerForm {
                notes: "  ankle, left \n".into(),
                ..with(" 2005-03-14 ", "185")
            }),
            PlayerDetails {
                birth_date: NaiveDate::from_ymd_opt(2005, 3, 14),
                height_cm: Some(185),
                notes: "ankle, left".into(),
            }
        );
    }

    #[test]
    fn the_birth_date_is_a_date_up_to_today() {
        for date in [
            "14/03/2005",
            "2005-02-30",
            "2005-3-14x",
            "1899-12-31",
            "2026-10-27",
        ] {
            assert_eq!(
                error(with(date, "")),
                PlayerFormError::InvalidBirthDate,
                "{date:?}"
            );
        }
        for date in ["1900-01-01", "2026-10-26", "2005-3-4"] {
            assert!(details(with(date, "")).birth_date.is_some(), "{date:?}");
        }
    }

    #[test]
    fn the_height_is_in_centimetres_from_100_to_250() {
        for height in ["99", "251", "1,85", "1.85", "185cm", "-180"] {
            assert_eq!(
                error(with("", height)),
                PlayerFormError::InvalidHeight,
                "{height:?}"
            );
        }
        for height in ["100", "250", " 178 "] {
            assert!(details(with("", height)).height_cm.is_some(), "{height:?}");
        }
    }

    #[test]
    fn the_number_comes_before_the_birth_date_and_the_height() {
        assert_eq!(
            error(PlayerForm {
                number: "x".into(),
                ..with("x", "x")
            }),
            PlayerFormError::InvalidNumber
        );
        assert_eq!(error(with("x", "x")), PlayerFormError::InvalidBirthDate);
    }

    #[test]
    fn a_player_with_every_detail_validates_back_to_it() {
        let team = team();
        let anna = PlayerEntry {
            birth_date: NaiveDate::from_ymd_opt(2001, 12, 3),
            height_cm: Some(182),
            notes: "captain".into(),
            ..team.players[0].clone()
        };
        let form = PlayerForm::of(&anna);
        assert_eq!(form.birth_date, "2001-12-03");
        assert_eq!(form.height_cm, "182");
        match form.validate(&team, Some(&anna), today()) {
            Ok(PlayerInput::Existing(updated)) => {
                assert_eq!(updated.birth_date, anna.birth_date);
                assert_eq!(updated.height_cm, anna.height_cm);
                assert_eq!(updated.notes, anna.notes);
            }
            _ => unreachable!("expected the existing player"),
        }
    }

    /// Files without the details read as before, and a player without them
    /// is written as before: an older version reads it alike.
    #[test]
    fn the_details_are_written_only_when_given() {
        let old = r#"{"id":"5f0c7e4e-2c55-4d4e-9a57-6e3b1f6f9a10","name":"Anna","role":"Setter","number":7}"#;
        let anna: PlayerEntry = serde_json::from_str(old).unwrap();
        assert_eq!(
            (anna.birth_date, anna.height_cm, anna.notes.as_str()),
            (None, None, "")
        );
        let written = serde_json::to_value(&anna).unwrap();
        let keys: Vec<&str> = written
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["deleted", "id", "name", "number", "role"]);
        let detailed = PlayerEntry {
            birth_date: NaiveDate::from_ymd_opt(2001, 12, 3),
            height_cm: Some(182),
            notes: "captain".into(),
            ..anna
        };
        let written = serde_json::to_value(&detailed).unwrap();
        assert_eq!(written["birth_date"], "2001-12-03");
        assert_eq!(written["height_cm"], 182);
        assert_eq!(written["notes"], "captain");
    }
}
