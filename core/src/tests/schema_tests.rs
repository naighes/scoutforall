//! Schema v1 of the database (TODO/001): the tables and columns a new
//! database has, and every constraint the schema checks by itself (each
//! test inserts a valid row, then breaks one rule).

#[cfg(test)]
mod tests {
    use crate::db;
    use rusqlite::{Connection, params};

    const TEAM_A: &str = "aaaaaaaa-0000-4000-8000-000000000001";
    const TEAM_B: &str = "bbbbbbbb-0000-4000-8000-000000000002";
    const TEAM_C: &str = "cccccccc-0000-4000-8000-000000000003";
    const MATCH: &str = "dddddddd-0000-4000-8000-000000000004";
    const COMPETITION: &str = "eeeeeeee-0000-4000-8000-000000000005";
    const EVENT: &str = "ffffffff-0000-4000-8000-000000000006";

    /// Persons 1..=9 of A, 11..=19 of B.
    fn person(n: u8) -> String {
        format!("00000000-0000-4000-8000-0000000000{n:02}")
    }

    fn columns(db: &Connection, table: &str) -> Vec<String> {
        db.prepare(&format!(
            "SELECT name FROM pragma_table_info('{table}') ORDER BY cid"
        ))
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
    }

    fn names(db: &Connection, kind: &str) -> Vec<String> {
        db.prepare(
            "SELECT name FROM sqlite_schema WHERE type = ?1
             AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .unwrap()
        .query_map([kind], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
    }

    /// Two teams with their players, a match between them scouted on both
    /// sides, its first set with both lineups and an event.
    fn database() -> Connection {
        let db = db::open_in_memory().unwrap();
        db.execute_batch(&format!(
            "INSERT INTO teams (id, name, year) VALUES ('{TEAM_A}', 'A', 2026), ('{TEAM_B}', 'B', 2026),
                 ('{TEAM_C}', 'C', 2026);
             INSERT INTO competitions (id, name, format) VALUES ('{COMPETITION}', 'Serie C', 'league');
             INSERT INTO matches (id, team_a, team_b, home, scouting, start, max_substitutions)
                 VALUES ('{MATCH}', '{TEAM_A}', '{TEAM_B}', 'a', 'both',
                         '2026-11-12T18:00:00+01:00', 6);
             INSERT INTO sets (match_id, number, serving, lineup_rules) VALUES ('{MATCH}', 1, 'a', 'v2');"
        ))
        .unwrap();
        for (team, first) in [(TEAM_A, 1), (TEAM_B, 11)] {
            for n in first..first + 9 {
                db.execute(
                    "INSERT INTO persons (id, name) VALUES (?1, ?2)",
                    params![person(n), format!("player {n}")],
                )
                .unwrap();
                db.execute(
                    "INSERT INTO team_members (team_id, person_id, position, number, role)
                     VALUES (?1, ?2, ?3, ?4, 'outside_hitter')",
                    params![team, person(n), n - first, n],
                )
                .unwrap();
                db.execute(
                    "INSERT INTO match_roster (match_id, team_id, person_id, number)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![MATCH, team, person(n), n],
                )
                .unwrap();
            }
        }
        for (side, first) in [("a", 1), ("b", 11)] {
            insert_lineup(&db, side, first).unwrap();
        }
        insert_event(&db, EVENT, 0, Some(&person(1))).unwrap();
        db
    }

    fn insert_lineup(db: &Connection, side: &str, first: u8) -> rusqlite::Result<usize> {
        let p: Vec<String> = (first..first + 9).map(person).collect();
        lineup(
            db,
            side,
            [&p[0], &p[1], &p[2], &p[3], &p[4], &p[5]],
            &p[6],
            Some(&p[7]),
            &p[0],
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn lineup(
        db: &Connection,
        side: &str,
        court: [&String; 6],
        libero: &String,
        fallback: Option<&String>,
        setter: &String,
    ) -> rusqlite::Result<usize> {
        db.execute(
            "INSERT OR REPLACE INTO set_lineups (match_id, set_number, side, p1, p2, p3, p4, p5, p6,
                 libero, fallback_libero, setter)
             VALUES (?1, 1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                MATCH, side, court[0], court[1], court[2], court[3], court[4], court[5], libero,
                fallback, setter
            ],
        )
    }

    fn insert_event(
        db: &Connection,
        id: &str,
        seq: i64,
        player: Option<&String>,
    ) -> rusqlite::Result<usize> {
        db.execute(
            "INSERT INTO events (id, match_id, set_number, seq, side, timestamp, event_type, player,
                 eval, flags)
             VALUES (?1, ?2, 1, ?3, 'a', '2026-11-12T17:01:00Z', 'S', ?4, '#', 0)",
            params![id, MATCH, seq, player],
        )
    }

    fn fails(db: &Connection, sql: &str) {
        assert!(db.execute_batch(sql).is_err(), "accepted: {sql}");
    }

    fn passes(db: &Connection, sql: &str) {
        db.execute_batch(sql)
            .unwrap_or_else(|e| panic!("refused: {sql}: {e}"));
    }

    /// The schema of v1, table by table: a column added, removed or renamed
    /// by mistake shows up here (a released migration never changes).
    #[test]
    fn a_new_database_has_the_tables_of_v1() {
        let db = db::open_in_memory().unwrap();
        assert_eq!(db::schema_version(), 1);
        let expected: &[(&str, &[&str])] = &[
            (
                "competitions",
                &[
                    "id", "name", "format", "season", "gender", "country", "friendly", "deleted",
                ],
            ),
            (
                "events",
                &[
                    "id",
                    "match_id",
                    "set_number",
                    "seq",
                    "side",
                    "timestamp",
                    "event_type",
                    "player",
                    "eval",
                    "target_player",
                    "technique",
                    "flags",
                    "deleted",
                ],
            ),
            (
                "match_roster",
                &["match_id", "team_id", "person_id", "number", "role"],
            ),
            (
                "match_summary",
                &[
                    "match_id",
                    "sets",
                    "a_wins",
                    "b_wins",
                    "finished",
                    "resume",
                    "next_set_number",
                ],
            ),
            (
                "match_video_corrections",
                &["match_id", "event_id", "by_ms"],
            ),
            (
                "match_video_files",
                &["match_id", "position", "path", "start"],
            ),
            (
                "match_videos",
                &["match_id", "anchor_at", "anchor_offset_ms", "anchor_event"],
            ),
            (
                "matches",
                &[
                    "id",
                    "team_a",
                    "team_b",
                    "team_b_name",
                    "home",
                    "scouting",
                    "start",
                    "duration_min",
                    "friendly",
                    "competition_id",
                    "stage",
                    "round",
                    "max_substitutions",
                    "notes",
                    "source",
                    "video_clock",
                    "legacy_id",
                    "legacy_team",
                    "deleted",
                ],
            ),
            (
                "persons",
                &["id", "name", "birth_date", "height_cm", "notes", "deleted"],
            ),
            (
                "set_lineups",
                &[
                    "match_id",
                    "set_number",
                    "side",
                    "p1",
                    "p2",
                    "p3",
                    "p4",
                    "p5",
                    "p6",
                    "libero",
                    "fallback_libero",
                    "setter",
                ],
            ),
            (
                "sets",
                &["match_id", "number", "serving", "lineup_rules", "deleted"],
            ),
            (
                "team_members",
                &[
                    "team_id",
                    "person_id",
                    "position",
                    "number",
                    "role",
                    "deleted",
                ],
            ),
            (
                "teams",
                &[
                    "id",
                    "name",
                    "year",
                    "classification",
                    "gender",
                    "country",
                    "deleted",
                ],
            ),
            ("upload_queue", &["match_id", "queued_at", "retry_count"]),
        ];
        assert_eq!(
            names(&db, "table"),
            expected
                .iter()
                .map(|(t, _)| t.to_string())
                .collect::<Vec<_>>()
        );
        expected
            .iter()
            .for_each(|(table, cols)| assert_eq!(columns(&db, table), *cols, "{table}"));
        // every table STRICT, none WITHOUT ROWID
        let strict: Vec<(String, i64)> = db
            .prepare(
                "SELECT name, strict FROM pragma_table_list WHERE schema = 'main'
                      AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert!(strict.iter().all(|(_, s)| *s == 1), "{strict:?}");
        assert_eq!(
            names(&db, "trigger"),
            vec![
                "match_roster_team_insert",
                "match_roster_team_update",
                "match_video_anchor_insert",
                "match_video_anchor_update",
                "match_video_event_insert",
                "set_lineups_side_b_insert",
                "set_lineups_side_b_update",
            ]
        );
    }

    /// A file migrated to v1 has the same schema as one in memory, and the
    /// constraints hold after reopening.
    #[test]
    fn a_file_has_the_same_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite");
        drop(db::open(&path).unwrap());
        let file = db::open(&path).unwrap();
        let memory = db::open_in_memory().unwrap();
        let schema = |db: &Connection| -> Vec<String> {
            db.prepare("SELECT sql FROM sqlite_schema WHERE sql IS NOT NULL ORDER BY name")
                .unwrap()
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        assert_eq!(schema(&file), schema(&memory));
        fails(
            &file,
            &format!(
                "INSERT INTO team_members VALUES ('{TEAM_A}', '{}', 0, 1, NULL, 0)",
                person(1)
            ),
        );
    }

    #[test]
    fn the_valid_rows_go_in() {
        let db = database();
        // B only a name, neutral court, scouting only A, every optional field
        passes(
            &db,
            &format!(
                "INSERT INTO matches (id, team_a, team_b_name, home, scouting, start, duration_min,
                     friendly, competition_id, stage, round, max_substitutions, notes, source,
                     video_clock, legacy_id, legacy_team)
                 VALUES ('11111111-0000-4000-8000-000000000000', '{TEAM_A}', 'Volley Pisa', NULL,
                     'a', '2025-10-04T18:00:00+02:00', 120, 1, '{COMPETITION}', 'playoff',
                     'Semifinale – gara 2', 6, 'note', 'dvw', 1, '2025-10-04_volley_pisa',
                     '{TEAM_A}')"
            ),
        );
        passes(
            &db,
            &format!(
                "INSERT INTO persons (id, name, birth_date, height_cm, notes)
                 VALUES ('22222222-0000-4000-8000-000000000000', 'Giulia', '2004-03-01', 171, 'mancina');
                 UPDATE teams SET classification = 'national_mid_level', gender = 'women',
                     country = 'IT' WHERE id = '{TEAM_A}';
                 INSERT INTO match_summary VALUES ('{MATCH}', 1, 0, 0, 0, 1, NULL);
                 INSERT INTO upload_queue VALUES ('{MATCH}', 1794509535, 0);"
            ),
        );
    }

    #[test]
    fn the_ids_are_uuids() {
        let db = database();
        fails(&db, "INSERT INTO persons (id, name) VALUES ('1', 'x')");
        fails(
            &db,
            "INSERT INTO teams (id, name, year) VALUES (NULL, 'x', 2026)",
        );
        fails(
            &db,
            &format!(
                "INSERT INTO events (id, match_id, set_number, seq, side, timestamp, event_type)
                             VALUES ('e', '{MATCH}', 1, 9, 'a', 't', 'S')"
            ),
        );
        // STRICT: no text in an integer column
        fails(&db, "UPDATE teams SET year = 'duemila'");
    }

    #[test]
    fn the_flags_are_zero_or_one() {
        let db = database();
        [
            "persons",
            "teams",
            "team_members",
            "competitions",
            "matches",
            "sets",
            "events",
        ]
        .iter()
        .for_each(|table| fails(&db, &format!("UPDATE {table} SET deleted = 2")));
        fails(&db, "UPDATE matches SET friendly = 2");
        fails(&db, "UPDATE matches SET video_clock = -1");
        fails(&db, "UPDATE competitions SET friendly = 2");
        passes(
            &db,
            "UPDATE matches SET deleted = 1, friendly = 1, video_clock = 1",
        );
    }

    /// D16: a deleted person keeps only the id and the flag.
    #[test]
    fn a_deleted_person_has_no_personal_data() {
        let db = database();
        let id = person(1);
        fails(
            &db,
            &format!("UPDATE persons SET deleted = 1 WHERE id = '{id}'"),
        );
        passes(
            &db,
            &format!(
                "UPDATE persons SET birth_date = '2004-03-01', height_cm = 171,
                              notes = 'x' WHERE id = '{id}'"
            ),
        );
        let emptied = [
            ("name", "''"),
            ("birth_date", "NULL"),
            ("height_cm", "NULL"),
            ("notes", "''"),
        ];
        for kept in emptied.iter().map(|(c, _)| c) {
            let others: Vec<String> = emptied
                .iter()
                .filter(|(c, _)| c != kept)
                .map(|(c, v)| format!("{c} = {v}"))
                .collect();
            // all emptied but one: refused
            fails(
                &db,
                &format!(
                    "UPDATE persons SET deleted = 1, {} WHERE id = '{id}'",
                    others.join(", ")
                ),
            );
        }
        passes(
            &db,
            &format!(
                "UPDATE persons SET deleted = 1, name = '', birth_date = NULL,
                              height_cm = NULL, notes = '' WHERE id = '{id}'"
            ),
        );
        // and its events and memberships stay (an anonymous player)
        let events: i64 = db
            .query_row(
                "SELECT count(*) FROM events WHERE player = ?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(events, 1);
    }

    #[test]
    fn a_person_has_a_real_birth_date_and_height() {
        let db = database();
        let id = person(1);
        fails(
            &db,
            &format!("UPDATE persons SET birth_date = '2004-02-30' WHERE id = '{id}'"),
        );
        fails(
            &db,
            &format!("UPDATE persons SET birth_date = '01/03/2004' WHERE id = '{id}'"),
        );
        fails(
            &db,
            &format!("UPDATE persons SET birth_date = '2004-03-01T00:00:00' WHERE id = '{id}'"),
        );
        fails(
            &db,
            &format!("UPDATE persons SET height_cm = 0 WHERE id = '{id}'"),
        );
        fails(
            &db,
            &format!("UPDATE persons SET name = NULL WHERE id = '{id}'"),
        );
        passes(
            &db,
            &format!("UPDATE persons SET birth_date = '2004-02-29' WHERE id = '{id}'"),
        );
    }

    #[test]
    fn the_enumerations_are_closed() {
        let db = database();
        fails(&db, "UPDATE teams SET classification = 'regionale'");
        fails(&db, "UPDATE teams SET gender = 'mixed'");
        fails(&db, "UPDATE team_members SET role = 'Setter'");
        fails(&db, "UPDATE match_roster SET role = 'universal'");
        fails(&db, "UPDATE competitions SET format = 'friendly'");
        fails(&db, "UPDATE competitions SET gender = 'm'");
        fails(&db, "UPDATE matches SET stage = 'semifinal'");
        fails(&db, "UPDATE matches SET source = 'csv'");
        fails(&db, "UPDATE sets SET lineup_rules = 'v3'");
        fails(&db, "UPDATE sets SET serving = 'us'");
        fails(&db, "UPDATE events SET side = 'them'");
        for stage in [
            "regular",
            "pool",
            "qualification",
            "playoff",
            "playout",
            "final_round",
            "placement",
            "other",
        ] {
            passes(&db, &format!("UPDATE matches SET stage = '{stage}'"));
        }
        for role in [
            "libero",
            "opposite_hitter",
            "setter",
            "outside_hitter",
            "middle_blocker",
        ] {
            passes(
                &db,
                &format!(
                    "UPDATE team_members SET role = '{role}'; UPDATE match_roster SET role = '{role}'"
                ),
            );
        }
        passes(
            &db,
            "UPDATE team_members SET role = NULL; UPDATE sets SET lineup_rules = NULL",
        );
    }

    #[test]
    fn a_member_is_a_person_of_a_team_once() {
        let db = database();
        // the same person in another team: fine (D5)
        passes(
            &db,
            &format!(
                "INSERT INTO team_members (team_id, person_id, position, number)
                 VALUES ('{TEAM_C}', '{}', 0, 7)",
                person(1)
            ),
        );
        // twice in the same team: no
        fails(
            &db,
            &format!(
                "INSERT INTO team_members (team_id, person_id, position, number)
                 VALUES ('{TEAM_A}', '{}', 77, 77)",
                person(1)
            ),
        );
        fails(
            &db,
            &format!(
                "INSERT INTO team_members (team_id, person_id, position, number)
                 VALUES ('{TEAM_A}', '{}', 77, 1)",
                person(99)
            ),
        );
        fails(
            &db,
            &format!(
                "INSERT INTO team_members (team_id, person_id, position, number)
                 VALUES ('{MATCH}', '{}', 77, 1)",
                person(1)
            ),
        );
        fails(&db, "UPDATE team_members SET number = 256");
        fails(&db, "UPDATE team_members SET number = -1");
    }

    /// The players of a team are in the order of their places, one person
    /// per place.
    #[test]
    fn a_member_has_a_place_of_its_own_in_the_team() {
        let db = database();
        passes(
            &db,
            &format!(
                "INSERT INTO persons (id, name) VALUES ('{}', 'new')",
                person(99)
            ),
        );
        let new = |position: i64| {
            format!(
                "INSERT INTO team_members (team_id, person_id, position, number)
                 VALUES ('{TEAM_A}', '{}', {position}, 99)",
                person(99)
            )
        };
        // the place of another player of the team: no
        fails(&db, &new(0));
        fails(&db, &new(-1));
        fails(&db, &new(65536));
        // the same place in another team: fine
        passes(
            &db,
            &format!(
                "INSERT INTO team_members (team_id, person_id, position, number)
                 VALUES ('{TEAM_C}', '{}', 0, 1)",
                person(1)
            ),
        );
        passes(&db, &new(65535));
    }

    /// D3.
    #[test]
    fn team_b_is_a_team_or_a_name() {
        let db = database();
        fails(&db, "UPDATE matches SET team_b_name = 'B'");
        fails(&db, "UPDATE matches SET team_b = NULL");
        passes(
            &db,
            "UPDATE matches SET scouting = 'a', team_b = NULL, team_b_name = 'B'",
        );
        fails(&db, "UPDATE matches SET team_b_name = NULL");
        fails(
            &db,
            &format!("UPDATE matches SET team_b = '{COMPETITION}', team_b_name = NULL"),
        );
    }

    #[test]
    fn a_team_does_not_play_itself() {
        let db = database();
        fails(&db, &format!("UPDATE matches SET team_b = '{TEAM_A}'"));
        fails(&db, "UPDATE matches SET team_a = NULL");
    }

    /// D3: both sides need players on B.
    #[test]
    fn both_sides_are_scouted_only_against_a_team() {
        let db = database();
        fails(&db, "UPDATE matches SET team_b = NULL, team_b_name = 'B'");
        fails(
            &db,
            &format!(
                "INSERT INTO matches (id, team_a, team_b_name, scouting, start, max_substitutions)
             VALUES ('11111111-0000-4000-8000-000000000000', '{TEAM_A}', 'B', 'both', 's', 6)"
            ),
        );
        fails(&db, "UPDATE matches SET scouting = 'b'");
    }

    #[test]
    fn home_is_a_b_or_neutral() {
        let db = database();
        passes(&db, "UPDATE matches SET home = 'b'");
        passes(&db, "UPDATE matches SET home = NULL");
        fails(&db, "UPDATE matches SET home = 'us'");
        fails(&db, "UPDATE matches SET home = 1");
    }

    #[test]
    fn the_other_fields_of_a_match() {
        let db = database();
        fails(&db, "UPDATE matches SET duration_min = 0");
        fails(&db, "UPDATE matches SET max_substitutions = 256");
        fails(&db, "UPDATE matches SET start = NULL");
        fails(&db, "UPDATE matches SET notes = NULL");
        fails(
            &db,
            &format!("UPDATE matches SET competition_id = '{TEAM_A}'"),
        );
        passes(
            &db,
            &format!("UPDATE matches SET competition_id = '{COMPETITION}'"),
        );
    }

    /// The legacy id and its team go together, and they are unique.
    #[test]
    fn the_legacy_id_of_a_match() {
        let db = database();
        fails(&db, "UPDATE matches SET legacy_id = '2025-10-04_x'");
        fails(&db, &format!("UPDATE matches SET legacy_team = '{TEAM_A}'"));
        passes(
            &db,
            &format!("UPDATE matches SET legacy_id = '2025-10-04_x', legacy_team = '{TEAM_A}'"),
        );
        let other = |id: &str, team: &str| {
            format!(
                "INSERT INTO matches (id, team_a, team_b_name, start, max_substitutions, legacy_id, legacy_team)
                 VALUES ('{id}', '{TEAM_A}', 'X', 's', 6, '2025-10-04_x', '{team}')"
            )
        };
        fails(&db, &other("11111111-0000-4000-8000-000000000000", TEAM_A));
        // the same folder name in another team
        passes(&db, &other("11111111-0000-4000-8000-000000000000", TEAM_B));
    }

    /// D9.
    #[test]
    fn the_number_is_unique_in_the_roster_of_a_team() {
        let db = database();
        fails(
            &db,
            &format!(
                "UPDATE match_roster SET number = 2 WHERE person_id = '{}'",
                person(1)
            ),
        );
        // the same number on the other side: fine
        passes(
            &db,
            &format!(
                "UPDATE match_roster SET number = 1 WHERE person_id = '{}'",
                person(11)
            ),
        );
        // and a number other than the team's
        passes(
            &db,
            &format!(
                "UPDATE match_roster SET number = 42 WHERE person_id = '{}'",
                person(2)
            ),
        );
        fails(
            &db,
            &format!(
                "INSERT INTO match_roster (match_id, team_id, person_id, number) VALUES ('{MATCH}', '{TEAM_A}', '{}', 50)",
                person(1)
            ),
        );
    }

    #[test]
    fn a_roster_is_of_a_team_of_the_match() {
        let db = database();
        let insert = format!(
            "INSERT INTO match_roster (match_id, team_id, person_id, number) VALUES ('{MATCH}', '{TEAM_C}', '{}', 50)",
            person(1)
        );
        fails(&db, &insert);
        fails(
            &db,
            &format!(
                "UPDATE match_roster SET team_id = '{TEAM_C}' WHERE person_id = '{}'",
                person(1)
            ),
        );
        fails(
            &db,
            &format!(
                "INSERT INTO match_roster (match_id, team_id, person_id, number) VALUES ('{TEAM_A}', '{TEAM_A}', '{}', 50)",
                person(1)
            ),
        );
        fails(
            &db,
            &format!(
                "INSERT INTO match_roster (match_id, team_id, person_id, number) VALUES ('{MATCH}', '{TEAM_A}', '{}', 50)",
                person(99)
            ),
        );
        // C plays it as B: then yes
        passes(&db, &format!("DELETE FROM set_lineups WHERE side = 'b'; DELETE FROM match_roster WHERE team_id = '{TEAM_B}';
                              UPDATE matches SET team_b = '{TEAM_C}'"));
        passes(&db, &insert);
    }

    #[test]
    fn a_set_is_of_a_match_once() {
        let db = database();
        fails(
            &db,
            &format!("INSERT INTO sets (match_id, number, serving) VALUES ('{MATCH}', 1, 'b')"),
        );
        fails(
            &db,
            &format!("INSERT INTO sets (match_id, number, serving) VALUES ('{MATCH}', 6, 'b')"),
        );
        fails(
            &db,
            &format!("INSERT INTO sets (match_id, number, serving) VALUES ('{MATCH}', 0, 'b')"),
        );
        fails(
            &db,
            &format!("INSERT INTO sets (match_id, number, serving) VALUES ('{TEAM_A}', 2, 'b')"),
        );
        passes(
            &db,
            &format!("INSERT INTO sets (match_id, number, serving) VALUES ('{MATCH}', 5, 'b')"),
        );
    }

    #[test]
    fn a_lineup_has_six_different_players_and_its_libero_off_court() {
        let db = database();
        let p: Vec<String> = (1..=9).map(person).collect();
        let court = |a: usize, b: usize, c: usize, d: usize, e: usize, f: usize| {
            [&p[a], &p[b], &p[c], &p[d], &p[e], &p[f]]
        };
        assert!(lineup(&db, "a", court(0, 1, 2, 3, 4, 5), &p[6], Some(&p[7]), &p[3]).is_ok());
        assert!(lineup(&db, "a", court(0, 1, 2, 3, 4, 5), &p[6], None, &p[0]).is_ok());
        // a player twice, in every pair of positions
        for i in 0..6 {
            for j in i + 1..6 {
                let mut positions = [0, 1, 2, 3, 4, 5];
                positions[j] = positions[i];
                let [a, b, c, d, e, f] = positions;
                assert!(
                    lineup(&db, "a", court(a, b, c, d, e, f), &p[6], None, &p[0]).is_err(),
                    "{i} {j}"
                );
            }
        }
        // the libero on court
        assert!(lineup(&db, "a", court(0, 1, 2, 3, 4, 5), &p[5], None, &p[0]).is_err());
        // the fallback libero is the libero, or on court
        assert!(lineup(&db, "a", court(0, 1, 2, 3, 4, 5), &p[6], Some(&p[6]), &p[0]).is_err());
        assert!(lineup(&db, "a", court(0, 1, 2, 3, 4, 5), &p[6], Some(&p[2]), &p[0]).is_err());
        // the setter off court
        assert!(lineup(&db, "a", court(0, 1, 2, 3, 4, 5), &p[6], None, &p[6]).is_err());
        assert!(lineup(&db, "a", court(0, 1, 2, 3, 4, 5), &p[6], None, &p[8]).is_err());
        // someone who isn't a person
        let nobody = person(99);
        assert!(lineup(&db, "a", court(0, 1, 2, 3, 4, 5), &nobody, None, &p[0]).is_err());
    }

    #[test]
    fn a_lineup_is_of_a_set_of_the_match() {
        let db = database();
        // a set the match doesn't have
        fails(
            &db,
            "INSERT INTO set_lineups SELECT match_id, 2, side, p1, p2, p3, p4, p5, p6, libero,
                 fallback_libero, setter FROM set_lineups WHERE side = 'a'",
        );
        // the side twice
        fails(&db, "INSERT INTO set_lineups SELECT match_id, set_number, side, p1, p2, p3, p4, p5, p6, libero,
                        fallback_libero, setter FROM set_lineups WHERE side = 'a'");
    }

    /// B's lineup only when both sides are scouted.
    #[test]
    fn side_b_has_a_lineup_only_when_both_are_scouted() {
        let db = database();
        passes(
            &db,
            "DELETE FROM set_lineups WHERE side = 'b'; UPDATE matches SET scouting = 'a'",
        );
        assert!(insert_lineup(&db, "b", 11).is_err());
        fails(&db, "UPDATE set_lineups SET side = 'b'");
        passes(&db, "UPDATE matches SET scouting = 'both'");
        assert!(insert_lineup(&db, "b", 11).is_ok());
    }

    /// D8: one row per touch, in order.
    #[test]
    fn the_events_of_a_set_are_in_order() {
        let db = database();
        // the same place twice
        assert!(insert_event(&db, "ffffffff-0000-4000-8000-000000000007", 0, None).is_err());
        assert!(insert_event(&db, "ffffffff-0000-4000-8000-000000000007", -1, None).is_err());
        // the same id twice
        assert!(insert_event(&db, EVENT, 1, None).is_err());
        assert!(insert_event(&db, "ffffffff-0000-4000-8000-000000000007", 1, None).is_ok());
        // an undone event keeps its place
        passes(
            &db,
            &format!("UPDATE events SET deleted = 1 WHERE id = '{EVENT}'"),
        );
        assert!(insert_event(&db, "ffffffff-0000-4000-8000-000000000008", 0, None).is_err());
        // of a set that exists, by a person
        fails(&db, &format!("INSERT INTO events (id, match_id, set_number, seq, side, timestamp, event_type)
                             VALUES ('ffffffff-0000-4000-8000-000000000009', '{MATCH}', 2, 0, 'a', 't', 'S')"));
        assert!(
            insert_event(
                &db,
                "ffffffff-0000-4000-8000-000000000009",
                2,
                Some(&person(99))
            )
            .is_err()
        );
        fails(
            &db,
            &format!("UPDATE events SET target_player = '{}'", person(99)),
        );
        fails(&db, "UPDATE events SET flags = 65536");
        fails(&db, "UPDATE events SET flags = -1");
        fails(&db, "UPDATE events SET event_type = NULL");
        // the flags of a later version are kept
        passes(&db, "UPDATE events SET flags = 65535");
    }

    #[test]
    fn the_video_of_a_match() {
        let db = database();
        passes(&db, &format!(
            "INSERT INTO match_videos (match_id, anchor_at, anchor_offset_ms, anchor_event)
                 VALUES ('{MATCH}', '2026-11-12T17:01:00Z', 312000, '{EVENT}');
             INSERT INTO match_video_files VALUES ('{MATCH}', 0, 'videos/a.mp4', '2026-11-12T18:25:00+01:00'),
                 ('{MATCH}', 1, '/Volumes/camera/b.mp4', NULL);
             INSERT INTO match_video_corrections VALUES ('{MATCH}', '{EVENT}', -1500);"
        ));
        fails(
            &db,
            &format!("INSERT INTO match_video_files VALUES ('{MATCH}', 1, 'c.mp4', NULL)"),
        );
        fails(
            &db,
            &format!("INSERT INTO match_video_files VALUES ('{MATCH}', -1, 'c.mp4', NULL)"),
        );
        fails(
            &db,
            &format!("INSERT INTO match_video_files VALUES ('{TEAM_A}', 2, 'c.mp4', NULL)"),
        );
        fails(&db, "UPDATE match_video_corrections SET by_ms = 0");
        // the anchor half given
        fails(&db, "UPDATE match_videos SET anchor_at = NULL");
        fails(&db, "UPDATE match_videos SET anchor_offset_ms = NULL");
        // the event chosen without the anchor
        fails(
            &db,
            "UPDATE match_videos SET anchor_at = NULL, anchor_offset_ms = NULL",
        );
        passes(
            &db,
            "UPDATE match_videos SET anchor_at = NULL, anchor_offset_ms = NULL, anchor_event = NULL",
        );
    }

    /// The video's anchor and corrections are events of its own match.
    #[test]
    fn the_video_refers_to_events_of_its_match() {
        let db = database();
        let other = "11111111-0000-4000-8000-000000000000";
        let other_event = "11111111-0000-4000-8000-000000000001";
        passes(
            &db,
            &format!(
                "INSERT INTO matches (id, team_a, team_b_name, start, max_substitutions)
                 VALUES ('{other}', '{TEAM_A}', 'X', 's', 6);
             INSERT INTO sets (match_id, number, serving) VALUES ('{other}', 1, 'a');
             INSERT INTO events (id, match_id, set_number, seq, side, timestamp, event_type)
                 VALUES ('{other_event}', '{other}', 1, 0, 'a', 't', 'S');
             INSERT INTO match_videos (match_id) VALUES ('{MATCH}');"
            ),
        );
        fails(
            &db,
            &format!("INSERT INTO match_video_corrections VALUES ('{MATCH}', '{other_event}', 10)"),
        );
        fails(
            &db,
            &format!(
                "UPDATE match_videos SET anchor_at = 't', anchor_offset_ms = 0, anchor_event = '{other_event}'"
            ),
        );
        fails(
            &db,
            &format!("INSERT INTO match_videos VALUES ('{other}', 't', 0, '{EVENT}')"),
        );
        passes(
            &db,
            &format!("INSERT INTO match_videos VALUES ('{other}', 't', 0, '{other_event}')"),
        );
        fails(
            &db,
            &format!(
                "INSERT INTO match_video_corrections VALUES ('{MATCH}', '{}', 10)",
                person(1)
            ),
        );
    }

    #[test]
    fn the_upload_queue_has_a_match_once() {
        let db = database();
        passes(
            &db,
            &format!(
                "INSERT INTO upload_queue (match_id, queued_at) VALUES ('{MATCH}', 1794509535)"
            ),
        );
        fails(
            &db,
            &format!(
                "INSERT INTO upload_queue (match_id, queued_at) VALUES ('{MATCH}', 1794509536)"
            ),
        );
        fails(
            &db,
            &format!("INSERT INTO upload_queue (match_id, queued_at) VALUES ('{TEAM_A}', 1)"),
        );
        fails(&db, "UPDATE upload_queue SET retry_count = -1");
        let retries: i64 = db
            .query_row("SELECT retry_count FROM upload_queue", [], |r| r.get(0))
            .unwrap();
        assert_eq!(retries, 0);
    }

    /// D7: a cache, one row per match, from A's side.
    #[test]
    fn the_summary_is_a_cache_of_a_match() {
        let db = database();
        passes(
            &db,
            &format!("INSERT INTO match_summary VALUES ('{MATCH}', 5, 3, 2, 1, NULL, NULL)"),
        );
        fails(
            &db,
            &format!("INSERT INTO match_summary VALUES ('{MATCH}', 1, 0, 0, 0, 1, NULL)"),
        );
        fails(
            &db,
            &format!("INSERT INTO match_summary VALUES ('{TEAM_A}', 1, 0, 0, 0, 1, NULL)"),
        );
        fails(&db, "UPDATE match_summary SET sets = 6");
        fails(&db, "UPDATE match_summary SET a_wins = 4");
        fails(&db, "UPDATE match_summary SET b_wins = -1");
        fails(&db, "UPDATE match_summary SET finished = 2");
        fails(&db, "UPDATE match_summary SET resume = 0");
        fails(&db, "UPDATE match_summary SET next_set_number = 6");
    }

    /// Rows are referred to, not removed (D7): removing a referred row fails.
    #[test]
    fn a_referred_row_is_not_removed() {
        let db = database();
        fails(
            &db,
            &format!("DELETE FROM persons WHERE id = '{}'", person(1)),
        );
        fails(&db, &format!("DELETE FROM teams WHERE id = '{TEAM_B}'"));
        fails(&db, &format!("DELETE FROM matches WHERE id = '{MATCH}'"));
        fails(&db, &format!("DELETE FROM sets WHERE match_id = '{MATCH}'"));
        passes(
            &db,
            &format!("UPDATE matches SET competition_id = '{COMPETITION}'"),
        );
        fails(&db, "DELETE FROM competitions");
        // the summary is not a reference that holds the match
        passes(&db, &format!(
            "DELETE FROM events; DELETE FROM set_lineups; DELETE FROM sets; DELETE FROM match_roster;
             INSERT INTO match_summary VALUES ('{MATCH}', 0, 0, 0, 0, NULL, 1);
             DELETE FROM matches WHERE id = '{MATCH}';"
        ));
        let rows: i64 = db
            .query_row("SELECT count(*) FROM match_summary", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
    }
}
