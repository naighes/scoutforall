use crate::shapes::keybinding::KeyBindings;
use dirs::home_dir;
use scout4all_core::{
    constants::DEFAULT_LANGUAGE,
    localization::set_language,
    shapes::enums::{EvalEnum, EventTypeEnum, LanguageEnum, WithDesc},
};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::{path::PathBuf, str::FromStr};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub language: LanguageEnum,
    #[serde(default)]
    pub scouting_keybindings: KeyBindings<EventTypeEnum>,
    #[serde(default)]
    pub eval_keybindings: KeyBindings<EvalEnum>,
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
            scouting_keybindings: KeyBindings::default(),
            eval_keybindings: KeyBindings::default(),
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

/// What users can bind while scouting: events and evaluations, each with its
/// own bindings in the settings.
pub trait ScoutingShortcut:
    Copy + Eq + std::hash::Hash + WithDesc<Self> + Send + Sync + 'static
{
    fn bindings(settings: &Settings) -> KeyBindings<Self>;
    fn with_bindings(settings: &Settings, bindings: &KeyBindings<Self>) -> Settings;
}

impl ScoutingShortcut for EventTypeEnum {
    fn bindings(settings: &Settings) -> KeyBindings<Self> {
        settings.scouting_keybindings.clone()
    }
    fn with_bindings(settings: &Settings, bindings: &KeyBindings<Self>) -> Settings {
        Settings {
            scouting_keybindings: bindings.clone(),
            ..settings.clone()
        }
    }
}

impl ScoutingShortcut for EvalEnum {
    fn bindings(settings: &Settings) -> KeyBindings<Self> {
        settings.eval_keybindings.clone()
    }
    fn with_bindings(settings: &Settings, bindings: &KeyBindings<Self>) -> Settings {
        Settings {
            eval_keybindings: bindings.clone(),
            ..settings.clone()
        }
    }
}

static INITIALIZED: OnceLock<()> = OnceLock::new();

/// Default settings initialization (should be called once at startup).
pub fn init_settings(default: Settings) {
    INITIALIZED.set(()).ok().unwrap();
    set_language(default.language);
}

/// Change the current language: only once initialized, the tests keep the
/// default one.
pub fn set_settings(settings: Settings) {
    if let Some(()) = INITIALIZED.get() {
        set_language(settings.language);
    }
}
