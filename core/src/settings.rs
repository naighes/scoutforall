//! The settings every app shares: the terms of use seen and the choice about
//! the statistics. Each app keeps them in its `config.json`, along with its
//! own settings.

use serde::{Deserialize, Serialize};

/// The version of the terms of use (license and data collection): raise it
/// when they change, to show them again to every user.
pub const TERMS_VERSION: u8 = 2;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub struct Consent {
    #[serde(default = "default_analytics_enabled")]
    pub analytics_enabled: bool,
    /// the version of the terms of use shown to the user: older than
    /// `TERMS_VERSION` (0 if never shown), the app shows them at startup
    #[serde(default)]
    pub terms_version: u8,
}

fn default_analytics_enabled() -> bool {
    true
}

impl Default for Consent {
    fn default() -> Self {
        Self {
            analytics_enabled: default_analytics_enabled(),
            terms_version: 0,
        }
    }
}

impl Consent {
    /// Whether the user has seen the current terms of use.
    pub fn terms_seen(&self) -> bool {
        self.terms_version >= TERMS_VERSION
    }

    /// Whether analytics are uploaded: only once the user has seen the terms
    /// telling about them.
    pub fn uploads_enabled(&self) -> bool {
        self.analytics_enabled && self.terms_seen()
    }

    /// The current terms of use seen, the choice unchanged.
    pub fn with_terms_seen(self) -> Self {
        Self {
            terms_version: TERMS_VERSION,
            ..self
        }
    }
}
