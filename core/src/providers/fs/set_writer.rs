use crate::{
    errors::{AppError, IOError},
    localization::current_labels,
    providers::{
        fs::{
            path::{get_match_folder_path, get_set_descriptor_file_path, get_set_events_file_path},
            set_reader::read_events,
        },
        set_writer::SetWriter,
    },
    shapes::{enums::TeamSideEnum, r#match::MatchEntry, set::SetEntry, snapshot::EventEntry},
};
use async_trait::async_trait;
use csv::{ReaderBuilder, StringRecord, WriterBuilder};
use serde_json::to_vec_pretty;
use std::path::PathBuf;
use std::{fs::OpenOptions, path::Path};
use tokio::{
    fs::{File, create_dir_all, write},
    task::spawn_blocking,
};
use uuid::Uuid;

pub struct FileSystemSetWriter(PathBuf);

impl FileSystemSetWriter {
    pub fn new(base_path: &Path) -> Self {
        Self(base_path.to_path_buf())
    }

    async fn save_set_file(set: &SetEntry, path: &PathBuf) -> Result<(), AppError> {
        let json = to_vec_pretty(set).map_err(|e| AppError::IO(IOError::from(e)))?;
        write(path, json)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))
    }
}

#[async_trait]
impl SetWriter for FileSystemSetWriter {
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
        get_match_folder_path(&self.0, &m.team.id, &m.id)?;
        let mut set = SetEntry::new(
            set_number,
            serving_team,
            positions,
            libero,
            fallback_libero,
            setter,
        )?;
        let descriptor_path = get_set_descriptor_file_path(&self.0, &m.team.id, &m.id, set_number)?;
        if descriptor_path.exists() {
            return Err(AppError::IO(IOError::Msg(
                current_labels().match_already_exists.to_string(),
            )));
        }
        create_dir_all(descriptor_path.parent().unwrap())
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        Self::save_set_file(&set, &descriptor_path).await?;
        let events_path = get_set_events_file_path(&self.0, &m.team.id, &m.id, set_number)?;
        File::create(events_path)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        for event in &events {
            self.append_event(m, set_number, event).await?;
        }
        set.events = events;
        Ok(set)
    }

    async fn append_event(
        &self,
        m: &MatchEntry,
        set_number: u8,
        event: &EventEntry,
    ) -> Result<(), AppError> {
        let events_path = get_set_events_file_path(&self.0, &m.team.id, &m.id, set_number)?;
        let event = event.clone();
        spawn_blocking(move || {
            // the older versions read a file only when all its rows have the
            // same columns: the rows saved without the technique get it
            has_rows_without_technique(&events_path).and_then(|rewrite| match rewrite {
                true => read_events(&events_path).and_then(|mut events| {
                    events.push(event);
                    write_events(&events_path, &events)
                }),
                false => append(&events_path, &event),
            })
        })
        .await
        .map_err(|e| AppError::IO(IOError::Msg(format!("tokio join error: {}", e))))?
    }

    async fn remove_last_event(
        &self,
        m: &MatchEntry,
        set_number: u8,
    ) -> Result<Option<EventEntry>, AppError> {
        let path = get_set_events_file_path(&self.0, &m.team.id, &m.id, set_number)?;
        let path_clone = path.clone();
        spawn_blocking(move || {
            read_events(&path_clone).and_then(|mut events| {
                let last = events.pop();
                write_events(&path_clone, &events).map(|_| last)
            })
        })
        .await
        .map_err(|e| AppError::IO(IOError::Msg(format!("tokio join error: {}", e))))?
    }
}

/// The columns of a saved event, the technique included.
const EVENT_COLUMNS: usize = 6;

/// Whether some events were saved without the technique, by v0.4.3 or older.
fn has_rows_without_technique(path: &Path) -> Result<bool, AppError> {
    match path.exists() {
        false => Ok(false),
        true => ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_path(path)
            .and_then(|mut reader| {
                reader
                    .records()
                    .collect::<Result<Vec<StringRecord>, csv::Error>>()
            })
            .map(|records| records.iter().any(|r| r.len() < EVENT_COLUMNS))
            .map_err(|e| AppError::IO(IOError::from(e))),
    }
}

fn append(path: &Path, event: &EventEntry) -> Result<(), AppError> {
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| AppError::IO(IOError::from(e)))?;
    let mut writer = WriterBuilder::new().has_headers(false).from_writer(file);
    writer
        .serialize(event)
        .map_err(|e| AppError::IO(IOError::from(e)))?;
    writer.flush().map_err(|e| AppError::IO(IOError::from(e)))
}

/// Saves the events in place of the ones in the file.
fn write_events(path: &Path, events: &[EventEntry]) -> Result<(), AppError> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|e| AppError::IO(IOError::from(e)))?;
    let mut writer = WriterBuilder::new().has_headers(false).from_writer(file);
    events
        .iter()
        .try_for_each(|event| writer.serialize(event))
        .map_err(|e| AppError::IO(IOError::from(e)))?;
    writer.flush().map_err(|e| AppError::IO(IOError::from(e)))
}
