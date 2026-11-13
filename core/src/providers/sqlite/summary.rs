//! The cache `match_summary` (D7): what the lists show of a match, so the
//! home and the matches of a team don't read and replay every set. It
//! takes the place of the index of the files (`store::match_index`).
//!
//! - Updated in the transaction of every write that changes what the
//!   status of a match comes from (its sets, their events, its
//!   substitutions): never stale, also when the app stops halfway.
//! - Only the status is cached: opponent, date and court are read from
//!   the match each time.
//! - A match that doesn't replay has no row, as the files don't index it:
//!   its reason is in the language of the app, so it's read again.
//! - A missing row, or one replayed by another version of the app (whose
//!   rules may replay differently), is rebuilt by the next listing.
//! - A write of a team forgets the rows of its matches, as `team.json`
//!   changed the stamp of the files: the next listing rebuilds them.
//! - From A's side, never synced: `DELETE FROM match_summary` is always
//!   safe.

use crate::{
    domain::{Home, TeamRef, sql::Sql},
    errors::AppError,
    localization::Language,
    providers::sqlite::{matches::replayed, rows::team_row, sqlite, teams::read_team},
    shapes::{r#match::MatchSummary, team::TeamEntry},
};
use chrono::{DateTime, FixedOffset};
use rusqlite::{OptionalExtension, Transaction, params};
use uuid::Uuid;

/// The version of the app whose rules replayed a row.
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The row of the match as it replays now: written, or removed when the
/// match isn't there or doesn't replay.
pub(crate) fn refresh(tx: &Transaction, match_id: Uuid) -> Result<(), AppError> {
    let team_a: Option<Uuid> = tx
        .query_row(
            "SELECT team_a FROM matches WHERE id = ?1 AND deleted = 0",
            [Sql(match_id)],
            |row| row.get::<_, Sql<Uuid>>(0).map(|id| id.0),
        )
        .optional()
        .map_err(sqlite)?;
    let summary = team_a.and_then(|team| {
        read_team(tx, &team)
            .and_then(|team| replayed(tx, &team, &match_id.to_string(), &Language::default()))
            .ok()
            .map(|(m, status)| MatchSummary::of(&m, &status))
    });
    match summary {
        Some(summary) => save(tx, match_id, &summary),
        None => forget(tx, match_id),
    }
}

/// The row of the match, as `summary` has it.
pub(crate) fn save(
    tx: &Transaction,
    match_id: Uuid,
    summary: &MatchSummary,
) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO match_summary (match_id, sets, a_wins, b_wins, finished, resume,
             next_set_number, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (match_id) DO UPDATE SET sets = excluded.sets, a_wins = excluded.a_wins,
             b_wins = excluded.b_wins, finished = excluded.finished, resume = excluded.resume,
             next_set_number = excluded.next_set_number, version = excluded.version",
        params![
            Sql(match_id),
            summary.sets,
            summary.us_wins,
            summary.them_wins,
            summary.finished,
            summary.resume,
            summary.next_set_number,
            VERSION
        ],
    )
    .map(|_| ())
    .map_err(sqlite)
}

/// No row for the match: the next listing replays it.
pub(crate) fn forget(tx: &Transaction, match_id: Uuid) -> Result<(), AppError> {
    tx.execute(
        "DELETE FROM match_summary WHERE match_id = ?1",
        [Sql(match_id)],
    )
    .map(|_| ())
    .map_err(sqlite)
}

/// No row for the matches of team A `team`.
pub(crate) fn forget_team(tx: &Transaction, team: Uuid) -> Result<(), AppError> {
    tx.execute(
        "DELETE FROM match_summary
         WHERE match_id IN (SELECT id FROM matches WHERE team_a = ?1)",
        [Sql(team)],
    )
    .map(|_| ())
    .map_err(sqlite)
}

/// The summary of the match of team A `team` from its row, `None` when
/// the row is missing or of another version.
pub(crate) fn cached(
    tx: &Transaction,
    team: &TeamEntry,
    match_id: Uuid,
) -> Result<Option<MatchSummary>, AppError> {
    let row = tx
        .query_row(
            "SELECT m.team_b, m.team_b_name, m.home, m.start, s.sets, s.a_wins, s.b_wins,
                    s.finished, s.resume, s.next_set_number
             FROM match_summary s JOIN matches m ON m.id = s.match_id
             WHERE s.match_id = ?1 AND s.version = ?2 AND m.team_a = ?3 AND m.deleted = 0",
            params![Sql(match_id), VERSION, Sql(team.id)],
            |r| {
                Ok((
                    r.get::<_, Option<Sql<Uuid>>>(0)?.map(|id| id.0),
                    r.get::<_, Option<String>>(1)?,
                    MatchSummary {
                        id: match_id.to_string(),
                        opponent: String::new(),
                        date: r.get::<_, Sql<DateTime<FixedOffset>>>(3)?.0,
                        home: r.get::<_, Home>(2)?.is_a(),
                        sets: r.get(4)?,
                        us_wins: r.get(5)?,
                        them_wins: r.get(6)?,
                        finished: r.get(7)?,
                        resume: r.get(8)?,
                        next_set_number: r.get(9)?,
                    },
                ))
            },
        )
        .optional()
        .map_err(sqlite)?;
    let Some((team_b, team_b_name, summary)) = row else {
        return Ok(None);
    };
    // the name of B as the match reads it (`MatchRows::entry`)
    let opponent = match TeamRef::from_columns(team_b, team_b_name)? {
        TeamRef::Name(name) => Some(name),
        TeamRef::Team(b) => team_row(tx, b)?.map(|b| b.name),
    };
    Ok(opponent.map(|opponent| MatchSummary {
        opponent,
        ..summary
    }))
}
