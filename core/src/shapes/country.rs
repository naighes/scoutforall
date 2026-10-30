//! The country of a team: an ISO 3166-1 alpha-2 code (`IT`, `BR`...), with
//! its name in the languages of the app (`country_names.rs`, from CLDR).

use crate::shapes::{country_names::COUNTRIES, enums::LanguageEnum};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Two upper-case ASCII letters. A code missing from the table (one a later
/// version knows) is kept as it is, its name is the code itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CountryCode([u8; 2]);

impl CountryCode {
    /// The code in `code`, as written in `team.json` or by the system: two
    /// letters, any case, surrounding spaces aside.
    pub fn parse(code: &str) -> Option<Self> {
        match code.trim().as_bytes() {
            [a, b] if a.is_ascii_alphabetic() && b.is_ascii_alphabetic() => {
                Some(Self([a.to_ascii_uppercase(), b.to_ascii_uppercase()]))
            }
            _ => None,
        }
    }

    /// A country of the table, as the user can choose it.
    pub fn known(code: &str) -> Option<Self> {
        Self::parse(code).filter(|c| c.index().is_some())
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or_default()
    }

    fn index(&self) -> Option<usize> {
        COUNTRIES
            .binary_search_by(|(code, _)| code.as_bytes().cmp(&self.0[..]))
            .ok()
    }

    /// The name of the country in `language`; the code when it's not in
    /// the table.
    pub fn name(&self, language: LanguageEnum) -> String {
        self.index()
            .and_then(|i| COUNTRIES.get(i))
            .and_then(|(_, names)| names.get(language_index(language)))
            .map_or_else(|| self.as_str().to_string(), |name| name.to_string())
    }

    /// Every country, by name in `language`.
    pub fn all(language: LanguageEnum) -> Vec<(CountryCode, &'static str)> {
        let mut countries: Vec<_> = COUNTRIES
            .iter()
            .filter_map(|(code, names)| {
                Self::parse(code).zip(names.get(language_index(language)).copied())
            })
            .collect();
        countries.sort_by_cached_key(|(_, name)| name.to_lowercase());
        countries
    }

    /// The countries whose code is `query`, then those whose name in
    /// `language` has it in it, by name; all of them for an empty query.
    pub fn search(query: &str, language: LanguageEnum) -> Vec<(CountryCode, &'static str)> {
        let query = query.trim().to_lowercase();
        let (by_code, by_name): (Vec<_>, Vec<_>) = Self::all(language)
            .into_iter()
            .filter(|(code, name)| {
                name.to_lowercase().contains(&query) || code.as_str().eq_ignore_ascii_case(&query)
            })
            .partition(|(code, _)| code.as_str().eq_ignore_ascii_case(&query));
        by_code.into_iter().chain(by_name).collect()
    }

    /// The country of a locale of the system (`it_IT.UTF-8`, `pt-BR`,
    /// `zh-Hant-TW`) or a country code alone (`it`, from the SIM): the
    /// first part after the language that's a country of the table.
    pub fn from_locale(locale: &str) -> Option<Self> {
        let tag = locale.split(['.', '@']).next().unwrap_or_default();
        match tag.split(['-', '_']).collect::<Vec<_>>().as_slice() {
            [code] => Self::known(code),
            [_, rest @ ..] => rest.iter().find_map(|part| Self::known(part)),
            [] => None,
        }
    }

    /// The country most users of `language` play in: none for English.
    pub fn of_language(language: LanguageEnum) -> Option<Self> {
        match language {
            LanguageEnum::En => None,
            LanguageEnum::It => Self::known("IT"),
            LanguageEnum::Pt => Self::known("BR"),
            LanguageEnum::Pl => Self::known("PL"),
            LanguageEnum::Ja => Self::known("JP"),
        }
    }

    /// The country to start with on a new installation: the first of the
    /// hints of the system (SIM, network, locale, in the order given) that
    /// tells one, else the one of the language.
    pub fn guess<'a>(
        hints: impl IntoIterator<Item = &'a str>,
        language: LanguageEnum,
    ) -> Option<Self> {
        hints
            .into_iter()
            .find_map(Self::from_locale)
            .or_else(|| Self::of_language(language))
    }
}

fn language_index(language: LanguageEnum) -> usize {
    LanguageEnum::ALL
        .iter()
        .position(|l| *l == language)
        .unwrap_or_default()
}

impl fmt::Display for CountryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Serialize for CountryCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CountryCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        Self::parse(&code)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid country: {code}")))
    }
}

/// An optional country that's never an error: a value that isn't a code
/// (written by hand) is no country, the rest of the file still good.
pub fn lenient<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<CountryCode>, D::Error> {
    Ok(Option::<serde_json::Value>::deserialize(deserializer)?
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .and_then(CountryCode::parse))
}
