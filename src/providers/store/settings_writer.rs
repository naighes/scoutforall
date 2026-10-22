use crate::{
    providers::{settings_writer::SettingsWriter, store::CONFIG},
    shapes::settings::Settings,
};
use async_trait::async_trait;
use scout4all_core::{
    errors::{AppError, IOError},
    storage::Store,
};
use serde_json::to_vec_pretty;
use std::sync::Arc;
use tokio::task::spawn_blocking;

pub struct StoreSettingsWriter(Arc<dyn Store>);

impl StoreSettingsWriter {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self(store)
    }
}

#[async_trait]
impl SettingsWriter for StoreSettingsWriter {
    async fn save(&self, settings: Settings) -> Result<Settings, AppError> {
        let bytes = spawn_blocking({
            let settings = settings.clone();
            move || to_vec_pretty(&settings).map_err(|e| AppError::IO(IOError::from(e)))
        })
        .await
        .map_err(|e| AppError::IO(IOError::Msg(format!("tokio join error: {}", e))))??;
        self.0.write(CONFIG, &bytes).await?;
        Ok(settings)
    }
}
