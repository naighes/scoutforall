use crate::{
    errors::{AppError, IOError},
    localization::current_labels,
    providers::{
        fs::path::{get_match_folder_path, get_set_descriptor_file_path, get_set_events_file_path},
        set_writer::SetWriter,
    },
    shapes::{enums::TeamSideEnum, r#match::MatchEntry, set::SetEntry, snapshot::EventEntry},
};
use async_trait::async_trait;
use csv::ReaderBuilder;
use csv::WriterBuilder;
use serde_json::to_vec_pretty;
use std::path::PathBuf;
use std::{fs::OpenOptions, path::Path};
use tokio::{
    fs::{create_dir_all, write, File},
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
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(events_path)
                .map_err(|e| AppError::IO(IOError::from(e)))?;
            let mut writer = WriterBuilder::new().has_headers(false).from_writer(file);
            writer
                .serialize(event)
                .map_err(|e| AppError::IO(IOError::from(e)))?;
            writer.flush().map_err(|e| AppError::IO(IOError::from(e)))
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
            let mut reader = ReaderBuilder::new()
                .has_headers(false)
                .from_path(&path_clone)
                .map_err(|e| AppError::IO(IOError::from(e)))?;
            let mut records: Vec<EventEntry> = reader
                .deserialize()
                .collect::<Result<Vec<EventEntry>, _>>()
                .map_err(|e| AppError::IO(IOError::from(e)))?;
            let last = records.pop();
            let file = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&path_clone)
                .map_err(|e| AppError::IO(IOError::from(e)))?;
            let mut writer = WriterBuilder::new().has_headers(false).from_writer(file);
            for rec in &records {
                writer
                    .serialize(rec)
                    .map_err(|e| AppError::IO(IOError::from(e)))?;
            }
            writer.flush().map_err(|e| AppError::IO(IOError::from(e)))?;
            Ok(last)
        })
        .await
        .map_err(|e| AppError::IO(IOError::Msg(format!("tokio join error: {}", e))))?
    }
}
