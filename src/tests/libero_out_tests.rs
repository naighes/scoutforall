//! The libero out while scouting: "l" before a serve, and the middle
//! blocker plays in the back row until "l" again brings the libero back.
//! The list of the events says what "l" does now.

#[cfg(test)]
mod tests {
    use crate::localization::current_labels;
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;

    // the lineup: Anna the setter, in position 1, Bea outside hitter 1,
    // Carla middle blocker 2, Dora the opposite, Elsa outside hitter 2, Fede
    // middle blocker 1 (in 6, out for the libero), Gina the libero. We
    // serve.
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

    /// The row of the event in the list, with its key.
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

    /// Before the serve the list offers the libero out, on "l"; once out,
    /// back in. Each saved, the scout still choosing the event.
    #[tokio::test]
    async fn l_takes_the_libero_out_and_back_in() {
        let labels = current_labels().domain;
        let mut ui = scouting().await;
        let row = event_row(&mut ui, labels.libero_out).unwrap_or_default();
        assert!(row.ends_with(" l") || row.contains(" l "), "{row}");
        assert_eq!(event_row(&mut ui, labels.libero_in), None);
        keys(&mut ui, "l").await;
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        assert_eq!(saved_types(&ui), vec!["LR"]);
        assert!(
            event_row(&mut ui, labels.libero_in).is_some(),
            "{}",
            ui.screen()
        );
        assert_eq!(event_row(&mut ui, labels.libero_out), None);
        keys(&mut ui, "l").await;
        assert_eq!(saved_types(&ui), vec!["LR", "LR"]);
        assert!(
            event_row(&mut ui, labels.libero_out).is_some(),
            "{}",
            ui.screen()
        );
    }

    /// Out, the middle blocker receives in place of the libero, until the
    /// libero comes back in.
    #[tokio::test]
    async fn out_the_middle_blocker_receives() {
        let mut ui = scouting().await;
        // our serve error: they serve, the libero in for Fede
        keys(&mut ui, "s=").await;
        keys(&mut ui, "p").await;
        let choices = player_choices(&mut ui);
        assert!(choices.contains(&"Gina".to_string()), "{choices:?}");
        assert!(!choices.contains(&"Fede".to_string()), "{choices:?}");
        // undo the choice of the reception
        ui.press(KeyCode::Char('U')).await;
        keys(&mut ui, "l").await;
        keys(&mut ui, "p").await;
        let choices = player_choices(&mut ui);
        assert!(!choices.contains(&"Gina".to_string()), "{choices:?}");
        assert!(choices.contains(&"Fede".to_string()), "{choices:?}");
        // undo the choice of the reception
        ui.press(KeyCode::Char('U')).await;
        keys(&mut ui, "l").await;
        keys(&mut ui, "p").await;
        let choices = player_choices(&mut ui);
        assert!(choices.contains(&"Gina".to_string()), "{choices:?}");
        assert!(!choices.contains(&"Fede".to_string()), "{choices:?}");
    }

    /// Within the rally the libero doesn't go out: "l" saves nothing.
    #[tokio::test]
    async fn not_within_the_rally() {
        let labels = current_labels().domain;
        let mut ui = scouting().await;
        keys(&mut ui, "s-").await;
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        assert_eq!(event_row(&mut ui, labels.libero_out), None);
        let saved = ui.saved_first_set_events().len();
        keys(&mut ui, "l").await;
        assert_eq!(ui.saved_first_set_events().len(), saved);
    }
}
