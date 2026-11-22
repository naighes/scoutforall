//! The gyms in the database (TODO/021 §13): common to every team. The pin is
//! a column, the same on every device. The sessions and matches that hold a
//! gym are read here too, to tell the clashes of two teams in the same one.

use crate::{
    domain::sql::Sql,
    dvw::import::normalized,
    errors::AppError,
    providers::sqlite::training::{SESSION_COLUMNS, read_session, session_from_row},
    providers::sqlite::{Database, not_found, sqlite},
    training::{
        SessionStatus,
        agenda::Period,
        duplicate::{DuplicateCycleInput, DuplicateInput},
        overlap::Slot,
        series::SeriesInput,
        venue::{Occupation, Venue, VenueClash, VenueInput, clashes_in, shared},
    },
};
use chrono::{DateTime, Duration, FixedOffset, NaiveDateTime};
use rusqlite::{Row, Transaction, params};
use uuid::Uuid;

pub struct SqliteVenues(Database);

const VENUE_COLUMNS: &str =
    "id, name, address, map_url, latitude, longitude, notes, pinned, deleted";

pub(crate) fn venue_from_row(row: &Row) -> rusqlite::Result<Venue> {
    Ok(Venue {
        id: row.get::<_, Sql<Uuid>>(0)?.0,
        name: row.get(1)?,
        address: row.get(2)?,
        map_url: row.get(3)?,
        coordinates: match (row.get::<_, Option<f64>>(4)?, row.get::<_, Option<f64>>(5)?) {
            (Some(lat), Some(lon)) => Some((lat, lon)),
            _ => None,
        },
        notes: row.get(6)?,
        pinned: row.get::<_, u8>(7)? == 1,
        deleted: row.get::<_, u8>(8)? == 1,
    })
}

/// The gym, removed or not.
pub(crate) fn read_venue(tx: &Transaction, id: &Uuid) -> Result<Venue, AppError> {
    tx.query_row(
        &format!("SELECT {VENUE_COLUMNS} FROM venues WHERE id = ?1"),
        [Sql(*id)],
        venue_from_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => not_found(&format!("venue {id}")),
        e => sqlite(e),
    })
}

fn insert_venue(tx: &Transaction, input: &VenueInput) -> Result<Venue, AppError> {
    let id = Uuid::new_v4();
    tx.execute(
        "INSERT INTO venues (id, name, address, map_url, latitude, longitude, notes, pinned)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            Sql(id),
            input.name,
            input.address,
            input.map_url,
            input.coordinates.map(|(lat, _)| lat),
            input.coordinates.map(|(_, lon)| lon),
            input.notes,
            input.pinned,
        ],
    )
    .map_err(sqlite)?;
    read_venue(tx, &id)
}

/// The gym not removed whose name is `name` (compared as the names of the
/// DataVolley files are: no case, accents or extra spaces), or a new one,
/// not pinned, with `address`.
pub(crate) fn find_or_create(
    tx: &Transaction,
    name: &str,
    address: &str,
) -> Result<Venue, AppError> {
    let wanted = normalized(name);
    let found = tx
        .prepare(&format!(
            "SELECT {VENUE_COLUMNS} FROM venues WHERE deleted = 0 ORDER BY pinned DESC, id"
        ))
        .map_err(sqlite)?
        .query_map([], venue_from_row)
        .map_err(sqlite)?
        .collect::<Result<Vec<Venue>, _>>()
        .map_err(sqlite)?
        .into_iter()
        .find(|v| normalized(&v.name) == wanted);
    match found {
        Some(venue) => Ok(venue),
        None => insert_venue(
            tx,
            &VenueInput {
                name: name.split_whitespace().collect::<Vec<_>>().join(" "),
                address: address.split_whitespace().collect::<Vec<_>>().join(" "),
                map_url: String::new(),
                coordinates: None,
                notes: String::new(),
                pinned: false,
            },
        ),
    }
}

/// The sessions (not cancelled) and the matches in programma that hold a
/// gym, starting in `[from, to)` (all when none).
pub(crate) fn occupations(
    tx: &Transaction,
    venue: Option<&Uuid>,
    period: Option<&Period>,
) -> Result<Vec<Occupation>, AppError> {
    let (from, to) = match period {
        Some(p) => (Some(Sql(p.from)), Some(Sql(p.to))),
        None => (None, None),
    };
    let mut found: Vec<Occupation> = tx
        .prepare(
            "SELECT s.venue_id, s.id, s.team_id, t.name, s.start_local, s.duration_min
             FROM training_sessions s JOIN teams t ON t.id = s.team_id
             WHERE s.deleted = 0 AND t.deleted = 0 AND s.status <> ?4
               AND s.venue_id IS NOT NULL AND (?1 IS NULL OR s.venue_id = ?1)
               AND (?2 IS NULL OR substr(s.start_local, 1, 10) >= ?2)
               AND (?3 IS NULL OR substr(s.start_local, 1, 10) < ?3)",
        )
        .map_err(sqlite)?
        .query_map(
            params![venue.map(|v| Sql(*v)), from, to, SessionStatus::Cancelled],
            |row| {
                Ok(Occupation {
                    venue: row.get::<_, Sql<Uuid>>(0)?.0,
                    item: row.get::<_, Sql<Uuid>>(1)?.0,
                    teams: vec![(row.get::<_, Sql<Uuid>>(2)?.0, row.get(3)?)],
                    slot: Slot::session(row.get::<_, Sql<NaiveDateTime>>(4)?.0, row.get(5)?),
                })
            },
        )
        .map_err(sqlite)?
        .collect::<Result<_, _>>()
        .map_err(sqlite)?;
    let matches: Vec<Occupation> = tx
        .prepare(
            "SELECT m.venue_id, m.id, m.team_a, ta.name, m.team_b, tb.name, m.start,
                    m.duration_min
             FROM matches m JOIN teams ta ON ta.id = m.team_a
                  LEFT JOIN teams tb ON tb.id = m.team_b AND tb.deleted = 0
             WHERE m.deleted = 0 AND ta.deleted = 0 AND m.venue_id IS NOT NULL
               AND (?1 IS NULL OR m.venue_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM sets s WHERE s.match_id = m.id AND s.deleted = 0)",
        )
        .map_err(sqlite)?
        .query_map(params![venue.map(|v| Sql(*v))], |row| {
            let b = match (
                row.get::<_, Option<Sql<Uuid>>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ) {
                (Some(id), Some(name)) => Some((id.0, name)),
                _ => None,
            };
            Ok(Occupation {
                venue: row.get::<_, Sql<Uuid>>(0)?.0,
                item: row.get::<_, Sql<Uuid>>(1)?.0,
                teams: std::iter::once((row.get::<_, Sql<Uuid>>(2)?.0, row.get(3)?))
                    .chain(b)
                    .collect(),
                slot: Slot::match_of(
                    row.get::<_, Sql<DateTime<FixedOffset>>>(6)?.0.naive_local(),
                    row.get(7)?,
                ),
            })
        })
        .map_err(sqlite)?
        .collect::<Result<_, _>>()
        .map_err(sqlite)?;
    found.extend(matches.into_iter().filter(|o| match period {
        Some(p) => p.contains(o.slot.start.date()),
        None => true,
    }));
    found.sort_by_key(|o| (o.slot.start, o.slot.minutes));
    Ok(found)
}

impl SqliteVenues {
    pub fn new(db: Database) -> Self {
        Self(db)
    }

    /// The gyms not removed: the pinned first, then by name.
    pub fn venues(&self) -> Result<Vec<Venue>, AppError> {
        self.0.read(|tx| {
            tx.prepare(&format!(
                "SELECT {VENUE_COLUMNS} FROM venues WHERE deleted = 0
                 ORDER BY pinned DESC, name COLLATE NOCASE, id"
            ))
            .map_err(sqlite)?
            .query_map([], venue_from_row)
            .map_err(sqlite)?
            .collect::<Result<_, _>>()
            .map_err(sqlite)
        })
    }

    /// The gym, removed or not (a session keeps the one it had).
    pub fn venue(&self, id: &Uuid) -> Result<Venue, AppError> {
        self.0.read(|tx| read_venue(tx, id))
    }

    /// What a form of `team` offers whose name holds `query` (case aside; all
    /// when empty): the pinned ones first, then those the team used (the most
    /// used first), then the others; each by name. None removed.
    pub fn choices(&self, team: &Uuid, query: &str) -> Result<Vec<Venue>, AppError> {
        self.0.read(|tx| {
            tx.prepare(&format!(
                "SELECT {VENUE_COLUMNS} FROM venues v WHERE deleted = 0
                 ORDER BY pinned DESC,
                          (SELECT count(*) FROM training_sessions s
                           WHERE s.venue_id = v.id AND s.team_id = ?1 AND s.deleted = 0) DESC,
                          name COLLATE NOCASE, id"
            ))
            .map_err(sqlite)?
            .query_map([Sql(*team)], venue_from_row)
            .map_err(sqlite)?
            .collect::<Result<Vec<Venue>, _>>()
            .map_err(sqlite)
            .map(|all| {
                let wanted = query.trim().to_lowercase();
                all.into_iter()
                    .filter(|v| v.name.to_lowercase().contains(&wanted))
                    .collect()
            })
        })
    }

    pub fn create(&self, input: VenueInput) -> Result<Venue, AppError> {
        self.0.write(|tx| insert_venue(tx, &input))
    }

    pub fn update(&self, id: &Uuid, input: VenueInput) -> Result<Venue, AppError> {
        self.0.write(|tx| {
            let changed = tx
                .execute(
                    "UPDATE venues SET name = ?2, address = ?3, map_url = ?4, latitude = ?5,
                         longitude = ?6, notes = ?7, pinned = ?8
                     WHERE id = ?1 AND deleted = 0",
                    params![
                        Sql(*id),
                        input.name,
                        input.address,
                        input.map_url,
                        input.coordinates.map(|(lat, _)| lat),
                        input.coordinates.map(|(_, lon)| lon),
                        input.notes,
                        input.pinned,
                    ],
                )
                .map_err(sqlite)?;
            match changed {
                0 => Err(not_found(&format!("venue {id}"))),
                _ => read_venue(tx, id),
            }
        })
    }

    /// Only the pin: the same on every device.
    pub fn set_pinned(&self, id: &Uuid, pinned: bool) -> Result<Venue, AppError> {
        self.0.write(|tx| {
            let changed = tx
                .execute(
                    "UPDATE venues SET pinned = ?2 WHERE id = ?1 AND deleted = 0",
                    params![Sql(*id), pinned],
                )
                .map_err(sqlite)?;
            match changed {
                0 => Err(not_found(&format!("venue {id}"))),
                _ => read_venue(tx, id),
            }
        })
    }

    /// Marks the gym removed (D7): the sessions and matches keep it.
    pub fn remove(&self, id: &Uuid) -> Result<(), AppError> {
        self.0.write(|tx| {
            match tx
                .execute(
                    "UPDATE venues SET deleted = 1 WHERE id = ?1 AND deleted = 0",
                    [Sql(*id)],
                )
                .map_err(sqlite)?
            {
                0 => Err(not_found(&format!("venue {id}"))),
                _ => Ok(()),
            }
        })
    }

    /// What holds the gym in the period, by start, each with whether another
    /// team holds it at the same time (the card of the gym, its week).
    pub fn week(&self, venue: &Uuid, period: &Period) -> Result<Vec<(Occupation, bool)>, AppError> {
        self.0.read(|tx| {
            let all = occupations(tx, Some(venue), Some(period))?;
            let sharing = shared(&all);
            Ok(all
                .into_iter()
                .map(|o| {
                    let overlapping = sharing.contains(&o.item);
                    (o, overlapping)
                })
                .collect())
        })
    }

    /// The occupations of the other teams that clash with `windows` in
    /// `venue`: what a save warns of. `own` are the teams that save (a
    /// session, or the two of a match): what is theirs is not reported.
    pub fn clashes(
        &self,
        venue: &Uuid,
        windows: &[(NaiveDateTime, u16)],
        own: &[Uuid],
    ) -> Result<Vec<VenueClash>, AppError> {
        self.0
            .read(|tx| occupations(tx, Some(venue), None))
            .map(|all| clashes_in(venue, windows, own, &all))
    }
}

/// The clashes of `items` (gym, start, minutes) of the teams `own`, in the
/// order of the items.
fn clashes_of(
    tx: &Transaction,
    own: &[Uuid],
    items: &[(Uuid, NaiveDateTime, u16)],
) -> Result<Vec<VenueClash>, AppError> {
    let all = occupations(tx, None, None)?;
    Ok(items
        .iter()
        .flat_map(|(venue, start, minutes)| clashes_in(venue, &[(*start, *minutes)], own, &all))
        .collect())
}

impl SqliteVenues {
    /// The sessions a repetition would make that hold a gym another team
    /// holds then: the dialog lists them (a warning, not a conflict).
    pub fn series_clashes(
        &self,
        team: &Uuid,
        input: &SeriesInput,
    ) -> Result<Vec<VenueClash>, AppError> {
        let windows = input.generate().map_err(|e| {
            AppError::IO(crate::errors::IOError::Msg(format!(
                "invalid series: {e:?}"
            )))
        })?;
        let items: Vec<(Uuid, NaiveDateTime, u16)> = windows
            .into_iter()
            .filter_map(|(start, minutes)| input.venue_of(start).map(|v| (v, start, minutes)))
            .collect();
        self.0.read(|tx| clashes_of(tx, &[*team], &items))
    }

    /// The same for the copy of a session.
    pub fn duplicate_clashes(
        &self,
        session_id: &Uuid,
        input: &DuplicateInput,
    ) -> Result<Vec<VenueClash>, AppError> {
        self.0.read(|tx| {
            let session = read_session(tx, session_id)?;
            let items: Vec<(Uuid, NaiveDateTime, u16)> = session
                .venue_id
                .map(|v| (v, input.start, session.duration_min))
                .into_iter()
                .collect();
            clashes_of(tx, &[session.team_id], &items)
        })
    }

    /// The same for the copies of a microcycle.
    pub fn microcycle_clashes(
        &self,
        microcycle_id: &Uuid,
        input: &DuplicateCycleInput,
    ) -> Result<Vec<VenueClash>, AppError> {
        self.0.read(|tx| {
            let sessions = tx
                .prepare(&format!(
                    "SELECT {SESSION_COLUMNS} FROM training_sessions
                     WHERE microcycle_id = ?1 AND deleted = 0 ORDER BY start_local, id"
                ))
                .map_err(sqlite)?
                .query_map([Sql(*microcycle_id)], session_from_row)
                .map_err(sqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sqlite)?;
            let own: Vec<Uuid> = sessions.iter().map(|s| s.team_id).take(1).collect();
            let items: Vec<(Uuid, NaiveDateTime, u16)> = (1..=i64::from(input.copies))
                .map(|k| Duration::weeks(k * i64::from(input.shift_weeks)))
                .flat_map(|shift| {
                    sessions.iter().filter_map(move |s| {
                        s.venue_id
                            .zip(s.start.checked_add_signed(shift))
                            .map(|(v, start)| (v, start, s.duration_min))
                    })
                })
                .collect();
            clashes_of(tx, &own, &items)
        })
    }
}
