//! Duplicating a session (TODO/021 step 8): what is copied, what never is,
//! and the overlap check.

#[cfg(test)]
mod tests {
    use crate::{
        errors::{AppError, TrainingError},
        providers::{
            sqlite::training::SqliteTraining,
            team_writer::{TeamInput, TeamWriter},
            training_reader::TrainingReader,
            training_writer::TrainingWriter,
        },
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, TeamClassificationEnum},
            team::TeamEntry,
        },
        tests::Sqlite,
        training::{
            ActivityStatus, SessionStatus, TrainingSession,
            duplicate::DuplicateInput,
            form::{ActivityForm, SessionForm},
        },
    };
    use chrono::{NaiveDate, NaiveDateTime};

    fn at(m: u32, d: u32, h: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, m, d)
            .and_then(|x| x.and_hms_opt(h, 0, 0))
            .unwrap()
    }

    async fn team(p: &Sqlite) -> TeamEntry {
        p.team_writer
            .save(TeamInput::New {
                id: None,
                name: "A".into(),
                year: 2026,
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Women),
                country: CountryCode::known("IT"),
                players: vec![],
            })
            .await
            .unwrap()
    }

    /// A completed session with everything filled in, and three activities:
    /// one planned, one completed, one planned.
    async fn rich(t: &SqliteTraining, team: &TeamEntry) -> TrainingSession {
        let s = t
            .create_session(
                &team.id,
                SessionForm {
                    start: Some(at(11, 24, 18)),
                    duration: "120".into(),
                    actual_duration: "100".into(),
                    location: "Gym".into(),
                    title: "Serve".into(),
                    objectives: "Reception".into(),
                    notes: "a note".into(),
                    final_remarks: "went well".into(),
                    ..SessionForm::default()
                }
                .validate()
                .unwrap(),
            )
            .await
            .unwrap();
        for (name, planned, status) in [
            ("Warm up", "15", ActivityStatus::Planned),
            ("Serving", "30", ActivityStatus::Completed),
            ("Drill", "", ActivityStatus::Planned),
        ] {
            t.add_activity(
                &s.id,
                ActivityForm {
                    name: name.into(),
                    description: format!("{name} d"),
                    planned: planned.into(),
                    actual: "10".into(),
                    notes: "n".into(),
                    status,
                }
                .validate()
                .unwrap(),
            )
            .await
            .unwrap();
        }
        t.complete_session(&s.id, vec![]).await.unwrap()
    }

    #[test]
    fn the_proposal_is_a_week_later_with_both_boxes_ticked() {
        let p = DuplicateInput::proposed(at(11, 24, 18), None);
        assert_eq!(p.start, at(12, 1, 18));
        assert!(p.copy_objectives && p.copy_activities);
    }

    #[tokio::test]
    async fn copies_what_is_always_copied_and_never_what_is_not() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let original = rich(&t, &team).await;
        let copy = t
            .duplicate_session(&original.id, DuplicateInput::proposed(original.start, None))
            .await
            .unwrap();
        assert_ne!(copy.id, original.id);
        assert_eq!(copy.team_id, team.id);
        assert_eq!(copy.start, at(12, 1, 18));
        assert_eq!(
            (
                copy.duration_min,
                copy.location.as_str(),
                copy.title.as_str()
            ),
            (120, "Gym", "Serve")
        );
        assert_eq!(copy.objectives, "Reception");
        assert_eq!(copy.status, SessionStatus::Planned);
        assert_eq!(copy.actual_duration_min, None);
        assert_eq!((copy.notes.as_str(), copy.final_remarks.as_str()), ("", ""));
        assert_eq!(copy.series_id, None);
    }

    #[tokio::test]
    async fn the_copy_of_a_series_session_is_in_no_series() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let original = rich(&t, &team).await;
        let copy = t
            .duplicate_session(&original.id, DuplicateInput::proposed(original.start, None))
            .await
            .unwrap();
        assert!(copy.series_id.is_none());
        assert_eq!(t.sessions(&team.id).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn the_activities_are_the_planned_ones_in_order_without_actuals() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let original = rich(&t, &team).await;
        let copy = t
            .duplicate_session(&original.id, DuplicateInput::proposed(original.start, None))
            .await
            .unwrap();
        let list = t.activities(&copy.id).await.unwrap();
        assert_eq!(
            list.iter()
                .map(|a| (a.position, a.name.as_str(), a.planned_min))
                .collect::<Vec<_>>(),
            [(0, "Warm up", Some(15)), (1, "Drill", None)]
        );
        assert!(list.iter().all(|a| {
            a.status == ActivityStatus::Planned && a.actual_min.is_none() && a.notes.is_empty()
        }));
        assert_eq!(list[0].description, "Warm up d");
        // the original keeps its three
        assert_eq!(t.activities(&original.id).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn the_boxes_leave_out_objectives_and_activities() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let original = rich(&t, &team).await;
        let copy = t
            .duplicate_session(
                &original.id,
                DuplicateInput {
                    copy_objectives: false,
                    copy_activities: false,
                    ..DuplicateInput::proposed(original.start, None)
                },
            )
            .await
            .unwrap();
        assert_eq!(copy.objectives, "");
        assert!(t.activities(&copy.id).await.unwrap().is_empty());
        assert_eq!(
            (
                copy.duration_min,
                copy.location.as_str(),
                copy.title.as_str()
            ),
            (120, "Gym", "Serve")
        );
    }

    #[tokio::test]
    async fn no_attendance_is_written() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let original = rich(&t, &team).await;
        let copy = t
            .duplicate_session(&original.id, DuplicateInput::proposed(original.start, None))
            .await
            .unwrap();
        assert!(!t.attendance(&copy.id).await.unwrap().recorded);
    }

    #[tokio::test]
    async fn an_overlap_writes_nothing_and_says_which() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let original = rich(&t, &team).await;
        let err = t
            .duplicate_session(
                &original.id,
                DuplicateInput {
                    start: at(11, 24, 19),
                    ..DuplicateInput::proposed(original.start, None)
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            AppError::Training(TrainingError::Overlap(ref s)) if s.len() == 1
        ));
        assert_eq!(t.sessions(&team.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_missing_session_is_not_found() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let err = t
            .duplicate_session(
                &uuid::Uuid::new_v4(),
                DuplicateInput::proposed(at(11, 24, 18), None),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
