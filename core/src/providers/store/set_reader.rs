use crate::{
    errors::{AppError, IOError, MatchError},
    localization::Language,
    logging::logger::log_error,
    providers::set_reader::SetReader,
    shapes::{r#match::MatchEntry, set::SetEntry, snapshot::EventEntry},
    storage::{Store, StoreEntry, layout},
};
use async_trait::async_trait;
use csv::ReaderBuilder;
use futures::future::try_join_all;
use regex::Regex;
use serde_json::from_slice;
use std::sync::Arc;

pub struct StoreSetReader {
    store: Arc<dyn Store>,
    language: Language,
}

impl StoreSetReader {
    pub fn new(store: Arc<dyn Store>, language: Language) -> Self {
        Self { store, language }
    }

    /// The set whose lineup is in the blob, with its events if any.
    async fn parse_set(
        &self,
        folder: &str,
        name: &str,
        set_file_regex: &Regex,
    ) -> Option<SetEntry> {
        let caps = set_file_regex.captures(name)?;
        let set_number: u8 = caps.get(1)?.as_str().parse().ok()?;
        let json = self.store.read(&layout::child(folder, name)).await.ok()??;
        let mut set: SetEntry = from_slice(&json).ok()?;
        set.set_number = set_number;
        let events_key = layout::child(folder, &layout::set_events_name(set_number));
        if let Some(csv) = self.store.read(&events_key).await.ok()? {
            set.events = read_events(&csv).ok()?;
        }
        Some(set)
    }
}

/// The events saved in the blob. Rows with and without the technique may be
/// mixed, e.g. when v0.4.3 or older adds events to a set scouted by a later
/// version. A last row without its line end was cut while appended (the
/// battery out): it's left out, the set stays readable.
pub(crate) fn read_events(csv: &[u8]) -> Result<Vec<EventEntry>, AppError> {
    let whole = whole_rows(csv);
    if whole.len() < csv.len() {
        log_error(&format!(
            "the last event of a set was cut, left out: {:?}",
            String::from_utf8_lossy(csv.get(whole.len()..).unwrap_or_default())
        ));
    }
    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(whole);
    // a row that can't be read makes the set unreadable: without it the set
    // would replay to something else
    reader
        .deserialize()
        .collect::<Result<Vec<EventEntry>, csv::Error>>()
        .map_err(|e| AppError::IO(IOError::from(e)))
}

/// The rows that end with a line end: every event is saved with one.
pub(crate) fn whole_rows(csv: &[u8]) -> &[u8] {
    csv.iter()
        .rposition(|b| *b == b'\n')
        .and_then(|end| csv.get(..=end))
        .unwrap_or_default()
}

#[async_trait]
impl SetReader for StoreSetReader {
    async fn read_all(&self, m: &MatchEntry) -> Result<Vec<SetEntry>, AppError> {
        let folder = layout::match_folder(&m.team.id, &m.id);
        let entries = self.store.list(&folder).await.map_err(|_| {
            let template = self.language.labels().could_not_read_folder;
            AppError::Match(MatchError::LoadSetError(template.replace("{}", &folder)))
        })?;
        let regex = Regex::new(r"^set_(\d+)\.json$").map_err(|_| {
            AppError::Match(MatchError::LoadSetError(format!(
                "could not compile regex {:?}",
                folder,
            )))
        })?;
        let tasks = entries
            .iter()
            .filter_map(|entry| match entry {
                StoreEntry::Blob(name) if regex.is_match(name) => Some(name),
                _ => None,
            })
            .map(|name| async {
                self.parse_set(&folder, name, &regex).await.ok_or_else(|| {
                    AppError::IO(IOError::Msg(self.language.labels().invalid_set.into()))
                })
            });
        let mut sets: Vec<SetEntry> = try_join_all(tasks).await?;
        if sets.len() > 5 {
            let template = self.language.labels().found_more_than_5_sets_in_match;
            return Err(AppError::Match(MatchError::LoadSetError(
                template.replace("{}", &m.id),
            )));
        }
        sets.sort_by_key(|s| s.set_number);
        for (i, set) in sets.iter().enumerate() {
            if set.set_number as usize != i + 1 {
                return Err(AppError::Match(MatchError::LoadSetError(
                    self.language.labels().wrong_set_numbering.into(),
                )));
            }
        }
        Ok(sets)
    }
}
