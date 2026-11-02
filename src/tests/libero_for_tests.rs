//! The libero for a player of the scout's choice while scouting: "k"
//! before a serve, then the player. In the back row the libero plays in
//! their place, after a rally if it was on court for someone else.

#[cfg(test)]
mod tests {
    use crate::localization::current_labels;
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;

    // the lineup: Anna the setter, in position 1, Bea outside hitter 1,
    // Carla middle blocker 2, Dora the opposite, Elsa outside hitter 2 (in
    // 5), Fede middle blocker 1 (in 6, out for the libero), Gina the libero.
    // We serve.
    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &["Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina"],
        opponents: &["Cortona Volley"],
    }];

    async fn scouting() -> Ui {
        let mut ui = Ui::start_with_sets(TEAM).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Enter])
            .await;
        assert!(choosing_the_event(&mut ui), "not scouting: {}", ui.screen());
        ui
    }

    async fn keys(ui: &mut Ui, keys: &str) {
        for c in keys.chars() {
            ui.press(KeyCode::Char(c)).await;
        }
    }

    fn choosing_the_event(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().choose_the_event)
    }

    fn event_row(ui: &mut Ui, name: &str) -> Option<String> {
        ui.rows_containing(&[name]).into_iter().next()
    }

    /// The names among the players to choose.
    fn player_choices(ui: &mut Ui) -> Vec<String> {
        ui.lines()
            .iter()
            .take_while(|l| !l.contains(current_labels().latest_events))
            .filter_map(|l| l.split('│').nth(1))
            .flat_map(|panel| {
                panel
                    .split_whitespace()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|word| TEAM[0].players.contains(&word.as_str()))
            .collect()
    }

    fn saved_types(ui: &Ui) -> Vec<String> {
        ui.saved_first_set_events()
            .iter()
            .filter_map(|e| e.split(',').nth(1).map(str::to_string))
            .collect()
    }

    /// Before the serve the list offers the libero for a player, on "k":
    /// the players in the game to choose (the one out for the libero too),
    /// not the libero; the choice saved with the player.
    #[tokio::test]
    async fn k_chooses_who_the_libero_is_for() {
        let labels = current_labels().domain;
        let mut ui = scouting().await;
        let row = event_row(&mut ui, labels.libero_for).unwrap_or_default();
        assert!(row.ends_with(" k") || row.contains(" k "), "{row}");
        keys(&mut ui, "k").await;
        let choices = player_choices(&mut ui);
        for name in ["Anna", "Bea", "Carla", "Dora", "Elsa", "Fede"] {
            assert!(choices.contains(&name.to_string()), "{name}: {choices:?}");
        }
        assert!(!choices.contains(&"Gina".to_string()), "{choices:?}");
        keys(&mut ui, "5").await;
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        assert_eq!(saved_types(&ui), vec!["LF"]);
    }

    /// Chosen Elsa, in the back row: the libero leaves Fede at once and
    /// comes in for Elsa after a rally; Fede receives, Elsa doesn't.
    #[tokio::test]
    async fn the_libero_for_the_outside_hitter_after_a_rally() {
        let mut ui = scouting().await;
        keys(&mut ui, "k5").await;
        // our ace, then our serve error: they serve
        keys(&mut ui, "s#").await;
        keys(&mut ui, "s=").await;
        keys(&mut ui, "p").await;
        let choices = player_choices(&mut ui);
        assert!(choices.contains(&"Gina".to_string()), "{choices:?}");
        assert!(choices.contains(&"Fede".to_string()), "{choices:?}");
        assert!(!choices.contains(&"Elsa".to_string()), "{choices:?}");
    }

    /// Within the rally the choice isn't offered: "k" saves nothing.
    #[tokio::test]
    async fn not_within_the_rally() {
        let labels = current_labels().domain;
        let mut ui = scouting().await;
        keys(&mut ui, "s-").await;
        assert_eq!(event_row(&mut ui, labels.libero_for), None);
        let saved = ui.saved_first_set_events().len();
        keys(&mut ui, "k").await;
        assert_eq!(ui.saved_first_set_events().len(), saved);
    }
}
