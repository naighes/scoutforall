use crate::{
    providers::settings_reader::SettingsReader,
    shapes::{
        keybinding::{KeyBindings, is_reserved_key},
        settings::Settings,
    },
};
use async_trait::async_trait;
use crokey::KeyCombination;
use scout4all_core::{
    errors::{AppError, IOError},
    providers::fs::path::get_config_file_path,
};
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
