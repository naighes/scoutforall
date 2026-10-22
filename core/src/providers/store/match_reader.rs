use crate::{
    errors::{AppError, IOError},
    localization::Language,
    logging::logger::log_error,
    providers::{match_reader::MatchReader, set_reader::SetReader},
    shapes::{
        enums::LineupRulesEnum,
        listed::{Listed, Unreadable},
        r#match::MatchEntry,
        team::TeamEntry,
    },
    storage::{Store, StoreEntry, layout},
};
use async_trait::async_trait;
use futures::future::join_all;
use serde_json::{Value, from_slice, from_value};
use std::{cmp::Ordering, sync::Arc};

pub struct StoreMatchReader {
    store: Arc<dyn Store>,
    set_reader: Arc<dyn SetReader + Send + Sync>,
    language: Language,
}

#[async_trait]
impl MatchReader for StoreMatchReader {
    async fn read_all(&self, team: &TeamEntry) -> Result<Vec<Listed<MatchEntry>>, AppError> {
        let folders = self
            .store
            .list(&layout::team_folder(&team.id))
            .await?
            .into_iter()
            .filter_map(|entry| match entry {
                StoreEntry::Folder(name) => Some(name),
                StoreEntry::Blob(_) => None,
            });
        let mut entries: Vec<Listed<MatchEntry>> = join_all(folders.map(|match_id| async move {
            self.map_entry(&match_id, team).await.map_err(|e| {
                log_error(&format!(
                    "match {:?} of team {} is unreadable: {}",
                    match_id, team.id, e
                ));
                Unreadable { id: match_id }
            })
        }))
        .await;
        // the most recent first, then the unreadable ones by name
        entries.sort_by(|a, b| match (a, b) {
            (Ok(a), Ok(b)) => b.date.cmp(&a.date),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(a), Err(b)) => a.id.cmp(&b.id),
        });
        Ok(entries)
    }

    async fn read_single(&self, team: &TeamEntry, match_id: &str) -> Result<MatchEntry, AppError> {
        self.map_entry(match_id, team).await
    }

    async fn exists(&self, team: &TeamEntry, match_id: &str) -> Result<bool, AppError> {
        self.store
            .exists(&layout::match_folder(&team.id, match_id))
            .await
    }
}

impl StoreMatchReader {
    pub fn new(
        store: Arc<dyn Store>,
        set_reader: Arc<dyn SetReader + Send + Sync>,
        language: Language,
    ) -> Self {
        Self {
            store,
            set_reader,
            language,
        }
    }

    async fn map_entry(&self, match_id: &str, team: &TeamEntry) -> Result<MatchEntry, AppError> {
        let content = self
            .store
            .read(&layout::match_descriptor(&team.id, match_id))
            .await?
            .ok_or_else(|| {
                AppError::IO(IOError::Msg(
                    self.language
                        .labels()
                        .match_descriptor_file_not_found
                        .to_string(),
                ))
            })?;
        let descriptor: Value = from_slice(&content).map_err(|e| AppError::IO(IOError::from(e)))?;
        // only matches created since v0.3.0 save the substitutions
        let created_since_v0_3_0 = descriptor.get("max_substitutions").is_some();
        let mut entry: MatchEntry =
            from_value(descriptor).map_err(|e| AppError::IO(IOError::from(e)))?;
        entry.id = match_id.to_string();
        entry.team = team.clone();
        entry.sets = self.set_reader.read_all(&entry).await?;
        if created_since_v0_3_0 {
            // v0.3.0 didn't save the lineup rules of its sets, but scouted
            // them with these ones
            for set in entry.sets.iter_mut() {
                set.lineup_rules.get_or_insert(LineupRulesEnum::V2);
            }
        }
        // a match whose sets don't replay can't be shown nor scouted
        entry.get_status()?;
        Ok(entry)
    }
}
