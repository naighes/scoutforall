//! A country as a form asks for it: written, with the countries it may be
//! offered below, each with its flag, as on Android (`Countries.kt`).

use {
    crate::form::Suggestion, scout4all_core::shapes::country::CountryCode,
    scout4all_mobile::Scout4All,
};

/// How many countries a search offers.
const OFFERED: usize = 12;

/// The flag of a country: its two letters as regional indicators, which
/// the system draws as the flag (Windows writes the letters).
pub fn flag(code: CountryCode) -> String {
    code.as_str()
        .chars()
        .filter_map(|c| char::from_u32(0x1F1E6 + u32::from(c) - u32::from('A')))
        .collect()
}

/// The countries `written` may be, flag first.
pub fn suggestions(core: &Scout4All, written: &str) -> Vec<Suggestion> {
    let start = written.trim().to_lowercase();
    match start.is_empty() {
        true => vec![],
        false => {
            let mut found = core.countries(written.to_string());
            // the code first, then the names that begin with it: Italia
            // before Haiti
            found.sort_by_key(|c| {
                (
                    !c.value.as_str().eq_ignore_ascii_case(&start),
                    !c.name.to_lowercase().starts_with(&start),
                )
            });
            found
        }
        .into_iter()
        .take(OFFERED)
        .map(|c| Suggestion {
            label: format!("{}  {}", flag(c.value), c.name),
            value: c.name,
        })
        .collect(),
    }
}

/// The country `written` is: none for nothing written, an error when it's
/// no country (its name, in the language of the app, or its code).
pub fn chosen(core: &Scout4All, written: &str) -> Result<Option<CountryCode>, ()> {
    let written = written.trim();
    match written.is_empty() {
        true => Ok(None),
        false => core
            .countries(written.to_string())
            .into_iter()
            .find(|c| {
                c.name.to_lowercase() == written.to_lowercase()
                    || c.value.as_str().eq_ignore_ascii_case(written)
            })
            .map(|c| Some(c.value))
            .ok_or(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_is_the_two_letters_as_regional_indicators() {
        let flag_of = |code| CountryCode::parse(code).map(flag);
        assert_eq!(flag_of("IT").as_deref(), Some("🇮🇹"));
        assert_eq!(flag_of("br").as_deref(), Some("🇧🇷"));
    }
}
