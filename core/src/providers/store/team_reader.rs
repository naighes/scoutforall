use crate::{
    errors::{AppError, IOError},
    localization::Language,
    logging::logger::log_error,
    providers::{store::read_json, team_reader::TeamReader},
    shapes::{
        listed::{Listed, Unreadable},
        team::TeamEntry,
    },
    storage::{Store, StoreEntry, layout},
};
use async_trait::async_trait;
use futures::future::join_all;
use std::sync::Arc;
use uuid::Uuid;

pub struct StoreTeamReader {
    store: Arc<dyn Store>,
    language: Language,
}

impl StoreTeamReader {
    pub fn new(store: Arc<dyn Store>, language: Language) -> Self {
        Self { store, language }
    }

    /// The team in the folder, named after its id.
    async fn map_entry(&self, folder: &str) -> Result<TeamEntry, AppError> {
        let id = Uuid::parse_str(folder).map_err(|_| {
            AppError::IO(IOError::Msg(
                self.language.labels().invalid_team_folder_name.to_string(),
            ))
        })?;
        let team: TeamEntry = read_json(&*self.store, &layout::team_descriptor(&id)).await?;
        Ok(TeamEntry { id, ..team })
    }
}

#[async_trait]
impl TeamReader for StoreTeamReader {
    async fn read_all(&self) -> Result<Vec<Listed<TeamEntry>>, AppError> {
        let folders = self
            .store
            .list(layout::ROOT)
            .await?
            .into_iter()
            .filter_map(|entry| match entry {
                StoreEntry::Folder(name) => Some(name),
                StoreEntry::Blob(_) => None,
            });
        Ok(join_all(folders.map(|folder| async move {
            self.map_entry(&folder).await.map_err(|e| {
                log_error(&format!("team {:?} is unreadable: {}", folder, e));
                Unreadable { id: folder }
            })
        }))
        .await)
    }

    async fn read_single(&self, team_id: &Uuid) -> Result<TeamEntry, AppError> {
        self.map_entry(&layout::team_folder(team_id)).await
    }

    async fn exists(&self, team_id: &Uuid) -> Result<bool, AppError> {
        self.store.exists(&layout::team_folder(team_id)).await
    }
}
