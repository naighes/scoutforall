//! The terms of use (the program is free, its license, the data it sends)
//! are shown at startup until the user has seen them, also to the users of
//! the versions before them. The statistics are sent only once the terms are
//! seen, unless the user unticks them.

#[cfg(test)]
mod tests {
    use crate::{
        shapes::settings::{Settings, TERMS_VERSION},
        tests::ui::{TeamFixture, Ui},
    };
    use crokey::crossterm::event::KeyCode;
    use scout4all_core::{localization::current_labels, shapes::enums::LanguageEnum};

    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &[],
        opponents: &[],
    }];

    fn shows_terms(ui: &mut Ui) -> bool {
        ui.screen()
            .contains(&format!("┌{}", current_labels().terms_of_use))
    }

    fn shows_teams(ui: &mut Ui) -> bool {
        ui.screen().contains("Volley Club")
    }

    #[tokio::test]
    async fn the_terms_are_shown_at_the_first_startup() {
        let mut ui = Ui::first_startup(TEAM).await;
        assert!(shows_terms(&mut ui), "{}", ui.screen());
        assert!(!shows_teams(&mut ui), "{}", ui.screen());
        let checked = format!("[X] {}", current_labels().enable_send_analytics);
        assert!(ui.screen().contains(&checked), "{}", ui.screen());
    }

    #[tokio::test]
    async fn going_on_saves_the_terms_as_seen_and_shows_the_teams() {
        let mut ui = Ui::first_startup(TEAM).await;
        ui.press(KeyCode::Enter).await;
        assert!(shows_teams(&mut ui), "{}", ui.screen());
        let saved = ui.saved_settings().await.unwrap();
        assert_eq!(saved.terms_version, TERMS_VERSION);
        assert!(saved.analytics_enabled);
        assert!(saved.uploads_enabled());
    }

    #[tokio::test]
    async fn space_unticks_the_statistics() {
        let mut ui = Ui::first_startup(TEAM).await;
        ui.press(KeyCode::Char(' ')).await;
        let unchecked = format!("[ ] {}", current_labels().enable_send_analytics);
        assert!(ui.screen().contains(&unchecked), "{}", ui.screen());
        ui.press(KeyCode::Enter).await;
        let saved = ui.saved_settings().await.unwrap();
        assert_eq!(saved.terms_version, TERMS_VERSION);
        assert!(!saved.analytics_enabled);
        assert!(!saved.uploads_enabled());
    }

    #[tokio::test]
    async fn the_arrows_choose_the_language() {
        let mut ui = Ui::first_startup(TEAM).await;
        ui.press(KeyCode::Right).await;
        ui.press(KeyCode::Enter).await;
        let saved = ui.saved_settings().await.unwrap();
        assert_eq!(saved.language, LanguageEnum::It);
    }

    #[tokio::test]
    async fn quitting_leaves_the_terms_to_show_again() {
        let mut ui = Ui::first_startup(TEAM).await;
        ui.press(KeyCode::Char(' ')).await;
        assert!(ui.press(KeyCode::Char('q')).await);
        assert!(ui.saved_settings().await.is_none());
    }

    /// The users of the versions before the terms see them too, with the
    /// choice they made in the settings.
    #[tokio::test]
    async fn the_users_of_older_versions_see_the_terms_with_their_choice() {
        let settings = Settings {
            analytics_enabled: false,
            ..Settings::default()
        };
        let mut ui = Ui::start_as(TEAM, settings).await;
        assert!(shows_terms(&mut ui), "{}", ui.screen());
        let unchecked = format!("[ ] {}", current_labels().enable_send_analytics);
        assert!(ui.screen().contains(&unchecked), "{}", ui.screen());
    }

    #[tokio::test]
    async fn the_terms_seen_are_not_shown_again() {
        let settings = Settings {
            terms_version: TERMS_VERSION,
            ..Settings::default()
        };
        let mut ui = Ui::start_as(TEAM, settings).await;
        assert!(!shows_terms(&mut ui), "{}", ui.screen());
        assert!(shows_teams(&mut ui), "{}", ui.screen());
    }

    #[test]
    fn no_statistics_are_sent_before_the_terms_are_seen() {
        let unseen = Settings::default();
        assert!(unseen.analytics_enabled);
        assert!(!unseen.uploads_enabled());
    }

    /// On a small terminal the terms scroll down to their end, and no
    /// further: the first arrow up scrolls back right away.
    #[tokio::test]
    async fn the_terms_scroll_to_their_end_on_a_small_terminal() {
        let mut ui = Ui::first_startup(TEAM).await;
        ui.resize(60, 20);
        let contact = "nic.baldi@gmail.com";
        assert!(!ui.screen().contains(contact), "{}", ui.screen());
        for _ in 0..100 {
            ui.press(KeyCode::Down).await;
        }
        let end = ui.screen();
        assert!(end.contains(contact), "{end}");
        ui.press(KeyCode::Up).await;
        assert_ne!(ui.screen(), end);
    }

    #[tokio::test]
    async fn the_terms_fit_a_large_terminal() {
        let mut ui = Ui::first_startup(TEAM).await;
        ui.resize(200, 60);
        let screen = ui.screen();
        assert!(screen.contains("AGPL-3.0-only"), "{screen}");
        assert!(screen.contains("nic.baldi@gmail.com"), "{screen}");
    }
}
