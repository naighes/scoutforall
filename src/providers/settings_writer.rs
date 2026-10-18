use crate::shapes::settings::Settings;
use async_trait::async_trait;
use scout4all_core::errors::AppError;

#[async_trait]
pub trait SettingsWriter {
    async fn save(&self, settings: Settings) -> Result<Settings, AppError>;
}
