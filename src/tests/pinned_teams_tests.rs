//! The pinned teams: at the top of the list with a mark, pinned and
//! unpinned with space, the new teams pinned once saved; kept in
//! `config.json` with the other settings.

#[cfg(test)]
mod tests {
    use crate::{
        localization::current_labels,
        screens::team_list_screen::PINNED,
        shapes::settings::Settings,
        tests::ui::{TeamFixture, Ui},
    };
    use crokey::crossterm::event::KeyCode;
    use scout4all_core::{
        settings::{Consent, TeamDefaults},
        shapes::country::CountryCode,
    };

    const TEAMS: &[TeamFixture] = &[
        TeamFixture {
            name: "Zeta",
            players: &[],
            opponents: &[],
        },
        TeamFixture {
            name: "alfa",
            players: &[],
            opponents: &[],
        },
        TeamFixture {
            name: "Beta",
            players: &[],
            opponents: &[],
        },
    ];

    fn chars(s: &str) -> Vec<KeyCode> {
        s.chars().map(KeyCode::Char).collect()
    }

    /// The names of the teams as listed, a pinned one with its mark.
    fn listed(ui: &mut Ui) -> Vec<String> {
        ui.rows_containing(&["2025)", "2026)"])
            .into_iter()
            .map(|row| row.split(" (").next().unwrap_or_default().to_string())
            .collect()
    }

    fn pinned_mark(name: &str) -> String {
        format!("{PINNED}{name}")
    }

    #[tokio::test]
    async fn the_teams_are_listed_by_name() {
        let mut ui = Ui::start(TEAMS, &[]).await;
        assert_eq!(listed(&mut ui), ["alfa", "Beta", "Zeta"]);
    }

    #[tokio::test]
    async fn space_pins_the_selected_team_to_the_top_and_unpins_it() {
        let mut ui = Ui::start(TEAMS, &[]).await;
        // Zeta, the last one
        ui.type_keys(&[KeyCode::Down, KeyCode::Down]).await;
        assert!(
            ui.footer().contains(current_labels().pin),
            "{}",
            ui.footer()
        );
        ui.press(KeyCode::Char(' ')).await;
        assert_eq!(
            listed(&mut ui),
            [pinned_mark("Zeta"), "alfa".into(), "Beta".into()]
        );
        // still the one selected, now to unpin
        assert_eq!(
            ui.selected(),
            Some(format!("{} (2025)", pinned_mark("Zeta")))
        );
        assert!(
            ui.footer().contains(current_labels().unpin),
            "{}",
            ui.footer()
        );
        let saved = ui.saved_settings_json().await;
        assert_eq!(
            saved["pinned_teams"].as_array().map(Vec::len),
            Some(1),
            "{saved}"
        );

        ui.press(KeyCode::Char(' ')).await;
        assert_eq!(listed(&mut ui), ["alfa", "Beta", "Zeta"]);
        assert_eq!(ui.selected(), Some("Zeta (2025)".to_string()));
        let saved = ui.saved_settings_json().await;
        assert_eq!(saved.get("pinned_teams"), None, "{saved}");
    }

    #[tokio::test]
    async fn a_team_created_by_hand_is_pinned() {
        // the country of the settings: the form requires one
        let settings = Settings {
            consent: Consent::default().with_terms_seen(),
            team_defaults: TeamDefaults {
                country: CountryCode::known("IT"),
            },
            ..Settings::default()
        };
        let mut ui = Ui::start_as(TEAMS, settings).await;
        ui.press(KeyCode::Char('n')).await;
        ui.type_keys(&chars("Cortona")).await;
        // classification, gender, country, year
        ui.type_keys(&[KeyCode::Down; 4]).await;
        ui.type_keys(&chars("2026")).await;
        ui.press(KeyCode::Enter).await;
        // the banner, then back to the list
        ui.press(KeyCode::Enter).await;
        assert_eq!(
            listed(&mut ui),
            [
                pinned_mark("Cortona"),
                "alfa".into(),
                "Beta".into(),
                "Zeta".into()
            ]
        );
    }

    /// The pins don't touch the other settings in `config.json`.
    #[tokio::test]
    async fn pinning_keeps_the_other_settings() {
        let mut ui = Ui::start(TEAMS, &[]).await;
        ui.press(KeyCode::Char(' ')).await;
        let saved = ui.saved_settings_json().await;
        assert!(saved.get("language").is_some(), "{saved}");
        assert!(saved.get("terms_version").is_some(), "{saved}");
    }
}
