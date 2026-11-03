//! The settings every app shares: the terms of use seen, the choice about
//! the statistics, the country of a new team, the pinned teams. Each app keeps them in its
//! `config.json`, along with its own settings.

use crate::shapes::{
    country::{self, CountryCode},
    listed::Listed,
    team::TeamEntry,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;

/// The version of the terms of use (license and data collection): raise it
/// when they change, to show them again to every user. 3: the country of
/// the team among the data sent. 4: the names of team and opponent through
/// the server, for the keys of teams and matches.
pub const TERMS_VERSION: u8 = 4;

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

/// What a new team starts with: the country chosen in the settings, guessed
/// from the system on the first start (`CountryCode::guess`).
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
pub struct TeamDefaults {
    #[serde(
        default,
        deserialize_with = "country::lenient",
        skip_serializing_if = "Option::is_none"
    )]
    pub country: Option<CountryCode>,
}

/// The teams kept at the top of the list: a preference of the device, not
/// of the team, so it travels neither in the archives nor in the uploads.
/// The teams created by hand start pinned, the imported ones don't.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct PinnedTeams {
    #[serde(
        default,
        deserialize_with = "lenient_ids",
        skip_serializing_if = "BTreeSet::is_empty"
    )]
    pub pinned_teams: BTreeSet<Uuid>,
}

/// The ids that can be read: something else in `config.json` (written by
/// hand, or by a later version) loses the pins, not the settings.
fn lenient_ids<'de, D: Deserializer<'de>>(deserializer: D) -> Result<BTreeSet<Uuid>, D::Error> {
    Ok(Option::<serde_json::Value>::deserialize(deserializer)?
        .as_ref()
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|id| id.as_str().and_then(|id| Uuid::parse_str(id).ok()))
        .collect())
}

impl PinnedTeams {
    pub fn is_pinned(&self, id: &Uuid) -> bool {
        self.pinned_teams.contains(id)
    }

    /// The pins with the team pinned or not.
    pub fn with(self, id: Uuid, pinned: bool) -> Self {
        let mut pinned_teams = self.pinned_teams;
        match pinned {
            true => pinned_teams.insert(id),
            false => pinned_teams.remove(&id),
        };
        Self { pinned_teams }
    }

    /// The teams as the list shows them: the pinned ones at the top, each
    /// group by name, case aside; an unreadable team by its folder, never
    /// pinned.
    pub fn sorted(&self, teams: Vec<Listed<TeamEntry>>) -> Vec<Listed<TeamEntry>> {
        let mut teams = teams;
        teams.sort_by_cached_key(|team| match team {
            Ok(team) => (!self.is_pinned(&team.id), team.name.to_lowercase()),
            Err(unreadable) => (true, unreadable.id.to_lowercase()),
        });
        teams
    }
}
