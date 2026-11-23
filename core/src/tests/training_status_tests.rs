//! The status of a session (TODO/021 step 6): complete, cancel, reopen, and
//! "to close" with the clock given.

#[cfg(test)]
mod tests {
    use crate::{
        providers::{
            sqlite::{Database, teams::SqliteTeamWriter, training::SqliteTraining},
            team_writer::{TeamInput, TeamWriter},
            training_reader::TrainingReader,
            training_writer::TrainingWriter,
        },
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, TeamClassificationEnum},
            team::TeamEntry,
        },
        training::{
            Activity, ActivityInput, ActivityStatus, SessionStatus, TrainingSession,
            form::SessionForm,
        },
    };
    use chrono::{NaiveDate, NaiveDateTime};
    use uuid::Uuid;

    fn providers() -> (SqliteTeamWriter, SqliteTraining) {
        let db = Database::new(crate::db::open_in_memory().unwrap());
        (SqliteTeamWriter::new(db.clone()), SqliteTraining::new(db))
    }

    async fn team(teams: &SqliteTeamWriter) -> TeamEntry {
        teams
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

    fn at(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 23)
            .and_then(|d| d.and_hms_opt(hour, minute, 0))
            .unwrap()
    }

    async fn session(training: &SqliteTraining, team: &Uuid) -> TrainingSession {
        let input = SessionForm {
            start: Some(at(20, 0)),
            duration: "90".into(),
            ..SessionForm::default()
        }
        .validate()
        .unwrap();
        training.create_session(team, input).await.unwrap()
    }

    async fn activity(
        training: &SqliteTraining,
        session: &Uuid,
        name: &str,
        status: ActivityStatus,
    ) -> Activity {
        training
            .add_activity(
                session,
                ActivityInput {
                    name: name.into(),
                    description: String::new(),
                    planned_min: Some(10),
                    actual_min: None,
                    notes: String::new(),
                    status,
                },
            )
            .await
            .unwrap()
    }

    async fn statuses(training: &SqliteTraining, session: &Uuid) -> Vec<ActivityStatus> {
        training
            .activities(session)
            .await
            .unwrap()
            .iter()
            .map(|a| a.status)
            .collect()
    }

    #[tokio::test]
    async fn a_planned_session_is_to_close_only_when_it_is_over() {
        let (teams, training) = providers();
        let t = team(&teams).await;
        let s = session(&training, &t.id).await;
        // 20:00 + 90 minutes = 21:30
        assert!(!s.is_to_close(at(19, 0)));
        assert!(!s.is_to_close(at(21, 29)));
        assert!(!s.is_to_close(at(21, 30)));
        assert!(s.is_to_close(at(21, 31)));
    }

    #[tokio::test]
    async fn a_completed_or_cancelled_session_is_never_to_close() {
        let (teams, training) = providers();
        let t = team(&teams).await;
        let s = session(&training, &t.id).await;
        let late = at(23, 59);
        for status in [SessionStatus::Completed, SessionStatus::Cancelled] {
            let changed = training.set_session_status(&s.id, status).await.unwrap();
            assert_eq!(changed.status, status);
            assert!(!changed.is_to_close(late));
        }
        let reopened = training
            .set_session_status(&s.id, SessionStatus::Planned)
            .await
            .unwrap();
        assert!(reopened.is_to_close(late));
    }

    #[tokio::test]
    async fn the_actual_duration_counts_the_planned_one_when_empty() {
        let (teams, training) = providers();
        let t = team(&teams).await;
        let mut s = session(&training, &t.id).await;
        assert_eq!(s.counted_minutes(), 90);
        s.actual_duration_min = Some(60);
        assert_eq!(s.counted_minutes(), 60);
    }

    #[tokio::test]
    async fn changing_the_status_touches_nothing_else() {
        let (teams, training) = providers();
        let t = team(&teams).await;
        let s = session(&training, &t.id).await;
        let a = activity(&training, &s.id, "serve", ActivityStatus::Planned).await;
        let cancelled = training
            .set_session_status(&s.id, SessionStatus::Cancelled)
            .await
            .unwrap();
        assert_eq!(
            TrainingSession {
                status: s.status,
                ..cancelled
            },
            s
        );
        // cancelling leaves the activities as they are
        assert_eq!(statuses(&training, &s.id).await, [a.status]);
    }

    #[tokio::test]
    async fn completing_marks_the_chosen_planned_activities() {
        let (teams, training) = providers();
        let t = team(&teams).await;
        let s = session(&training, &t.id).await;
        let a = activity(&training, &s.id, "a", ActivityStatus::Planned).await;
        let b = activity(&training, &s.id, "b", ActivityStatus::Planned).await;
        let c = activity(&training, &s.id, "c", ActivityStatus::Skipped).await;
        let done = training
            .complete_session(&s.id, vec![a.id, c.id])
            .await
            .unwrap();
        assert_eq!(done.status, SessionStatus::Completed);
        assert_eq!(
            statuses(&training, &s.id).await,
            [
                ActivityStatus::Completed,
                ActivityStatus::Planned,
                // a skipped one stays skipped
                ActivityStatus::Skipped
            ]
        );
        assert_eq!(b.status, ActivityStatus::Planned);
    }

    #[tokio::test]
    async fn completing_with_no_activity_chosen_changes_only_the_session() {
        let (teams, training) = providers();
        let t = team(&teams).await;
        let s = session(&training, &t.id).await;
        activity(&training, &s.id, "a", ActivityStatus::Planned).await;
        training.complete_session(&s.id, vec![]).await.unwrap();
        assert_eq!(statuses(&training, &s.id).await, [ActivityStatus::Planned]);
    }

    #[tokio::test]
    async fn an_activity_of_another_session_stops_the_completion() {
        let (teams, training) = providers();
        let t = team(&teams).await;
        let s = session(&training, &t.id).await;
        let other = training
            .create_session(
                &t.id,
                SessionForm {
                    start: Some(at(8, 0)),
                    duration: "30".into(),
                    ..SessionForm::default()
                }
                .validate()
                .unwrap(),
            )
            .await
            .unwrap();
        let mine = activity(&training, &s.id, "a", ActivityStatus::Planned).await;
        let theirs = activity(&training, &other.id, "b", ActivityStatus::Planned).await;
        let err = training
            .complete_session(&s.id, vec![mine.id, theirs.id])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
        // nothing was written: all or none
        assert_eq!(
            training.session(&s.id).await.unwrap().status,
            SessionStatus::Planned
        );
        assert_eq!(statuses(&training, &s.id).await, [ActivityStatus::Planned]);
        assert_eq!(
            statuses(&training, &other.id).await,
            [ActivityStatus::Planned]
        );
    }

    #[tokio::test]
    async fn a_removed_session_has_no_status_to_change() {
        let (teams, training) = providers();
        let t = team(&teams).await;
        let s = session(&training, &t.id).await;
        training.remove_session(&s.id).await.unwrap();
        for e in [
            training
                .set_session_status(&s.id, SessionStatus::Completed)
                .await
                .unwrap_err(),
            training.complete_session(&s.id, vec![]).await.unwrap_err(),
        ] {
            assert!(e.to_string().contains("not found"));
        }
    }
}
