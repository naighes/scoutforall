use crate::{
    constants::DEFAULT_LANGUAGE,
    shapes::{enums::LanguageEnum, keybinding::KeyBindings},
};
use dirs::home_dir;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, str::FromStr, sync::RwLock};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub language: LanguageEnum,
    pub keybindings: KeyBindings,
    #[serde(default = "default_analytics_enabled")]
    pub analytics_enabled: bool,
    #[serde(default = "default_last_used_dir")]
    pub last_used_dir: Option<PathBuf>,
}

fn default_analytics_enabled() -> bool {
    true
}

fn default_last_used_dir() -> Option<PathBuf> {
    None
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: LanguageEnum::from_str(DEFAULT_LANGUAGE).unwrap_or(LanguageEnum::En),
            keybindings: KeyBindings::default(),
            analytics_enabled: true,
            last_used_dir: None,
        }
    }
}

impl Settings {
    pub fn get_default_path(&self) -> Option<PathBuf> {
        self.last_used_dir.clone().or_else(home_dir)
    }
}

static CURRENT_SETTING: OnceCell<RwLock<Settings>> = OnceCell::new();

/// Default settings initialization (should be called once at startup).
pub fn init_settings(default: Settings) {
    CURRENT_SETTING.set(RwLock::new(default)).ok().unwrap();
}

/// Change the current language.
pub fn set_settings(settings: Settings) {
    if let Some(lock) = CURRENT_SETTING.get() {
        *lock.write().unwrap() = settings;
    }
}

/// Returns the current language.
pub fn current_settings() -> Settings {
    if let Some(lock) = CURRENT_SETTING.get() {
        lock.read().unwrap().clone()
    } else {
        Settings::default()
    }
}
