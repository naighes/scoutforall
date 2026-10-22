use crate::shapes::keybinding::{KeyBindings, is_fixed_key, is_reserved_key};
use crokey::KeyCombination;
use dirs::home_dir;
use scout4all_core::{
    analytics::global::set_uploads_enabled,
    constants::DEFAULT_LANGUAGE,
    localization::set_language,
    shapes::{
        enums::{EvalEnum, EventTypeEnum, LanguageEnum, WithDesc},
        technique::{ServeTypeEnum, SetTypeEnum},
    },
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
    #[serde(default)]
    pub set_type_keybindings: KeyBindings<SetTypeEnum>,
    #[serde(default)]
    pub serve_type_keybindings: KeyBindings<ServeTypeEnum>,
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
            set_type_keybindings: KeyBindings::default(),
            serve_type_keybindings: KeyBindings::default(),
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

/// What users can bind while scouting: events, evaluations, sets and serves,
/// each with its own bindings in the settings.
pub trait ScoutingShortcut:
    Copy + Eq + std::hash::Hash + WithDesc<Self> + Send + Sync + 'static
{
    fn bindings(settings: &Settings) -> KeyBindings<Self>;
    fn with_bindings(settings: &Settings, bindings: &KeyBindings<Self>) -> Settings;

    /// Whether the key can't be assigned: the fixed keys (see
    /// [`is_fixed_key`]).
    fn is_reserved(ck: &KeyCombination) -> bool {
        is_fixed_key(ck)
    }

    /// The shortcut of another kind bound to the key and active at the same
    /// time, if any (e.g. an evaluation and a set): the key can't be shared.
    fn rival(_settings: &Settings, _ck: &KeyCombination) -> Option<String> {
        None
    }
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

/// The description of the shortcut bound to the key, if any.
fn bound_to<T: ScoutingShortcut>(bindings: &KeyBindings<T>, ck: &KeyCombination) -> Option<String> {
    bindings.action_for(ck).map(|action| action.with_desc().1)
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
    /// the sets and serves are chosen along with the evaluation
    fn rival(settings: &Settings, ck: &KeyCombination) -> Option<String> {
        bound_to(&settings.set_type_keybindings, ck)
            .or_else(|| bound_to(&settings.serve_type_keybindings, ck))
    }
}

impl ScoutingShortcut for SetTypeEnum {
    fn bindings(settings: &Settings) -> KeyBindings<Self> {
        settings.set_type_keybindings.clone()
    }
    fn with_bindings(settings: &Settings, bindings: &KeyBindings<Self>) -> Settings {
        Settings {
            set_type_keybindings: bindings.clone(),
            ..settings.clone()
        }
    }
    fn is_reserved(ck: &KeyCombination) -> bool {
        is_reserved_key(ck)
    }
    fn rival(settings: &Settings, ck: &KeyCombination) -> Option<String> {
        bound_to(&settings.eval_keybindings, ck)
    }
}

impl ScoutingShortcut for ServeTypeEnum {
    fn bindings(settings: &Settings) -> KeyBindings<Self> {
        settings.serve_type_keybindings.clone()
    }
    fn with_bindings(settings: &Settings, bindings: &KeyBindings<Self>) -> Settings {
        Settings {
            serve_type_keybindings: bindings.clone(),
            ..settings.clone()
        }
    }
    fn is_reserved(ck: &KeyCombination) -> bool {
        is_reserved_key(ck)
    }
    fn rival(settings: &Settings, ck: &KeyCombination) -> Option<String> {
        bound_to(&settings.eval_keybindings, ck)
    }
}

static INITIALIZED: OnceLock<()> = OnceLock::new();

/// Default settings initialization (called once at startup).
pub fn init_settings(default: Settings) {
    // once at startup: a second call changes the settings all the same
    let _ = INITIALIZED.set(());
    set_language(default.language);
    set_uploads_enabled(default.analytics_enabled);
}

/// Change the current language and whether analytics uploads run: only once
/// initialized, the tests keep the defaults (no uploads).
pub fn set_settings(settings: Settings) {
    if let Some(()) = INITIALIZED.get() {
        set_language(settings.language);
        set_uploads_enabled(settings.analytics_enabled);
    }
}
