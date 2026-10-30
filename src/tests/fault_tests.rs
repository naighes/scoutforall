//! Our fault while scouting: "f", then the player making it (e.g. a
//! setter's double) or, with "0", the team (e.g. a rotation fault). Only
//! the fault lists the team.

#[cfg(test)]
mod tests {
    use crate::localization::current_labels;
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;

    // the lineup: Anna the setter (1 in the player choice), Bea outside
    // hitter 1 (2), Carla middle blocker 2 (3), Dora the opposite (4), Elsa
    // outside hitter 2 (5), Fede middle blocker 1 (6), Gina the libero (7).
    // We serve, the setter in position 1.
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

    fn choosing_the_player(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().player_selection)
    }

    /// The row of the player choice with the key, its cells.
    fn choice_row(ui: &mut Ui, key: &str) -> Option<Vec<String>> {
        ui.lines()
            .iter()
            .take_while(|l| !l.contains(current_labels().latest_events))
            .filter_map(|l| l.split('│').nth(1))
            .map(|panel| {
                panel
                    .split_whitespace()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .find(|cells| cells.first().is_some_and(|k| k == key))
    }

    /// The last event saved: event and player.
    fn last_saved(ui: &Ui) -> (String, String) {
        let events = ui.saved_first_set_events();
        let last = events.last().expect("no event saved");
        let columns: Vec<_> = last.split(',').map(str::to_string).collect();
        (columns[1].clone(), columns[2].clone())
    }

    // the rallies before the fault, by the keys typed: our serve error,
    // then they serve; our serve, their reception
    const BEFORE: [(&str, &str); 3] = [
        ("", "we serve"),
        ("s=", "they serve"),
        ("s-", "in the rally"),
    ];

    /// After "f" the team comes first, with "0", then the players; "0"
    /// saves the fault without a player, the score to them.
    #[tokio::test]
    async fn the_team_makes_the_fault_with_0() {
        for (before, case) in BEFORE {
            let mut ui = scouting().await;
            keys(&mut ui, before).await;
            let saved = ui.saved_first_set_events().len();
            keys(&mut ui, "f").await;
            assert!(choosing_the_player(&mut ui), "{case}: {}", ui.screen());
            assert_eq!(
                choice_row(&mut ui, "0"),
                Some(vec!["0".to_string(), current_labels().team.to_string()]),
                "{case}: {}",
                ui.screen()
            );
            let anna = choice_row(&mut ui, "1").unwrap_or_default();
            assert!(anna.contains(&"Anna".to_string()), "{case}: {anna:?}");
            keys(&mut ui, "0").await;
            assert!(choosing_the_event(&mut ui), "{case}: {}", ui.screen());
            assert_eq!(ui.saved_first_set_events().len(), saved + 1, "{case}");
            assert_eq!(last_saved(&ui), ("F".to_string(), String::new()), "{case}");
        }
    }

    /// A player's fault, as ever: her key, her name saved.
    #[tokio::test]
    async fn the_player_makes_the_fault_with_her_key() {
        let mut ui = scouting().await;
        keys(&mut ui, "s-f1").await;
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        let (event, player) = last_saved(&ui);
        assert_eq!(event, "F");
        assert!(!player.is_empty());
        // the serve of the same player, Anna
        assert_eq!(
            player,
            ui.saved_first_set_events()[0]
                .split(',')
                .nth(2)
                .unwrap_or_default()
        );
    }

    /// The other events of a player don't list the team, and "0" saves
    /// nothing.
    #[tokio::test]
    async fn only_the_fault_lists_the_team() {
        for (before, event) in [("s-", "d"), ("s-", "b"), ("s-", "a"), ("", "r")] {
            let mut ui = scouting().await;
            keys(&mut ui, before).await;
            keys(&mut ui, event).await;
            assert!(choosing_the_player(&mut ui), "{event}: {}", ui.screen());
            assert_eq!(choice_row(&mut ui, "0"), None, "{event}: {}", ui.screen());
            let saved = ui.saved_first_set_events().len();
            keys(&mut ui, "0").await;
            assert_eq!(ui.saved_first_set_events().len(), saved, "{event}");
        }
    }
}
