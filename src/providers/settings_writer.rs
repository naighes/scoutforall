use crate::{
    errors::AppError,
    shapes::{enums::LanguageEnum, settings::Settings},
};
use async_trait::async_trait;

#[async_trait]
pub trait SettingsWriter {
    async fn save(&self, language: LanguageEnum) -> Result<Settings, AppError>;
}
