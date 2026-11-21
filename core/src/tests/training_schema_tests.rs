//! Schema v5 of the database (TODO/021 step 1): the tables of the training
//! and every constraint the schema checks by itself (each test inserts a
//! valid row, then breaks one rule), and the migration of a database of v4
//! with its data.

#[cfg(test)]
mod tests {
    use crate::db::{self, MIGRATIONS};
    use rusqlite::Connection;

    const TEAM_A: &str = "aaaaaaaa-0000-4000-8000-000000000001";
    const TEAM_B: &str = "bbbbbbbb-0000-4000-8000-000000000002";
    const PROGRAM: &str = "11111111-0000-4000-8000-000000000001";
    const PROGRAM_B: &str = "11111111-0000-4000-8000-000000000002";
    const MESO: &str = "22222222-0000-4000-8000-000000000001";
    const MESO_B: &str = "22222222-0000-4000-8000-000000000002";
    const MICRO: &str = "33333333-0000-4000-8000-000000000001";
    const MICRO_B: &str = "33333333-0000-4000-8000-000000000002";
    const SESSION: &str = "44444444-0000-4000-8000-000000000001";
    const ACTIVITY: &str = "55555555-0000-4000-8000-000000000001";
    const PERSON: &str = "66666666-0000-4000-8000-000000000001";

    /// Two teams, a person, and the training of A: a program, a mesocycle,
    /// a microcycle, a session with an activity and an attendance; the
    /// training of B is a program, a mesocycle and a microcycle.
    fn database() -> Connection {
        let db = db::open_in_memory().unwrap();
        db.execute_batch(&format!(
            "INSERT INTO teams (id, name, year) VALUES ('{TEAM_A}', 'A', 2026), ('{TEAM_B}', 'B', 2026);
             INSERT INTO persons (id, name) VALUES ('{PERSON}', 'p');
             INSERT INTO training_programs (id, team_id, start_date, end_date)
                 VALUES ('{PROGRAM}', '{TEAM_A}', '2026-09-01', '2026-12-20'),
                        ('{PROGRAM_B}', '{TEAM_B}', '2026-09-01', '2026-12-20');
             INSERT INTO mesocycles (id, team_id, program_id, start_date, weeks)
                 VALUES ('{MESO}', '{TEAM_A}', '{PROGRAM}', '2026-11-02', 4),
                        ('{MESO_B}', '{TEAM_B}', '{PROGRAM_B}', '2026-11-02', 4);
             INSERT INTO microcycles (id, team_id, mesocycle_id, start_date, weeks)
                 VALUES ('{MICRO}', '{TEAM_A}', '{MESO}', '2026-11-02', 2),
                        ('{MICRO_B}', '{TEAM_B}', '{MESO_B}', '2026-11-02', 2);
             INSERT INTO training_sessions (id, team_id, microcycle_id, start_local, duration_min)
                 VALUES ('{SESSION}', '{TEAM_A}', '{MICRO}', '2026-11-03T20:30', 90);
             INSERT INTO training_activities (id, session_id, position, name)
                 VALUES ('{ACTIVITY}', '{SESSION}', 0, 'serve');
             INSERT INTO training_attendance (session_id, person_id, status)
                 VALUES ('{SESSION}', '{PERSON}', 'present');"
        ))
        .unwrap();
        db
    }

    fn fails(db: &Connection, sql: &str) {
        assert!(db.execute_batch(sql).is_err(), "accepted: {sql}");
    }

    fn passes(db: &Connection, sql: &str) {
        db.execute_batch(sql)
            .unwrap_or_else(|e| panic!("refused: {sql}: {e}"));
    }

    fn session(start: &str, duration: &str) -> String {
        format!(
            "INSERT INTO training_sessions (id, team_id, start_local, duration_min)
             VALUES ('44444444-0000-4000-8000-0000000000ff', '{TEAM_A}', '{start}', {duration})"
        )
    }

    #[test]
    fn a_new_database_is_at_the_version_of_the_training() {
        assert!(MIGRATIONS.len() >= 5);
        let db = database();
        let version: i64 = db
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, db::schema_version());
    }

    #[test]
    fn a_program_ends_with_its_start_or_later() {
        let db = database();
        passes(&db, &format!(
            "INSERT INTO training_programs (id, team_id, start_date, end_date)
             VALUES ('11111111-0000-4000-8000-0000000000f1', '{TEAM_A}', '2026-12-01', '2026-12-01')"));
        fails(&db, &format!(
            "INSERT INTO training_programs (id, team_id, start_date, end_date)
             VALUES ('11111111-0000-4000-8000-0000000000f2', '{TEAM_A}', '2026-12-02', '2026-12-01')"));
        fails(&db, &format!(
            "INSERT INTO training_programs (id, team_id, start_date, end_date)
             VALUES ('11111111-0000-4000-8000-0000000000f3', '{TEAM_A}', '2026-13-01', '2026-13-02')"));
    }

    #[test]
    fn the_cycles_have_at_least_a_week_and_a_real_date() {
        let db = database();
        for table in ["mesocycles", "microcycles"] {
            let insert = |date: &str, weeks: &str| {
                format!(
                    "INSERT INTO {table} (id, team_id, start_date, weeks)
                     VALUES ('77777777-0000-4000-8000-0000000000{}', '{TEAM_A}', '{date}', {weeks})",
                    if table == "mesocycles" { "a1" } else { "a2" }
                )
            };
            fails(&db, &insert("2026-11-02", "0"));
            fails(&db, &insert("2026-02-30", "1"));
            fails(&db, &insert("2026-11-02", "65536"));
            passes(&db, &insert("2026-11-02", "1"));
        }
    }

    #[test]
    fn a_session_has_a_local_time_and_a_positive_duration() {
        let db = database();
        fails(&db, &session("2026-11-03T20:30:00", "90"));
        fails(&db, &session("2026-11-03T20:30+01:00", "90"));
        fails(&db, &session("2026-11-03 20:30", "90"));
        fails(&db, &session("2026-11-03T25:30", "90"));
        fails(&db, &session("2026-11-03", "90"));
        fails(&db, &session("2026-11-03T20:30", "0"));
        fails(&db, &session("2026-11-03T20:30", "NULL"));
        passes(&db, &session("2026-11-03T20:30", "90"));
        fails(
            &db,
            &format!("UPDATE training_sessions SET actual_duration_min = 0 WHERE id = '{SESSION}'"),
        );
        passes(
            &db,
            &format!(
                "UPDATE training_sessions SET actual_duration_min = 75 WHERE id = '{SESSION}'"
            ),
        );
    }

    #[test]
    fn the_status_is_one_of_the_closed_sets() {
        let db = database();
        for status in ["planned", "completed", "cancelled"] {
            passes(
                &db,
                &format!("UPDATE training_sessions SET status = '{status}' WHERE id = '{SESSION}'"),
            );
        }
        fails(
            &db,
            &format!("UPDATE training_sessions SET status = 'done' WHERE id = '{SESSION}'"),
        );
        for status in ["planned", "completed", "skipped"] {
            passes(
                &db,
                &format!(
                    "UPDATE training_activities SET status = '{status}' WHERE id = '{ACTIVITY}'"
                ),
            );
        }
        fails(
            &db,
            &format!("UPDATE training_activities SET status = 'cancelled' WHERE id = '{ACTIVITY}'"),
        );
        for status in ["present", "absent", "late", "unavailable"] {
            passes(
                &db,
                &format!(
                    "UPDATE training_attendance SET status = '{status}' WHERE session_id = '{SESSION}'"
                ),
            );
        }
        fails(
            &db,
            &format!(
                "UPDATE training_attendance SET status = 'injured' WHERE session_id = '{SESSION}'"
            ),
        );
    }

    #[test]
    fn an_activity_has_a_name_and_a_place_in_its_session() {
        let db = database();
        let insert = |id: &str, pos: i64, name: &str| {
            format!(
                "INSERT INTO training_activities (id, session_id, position, name)
                 VALUES ('{id}', '{SESSION}', {pos}, '{name}')"
            )
        };
        fails(&db, &insert("55555555-0000-4000-8000-0000000000a1", 1, ""));
        fails(
            &db,
            &insert("55555555-0000-4000-8000-0000000000a1", 0, "again"),
        );
        fails(
            &db,
            &insert("55555555-0000-4000-8000-0000000000a1", -1, "x"),
        );
        passes(
            &db,
            &insert("55555555-0000-4000-8000-0000000000a1", 1, "block"),
        );
        fails(
            &db,
            &format!("UPDATE training_activities SET planned_min = 0 WHERE id = '{ACTIVITY}'"),
        );
    }

    #[test]
    fn the_attendance_is_one_row_per_person_and_session() {
        let db = database();
        fails(
            &db,
            &format!(
                "INSERT INTO training_attendance (session_id, person_id, status)
             VALUES ('{SESSION}', '{PERSON}', 'late')"
            ),
        );
        fails(
            &db,
            &format!(
                "INSERT INTO training_attendance (session_id, person_id, status)
             VALUES ('{SESSION}', '66666666-0000-4000-8000-0000000000ee', 'late')"
            ),
        );
        fails(
            &db,
            &format!("UPDATE training_attendance SET guest = 2 WHERE session_id = '{SESSION}'"),
        );
        fails(
            &db,
            &format!("UPDATE training_attendance SET number = 256 WHERE session_id = '{SESSION}'"),
        );
        passes(
            &db,
            &format!(
                "UPDATE training_attendance SET guest = 1, number = 7, note = 'visiting'
             WHERE session_id = '{SESSION}'"
            ),
        );
    }

    #[test]
    fn a_parent_and_its_child_belong_to_the_same_team() {
        let db = database();
        fails(&db, &format!(
            "INSERT INTO mesocycles (id, team_id, program_id, start_date, weeks)
             VALUES ('22222222-0000-4000-8000-0000000000b1', '{TEAM_A}', '{PROGRAM_B}', '2026-11-02', 1)"));
        fails(&db, &format!(
            "INSERT INTO microcycles (id, team_id, mesocycle_id, start_date, weeks)
             VALUES ('33333333-0000-4000-8000-0000000000b1', '{TEAM_A}', '{MESO_B}', '2026-11-02', 1)"));
        fails(&db, &format!(
            "INSERT INTO training_sessions (id, team_id, microcycle_id, start_local, duration_min)
             VALUES ('44444444-0000-4000-8000-0000000000b1', '{TEAM_A}', '{MICRO_B}', '2026-11-03T20:30', 90)"));
        fails(
            &db,
            &format!("UPDATE mesocycles SET program_id = '{PROGRAM_B}' WHERE id = '{MESO}'"),
        );
        fails(
            &db,
            &format!("UPDATE microcycles SET mesocycle_id = '{MESO_B}' WHERE id = '{MICRO}'"),
        );
        fails(
            &db,
            &format!(
                "UPDATE training_sessions SET microcycle_id = '{MICRO_B}' WHERE id = '{SESSION}'"
            ),
        );
        fails(
            &db,
            &format!("UPDATE training_sessions SET team_id = '{TEAM_B}' WHERE id = '{SESSION}'"),
        );
        // a child without a parent, and one moved out of it
        passes(
            &db,
            &format!("UPDATE training_sessions SET microcycle_id = NULL WHERE id = '{SESSION}'"),
        );
        passes(
            &db,
            &format!("UPDATE microcycles SET mesocycle_id = NULL WHERE id = '{MICRO}'"),
        );
        passes(
            &db,
            &format!("UPDATE mesocycles SET program_id = NULL WHERE id = '{MESO}'"),
        );
    }

    #[test]
    fn a_parent_may_be_empty_and_a_child_may_be_free() {
        let db = database();
        passes(&db, &format!(
            "INSERT INTO training_programs (id, team_id, start_date, end_date)
             VALUES ('11111111-0000-4000-8000-0000000000c1', '{TEAM_A}', '2027-01-01', '2027-06-30');
             INSERT INTO training_sessions (id, team_id, start_local, duration_min)
             VALUES ('44444444-0000-4000-8000-0000000000c1', '{TEAM_A}', '2026-11-04T20:30', 90)"));
    }

    #[test]
    fn a_referred_row_is_not_removed() {
        let db = database();
        fails(
            &db,
            &format!("DELETE FROM training_programs WHERE id = '{PROGRAM}'"),
        );
        fails(&db, &format!("DELETE FROM mesocycles WHERE id = '{MESO}'"));
        fails(
            &db,
            &format!("DELETE FROM microcycles WHERE id = '{MICRO}'"),
        );
        fails(
            &db,
            &format!("DELETE FROM training_sessions WHERE id = '{SESSION}'"),
        );
        fails(&db, &format!("DELETE FROM persons WHERE id = '{PERSON}'"));
        fails(&db, &format!("DELETE FROM teams WHERE id = '{TEAM_A}'"));
    }

    /// A database of v4 with its data (a team, a person, a match) migrates
    /// to v5 keeping all of it, with the training empty.
    #[test]
    fn a_database_of_v4_migrates_keeping_its_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite");
        db::open_with(&path, &MIGRATIONS[..4]).unwrap().execute_batch(&format!(
            "INSERT INTO teams (id, name, year) VALUES ('{TEAM_A}', 'A', 2026), ('{TEAM_B}', 'B', 2026);
             INSERT INTO persons (id, name) VALUES ('{PERSON}', 'p');
             INSERT INTO team_members (team_id, person_id, position, number) VALUES ('{TEAM_A}', '{PERSON}', 0, 7);
             INSERT INTO matches (id, team_a, team_b, home, scouting, start, max_substitutions)
                 VALUES ('dddddddd-0000-4000-8000-000000000004', '{TEAM_A}', '{TEAM_B}', 'a', 'both',
                         '2026-11-12T18:00:00+01:00', 6);
             INSERT INTO sets (match_id, number, serving, lineup_rules, scouting_rules)
                 VALUES ('dddddddd-0000-4000-8000-000000000004', 1, 'a', 'v2', 'v4');"
        )).unwrap();
        let db = db::open(&path).unwrap();
        let count = |table: &str| -> i64 {
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(db::user_version(&db).unwrap(), db::schema_version());
        assert_eq!(
            ["teams", "persons", "team_members", "matches", "sets"].map(count),
            [2, 1, 1, 1, 1]
        );
        assert_eq!(
            [
                "training_programs",
                "mesocycles",
                "microcycles",
                "training_sessions",
                "training_activities",
                "training_attendance"
            ]
            .map(count),
            [0; 6]
        );
        passes(
            &db,
            &format!(
                "INSERT INTO training_sessions (id, team_id, start_local, duration_min)
             VALUES ('{SESSION}', '{TEAM_A}', '2026-11-03T20:30', 90)"
            ),
        );
    }
}
