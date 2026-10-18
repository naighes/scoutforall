use crate::shapes::settings::Settings;
use async_trait::async_trait;
use scout4all_core::errors::AppError;

#[async_trait]
pub trait SettingsReader {
    async fn read(&self) -> Result<Settings, AppError>;
}
