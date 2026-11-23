//! The form of a training session (TODO/021 step 2): what it validates and
//! the message of each error, in every language.

#[cfg(test)]
mod tests {
    use crate::{
        localization::labels,
        shapes::enums::LanguageEnum,
        training::{
            DEFAULT_SESSION_MINUTES, SessionStatus, TrainingSession,
            form::{SessionForm, SessionFormError},
        },
    };
    use chrono::{NaiveDate, NaiveDateTime};
    use uuid::Uuid;

    fn start() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 23)
            .and_then(|d| d.and_hms_opt(20, 30, 0))
            .unwrap()
    }

    fn form() -> SessionForm {
        SessionForm {
            start: Some(start()),
            ..SessionForm::default()
        }
    }

    #[test]
    fn a_new_form_is_planned_and_of_the_usual_duration() {
        let f = SessionForm::default();
        assert_eq!(f.duration, DEFAULT_SESSION_MINUTES.to_string());
        // two hours, as a session usually lasts (decided 2026-11-23)
        assert_eq!(f.duration, "120");
        assert_eq!(f.status, SessionStatus::Planned);
        assert_eq!(f.start, None);
    }

    #[test]
    fn a_session_needs_its_start() {
        let f = SessionForm::default();
        assert_eq!(f.validate(), Err(SessionFormError::InvalidStart));
    }

    #[test]
    fn the_duration_is_a_number_of_minutes_from_one() {
        for bad in ["", "  ", "0", "-5", "9 0", "1.5", "abc", "65536", "٣"] {
            let f = SessionForm {
                duration: bad.into(),
                ..form()
            };
            assert_eq!(
                f.validate(),
                Err(SessionFormError::InvalidDuration),
                "{bad:?}"
            );
        }
        for (good, minutes) in [("1", 1), (" 90 ", 90), ("65535", 65535), ("007", 7)] {
            let f = SessionForm {
                duration: good.into(),
                ..form()
            };
            assert_eq!(
                f.validate().map(|i| i.duration_min),
                Ok(minutes),
                "{good:?}"
            );
        }
    }

    #[test]
    fn the_actual_duration_may_be_empty_not_wrong() {
        let empty = SessionForm {
            actual_duration: "  ".into(),
            ..form()
        };
        assert_eq!(empty.validate().map(|i| i.actual_duration_min), Ok(None));
        let given = SessionForm {
            actual_duration: " 75".into(),
            ..form()
        };
        assert_eq!(
            given.validate().map(|i| i.actual_duration_min),
            Ok(Some(75))
        );
        for bad in ["0", "x", "-1", "70000"] {
            let f = SessionForm {
                actual_duration: bad.into(),
                ..form()
            };
            assert_eq!(
                f.validate(),
                Err(SessionFormError::InvalidActualDuration),
                "{bad:?}"
            );
        }
    }

    /// The first error in the order of the fields.
    #[test]
    fn the_first_error_wins() {
        let f = SessionForm {
            start: None,
            duration: "0".into(),
            actual_duration: "x".into(),
            ..SessionForm::default()
        };
        assert_eq!(f.validate(), Err(SessionFormError::InvalidStart));
        let f = SessionForm {
            duration: "0".into(),
            actual_duration: "x".into(),
            ..form()
        };
        assert_eq!(f.validate(), Err(SessionFormError::InvalidDuration));
    }

    #[test]
    fn the_texts_are_trimmed() {
        let micro = Uuid::new_v4();
        let f = SessionForm {
            title: " Difesa ".into(),
            objectives: "\nbattuta\n".into(),
            notes: " n ".into(),
            final_remarks: " f ".into(),
            microcycle: Some(micro),
            status: SessionStatus::Completed,
            ..form()
        };
        let input = f.validate().unwrap();
        assert_eq!(
            (
                input.title.as_str(),
                input.objectives.as_str(),
                input.notes.as_str(),
                input.final_remarks.as_str(),
            ),
            ("Difesa", "battuta", "n", "f")
        );
        assert_eq!(input.microcycle_id, Some(micro));
        assert_eq!(input.status, SessionStatus::Completed);
        assert_eq!(input.start, start());
    }

    #[test]
    fn the_form_of_a_session_gives_it_back() {
        let gym = Uuid::new_v4();
        let session = TrainingSession {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            microcycle_id: Some(Uuid::new_v4()),
            series_id: Some(Uuid::new_v4()),
            start: start(),
            duration_min: 120,
            actual_duration_min: Some(100),
            venue_id: Some(gym),
            title: "t".into(),
            objectives: "o".into(),
            notes: "n".into(),
            final_remarks: "f".into(),
            status: SessionStatus::Cancelled,
        };
        let input = SessionForm::of(&session).validate().unwrap();
        assert_eq!(input.microcycle_id, session.microcycle_id);
        assert_eq!(input.start, session.start);
        assert_eq!(input.duration_min, 120);
        assert_eq!(input.actual_duration_min, Some(100));
        assert_eq!(input.status, SessionStatus::Cancelled);
        assert_eq!(input.venue_id, Some(gym));
    }

    #[test]
    fn every_error_has_a_message_in_every_language() {
        for language in LanguageEnum::ALL {
            let l = labels(language);
            for error in [
                SessionFormError::InvalidStart,
                SessionFormError::InvalidDuration,
                SessionFormError::InvalidActualDuration,
            ] {
                assert!(!error.message(l).is_empty(), "{language:?} {error:?}");
            }
            assert_ne!(
                SessionFormError::InvalidDuration.message(l),
                SessionFormError::InvalidStart.message(l)
            );
        }
    }
}
