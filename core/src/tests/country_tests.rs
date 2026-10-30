//! The country of a team: the codes, their names, the guess on the first
//! start, `team.json` with and without it, the statistics sent.

#[cfg(test)]
mod tests {
    use crate::{
        analytics::upload::create_match_archive,
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        providers::{
            match_writer::MatchWriter,
            store::{match_writer::StoreMatchWriter, team_writer::StoreTeamWriter},
            team_writer::{TeamInput, TeamWriter},
        },
        settings::TeamDefaults,
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, LanguageEnum, TeamClassificationEnum},
            player::PlayerEntry,
            team::TeamEntry,
        },
        storage::{Store, memory::MemoryStore},
    };
    use chrono::Utc;
    use std::{
        io::{Cursor, Read},
        sync::Arc,
    };

    fn code(code: &str) -> CountryCode {
        CountryCode::known(code).unwrap()
    }

    #[test]
    fn every_iso_country_and_nothing_else() {
        let all = CountryCode::all(LanguageEnum::En);
        assert_eq!(all.len(), 249);
        for not_a_country in ["EU", "UN", "XK", "ZZ", "AC"] {
            assert_eq!(CountryCode::known(not_a_country), None, "{not_a_country}");
        }
    }

    #[test]
    fn codes_in_any_case_and_nothing_but_two_letters() {
        assert_eq!(CountryCode::parse(" it "), Some(code("IT")));
        assert_eq!(
            CountryCode::parse("Br").map(|c| c.to_string()),
            Some("BR".into())
        );
        for wrong in ["", "I", "ITA", "1T", "it-IT", "ÌT"] {
            assert_eq!(CountryCode::parse(wrong), None, "{wrong:?}");
        }
    }

    #[test]
    fn names_in_every_language() {
        let italy = code("IT");
        let names: Vec<_> = LanguageEnum::ALL.iter().map(|l| italy.name(*l)).collect();
        assert_eq!(names, ["Italy", "Italia", "Itália", "Włochy", "イタリア"]);
        assert_eq!(code("HK").name(LanguageEnum::En), "Hong Kong");
        // a code a later version knows: the code itself
        assert_eq!(
            CountryCode::parse("QQ").unwrap().name(LanguageEnum::It),
            "QQ"
        );
    }

    #[test]
    fn the_list_is_by_name_in_the_language() {
        let first = |language| CountryCode::all(language)[0].1;
        assert_eq!(first(LanguageEnum::En), "Afghanistan");
        let italian: Vec<_> = CountryCode::all(LanguageEnum::It)
            .into_iter()
            .map(|(_, name)| name)
            .collect();
        let position = |name| italian.iter().position(|n| *n == name).unwrap();
        // Germany is Germania in Italian, before Giappone
        assert!(position("Germania") < position("Giappone"));
    }

    #[test]
    fn search_by_name_or_code() {
        let names = |query, language| -> Vec<&str> {
            CountryCode::search(query, language)
                .into_iter()
                .map(|(_, name)| name)
                .collect()
        };
        assert_eq!(names("ital", LanguageEnum::En), ["Italy"]);
        // the code first, then the names with it
        assert_eq!(names(" BR", LanguageEnum::Pt)[..2], ["Brasil", "Brunei"]);
        assert!(names("uniti", LanguageEnum::It).contains(&"Stati Uniti"));
        assert_eq!(names("", LanguageEnum::Ja).len(), 249);
        assert!(names("xyz", LanguageEnum::En).is_empty());
    }

    #[test]
    fn the_country_of_a_locale() {
        let of = |locale| CountryCode::from_locale(locale).map(|c| c.to_string());
        assert_eq!(of("it_IT.UTF-8"), Some("IT".into()));
        assert_eq!(of("pt-BR"), Some("BR".into()));
        assert_eq!(of("zh-Hant-TW"), Some("TW".into()));
        assert_eq!(of("en_US@euro"), Some("US".into()));
        // the region chosen apart from the language
        assert_eq!(of("en_US@rg=itzzzz"), Some("IT".into()));
        assert_eq!(of("en-US-u-rg-brzzzz"), Some("BR".into()));
        assert_eq!(of("en_US@rg=zz"), Some("US".into()));
        // the country code of the SIM, lower case
        assert_eq!(of("pl"), Some("PL".into()));
        // a language alone, the region a number, nothing
        assert_eq!(of("en"), None);
        assert_eq!(of("es-419"), None);
        assert_eq!(of("C"), None);
        assert_eq!(of(""), None);
    }

    #[test]
    fn the_guess_takes_the_first_hint_then_the_language() {
        let guess = |hints: &[&str], language| {
            CountryCode::guess(hints.iter().copied(), language).map(|c| c.to_string())
        };
        // SIM empty (no SIM), network then locale
        assert_eq!(
            guess(&["", "fr", "it-IT"], LanguageEnum::It),
            Some("FR".into())
        );
        assert_eq!(
            guess(&["", "", "en-GB"], LanguageEnum::En),
            Some("GB".into())
        );
        assert_eq!(guess(&["", "", "ja"], LanguageEnum::Ja), Some("JP".into()));
        assert_eq!(guess(&[], LanguageEnum::Pt), Some("BR".into()));
        assert_eq!(guess(&["en"], LanguageEnum::En), None);
    }

    #[test]
    fn a_team_saved_before_has_no_country() {
        let team: TeamEntry =
            serde_json::from_str(r#"{"name":"A","year":2024,"players":[]}"#).unwrap();
        assert_eq!(team.country, None);
        let written = serde_json::to_value(&team).unwrap();
        assert_eq!(written.get("country"), None);
    }

    #[test]
    fn a_wrong_country_leaves_the_team_readable() {
        for country in [r#""Italia""#, "12", r#"["IT"]"#, "null"] {
            let json = format!(r#"{{"name":"A","year":2024,"players":[],"country":{country}}}"#);
            let team: TeamEntry = serde_json::from_str(&json).unwrap();
            assert_eq!(team.country, None, "{country}");
        }
    }

    #[test]
    fn the_country_is_saved_and_read_back() {
        let team = TeamEntry {
            name: "A".into(),
            country: Some(code("JP")),
            ..Default::default()
        };
        let written = serde_json::to_value(&team).unwrap();
        assert_eq!(written["country"], "JP");
        let read: TeamEntry = serde_json::from_value(written).unwrap();
        assert_eq!(read.country, Some(code("JP")));
    }

    #[test]
    fn the_default_of_the_settings_keeps_the_other_keys() {
        let defaults: TeamDefaults =
            serde_json::from_str(r#"{"country":"pl","language":"it"}"#).unwrap();
        assert_eq!(defaults.country, Some(code("PL")));
        assert_eq!(
            serde_json::to_string(&TeamDefaults::default()).unwrap(),
            "{}"
        );
    }

    /// The statistics sent tell the country, the gender and the level of
    /// the team; not the names nor the numbers of the players.
    #[tokio::test]
    async fn the_uploaded_metadata_has_the_country_and_no_names() {
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let team = StoreTeamWriter::new(Arc::clone(&store))
            .save(TeamInput::New {
                id: None,
                name: "Volley Club".to_string(),
                year: 2025,
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Women),
                country: Some(code("IT")),
                players: vec![PlayerEntry {
                    name: "Mario Rossi".into(),
                    number: 17,
                    ..Default::default()
                }],
            })
            .await
            .unwrap();
        let m = StoreMatchWriter::new(Arc::clone(&store))
            .create(
                &team,
                "Cortona Volley".to_string(),
                Utc::now().fixed_offset(),
                true,
                DEFAULT_MAX_SUBSTITUTIONS,
                String::new(),
                vec![],
            )
            .await
            .unwrap();
        let zip = create_match_archive(&m, &*store).await.unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(zip)).unwrap();
        let mut content = String::new();
        archive
            .by_name("match.json")
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        let metadata: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(metadata["country"], "IT");
        assert_eq!(metadata["gender"], "Women");
        assert_eq!(metadata["classification"], "Regional");
        assert!(!content.contains("Mario Rossi"));
        assert!(!content.contains("Volley Club"));
        assert!(!content.contains("Cortona"));
    }
}
