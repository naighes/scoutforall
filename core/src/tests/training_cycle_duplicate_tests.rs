//! Duplicating a microcycle (TODO/021 step 10): the copies, what they keep,
//! the mesocycle, and the overlaps that stop them all.

#[cfg(test)]
mod tests {
    use crate::{
        errors::{AppError, TrainingError},
        providers::{
            sqlite::training::SqliteTraining,
            team_writer::{TeamInput, TeamWriter},
            training_cycles::TrainingCycles,
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
            ActivityStatus, SessionStatus,
            cycles::{Cycle, CycleInput, Level, Span},
            duplicate::DuplicateCycleInput,
            form::{ActivityForm, SessionForm},
        },
    };
    use chrono::{NaiveDate, NaiveDateTime};
    use uuid::Uuid;

    fn day(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, d).unwrap()
    }

    fn at(m: u32, d: u32, h: u32) -> NaiveDateTime {
        day(m, d).and_hms_opt(h, 0, 0).unwrap()
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

    fn cycle(parent: Option<Uuid>, start: NaiveDate, weeks: u16) -> CycleInput {
        CycleInput {
            parent_id: parent,
            title: "Block".into(),
            start,
            span: Span::Weeks(weeks),
            objectives: "Defence".into(),
            notes: "a note".into(),
        }
    }

    async fn make(t: &SqliteTraining, team: &TeamEntry, level: Level, i: CycleInput) -> Cycle {
        t.create_cycle(&team.id, level, i, vec![]).await.unwrap()
    }

    async fn session(
        t: &SqliteTraining,
        team: &TeamEntry,
        start: NaiveDateTime,
        micro: Option<Uuid>,
    ) -> crate::training::TrainingSession {
        t.create_session(
            &team.id,
            SessionForm {
                start: Some(start),
                microcycle: micro,
                duration: "90".into(),
                actual_duration: "80".into(),
                location: "Gym".into(),
                title: "Serve".into(),
                objectives: "Reception".into(),
                notes: "n".into(),
                final_remarks: "r".into(),
                ..SessionForm::default()
            }
            .validate()
            .unwrap(),
        )
        .await
        .unwrap()
    }

    async fn activity(t: &SqliteTraining, s: &Uuid, name: &str, status: ActivityStatus) {
        t.add_activity(
            s,
            ActivityForm {
                name: name.into(),
                description: "d".into(),
                planned: "20".into(),
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

    fn err(e: AppError) -> TrainingError {
        match e {
            AppError::Training(e) => e,
            e => panic!("not a training error: {e:?}"),
        }
    }

    fn input(shift: u16, copies: u16) -> DuplicateCycleInput {
        DuplicateCycleInput {
            shift_weeks: shift,
            copies,
            ..DuplicateCycleInput::proposed(shift)
        }
    }

    #[test]
    fn the_dialog_proposes_the_next_weeks_with_the_boxes_ticked() {
        assert_eq!(
            DuplicateCycleInput::proposed(2),
            DuplicateCycleInput {
                shift_weeks: 2,
                copies: 1,
                copy_objectives: true,
                copy_activities: true
            }
        );
    }

    #[tokio::test]
    async fn the_copies_move_by_k_times_the_shift_keeping_weekday_and_time() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let meso = make(&t, &team, Level::Mesocycle, cycle(None, day(11, 2), 8)).await;
        let micro = make(
            &t,
            &team,
            Level::Microcycle,
            cycle(Some(meso.id), day(11, 2), 1),
        )
        .await;
        session(&t, &team, at(11, 3, 18), Some(micro.id)).await;
        session(&t, &team, at(11, 5, 19), Some(micro.id)).await;
        let copies = t
            .duplicate_microcycle(&micro.id, input(1, 3))
            .await
            .unwrap();
        assert_eq!(
            copies.iter().map(|c| c.window.start).collect::<Vec<_>>(),
            [day(11, 9), day(11, 16), day(11, 23)]
        );
        assert!(copies.iter().all(|c| c.parent_id == Some(meso.id)));
        let mut starts = vec![];
        for c in &copies {
            for child in t.children(Level::Microcycle, &c.id).await.unwrap() {
                starts.push(t.session(&child.id).await.unwrap().start);
            }
        }
        starts.sort();
        assert_eq!(
            starts,
            [
                at(11, 10, 18),
                at(11, 12, 19),
                at(11, 17, 18),
                at(11, 19, 19),
                at(11, 24, 18),
                at(11, 26, 19)
            ]
        );
    }

    #[tokio::test]
    async fn what_is_copied_and_what_never_is() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let micro = make(&t, &team, Level::Microcycle, cycle(None, day(11, 2), 1)).await;
        let s = session(&t, &team, at(11, 3, 18), Some(micro.id)).await;
        t.set_session_status(&s.id, SessionStatus::Cancelled)
            .await
            .unwrap();
        activity(&t, &s.id, "Warm up", ActivityStatus::Planned).await;
        activity(&t, &s.id, "Done", ActivityStatus::Completed).await;
        let copy = t
            .duplicate_microcycle(&micro.id, input(1, 1))
            .await
            .unwrap()
            .remove(0);
        assert_eq!(
            (
                copy.title.as_str(),
                copy.objectives.as_str(),
                copy.notes.as_str()
            ),
            ("Block", "Defence", "")
        );
        let c = t
            .children(Level::Microcycle, &copy.id)
            .await
            .unwrap()
            .remove(0);
        let c = t.session(&c.id).await.unwrap();
        assert_eq!(
            (
                c.title.as_str(),
                c.location.as_str(),
                c.duration_min,
                c.objectives.as_str(),
                c.notes.as_str(),
                c.final_remarks.as_str(),
                c.actual_duration_min,
                c.status,
                c.series_id
            ),
            (
                "Serve",
                "Gym",
                90,
                "Reception",
                "",
                "",
                None,
                SessionStatus::Planned,
                None
            )
        );
        let names: Vec<_> = t
            .activities(&c.id)
            .await
            .unwrap()
            .into_iter()
            .map(|a| (a.name, a.status, a.actual_min))
            .collect();
        assert_eq!(
            names,
            [("Warm up".to_string(), ActivityStatus::Planned, None)]
        );
    }

    #[tokio::test]
    async fn the_boxes_leave_objectives_and_activities_out() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let micro = make(&t, &team, Level::Microcycle, cycle(None, day(11, 2), 1)).await;
        let s = session(&t, &team, at(11, 3, 18), Some(micro.id)).await;
        activity(&t, &s.id, "Warm up", ActivityStatus::Planned).await;
        let copy = t
            .duplicate_microcycle(
                &micro.id,
                DuplicateCycleInput {
                    copy_objectives: false,
                    copy_activities: false,
                    ..input(1, 1)
                },
            )
            .await
            .unwrap()
            .remove(0);
        let c = t
            .children(Level::Microcycle, &copy.id)
            .await
            .unwrap()
            .remove(0);
        let c = t.session(&c.id).await.unwrap();
        assert_eq!((copy.objectives.as_str(), c.objectives.as_str()), ("", ""));
        assert!(t.activities(&c.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_copy_outside_the_mesocycle_is_free_the_others_stay() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let meso = make(&t, &team, Level::Mesocycle, cycle(None, day(11, 2), 2)).await;
        let micro = make(
            &t,
            &team,
            Level::Microcycle,
            cycle(Some(meso.id), day(11, 2), 1),
        )
        .await;
        let copies = t
            .duplicate_microcycle(&micro.id, input(1, 2))
            .await
            .unwrap();
        assert_eq!(
            copies.iter().map(|c| c.parent_id).collect::<Vec<_>>(),
            [Some(meso.id), None]
        );
    }

    #[tokio::test]
    async fn an_overlapping_microcycle_stops_all_the_copies_and_says_which() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let micro = make(&t, &team, Level::Microcycle, cycle(None, day(11, 2), 1)).await;
        make(&t, &team, Level::Microcycle, cycle(None, day(11, 16), 1)).await;
        session(&t, &team, at(11, 3, 18), Some(micro.id)).await;
        let e = t
            .duplicate_microcycle(&micro.id, input(1, 3))
            .await
            .unwrap_err();
        match err(e) {
            TrainingError::CycleOverlap(w) => {
                assert_eq!(w.len(), 1);
                assert_eq!(w[0].start, day(11, 16));
            }
            e => panic!("{e:?}"),
        }
        assert_eq!(
            t.cycles(&team.id, Level::Microcycle).await.unwrap().len(),
            2
        );
        assert_eq!(
            t.children(Level::Microcycle, &micro.id)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn a_session_in_the_way_stops_all_the_copies_and_says_which() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let micro = make(&t, &team, Level::Microcycle, cycle(None, day(11, 2), 1)).await;
        session(&t, &team, at(11, 3, 18), Some(micro.id)).await;
        // free session, two weeks later, at the same time
        let other = session(&t, &team, at(11, 17, 18), None).await;
        let e = t
            .duplicate_microcycle(&micro.id, input(1, 3))
            .await
            .unwrap_err();
        match err(e) {
            TrainingError::Overlap(slots) => {
                assert_eq!(slots.len(), 1);
                assert_eq!(slots[0].start, other.start);
            }
            e => panic!("{e:?}"),
        }
        assert_eq!(
            t.cycles(&team.id, Level::Microcycle).await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn a_shift_shorter_than_the_microcycle_overlaps_the_original() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let micro = make(&t, &team, Level::Microcycle, cycle(None, day(11, 2), 2)).await;
        let e = t
            .duplicate_microcycle(&micro.id, input(1, 1))
            .await
            .unwrap_err();
        assert!(matches!(err(e), TrainingError::CycleOverlap(_)));
    }

    #[tokio::test]
    async fn shift_and_copies_are_in_range() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let micro = make(&t, &team, Level::Microcycle, cycle(None, day(11, 2), 1)).await;
        for bad in [input(0, 1), input(1, 0), input(1, 53)] {
            let e = t.duplicate_microcycle(&micro.id, bad).await.unwrap_err();
            assert!(matches!(err(e), TrainingError::InvalidWindow));
        }
    }

    #[tokio::test]
    async fn removed_sessions_and_other_teams_are_left_alone() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p).await;
        let micro = make(&t, &team, Level::Microcycle, cycle(None, day(11, 2), 1)).await;
        let gone = session(&t, &team, at(11, 3, 18), Some(micro.id)).await;
        t.remove_session(&gone.id).await.unwrap();
        let copy = t
            .duplicate_microcycle(&micro.id, input(1, 1))
            .await
            .unwrap()
            .remove(0);
        assert!(
            t.children(Level::Microcycle, &copy.id)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
