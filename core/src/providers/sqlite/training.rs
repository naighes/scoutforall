//! The training of a team in the database (TODO/021). That a session is
//! inside its microcycle and doesn't overlap another is for the core to
//! check, with messages that name the other one.

use crate::{
    domain::sql::Sql,
    errors::{AppError, IOError, TrainingError},
    providers::{
        sqlite::{Database, not_found, sqlite},
        training_reader::TrainingReader,
        training_writer::TrainingWriter,
    },
    training::{
        Activity, ActivityHit, ActivityInput, ActivityStatus, AttendanceInput, AttendanceRow,
        AttendanceSheet, Move, SessionInput, SessionStatus, TrainingSession,
        duplicate::DuplicateInput,
        overlap::{Slot, clashes},
        series::{self, SeriesEdit, SeriesInput, SeriesOutcome, SeriesScope},
    },
};
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDateTime};
use rusqlite::{Row, Transaction, params};
use std::collections::HashSet;
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

const ACTIVITY_COLUMNS: &str =
    "id, session_id, position, name, description, planned_min, actual_min, notes, status";

fn activity_from_row(row: &Row) -> rusqlite::Result<Activity> {
    Ok(Activity {
        id: row.get::<_, Sql<Uuid>>(0)?.0,
        session_id: row.get::<_, Sql<Uuid>>(1)?.0,
        position: row.get(2)?,
        name: row.get(3)?,
        description: row.get(4)?,
        planned_min: row.get(5)?,
        actual_min: row.get(6)?,
        notes: row.get(7)?,
        status: row.get(8)?,
    })
}

/// The activity not removed, of a session not removed, or an error.
fn read_activity(tx: &Transaction, id: &Uuid) -> Result<Activity, AppError> {
    tx.query_row(
        &format!(
            "SELECT {ACTIVITY_COLUMNS} FROM training_activities
             WHERE id = ?1 AND deleted = 0
               AND session_id IN (SELECT id FROM training_sessions WHERE deleted = 0)"
        ),
        [Sql(*id)],
        activity_from_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => not_found(&format!("activity {id}")),
        e => sqlite(e),
    })
}

fn attendance_from_row(row: &Row) -> rusqlite::Result<AttendanceRow> {
    Ok(AttendanceRow {
        person_id: row.get::<_, Sql<Uuid>>(0)?.0,
        name: row.get(1)?,
        number: row.get(2)?,
        status: row.get(3)?,
        note: row.get(4)?,
        guest: row.get::<_, u8>(5)? == 1,
    })
}

/// The active players of the team, in the order of the team, all present;
/// with `unlisted` only those the saved attendance of the session lacks.
fn roster_rows(
    tx: &Transaction,
    team_id: &Uuid,
    session_id: &Uuid,
    unlisted: bool,
) -> Result<Vec<AttendanceRow>, AppError> {
    tx.prepare(
        "SELECT p.id, p.name, tm.number, 'present', '', 0
         FROM team_members tm JOIN persons p ON p.id = tm.person_id
         WHERE tm.team_id = ?1 AND tm.deleted = 0 AND p.deleted = 0
           AND (?3 = 0 OR NOT EXISTS (
                SELECT 1 FROM training_attendance a
                WHERE a.session_id = ?2 AND a.person_id = p.id AND a.deleted = 0))
         ORDER BY tm.position",
    )
    .map_err(sqlite)?
    .query_map(
        params![Sql(*team_id), Sql(*session_id), unlisted],
        attendance_from_row,
    )
    .map_err(sqlite)?
    .collect::<Result<_, _>>()
    .map_err(sqlite)
}

/// The last place an activity can take: one is kept free to swap two.
const LAST_POSITION: u16 = 65534;

/// The session not removed, or an error.
fn write_status(
    tx: &Transaction,
    session_id: &Uuid,
    status: SessionStatus,
) -> Result<(), AppError> {
    tx.execute(
        "UPDATE training_sessions SET status = ?2 WHERE id = ?1 AND deleted = 0",
        params![Sql(*session_id), status],
    )
    .map(|_| ())
    .map_err(sqlite)
}

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

/// Writes a new session and reads it back.
fn insert_session(
    tx: &Transaction,
    team_id: &Uuid,
    series_id: Option<Uuid>,
    input: &SessionInput,
) -> Result<TrainingSession, AppError> {
    let id = Uuid::new_v4();
    tx.execute(
        "INSERT INTO training_sessions (id, team_id, microcycle_id, series_id, start_local,
             duration_min, actual_duration_min, location, title, objectives,
             notes, final_remarks, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            Sql(id),
            Sql(*team_id),
            input.microcycle_id.map(Sql),
            series_id.map(Sql),
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

/// The sessions not removed of the series of `anchor` the scope reaches
/// (only the planned ones), and the busy slots of its team: the other
/// sessions and the matches in programma.
fn series_targets(
    tx: &Transaction,
    session_id: &Uuid,
    scope: SeriesScope,
) -> Result<(Vec<TrainingSession>, Vec<Slot>), AppError> {
    let anchor = read_session(tx, session_id)?;
    let members: Vec<TrainingSession> = match anchor.series_id {
        Some(series) => tx
            .prepare(&format!(
                "SELECT {SESSION_COLUMNS} FROM training_sessions
                 WHERE series_id = ?1 AND deleted = 0 ORDER BY start_local, id"
            ))
            .map_err(sqlite)?
            .query_map([Sql(series)], session_from_row)
            .map_err(sqlite)?
            .collect::<Result<_, _>>()
            .map_err(sqlite)?,
        None => vec![],
    };
    let targets = series::select(scope, &anchor, &members);
    let ids: HashSet<Uuid> = targets.iter().map(|s| s.id).collect();
    let mut busy: Vec<Slot> = tx
        .prepare(
            "SELECT id, start_local, duration_min FROM training_sessions
             WHERE team_id = ?1 AND deleted = 0",
        )
        .map_err(sqlite)?
        .query_map([Sql(anchor.team_id)], |row| {
            Ok((
                row.get::<_, Sql<Uuid>>(0)?.0,
                Slot::session(row.get::<_, Sql<NaiveDateTime>>(1)?.0, row.get(2)?),
            ))
        })
        .map_err(sqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(sqlite)?
        .into_iter()
        .filter(|(id, _)| !ids.contains(id))
        .map(|(_, slot)| slot)
        .collect();
    busy.extend(match_slots(tx, &anchor.team_id, None)?);
    Ok((targets, busy))
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

    async fn activities(&self, session_id: &Uuid) -> Result<Vec<Activity>, AppError> {
        self.0.read(|tx| {
            tx.prepare(&format!(
                "SELECT {ACTIVITY_COLUMNS} FROM training_activities
                 WHERE session_id = ?1 AND deleted = 0
                   AND session_id IN (SELECT id FROM training_sessions WHERE deleted = 0)
                 ORDER BY position"
            ))
            .map_err(sqlite)?
            .query_map([Sql(*session_id)], activity_from_row)
            .map_err(sqlite)?
            .collect::<Result<_, _>>()
            .map_err(sqlite)
        })
    }

    async fn attendance(&self, session_id: &Uuid) -> Result<AttendanceSheet, AppError> {
        self.0.read(|tx| {
            let session = read_session(tx, session_id)?;
            let recorded: bool = tx
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM training_attendance WHERE session_id = ?1)",
                    [Sql(*session_id)],
                    |row| row.get(0),
                )
                .map_err(sqlite)?;
            match recorded {
                false => Ok(AttendanceSheet {
                    recorded,
                    rows: roster_rows(tx, &session.team_id, session_id, false)?,
                }),
                true => tx
                    .prepare(
                        "SELECT a.person_id, p.name, a.number, a.status, a.note, a.guest
                         FROM training_attendance a
                         JOIN persons p ON p.id = a.person_id
                         LEFT JOIN team_members tm
                           ON tm.team_id = ?2 AND tm.person_id = a.person_id
                         WHERE a.session_id = ?1 AND a.deleted = 0
                         ORDER BY a.guest, COALESCE(tm.position, 65536),
                                  p.name COLLATE NOCASE, a.person_id",
                    )
                    .map_err(sqlite)?
                    .query_map(
                        params![Sql(*session_id), Sql(session.team_id)],
                        attendance_from_row,
                    )
                    .map_err(sqlite)?
                    .collect::<Result<_, _>>()
                    .map_err(sqlite)
                    .map(|rows| AttendanceSheet { recorded, rows }),
            }
        })
    }

    async fn missing_players(&self, session_id: &Uuid) -> Result<Vec<AttendanceRow>, AppError> {
        self.0.read(|tx| {
            let session = read_session(tx, session_id)?;
            let recorded: bool = tx
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM training_attendance WHERE session_id = ?1)",
                    [Sql(*session_id)],
                    |row| row.get(0),
                )
                .map_err(sqlite)?;
            match recorded {
                false => Ok(vec![]),
                true => roster_rows(tx, &session.team_id, session_id, true),
            }
        })
    }

    async fn search_activities(
        &self,
        team_id: &Uuid,
        query: &str,
        only_team: bool,
        limit: usize,
    ) -> Result<Vec<ActivityHit>, AppError> {
        let query = query.trim().to_lowercase();
        let found: Vec<ActivityHit> = self.0.read(|tx| {
            tx.prepare(
                "SELECT a.name, a.description, a.planned_min, t.id, t.name, s.start_local
                 FROM training_activities a
                 JOIN training_sessions s ON s.id = a.session_id
                 JOIN teams t ON t.id = s.team_id
                 WHERE a.deleted = 0 AND s.deleted = 0 AND t.deleted = 0
                   AND (?1 = 0 OR s.team_id = ?2)
                 ORDER BY s.team_id <> ?2, s.start_local DESC, a.position, a.id",
            )
            .map_err(sqlite)?
            .query_map(params![only_team, Sql(*team_id)], |row| {
                Ok(ActivityHit {
                    name: row.get(0)?,
                    description: row.get(1)?,
                    planned_min: row.get(2)?,
                    team_id: row.get::<_, Sql<Uuid>>(3)?.0,
                    team_name: row.get(4)?,
                    session_start: row.get::<_, Sql<NaiveDateTime>>(5)?.0,
                })
            })
            .map_err(sqlite)?
            .collect::<Result<_, _>>()
            .map_err(sqlite)
        })?;
        let mut seen = HashSet::new();
        Ok(found
            .into_iter()
            .filter(|hit| hit.name.to_lowercase().contains(&query))
            .filter(|hit| seen.insert((hit.name.clone(), hit.description.clone())))
            .take(limit)
            .collect())
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
                    insert_session(tx, team_id, None, &input)
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

    async fn create_series(
        &self,
        team_id: &Uuid,
        input: SeriesInput,
        create_others: bool,
    ) -> Result<SeriesOutcome, AppError> {
        let windows = input
            .generate()
            .map_err(|e| AppError::IO(IOError::Msg(format!("invalid series: {e:?}"))))?;
        self.0.write(|tx| {
            let team: bool = tx
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM teams WHERE id = ?1 AND deleted = 0)",
                    [Sql(*team_id)],
                    |row| row.get(0),
                )
                .map_err(sqlite)?;
            match team {
                false => Err(not_found(&format!("team {team_id}"))),
                true => {
                    let mut busy = session_slots(tx, team_id, None)?;
                    busy.extend(match_slots(tx, team_id, None)?);
                    let (accepted, conflicts) = series::conflicts_of(&windows, busy);
                    match conflicts.is_empty() || create_others {
                        false => Ok(SeriesOutcome {
                            created: vec![],
                            conflicts,
                        }),
                        true => {
                            let series_id = Uuid::new_v4();
                            let created = accepted
                                .into_iter()
                                .map(|(start, duration_min)| {
                                    insert_session(
                                        tx,
                                        team_id,
                                        Some(series_id),
                                        &SessionInput {
                                            microcycle_id: None,
                                            start,
                                            duration_min,
                                            actual_duration_min: None,
                                            location: input.location.trim().to_string(),
                                            title: String::new(),
                                            objectives: String::new(),
                                            notes: String::new(),
                                            final_remarks: String::new(),
                                            status: SessionStatus::Planned,
                                        },
                                    )
                                })
                                .collect::<Result<_, _>>()?;
                            Ok(SeriesOutcome { created, conflicts })
                        }
                    }
                }
            }
        })
    }

    async fn update_series(
        &self,
        session_id: &Uuid,
        scope: SeriesScope,
        edit: SeriesEdit,
    ) -> Result<Vec<TrainingSession>, AppError> {
        match edit.duration_min {
            Some(0) => Err(AppError::IO(IOError::Msg("invalid duration".into()))),
            _ => self.0.write(|tx| {
                let (targets, busy) = series_targets(tx, session_id, scope)?;
                let moved: Vec<(NaiveDateTime, u16, String)> =
                    targets.iter().map(|s| edit.apply(s)).collect();
                let windows: Vec<(NaiveDateTime, u16)> =
                    moved.iter().map(|(start, min, _)| (*start, *min)).collect();
                let (_, conflicts) = series::conflicts_of(&windows, busy);
                match conflicts.is_empty() {
                    false => {
                        let mut hit: Vec<Slot> =
                            conflicts.into_iter().flat_map(|c| c.with).collect();
                        hit.sort_by_key(|s| (s.start, s.minutes));
                        hit.dedup();
                        Err(TrainingError::Overlap(hit).into())
                    }
                    true => targets
                        .iter()
                        .zip(moved)
                        .map(|(session, (start, minutes, location))| {
                            tx.execute(
                                "UPDATE training_sessions SET start_local = ?2,
                                     duration_min = ?3, location = ?4 WHERE id = ?1",
                                params![Sql(session.id), Sql(start), minutes, location],
                            )
                            .map_err(sqlite)?;
                            read_session(tx, &session.id)
                        })
                        .collect(),
                }
            }),
        }
    }

    async fn remove_series(
        &self,
        session_id: &Uuid,
        scope: SeriesScope,
    ) -> Result<usize, AppError> {
        self.0.write(|tx| {
            let (targets, _) = series_targets(tx, session_id, scope)?;
            targets.iter().try_for_each(|s| {
                tx.execute(
                    "UPDATE training_sessions SET deleted = 1 WHERE id = ?1",
                    [Sql(s.id)],
                )
                .map(|_| ())
                .map_err(sqlite)
            })?;
            Ok(targets.len())
        })
    }

    async fn duplicate_session(
        &self,
        session_id: &Uuid,
        input: DuplicateInput,
    ) -> Result<TrainingSession, AppError> {
        self.0.write(|tx| {
            let original = read_session(tx, session_id)?;
            check_session(
                tx,
                &original.team_id,
                None,
                input.start,
                original.duration_min,
            )?;
            let copy = insert_session(
                tx,
                &original.team_id,
                None,
                &SessionInput {
                    microcycle_id: input.microcycle_id,
                    start: input.start,
                    duration_min: original.duration_min,
                    actual_duration_min: None,
                    location: original.location,
                    title: original.title,
                    objectives: match input.copy_objectives {
                        true => original.objectives,
                        false => String::new(),
                    },
                    notes: String::new(),
                    final_remarks: String::new(),
                    status: SessionStatus::Planned,
                },
            )?;
            match input.copy_activities {
                true => tx
                    .prepare(
                        "SELECT name, description, planned_min FROM training_activities
                         WHERE session_id = ?1 AND deleted = 0 AND status = 'planned'
                         ORDER BY position",
                    )
                    .map_err(sqlite)?
                    .query_map([Sql(*session_id)], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<u16>>(2)?,
                        ))
                    })
                    .map_err(sqlite)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(sqlite)?
                    .into_iter()
                    .zip(0u16..)
                    .try_for_each(|((name, description, planned), position)| {
                        tx.execute(
                            "INSERT INTO training_activities (id, session_id, position, name,
                                 description, planned_min)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                            params![
                                Sql(Uuid::new_v4()),
                                Sql(copy.id),
                                position,
                                name,
                                description,
                                planned
                            ],
                        )
                        .map(|_| ())
                        .map_err(sqlite)
                    }),
                false => Ok(()),
            }?;
            Ok(copy)
        })
    }

    async fn set_session_status(
        &self,
        session_id: &Uuid,
        status: SessionStatus,
    ) -> Result<TrainingSession, AppError> {
        self.0.write(|tx| {
            read_session(tx, session_id)?;
            write_status(tx, session_id, status)?;
            read_session(tx, session_id)
        })
    }

    async fn complete_session(
        &self,
        session_id: &Uuid,
        activities: Vec<Uuid>,
    ) -> Result<TrainingSession, AppError> {
        self.0.write(|tx| {
            read_session(tx, session_id)?;
            activities.iter().try_for_each(|id| {
                Some(read_activity(tx, id)?)
                    .filter(|a| a.session_id == *session_id)
                    .ok_or_else(|| not_found(&format!("activity {id}")))
                    .and_then(|a| match a.status {
                        ActivityStatus::Planned => tx
                            .execute(
                                "UPDATE training_activities SET status = ?2 WHERE id = ?1",
                                params![Sql(*id), ActivityStatus::Completed],
                            )
                            .map(|_| ())
                            .map_err(sqlite),
                        _ => Ok(()),
                    })
            })?;
            write_status(tx, session_id, SessionStatus::Completed)?;
            read_session(tx, session_id)
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

    async fn add_activity(
        &self,
        session_id: &Uuid,
        input: ActivityInput,
    ) -> Result<Activity, AppError> {
        self.0.write(|tx| {
            read_session(tx, session_id)?;
            // a removed activity keeps its place: after the last one ever
            let next: Option<u32> = tx
                .query_row(
                    "SELECT MAX(position) + 1 FROM training_activities WHERE session_id = ?1",
                    [Sql(*session_id)],
                    |row| row.get(0),
                )
                .map_err(sqlite)?;
            let position = u16::try_from(next.unwrap_or(0))
                .ok()
                .filter(|p| *p <= LAST_POSITION)
                .ok_or_else(|| AppError::IO(IOError::Msg("too many activities".into())))?;
            let id = Uuid::new_v4();
            tx.execute(
                "INSERT INTO training_activities (id, session_id, position, name, description,
                     planned_min, actual_min, notes, status)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    Sql(id),
                    Sql(*session_id),
                    position,
                    input.name,
                    input.description,
                    input.planned_min,
                    input.actual_min,
                    input.notes,
                    input.status,
                ],
            )
            .map_err(sqlite)?;
            read_activity(tx, &id)
        })
    }

    async fn update_activity(
        &self,
        activity_id: &Uuid,
        input: ActivityInput,
    ) -> Result<Activity, AppError> {
        self.0.write(|tx| {
            read_activity(tx, activity_id)?;
            tx.execute(
                "UPDATE training_activities SET name = ?2, description = ?3, planned_min = ?4,
                     actual_min = ?5, notes = ?6, status = ?7
                 WHERE id = ?1 AND deleted = 0",
                params![
                    Sql(*activity_id),
                    input.name,
                    input.description,
                    input.planned_min,
                    input.actual_min,
                    input.notes,
                    input.status,
                ],
            )
            .map_err(sqlite)?;
            read_activity(tx, activity_id)
        })
    }

    async fn remove_activity(&self, activity_id: &Uuid) -> Result<(), AppError> {
        self.0.write(|tx| {
            read_activity(tx, activity_id)?;
            tx.execute(
                "UPDATE training_activities SET deleted = 1 WHERE id = ?1",
                [Sql(*activity_id)],
            )
            .map_err(sqlite)
            .map(|_| ())
        })
    }

    async fn move_activity(&self, activity_id: &Uuid, to: Move) -> Result<(), AppError> {
        self.0.write(|tx| {
            let activity = read_activity(tx, activity_id)?;
            let (compare, order) = match to {
                Move::Up => ("<", "DESC"),
                Move::Down => (">", "ASC"),
            };
            let neighbour: Option<(Sql<Uuid>, u16)> = tx
                .query_row(
                    &format!(
                        "SELECT id, position FROM training_activities
                         WHERE session_id = ?1 AND deleted = 0 AND position {compare} ?2
                         ORDER BY position {order} LIMIT 1"
                    ),
                    params![Sql(activity.session_id), activity.position],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(sqlite(e)),
                })?;
            match neighbour {
                // the places are unique: the spare one holds the first meanwhile
                Some((other, place)) => [
                    (Sql(activity.id), 65535),
                    (other, activity.position),
                    (Sql(activity.id), place),
                ]
                .into_iter()
                .try_for_each(|(id, position)| {
                    tx.execute(
                        "UPDATE training_activities SET position = ?2 WHERE id = ?1",
                        params![id, position],
                    )
                    .map(|_| ())
                    .map_err(sqlite)
                }),
                None => Ok(()),
            }
        })
    }

    async fn save_attendance(
        &self,
        session_id: &Uuid,
        rows: Vec<AttendanceInput>,
    ) -> Result<(), AppError> {
        self.0.write(|tx| {
            let session = read_session(tx, session_id)?;
            let mut seen = HashSet::new();
            rows.iter().try_for_each(|row| {
                let member: bool = tx
                    .query_row(
                        "SELECT EXISTS (SELECT 1 FROM team_members
                                        WHERE team_id = ?1 AND person_id = ?2 AND deleted = 0)",
                        params![Sql(session.team_id), Sql(row.person_id)],
                        |r| r.get(0),
                    )
                    .map_err(sqlite)?;
                let alive: bool = tx
                    .query_row(
                        "SELECT EXISTS (SELECT 1 FROM persons WHERE id = ?1 AND deleted = 0)",
                        [Sql(row.person_id)],
                        |r| r.get(0),
                    )
                    .map_err(sqlite)?;
                match (seen.insert(row.person_id), alive, row.guest && member) {
                    (false, _, _) => Err(AppError::IO(IOError::Msg(format!(
                        "person {} twice in the attendance",
                        row.person_id
                    )))),
                    (_, false, _) => Err(not_found(&format!("person {}", row.person_id))),
                    (_, _, true) => Err(AppError::IO(IOError::Msg(format!(
                        "person {} is of the team, not a guest",
                        row.person_id
                    )))),
                    _ => Ok(()),
                }
            })?;
            tx.execute(
                "UPDATE training_attendance SET deleted = 1 WHERE session_id = ?1",
                [Sql(*session_id)],
            )
            .map_err(sqlite)?;
            rows.iter().try_for_each(|row| {
                tx.execute(
                    "INSERT INTO training_attendance
                         (session_id, person_id, status, note, number, guest, deleted)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)
                     ON CONFLICT (session_id, person_id) DO UPDATE SET
                         status = ?3, note = ?4, number = ?5, guest = ?6, deleted = 0",
                    params![
                        Sql(*session_id),
                        Sql(row.person_id),
                        row.status,
                        row.note,
                        row.number,
                        row.guest,
                    ],
                )
                .map(|_| ())
                .map_err(sqlite)
            })
        })
    }
}
