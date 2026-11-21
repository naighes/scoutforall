//! The activities of a session (TODO/021 step 4): the form, the order, the
//! search to copy one from another session, and the setting that remembers
//! where the search looks.

#[cfg(test)]
mod tests {
    use crate::{
        errors::AppError,
        localization::labels,
        providers::{
            sqlite::{Database, teams::SqliteTeamWriter, training::SqliteTraining},
            team_writer::{TeamInput, TeamWriter},
            training_reader::TrainingReader,
            training_writer::TrainingWriter,
        },
        settings::ActivitySearch,
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, LanguageEnum, TeamClassificationEnum},
            team::TeamEntry,
        },
        training::{
            Activity, ActivityHit, ActivityStatus, Move, SessionInput,
            form::{ActivityForm, ActivityFormError, SessionForm},
        },
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

    fn at(day: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, day)
            .and_then(|d| d.and_hms_opt(18, 0, 0))
            .unwrap()
    }

    fn session_input(day: u32) -> SessionInput {
        SessionForm {
            start: Some(at(day)),
            ..SessionForm::default()
        }
        .validate()
        .unwrap()
    }

    async fn session(p: &P, team: &TeamEntry, day: u32) -> Uuid {
        p.training
            .create_session(&team.id, session_input(day))
            .await
            .unwrap()
            .id
    }

    fn form(name: &str) -> ActivityForm {
        ActivityForm {
            name: name.into(),
            ..ActivityForm::default()
        }
    }

    async fn add(p: &P, session: &Uuid, name: &str) -> Activity {
        p.training
            .add_activity(session, form(name).validate().unwrap())
            .await
            .unwrap()
    }

    async fn names(p: &P, session: &Uuid) -> Vec<String> {
        p.training
            .activities(session)
            .await
            .unwrap()
            .into_iter()
            .map(|a| a.name)
            .collect()
    }

    fn is_not_found(e: &AppError) -> bool {
        e.to_string().contains("not found")
    }

    // the form

    #[test]
    fn a_valid_form_trims_and_the_minutes_may_be_empty() {
        let input = ActivityForm {
            name: "  Serve ".into(),
            description: " from 6 ".into(),
            planned: " 20 ".into(),
            notes: " n ".into(),
            ..ActivityForm::default()
        }
        .validate()
        .unwrap();
        assert_eq!(input.name, "Serve");
        assert_eq!(input.description, "from 6");
        assert_eq!(input.planned_min, Some(20));
        assert_eq!(input.actual_min, None);
        assert_eq!(input.notes, "n");
        assert_eq!(input.status, ActivityStatus::Planned);
    }

    #[test]
    fn the_form_refuses_a_missing_name_and_bad_minutes_in_field_order() {
        let bad = |planned: &str, actual: &str| ActivityForm {
            planned: planned.into(),
            actual: actual.into(),
            ..form("x")
        };
        assert_eq!(form("  ").validate(), Err(ActivityFormError::EmptyName));
        assert_eq!(
            ActivityForm {
                planned: "x".into(),
                ..form("")
            }
            .validate(),
            Err(ActivityFormError::EmptyName)
        );
        for planned in ["0", "-5", "1.5", "abc", "70000"] {
            assert_eq!(
                bad(planned, "").validate(),
                Err(ActivityFormError::InvalidPlanned),
                "{planned}"
            );
            assert_eq!(
                bad("", planned).validate(),
                Err(ActivityFormError::InvalidActual),
                "{planned}"
            );
        }
        assert_eq!(
            bad("x", "y").validate(),
            Err(ActivityFormError::InvalidPlanned)
        );
    }

    #[test]
    fn editing_and_copying_start_from_what_is_there() {
        let activity = Activity {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            position: 3,
            name: "Dig".into(),
            description: "d".into(),
            planned_min: Some(15),
            actual_min: Some(10),
            notes: "n".into(),
            status: ActivityStatus::Skipped,
        };
        let input = ActivityForm::of(&activity).validate().unwrap();
        assert_eq!(input.actual_min, Some(10));
        assert_eq!(input.status, ActivityStatus::Skipped);
        let hit = ActivityHit {
            name: "Dig".into(),
            description: "d".into(),
            planned_min: Some(15),
            team_id: Uuid::new_v4(),
            team_name: "A".into(),
            session_start: at(1),
        };
        let copy = ActivityForm::copy_of(&hit).validate().unwrap();
        assert_eq!(
            (copy.name.as_str(), copy.planned_min, copy.actual_min),
            ("Dig", Some(15), None)
        );
        assert_eq!(copy.status, ActivityStatus::Planned);
        assert_eq!(copy.notes, "");
    }

    #[test]
    fn every_form_error_has_a_message_in_every_language() {
        for language in LanguageEnum::ALL {
            let l = labels(language);
            for error in [
                ActivityFormError::EmptyName,
                ActivityFormError::InvalidPlanned,
                ActivityFormError::InvalidActual,
            ] {
                assert!(!error.message(l).is_empty(), "{language:?} {error:?}");
            }
        }
    }

    // the order

    #[tokio::test]
    async fn the_activities_are_added_at_the_end_and_read_in_order() {
        let p = providers();
        let t = team(&p, "A").await;
        let s = session(&p, &t, 23).await;
        let first = add(&p, &s, "one").await;
        add(&p, &s, "two").await;
        add(&p, &s, "three").await;
        assert_eq!(first.position, 0);
        assert_eq!(first.session_id, s);
        assert_eq!(names(&p, &s).await, ["one", "two", "three"]);
        let other = session(&p, &t, 24).await;
        assert!(names(&p, &other).await.is_empty());
    }

    #[tokio::test]
    async fn an_activity_is_edited_without_changing_place_or_session() {
        let p = providers();
        let t = team(&p, "A").await;
        let s = session(&p, &t, 23).await;
        add(&p, &s, "one").await;
        let two = add(&p, &s, "two").await;
        let edited = p
            .training
            .update_activity(
                &two.id,
                ActivityForm {
                    name: "TWO".into(),
                    planned: "30".into(),
                    actual: "25".into(),
                    status: ActivityStatus::Completed,
                    ..ActivityForm::default()
                }
                .validate()
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!((edited.position, edited.session_id), (1, s));
        assert_eq!(edited.status, ActivityStatus::Completed);
        assert_eq!(edited.planned_min, Some(30));
        assert_eq!(names(&p, &s).await, ["one", "TWO"]);
    }

    #[tokio::test]
    async fn moving_swaps_with_the_neighbour_and_the_ends_stay() {
        let p = providers();
        let t = team(&p, "A").await;
        let s = session(&p, &t, 23).await;
        let one = add(&p, &s, "one").await;
        let two = add(&p, &s, "two").await;
        let three = add(&p, &s, "three").await;
        p.training.move_activity(&three.id, Move::Up).await.unwrap();
        assert_eq!(names(&p, &s).await, ["one", "three", "two"]);
        p.training.move_activity(&one.id, Move::Down).await.unwrap();
        assert_eq!(names(&p, &s).await, ["three", "one", "two"]);
        p.training.move_activity(&three.id, Move::Up).await.unwrap();
        p.training.move_activity(&two.id, Move::Down).await.unwrap();
        assert_eq!(names(&p, &s).await, ["three", "one", "two"]);
    }

    /// A removed activity keeps its place in the database, but the others
    /// move across it and a new one goes after the last, never in the gap.
    #[tokio::test]
    async fn a_removed_activity_leaves_a_gap_the_order_ignores() {
        let p = providers();
        let t = team(&p, "A").await;
        let s = session(&p, &t, 23).await;
        let one = add(&p, &s, "one").await;
        let two = add(&p, &s, "two").await;
        let three = add(&p, &s, "three").await;
        p.training.remove_activity(&two.id).await.unwrap();
        assert_eq!(names(&p, &s).await, ["one", "three"]);
        p.training.move_activity(&three.id, Move::Up).await.unwrap();
        assert_eq!(names(&p, &s).await, ["three", "one"]);
        let four = add(&p, &s, "four").await;
        assert!(four.position > three.position.max(one.position));
        assert_eq!(names(&p, &s).await, ["three", "one", "four"]);
        // removed, not deleted from the table
        let rows: i64 =
            p.db.read(|tx| {
                tx.query_row("SELECT COUNT(*) FROM training_activities", [], |r| r.get(0))
                    .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        assert_eq!(rows, 4);
    }

    #[tokio::test]
    async fn a_missing_or_removed_target_is_not_found() {
        let p = providers();
        let t = team(&p, "A").await;
        let s = session(&p, &t, 23).await;
        let one = add(&p, &s, "one").await;
        let input = form("x").validate().unwrap();
        let nobody = Uuid::new_v4();
        assert!(is_not_found(
            &p.training
                .add_activity(&nobody, input.clone())
                .await
                .unwrap_err()
        ));
        assert!(is_not_found(
            &p.training
                .update_activity(&nobody, input.clone())
                .await
                .unwrap_err()
        ));
        assert!(is_not_found(
            &p.training.remove_activity(&nobody).await.unwrap_err()
        ));
        assert!(is_not_found(
            &p.training
                .move_activity(&nobody, Move::Up)
                .await
                .unwrap_err()
        ));
        p.training.remove_activity(&one.id).await.unwrap();
        assert!(is_not_found(
            &p.training.remove_activity(&one.id).await.unwrap_err()
        ));
        assert!(is_not_found(
            &p.training
                .update_activity(&one.id, input)
                .await
                .unwrap_err()
        ));
    }

    #[tokio::test]
    async fn removing_the_session_hides_its_activities() {
        let p = providers();
        let t = team(&p, "A").await;
        let s = session(&p, &t, 23).await;
        let one = add(&p, &s, "one").await;
        p.training.remove_session(&s).await.unwrap();
        assert!(names(&p, &s).await.is_empty());
        assert!(is_not_found(
            &p.training
                .move_activity(&one.id, Move::Down)
                .await
                .unwrap_err()
        ));
        assert!(is_not_found(
            &p.training
                .add_activity(&s, form("x").validate().unwrap())
                .await
                .unwrap_err()
        ));
        assert!(
            p.training
                .search_activities(&t.id, "", false, 10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    // the search

    async fn added(p: &P, session: &Uuid, name: &str, description: &str, planned: &str) {
        p.training
            .add_activity(
                session,
                ActivityForm {
                    description: description.into(),
                    planned: planned.into(),
                    ..form(name)
                }
                .validate()
                .unwrap(),
            )
            .await
            .unwrap();
    }

    fn found(hits: &[ActivityHit]) -> Vec<(&str, &str)> {
        hits.iter()
            .map(|h| (h.name.as_str(), h.team_name.as_str()))
            .collect()
    }

    #[tokio::test]
    async fn the_search_matches_the_name_whatever_the_case() {
        let p = providers();
        let t = team(&p, "A").await;
        let s = session(&p, &t, 23).await;
        added(&p, &s, "Serve and Receive", "", "").await;
        added(&p, &s, "ÉCHAUFFEMENT", "", "").await;
        added(&p, &s, "Block", "", "").await;
        let hits = |q: &str| {
            let p = &p;
            let id = t.id;
            let q = q.to_string();
            async move {
                p.training
                    .search_activities(&id, &q, false, 10)
                    .await
                    .unwrap()
            }
        };
        assert_eq!(found(&hits("RECEIVE").await), [("Serve and Receive", "A")]);
        assert_eq!(found(&hits("  échauf ").await), [("ÉCHAUFFEMENT", "A")]);
        assert_eq!(hits("").await.len(), 3);
        assert!(hits("zzz").await.is_empty());
    }

    #[tokio::test]
    async fn the_team_comes_first_then_the_latest_and_the_same_text_once() {
        let p = providers();
        let a = team(&p, "A").await;
        let b = team(&p, "B").await;
        let (sa1, sa2) = (session(&p, &a, 10).await, session(&p, &a, 12).await);
        let sb = session(&p, &b, 20).await;
        added(&p, &sa1, "Pepper", "old", "10").await;
        added(&p, &sa2, "Pepper", "old", "15").await;
        added(&p, &sa1, "Pepper", "other text", "").await;
        added(&p, &sb, "Pepper", "old", "99").await;
        added(&p, &sb, "Pepper", "b only", "").await;
        let hits = p
            .training
            .search_activities(&a.id, "pepper", false, 10)
            .await
            .unwrap();
        assert_eq!(hits.len(), 3);
        // the team of the open one first, the latest of its copies kept
        assert_eq!(
            (
                hits[0].team_name.as_str(),
                hits[0].description.as_str(),
                hits[0].planned_min
            ),
            ("A", "old", Some(15))
        );
        assert_eq!(hits[0].session_start, at(12));
        assert_eq!(hits[1].description, "other text");
        assert_eq!(
            (hits[2].team_id, hits[2].description.as_str()),
            (b.id, "b only")
        );
        // from the point of view of the other team
        let from_b = p
            .training
            .search_activities(&b.id, "pepper", false, 10)
            .await
            .unwrap();
        assert_eq!(from_b[0].team_id, b.id);
        assert_eq!(from_b[0].planned_min, Some(99));
    }

    #[tokio::test]
    async fn only_this_team_and_the_limit_and_the_removed_ones() {
        let p = providers();
        let a = team(&p, "A").await;
        let b = team(&p, "B").await;
        let sa = session(&p, &a, 10).await;
        let sb = session(&p, &b, 11).await;
        added(&p, &sa, "mine 1", "", "").await;
        added(&p, &sa, "mine 2", "", "").await;
        added(&p, &sb, "theirs", "", "").await;
        let all = p
            .training
            .search_activities(&a.id, "", false, 10)
            .await
            .unwrap();
        assert_eq!(
            found(&all),
            [("mine 1", "A"), ("mine 2", "A"), ("theirs", "B")]
        );
        let only = p
            .training
            .search_activities(&a.id, "", true, 10)
            .await
            .unwrap();
        assert_eq!(only.len(), 2);
        assert_eq!(
            p.training
                .search_activities(&a.id, "", false, 1)
                .await
                .unwrap()
                .len(),
            1
        );
        let gone = p.training.activities(&sb).await.unwrap().remove(0);
        p.training.remove_activity(&gone.id).await.unwrap();
        assert_eq!(
            p.training
                .search_activities(&a.id, "", false, 10)
                .await
                .unwrap()
                .len(),
            2
        );
    }

    // the setting

    #[test]
    fn the_search_setting_is_remembered_and_left_out_when_off() {
        assert!(!ActivitySearch::default().activity_search_team_only);
        assert_eq!(
            serde_json::to_string(&ActivitySearch::default()).unwrap(),
            "{}"
        );
        let on: ActivitySearch =
            serde_json::from_str(r#"{"activity_search_team_only":true,"language":"it"}"#).unwrap();
        assert!(on.activity_search_team_only);
        assert_eq!(
            serde_json::to_value(on).unwrap()["activity_search_team_only"],
            true
        );
    }

    #[test]
    fn an_unreadable_search_setting_means_every_team() {
        for json in [
            r#"{"activity_search_team_only":"yes"}"#,
            r#"{"activity_search_team_only":null}"#,
            r#"{"activity_search_team_only":3}"#,
            "{}",
        ] {
            let read: ActivitySearch = serde_json::from_str(json).unwrap();
            assert!(!read.activity_search_team_only, "{json}");
        }
    }
}
