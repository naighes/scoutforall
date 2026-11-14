//! The SQLite built into the binary (`rusqlite` with `bundled`): the same
//! engine on every platform, with what the database of the data will need
//! (TODO/001, D6): strict tables, foreign keys, `RETURNING`, JSON, WAL on a
//! file.

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};

    /// The bundled one, not the one of the system: a recent version, the same
    /// everywhere (macOS and Android ship older ones).
    #[test]
    fn the_engine_is_the_bundled_one() {
        // the library that runs is the one of the headers it was built with
        assert_eq!(
            rusqlite::version_number(),
            rusqlite::ffi::SQLITE_VERSION_NUMBER
        );
        assert!(rusqlite::version_number() >= 3_054_000);
    }

    #[test]
    fn strict_tables_reject_a_wrong_type() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE t (n INTEGER NOT NULL) STRICT;")
            .unwrap();
        assert!(db.execute("INSERT INTO t (n) VALUES ('x')", []).is_err());
        db.execute("INSERT INTO t (n) VALUES (?1)", params![3])
            .unwrap();
    }

    #[test]
    fn foreign_keys_are_enforced_when_turned_on() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE teams (id TEXT PRIMARY KEY) STRICT;
             CREATE TABLE matches (
                 id TEXT PRIMARY KEY,
                 team_a TEXT NOT NULL REFERENCES teams (id)
             ) STRICT;",
        )
        .unwrap();
        assert!(
            db.execute("INSERT INTO matches VALUES ('m', 'missing')", [])
                .is_err()
        );
        db.execute("INSERT INTO teams VALUES ('t')", []).unwrap();
        db.execute("INSERT INTO matches VALUES ('m', 't')", [])
            .unwrap();
    }

    #[test]
    fn returning_and_json_work() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, j TEXT) STRICT;")
            .unwrap();
        let id: i64 = db
            .query_row(
                "INSERT INTO t (j) VALUES (json_object('a', 1)) RETURNING id",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let a: i64 = db
            .query_row("SELECT j ->> '$.a' FROM t WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(a, 1);
    }

    /// A transaction not committed leaves nothing, and a file in WAL mode
    /// reopens with what was committed.
    #[test]
    fn a_file_in_wal_mode_keeps_only_what_was_committed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite");
        {
            let mut db = Connection::open(&path).unwrap();
            let mode: String = db
                .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
                .unwrap();
            assert_eq!(mode, "wal");
            db.execute_batch("CREATE TABLE t (n INTEGER) STRICT;")
                .unwrap();
            let tx = db.transaction().unwrap();
            tx.execute("INSERT INTO t VALUES (1)", []).unwrap();
            tx.commit().unwrap();
            let tx = db.transaction().unwrap();
            tx.execute("INSERT INTO t VALUES (2)", []).unwrap();
            drop(tx);
        }
        let db = Connection::open(&path).unwrap();
        let ns: Vec<i64> = db
            .prepare("SELECT n FROM t")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(ns, vec![1]);
    }
}
