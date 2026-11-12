//! The module `db`: how the database opens (the settings of every
//! connection, WAL on a file) and how its schema migrates (`user_version`,
//! one transaction per migration, a newer database left alone).

#[cfg(test)]
mod tests {
    use crate::{
        db::{self, BUSY_TIMEOUT, MIGRATIONS},
        errors::{AppError, IOError},
        localization::labels,
        shapes::enums::LanguageEnum,
    };
    use rusqlite::Connection;
    use std::path::Path;

    const PEOPLE: &str = "CREATE TABLE people (id TEXT PRIMARY KEY, name TEXT NOT NULL) STRICT;";
    const TEAMS: &str = "CREATE TABLE teams (
             id TEXT PRIMARY KEY,
             coach TEXT REFERENCES people (id)
         ) STRICT;";
    const NOTES: &str = "ALTER TABLE people ADD COLUMN notes TEXT;";

    fn pragma(db: &Connection, name: &str) -> String {
        db.query_row(&format!("PRAGMA {name}"), [], |row| {
            row.get::<_, rusqlite::types::Value>(0)
        })
        .map(|value| match value {
            rusqlite::types::Value::Integer(n) => n.to_string(),
            rusqlite::types::Value::Text(s) => s,
            other => format!("{other:?}"),
        })
        .unwrap()
    }

    fn tables(db: &Connection) -> Vec<String> {
        db.prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn version_in_file(path: &Path) -> i64 {
        // a bare connection: what is in the file, not what `open` makes of it
        db::user_version(&Connection::open(path).unwrap()).unwrap()
    }

    /// The settings SQLite doesn't keep in the file, on every connection.
    /// The foreign keys are off by default in SQLite, but the bundled one is
    /// built with `SQLITE_DEFAULT_FOREIGN_KEYS=1` (libsqlite3-sys): `open`
    /// turns them on anyway, for a build on another SQLite.
    #[test]
    fn every_connection_is_set_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite");
        drop(db::open(&path).unwrap());
        let db = db::open(&path).unwrap();
        assert_eq!(pragma(&db, "foreign_keys"), "1");
        assert_eq!(pragma(&db, "journal_mode"), "wal");
        // NORMAL
        assert_eq!(pragma(&db, "synchronous"), "1");
        assert_eq!(
            pragma(&db, "busy_timeout"),
            BUSY_TIMEOUT.as_millis().to_string()
        );
        let memory = db::open_in_memory().unwrap();
        assert_eq!(pragma(&memory, "foreign_keys"), "1");
        assert_eq!(pragma(&memory, "journal_mode"), "memory");
    }

    #[test]
    fn a_foreign_key_is_enforced_after_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite");
        drop(db::open_with(&path, &[PEOPLE, TEAMS]).unwrap());
        let db = db::open_with(&path, &[PEOPLE, TEAMS]).unwrap();
        assert!(
            db.execute("INSERT INTO teams VALUES ('t', 'nobody')", [])
                .is_err()
        );
    }

    /// The schema of this version: a new database has it all.
    #[test]
    fn a_new_database_has_the_schema_of_this_version() {
        assert_eq!(db::schema_version(), MIGRATIONS.len() as i64);
        let memory = db::open_in_memory().unwrap();
        assert_eq!(db::user_version(&memory).unwrap(), db::schema_version());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite");
        drop(db::open(&path).unwrap());
        assert_eq!(version_in_file(&path), db::schema_version());
        // and opening it again changes nothing
        let mut db = db::open(&path).unwrap();
        assert_eq!(db::migrate(&mut db, MIGRATIONS).unwrap(), Vec::<i64>::new());
    }

    #[test]
    fn the_migrations_run_in_order_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite");
        let mut db = db::open_with(&path, &[]).unwrap();
        assert_eq!(db::user_version(&db).unwrap(), 0);
        assert_eq!(db::migrate(&mut db, &[PEOPLE, TEAMS]).unwrap(), vec![1, 2]);
        assert_eq!(tables(&db), vec!["people", "teams"]);
        db.execute("INSERT INTO people (id, name) VALUES ('p', 'Ann')", [])
            .unwrap();
        drop(db);
        // the same migrations again: nothing to do, the data stay
        let mut db = db::open_with(&path, &[PEOPLE, TEAMS]).unwrap();
        assert_eq!(
            db::migrate(&mut db, &[PEOPLE, TEAMS]).unwrap(),
            Vec::<i64>::new()
        );
        // a later version adds one: only that one runs
        assert_eq!(
            db::migrate(&mut db, &[PEOPLE, TEAMS, NOTES]).unwrap(),
            vec![3]
        );
        let (name, notes): (String, Option<String>) = db
            .query_row("SELECT name, notes FROM people", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!((name.as_str(), notes), ("Ann", None));
        drop(db);
        assert_eq!(version_in_file(&path), 3);
    }

    /// A migration that fails leaves the database at the version before it,
    /// with nothing of the migration (even its statements that worked); the
    /// ones before it stay.
    #[test]
    fn a_migration_that_fails_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite");
        let broken = "CREATE TABLE teams (id TEXT PRIMARY KEY) STRICT;
                      INSERT INTO teams VALUES ('t');
                      INSERT INTO nowhere VALUES (1);";
        let error = db::open_with(&path, &[PEOPLE, broken]).unwrap_err();
        assert!(
            matches!(error, AppError::IO(IOError::Sqlite(_))),
            "{error:?}"
        );
        assert_eq!(version_in_file(&path), 1);
        assert_eq!(tables(&Connection::open(&path).unwrap()), vec!["people"]);
        // fixed in a later build, it runs
        let db = db::open_with(&path, &[PEOPLE, TEAMS]).unwrap();
        assert_eq!(db::user_version(&db).unwrap(), 2);
        assert_eq!(tables(&db), vec!["people", "teams"]);
    }

    /// A newer app wrote the database: this one doesn't touch it, and says
    /// to update.
    #[test]
    fn a_newer_database_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite");
        drop(db::open_with(&path, &[PEOPLE, TEAMS, NOTES]).unwrap());
        let error = db::open_with(&path, &[PEOPLE, TEAMS]).unwrap_err();
        assert!(
            matches!(
                error,
                AppError::IO(IOError::DatabaseNewer { found: 3, known: 2 })
            ),
            "{error:?}"
        );
        assert_eq!(
            error.message(labels(LanguageEnum::En)),
            "the data were saved by a newer version of the app: update it"
        );
        assert_eq!(version_in_file(&path), 3);
        assert_eq!(
            tables(&Connection::open(&path).unwrap()),
            vec!["people", "teams"]
        );
        // the newer app still opens it
        assert!(db::open_with(&path, &[PEOPLE, TEAMS, NOTES]).is_ok());
    }

    #[test]
    fn a_negative_version_is_an_error() {
        let mut db = db::open_in_memory_with(&[]).unwrap();
        db.pragma_update(None, "user_version", -1).unwrap();
        assert!(db::migrate(&mut db, &[PEOPLE]).is_err());
        assert!(tables(&db).is_empty());
    }

    /// Two apps on the same file (the desktop and a script, or two windows):
    /// the second finds the schema already migrated.
    #[test]
    fn a_database_migrated_by_another_connection_is_not_migrated_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite");
        let mut first = db::open_with(&path, &[]).unwrap();
        let second = db::open_with(&path, &[PEOPLE, TEAMS]).unwrap();
        assert_eq!(
            db::migrate(&mut first, &[PEOPLE, TEAMS]).unwrap(),
            Vec::<i64>::new()
        );
        assert_eq!(tables(&second), vec!["people", "teams"]);
    }

    #[test]
    fn a_file_that_is_not_a_database_is_an_invalid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite");
        std::fs::write(&path, b"not a database, just some text long enough").unwrap();
        let error = db::open(&path).unwrap_err();
        assert!(
            error
                .message(labels(LanguageEnum::It))
                .starts_with("il file non è valido: "),
            "{error:?}"
        );
        // and it stays as it was
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"not a database, just some text long enough"
        );
    }

    #[test]
    fn a_folder_that_does_not_exist_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing").join("data.sqlite");
        assert!(matches!(
            db::open(&path),
            Err(AppError::IO(IOError::Sqlite(_)))
        ));
    }
}
