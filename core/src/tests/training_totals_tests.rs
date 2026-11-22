//! The totals of the cycles and the attendance per player (TODO/021 step
//! 12): sessions by status, planned against actual hours, the base of the
//! percentage, guests and unavailable players left out.

#[cfg(test)]
mod tests {
    use crate::{
        providers::{
            sqlite::{Database, teams::SqliteTeamWriter, training::SqliteTraining},
            team_writer::{PlayerDetails, PlayerInput, TeamInput, TeamWriter},
            training_cycles::TrainingCycles,
            training_reader::TrainingReader,
            training_totals::TrainingTotals,
            training_writer::TrainingWriter,
        },
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, RoleEnum, TeamClassificationEnum},
            team::TeamEntry,
        },
        training::{
            AttendanceInput, AttendanceStatus, SessionStatus,
            cycles::{CycleInput, Level, Span, Window},
            form::SessionForm,
            totals::{Scope, Totals},
        },
    };
    use chrono::NaiveDate;
    use uuid::Uuid;

    struct P {
        teams: SqliteTeamWriter,
        training: SqliteTraining,
    }

    fn providers() -> P {
        let db = Database::new(crate::db::open_in_memory().unwrap());
        P {
            teams: SqliteTeamWriter::new(db.clone()),
            training: SqliteTraining::new(db),
        }
    }

    fn day(m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, d).unwrap()
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

    async fn player(p: &P, team: &mut TeamEntry, name: &str, number: u8) -> Uuid {
        p.teams
            .save_player(
                PlayerInput::New {
                    name: name.into(),
                    role: RoleEnum::Setter,
                    number,
                    details: PlayerDetails::default(),
                },
                team,
            )
            .await
            .unwrap()
            .id
    }

    /// A session on the day, 90 minutes (or `minutes`), in the status given.
    async fn session(
        p: &P,
        team: &TeamEntry,
        date: NaiveDate,
        minutes: u16,
        actual: &str,
        status: SessionStatus,
        microcycle: Option<Uuid>,
    ) -> Uuid {
        let input = SessionForm {
            start: date.and_hms_opt(18, 0, 0),
            duration: minutes.to_string(),
            actual_duration: actual.into(),
            microcycle,
            status,
            ..SessionForm::default()
        }
        .validate()
        .unwrap();
        p.training.create_session(&team.id, input).await.unwrap().id
    }

    async fn completed(p: &P, team: &TeamEntry, date: NaiveDate) -> Uuid {
        session(p, team, date, 90, "", SessionStatus::Completed, None).await
    }

    fn line(person: Uuid, status: AttendanceStatus, guest: bool) -> AttendanceInput {
        AttendanceInput {
            person_id: person,
            number: None,
            status,
            note: String::new(),
            guest,
            makeup: false,
        }
    }

    /// The lines of the players with the number the roster gives them.
    async fn mark(p: &P, session: &Uuid, lines: Vec<(Uuid, AttendanceStatus)>) {
        let roster = p.training.attendance(session).await.unwrap().rows;
        p.training
            .save_attendance(
                session,
                lines
                    .into_iter()
                    .map(|(id, status)| AttendanceInput {
                        number: roster
                            .iter()
                            .find(|r| r.person_id == id)
                            .and_then(|r| r.number),
                        ..line(id, status, false)
                    })
                    .collect(),
            )
            .await
            .unwrap();
    }

    fn cycle(parent: Option<Uuid>, start: NaiveDate, span: Span) -> CycleInput {
        CycleInput {
            parent_id: parent,
            title: String::new(),
            start,
            span,
            objectives: String::new(),
            notes: String::new(),
        }
    }

    use AttendanceStatus::{Absent, Late, Present, Unavailable};

    #[tokio::test]
    async fn no_sessions_give_zero_everywhere() {
        let p = providers();
        let t = team(&p, "A").await;
        assert_eq!(
            p.training.totals(&t.id, Scope::All).await.unwrap(),
            Totals::default()
        );
        let report = p
            .training
            .attendance_report(&t.id, Scope::All)
            .await
            .unwrap();
        assert_eq!((report.sessions, report.players.len()), (0, 0));
    }

    #[tokio::test]
    async fn sessions_are_counted_by_status_and_minutes_by_what_is_counted() {
        let p = providers();
        let t = team(&p, "A").await;
        // planned 90, completed 60 planned / 75 actual, completed 60 with no actual
        // (counts the planned one), cancelled 120 (counts nowhere)
        session(&p, &t, day(11, 2), 90, "", SessionStatus::Planned, None).await;
        session(&p, &t, day(11, 3), 60, "75", SessionStatus::Completed, None).await;
        session(&p, &t, day(11, 4), 60, "", SessionStatus::Completed, None).await;
        session(&p, &t, day(11, 5), 120, "", SessionStatus::Cancelled, None).await;
        let totals = p.training.totals(&t.id, Scope::All).await.unwrap();
        assert_eq!(
            totals,
            Totals {
                planned: 1,
                completed: 2,
                cancelled: 1,
                planned_minutes: 90 + 60 + 60,
                actual_minutes: 75 + 60,
            }
        );
    }

    #[tokio::test]
    async fn a_change_after_the_completion_changes_the_totals() {
        let p = providers();
        let t = team(&p, "A").await;
        let s = completed(&p, &t, day(11, 2)).await;
        assert_eq!(
            p.training
                .totals(&t.id, Scope::All)
                .await
                .unwrap()
                .actual_minutes,
            90
        );
        p.training
            .set_session_status(&s, SessionStatus::Cancelled)
            .await
            .unwrap();
        let totals = p.training.totals(&t.id, Scope::All).await.unwrap();
        assert_eq!((totals.completed, totals.cancelled), (0, 1));
        assert_eq!((totals.planned_minutes, totals.actual_minutes), (0, 0));
    }

    #[tokio::test]
    async fn a_removed_session_and_another_team_count_for_nobody() {
        let p = providers();
        let t = team(&p, "A").await;
        let other = team(&p, "B").await;
        let s = completed(&p, &t, day(11, 2)).await;
        completed(&p, &other, day(11, 2)).await;
        p.training.remove_session(&s).await.unwrap();
        assert_eq!(
            p.training.totals(&t.id, Scope::All).await.unwrap(),
            Totals::default()
        );
    }

    #[tokio::test]
    async fn the_scope_is_a_cycle_down_to_its_sessions_or_a_range_of_dates() {
        let p = providers();
        let t = team(&p, "A").await;
        let program = p
            .training
            .create_cycle(
                &t.id,
                Level::Program,
                cycle(None, day(11, 2), Span::Until(day(12, 27))),
                vec![],
            )
            .await
            .unwrap();
        let meso = p
            .training
            .create_cycle(
                &t.id,
                Level::Mesocycle,
                cycle(Some(program.id), day(11, 2), Span::Weeks(4)),
                vec![],
            )
            .await
            .unwrap();
        let micro_a = p
            .training
            .create_cycle(
                &t.id,
                Level::Microcycle,
                cycle(Some(meso.id), day(11, 2), Span::Weeks(1)),
                vec![],
            )
            .await
            .unwrap();
        let micro_b = p
            .training
            .create_cycle(
                &t.id,
                Level::Microcycle,
                cycle(Some(meso.id), day(11, 9), Span::Weeks(1)),
                vec![],
            )
            .await
            .unwrap();
        // a microcycle in the program but in no mesocycle is not in the meso
        let lone = p
            .training
            .create_cycle(
                &t.id,
                Level::Microcycle,
                cycle(None, day(12, 14), Span::Weeks(1)),
                vec![],
            )
            .await
            .unwrap();
        let on = |d, m| session(&p, &t, d, 60, "", SessionStatus::Planned, m);
        on(day(11, 3), Some(micro_a.id)).await;
        on(day(11, 10), Some(micro_b.id)).await;
        on(day(11, 11), Some(micro_b.id)).await;
        on(day(12, 15), Some(lone.id)).await;
        on(day(11, 20), None).await;
        let (training, team_id) = (&p.training, t.id);
        let count = |scope| async move { training.totals(&team_id, scope).await.unwrap().planned };
        assert_eq!(count(Scope::Cycle(Level::Microcycle, micro_a.id)).await, 1);
        assert_eq!(count(Scope::Cycle(Level::Microcycle, micro_b.id)).await, 2);
        assert_eq!(count(Scope::Cycle(Level::Mesocycle, meso.id)).await, 3);
        assert_eq!(count(Scope::Cycle(Level::Program, program.id)).await, 3);
        assert_eq!(count(Scope::All).await, 5);
        // dates: first day included, end excluded
        let w = |a, b| Scope::Dates(Window { start: a, end: b });
        assert_eq!(count(w(day(11, 10), day(11, 12))).await, 2);
        assert_eq!(count(w(day(11, 10), day(11, 11))).await, 1);
        assert_eq!(count(w(day(11, 21), day(11, 30))).await, 0);
    }

    #[tokio::test]
    async fn a_cycle_of_another_team_gives_nothing() {
        let p = providers();
        let t = team(&p, "A").await;
        let other = team(&p, "B").await;
        let micro = p
            .training
            .create_cycle(
                &t.id,
                Level::Microcycle,
                cycle(None, day(11, 2), Span::Weeks(1)),
                vec![],
            )
            .await
            .unwrap();
        session(
            &p,
            &t,
            day(11, 3),
            60,
            "",
            SessionStatus::Planned,
            Some(micro.id),
        )
        .await;
        let totals = p
            .training
            .totals(&other.id, Scope::Cycle(Level::Microcycle, micro.id))
            .await
            .unwrap();
        assert_eq!(totals, Totals::default());
    }

    #[tokio::test]
    async fn late_is_a_presence_unavailable_is_out_of_the_base() {
        let p = providers();
        let mut t = team(&p, "A").await;
        let ann = player(&p, &mut t, "Ann", 1).await;
        let bea = player(&p, &mut t, "Bea", 2).await;
        for (d, a, b) in [
            (2, Present, Late),
            (3, Late, Absent),
            (4, Absent, Unavailable),
            (5, Present, Unavailable),
        ] {
            let s = completed(&p, &t, day(11, d)).await;
            mark(&p, &s, vec![(ann, a), (bea, b)]).await;
        }
        let report = p
            .training
            .attendance_report(&t.id, Scope::All)
            .await
            .unwrap();
        assert_eq!(report.sessions, 4);
        let by = |id| report.players.iter().find(|r| r.person_id == id).unwrap();
        let a = by(ann);
        assert_eq!(
            (a.present, a.late, a.absent, a.unavailable, a.base()),
            (3, 1, 1, 0, 4)
        );
        assert_eq!(a.percent(), Some(75));
        let b = by(bea);
        assert_eq!(
            (b.present, b.late, b.absent, b.unavailable, b.base()),
            (1, 1, 1, 2, 2)
        );
        assert_eq!(b.percent(), Some(50));
        assert_eq!(b.number, Some(2));
    }

    #[tokio::test]
    async fn only_completed_sessions_with_attendance_taken_count() {
        let p = providers();
        let mut t = team(&p, "A").await;
        let ann = player(&p, &mut t, "Ann", 1).await;
        let done = completed(&p, &t, day(11, 2)).await;
        mark(&p, &done, vec![(ann, Present)]).await;
        // not taken: the roster is not written
        completed(&p, &t, day(11, 3)).await;
        // taken but planned, and taken but cancelled
        for (d, status) in [(4, SessionStatus::Planned), (5, SessionStatus::Cancelled)] {
            let s = session(&p, &t, day(11, d), 90, "", status, None).await;
            mark(&p, &s, vec![(ann, Absent)]).await;
        }
        let report = p
            .training
            .attendance_report(&t.id, Scope::All)
            .await
            .unwrap();
        assert_eq!(report.sessions, 1);
        assert_eq!(report.players.len(), 1);
        assert_eq!(report.players[0].base(), 1);
    }

    #[tokio::test]
    async fn guests_enter_no_percentage_and_no_row() {
        let p = providers();
        let mut t = team(&p, "A").await;
        let mut other = team(&p, "B").await;
        let ann = player(&p, &mut t, "Ann", 1).await;
        let visitor = player(&p, &mut other, "Vera", 9).await;
        let s = completed(&p, &t, day(11, 2)).await;
        p.training
            .save_attendance(
                &s,
                vec![line(ann, Present, false), line(visitor, Present, true)],
            )
            .await
            .unwrap();
        let report = p
            .training
            .attendance_report(&t.id, Scope::All)
            .await
            .unwrap();
        assert_eq!(report.players.len(), 1);
        assert_eq!(report.players[0].person_id, ann);
        // and the other team's own report has no row for the visitor either
        assert_eq!(
            p.training
                .attendance_report(&other.id, Scope::All)
                .await
                .unwrap()
                .players
                .len(),
            0
        );
    }

    #[tokio::test]
    async fn all_unavailable_has_a_row_without_percentage() {
        let p = providers();
        let mut t = team(&p, "A").await;
        let ann = player(&p, &mut t, "Ann", 1).await;
        for d in [2, 3] {
            let s = completed(&p, &t, day(11, d)).await;
            mark(&p, &s, vec![(ann, Unavailable)]).await;
        }
        let report = p
            .training
            .attendance_report(&t.id, Scope::All)
            .await
            .unwrap();
        let row = &report.players[0];
        assert_eq!((row.base(), row.percent(), row.unavailable), (0, None, 2));
    }

    #[tokio::test]
    async fn a_player_who_left_the_team_keeps_the_row_and_the_list_is_ordered() {
        let p = providers();
        let mut t = team(&p, "A").await;
        let ann = player(&p, &mut t, "Ann", 1).await;
        let bea = player(&p, &mut t, "Bea", 2).await;
        let cleo = player(&p, &mut t, "Cleo", 3).await;
        let dina = player(&p, &mut t, "Dina", 4).await;
        // Ann 1/2, Bea 2/2, Cleo 2/2 (also unavailable once), Dina all unavailable
        let s1 = completed(&p, &t, day(11, 2)).await;
        mark(
            &p,
            &s1,
            vec![
                (ann, Present),
                (bea, Present),
                (cleo, Present),
                (dina, Unavailable),
            ],
        )
        .await;
        let s2 = completed(&p, &t, day(11, 3)).await;
        mark(
            &p,
            &s2,
            vec![
                (ann, Absent),
                (bea, Present),
                (cleo, Late),
                (dina, Unavailable),
            ],
        )
        .await;
        let s3 = completed(&p, &t, day(11, 4)).await;
        mark(
            &p,
            &s3,
            vec![
                (ann, Unavailable),
                (bea, Unavailable),
                (cleo, Unavailable),
                (dina, Unavailable),
            ],
        )
        .await;
        // Bea leaves the team: her lines stay
        p.teams.remove_player(bea, &mut t).await.unwrap();
        let report = p
            .training
            .attendance_report(&t.id, Scope::All)
            .await
            .unwrap();
        let order: Vec<&str> = report.players.iter().map(|r| r.name.as_str()).collect();
        // 100%, 100% (by name), 50%, none last
        assert_eq!(order, ["Bea", "Cleo", "Ann", "Dina"]);
        assert_eq!(report.sessions, 3);
    }

    #[tokio::test]
    async fn the_attendance_of_a_cycle_or_a_range_counts_its_sessions_only() {
        let p = providers();
        let mut t = team(&p, "A").await;
        let ann = player(&p, &mut t, "Ann", 1).await;
        let micro = p
            .training
            .create_cycle(
                &t.id,
                Level::Microcycle,
                cycle(None, day(11, 2), Span::Weeks(1)),
                vec![],
            )
            .await
            .unwrap();
        let inside = session(
            &p,
            &t,
            day(11, 3),
            90,
            "",
            SessionStatus::Completed,
            Some(micro.id),
        )
        .await;
        let outside = completed(&p, &t, day(11, 12)).await;
        mark(&p, &inside, vec![(ann, Present)]).await;
        mark(&p, &outside, vec![(ann, Absent)]).await;
        let (training, team_id) = (&p.training, t.id);
        let of = |scope| async move {
            let r = training.attendance_report(&team_id, scope).await.unwrap();
            (r.sessions, r.players[0].percent())
        };
        assert_eq!(
            of(Scope::Cycle(Level::Microcycle, micro.id)).await,
            (1, Some(100))
        );
        assert_eq!(
            of(Scope::Dates(Window {
                start: day(11, 10),
                end: day(11, 30)
            }))
            .await,
            (1, Some(0))
        );
        assert_eq!(of(Scope::All).await, (2, Some(50)));
    }

    #[test]
    fn the_percentage_rounds_to_the_nearest() {
        use crate::training::totals::PlayerAttendance;
        let row = |present, absent| PlayerAttendance {
            person_id: Uuid::nil(),
            name: String::new(),
            number: None,
            present,
            late: 0,
            absent,
            unavailable: 0,
            makeups: 0,
        };
        assert_eq!(row(1, 2).percent(), Some(33));
        assert_eq!(row(2, 1).percent(), Some(67));
        assert_eq!(row(0, 0).percent(), None);
        assert_eq!(row(3, 0).percent(), Some(100));
    }

    // the makeups: a guest line marked so makes up for an absence

    /// Ada (A) is absent on the 3rd, 4th and 5th and present on the 2nd.
    async fn ada_misses_three(p: &P) -> (TeamEntry, TeamEntry, Uuid) {
        let mut a = team(p, "A").await;
        let b = team(p, "B").await;
        let ada = player(p, &mut a, "Ada", 1).await;
        for (d, status) in [(2, Present), (3, Absent), (4, Absent), (5, Absent)] {
            let s = completed(p, &a, day(11, d)).await;
            mark(p, &s, vec![(ada, status)]).await;
        }
        (a, b, ada)
    }

    /// A session of B on the day, completed, where Ada is a guest.
    async fn guest_of_b(
        p: &P,
        b: &TeamEntry,
        ada: Uuid,
        d: u32,
        status: AttendanceStatus,
        makeup: bool,
    ) -> Uuid {
        let s = completed(p, b, day(11, d)).await;
        p.training
            .save_attendance(
                &s,
                vec![AttendanceInput {
                    number: Some(1),
                    makeup,
                    ..line(ada, status, true)
                }],
            )
            .await
            .unwrap();
        s
    }

    async fn ada_row(
        p: &P,
        a: &TeamEntry,
        scope: Scope,
    ) -> crate::training::totals::PlayerAttendance {
        p.training
            .attendance_report(&a.id, scope)
            .await
            .unwrap()
            .players
            .remove(0)
    }

    #[tokio::test]
    async fn a_makeup_takes_the_place_of_an_absence() {
        let p = providers();
        let (a, b, ada) = ada_misses_three(&p).await;
        let before = ada_row(&p, &a, Scope::All).await;
        assert_eq!((before.present, before.absent, before.makeups), (1, 3, 0));
        assert_eq!(before.percent(), Some(25));
        guest_of_b(&p, &b, ada, 6, Present, true).await;
        guest_of_b(&p, &b, ada, 7, Late, true).await;
        let after = ada_row(&p, &a, Scope::All).await;
        assert_eq!(
            (after.present, after.late, after.absent, after.makeups),
            (3, 0, 1, 2)
        );
        assert_eq!(after.percent(), Some(75));
    }

    #[tokio::test]
    async fn the_makeups_are_at_most_the_absences() {
        let p = providers();
        let (a, b, ada) = ada_misses_three(&p).await;
        for d in 6..=10 {
            guest_of_b(&p, &b, ada, d, Present, true).await;
        }
        let row = ada_row(&p, &a, Scope::All).await;
        // three absences made up, two more makeups that make up nothing
        assert_eq!((row.present, row.absent, row.makeups), (4, 0, 3));
        assert_eq!(row.percent(), Some(100));
        // with no absence, none at all
        let mut c = team(&p, "C").await;
        let eve = player(&p, &mut c, "Eve", 2).await;
        let s = completed(&p, &c, day(11, 2)).await;
        mark(&p, &s, vec![(eve, Present)]).await;
        guest_of_b(&p, &b, eve, 11, Present, true).await;
        let row = ada_row(&p, &c, Scope::All).await;
        assert_eq!((row.present, row.absent, row.makeups), (1, 0, 0));
        assert_eq!(row.percent(), Some(100));
    }

    #[tokio::test]
    async fn only_a_present_marked_guest_line_in_a_completed_session_of_another_team_counts() {
        let p = providers();
        let (a, b, ada) = ada_misses_three(&p).await;
        // not marked as a makeup
        guest_of_b(&p, &b, ada, 6, Present, false).await;
        // marked, but absent or unavailable
        guest_of_b(&p, &b, ada, 7, Absent, true).await;
        guest_of_b(&p, &b, ada, 8, Unavailable, true).await;
        // marked and present, in a session not completed
        let s = guest_of_b(&p, &b, ada, 9, Present, true).await;
        p.training
            .set_session_status(&s, SessionStatus::Cancelled)
            .await
            .unwrap();
        let s = guest_of_b(&p, &b, ada, 10, Present, true).await;
        p.training
            .set_session_status(&s, SessionStatus::Planned)
            .await
            .unwrap();
        // marked and present, in a session removed
        let s = guest_of_b(&p, &b, ada, 11, Present, true).await;
        p.training.remove_session(&s).await.unwrap();
        // a person who is not of the team
        let mut c = team(&p, "C").await;
        let eve = player(&p, &mut c, "Eve", 2).await;
        guest_of_b(&p, &b, eve, 12, Present, true).await;
        let row = ada_row(&p, &a, Scope::All).await;
        assert_eq!((row.present, row.absent, row.makeups), (1, 3, 0));
        // the one that counts, the day after
        guest_of_b(&p, &b, ada, 13, Present, true).await;
        assert_eq!(ada_row(&p, &a, Scope::All).await.makeups, 1);
    }

    #[tokio::test]
    async fn the_makeups_of_a_period_are_those_that_start_in_it() {
        let p = providers();
        let (a, b, ada) = ada_misses_three(&p).await;
        guest_of_b(&p, &b, ada, 6, Present, true).await;
        guest_of_b(&p, &b, ada, 17, Present, true).await;
        let in_dates = |from: NaiveDate, to: NaiveDate| {
            Scope::Dates(Window {
                start: from,
                end: to,
            })
        };
        let made = |scope: Scope| {
            let (p, id) = (&p, a.id);
            async move {
                p.training
                    .attendance_report(&id, scope)
                    .await
                    .unwrap()
                    .players
                    .first()
                    .map(|row| row.makeups)
                    .unwrap_or(0)
            }
        };
        // the sessions of A are from the 2nd to the 5th: the period has to hold
        // them to show the player; the makeups are those that start in it
        assert_eq!(made(in_dates(day(11, 2), day(11, 9))).await, 1);
        assert_eq!(made(in_dates(day(11, 2), day(11, 6))).await, 0);
        assert_eq!(made(Scope::All).await, 2);
        assert_eq!(made(in_dates(day(11, 9), day(11, 20))).await, 0);
    }

    #[tokio::test]
    async fn the_window_of_a_cycle_is_the_period_of_its_makeups() {
        let p = providers();
        let (a, b, ada) = ada_misses_three(&p).await;
        // the week of the 2nd holds the sessions of A, the makeup is on the 6th (Friday)
        let micro = p
            .training
            .create_cycle(
                &a.id,
                Level::Microcycle,
                cycle(None, day(11, 2), Span::Weeks(1)),
                vec![],
            )
            .await
            .unwrap();
        let sessions = p.training.sessions(&a.id).await.unwrap();
        p.training
            .add_children(
                Level::Microcycle,
                &micro.id,
                sessions.iter().map(|s| s.id).collect(),
            )
            .await
            .unwrap();
        guest_of_b(&p, &b, ada, 6, Present, true).await;
        guest_of_b(&p, &b, ada, 17, Present, true).await;
        let row = ada_row(&p, &a, Scope::Cycle(Level::Microcycle, micro.id)).await;
        assert_eq!((row.present, row.absent, row.makeups), (2, 2, 1));
        // the cycle of another team gives nothing
        let other = p
            .training
            .attendance_report(&b.id, Scope::Cycle(Level::Microcycle, micro.id))
            .await
            .unwrap();
        assert!(other.players.is_empty());
    }

    #[tokio::test]
    async fn the_makeup_is_kept_only_for_a_guest() {
        let p = providers();
        let (a, b, ada) = ada_misses_three(&p).await;
        let s = completed(&p, &a, day(11, 9)).await;
        p.training
            .save_attendance(
                &s,
                vec![AttendanceInput {
                    makeup: true,
                    ..line(ada, Present, false)
                }],
            )
            .await
            .unwrap();
        assert!(!p.training.attendance(&s).await.unwrap().rows[0].makeup);
        let g = guest_of_b(&p, &b, ada, 6, Present, true).await;
        let sheet = p.training.attendance(&g).await.unwrap();
        assert!(sheet.rows[0].guest && sheet.rows[0].makeup);
        // and the same line gives back what was saved
        assert!(AttendanceInput::from(&sheet.rows[0]).makeup);
    }
}
