//! The agenda of a team, or of all, from the tables: the matches are read
//! as rows of `matches` and a summary, never replayed.

use crate::{
    domain::sql::Sql,
    errors::AppError,
    providers::{
        sqlite::{sqlite, training::SqliteTraining, venues::occupations},
        training_agenda::TrainingAgenda,
    },
    training::{
        SessionStatus,
        agenda::{AgendaEvent, AgendaItem, MatchState, Period, played_minutes, score_of},
        overlap::DEFAULT_MATCH_MINUTES,
        venue::{VenueRef, shared},
    },
};
use async_trait::async_trait;
use chrono::{DateTime, Duration, FixedOffset, NaiveDateTime};
use rusqlite::{Transaction, params};
use uuid::Uuid;

fn sessions(
    tx: &Transaction,
    team: Option<&Uuid>,
    period: &Period,
    now: NaiveDateTime,
) -> Result<Vec<AgendaEvent>, AppError> {
    tx.prepare(
        "SELECT s.id, s.team_id, t.name, s.start_local, s.duration_min, s.title, s.status,
                v.id, v.name, v.deleted
         FROM training_sessions s JOIN teams t ON t.id = s.team_id
              LEFT JOIN venues v ON v.id = s.venue_id
         WHERE s.deleted = 0 AND t.deleted = 0 AND (?1 IS NULL OR s.team_id = ?1)
           AND substr(s.start_local, 1, 10) >= ?2 AND substr(s.start_local, 1, 10) < ?3",
    )
    .map_err(sqlite)?
    .query_map(
        params![team.map(|t| Sql(*t)), Sql(period.from), Sql(period.to)],
        |row| {
            let start = row.get::<_, Sql<NaiveDateTime>>(3)?.0;
            let minutes: u16 = row.get(4)?;
            let status: SessionStatus = row.get(6)?;
            Ok(AgendaEvent {
                team_id: row.get::<_, Sql<Uuid>>(1)?.0,
                team_name: row.get(2)?,
                start,
                minutes,
                item: AgendaItem::Session {
                    id: row.get::<_, Sql<Uuid>>(0)?.0,
                    title: row.get(5)?,
                    status,
                    to_close: status == SessionStatus::Planned
                        && start + Duration::minutes(i64::from(minutes)) < now,
                },
                venue: venue_ref(row, 7)?,
                shared_venue: false,
            })
        },
    )
    .map_err(sqlite)?
    .collect::<Result<_, _>>()
    .map_err(sqlite)
}

type Side = (Uuid, String);

/// The gym of a row: its id, name and removal, from column `first`.
fn venue_ref(row: &rusqlite::Row, first: usize) -> rusqlite::Result<Option<VenueRef>> {
    row.get::<_, Option<Sql<Uuid>>>(first)?
        .map(|id| -> rusqlite::Result<VenueRef> {
            Ok(VenueRef {
                id: id.0,
                name: row.get(first + 1)?,
                deleted: row.get::<_, u8>(first + 2)? == 1,
            })
        })
        .transpose()
}

struct MatchRow {
    id: Uuid,
    start: NaiveDateTime,
    scheduled: Option<u16>,
    a: Side,
    b: Option<Side>,
    b_name: Option<String>,
    played: bool,
    wins: Option<(u8, u8)>,
    span: Option<(NaiveDateTime, NaiveDateTime)>,
    venue: Option<VenueRef>,
}

fn matches(tx: &Transaction, period: &Period) -> Result<Vec<MatchRow>, AppError> {
    tx.prepare(
        "SELECT m.id, m.start, m.duration_min, m.team_a, ta.name, m.team_b, tb.name,
                m.team_b_name,
                EXISTS (SELECT 1 FROM sets s WHERE s.match_id = m.id AND s.deleted = 0),
                ms.a_wins, ms.b_wins,
                (SELECT MIN(e.timestamp) FROM events e JOIN sets s
                   ON s.match_id = e.match_id AND s.number = e.set_number
                  WHERE e.match_id = m.id AND e.deleted = 0 AND s.deleted = 0),
                (SELECT MAX(e.timestamp) FROM events e JOIN sets s
                   ON s.match_id = e.match_id AND s.number = e.set_number
                  WHERE e.match_id = m.id AND e.deleted = 0 AND s.deleted = 0),
                v.id, v.name, v.deleted
         FROM matches m
           LEFT JOIN venues v ON v.id = m.venue_id
           JOIN teams ta ON ta.id = m.team_a
           LEFT JOIN teams tb ON tb.id = m.team_b
           LEFT JOIN match_summary ms ON ms.match_id = m.id
         WHERE m.deleted = 0 AND ta.deleted = 0
           AND substr(m.start, 1, 10) >= ?1 AND substr(m.start, 1, 10) < ?2",
    )
    .map_err(sqlite)?
    .query_map(params![Sql(period.from), Sql(period.to)], |row| {
        let local = |i| -> rusqlite::Result<Option<NaiveDateTime>> {
            Ok(row
                .get::<_, Option<Sql<DateTime<FixedOffset>>>>(i)?
                .map(|t| t.0.naive_local()))
        };
        let wins = match (row.get::<_, Option<u8>>(9)?, row.get::<_, Option<u8>>(10)?) {
            (Some(a), Some(b)) => Some((a, b)),
            _ => None,
        };
        let b = match (
            row.get::<_, Option<Sql<Uuid>>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ) {
            (Some(id), Some(name)) => Some((id.0, name)),
            _ => None,
        };
        let span = match (local(11)?, local(12)?) {
            (Some(first), Some(last)) => Some((first, last)),
            _ => None,
        };
        Ok(MatchRow {
            id: row.get::<_, Sql<Uuid>>(0)?.0,
            start: row.get::<_, Sql<DateTime<FixedOffset>>>(1)?.0.naive_local(),
            scheduled: row.get(2)?,
            a: (row.get::<_, Sql<Uuid>>(3)?.0, row.get(4)?),
            b,
            b_name: row.get(7)?,
            played: row.get(8)?,
            wins,
            span,
            venue: venue_ref(row, 13)?,
        })
    })
    .map_err(sqlite)?
    .collect::<Result<_, _>>()
    .map_err(sqlite)
}

/// The match as an event of the team `me` (`a_side`: it is A), against `other`.
fn event_of(m: &MatchRow, me: &Side, a_side: bool, other: Option<&Side>) -> AgendaEvent {
    let state = match m.played {
        false => MatchState::Scheduled,
        true => MatchState::Played(m.wins.map(|(a, b)| score_of(a_side, a, b))),
    };
    let minutes = match (m.played, m.span) {
        (true, Some((first, last))) => played_minutes(first, last),
        _ => m.scheduled.unwrap_or(DEFAULT_MATCH_MINUTES),
    };
    AgendaEvent {
        team_id: me.0,
        team_name: me.1.clone(),
        start: m.start,
        minutes,
        item: AgendaItem::Match {
            id: m.id,
            opponent: other
                .map(|o| o.1.clone())
                .or_else(|| m.b_name.clone())
                .unwrap_or_default(),
            opponent_team: other.map(|o| o.0),
            state,
        },
        venue: m.venue.clone(),
        shared_venue: false,
    }
}

fn match_events(rows: &[MatchRow], team: Option<&Uuid>) -> Vec<AgendaEvent> {
    rows.iter()
        .flat_map(|m| match team {
            Some(t) if *t != m.a.0 => {
                m.b.iter()
                    .filter(|b| b.0 == *t)
                    .map(|b| event_of(m, b, false, Some(&m.a)))
                    .collect()
            }
            _ => vec![event_of(m, &m.a, true, m.b.as_ref())],
        })
        .collect()
}

#[async_trait]
impl TrainingAgenda for SqliteTraining {
    async fn agenda(
        &self,
        team_id: Option<&Uuid>,
        period: Period,
        now: NaiveDateTime,
    ) -> Result<Vec<AgendaEvent>, AppError> {
        self.db().read(|tx| {
            let mut events = sessions(tx, team_id, &period, now)?;
            events.extend(match_events(&matches(tx, &period)?, team_id));
            let sharing = shared(&occupations(tx, None, Some(&period))?);
            events.iter_mut().for_each(|e| {
                e.shared_venue = e.venue.is_some() && sharing.contains(&e.item.id());
            });
            events.sort_by_key(|e| (e.start, e.minutes, e.team_name.clone()));
            Ok(events)
        })
    }
}
