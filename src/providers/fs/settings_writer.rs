use crate::{
    errors::{AppError, IOError},
    providers::{fs::path::get_config_file_path, settings_writer::SettingsWriter},
    shapes::{enums::LanguageEnum, settings::Settings},
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
    async fn save(
        &self,
        language: LanguageEnum,
        analytics_enabled: bool,
    ) -> Result<Settings, AppError> {
        let config = Settings {
            language,
            analytics_enabled,
        };
        let path = get_config_file_path(&self.0);
        let bytes = spawn_blocking({
            let config = config.clone();
            move || to_vec_pretty(&config).map_err(|e| AppError::IO(IOError::from(e)))
        })
        .await
        .map_err(|e| AppError::IO(IOError::Msg(format!("tokio join error: {}", e))))??;
        fs::write(&path, bytes)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        Ok(config)
    }
}
