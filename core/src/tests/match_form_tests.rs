//! The checks of the match form, shared by every user interface.

#[cfg(test)]
mod tests {
    use crate::{
        match_form::{MatchForm, MatchFormError, MatchInput},
        shapes::{
            enums::{EventTypeEnum, TeamSideEnum},
            r#match::MatchEntry,
            set::SetEntry,
            snapshot::EventEntry,
            team::TeamEntry,
        },
        tests::EN,
    };
    use chrono::{DateTime, FixedOffset, Utc};
    use uuid::Uuid;

    fn date() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-24T18:00:00+02:00").unwrap_or_default()
    }

    fn form(max_substitutions: &str) -> MatchForm {
        MatchForm {
            opponent: " Arezzo ".into(),
            date: Some(date()),
            home: true,
            max_substitutions: max_substitutions.into(),
        }
    }

    fn substitution() -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type: EventTypeEnum::R,
            player: Some(Uuid::new_v4()),
            eval: None,
            target_player: Some(Uuid::new_v4()),
            technique: None,
        }
    }

    fn set(set_number: u8, substitutions: usize) -> SetEntry {
        SetEntry {
            set_number,
            serving_team: TeamSideEnum::Us,
            initial_positions: [Uuid::nil(); 6],
            libero: Uuid::nil(),
            fallback_libero: None,
            setter: Uuid::nil(),
            lineup_rules: None,
            events: (0..substitutions).map(|_| substitution()).collect(),
        }
    }

    /// A match with 2 substitutions in the first set and 4 in the second.
    fn played() -> MatchEntry {
        MatchEntry {
            opponent: "Arezzo".into(),
            date: date(),
            id: "2026-10-24_arezzo".into(),
            team: TeamEntry::default(),
            home: false,
            max_substitutions: 6,
            sets: vec![set(1, 2), set(2, 4)],
        }
    }

    fn error(form: MatchForm) -> MatchFormError {
        match form.validate(None) {
            Err(e) => e,
            Ok(_) => unreachable!("the form should not be valid"),
        }
    }

    #[test]
    fn a_valid_form_has_the_opponent_trimmed() {
        assert_eq!(
            form("6").validate(None),
            Ok(MatchInput {
                opponent: "Arezzo".into(),
                date: date(),
                home: true,
                max_substitutions: 6,
            })
        );
    }

    #[test]
    fn a_new_form_starts_on_the_date_with_the_default_substitutions() {
        let new = MatchForm::new(date());
        assert_eq!(new.date, Some(date()));
        assert_eq!(new.max_substitutions, "8");
        assert!(new.opponent.is_empty());
        assert!(!new.home);
    }

    #[test]
    fn the_substitutions_are_from_1_to_99() {
        for max in ["", "0", "00", "100", "-1", "6a", "６"] {
            assert_eq!(
                error(form(max)),
                MatchFormError::InvalidMaxSubstitutions,
                "{max:?}"
            );
        }
        for max in ["1", "99", " 6 ", "08"] {
            assert!(form(max).validate(None).is_ok(), "{max:?}");
        }
    }

    #[test]
    fn editing_keeps_at_least_the_substitutions_of_a_set() {
        let m = played();
        assert_eq!(
            form("3").validate(Some(&m)),
            Err(MatchFormError::MaxSubstitutionsBelowUsed)
        );
        assert!(form("4").validate(Some(&m)).is_ok());
        // a new match has no substitutions yet
        assert!(form("1").validate(None).is_ok());
    }

    #[test]
    fn the_errors_are_in_the_order_of_the_fields() {
        assert_eq!(
            error(MatchForm {
                opponent: "  ".into(),
                date: None,
                home: false,
                max_substitutions: String::new(),
            }),
            MatchFormError::EmptyOpponent
        );
        assert_eq!(
            error(MatchForm {
                date: None,
                ..form("")
            }),
            MatchFormError::InvalidDate
        );
    }

    #[test]
    fn the_form_of_a_match_validates_back_to_it() {
        let m = played();
        assert_eq!(
            MatchForm::of(&m),
            MatchForm {
                opponent: "Arezzo".into(),
                date: Some(date()),
                home: false,
                max_substitutions: "6".into(),
            }
        );
        assert!(MatchForm::of(&m).validate(Some(&m)).is_ok());
    }

    #[test]
    fn the_errors_are_in_the_domain_labels() {
        assert_eq!(
            MatchFormError::EmptyOpponent.message(EN),
            "opponent cannot be empty"
        );
        assert_eq!(MatchFormError::InvalidDate.message(EN), "invalid date");
    }
}
