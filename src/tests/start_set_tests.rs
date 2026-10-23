//! Starting a set: who serves, the lineup, the setter and the liberos, as
//! the core's `SetStartForm` allows. Up to v0.9.0 the lineup offered the
//! removed players too, and counted them for the fallback libero.

#[cfg(test)]
mod tests {
    use crate::localization::current_labels;
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;
    use std::path::Path;

    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &[
            "Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina", "Ilaria",
        ],
        opponents: &["Cortona Volley"],
    }];

    /// Opens the start of the first set of the match, on who serves.
    async fn start_set(ui: &mut Ui) {
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Enter])
            .await;
        assert!(
            ui.screen().contains(current_labels().serving_team),
            "not starting the set: {}",
            ui.screen()
        );
    }

    /// Removes the last player of every team, as the team page does.
    fn remove_last_player(base: &Path) {
        for team in std::fs::read_dir(base).unwrap() {
            let path = team.unwrap().path().join("team.json");
            let mut value: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let players = value["players"].as_array_mut().unwrap();
            players.last_mut().unwrap()["deleted"] = true.into();
            std::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        }
    }

    fn is_scouting(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().choose_the_event)
    }

    #[tokio::test]
    async fn the_lineup_the_setter_and_the_liberos_start_the_set() {
        let mut ui = Ui::start(TEAM, &[]).await;
        start_set(&mut ui).await;
        // we serve, the first six on court, the first the setter
        ui.type_keys(&[KeyCode::Left, KeyCode::Enter]).await;
        for _ in 0..7 {
            ui.press(KeyCode::Enter).await;
        }
        assert!(
            ui.screen()
                .contains(current_labels().lineup_selection_libero),
            "not choosing the libero: {}",
            ui.screen()
        );
        // Gina the libero, Ilaria the fallback one
        ui.type_keys(&[KeyCode::Enter, KeyCode::Enter]).await;
        assert!(is_scouting(&mut ui), "not scouting: {}", ui.screen());
        let set = &ui.saved_first_sets()[0];
        assert_eq!(set["serving_team"], "Us");
        assert_eq!(set["initial_positions"].as_array().unwrap().len(), 6);
        assert_eq!(set["setter"], set["initial_positions"][0]);
        assert!(set["libero"].is_string());
        assert!(set["fallback_libero"].is_string());
    }

    #[tokio::test]
    async fn back_from_the_setter_frees_position_six() {
        let mut ui = Ui::start(TEAM, &[]).await;
        start_set(&mut ui).await;
        ui.type_keys(&[KeyCode::Left, KeyCode::Enter]).await;
        for _ in 0..6 {
            ui.press(KeyCode::Enter).await;
        }
        ui.press(KeyCode::Esc).await;
        assert!(ui.screen().contains("pos 6"), "{}", ui.screen());
        assert_eq!(ui.rows_containing(&["Fede"]).len(), 1, "{}", ui.screen());
    }

    #[tokio::test]
    async fn removed_players_are_neither_offered_nor_counted() {
        let mut ui = Ui::start_editing(TEAM, remove_last_player).await;
        start_set(&mut ui).await;
        ui.type_keys(&[KeyCode::Left, KeyCode::Enter]).await;
        assert!(
            !ui.screen().contains("Ilaria"),
            "removed player offered: {}",
            ui.screen()
        );
        for _ in 0..7 {
            ui.press(KeyCode::Enter).await;
        }
        // seven active players: no fallback libero to choose
        ui.press(KeyCode::Enter).await;
        assert!(is_scouting(&mut ui), "not scouting: {}", ui.screen());
        assert!(ui.saved_first_sets()[0]["fallback_libero"].is_null());
    }
}
