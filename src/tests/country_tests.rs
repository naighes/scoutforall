//! The country of a team: proposed from the settings, chosen in the team
//! form by typing and with left and right, required; in the settings, a
//! screen of its own; guessed from the system on the first start.

#[cfg(test)]
mod tests {
    use crate::localization::current_labels;
    use crate::{
        shapes::settings::Settings,
        tests::ui::{TeamFixture, Ui},
    };
    use crokey::crossterm::event::KeyCode;
    use scout4all_core::{
        settings::{Consent, TeamDefaults},
        shapes::{country::CountryCode, enums::LanguageEnum},
    };

    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &[],
        opponents: &[],
    }];

    fn chars(s: &str) -> Vec<KeyCode> {
        s.chars().map(KeyCode::Char).collect()
    }

    fn country(code: &str) -> Option<CountryCode> {
        CountryCode::known(code)
    }

    /// The terms seen, the country of a new team `code`.
    fn settings(code: Option<&str>) -> Settings {
        Settings {
            consent: Consent::default().with_terms_seen(),
            team_defaults: TeamDefaults {
                country: code.and_then(country),
            },
            ..Settings::default()
        }
    }

    /// The form of a new team, on the country.
    async fn new_team_on_the_country(code: Option<&str>) -> Ui {
        let mut ui = Ui::start_as(TEAM, settings(code)).await;
        ui.press(KeyCode::Char('n')).await;
        ui.type_keys(&chars("Cortona")).await;
        // classification, gender, country, year, then back to the country
        ui.type_keys(&[KeyCode::Down; 4]).await;
        ui.type_keys(&chars("2026")).await;
        ui.press(KeyCode::Up).await;
        ui
    }

    fn saved_country(ui: &Ui, name: &str) -> Option<String> {
        ui.saved_teams()
            .into_iter()
            .find(|team| team["name"] == name)
            .and_then(|team| team["country"].as_str().map(String::from))
    }

    #[tokio::test]
    async fn a_new_team_starts_with_the_country_of_the_settings() {
        let mut ui = new_team_on_the_country(Some("IT")).await;
        assert!(ui.screen().contains("◀ Italy ▶"), "{}", ui.screen());
        ui.type_keys(&[KeyCode::Down, KeyCode::Enter]).await;
        assert!(
            ui.screen().contains(current_labels().operation_successful),
            "{}",
            ui.screen()
        );
        assert_eq!(saved_country(&ui, "Cortona").as_deref(), Some("IT"));
    }

    #[tokio::test]
    async fn typing_finds_the_country_and_the_arrows_go_through_the_others() {
        let mut ui = new_team_on_the_country(Some("IT")).await;
        ui.type_keys(&chars("united")).await;
        let screen = ui.screen();
        assert!(screen.contains("united█"), "{screen}");
        assert!(screen.contains("◀ United Arab Emirates ▶"), "{screen}");
        ui.press(KeyCode::Right).await;
        assert!(
            ui.screen().contains("◀ United Kingdom ▶"),
            "{}",
            ui.screen()
        );
        ui.press(KeyCode::Left).await;
        ui.press(KeyCode::Left).await;
        // round to the last one found
        assert!(ui.screen().contains("◀ United States ▶"), "{}", ui.screen());
        // nothing found keeps the choice
        ui.type_keys(&chars("xx")).await;
        assert!(
            ui.screen().contains(current_labels().no_country_found),
            "{}",
            ui.screen()
        );
        ui.type_keys(&[KeyCode::Down, KeyCode::Enter]).await;
        assert_eq!(saved_country(&ui, "Cortona").as_deref(), Some("US"));
    }

    #[tokio::test]
    async fn the_code_finds_the_country_first() {
        let mut ui = new_team_on_the_country(None).await;
        ui.type_keys(&chars("jp")).await;
        assert!(ui.screen().contains("◀ Japan ▶"), "{}", ui.screen());
        ui.press(KeyCode::Backspace).await;
        ui.press(KeyCode::Backspace).await;
        // back to no text: the choice stays, the hint comes back
        let screen = ui.screen();
        assert!(screen.contains("◀ Japan ▶"), "{screen}");
        assert!(screen.contains(current_labels().type_to_search), "{screen}");
    }

    #[tokio::test]
    async fn a_team_needs_a_country() {
        let mut ui = new_team_on_the_country(None).await;
        ui.type_keys(&[KeyCode::Down, KeyCode::Enter]).await;
        assert!(
            ui.screen()
                .contains(current_labels().domain.country_is_required),
            "{}",
            ui.screen()
        );
        assert_eq!(saved_country(&ui, "Cortona"), None);
    }

    /// A team saved before v0.12 gets the country of the settings, shown
    /// in the form, once saved.
    #[tokio::test]
    async fn editing_a_team_without_country_proposes_the_one_of_the_settings() {
        let mut ui = Ui::start_as(TEAM, settings(Some("PL"))).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('e')]).await;
        let screen = ui.screen();
        assert!(screen.contains("Poland"), "{screen}");
        ui.press(KeyCode::Enter).await;
        assert_eq!(saved_country(&ui, "Volley Club").as_deref(), Some("PL"));
    }

    #[tokio::test]
    async fn the_settings_choose_the_country_in_a_screen() {
        let mut ui = Ui::start_as(TEAM, settings(Some("IT"))).await;
        ui.press(KeyCode::F(2)).await;
        ui.press(KeyCode::Down).await;
        let row = ui.selected().unwrap_or_default();
        assert!(row.contains("Italy"), "{row}");
        ui.press(KeyCode::Enter).await;
        // the whole list, the country chosen highlighted
        assert_eq!(ui.selected().as_deref(), Some("Italy (IT)"));
        assert!(ui.screen().contains("Israel (IL)"), "{}", ui.screen());
        ui.type_keys(&chars("bra")).await;
        assert_eq!(ui.selected().as_deref(), Some("Brazil (BR)"));
        assert!(!ui.screen().contains("Italy"), "{}", ui.screen());
        ui.press(KeyCode::Down).await;
        assert_eq!(ui.selected().as_deref(), Some("Congo - Brazzaville (CG)"));
        ui.press(KeyCode::Down).await;
        assert_eq!(ui.selected().as_deref(), Some("Gibraltar (GI)"));
        ui.press(KeyCode::Enter).await;
        let saved = ui.saved_settings().await.unwrap();
        assert_eq!(saved.team_defaults.country, country("GI"));
        // back on the settings, the other ones kept
        assert!(ui.screen().contains("Gibraltar ▶"), "{}", ui.screen());
        assert!(
            ui.saved_settings_json()
                .await
                .get("terms_version")
                .is_some()
        );
    }

    #[tokio::test]
    async fn esc_leaves_the_country_as_it_was() {
        let mut ui = Ui::start_as(TEAM, settings(Some("IT"))).await;
        ui.type_keys(&[KeyCode::F(2), KeyCode::Down, KeyCode::Enter])
            .await;
        ui.type_keys(&chars("bra")).await;
        ui.press(KeyCode::Esc).await;
        assert!(ui.screen().contains("Italy ▶"), "{}", ui.screen());
        assert_eq!(ui.saved_settings().await.map(|s| s.team_defaults), None);
    }

    #[test]
    fn the_country_is_guessed_from_the_locales_then_the_language() {
        let guessed = |settings: &Settings, locales: &[&str]| {
            let locales: Vec<String> = locales.iter().map(|l| l.to_string()).collect();
            settings
                .with_guessed_country(&locales)
                .and_then(|s| s.team_defaults.country)
        };
        let english = Settings::default();
        assert_eq!(guessed(&english, &["en-GB", "it-IT"]), country("GB"));
        assert_eq!(guessed(&english, &["en"]), None);
        let japanese = Settings {
            language: LanguageEnum::Ja,
            ..Settings::default()
        };
        assert_eq!(guessed(&japanese, &[]), country("JP"));
        // once chosen, never guessed again
        assert_eq!(guessed(&settings(Some("IT")), &["fr-FR"]), None);
    }
}
