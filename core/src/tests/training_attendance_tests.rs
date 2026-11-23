//! The attendance of a session (TODO/021 step 5): the first open fills in
//! the active roster, then what was saved is the list; guests; removed
//! persons.

#[cfg(test)]
mod tests {
    use crate::{
        providers::{
            sqlite::{
                Database, persons::SqlitePersons, teams::SqliteTeamWriter, training::SqliteTraining,
            },
            team_writer::{PlayerDetails, PlayerInput, TeamInput, TeamWriter},
            training_reader::TrainingReader,
            training_writer::TrainingWriter,
        },
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, RoleEnum, TeamClassificationEnum},
            team::TeamEntry,
        },
        training::{AttendanceInput, AttendanceRow, AttendanceStatus, form::SessionForm},
    };
    use chrono::NaiveDate;
    use uuid::Uuid;

    struct P {
        teams: SqliteTeamWriter,
        training: SqliteTraining,
        persons: SqlitePersons,
    }

    fn providers() -> P {
        let db = Database::new(crate::db::open_in_memory().unwrap());
        P {
            teams: SqliteTeamWriter::new(db.clone()),
            persons: SqlitePersons::new(db.clone()),
            training: SqliteTraining::new(db),
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

    async fn session(p: &P, team: &TeamEntry) -> Uuid {
        session_on(p, team, 23).await
    }

    async fn session_on(p: &P, team: &TeamEntry, day: u32) -> Uuid {
        let input = SessionForm {
            start: NaiveDate::from_ymd_opt(2026, 11, day).and_then(|d| d.and_hms_opt(18, 0, 0)),
            ..SessionForm::default()
        }
        .validate()
        .unwrap();
        p.training.create_session(&team.id, input).await.unwrap().id
    }

    fn names(rows: &[AttendanceRow]) -> Vec<&str> {
        rows.iter().map(|r| r.name.as_str()).collect()
    }

    fn line(row: &AttendanceRow, status: AttendanceStatus, note: &str) -> AttendanceInput {
        AttendanceInput {
            status,
            note: note.into(),
            ..AttendanceInput::from(row)
        }
    }

    #[tokio::test]
    async fn a_new_session_has_no_attendance_written() {
        let p = providers();
        let mut t = team(&p, "A").await;
        player(&p, &mut t, "Ada", 1).await;
        let s = session(&p, &t).await;
        let count: u32 = {
            let sheet = p.training.attendance(&s).await.unwrap();
            assert!(!sheet.recorded);
            sheet.rows.len() as u32
        };
        // reading doesn't write
        assert_eq!(count, 1);
        assert!(!p.training.attendance(&s).await.unwrap().recorded);
    }

    #[tokio::test]
    async fn the_first_open_lists_the_active_roster_all_present_in_order() {
        let p = providers();
        let mut t = team(&p, "A").await;
        player(&p, &mut t, "Zoe", 9).await;
        let removed = player(&p, &mut t, "Bea", 5).await;
        player(&p, &mut t, "Ada", 1).await;
        p.teams.remove_player(removed, &mut t).await.unwrap();
        let s = session(&p, &t).await;
        let sheet = p.training.attendance(&s).await.unwrap();
        assert!(!sheet.recorded);
        assert_eq!(names(&sheet.rows), ["Zoe", "Ada"]);
        assert!(
            sheet
                .rows
                .iter()
                .all(|r| r.status == AttendanceStatus::Present && !r.guest && r.note.is_empty())
        );
        assert_eq!(sheet.rows[0].number, Some(9));
        assert!(p.training.missing_players(&s).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn what_is_saved_is_the_list_with_status_and_note() {
        let p = providers();
        let mut t = team(&p, "A").await;
        player(&p, &mut t, "Ada", 1).await;
        player(&p, &mut t, "Bea", 2).await;
        let s = session(&p, &t).await;
        let rows = p.training.attendance(&s).await.unwrap().rows;
        p.training
            .save_attendance(
                &s,
                vec![
                    line(&rows[0], AttendanceStatus::Late, "traffic"),
                    line(&rows[1], AttendanceStatus::Absent, ""),
                ],
            )
            .await
            .unwrap();
        let sheet = p.training.attendance(&s).await.unwrap();
        assert!(sheet.recorded);
        assert_eq!(names(&sheet.rows), ["Ada", "Bea"]);
        assert_eq!(sheet.rows[0].status, AttendanceStatus::Late);
        assert_eq!(sheet.rows[0].note, "traffic");
        assert_eq!(sheet.rows[1].status, AttendanceStatus::Absent);
    }

    #[tokio::test]
    async fn saving_again_replaces_the_list_and_a_left_out_row_is_removed() {
        let p = providers();
        let mut t = team(&p, "A").await;
        player(&p, &mut t, "Ada", 1).await;
        player(&p, &mut t, "Bea", 2).await;
        let s = session(&p, &t).await;
        let rows = p.training.attendance(&s).await.unwrap().rows;
        let all: Vec<_> = rows.iter().map(AttendanceInput::from).collect();
        p.training.save_attendance(&s, all.clone()).await.unwrap();
        p.training
            .save_attendance(&s, vec![all[1].clone()])
            .await
            .unwrap();
        let sheet = p.training.attendance(&s).await.unwrap();
        assert_eq!(names(&sheet.rows), ["Bea"]);
        // the left out one is added back by hand
        let missing = p.training.missing_players(&s).await.unwrap();
        assert_eq!(names(&missing), ["Ada"]);
        p.training.save_attendance(&s, all).await.unwrap();
        assert_eq!(
            names(&p.training.attendance(&s).await.unwrap().rows),
            ["Ada", "Bea"]
        );
        assert!(p.training.missing_players(&s).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_player_who_joined_later_is_missing_and_the_saved_list_stays() {
        let p = providers();
        let mut t = team(&p, "A").await;
        player(&p, &mut t, "Ada", 1).await;
        let s = session(&p, &t).await;
        let rows = p.training.attendance(&s).await.unwrap().rows;
        p.training
            .save_attendance(&s, rows.iter().map(AttendanceInput::from).collect())
            .await
            .unwrap();
        player(&p, &mut t, "Cleo", 3).await;
        assert_eq!(
            names(&p.training.attendance(&s).await.unwrap().rows),
            ["Ada"]
        );
        let missing = p.training.missing_players(&s).await.unwrap();
        assert_eq!(names(&missing), ["Cleo"]);
        assert_eq!(missing[0].number, Some(3));
    }

    #[tokio::test]
    async fn a_guest_of_another_team_is_listed_last_and_can_be_taken_off() {
        let p = providers();
        let mut a = team(&p, "A").await;
        let mut b = team(&p, "B").await;
        player(&p, &mut a, "Ada", 1).await;
        let guest = player(&p, &mut b, "Gina", 7).await;
        let s = session(&p, &a).await;
        let mut lines: Vec<_> = p
            .training
            .attendance(&s)
            .await
            .unwrap()
            .rows
            .iter()
            .map(AttendanceInput::from)
            .collect();
        lines.insert(
            0,
            AttendanceInput {
                person_id: guest,
                number: Some(7),
                status: AttendanceStatus::Present,
                note: String::new(),
                guest: true,
                makeup: false,
            },
        );
        p.training.save_attendance(&s, lines.clone()).await.unwrap();
        let sheet = p.training.attendance(&s).await.unwrap();
        assert_eq!(names(&sheet.rows), ["Ada", "Gina"]);
        assert!(sheet.rows[1].guest);
        assert_eq!(sheet.rows[1].number, Some(7));
        // the guest stays of their team
        assert_eq!(p.persons.teams_of(&guest).unwrap().len(), 1);
        assert!(p.training.missing_players(&s).await.unwrap().is_empty());
        lines.remove(0);
        p.training.save_attendance(&s, lines).await.unwrap();
        assert_eq!(
            names(&p.training.attendance(&s).await.unwrap().rows),
            ["Ada"]
        );
    }

    #[tokio::test]
    async fn impossible_lines_are_refused_and_nothing_is_written() {
        let p = providers();
        let mut t = team(&p, "A").await;
        let ada = player(&p, &mut t, "Ada", 1).await;
        let s = session(&p, &t).await;
        let ok = AttendanceInput {
            person_id: ada,
            number: Some(1),
            status: AttendanceStatus::Present,
            note: String::new(),
            guest: false,
            makeup: false,
        };
        // twice
        assert!(
            p.training
                .save_attendance(&s, vec![ok.clone(), ok.clone()])
                .await
                .is_err()
        );
        // a member of the team as a guest
        assert!(
            p.training
                .save_attendance(
                    &s,
                    vec![AttendanceInput {
                        guest: true,
                        ..ok.clone()
                    }]
                )
                .await
                .is_err()
        );
        // a person that doesn't exist
        assert!(
            p.training
                .save_attendance(
                    &s,
                    vec![AttendanceInput {
                        person_id: Uuid::new_v4(),
                        ..ok.clone()
                    }]
                )
                .await
                .is_err()
        );
        assert!(!p.training.attendance(&s).await.unwrap().recorded);
        // a removed session
        p.training.remove_session(&s).await.unwrap();
        assert!(p.training.save_attendance(&s, vec![ok]).await.is_err());
        assert!(p.training.attendance(&s).await.is_err());
    }

    #[tokio::test]
    async fn a_person_removed_from_the_registry_stays_without_a_name() {
        let p = providers();
        let mut t = team(&p, "A").await;
        let ada = player(&p, &mut t, "Ada", 1).await;
        player(&p, &mut t, "Bea", 2).await;
        let s = session(&p, &t).await;
        let rows = p.training.attendance(&s).await.unwrap().rows;
        p.training
            .save_attendance(&s, rows.iter().map(AttendanceInput::from).collect())
            .await
            .unwrap();
        p.persons.remove(&ada).unwrap();
        let sheet = p.training.attendance(&s).await.unwrap();
        assert_eq!(sheet.rows.len(), 2);
        let gone = sheet.rows.iter().find(|r| r.person_id == ada).unwrap();
        assert!(gone.name.is_empty());
        assert_eq!(gone.number, Some(1));
        // the sheet saves again with their row, which keeps its status
        let mut again: Vec<AttendanceInput> =
            sheet.rows.iter().map(AttendanceInput::from).collect();
        again
            .iter_mut()
            .filter(|r| r.person_id != ada)
            .for_each(|r| r.status = AttendanceStatus::Late);
        p.training.save_attendance(&s, again).await.unwrap();
        assert_eq!(p.training.attendance(&s).await.unwrap().rows.len(), 2);
        // never a removed person to add
        assert!(p.training.missing_players(&s).await.unwrap().is_empty());
        // a roster not yet saved doesn't list them
        let other = session_on(&p, &t, 24).await;
        assert_eq!(
            names(&p.training.attendance(&other).await.unwrap().rows),
            ["Bea"]
        );
    }

    #[tokio::test]
    async fn a_note_is_saved_trimmed() {
        let p = providers();
        let mut t = team(&p, "A").await;
        player(&p, &mut t, "Ada", 1).await;
        let s = session(&p, &t).await;
        let mut row = p.training.attendance(&s).await.unwrap().rows.remove(0);
        row.note = "  knee \n".into();
        p.training
            .save_attendance(&s, vec![AttendanceInput::from(&row)])
            .await
            .unwrap();
        assert_eq!(
            p.training.attendance(&s).await.unwrap().rows[0].note,
            "knee"
        );
    }
}
