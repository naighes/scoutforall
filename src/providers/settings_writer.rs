use crate::{errors::AppError, shapes::settings::Settings};
use async_trait::async_trait;

#[async_trait]
pub trait SettingsWriter {
    async fn save(&self, settings: Settings) -> Result<Settings, AppError>;
}
