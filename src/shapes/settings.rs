use crate::{constants::DEFAULT_LANGUAGE, shapes::enums::LanguageEnum};
use dirs::home_dir;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, str::FromStr};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub language: LanguageEnum,
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
