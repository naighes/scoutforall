use crate::{
    errors::{AppError, IOError},
    localization::Language,
    providers::{
        set_writer::SetWriter,
        store::{set_reader::read_events, write_json},
    },
    shapes::{enums::TeamSideEnum, r#match::MatchEntry, set::SetEntry, snapshot::EventEntry},
    storage::{Store, layout},
};
use async_trait::async_trait;
use csv::{ReaderBuilder, StringRecord, WriterBuilder};
use std::sync::Arc;
use uuid::Uuid;

pub struct StoreSetWriter {
    store: Arc<dyn Store>,
    language: Language,
}

impl StoreSetWriter {
    pub fn new(store: Arc<dyn Store>, language: Language) -> Self {
        Self { store, language }
    }
}

#[async_trait]
impl SetWriter for StoreSetWriter {
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
        let mut set = SetEntry::new(
            set_number,
            serving_team,
            positions,
            libero,
            fallback_libero,
            setter,
        )?;
        let descriptor_key = layout::set_descriptor(&m.team.id, &m.id, set_number);
        if self.store.exists(&descriptor_key).await? {
            return Err(AppError::IO(IOError::Msg(
                self.language.labels().match_already_exists.to_string(),
            )));
        }
        write_json(&*self.store, &descriptor_key, &set).await?;
        let events_key = layout::set_events(&m.team.id, &m.id, set_number);
        self.store.write(&events_key, &events_csv(&events)?).await?;
        set.events = events;
        Ok(set)
    }

    async fn append_event(
        &self,
        m: &MatchEntry,
        set_number: u8,
        event: &EventEntry,
    ) -> Result<(), AppError> {
        let key = layout::set_events(&m.team.id, &m.id, set_number);
        let saved = self.store.read(&key).await?.unwrap_or_default();
        // the older versions read a file only when all its rows have the
        // same columns: the rows saved without the technique get it
        match has_rows_without_technique(&saved)? {
            true => {
                let mut events = read_events(&saved)?;
                events.push(event.clone());
                self.store.write(&key, &events_csv(&events)?).await
            }
            false => {
                self.store
                    .append(&key, &events_csv(std::slice::from_ref(event))?)
                    .await
            }
        }
    }

    async fn remove_last_event(
        &self,
        m: &MatchEntry,
        set_number: u8,
    ) -> Result<Option<EventEntry>, AppError> {
        let key = layout::set_events(&m.team.id, &m.id, set_number);
        let saved = self.store.read(&key).await?.unwrap_or_default();
        let mut events = read_events(&saved)?;
        let last = events.pop();
        self.store.write(&key, &events_csv(&events)?).await?;
        Ok(last)
    }

    async fn update(&self, m: &MatchEntry, set: &SetEntry) -> Result<(), AppError> {
        // the descriptor first: it gets the rules the events replay with
        write_json(
            &*self.store,
            &layout::set_descriptor(&m.team.id, &m.id, set.set_number),
            set,
        )
        .await?;
        self.store
            .write(
                &layout::set_events(&m.team.id, &m.id, set.set_number),
                &events_csv(&set.events)?,
            )
            .await
    }
}

/// The columns of a saved event, the technique included.
const EVENT_COLUMNS: usize = 6;

/// Whether some events were saved without the technique, by v0.4.3 or older.
fn has_rows_without_technique(csv: &[u8]) -> Result<bool, AppError> {
    ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(csv)
        .records()
        .collect::<Result<Vec<StringRecord>, csv::Error>>()
        .map(|records| records.iter().any(|r| r.len() < EVENT_COLUMNS))
        .map_err(|e| AppError::IO(IOError::from(e)))
}

/// The rows of the events, as saved.
fn events_csv(events: &[EventEntry]) -> Result<Vec<u8>, AppError> {
    let mut writer = WriterBuilder::new().has_headers(false).from_writer(vec![]);
    events
        .iter()
        .try_for_each(|event| writer.serialize(event))
        .map_err(|e| AppError::IO(IOError::from(e)))?;
    writer
        .into_inner()
        .map_err(|e| AppError::IO(IOError::Msg(e.to_string())))
}
