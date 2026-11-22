//! The cycles of the training in the database (TODO/021 §4): program,
//! mesocycle, microcycle, one code for the three. That a window is inside
//! its parent's and overlaps no other of its level is checked here, with
//! errors that say which; the database only checks that parent and child
//! are of the same team.

use crate::{
    domain::sql::Sql,
    errors::{AppError, TrainingError},
    providers::{
        sqlite::{
            not_found, sqlite,
            training::{
                SqliteTraining, copy_planned_activities, insert_session, match_slots, read_session,
                session_slots,
            },
        },
        training_cycles::TrainingCycles,
    },
    training::{
        SessionInput, SessionStatus,
        cycles::{Child, Cycle, CycleInput, Level, Span, Window},
        duplicate::{DuplicateCycleInput, MAX_CYCLE_COPIES},
        overlap::{Slot, clashes},
    },
};
use async_trait::async_trait;
use chrono::{Duration, NaiveDate, NaiveDateTime};
use rusqlite::{Row, Transaction, params};
use uuid::Uuid;

fn table(level: Level) -> &'static str {
    match level {
        Level::Program => "training_programs",
        Level::Mesocycle => "mesocycles",
        Level::Microcycle => "microcycles",
    }
}

/// The columns of every level in one shape: id, team, parent, title, start,
/// weeks, last day, objectives, notes.
fn columns(level: Level) -> &'static str {
    match level {
        Level::Program => "id, team_id, NULL, title, start_date, NULL, end_date, '', notes",
        Level::Mesocycle => {
            "id, team_id, program_id, title, start_date, weeks, NULL, objectives, notes"
        }
        Level::Microcycle => {
            "id, team_id, mesocycle_id, title, start_date, weeks, NULL, objectives, notes"
        }
    }
}

/// The table and the column that link what a cycle of `level` holds to it.
fn link(level: Level) -> (&'static str, &'static str) {
    match level {
        Level::Program => ("mesocycles", "program_id"),
        Level::Mesocycle => ("microcycles", "mesocycle_id"),
        Level::Microcycle => ("training_sessions", "microcycle_id"),
    }
}

fn cycle_from_row(level: Level, row: &Row) -> rusqlite::Result<Cycle> {
    let start = row.get::<_, Sql<NaiveDate>>(4)?.0;
    let span = match (
        row.get::<_, Option<u16>>(5)?,
        row.get::<_, Option<Sql<NaiveDate>>>(6)?,
    ) {
        (Some(weeks), _) => Span::Weeks(weeks),
        (None, Some(last)) => Span::Until(last.0),
        (None, None) => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(Cycle {
        id: row.get::<_, Sql<Uuid>>(0)?.0,
        team_id: row.get::<_, Sql<Uuid>>(1)?.0,
        level,
        parent_id: row.get::<_, Option<Sql<Uuid>>>(2)?.map(|id| id.0),
        title: row.get(3)?,
        span,
        window: Window::of(level, start, span).ok_or(rusqlite::Error::InvalidQuery)?,
        objectives: row.get(7)?,
        notes: row.get(8)?,
    })
}

pub(crate) fn read_cycle(tx: &Transaction, level: Level, id: &Uuid) -> Result<Cycle, AppError> {
    tx.query_row(
        &format!(
            "SELECT {} FROM {} WHERE id = ?1 AND deleted = 0",
            columns(level),
            table(level)
        ),
        [Sql(*id)],
        |row| cycle_from_row(level, row),
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => not_found(&format!("cycle {id}")),
        e => sqlite(e),
    })
}

fn team_cycles(tx: &Transaction, team_id: &Uuid, level: Level) -> Result<Vec<Cycle>, AppError> {
    tx.prepare(&format!(
        "SELECT {} FROM {} WHERE team_id = ?1 AND deleted = 0 ORDER BY start_date, id",
        columns(level),
        table(level)
    ))
    .map_err(sqlite)?
    .query_map([Sql(*team_id)], |row| cycle_from_row(level, row))
    .map_err(sqlite)?
    .collect::<Result<_, _>>()
    .map_err(sqlite)
}

/// What a cycle of `level` holds, chosen by `filter` (`{col}` is the link
/// column) with `key` as its only parameter.
fn child_rows(
    tx: &Transaction,
    level: Level,
    filter: &str,
    key: &Uuid,
) -> Result<Vec<Child>, AppError> {
    let (child_table, col) = link(level);
    let filter = filter.replace("{col}", col);
    match level.child() {
        Some(child) => tx
            .prepare(&format!(
                "SELECT {} FROM {child_table} WHERE {filter} AND deleted = 0
                 ORDER BY start_date, id",
                columns(child)
            ))
            .map_err(sqlite)?
            .query_map([Sql(*key)], |row| {
                cycle_from_row(child, row).map(|c| Child {
                    id: c.id,
                    window: c.window,
                })
            })
            .map_err(sqlite)?
            .collect::<Result<_, _>>()
            .map_err(sqlite),
        None => tx
            .prepare(&format!(
                "SELECT id, start_local FROM {child_table} WHERE {filter} AND deleted = 0
                 ORDER BY start_local, id"
            ))
            .map_err(sqlite)?
            .query_map([Sql(*key)], |row| {
                Ok(Child {
                    id: row.get::<_, Sql<Uuid>>(0)?.0,
                    window: Window::day(row.get::<_, Sql<NaiveDateTime>>(1)?.0.date()),
                })
            })
            .map_err(sqlite)?
            .collect::<Result<_, _>>()
            .map_err(sqlite),
    }
}

/// The team, the parent and the window of a child of a cycle of `level`.
fn child_info(
    tx: &Transaction,
    level: Level,
    id: &Uuid,
) -> Result<(Uuid, Option<Uuid>, Window), AppError> {
    match level.child() {
        Some(child) => read_cycle(tx, child, id).map(|c| (c.team_id, c.parent_id, c.window)),
        None => {
            read_session(tx, id).map(|s| (s.team_id, s.microcycle_id, Window::day(s.start.date())))
        }
    }
}

fn execute(tx: &Transaction, sql: &str, params: impl rusqlite::Params) -> Result<(), AppError> {
    tx.execute(sql, params).map(|_| ()).map_err(sqlite)
}

/// The cycle of `level` can be at `window`, under `parent`: inside it, and
/// overlapping no other cycle of the team of the level but `except`.
pub(crate) fn check_cycle(
    tx: &Transaction,
    level: Level,
    team_id: &Uuid,
    except: Option<&Uuid>,
    window: &Window,
    parent_id: Option<&Uuid>,
) -> Result<(), AppError> {
    match (level.parent(), parent_id) {
        (Some(parent_level), Some(parent_id)) => {
            let parent = read_cycle(tx, parent_level, parent_id)?;
            match (parent.team_id == *team_id, parent.window.contains(window)) {
                (false, _) => Err(not_found(&format!("cycle {parent_id}"))),
                (_, false) => Err(TrainingError::OutsideParent.into()),
                _ => Ok(()),
            }
        }
        _ => Ok(()),
    }?;
    let clashes: Vec<Window> = team_cycles(tx, team_id, level)?
        .into_iter()
        .filter(|other| Some(&other.id) != except && other.window.overlaps(window))
        .map(|other| other.window)
        .collect();
    match clashes.is_empty() {
        true => Ok(()),
        false => Err(TrainingError::CycleOverlap(clashes).into()),
    }
}

/// The session may be in the microcycle: of its team, and its day inside
/// the window. Nothing to check without a microcycle.
pub(crate) fn check_microcycle(
    tx: &Transaction,
    team_id: &Uuid,
    microcycle_id: Option<&Uuid>,
    start: NaiveDateTime,
) -> Result<(), AppError> {
    match microcycle_id {
        None => Ok(()),
        Some(id) => {
            let cycle = read_cycle(tx, Level::Microcycle, id)?;
            match (
                cycle.team_id == *team_id,
                cycle.window.has_date(start.date()),
            ) {
                (false, _) => Err(not_found(&format!("cycle {id}"))),
                (_, false) => Err(TrainingError::OutsideParent.into()),
                _ => Ok(()),
            }
        }
    }
}

/// Puts the free children under the cycle: of its team, free, and each
/// inside its window.
fn attach(tx: &Transaction, parent: &Cycle, children: &[Uuid]) -> Result<(), AppError> {
    let (child_table, col) = link(parent.level);
    children.iter().try_for_each(|id| {
        let (team_id, linked, window) = child_info(tx, parent.level, id)?;
        match (
            team_id == parent.team_id && linked.is_none(),
            parent.window.contains(&window),
        ) {
            (false, _) => Err(not_found(&format!("free child {id}"))),
            (_, false) => Err(TrainingError::OutsideParent.into()),
            _ => execute(
                tx,
                &format!("UPDATE {child_table} SET {col} = ?2 WHERE id = ?1"),
                params![Sql(*id), Sql(parent.id)],
            ),
        }
    })
}

fn detach(tx: &Transaction, level: Level, children: &[Child]) -> Result<(), AppError> {
    let (child_table, col) = link(level);
    children.iter().try_for_each(|child| {
        execute(
            tx,
            &format!("UPDATE {child_table} SET {col} = NULL WHERE id = ?1"),
            [Sql(child.id)],
        )
    })
}

fn mark_removed(tx: &Transaction, level: Level, id: &Uuid) -> Result<(), AppError> {
    execute(
        tx,
        &format!("UPDATE {} SET deleted = 1 WHERE id = ?1", table(level)),
        [Sql(*id)],
    )
}

/// Removes the cycle with everything under it.
fn remove_tree(tx: &Transaction, level: Level, id: &Uuid) -> Result<(), AppError> {
    match level.child() {
        Some(child) => child_rows(tx, level, "{col} = ?1", id)?
            .iter()
            .try_for_each(|c| remove_tree(tx, child, &c.id)),
        None => execute(
            tx,
            "UPDATE training_sessions SET deleted = 1 WHERE microcycle_id = ?1",
            [Sql(*id)],
        ),
    }?;
    mark_removed(tx, level, id)
}

fn window_of(level: Level, input: &CycleInput) -> Result<Window, AppError> {
    Window::of(level, input.start, input.span).ok_or_else(|| TrainingError::InvalidWindow.into())
}

/// Writes a new cycle of `level` and returns its id.
fn insert_cycle(
    tx: &Transaction,
    level: Level,
    team_id: &Uuid,
    parent_id: Option<&Uuid>,
    input: &CycleInput,
    window: &Window,
) -> Result<Uuid, AppError> {
    let id = Uuid::new_v4();
    match (level, input.span) {
        (Level::Program, Span::Until(_)) => execute(
            tx,
            "INSERT INTO training_programs (id, team_id, title, start_date, end_date, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                Sql(id),
                Sql(*team_id),
                input.title,
                Sql(window.start),
                Sql(window.last()),
                input.notes
            ],
        ),
        (Level::Mesocycle, Span::Weeks(weeks)) => execute(
            tx,
            "INSERT INTO mesocycles (id, team_id, program_id, title, start_date, weeks,
                 objectives, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                Sql(id),
                Sql(*team_id),
                parent_id.map(|p| Sql(*p)),
                input.title,
                Sql(window.start),
                weeks,
                input.objectives,
                input.notes
            ],
        ),
        (Level::Microcycle, Span::Weeks(weeks)) => execute(
            tx,
            "INSERT INTO microcycles (id, team_id, mesocycle_id, title, start_date, weeks,
                 objectives, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                Sql(id),
                Sql(*team_id),
                parent_id.map(|p| Sql(*p)),
                input.title,
                Sql(window.start),
                weeks,
                input.objectives,
                input.notes
            ],
        ),
        _ => Err(TrainingError::InvalidWindow.into()),
    }?;
    Ok(id)
}

#[async_trait]
impl TrainingCycles for SqliteTraining {
    async fn cycles(&self, team_id: &Uuid, level: Level) -> Result<Vec<Cycle>, AppError> {
        self.db().read(|tx| team_cycles(tx, team_id, level))
    }

    async fn cycle(&self, level: Level, id: &Uuid) -> Result<Cycle, AppError> {
        self.db().read(|tx| read_cycle(tx, level, id))
    }

    async fn containing(
        &self,
        team_id: &Uuid,
        level: Level,
        date: NaiveDate,
    ) -> Result<Vec<Cycle>, AppError> {
        self.db().read(|tx| {
            team_cycles(tx, team_id, level).map(|all| {
                all.into_iter()
                    .filter(|c| c.window.has_date(date))
                    .collect()
            })
        })
    }

    async fn children(&self, level: Level, id: &Uuid) -> Result<Vec<Child>, AppError> {
        self.db().read(|tx| {
            read_cycle(tx, level, id)?;
            child_rows(tx, level, "{col} = ?1", id)
        })
    }

    async fn free_children(&self, level: Level, id: &Uuid) -> Result<Vec<Child>, AppError> {
        self.db().read(|tx| {
            let parent = read_cycle(tx, level, id)?;
            child_rows(tx, level, "team_id = ?1 AND {col} IS NULL", &parent.team_id).map(|free| {
                free.into_iter()
                    .filter(|c| parent.window.contains(&c.window))
                    .collect()
            })
        })
    }

    async fn create_cycle(
        &self,
        team_id: &Uuid,
        level: Level,
        input: CycleInput,
        children: Vec<Uuid>,
    ) -> Result<Cycle, AppError> {
        self.db().write(|tx| {
            let team: bool = tx
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM teams WHERE id = ?1 AND deleted = 0)",
                    [Sql(*team_id)],
                    |row| row.get(0),
                )
                .map_err(sqlite)?;
            let window = window_of(level, &input)?;
            let parent_id = input.parent_id.filter(|_| level.parent().is_some());
            match team {
                true => check_cycle(tx, level, team_id, None, &window, parent_id.as_ref()),
                false => Err(not_found(&format!("team {team_id}"))),
            }?;
            let id = insert_cycle(tx, level, team_id, parent_id.as_ref(), &input, &window)?;
            let cycle = read_cycle(tx, level, &id)?;
            attach(tx, &cycle, &children)?;
            Ok(cycle)
        })
    }

    async fn update_cycle(
        &self,
        level: Level,
        id: &Uuid,
        input: CycleInput,
        detach_children: bool,
    ) -> Result<Cycle, AppError> {
        self.db().write(|tx| {
            let current = read_cycle(tx, level, id)?;
            let window = window_of(level, &input)?;
            let parent_id = input.parent_id.filter(|_| level.parent().is_some());
            check_cycle(
                tx,
                level,
                &current.team_id,
                Some(id),
                &window,
                parent_id.as_ref(),
            )?;
            let outside: Vec<Child> = child_rows(tx, level, "{col} = ?1", id)?
                .into_iter()
                .filter(|c| !window.contains(&c.window))
                .collect();
            match (outside.is_empty(), detach_children) {
                (false, false) => Err(TrainingError::ChildrenOutside(outside).into()),
                _ => detach(tx, level, &outside),
            }?;
            match (level, input.span) {
                (Level::Program, Span::Until(_)) => execute(
                    tx,
                    "UPDATE training_programs SET title = ?2, start_date = ?3, end_date = ?4,
                         notes = ?5 WHERE id = ?1",
                    params![
                        Sql(*id),
                        input.title,
                        Sql(window.start),
                        Sql(window.last()),
                        input.notes
                    ],
                ),
                (Level::Mesocycle, Span::Weeks(weeks)) => execute(
                    tx,
                    "UPDATE mesocycles SET program_id = ?2, title = ?3, start_date = ?4,
                         weeks = ?5, objectives = ?6, notes = ?7 WHERE id = ?1",
                    params![
                        Sql(*id),
                        parent_id.map(Sql),
                        input.title,
                        Sql(window.start),
                        weeks,
                        input.objectives,
                        input.notes
                    ],
                ),
                (Level::Microcycle, Span::Weeks(weeks)) => execute(
                    tx,
                    "UPDATE microcycles SET mesocycle_id = ?2, title = ?3, start_date = ?4,
                         weeks = ?5, objectives = ?6, notes = ?7 WHERE id = ?1",
                    params![
                        Sql(*id),
                        parent_id.map(Sql),
                        input.title,
                        Sql(window.start),
                        weeks,
                        input.objectives,
                        input.notes
                    ],
                ),
                _ => Err(TrainingError::InvalidWindow.into()),
            }?;
            read_cycle(tx, level, id)
        })
    }

    async fn add_children(
        &self,
        level: Level,
        id: &Uuid,
        children: Vec<Uuid>,
    ) -> Result<(), AppError> {
        self.db().write(|tx| {
            let parent = read_cycle(tx, level, id)?;
            attach(tx, &parent, &children)
        })
    }

    async fn remove_cycle(
        &self,
        level: Level,
        id: &Uuid,
        with_children: bool,
    ) -> Result<(), AppError> {
        self.db().write(|tx| {
            read_cycle(tx, level, id)?;
            match with_children {
                true => remove_tree(tx, level, id),
                false => {
                    let held = child_rows(tx, level, "{col} = ?1", id)?;
                    detach(tx, level, &held)?;
                    mark_removed(tx, level, id)
                }
            }
        })
    }

    async fn duplicate_microcycle(
        &self,
        id: &Uuid,
        input: DuplicateCycleInput,
    ) -> Result<Vec<Cycle>, AppError> {
        self.db().write(|tx| {
            let original = read_cycle(tx, Level::Microcycle, id)?;
            match (
                (1..=MAX_CYCLE_COPIES).contains(&input.copies),
                input.shift_weeks >= 1,
            ) {
                (true, true) => Ok(()),
                _ => Err(TrainingError::InvalidWindow),
            }?;
            let sessions = child_rows(tx, Level::Microcycle, "{col} = ?1", id)?
                .iter()
                .map(|c| read_session(tx, &c.id))
                .collect::<Result<Vec<_>, _>>()?;
            let shifts: Vec<Duration> = (1..=i64::from(input.copies))
                .map(|k| Duration::weeks(k * i64::from(input.shift_weeks)))
                .collect();
            let windows = shifts
                .iter()
                .map(|d| {
                    original
                        .window
                        .start
                        .checked_add_signed(*d)
                        .and_then(|start| Window::of(Level::Microcycle, start, original.span))
                        .ok_or(TrainingError::InvalidWindow)
                })
                .collect::<Result<Vec<_>, _>>()?;
            // everything that is in the way, before writing anything
            let cycle_clashes: Vec<Window> = team_cycles(tx, &original.team_id, Level::Microcycle)?
                .into_iter()
                .filter(|other| windows.iter().any(|w| w.overlaps(&other.window)))
                .map(|other| other.window)
                .collect();
            let mut busy = session_slots(tx, &original.team_id, None)?;
            busy.extend(match_slots(tx, &original.team_id, None)?);
            let mut slot_clashes: Vec<Slot> = sessions
                .iter()
                .flat_map(|s| {
                    shifts.iter().flat_map(|d| {
                        s.start
                            .checked_add_signed(*d)
                            .map(|start| clashes(&Slot::session(start, s.duration_min), &busy))
                            .unwrap_or_default()
                    })
                })
                .collect();
            slot_clashes.sort_by_key(|s| (s.start, s.minutes));
            slot_clashes.dedup();
            match (cycle_clashes.is_empty(), slot_clashes.is_empty()) {
                (false, _) => Err(TrainingError::CycleOverlap(cycle_clashes)),
                (_, false) => Err(TrainingError::Overlap(slot_clashes)),
                _ => Ok(()),
            }?;
            let parent = original
                .parent_id
                .map(|p| read_cycle(tx, Level::Mesocycle, &p))
                .transpose()?;
            windows
                .iter()
                .zip(&shifts)
                .map(|(window, shift)| {
                    let parent_id = parent
                        .as_ref()
                        .filter(|p| p.window.contains(window))
                        .map(|p| p.id);
                    let copy_id = insert_cycle(
                        tx,
                        Level::Microcycle,
                        &original.team_id,
                        parent_id.as_ref(),
                        &CycleInput {
                            parent_id,
                            title: original.title.clone(),
                            start: window.start,
                            span: original.span,
                            objectives: match input.copy_objectives {
                                true => original.objectives.clone(),
                                false => String::new(),
                            },
                            notes: String::new(),
                        },
                        window,
                    )?;
                    sessions.iter().try_for_each(|s| {
                        let start = s
                            .start
                            .checked_add_signed(*shift)
                            .ok_or(TrainingError::InvalidWindow)?;
                        let copy = insert_session(
                            tx,
                            &s.team_id,
                            None,
                            &SessionInput {
                                microcycle_id: Some(copy_id),
                                start,
                                duration_min: s.duration_min,
                                actual_duration_min: None,
                                location: s.location.clone(),
                                title: s.title.clone(),
                                objectives: match input.copy_objectives {
                                    true => s.objectives.clone(),
                                    false => String::new(),
                                },
                                notes: String::new(),
                                final_remarks: String::new(),
                                status: SessionStatus::Planned,
                            },
                        )?;
                        match input.copy_activities {
                            true => copy_planned_activities(tx, &s.id, &copy.id),
                            false => Ok(()),
                        }
                    })?;
                    read_cycle(tx, Level::Microcycle, &copy_id)
                })
                .collect()
        })
    }
}
