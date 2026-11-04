//! Pinning a team in the settings, for the screens that don't keep them
//! (a new team is pinned once saved).

use crate::{
    providers::{settings_reader::SettingsReader, settings_writer::SettingsWriter},
    shapes::settings::Settings,
};
use async_trait::async_trait;
use scout4all_core::errors::AppError;
use std::sync::Arc;
use uuid::Uuid;

#[async_trait]
pub trait TeamPins: Send + Sync {
    async fn pin(&self, id: Uuid) -> Result<(), AppError>;
}

impl std::fmt::Debug for dyn TeamPins {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TeamPins")
    }
}

/// The pins of `config.json`, read again before changing them: the other
/// settings stay as they are; `current` when it can't be read.
pub struct SettingsPins<SR, SW> {
    pub reader: Arc<SR>,
    pub writer: Arc<SW>,
    pub current: Settings,
}

#[async_trait]
impl<SR: SettingsReader + Send + Sync, SW: SettingsWriter + Send + Sync> TeamPins
    for SettingsPins<SR, SW>
{
    async fn pin(&self, id: Uuid) -> Result<(), AppError> {
        let settings = self
            .reader
            .read()
            .await
            .unwrap_or_else(|_| self.current.clone());
        self.writer
            .save(Settings {
                pinned: settings.pinned.clone().with(id, true),
                ..settings
            })
            .await
            .map(|_| ())
    }
}
