use crate::{constants::DEFAULT_LANGUAGE, shapes::enums::LanguageEnum};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub language: LanguageEnum,
    #[serde(default = "default_analytics_enabled")]
    pub analytics_enabled: bool,
}

fn default_analytics_enabled() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: LanguageEnum::from_str(DEFAULT_LANGUAGE).unwrap_or(LanguageEnum::En),
            analytics_enabled: true,
        }
    }
}
