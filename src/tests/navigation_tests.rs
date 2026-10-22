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

    /// The arrows, with tab and shift tab moving the same way.
    fn assert_footer_advertises_arrows(ui: &mut Ui) {
        let footer = ui.footer();
        assert!(footer.contains("⬆/⇧-⇥ = previous"), "{footer}");
        assert!(footer.contains("⬇/⇥ = next"), "{footer}");
    }

    /// Tab and shift tab move the selection as down and up do.
    async fn assert_tab_moves_as_the_arrows(ui: &mut Ui) {
        let first = ui.selected();
        ui.press(KeyCode::Down).await;
        let second = ui.selected();
        assert_ne!(first, second, "down: {}", ui.screen());
        ui.press(KeyCode::Up).await;
        assert_eq!(ui.selected(), first, "up: {}", ui.screen());
        ui.press(KeyCode::Tab).await;
        assert_eq!(ui.selected(), second, "tab: {}", ui.screen());
        ui.press(KeyCode::BackTab).await;
        assert_eq!(ui.selected(), first, "shift-tab: {}", ui.screen());
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

    const SEVEN_PLAYERS: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &["Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina"],
        opponents: &["Rivals A"],
    }];

    /// The events of the first set: tab moves through them too.
    #[tokio::test]
    async fn set_events_tab_navigation() {
        let mut ui = Ui::start_with_sets(SEVEN_PLAYERS).await;
        // three aces of ours, then the events of the set
        for key in "\nm\ns#s#s#\x1bv\n".chars() {
            ui.press(match key {
                '\n' => KeyCode::Enter,
                '\x1b' => KeyCode::Esc,
                c => KeyCode::Char(c),
            })
            .await;
        }
        assert_tab_moves_as_the_arrows(&mut ui).await;
        assert_footer_advertises_arrows(&mut ui);
    }

    /// The selected entry of a panel of the statistics.
    fn selected_in(ui: &mut Ui, panel: &str) -> String {
        ui.lines()
            .into_iter()
            .skip_while(|l| !l.contains(&format!("┌{panel}─")))
            .find_map(|l| {
                l.find(">> ").map(|i| {
                    l[i + 3..]
                        .split('│')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string()
                })
            })
            .unwrap_or_else(|| panic!("no selection in {panel}: {}", ui.screen()))
    }

    /// In the statistics, down selects the entry below in the filter having
    /// the focus, up the one above, going round; tab and shift tab (as right
    /// and left) move the focus to the next and the previous filter.
    #[tokio::test]
    async fn match_stats_arrow_navigation() {
        let mut ui = Ui::start_with_sets(SEVEN_PLAYERS).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Char(' ')])
            .await;
        // the sets: all of them, then the first one
        assert_eq!(selected_in(&mut ui, "set"), "-");
        ui.press(KeyCode::Down).await;
        assert_eq!(selected_in(&mut ui, "set"), "1", "{}", ui.screen());
        ui.press(KeyCode::Up).await;
        assert_eq!(selected_in(&mut ui, "set"), "-");
        ui.press(KeyCode::Up).await;
        assert_eq!(selected_in(&mut ui, "set"), "1", "round: {}", ui.screen());
        // the rotations have the focus: the sets stay as they are
        for (forward, back) in [
            (KeyCode::Tab, KeyCode::BackTab),
            (KeyCode::Right, KeyCode::Left),
        ] {
            ui.press(forward).await;
            ui.press(KeyCode::Down).await;
            assert_eq!(selected_in(&mut ui, "rotation"), "1", "{}", ui.screen());
            assert_eq!(selected_in(&mut ui, "set"), "1");
            ui.press(KeyCode::Up).await;
            ui.press(back).await;
            ui.press(KeyCode::Down).await;
            assert_eq!(selected_in(&mut ui, "set"), "-", "{}", ui.screen());
            ui.press(KeyCode::Up).await;
        }
        let footer = ui.footer();
        assert!(footer.contains("⬅/⇧-⇥ = previous"), "{footer}");
        assert!(footer.contains("⮕/⇥ = next"), "{footer}");
    }
}
