//! The languages: read from the code of the phone or of the settings
//! ("pt", "ja"), from the name shown in the settings, or from a locale of
//! the system ("it_IT.UTF-8").
#[cfg(test)]
mod tests {
    use crate::shapes::enums::LanguageEnum;
    use std::str::FromStr;

    #[test]
    fn every_language_is_read_from_its_code() {
        for language in LanguageEnum::ALL {
            let code = serde_json::to_value(language).unwrap();
            let code = code.as_str().unwrap();
            assert_eq!(LanguageEnum::from_str(code).unwrap(), language, "{code}");
        }
        assert_eq!(LanguageEnum::from_str("pt").unwrap(), LanguageEnum::Pt);
        assert_eq!(LanguageEnum::from_str("pl").unwrap(), LanguageEnum::Pl);
        assert_eq!(LanguageEnum::from_str("ja").unwrap(), LanguageEnum::Ja);
    }

    #[test]
    fn the_language_of_a_locale_of_the_system() {
        let of = LanguageEnum::of_locale;
        assert_eq!(of("it_IT.UTF-8"), Some(LanguageEnum::It));
        assert_eq!(of("pt-BR"), Some(LanguageEnum::Pt));
        // the Portuguese of Portugal too: the only one the app speaks
        assert_eq!(of("pt_PT"), Some(LanguageEnum::Pt));
        assert_eq!(of("ja"), Some(LanguageEnum::Ja));
        assert_eq!(of("PL-pl"), Some(LanguageEnum::Pl));
        assert_eq!(of("en-GB"), Some(LanguageEnum::En));
        for unknown in ["fr-FR", "C", "POSIX", "", "italian"] {
            assert_eq!(of(unknown), None, "{unknown:?}");
        }
    }

    #[test]
    fn every_language_is_read_from_its_name() {
        for language in LanguageEnum::ALL {
            let name = language.to_string();
            assert_eq!(LanguageEnum::from_str(&name).unwrap(), language, "{name}");
        }
    }
}
