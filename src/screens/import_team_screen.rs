use crate::localization::current_labels;
use crate::screens::{
    file_system_screen::FileSystemAction,
    import_dvw_screen::{DvwImportScreen, DvwProviders},
    screen::AppAction,
};
use async_trait::async_trait;
use hf::is_hidden;
use scout4all_core::{
    archive::{file_team_id, import_team},
    dvw::parse,
    errors::{AppError, IOError},
    providers::{team_reader::TeamReader, team_writer::TeamWriter},
};
use std::{fs, path::Path, sync::Arc};

/// Imports the archive of a team (`.zip`) or, given the providers, a
/// DataVolley file (`.dvw`), whose import goes on in a screen of its own.
pub struct ImportTeamAction<TR: TeamReader + Send + Sync, TW: TeamWriter + Send + Sync> {
    team_reader: Arc<TR>,
    team_writer: Arc<TW>,
    dvw: Option<DvwProviders>,
}

fn has_extension(path: &Path, extensions: &[&str]) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|e| extensions.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

#[async_trait]
impl<TR: TeamReader + Send + Sync, TW: TeamWriter + Send + Sync> FileSystemAction
    for ImportTeamAction<TR, TW>
{
    fn is_selectable(&self, path: &Path) -> bool {
        !is_hidden(path).unwrap_or_default() && has_extension(path, self.extensions())
    }

    fn is_visible(&self, path: &Path) -> bool {
        !is_hidden(path).unwrap_or_default()
            && (path.is_dir() || has_extension(path, self.extensions()))
    }

    async fn on_selected(&mut self, path: &Path) -> Result<AppAction, AppError> {
        let bytes = fs::read(path).map_err(|e| AppError::IO(IOError::from(e)))?;
        match (&self.dvw, has_extension(path, &["dvw"])) {
            (Some(providers), true) => {
                let labels = current_labels().domain;
                let file = parse(&bytes).map_err(|e| {
                    AppError::IO(IOError::Msg(
                        labels.dvw_unreadable.replace("{detail}", &e.to_string()),
                    ))
                })?;
                let teams = providers
                    .team_reader
                    .read_all()
                    .await?
                    .into_iter()
                    .filter_map(|t| t.ok())
                    .collect();
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();
                Ok(AppAction::SwitchScreen(Box::new(DvwImportScreen::new(
                    file,
                    name,
                    providers.clone(),
                    teams,
                ))))
            }
            _ => {
                // the archives exported up to v0.9.0 have the id only in
                // their name
                let file_id = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(file_team_id);
                import_team(
                    &bytes,
                    file_id,
                    &*self.team_reader,
                    &*self.team_writer,
                    current_labels().domain,
                )
                .await?;
                Ok(AppAction::Back(true, Some(1)))
            }
        }
    }
}

impl<TR: TeamReader + Send + Sync, TW: TeamWriter + Send + Sync> ImportTeamAction<TR, TW> {
    pub fn new(team_reader: Arc<TR>, team_writer: Arc<TW>) -> Self {
        Self {
            team_reader,
            team_writer,
            dvw: None,
        }
    }

    /// The DataVolley files too.
    pub fn with_dvw(mut self, providers: DvwProviders) -> Self {
        self.dvw = Some(providers);
        self
    }

    fn extensions(&self) -> &'static [&'static str] {
        match self.dvw {
            Some(_) => &["zip", "dvw"],
            None => &["zip"],
        }
    }
}
