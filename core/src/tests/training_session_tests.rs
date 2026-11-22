//! The sessions of a team in the database (TODO/021 step 2): create, read,
//! edit and remove; the places used; a team's sessions apart from another's.

#[cfg(test)]
mod tests {
    use crate::{
        errors::AppError,
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
        training::{SessionInput, SessionStatus, form::SessionForm},
    };
    use chrono::{NaiveDate, NaiveDateTime};
    use uuid::Uuid;

    struct P {
        db: Database,
        teams: SqliteTeamWriter,
        training: SqliteTraining,
    }

    fn providers() -> P {
        let db = Database::new(crate::db::open_in_memory().unwrap());
        P {
            teams: SqliteTeamWriter::new(db.clone()),
            training: SqliteTraining::new(db.clone()),
            db,
        }
    }

    async fn team(p: &P, name: &str) -> TeamEntry {
        p.teams
            .save(TeamInput::New {
                id: None,
                name: name.into(),
                year: 2026,
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Women),
                country: CountryCode::known("IT"),
                players: vec![],
            })
            .await
            .unwrap()
    }

    fn at(day: u32, hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, day)
            .and_then(|d| d.and_hms_opt(hour, minute, 0))
            .unwrap()
    }

    fn input(start: NaiveDateTime, venue: Option<Uuid>) -> SessionInput {
        SessionForm {
            start: Some(start),
            venue,
            ..SessionForm::default()
        }
        .validate()
        .unwrap()
    }

    fn is_not_found(e: &AppError) -> bool {
        e.to_string().contains("not found")
    }

    #[tokio::test]
    async fn a_session_is_saved_and_read_back() {
        let p = providers();
        let t = team(&p, "A").await;
        let made = SessionForm {
            start: Some(at(23, 20, 30)),
            duration: "120".into(),
            actual_duration: "100".into(),
            venue: Some(crate::tests::gym(&p.db)),
            title: "Defense".into(),
            objectives: "dig".into(),
            notes: "n".into(),
            final_remarks: "f".into(),
            status: SessionStatus::Completed,
            ..SessionForm::default()
        }
        .validate()
        .unwrap();
        let session = p.training.create_session(&t.id, made).await.unwrap();
        assert_eq!(session.team_id, t.id);
        assert_eq!(session.series_id, None);
        assert_eq!(session.start, at(23, 20, 30));
        assert_eq!(session.duration_min, 120);
        assert_eq!(session.actual_duration_min, Some(100));
        assert_eq!(session.status, SessionStatus::Completed);
        assert_eq!(p.training.session(&session.id).await.unwrap(), session);
        assert_eq!(p.training.sessions(&t.id).await.unwrap(), [session]);
    }

    /// The time written is the time read, to the minute: the database
    /// keeps what the user wrote, with no offset.
    #[tokio::test]
    async fn the_local_time_is_kept_as_written() {
        let p = providers();
        let t = team(&p, "A").await;
        p.training
            .create_session(&t.id, input(at(29, 0, 5), None))
            .await
            .unwrap();
        let stored: String =
            p.db.read(|tx| {
                tx.query_row("SELECT start_local FROM training_sessions", [], |r| {
                    r.get(0)
                })
                .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        assert_eq!(stored, "2026-11-29T00:05");
    }

    #[tokio::test]
    async fn the_sessions_come_by_start_and_by_team() {
        let p = providers();
        let a = team(&p, "A").await;
        let b = team(&p, "B").await;
        let late = p
            .training
            .create_session(&a.id, input(at(25, 20, 0), None))
            .await
            .unwrap();
        let early = p
            .training
            .create_session(&a.id, input(at(24, 18, 0), None))
            .await
            .unwrap();
        let other = p
            .training
            .create_session(&b.id, input(at(24, 18, 0), None))
            .await
            .unwrap();
        assert_eq!(p.training.sessions(&a.id).await.unwrap(), [early, late]);
        assert_eq!(p.training.sessions(&b.id).await.unwrap(), [other]);
    }

    #[tokio::test]
    async fn an_edit_changes_the_fields_and_keeps_team_and_series() {
        let p = providers();
        let t = team(&p, "A").await;
        let s = p
            .training
            .create_session(&t.id, input(at(23, 20, 30), Some(crate::tests::gym(&p.db))))
            .await
            .unwrap();
        p.db.write(|tx| {
            tx.execute(
                "UPDATE training_sessions SET series_id = ?1",
                [Uuid::new_v4().to_string()],
            )
            .map_err(crate::providers::sqlite::sqlite)
        })
        .unwrap();
        let series = p.training.session(&s.id).await.unwrap().series_id;
        assert!(series.is_some());
        let edited = SessionForm {
            duration: "60".into(),
            title: "New".into(),
            status: SessionStatus::Cancelled,
            ..SessionForm::of(&s)
        };
        let edited = SessionForm {
            start: Some(at(24, 19, 0)),
            ..edited
        }
        .validate()
        .unwrap();
        let after = p.training.update_session(&s.id, edited).await.unwrap();
        assert_eq!(after.id, s.id);
        assert_eq!(after.team_id, t.id);
        assert_eq!(after.series_id, series);
        assert_eq!(after.start, at(24, 19, 0));
        assert_eq!(after.duration_min, 60);
        assert_eq!(after.title, "New");
        assert_eq!(after.status, SessionStatus::Cancelled);
        assert_eq!(p.training.session(&s.id).await.unwrap(), after);
    }

    #[tokio::test]
    async fn a_removed_session_is_no_longer_read_nor_edited_nor_removed() {
        let p = providers();
        let t = team(&p, "A").await;
        let s = p
            .training
            .create_session(&t.id, input(at(23, 20, 30), Some(crate::tests::gym(&p.db))))
            .await
            .unwrap();
        let keep = p
            .training
            .create_session(&t.id, input(at(24, 20, 30), None))
            .await
            .unwrap();
        p.training.remove_session(&s.id).await.unwrap();
        assert_eq!(p.training.sessions(&t.id).await.unwrap(), [keep]);
        assert!(is_not_found(&p.training.session(&s.id).await.unwrap_err()));
        assert!(is_not_found(
            &p.training
                .update_session(&s.id, input(at(23, 20, 30), None))
                .await
                .unwrap_err()
        ));
        assert!(is_not_found(
            &p.training.remove_session(&s.id).await.unwrap_err()
        ));
        // never removed for good (D7)
        let rows: i64 =
            p.db.read(|tx| {
                tx.query_row("SELECT COUNT(*) FROM training_sessions", [], |r| r.get(0))
                    .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        assert_eq!(rows, 2);
    }

    #[tokio::test]
    async fn a_session_needs_a_team_that_is_there() {
        let p = providers();
        let t = team(&p, "A").await;
        let missing = p
            .training
            .create_session(&Uuid::new_v4(), input(at(23, 20, 0), None))
            .await;
        assert!(is_not_found(&missing.unwrap_err()));
        p.teams.remove(&t.id).await.unwrap();
        let removed = p
            .training
            .create_session(&t.id, input(at(23, 20, 0), None))
            .await;
        assert!(is_not_found(&removed.unwrap_err()));
    }

    /// A microcycle of another team is refused by the schema.
    #[tokio::test]
    async fn a_microcycle_of_another_team_is_refused() {
        let p = providers();
        let a = team(&p, "A").await;
        let b = team(&p, "B").await;
        let micro = Uuid::new_v4();
        p.db.write(|tx| {
            tx.execute(
                "INSERT INTO microcycles (id, team_id, start_date, weeks)
                     VALUES (?1, ?2, '2026-11-23', 1)",
                [micro.to_string(), b.id.to_string()],
            )
            .map_err(crate::providers::sqlite::sqlite)
        })
        .unwrap();
        let form = SessionForm {
            start: Some(at(24, 20, 0)),
            microcycle: Some(micro),
            ..SessionForm::default()
        };
        assert!(
            p.training
                .create_session(&a.id, form.validate().unwrap())
                .await
                .is_err()
        );
        assert!(p.training.sessions(&a.id).await.unwrap().is_empty());
    }
}
