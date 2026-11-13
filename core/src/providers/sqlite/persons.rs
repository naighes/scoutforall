//! The registry of the persons (D5), apart from the teams.

use crate::{
    domain::{Person, sql::Sql},
    errors::AppError,
    providers::sqlite::{Database, not_found, sqlite},
    shapes::enums::RoleEnum,
};
use chrono::NaiveDate;
use rusqlite::Transaction;
use uuid::Uuid;

pub struct SqlitePersons(Database);

/// A team a person plays in, with the number and the role they have there.
#[derive(Debug, Clone, PartialEq)]
pub struct PersonTeam {
    pub team: Uuid,
    pub name: String,
    pub year: u16,
    pub number: u8,
    pub role: Option<RoleEnum>,
}

/// A person of the registry that can be added to a team, with the teams
/// they play in (the most recent first): none, when every team of theirs
/// was removed or removed them.
#[derive(Debug, Clone, PartialEq)]
pub struct PersonChoice {
    pub person: Person,
    pub teams: Vec<PersonTeam>,
}

impl SqlitePersons {
    pub fn new(db: Database) -> Self {
        Self(db)
    }

    /// The persons not removed from the registry that aren't players of
    /// `team` (removed from it, they are), by name.
    pub fn choices(&self, team: &Uuid) -> Result<Vec<PersonChoice>, AppError> {
        self.0.read(|tx| {
            let persons: Vec<Person> = tx
                .prepare(
                    "SELECT id, name, birth_date, height_cm, notes FROM persons p
                     WHERE deleted = 0 AND NOT EXISTS (
                         SELECT 1 FROM team_members
                         WHERE team_id = ?1 AND person_id = p.id AND deleted = 0)
                     ORDER BY name COLLATE NOCASE, id",
                )
                .and_then(|mut statement| {
                    statement
                        .query_map([Sql(*team)], |r| {
                            Ok(Person {
                                id: r.get::<_, Sql<Uuid>>(0)?.0,
                                name: r.get(1)?,
                                birth_date: r.get::<_, Option<Sql<NaiveDate>>>(2)?.map(|d| d.0),
                                height_cm: r.get(3)?,
                                notes: r.get(4)?,
                                deleted: false,
                            })
                        })?
                        .collect::<Result<_, _>>()
                })
                .map_err(sqlite)?;
            persons
                .into_iter()
                .map(|person| {
                    Ok(PersonChoice {
                        teams: teams_of(tx, person.id)?,
                        person,
                    })
                })
                .collect()
        })
    }

    /// The teams not removed the person plays in, the most recent first.
    pub fn teams_of(&self, person_id: &Uuid) -> Result<Vec<PersonTeam>, AppError> {
        self.0.read(|tx| teams_of(tx, *person_id))
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

/// The teams not removed `person` is an active member of, the most recent
/// first, then by name.
fn teams_of(tx: &Transaction, person: Uuid) -> Result<Vec<PersonTeam>, AppError> {
    tx.prepare(
        "SELECT t.id, t.name, t.year, tm.number, tm.role
         FROM team_members tm JOIN teams t ON t.id = tm.team_id
         WHERE tm.person_id = ?1 AND tm.deleted = 0 AND t.deleted = 0
         ORDER BY t.year DESC, t.name COLLATE NOCASE, t.id",
    )
    .and_then(|mut statement| {
        statement
            .query_map([Sql(person)], |r| {
                Ok(PersonTeam {
                    team: r.get::<_, Sql<Uuid>>(0)?.0,
                    name: r.get(1)?,
                    year: r.get(2)?,
                    number: r.get(3)?,
                    role: r.get(4)?,
                })
            })?
            .collect::<Result<_, _>>()
    })
    .map_err(sqlite)
}
