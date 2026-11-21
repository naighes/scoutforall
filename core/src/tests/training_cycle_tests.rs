//! The cycles of the training (TODO/021 step 9): windows, containment,
//! overlaps, removal with and without children, cycles made from below,
//! the change of a window.

#[cfg(test)]
mod tests {
    use crate::{
        errors::{AppError, TrainingError},
        localization::labels,
        providers::{
            sqlite::training::SqliteTraining,
            team_writer::{TeamInput, TeamWriter},
            training_cycles::TrainingCycles,
            training_reader::TrainingReader,
            training_writer::TrainingWriter,
        },
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, LanguageEnum, TeamClassificationEnum},
            team::TeamEntry,
        },
        tests::Sqlite,
        training::{
            SessionInput,
            cycles::{
                Cycle, CycleForm, CycleFormError, CycleInput, Level, Span, Window,
                proposed_container, proposed_microcycle,
            },
            duplicate::DuplicateInput,
            form::SessionForm,
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

    async fn team(p: &Sqlite, name: &str) -> TeamEntry {
        p.team_writer
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

    fn input(parent: Option<Uuid>, start: NaiveDate, span: Span) -> CycleInput {
        CycleInput {
            parent_id: parent,
            title: String::new(),
            start,
            span,
            objectives: String::new(),
            notes: String::new(),
        }
    }

    /// Mondays 2026-11-02, 11-09 …
    fn weeks(parent: Option<Uuid>, start: NaiveDate, n: u16) -> CycleInput {
        input(parent, start, Span::Weeks(n))
    }

    async fn make(t: &SqliteTraining, team: &TeamEntry, level: Level, i: CycleInput) -> Cycle {
        t.create_cycle(&team.id, level, i, vec![]).await.unwrap()
    }

    fn session(start: NaiveDateTime, microcycle: Option<Uuid>) -> SessionInput {
        SessionForm {
            start: Some(start),
            microcycle,
            ..SessionForm::default()
        }
        .validate()
        .unwrap()
    }

    fn err(e: AppError) -> TrainingError {
        match e {
            AppError::Training(e) => e,
            e => panic!("not a training error: {e:?}"),
        }
    }

    #[test]
    fn a_window_ends_the_day_before_its_end() {
        let w = Window::of(Level::Microcycle, day(11, 23), Span::Weeks(2)).unwrap();
        assert_eq!(
            (w.start, w.end, w.last()),
            (day(11, 23), day(12, 7), day(12, 6))
        );
        let p = Window::of(Level::Program, day(9, 1), Span::Until(day(9, 1))).unwrap();
        assert_eq!(p.end, day(9, 2));
        assert!(Window::of(Level::Program, day(9, 2), Span::Until(day(9, 1))).is_none());
        assert!(Window::of(Level::Program, day(9, 2), Span::Weeks(2)).is_none());
        assert!(Window::of(Level::Mesocycle, day(9, 2), Span::Weeks(0)).is_none());
        assert!(Window::of(Level::Mesocycle, day(9, 2), Span::Until(day(9, 3))).is_none());
    }

    #[test]
    fn windows_that_touch_do_not_overlap() {
        let a = Window::of(Level::Microcycle, day(11, 2), Span::Weeks(1)).unwrap();
        let b = Window::of(Level::Microcycle, day(11, 9), Span::Weeks(1)).unwrap();
        assert!(!a.overlaps(&b) && !b.overlaps(&a));
        assert!(a.overlaps(&a));
        assert_eq!(a.text(), "2026-11-02 – 2026-11-08");
        assert_eq!(Window::day(day(11, 2)).text(), "2026-11-02");
    }

    #[test]
    fn the_form_reads_the_weeks_or_the_last_day_of_its_level() {
        let form = CycleForm {
            start: Some(day(11, 2)),
            weeks: " 3 ".into(),
            end: Some(day(12, 1)),
            title: " T ".into(),
            objectives: " o ".into(),
            ..CycleForm::default()
        };
        let micro = form.clone().validate(Level::Microcycle).unwrap();
        assert_eq!(
            (micro.span, micro.title.as_str(), micro.objectives.as_str()),
            (Span::Weeks(3), "T", "o")
        );
        let program = form.clone().validate(Level::Program).unwrap();
        assert_eq!(program.span, Span::Until(day(12, 1)));
        assert_eq!(program.objectives, "");
        for weeks in ["", "0", "x", "-1", "65536"] {
            let bad = CycleForm {
                weeks: weeks.into(),
                ..form.clone()
            };
            assert_eq!(
                bad.validate(Level::Mesocycle).unwrap_err(),
                CycleFormError::InvalidWeeks,
                "{weeks}"
            );
        }
        let before = CycleForm {
            end: Some(day(11, 1)),
            ..form.clone()
        };
        assert_eq!(
            before.validate(Level::Program).unwrap_err(),
            CycleFormError::InvalidEnd
        );
        let none = CycleForm {
            end: None,
            ..form.clone()
        };
        assert_eq!(
            none.validate(Level::Program).unwrap_err(),
            CycleFormError::InvalidEnd
        );
        let late = CycleForm {
            start: None,
            ..form
        };
        assert_eq!(
            late.validate(Level::Microcycle).unwrap_err(),
            CycleFormError::InvalidStart
        );
    }

    #[test]
    fn a_microcycle_is_proposed_from_the_monday_of_the_first_session() {
        // Wed 2026-11-04 and Mon 2026-11-16: from Monday 11-02, three weeks
        assert_eq!(
            proposed_microcycle(&[at(11, 16, 18), at(11, 4, 18)]),
            Some((day(11, 2), Span::Weeks(3)))
        );
        assert_eq!(
            proposed_microcycle(&[at(11, 8, 18)]),
            Some((day(11, 2), Span::Weeks(1)))
        );
        assert_eq!(proposed_microcycle(&[]), None);
    }

    #[test]
    fn a_container_is_proposed_from_the_first_start_to_the_last_end() {
        let a = Window::of(Level::Microcycle, day(11, 2), Span::Weeks(1)).unwrap();
        let b = Window::of(Level::Microcycle, day(11, 16), Span::Weeks(2)).unwrap();
        assert_eq!(
            proposed_container(Level::Mesocycle, &[b, a]),
            Some((day(11, 2), Span::Weeks(4)))
        );
        assert_eq!(
            proposed_container(Level::Program, &[a, b]),
            Some((day(11, 2), Span::Until(day(11, 29))))
        );
        assert_eq!(proposed_container(Level::Program, &[]), None);
    }

    #[tokio::test]
    async fn a_cycle_is_saved_and_read_back_at_every_level() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let program = make(
            &t,
            &team,
            Level::Program,
            CycleInput {
                title: "Season".into(),
                notes: "n".into(),
                ..input(None, day(9, 1), Span::Until(day(12, 20)))
            },
        )
        .await;
        let meso = make(
            &t,
            &team,
            Level::Mesocycle,
            weeks(Some(program.id), day(11, 2), 4),
        )
        .await;
        let micro = make(
            &t,
            &team,
            Level::Microcycle,
            CycleInput {
                objectives: "serve".into(),
                ..weeks(Some(meso.id), day(11, 2), 2)
            },
        )
        .await;
        assert_eq!(t.cycle(Level::Program, &program.id).await.unwrap(), program);
        assert_eq!(program.window.last(), day(12, 20));
        assert_eq!(
            (meso.parent_id, meso.window.last()),
            (Some(program.id), day(11, 29))
        );
        assert_eq!(
            (micro.parent_id, micro.objectives.as_str(), micro.window.end),
            (Some(meso.id), "serve", day(11, 16))
        );
        assert_eq!(
            t.cycles(&team.id, Level::Microcycle).await.unwrap(),
            vec![micro]
        );
    }

    #[tokio::test]
    async fn a_program_has_no_parent_even_if_the_input_names_one() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let program = make(
            &t,
            &team,
            Level::Program,
            input(Some(Uuid::new_v4()), day(9, 1), Span::Until(day(9, 30))),
        )
        .await;
        assert_eq!(program.parent_id, None);
    }

    #[tokio::test]
    async fn the_span_must_be_the_one_of_the_level() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let e = t
            .create_cycle(
                &team.id,
                Level::Microcycle,
                input(None, day(11, 2), Span::Until(day(11, 9))),
                vec![],
            )
            .await
            .unwrap_err();
        assert!(matches!(err(e), TrainingError::InvalidWindow));
        assert!(
            t.cycles(&team.id, Level::Microcycle)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn a_child_stays_inside_its_parent_always() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let program = make(
            &t,
            &team,
            Level::Program,
            input(None, day(11, 2), Span::Until(day(11, 29))),
        )
        .await;
        // exactly the window of the parent: fine
        let meso = make(
            &t,
            &team,
            Level::Mesocycle,
            weeks(Some(program.id), day(11, 2), 4),
        )
        .await;
        for (start, n) in [(day(10, 26), 4), (day(11, 9), 4)] {
            let e = t
                .create_cycle(
                    &team.id,
                    Level::Mesocycle,
                    weeks(Some(program.id), start, n),
                    vec![],
                )
                .await
                .unwrap_err();
            assert!(matches!(err(e), TrainingError::OutsideParent), "{start}");
        }
        assert_eq!(
            t.cycles(&team.id, Level::Mesocycle).await.unwrap(),
            vec![meso]
        );
    }

    #[tokio::test]
    async fn a_parent_of_another_team_is_refused() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let (a, b) = (team(&p, "A").await, team(&p, "B").await);
        let program = make(
            &t,
            &a,
            Level::Program,
            input(None, day(11, 2), Span::Until(day(11, 29))),
        )
        .await;
        let e = t
            .create_cycle(
                &b.id,
                Level::Mesocycle,
                weeks(Some(program.id), day(11, 2), 1),
                vec![],
            )
            .await
            .unwrap_err();
        assert!(matches!(e, AppError::IO(_)), "{e:?}");
    }

    #[tokio::test]
    async fn cycles_of_a_level_do_not_overlap_but_touching_is_fine() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let (a, b) = (team(&p, "A").await, team(&p, "B").await);
        let first = make(&t, &a, Level::Microcycle, weeks(None, day(11, 2), 2)).await;
        // starts the day the first ends (excluded)
        make(&t, &a, Level::Microcycle, weeks(None, day(11, 16), 1)).await;
        // another team overlaps freely
        make(&t, &b, Level::Microcycle, weeks(None, day(11, 2), 2)).await;
        let e = t
            .create_cycle(&a.id, Level::Microcycle, weeks(None, day(11, 9), 1), vec![])
            .await
            .unwrap_err();
        assert!(matches!(
            &e,
            AppError::Training(TrainingError::CycleOverlap(w)) if w == &vec![first.window]
        ));
        assert_eq!(
            e.message(labels(LanguageEnum::En)),
            "overlaps the cycle of 2026-11-02 – 2026-11-15"
        );
        assert_eq!(t.cycles(&a.id, Level::Microcycle).await.unwrap().len(), 2);
        // a mesocycle of the same days is another level: no clash
        make(&t, &a, Level::Mesocycle, weeks(None, day(11, 2), 2)).await;
    }

    #[tokio::test]
    async fn a_cycle_can_be_saved_over_its_own_window() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let c = make(&t, &team, Level::Microcycle, weeks(None, day(11, 2), 2)).await;
        let saved = t
            .update_cycle(
                Level::Microcycle,
                &c.id,
                CycleInput {
                    title: "Renamed".into(),
                    ..weeks(None, day(11, 3), 2)
                },
                false,
            )
            .await
            .unwrap();
        assert_eq!(
            (saved.title.as_str(), saved.window.start),
            ("Renamed", day(11, 3))
        );
    }

    #[tokio::test]
    async fn an_edit_cannot_overlap_a_sibling_or_leave_its_parent() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let meso = make(&t, &team, Level::Mesocycle, weeks(None, day(11, 2), 4)).await;
        let one = make(
            &t,
            &team,
            Level::Microcycle,
            weeks(Some(meso.id), day(11, 2), 1),
        )
        .await;
        make(
            &t,
            &team,
            Level::Microcycle,
            weeks(Some(meso.id), day(11, 9), 1),
        )
        .await;
        let e = t
            .update_cycle(
                Level::Microcycle,
                &one.id,
                weeks(Some(meso.id), day(11, 2), 2),
                false,
            )
            .await
            .unwrap_err();
        assert!(matches!(err(e), TrainingError::CycleOverlap(_)));
        let e = t
            .update_cycle(
                Level::Microcycle,
                &one.id,
                weeks(Some(meso.id), day(10, 26), 1),
                false,
            )
            .await
            .unwrap_err();
        assert!(matches!(err(e), TrainingError::OutsideParent));
        assert_eq!(t.cycle(Level::Microcycle, &one.id).await.unwrap(), one);
    }

    #[tokio::test]
    async fn a_session_goes_in_a_microcycle_only_on_its_days() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let micro = make(&t, &team, Level::Microcycle, weeks(None, day(11, 2), 1)).await;
        // the last day counts even late at night; the duration may pass midnight
        let last = t
            .create_session(&team.id, session(at(11, 8, 23), Some(micro.id)))
            .await
            .unwrap();
        assert_eq!(last.microcycle_id, Some(micro.id));
        for start in [at(11, 9, 8), at(11, 1, 23)] {
            let e = t
                .create_session(&team.id, session(start, Some(micro.id)))
                .await
                .unwrap_err();
            assert!(matches!(err(e), TrainingError::OutsideParent), "{start}");
        }
        assert_eq!(t.sessions(&team.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn moving_a_session_out_stops_unless_it_leaves_the_microcycle() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let micro = make(&t, &team, Level::Microcycle, weeks(None, day(11, 2), 1)).await;
        let s = t
            .create_session(&team.id, session(at(11, 3, 18), Some(micro.id)))
            .await
            .unwrap();
        let e = t
            .update_session(&s.id, session(at(11, 10, 18), Some(micro.id)))
            .await
            .unwrap_err();
        assert!(matches!(err(e), TrainingError::OutsideParent));
        assert_eq!(t.session(&s.id).await.unwrap().start, at(11, 3, 18));
        // "take it out of the microcycle and save"
        let moved = t
            .update_session(&s.id, session(at(11, 10, 18), None))
            .await
            .unwrap();
        assert_eq!((moved.start, moved.microcycle_id), (at(11, 10, 18), None));
    }

    #[tokio::test]
    async fn a_copy_of_a_session_goes_in_the_microcycle_only_on_its_days() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let micro = make(&t, &team, Level::Microcycle, weeks(None, day(11, 2), 2)).await;
        let s = t
            .create_session(&team.id, session(at(11, 3, 18), Some(micro.id)))
            .await
            .unwrap();
        let copy = t
            .duplicate_session(&s.id, DuplicateInput::proposed(s.start, s.microcycle_id))
            .await
            .unwrap();
        assert_eq!(
            (copy.start, copy.microcycle_id),
            (at(11, 10, 18), Some(micro.id))
        );
        // a week more is outside: the dialog proposes it free instead
        let e = t
            .duplicate_session(
                &s.id,
                DuplicateInput::proposed(copy.start, copy.microcycle_id),
            )
            .await
            .unwrap_err();
        assert!(matches!(err(e), TrainingError::OutsideParent));
        let free = t
            .duplicate_session(&s.id, DuplicateInput::proposed(copy.start, None))
            .await
            .unwrap();
        assert_eq!(free.microcycle_id, None);
    }

    #[tokio::test]
    async fn a_session_of_another_teams_microcycle_is_refused() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let (a, b) = (team(&p, "A").await, team(&p, "B").await);
        let micro = make(&t, &a, Level::Microcycle, weeks(None, day(11, 2), 1)).await;
        let e = t
            .create_session(&b.id, session(at(11, 3, 18), Some(micro.id)))
            .await
            .unwrap_err();
        assert!(matches!(e, AppError::IO(_)), "{e:?}");
    }

    #[tokio::test]
    async fn shortening_a_window_names_the_children_it_leaves_out() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let micro = make(&t, &team, Level::Microcycle, weeks(None, day(11, 2), 2)).await;
        let inside = t
            .create_session(&team.id, session(at(11, 3, 18), Some(micro.id)))
            .await
            .unwrap();
        let outside = t
            .create_session(&team.id, session(at(11, 11, 18), Some(micro.id)))
            .await
            .unwrap();
        let e = t
            .update_cycle(
                Level::Microcycle,
                &micro.id,
                weeks(None, day(11, 2), 1),
                false,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            &e,
            AppError::Training(TrainingError::ChildrenOutside(c))
                if c.len() == 1 && c[0].id == outside.id && c[0].window == Window::day(day(11, 11))
        ));
        assert_eq!(
            e.message(labels(LanguageEnum::It)),
            "lascia fuori: 2026-11-11"
        );
        // nothing changed
        assert_eq!(t.cycle(Level::Microcycle, &micro.id).await.unwrap(), micro);
        assert_eq!(
            t.session(&outside.id).await.unwrap().microcycle_id,
            Some(micro.id)
        );
        // "detach them and save"
        let saved = t
            .update_cycle(
                Level::Microcycle,
                &micro.id,
                weeks(None, day(11, 2), 1),
                true,
            )
            .await
            .unwrap();
        assert_eq!(saved.window.end, day(11, 9));
        assert_eq!(t.session(&outside.id).await.unwrap().microcycle_id, None);
        assert_eq!(
            t.session(&inside.id).await.unwrap().microcycle_id,
            Some(micro.id)
        );
    }

    #[tokio::test]
    async fn the_levels_above_check_the_levels_below_too() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let meso = make(&t, &team, Level::Mesocycle, weeks(None, day(11, 2), 4)).await;
        let m1 = make(
            &t,
            &team,
            Level::Microcycle,
            weeks(Some(meso.id), day(11, 2), 2),
        )
        .await;
        let m2 = make(
            &t,
            &team,
            Level::Microcycle,
            weeks(Some(meso.id), day(11, 16), 2),
        )
        .await;
        let e = t
            .update_cycle(
                Level::Mesocycle,
                &meso.id,
                weeks(None, day(11, 2), 2),
                false,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err(e),
            TrainingError::ChildrenOutside(c) if c.len() == 1 && c[0].id == m2.id
        ));
        t.update_cycle(Level::Mesocycle, &meso.id, weeks(None, day(11, 2), 2), true)
            .await
            .unwrap();
        assert_eq!(
            t.cycle(Level::Microcycle, &m2.id).await.unwrap().parent_id,
            None
        );
        assert_eq!(
            t.cycle(Level::Microcycle, &m1.id).await.unwrap().parent_id,
            Some(meso.id)
        );
    }

    #[tokio::test]
    async fn moving_a_cycle_to_a_parent_checks_the_window() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let meso = make(&t, &team, Level::Mesocycle, weeks(None, day(11, 2), 2)).await;
        let micro = make(&t, &team, Level::Microcycle, weeks(None, day(11, 23), 1)).await;
        let e = t
            .update_cycle(
                Level::Microcycle,
                &micro.id,
                weeks(Some(meso.id), day(11, 23), 1),
                false,
            )
            .await
            .unwrap_err();
        assert!(matches!(err(e), TrainingError::OutsideParent));
        let ok = t
            .update_cycle(
                Level::Microcycle,
                &micro.id,
                weeks(Some(meso.id), day(11, 9), 1),
                false,
            )
            .await
            .unwrap();
        assert_eq!(ok.parent_id, Some(meso.id));
        let free = t
            .update_cycle(
                Level::Microcycle,
                &micro.id,
                weeks(None, day(11, 9), 1),
                false,
            )
            .await
            .unwrap();
        assert_eq!(free.parent_id, None);
    }

    #[tokio::test]
    async fn removing_a_cycle_can_leave_its_children_free() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let meso = make(&t, &team, Level::Mesocycle, weeks(None, day(11, 2), 4)).await;
        let micro = make(
            &t,
            &team,
            Level::Microcycle,
            weeks(Some(meso.id), day(11, 2), 1),
        )
        .await;
        let s = t
            .create_session(&team.id, session(at(11, 3, 18), Some(micro.id)))
            .await
            .unwrap();
        t.remove_cycle(Level::Mesocycle, &meso.id, false)
            .await
            .unwrap();
        assert!(
            t.cycles(&team.id, Level::Mesocycle)
                .await
                .unwrap()
                .is_empty()
        );
        let micro = t.cycle(Level::Microcycle, &micro.id).await.unwrap();
        assert_eq!(micro.parent_id, None);
        t.remove_cycle(Level::Microcycle, &micro.id, false)
            .await
            .unwrap();
        assert_eq!(t.session(&s.id).await.unwrap().microcycle_id, None);
        assert!(t.cycle(Level::Mesocycle, &meso.id).await.is_err());
    }

    #[tokio::test]
    async fn removing_a_cycle_with_its_children_removes_every_level_below() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let program = make(
            &t,
            &team,
            Level::Program,
            input(None, day(11, 2), Span::Until(day(11, 29))),
        )
        .await;
        let meso = make(
            &t,
            &team,
            Level::Mesocycle,
            weeks(Some(program.id), day(11, 2), 4),
        )
        .await;
        let micro = make(
            &t,
            &team,
            Level::Microcycle,
            weeks(Some(meso.id), day(11, 2), 1),
        )
        .await;
        let s = t
            .create_session(&team.id, session(at(11, 3, 18), Some(micro.id)))
            .await
            .unwrap();
        let other = t
            .create_session(&team.id, session(at(11, 20, 18), None))
            .await
            .unwrap();
        t.remove_cycle(Level::Program, &program.id, true)
            .await
            .unwrap();
        assert!(
            t.cycles(&team.id, Level::Mesocycle)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            t.cycles(&team.id, Level::Microcycle)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(t.session(&s.id).await.is_err());
        assert!(t.session(&other.id).await.is_ok());
        assert!(
            t.remove_cycle(Level::Program, &program.id, true)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_removed_cycle_no_longer_blocks_its_window() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let c = make(&t, &team, Level::Microcycle, weeks(None, day(11, 2), 2)).await;
        t.remove_cycle(Level::Microcycle, &c.id, true)
            .await
            .unwrap();
        make(&t, &team, Level::Microcycle, weeks(None, day(11, 2), 2)).await;
    }

    #[tokio::test]
    async fn a_microcycle_is_made_from_free_sessions() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let mut ids = vec![];
        for start in [at(11, 4, 18), at(11, 16, 18)] {
            ids.push(
                t.create_session(&team.id, session(start, None))
                    .await
                    .unwrap(),
            );
        }
        let (start, span) =
            proposed_microcycle(&ids.iter().map(|s| s.start).collect::<Vec<_>>()).unwrap();
        let micro = t
            .create_cycle(
                &team.id,
                Level::Microcycle,
                input(None, start, span),
                ids.iter().map(|s| s.id).collect(),
            )
            .await
            .unwrap();
        assert_eq!(micro.window.start, day(11, 2));
        for s in &ids {
            assert_eq!(
                t.session(&s.id).await.unwrap().microcycle_id,
                Some(micro.id)
            );
        }
        assert_eq!(
            t.children(Level::Microcycle, &micro.id)
                .await
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn from_below_a_window_that_misses_a_child_writes_nothing() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let s = t
            .create_session(&team.id, session(at(11, 20, 18), None))
            .await
            .unwrap();
        let e = t
            .create_cycle(
                &team.id,
                Level::Microcycle,
                weeks(None, day(11, 2), 1),
                vec![s.id],
            )
            .await
            .unwrap_err();
        assert!(matches!(err(e), TrainingError::OutsideParent));
        assert!(
            t.cycles(&team.id, Level::Microcycle)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(t.session(&s.id).await.unwrap().microcycle_id, None);
    }

    #[tokio::test]
    async fn only_free_children_can_be_taken() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let first = make(&t, &team, Level::Microcycle, weeks(None, day(11, 2), 1)).await;
        let s = t
            .create_session(&team.id, session(at(11, 3, 18), Some(first.id)))
            .await
            .unwrap();
        let e = t
            .create_cycle(
                &team.id,
                Level::Microcycle,
                weeks(None, day(11, 9), 1),
                vec![s.id],
            )
            .await
            .unwrap_err();
        assert!(matches!(e, AppError::IO(_)), "{e:?}");
        assert_eq!(
            t.cycles(&team.id, Level::Microcycle).await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn microcycles_make_a_mesocycle_and_mesocycles_a_program() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let m1 = make(&t, &team, Level::Microcycle, weeks(None, day(11, 2), 1)).await;
        let m2 = make(&t, &team, Level::Microcycle, weeks(None, day(11, 16), 2)).await;
        let (start, span) = proposed_container(Level::Mesocycle, &[m1.window, m2.window]).unwrap();
        let meso = t
            .create_cycle(
                &team.id,
                Level::Mesocycle,
                input(None, start, span),
                vec![m1.id, m2.id],
            )
            .await
            .unwrap();
        assert_eq!(meso.span, Span::Weeks(4));
        assert_eq!(
            t.cycle(Level::Microcycle, &m2.id).await.unwrap().parent_id,
            Some(meso.id)
        );
        let (start, span) = proposed_container(Level::Program, &[meso.window]).unwrap();
        let program = t
            .create_cycle(
                &team.id,
                Level::Program,
                input(None, start, span),
                vec![meso.id],
            )
            .await
            .unwrap();
        assert_eq!(program.window, meso.window);
        assert_eq!(
            t.cycle(Level::Mesocycle, &meso.id).await.unwrap().parent_id,
            Some(program.id)
        );
    }

    #[tokio::test]
    async fn add_offers_the_free_children_that_fit() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let (team, other) = (team(&p, "A").await, team(&p, "B").await);
        let meso = make(&t, &team, Level::Mesocycle, weeks(None, day(11, 2), 2)).await;
        let fits = make(&t, &team, Level::Microcycle, weeks(None, day(11, 9), 1)).await;
        let held = make(
            &t,
            &team,
            Level::Microcycle,
            weeks(Some(meso.id), day(11, 2), 1),
        )
        .await;
        make(&t, &team, Level::Microcycle, weeks(None, day(11, 16), 1)).await; // outside
        make(&t, &other, Level::Microcycle, weeks(None, day(11, 9), 1)).await; // other team
        let free = t.free_children(Level::Mesocycle, &meso.id).await.unwrap();
        assert_eq!(free.iter().map(|c| c.id).collect::<Vec<_>>(), vec![fits.id]);
        t.add_children(Level::Mesocycle, &meso.id, vec![fits.id])
            .await
            .unwrap();
        let held_ids: Vec<Uuid> = t
            .children(Level::Mesocycle, &meso.id)
            .await
            .unwrap()
            .iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(held_ids, vec![held.id, fits.id]);
        assert!(
            t.free_children(Level::Mesocycle, &meso.id)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn add_offers_the_free_sessions_of_the_microcycle_days() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let micro = make(&t, &team, Level::Microcycle, weeks(None, day(11, 2), 1)).await;
        let inside = t
            .create_session(&team.id, session(at(11, 4, 18), None))
            .await
            .unwrap();
        t.create_session(&team.id, session(at(11, 12, 18), None))
            .await
            .unwrap();
        let free = t.free_children(Level::Microcycle, &micro.id).await.unwrap();
        assert_eq!(
            free.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![inside.id]
        );
        let outside = t
            .create_session(&team.id, session(at(11, 20, 18), None))
            .await
            .unwrap();
        let e = t
            .add_children(Level::Microcycle, &micro.id, vec![inside.id, outside.id])
            .await
            .unwrap_err();
        assert!(matches!(err(e), TrainingError::OutsideParent));
        // all or nothing
        assert_eq!(t.session(&inside.id).await.unwrap().microcycle_id, None);
        t.add_children(Level::Microcycle, &micro.id, vec![inside.id])
            .await
            .unwrap();
        assert_eq!(
            t.session(&inside.id).await.unwrap().microcycle_id,
            Some(micro.id)
        );
    }

    #[tokio::test]
    async fn the_form_proposes_the_microcycle_that_holds_the_date() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        let micro = make(&t, &team, Level::Microcycle, weeks(None, day(11, 2), 1)).await;
        let held = t
            .containing(&team.id, Level::Microcycle, day(11, 8))
            .await
            .unwrap();
        assert_eq!(held, vec![micro]);
        assert!(
            t.containing(&team.id, Level::Microcycle, day(11, 9))
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn a_team_without_training_has_no_cycles() {
        let p = Sqlite::new();
        let t = SqliteTraining::new(p.db.clone());
        let team = team(&p, "A").await;
        for level in Level::ALL {
            assert!(t.cycles(&team.id, level).await.unwrap().is_empty());
        }
        let e = t.cycle(Level::Program, &Uuid::new_v4()).await.unwrap_err();
        assert!(matches!(e, AppError::IO(_)), "{e:?}");
    }
}
