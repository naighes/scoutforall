//! The totals and the attendance per player from the tables (TODO/021 §6).

use crate::{
    domain::sql::Sql,
    errors::AppError,
    providers::{
        sqlite::{
            sqlite,
            training::{SESSION_COLUMNS, SqliteTraining, session_from_row},
            training_cycles::read_cycle,
        },
        training_totals::TrainingTotals,
    },
    training::{
        cycles::Level,
        totals::{AttendanceLine, AttendanceReport, Scope, Totals, attendance_report, totals},
    },
};
use async_trait::async_trait;
use chrono::NaiveDate;
use rusqlite::{Transaction, params};
use uuid::Uuid;

/// The sessions of the team in the scope, the `s` of the query: the scope
/// sets one of the four parameters from ?2, the others stay NULL.
const IN_SCOPE: &str = "s.team_id = ?1 AND s.deleted = 0
    AND (?2 IS NULL OR substr(s.start_local, 1, 10) >= ?2)
    AND (?3 IS NULL OR substr(s.start_local, 1, 10) < ?3)
    AND (?4 IS NULL OR s.microcycle_id = ?4)
    AND (?5 IS NULL OR s.microcycle_id IN (
          SELECT id FROM microcycles WHERE mesocycle_id = ?5 AND deleted = 0))
    AND (?6 IS NULL OR s.microcycle_id IN (
          SELECT mc.id FROM microcycles mc JOIN mesocycles m ON m.id = mc.mesocycle_id
          WHERE m.program_id = ?6 AND m.deleted = 0 AND mc.deleted = 0))";

/// The values of `?2..?6` for a scope.
type Bounds = (
    Option<Sql<NaiveDate>>,
    Option<Sql<NaiveDate>>,
    Option<Sql<Uuid>>,
    Option<Sql<Uuid>>,
    Option<Sql<Uuid>>,
);

fn bounds(scope: Scope) -> Bounds {
    match scope {
        Scope::All => (None, None, None, None, None),
        Scope::Dates(w) => (Some(Sql(w.start)), Some(Sql(w.end)), None, None, None),
        Scope::Cycle(Level::Microcycle, id) => (None, None, Some(Sql(id)), None, None),
        Scope::Cycle(Level::Mesocycle, id) => (None, None, None, Some(Sql(id)), None),
        Scope::Cycle(Level::Program, id) => (None, None, None, None, Some(Sql(id))),
    }
}

/// The person of each makeup of the team in the scope: a guest line, marked
/// so, present or late, of a player of the team in a completed session of
/// another team, whose start is in the period of the scope (that of the
/// window for a cycle, none for all).
fn makeups(tx: &Transaction, team_id: &Uuid, scope: Scope) -> Result<Vec<Uuid>, AppError> {
    let period: Option<Option<(NaiveDate, NaiveDate)>> = match scope {
        Scope::All => Some(None),
        Scope::Dates(w) => Some(Some((w.start, w.end))),
        Scope::Cycle(level, id) => read_cycle(tx, level, &id)
            .ok()
            .filter(|c| c.team_id == *team_id)
            .map(|c| Some((c.window.start, c.window.end))),
    };
    match period {
        None => Ok(vec![]),
        Some(period) => tx
            .prepare(
                "SELECT a.person_id
                 FROM training_attendance a JOIN training_sessions s ON s.id = a.session_id
                 WHERE a.deleted = 0 AND a.guest = 1 AND a.makeup = 1
                   AND a.status IN ('present', 'late')
                   AND s.deleted = 0 AND s.status = 'completed' AND s.team_id <> ?1
                   AND EXISTS (SELECT 1 FROM team_members tm
                               WHERE tm.team_id = ?1 AND tm.person_id = a.person_id)
                   AND (?2 IS NULL OR substr(s.start_local, 1, 10) >= ?2)
                   AND (?3 IS NULL OR substr(s.start_local, 1, 10) < ?3)",
            )
            .map_err(sqlite)?
            .query_map(
                params![
                    Sql(*team_id),
                    period.map(|(from, _)| Sql(from)),
                    period.map(|(_, to)| Sql(to))
                ],
                |row| Ok(row.get::<_, Sql<Uuid>>(0)?.0),
            )
            .map_err(sqlite)?
            .collect::<Result<_, _>>()
            .map_err(sqlite),
    }
}

#[async_trait]
impl TrainingTotals for SqliteTraining {
    async fn totals(&self, team_id: &Uuid, scope: Scope) -> Result<Totals, AppError> {
        let b = bounds(scope);
        self.db().read(|tx| {
            tx.prepare(&format!(
                "SELECT {} FROM training_sessions s WHERE {IN_SCOPE}",
                SESSION_COLUMNS
            ))
            .map_err(sqlite)?
            .query_map(
                params![Sql(*team_id), b.0, b.1, b.2, b.3, b.4],
                session_from_row,
            )
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)
            .map(|sessions| totals(&sessions))
        })
    }

    async fn attendance_report(
        &self,
        team_id: &Uuid,
        scope: Scope,
    ) -> Result<AttendanceReport, AppError> {
        let b = bounds(scope);
        self.db().read(|tx| {
            tx.prepare(&format!(
                "SELECT s.id, s.start_local, a.person_id, p.name, a.number, a.status, a.guest
                 FROM training_sessions s
                   JOIN training_attendance a ON a.session_id = s.id AND a.deleted = 0
                   JOIN persons p ON p.id = a.person_id
                 WHERE s.status = 'completed' AND {IN_SCOPE}"
            ))
            .map_err(sqlite)?
            .query_map(params![Sql(*team_id), b.0, b.1, b.2, b.3, b.4], |row| {
                Ok(AttendanceLine {
                    session_id: row.get::<_, Sql<Uuid>>(0)?.0,
                    start: row.get::<_, Sql<chrono::NaiveDateTime>>(1)?.0,
                    person_id: row.get::<_, Sql<Uuid>>(2)?.0,
                    name: row.get(3)?,
                    number: row.get(4)?,
                    status: row.get(5)?,
                    guest: row.get::<_, u8>(6)? == 1,
                })
            })
            .map_err(sqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite)
            .and_then(|lines| {
                makeups(tx, team_id, scope).map(|makeups| attendance_report(&lines, &makeups))
            })
        })
    }
}
