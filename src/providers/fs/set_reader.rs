use crate::{
    errors::{AppError, IOError, MatchError},
    localization::current_labels,
    providers::{fs::path::get_match_folder_path, set_reader::SetReader},
    shapes::{r#match::MatchEntry, set::SetEntry, snapshot::EventEntry},
};
use async_trait::async_trait;
use csv::ReaderBuilder;
use futures::future::try_join_all;
use regex::Regex;
use serde_json::from_str;
use std::path::PathBuf;
use std::{fs::File, path::Path};
use tokio::{
    fs::{read_dir, read_to_string, DirEntry},
    task::spawn_blocking,
};

pub struct FileSystemSetReader(PathBuf);

impl FileSystemSetReader {
    pub fn new(base_path: &Path) -> Self {
        Self(base_path.to_path_buf())
    }

    async fn parse_set(
        entry: &DirEntry,
        set_file_regex: &Regex,
    ) -> Option<Result<SetEntry, AppError>> {
        let path = entry.path();
        if !path.is_file() {
            return None;
        }
        let filename = path.file_name()?.to_str()?;
        let caps = set_file_regex.captures(filename)?;
        let set_number: u8 = caps.get(1)?.as_str().parse().ok()?;
        let json_str = read_to_string(&path).await.ok()?;
        let mut set: SetEntry = from_str(&json_str).ok()?;
        set.set_number = set_number;
        let csv_path = path.with_extension("csv");
        if csv_path.exists() {
            let events = Self::parse_events(&csv_path).await.ok()?;
            set.events = events;
        }
        Some(Ok(set))
    }

    async fn parse_events(path: &Path) -> Result<Vec<EventEntry>, AppError> {
        let p = path.to_path_buf();
        spawn_blocking(move || -> Result<Vec<EventEntry>, AppError> {
            let file = File::open(&p).map_err(|e| AppError::IO(IOError::from(e)))?;
            let mut reader = ReaderBuilder::new().has_headers(false).from_reader(file);
            let events: Vec<EventEntry> = reader
                .deserialize()
                .filter_map(|r: Result<EventEntry, csv::Error>| r.ok())
                .collect();
            Ok(events)
        })
        .await
        .map_err(|e| AppError::IO(IOError::Msg(format!("tokio join error: {}", e))))?
    }
}

#[async_trait]
impl SetReader for FileSystemSetReader {
    async fn read_all(&self, m: &MatchEntry) -> Result<Vec<SetEntry>, AppError> {
        let match_path = get_match_folder_path(&self.0, &m.team.id, &m.id)?;
        let mut dir = read_dir(&match_path).await.map_err(|_| {
            let template = current_labels().could_not_read_folder;
            AppError::Match(MatchError::LoadSetError(
                template.replace("{}", match_path.to_str().unwrap_or("")),
            ))
        })?;
        let regex = Regex::new(r"^set_(\d+)\.json$").map_err(|_| {
            AppError::Match(MatchError::LoadSetError(format!(
                "could not compile regex {:?}",
                match_path,
            )))
        })?;
        let mut tasks = Vec::new();
        while let Some(entry) = dir
            .next_entry()
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?
        {
            let path = entry.path();
            let filename = match path.file_name().and_then(|f| f.to_str()) {
                Some(name) => name,
                None => continue,
            };
            if !regex.is_match(filename) {
                continue;
            }
            let r: &Regex = &regex;
            tasks.push(async move {
                if let Some(result) = Self::parse_set(&entry, r).await {
                    result
                } else {
                    Err(AppError::IO(IOError::Msg(
                        current_labels().invalid_set.into(),
                    )))
                }
            });
        }
        let mut sets: Vec<SetEntry> = try_join_all(tasks).await?;
        if sets.len() > 5 {
            let template = current_labels().found_more_than_5_sets_in_match;
            return Err(AppError::Match(MatchError::LoadSetError(
                template.replace("{}", &m.id),
            )));
        }
        sets.sort_by_key(|s| s.set_number);
        for (i, set) in sets.iter().enumerate() {
            if set.set_number as usize != i + 1 {
                return Err(AppError::Match(MatchError::LoadSetError(
                    current_labels().wrong_set_numbering.into(),
                )));
            }
        }
        Ok(sets)
    }
}
