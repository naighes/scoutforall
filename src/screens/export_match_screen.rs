use crate::localization::current_labels;
use crate::screens::{file_system_screen::FileSystemAction, screen::AppAction};
use async_trait::async_trait;
use hf::is_hidden;
use scout4all_core::{
    archive::export_match,
    errors::{AppError, IOError},
    shapes::team::TeamEntry,
    storage::Store,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct ExportMatchAction {
    team: TeamEntry,
    match_id: String,
    store: Arc<dyn Store>,
    exported_file_path: Option<PathBuf>,
}

impl ExportMatchAction {
    pub fn new(team: TeamEntry, match_id: String, store: Arc<dyn Store>) -> Self {
        Self {
            team,
            match_id,
            store,
            exported_file_path: None,
        }
    }
}

#[async_trait]
impl FileSystemAction for ExportMatchAction {
    fn is_selectable(&self, path: &Path) -> bool {
        !is_hidden(path).unwrap_or_default() && path.is_dir()
    }

    fn is_visible(&self, path: &Path) -> bool {
        !is_hidden(path).unwrap_or_default() && path.is_dir()
    }

    fn success_message_suffix(&self) -> Option<String> {
        self.exported_file_path
            .as_ref()
            .map(|p| p.display().to_string())
    }

    async fn on_selected(&mut self, path: &Path) -> Result<AppAction, AppError> {
        let zip_file_path = path.join(format!("{}.zip", self.match_id));
        if zip_file_path.exists() {
            return Err(AppError::IO(IOError::Msg(
                current_labels().file_already_exists.to_string(),
            )));
        }
        let archive = export_match(&*self.store, &self.team.id, &self.match_id).await?;
        fs::write(&zip_file_path, archive).map_err(|e| AppError::IO(IOError::from(e)))?;
        self.exported_file_path = Some(zip_file_path);
        Ok(AppAction::Back(true, Some(1)))
    }
}
