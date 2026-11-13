//! The registry of the persons (D5), apart from the teams.

use crate::{
    domain::sql::Sql,
    errors::AppError,
    providers::sqlite::{Database, not_found, sqlite},
};
use uuid::Uuid;

pub struct SqlitePersons(Database);

impl SqlitePersons {
    pub fn new(db: Database) -> Self {
        Self(db)
    }

    /// Removed from the registry (D16), so from every team: the row keeps
    /// only its id and the flag, no personal details (minors, 017). The
    /// matches played keep the person as an anonymous player, with the
    /// number of their roster: what the teams did doesn't change.
    pub fn remove(&self, person_id: &Uuid) -> Result<(), AppError> {
        let id = Sql(*person_id);
        self.0.write(|tx| {
            let found = tx
                .execute(
                    "UPDATE persons SET deleted = 1, name = '', birth_date = NULL,
                         height_cm = NULL, notes = ''
                     WHERE id = ?1",
                    [id],
                )
                .map_err(sqlite)?;
            match found {
                0 => Err(not_found(&format!("person {person_id}"))),
                _ => Ok(()),
            }?;
            tx.execute(
                "UPDATE team_members SET deleted = 1 WHERE person_id = ?1",
                [id],
            )
            // what the lists show doesn't come from the persons, but the
            // matches that tell of them are read again, as a write of their
            // team does
            .and_then(|_| {
                tx.execute(
                    "DELETE FROM match_summary WHERE match_id IN (
                         SELECT match_id FROM match_roster WHERE person_id = ?1
                         UNION SELECT match_id FROM set_lineups
                               WHERE ?1 IN (p1, p2, p3, p4, p5, p6, libero,
                                            fallback_libero, setter)
                         UNION SELECT match_id FROM events WHERE ?1 IN (player, target_player))",
                    [id],
                )
            })
            .map(|_| ())
            .map_err(sqlite)
        })
    }
}
