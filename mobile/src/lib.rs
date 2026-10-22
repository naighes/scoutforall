//! The core of scout4all for the native apps: UniFFI generates the Kotlin
//! and Swift bindings of what's exported here.
//!
//! The API is blocking for now: the app calls it off the main thread
//! (`Dispatchers.IO` on Android) and a tokio runtime here drives the async
//! providers of the core. The final app must never block the UI: the plan is
//! to move to the `async fn` of UniFFI.

use scout4all_core::{
    errors::AppError,
    localization::Language,
    logging::logger::init_logger,
    providers::{store::team_reader::StoreTeamReader, team_reader::TeamReader},
    shapes::enums::LanguageEnum,
    storage::{Store, fs::FsStore},
};
use std::{path::PathBuf, str::FromStr, sync::Arc};
use tokio::runtime::{Builder, Runtime};

uniffi::setup_scaffolding!();

/// Flat: the app gets the message alone.
#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum MobileError {
    #[error("{message}")]
    Failed { message: String },
}

impl From<AppError> for MobileError {
    fn from(e: AppError) -> Self {
        MobileError::Failed {
            message: e.to_string(),
        }
    }
}

/// A team as the list shows it: one that can't be read keeps its folder.
#[derive(Debug, PartialEq, uniffi::Enum)]
pub enum TeamListing {
    Team {
        id: String,
        name: String,
        year: u16,
        players: u32,
    },
    Unreadable {
        folder: String,
    },
}

/// The data of the app, in a folder with the same layout as `~/.scout4all`.
#[derive(uniffi::Object)]
pub struct Scout4All {
    runtime: Runtime,
    team_reader: StoreTeamReader,
}

#[uniffi::export]
impl Scout4All {
    /// The data in `data_dir`; `language` is a code like `"it"`, English when
    /// it isn't supported.
    #[uniffi::constructor]
    pub fn new(data_dir: String, language: String) -> Result<Arc<Self>, MobileError> {
        let data_dir = PathBuf::from(data_dir);
        init_logger(data_dir.join("scout4all.log"));
        let language = Language::new(LanguageEnum::from_str(&language).unwrap_or(LanguageEnum::En));
        let store: Arc<dyn Store> = Arc::new(FsStore::new(&data_dir));
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| MobileError::Failed {
                message: e.to_string(),
            })?;
        Ok(Arc::new(Self {
            runtime,
            team_reader: StoreTeamReader::new(store, language),
        }))
    }

    /// The teams, sorted by name.
    pub fn teams(&self) -> Result<Vec<TeamListing>, MobileError> {
        let mut teams: Vec<TeamListing> = self
            .runtime
            .block_on(self.team_reader.read_all())?
            .into_iter()
            .map(|listed| match listed {
                Ok(team) => TeamListing::Team {
                    id: team.id.to_string(),
                    name: team.name,
                    year: team.year,
                    players: u32::try_from(team.players.len()).unwrap_or(u32::MAX),
                },
                Err(unreadable) => TeamListing::Unreadable {
                    folder: unreadable.id,
                },
            })
            .collect();
        teams.sort_by_key(|t| match t {
            TeamListing::Team { name, .. } => name.to_lowercase(),
            TeamListing::Unreadable { folder } => folder.to_lowercase(),
        });
        Ok(teams)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_folder_has_no_teams() {
        let dir = tempfile::tempdir().unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        assert_eq!(app.teams().unwrap(), vec![]);
    }

    #[test]
    fn a_team_folder_that_is_not_an_id_is_unreadable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("not-a-team")).unwrap();
        let app = Scout4All::new(dir.path().to_string_lossy().into_owned(), "xx".into()).unwrap();
        assert_eq!(
            app.teams().unwrap(),
            vec![TeamListing::Unreadable {
                folder: "not-a-team".into()
            }]
        );
    }
}
