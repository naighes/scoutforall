use crate::{
    domain::{Event, Side, adapters::SetRows, sql::Sql},
    errors::{AppError, IOError, MatchError},
    localization::Language,
    providers::{
        set_reader::SetReader,
        set_writer::SetWriter,
        sqlite::{
            Database,
            matches::match_uuid,
            not_found,
            rows::{
                events, insert_event, next_seq, remove_event, set_rows, update_event, write_set,
            },
            sqlite,
            summary::refresh,
        },
    },
    shapes::{enums::TeamSideEnum, r#match::MatchEntry, set::SetEntry, snapshot::EventEntry},
};
use async_trait::async_trait;
use rusqlite::{OptionalExtension, Transaction, params};
use uuid::Uuid;

/// The sets numbered from 1 without gaps, as the files want them: a
/// match with a gap can't be read.
pub(crate) fn numbered(
    sets: Vec<SetEntry>,
    language: &Language,
    match_id: &str,
) -> Result<Vec<SetEntry>, AppError> {
    let gap = sets
        .iter()
        .zip(1..)
        .any(|(set, number)| set.set_number != number);
    match gap {
        true => Err(AppError::Match(MatchError::LoadSetError(format!(
            "{} ({match_id})",
            language.labels().wrong_set_numbering
        )))),
        false => Ok(sets),
    }
}

pub struct SqliteSetReader {
    db: Database,
    language: Language,
}

impl SqliteSetReader {
    pub fn new(db: Database, language: Language) -> Self {
        Self { db, language }
    }
}

#[async_trait]
impl SetReader for SqliteSetReader {
    async fn read_all(&self, m: &MatchEntry) -> Result<Vec<SetEntry>, AppError> {
        let id = match_uuid(&m.id)?;
        let sets = self.db.read(|tx| {
            set_rows(tx, id)?
                .iter()
                .map(SetRows::entry)
                .collect::<Result<Vec<_>, _>>()
        })?;
        numbered(sets, &self.language, &m.id)
    }
}

pub struct SqliteSetWriter {
    db: Database,
    language: Language,
}

impl SqliteSetWriter {
    pub fn new(db: Database, language: Language) -> Self {
        Self { db, language }
    }
}

/// Whether the set is saved: `Some(true)` removed.
fn saved(tx: &Transaction, match_id: Uuid, set_number: u8) -> Result<Option<bool>, AppError> {
    tx.query_row(
        "SELECT deleted FROM sets WHERE match_id = ?1 AND number = ?2",
        params![Sql(match_id), set_number],
        |row| row.get(0),
    )
    .optional()
    .map_err(sqlite)
}

/// The set saved and not removed, or an error.
fn played(tx: &Transaction, match_id: Uuid, set_number: u8) -> Result<(), AppError> {
    match saved(tx, match_id, set_number)? {
        Some(false) => Ok(()),
        _ => Err(not_found(&format!("set {set_number} of match {match_id}"))),
    }
}

/// The events of A, appended after the last place of the set.
fn append(
    tx: &Transaction,
    match_id: Uuid,
    set_number: u8,
    entries: &[EventEntry],
) -> Result<(), AppError> {
    let first = next_seq(tx, match_id, set_number)?;
    entries.iter().zip(first..).try_for_each(|(entry, seq)| {
        insert_event(
            tx,
            &Event {
                id: Uuid::new_v4(),
                match_id,
                set_number,
                seq,
                side: Side::A,
                entry: entry.clone(),
                deleted: false,
            },
        )
    })
}

/// The events of A of the set, not removed, in order.
fn events_of_a(tx: &Transaction, match_id: Uuid, set_number: u8) -> Result<Vec<Event>, AppError> {
    Ok(events(tx, match_id, set_number)?
        .into_iter()
        .filter(|e| e.side == Side::A)
        .collect())
}

#[async_trait]
impl SetWriter for SqliteSetWriter {
    /// A removed set with the same number comes back as this one, its old
    /// events left removed.
    async fn create(
        &self,
        m: &MatchEntry,
        set_number: u8,
        serving_team: TeamSideEnum,
        positions: [Uuid; 6],
        libero: Uuid,
        fallback_libero: Option<Uuid>,
        setter: Uuid,
        events: Vec<EventEntry>,
    ) -> Result<SetEntry, AppError> {
        let id = match_uuid(&m.id)?;
        let set = SetEntry {
            events,
            ..SetEntry::new(
                set_number,
                serving_team,
                positions,
                libero,
                fallback_libero,
                setter,
            )?
        };
        let rows = SetRows::of(&set, id)?;
        self.db.write(|tx| match saved(tx, id, set_number)? {
            Some(false) => Err(AppError::IO(IOError::Msg(
                self.language.labels().match_already_exists.to_string(),
            ))),
            _ => {
                write_set(tx, &rows)?;
                append(tx, id, set_number, &set.events)?;
                refresh(tx, id)
            }
        })?;
        Ok(set)
    }

    async fn append_event(
        &self,
        m: &MatchEntry,
        set_number: u8,
        event: &EventEntry,
    ) -> Result<(), AppError> {
        let id = match_uuid(&m.id)?;
        self.db.write(|tx| {
            played(tx, id, set_number)?;
            append(tx, id, set_number, std::slice::from_ref(event))?;
            refresh(tx, id)
        })
    }

    /// The last event of A, marked removed: it keeps its place.
    async fn remove_last_event(
        &self,
        m: &MatchEntry,
        set_number: u8,
    ) -> Result<Option<EventEntry>, AppError> {
        let id = match_uuid(&m.id)?;
        self.db.write(|tx| {
            played(tx, id, set_number)?;
            let removed = events_of_a(tx, id, set_number)?
                .pop()
                .map(|last| remove_event(tx, last.id).map(|()| last.entry))
                .transpose()?;
            refresh(tx, id)?;
            Ok(removed)
        })
    }

    /// The set as `set` has it: its events in their places, a changed one
    /// updated in its row, the ones beyond removed or appended.
    async fn update(&self, m: &MatchEntry, set: &SetEntry) -> Result<(), AppError> {
        let id = match_uuid(&m.id)?;
        let rows = SetRows::of(set, id)?;
        self.db.write(|tx| {
            let saved = events_of_a(tx, id, set.set_number)?;
            write_set(tx, &rows)?;
            saved
                .iter()
                .zip(&set.events)
                .filter(|(old, new)| old.entry != **new)
                .try_for_each(|(old, new)| update_event(tx, old.id, new))?;
            saved
                .iter()
                .skip(set.events.len())
                .try_for_each(|old| remove_event(tx, old.id))?;
            append(
                tx,
                id,
                set.set_number,
                set.events.get(saved.len()..).unwrap_or_default(),
            )?;
            refresh(tx, id)
        })
    }

    /// Marked removed, with its events.
    async fn remove(&self, m: &MatchEntry, set_number: u8) -> Result<(), AppError> {
        let id = match_uuid(&m.id)?;
        self.db.write(|tx| {
            tx.execute(
                "UPDATE events SET deleted = 1 WHERE match_id = ?1 AND set_number = ?2",
                params![Sql(id), set_number],
            )
            .and_then(|_| {
                tx.execute(
                    "UPDATE sets SET deleted = 1 WHERE match_id = ?1 AND number = ?2",
                    params![Sql(id), set_number],
                )
            })
            .map_err(sqlite)?;
            refresh(tx, id)
        })
    }
}
