use crate::{
    providers::{settings_reader::SettingsReader, store::CONFIG},
    shapes::{
        keybinding::{KeyBindings, is_reserved_key},
        settings::Settings,
    },
};
use async_trait::async_trait;
use crokey::KeyCombination;
use scout4all_core::{
    errors::{AppError, IOError},
    storage::Store,
};
use serde_json::from_slice;
use std::sync::Arc;

pub struct StoreSettingsReader(Arc<dyn Store>);

impl StoreSettingsReader {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self(store)
    }
}

#[async_trait]
impl SettingsReader for StoreSettingsReader {
    async fn read(&self) -> Result<Settings, AppError> {
        let content = self
            .0
            .read(CONFIG)
            .await?
            .ok_or_else(|| AppError::IO(IOError::Msg(format!("not found: {}", CONFIG))))?;
        let settings =
            from_slice::<Settings>(&content).map_err(|e| AppError::IO(IOError::from(e)))?;
        // fixed keys, and more keys for the same action, may come from older
        // versions or hand-edited files
        let eval_keybindings = settings
            .eval_keybindings
            .without_fixed_keys()
            .one_key_each(&KeyBindings::default());
        // sets and serves are chosen along with the evaluation: they lose the
        // keys of the evaluations, and don't get the default ones taken by an
        // evaluation (it could be bound to a letter before sets existed)
        let taken = |ck: &KeyCombination| eval_keybindings.action_for(ck).is_some();
        let reserved = |ck: &KeyCombination| is_reserved_key(ck) || taken(ck);
        Ok(Settings {
            scouting_keybindings: settings
                .scouting_keybindings
                .without_fixed_keys()
                .one_key_each(&KeyBindings::default()),
            set_type_keybindings: settings
                .set_type_keybindings
                .without_keys(reserved)
                .one_key_each(&KeyBindings::default().without_keys(taken)),
            serve_type_keybindings: settings
                .serve_type_keybindings
                .without_keys(reserved)
                .one_key_each(&KeyBindings::default().without_keys(taken)),
            eval_keybindings,
            ..settings
        })
    }
}
