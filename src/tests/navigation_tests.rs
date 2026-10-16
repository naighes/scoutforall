//! List navigation as users see it. In v0.1.0 the list screens stopped
//! reacting to the up/down arrows (only tab and left/right moved the
//! selection): these tests pin the expected behavior on every list screen.

#[cfg(test)]
mod tests {
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;

    const TEAMS: &[TeamFixture] = &[
        TeamFixture {
            name: "Alpha",
            players: &[],
            opponents: &[],
        },
        TeamFixture {
            name: "Bravo",
            players: &[],
            opponents: &[],
        },
        TeamFixture {
            name: "Charlie",
            players: &[],
            opponents: &[],
        },
    ];

    const ONE_TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &["Anna", "Bea", "Carla"],
        opponents: &["Rivals A", "Rivals B", "Rivals C"],
    }];

    /// Checks that down/up and tab/shift-tab move the selection through
    /// `rows`, which must be listed in display order.
    async fn assert_list_navigation(ui: &mut Ui, rows: &[String]) {
        assert!(
            rows.len() >= 3,
            "fixture rows not rendered: {}",
            ui.screen()
        );
        assert_eq!(ui.selected().as_ref(), Some(&rows[0]), "{}", ui.screen());

        ui.press(KeyCode::Down).await;
        assert_eq!(
            ui.selected().as_ref(),
            Some(&rows[1]),
            "down: {}",
            ui.screen()
        );
        ui.press(KeyCode::Down).await;
        assert_eq!(
            ui.selected().as_ref(),
            Some(&rows[2]),
            "down: {}",
            ui.screen()
        );
        ui.press(KeyCode::Up).await;
        assert_eq!(
            ui.selected().as_ref(),
            Some(&rows[1]),
            "up: {}",
            ui.screen()
        );

        ui.press(KeyCode::Tab).await;
        assert_eq!(
            ui.selected().as_ref(),
            Some(&rows[2]),
            "tab: {}",
            ui.screen()
        );
        ui.press(KeyCode::BackTab).await;
        assert_eq!(
            ui.selected().as_ref(),
            Some(&rows[1]),
            "shift-tab: {}",
            ui.screen()
        );
    }

    fn assert_footer_advertises_arrows(ui: &mut Ui) {
        let footer = ui.footer();
        assert!(footer.contains("⬆ = previous"), "{footer}");
        assert!(footer.contains("⬇ = next"), "{footer}");
        assert!(
            !footer.contains("⇥"),
            "tab should not be advertised: {footer}"
        );
    }

    #[tokio::test]
    async fn team_list_arrow_navigation() {
        let mut ui = Ui::start(TEAMS, &[]).await;
        let rows = ui.rows_containing(&["Alpha", "Bravo", "Charlie"]);
        assert_list_navigation(&mut ui, &rows).await;
        assert_footer_advertises_arrows(&mut ui);
    }

    #[tokio::test]
    async fn players_list_arrow_navigation() {
        let mut ui = Ui::start(ONE_TEAM, &[]).await;
        ui.press(KeyCode::Enter).await;
        let rows = ui.rows_containing(&["Anna", "Bea", "Carla"]);
        assert_list_navigation(&mut ui, &rows).await;
        assert_footer_advertises_arrows(&mut ui);
    }

    #[tokio::test]
    async fn match_list_arrow_navigation() {
        let mut ui = Ui::start(ONE_TEAM, &[]).await;
        ui.press(KeyCode::Enter).await;
        ui.press(KeyCode::Char('m')).await;
        let rows = ui.rows_containing(&["Rivals A", "Rivals B", "Rivals C"]);
        assert_list_navigation(&mut ui, &rows).await;
        assert_footer_advertises_arrows(&mut ui);
    }

    #[tokio::test]
    async fn file_system_arrow_navigation() {
        let mut ui = Ui::start(TEAMS, &["folder-a", "folder-b", "folder-c"]).await;
        ui.press(KeyCode::Char('i')).await;
        let rows = ui.rows_containing(&["folder-a", "folder-b", "folder-c"]);
        assert_list_navigation(&mut ui, &rows).await;
        assert_footer_advertises_arrows(&mut ui);
    }

    /// Every screen advertising "q = quit" in its footer must actually quit.
    #[tokio::test]
    async fn quit_from_every_screen_advertising_it() {
        // keys to reach each screen from the team list
        let paths: &[(&str, &[KeyCode])] = &[
            ("team list", &[]),
            ("team details", &[KeyCode::Enter]),
            ("match list", &[KeyCode::Enter, KeyCode::Char('m')]),
            (
                "match stats",
                &[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Char(' ')],
            ),
            ("import team", &[KeyCode::Char('i')]),
            ("settings", &[KeyCode::Char('s')]),
        ];
        for (name, keys) in paths {
            let mut ui = Ui::start(ONE_TEAM, &["folder"]).await;
            for key in keys.iter() {
                assert!(!ui.press(*key).await, "{name}: quit while navigating");
            }
            let footer = ui.footer();
            assert!(footer.contains("q = quit"), "{name}: {}", ui.screen());
            assert!(
                ui.press(KeyCode::Char('q')).await,
                "{name}: {}",
                ui.screen()
            );
        }
    }
}
