use crate::{
    errors::{AppError, IOError},
    providers::{fs::path::get_config_file_path, settings_writer::SettingsWriter},
    shapes::settings::Settings,
};
use async_trait::async_trait;
use serde_json::to_vec_pretty;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::task::spawn_blocking;

pub struct FileSystemSettingsWriter(PathBuf);

impl FileSystemSettingsWriter {
    pub fn new(base_path: &Path) -> Self {
        Self(base_path.to_path_buf())
    }
}

#[async_trait]
impl SettingsWriter for FileSystemSettingsWriter {
    async fn save(&self, settings: Settings) -> Result<Settings, AppError> {
        let path = get_config_file_path(&self.0);
        let bytes = spawn_blocking({
            let settings = settings.clone();
            move || to_vec_pretty(&settings).map_err(|e| AppError::IO(IOError::from(e)))
        })
        .await
        .map_err(|e| AppError::IO(IOError::Msg(format!("tokio join error: {}", e))))??;
        fs::write(&path, bytes)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        Ok(settings)
    }
}
