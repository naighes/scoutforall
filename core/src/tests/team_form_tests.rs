//! The checks of the team form, shared by every user interface.

#[cfg(test)]
mod tests {
    use crate::{
        providers::team_writer::TeamInput,
        shapes::{
            enums::{GenderEnum, TeamClassificationEnum},
            player::PlayerEntry,
            team::TeamEntry,
        },
        team_form::{TeamForm, TeamFormError},
        tests::EN,
    };
    use uuid::Uuid;

    fn form() -> TeamForm {
        TeamForm {
            name: "  Cortona Volley ".into(),
            classification: Some(TeamClassificationEnum::Regional),
            gender: Some(GenderEnum::Women),
            year: "2026".into(),
        }
    }

    fn error(form: TeamForm) -> TeamFormError {
        match form.validate(None) {
            Err(e) => e,
            Ok(_) => unreachable!("the form should not be valid"),
        }
    }

    #[test]
    fn a_valid_form_creates_a_team_with_the_name_trimmed() {
        match form().validate(None) {
            Ok(TeamInput::New {
                id: None,
                name,
                year: 2026,
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Women),
                players,
            }) => {
                assert_eq!(name, "Cortona Volley");
                assert!(players.is_empty());
            }
            _ => unreachable!("expected a new team"),
        }
    }

    #[test]
    fn editing_keeps_the_id_and_the_players() {
        let id = Uuid::new_v4();
        let team = TeamEntry {
            id,
            name: "old".into(),
            year: 2020,
            players: vec![PlayerEntry::default()],
            classification: None,
            gender: None,
        };
        match form().validate(Some(&team)) {
            Ok(TeamInput::Existing(updated)) => {
                assert_eq!(updated.id, id);
                assert_eq!(updated.players.len(), 1);
                assert_eq!(updated.name, "Cortona Volley");
                assert_eq!(updated.year, 2026);
                assert_eq!(updated.gender, Some(GenderEnum::Women));
            }
            _ => unreachable!("expected the existing team"),
        }
    }

    #[test]
    fn the_form_of_a_team_validates_back_to_it() {
        let team = TeamEntry {
            name: "Italy".into(),
            year: 2024,
            classification: Some(TeamClassificationEnum::TopInternational),
            gender: Some(GenderEnum::Men),
            ..Default::default()
        };
        assert_eq!(
            TeamForm::of(&team),
            TeamForm {
                name: "Italy".into(),
                classification: Some(TeamClassificationEnum::TopInternational),
                gender: Some(GenderEnum::Men),
                year: "2024".into(),
            }
        );
    }

    #[test]
    fn a_blank_name_is_rejected() {
        assert_eq!(
            error(TeamForm {
                name: "   ".into(),
                ..form()
            }),
            TeamFormError::EmptyName
        );
    }

    #[test]
    fn classification_and_gender_are_required() {
        assert_eq!(
            error(TeamForm {
                classification: None,
                ..form()
            }),
            TeamFormError::MissingClassification
        );
        assert_eq!(
            error(TeamForm {
                gender: None,
                ..form()
            }),
            TeamFormError::MissingGender
        );
    }

    #[test]
    fn the_year_has_exactly_four_digits() {
        for year in ["", "26", "202", "20266", "２０２６", "20a6", "-202"] {
            assert_eq!(
                error(TeamForm {
                    year: year.into(),
                    ..form()
                }),
                TeamFormError::InvalidYear,
                "{year:?}"
            );
        }
        assert!(
            TeamForm {
                year: " 1999 ".into(),
                ..form()
            }
            .validate(None)
            .is_ok()
        );
    }

    #[test]
    fn the_first_error_follows_the_order_of_the_fields() {
        let empty = TeamForm {
            name: String::new(),
            classification: None,
            gender: None,
            year: String::new(),
        };
        assert_eq!(error(empty), TeamFormError::EmptyName);
    }

    #[test]
    fn the_errors_are_in_the_domain_labels() {
        assert_eq!(
            TeamFormError::InvalidYear.message(EN),
            "year must be a 4-digit number"
        );
        assert_eq!(TeamFormError::EmptyName.message(EN), "name cannot be empty");
    }
}
