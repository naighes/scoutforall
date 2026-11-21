//! The training of a team in the database (TODO/021). That a session is
//! inside its microcycle and doesn't overlap another is for the core to
//! check, with messages that name the other one.

use crate::{
    domain::sql::Sql,
    errors::{AppError, TrainingError},
    providers::{
        sqlite::{Database, not_found, sqlite},
        training_reader::TrainingReader,
        training_writer::TrainingWriter,
    },
    training::{
        SessionInput, TrainingSession,
        overlap::{Slot, clashes},
    },
};
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDateTime};
use rusqlite::{Row, Transaction, params};
use uuid::Uuid;

pub struct SqliteTraining(Database);

impl SqliteTraining {
    pub fn new(db: Database) -> Self {
        Self(db)
    }
}

const SESSION_COLUMNS: &str = "id, team_id, microcycle_id, series_id, start_local, duration_min,
     actual_duration_min, location, title, objectives, notes, final_remarks, status";

fn session_from_row(row: &Row) -> rusqlite::Result<TrainingSession> {
    Ok(TrainingSession {
        id: row.get::<_, Sql<Uuid>>(0)?.0,
        team_id: row.get::<_, Sql<Uuid>>(1)?.0,
        microcycle_id: row.get::<_, Option<Sql<Uuid>>>(2)?.map(|id| id.0),
        series_id: row.get::<_, Option<Sql<Uuid>>>(3)?.map(|id| id.0),
        start: row.get::<_, Sql<NaiveDateTime>>(4)?.0,
        duration_min: row.get(5)?,
        actual_duration_min: row.get(6)?,
        location: row.get(7)?,
        title: row.get(8)?,
        objectives: row.get(9)?,
        notes: row.get(10)?,
        final_remarks: row.get(11)?,
        status: row.get(12)?,
    })
}

/// The session not removed, or an error.
pub(crate) fn read_session(tx: &Transaction, id: &Uuid) -> Result<TrainingSession, AppError> {
    tx.query_row(
        &format!("SELECT {SESSION_COLUMNS} FROM training_sessions WHERE id = ?1 AND deleted = 0"),
        [Sql(*id)],
        session_from_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => not_found(&format!("session {id}")),
        e => sqlite(e),
    })
}

/// The sessions of the team not removed, but `except`: what a new or edited
/// session, or a match, mustn't overlap.
fn session_slots(
    tx: &Transaction,
    team_id: &Uuid,
    except: Option<&Uuid>,
) -> Result<Vec<Slot>, AppError> {
    tx.prepare(
        "SELECT start_local, duration_min FROM training_sessions
         WHERE team_id = ?1 AND deleted = 0 AND (?2 IS NULL OR id <> ?2)",
    )
    .map_err(sqlite)?
    .query_map(params![Sql(*team_id), except.map(|id| Sql(*id))], |row| {
        Ok(Slot::session(
            row.get::<_, Sql<NaiveDateTime>>(0)?.0,
            row.get(1)?,
        ))
    })
    .map_err(sqlite)?
    .collect::<Result<_, _>>()
    .map_err(sqlite)
}

/// The matches in programma of the team, as A or as B: not removed, without
/// a set. Their time is the clock time of the offset they were written with.
fn match_slots(
    tx: &Transaction,
    team_id: &Uuid,
    except: Option<&Uuid>,
) -> Result<Vec<Slot>, AppError> {
    tx.prepare(
        "SELECT m.start, m.duration_min FROM matches m
         WHERE (m.team_a = ?1 OR m.team_b = ?1) AND m.deleted = 0
           AND (?2 IS NULL OR m.id <> ?2)
           AND NOT EXISTS (SELECT 1 FROM sets s WHERE s.match_id = m.id AND s.deleted = 0)",
    )
    .map_err(sqlite)?
    .query_map(params![Sql(*team_id), except.map(|id| Sql(*id))], |row| {
        Ok(Slot::match_of(
            row.get::<_, Sql<DateTime<FixedOffset>>>(0)?.0.naive_local(),
            row.get(1)?,
        ))
    })
    .map_err(sqlite)?
    .collect::<Result<_, _>>()
    .map_err(sqlite)
}

fn overlap(slot: &Slot, others: Vec<Slot>) -> Result<(), AppError> {
    match clashes(slot, &others) {
        found if found.is_empty() => Ok(()),
        found => Err(TrainingError::Overlap(found).into()),
    }
}

/// A session of the team in this window overlaps no other session of the
/// team, nor a match in programma of it.
fn check_session(
    tx: &Transaction,
    team_id: &Uuid,
    except: Option<&Uuid>,
    start: NaiveDateTime,
    minutes: u16,
) -> Result<(), AppError> {
    let mut busy = session_slots(tx, team_id, except)?;
    busy.extend(match_slots(tx, team_id, None)?);
    overlap(&Slot::session(start, minutes), busy)
}

/// A match in programma of these teams (A, and B when a team) at `start`
/// overlaps no session of either. The other direction of `check_session`.
pub(crate) fn check_match(
    tx: &Transaction,
    teams: &[Uuid],
    start: DateTime<FixedOffset>,
    minutes: Option<u16>,
) -> Result<(), AppError> {
    let slot = Slot::match_of(start.naive_local(), minutes);
    teams
        .iter()
        .try_for_each(|team| overlap(&slot, session_slots(tx, team, None)?))
}

#[async_trait]
impl TrainingReader for SqliteTraining {
    async fn sessions(&self, team_id: &Uuid) -> Result<Vec<TrainingSession>, AppError> {
        self.0.read(|tx| {
            tx.prepare(&format!(
                "SELECT {SESSION_COLUMNS} FROM training_sessions
                 WHERE team_id = ?1 AND deleted = 0 ORDER BY start_local, id"
            ))
            .map_err(sqlite)?
            .query_map([Sql(*team_id)], session_from_row)
            .map_err(sqlite)?
            .collect::<Result<_, _>>()
            .map_err(sqlite)
        })
    }

    async fn session(&self, session_id: &Uuid) -> Result<TrainingSession, AppError> {
        self.0.read(|tx| read_session(tx, session_id))
    }

    async fn locations(&self, team_id: &Uuid) -> Result<Vec<String>, AppError> {
        self.0.read(|tx| {
            tx.prepare(
                "SELECT location FROM training_sessions
                 WHERE team_id = ?1 AND deleted = 0 AND location <> ''
                 GROUP BY location ORDER BY MAX(start_local) DESC, location",
            )
            .map_err(sqlite)?
            .query_map([Sql(*team_id)], |row| row.get(0))
            .map_err(sqlite)?
            .collect::<Result<_, _>>()
            .map_err(sqlite)
        })
    }
}

#[async_trait]
impl TrainingWriter for SqliteTraining {
    async fn create_session(
        &self,
        team_id: &Uuid,
        input: SessionInput,
    ) -> Result<TrainingSession, AppError> {
        self.0.write(|tx| {
            let team: bool = tx
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM teams WHERE id = ?1 AND deleted = 0)",
                    [Sql(*team_id)],
                    |row| row.get(0),
                )
                .map_err(sqlite)?;
            match team {
                true => {
                    check_session(tx, team_id, None, input.start, input.duration_min)?;
                    let id = Uuid::new_v4();
                    tx.execute(
                        "INSERT INTO training_sessions (id, team_id, microcycle_id, start_local,
                             duration_min, actual_duration_min, location, title, objectives,
                             notes, final_remarks, status)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                        params![
                            Sql(id),
                            Sql(*team_id),
                            input.microcycle_id.map(Sql),
                            Sql(input.start),
                            input.duration_min,
                            input.actual_duration_min,
                            input.location,
                            input.title,
                            input.objectives,
                            input.notes,
                            input.final_remarks,
                            input.status,
                        ],
                    )
                    .map_err(sqlite)?;
                    read_session(tx, &id)
                }
                false => Err(not_found(&format!("team {team_id}"))),
            }
        })
    }

    async fn update_session(
        &self,
        session_id: &Uuid,
        input: SessionInput,
    ) -> Result<TrainingSession, AppError> {
        self.0.write(|tx| {
            let session = read_session(tx, session_id)?;
            check_session(
                tx,
                &session.team_id,
                Some(session_id),
                input.start,
                input.duration_min,
            )?;
            let changed = tx
                .execute(
                    "UPDATE training_sessions SET microcycle_id = ?2, start_local = ?3,
                         duration_min = ?4, actual_duration_min = ?5, location = ?6, title = ?7,
                         objectives = ?8, notes = ?9, final_remarks = ?10, status = ?11
                     WHERE id = ?1 AND deleted = 0",
                    params![
                        Sql(*session_id),
                        input.microcycle_id.map(Sql),
                        Sql(input.start),
                        input.duration_min,
                        input.actual_duration_min,
                        input.location,
                        input.title,
                        input.objectives,
                        input.notes,
                        input.final_remarks,
                        input.status,
                    ],
                )
                .map_err(sqlite)?;
            match changed {
                0 => Err(not_found(&format!("session {session_id}"))),
                _ => read_session(tx, session_id),
            }
        })
    }

    async fn remove_session(&self, session_id: &Uuid) -> Result<(), AppError> {
        self.0.write(|tx| {
            match tx
                .execute(
                    "UPDATE training_sessions SET deleted = 1 WHERE id = ?1 AND deleted = 0",
                    [Sql(*session_id)],
                )
                .map_err(sqlite)?
            {
                0 => Err(not_found(&format!("session {session_id}"))),
                _ => Ok(()),
            }
        })
    }
}
