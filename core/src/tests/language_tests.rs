//! The languages: read from the code of the phone or of the settings
//! ("pt", "ja"), or from the name shown in the settings.
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
    fn every_language_is_read_from_its_name() {
        for language in LanguageEnum::ALL {
            let name = language.to_string();
            assert_eq!(LanguageEnum::from_str(&name).unwrap(), language, "{name}");
        }
    }
}
