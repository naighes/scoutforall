use crate::{
    errors::{AppError, IOError},
    providers::{fs::path::get_config_file_path, settings_reader::SettingsReader},
    shapes::settings::Settings,
};
use async_trait::async_trait;
use serde_json::from_str;
use std::path::{Path, PathBuf};
use tokio::fs::read_to_string;

pub struct FileSystemSettingsReader(PathBuf);

impl FileSystemSettingsReader {
    pub fn new(base_path: &Path) -> Self {
        Self(base_path.to_path_buf())
    }
}

#[async_trait]
impl SettingsReader for FileSystemSettingsReader {
    async fn read(&self) -> Result<Settings, AppError> {
        let path = get_config_file_path(&self.0);
        let content = read_to_string(&path)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        let settings =
            from_str::<Settings>(&content).map_err(|e| AppError::IO(IOError::from(e)))?;
        Ok(settings)
    }
}
