use crate::{errors::AppError, shapes::settings::Settings};
use async_trait::async_trait;

#[async_trait]
pub trait SettingsReader {
    async fn read(&self) -> Result<Settings, AppError>;
}
