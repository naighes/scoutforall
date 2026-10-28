//! The roster of a match, the players on its score sheet: chosen after the
//! match form, from 7 to 14; who played in a set stays on it.

#[cfg(test)]
mod tests {
    use crate::localization::current_labels;
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;

    /// Fifteen players, one more than the score sheet takes, numbered from
    /// 1 in this order.
    const FIFTEEN: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &[
            "Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina", "Ilaria", "Laura", "Marta",
            "Nina", "Olga", "Paola", "Rita", "Sara",
        ],
        opponents: &["Cortona Volley"],
    }];

    fn chars(s: &str) -> Vec<KeyCode> {
        s.chars().map(KeyCode::Char).collect()
    }

    fn title(chosen: usize) -> String {
        format!("{} - {chosen}/14", current_labels().roster)
    }

    /// The number of the players of the roster saved, by `numbers` of the
    /// team.
    fn roster_numbers(ui: &Ui) -> Vec<u64> {
        let team = &ui.saved_teams()[0];
        let number = |id: &serde_json::Value| {
            team["players"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| &p["id"] == id)
                .and_then(|p| p["number"].as_u64())
                .unwrap()
        };
        ui.saved_matches()
            .iter()
            .find(|m| m["opponent"] == "Arezzo")
            .map(|m| m["roster"].as_array().unwrap().iter().map(number).collect())
            .unwrap_or_default()
    }

    /// A new match: the form, then the roster with the active players.
    #[tokio::test]
    async fn a_new_match_chooses_its_roster_when_the_team_is_larger() {
        let mut ui = Ui::start(FIFTEEN, &[]).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Char('n')])
            .await;
        ui.type_keys(&chars("Arezzo")).await;
        ui.press(KeyCode::Tab).await;
        ui.type_keys(&chars("20260920")).await;
        ui.press(KeyCode::Enter).await;
        assert!(ui.screen().contains(&title(15)), "{}", ui.screen());
        // 15 is too many
        ui.press(KeyCode::Enter).await;
        let error: String = current_labels()
            .domain
            .roster_too_large
            .chars()
            .take(30)
            .collect();
        assert!(ui.screen().contains(&error), "{}", ui.screen());
        assert!(roster_numbers(&ui).is_empty());
        // the banner away, the first player off, with space
        ui.press(KeyCode::Esc).await;
        ui.press(KeyCode::Char(' ')).await;
        assert!(ui.screen().contains(&title(14)), "{}", ui.screen());
        // the second one off and on again, with the arrows
        ui.type_keys(&[KeyCode::Down, KeyCode::Right, KeyCode::Left])
            .await;
        ui.press(KeyCode::Enter).await;
        assert_eq!(roster_numbers(&ui), (2..=15).collect::<Vec<_>>());
        // on to the first set
        assert!(
            ui.screen().contains(current_labels().serving_team),
            "{}",
            ui.screen()
        );
    }

    /// Who played in a set stays: the row doesn't change; someone who
    /// didn't leaves.
    #[tokio::test]
    async fn who_played_stays_on_the_roster() {
        let mut ui = Ui::start_with_sets(FIFTEEN).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Char('e')])
            .await;
        ui.type_keys(&[KeyCode::Backspace; "Cortona Volley".len()])
            .await;
        ui.type_keys(&chars("Arezzo")).await;
        ui.press(KeyCode::Enter).await;
        // a match saved before v0.11: the whole team, too many
        assert!(ui.screen().contains(&title(15)), "{}", ui.screen());
        assert_eq!(ui.rows_containing(&["[*]"]).len(), 7, "{}", ui.screen());
        ui.press(KeyCode::Char(' ')).await;
        assert!(ui.screen().contains(&title(15)), "{}", ui.screen());
        for _ in 0..14 {
            ui.press(KeyCode::Down).await;
        }
        ui.press(KeyCode::Char(' ')).await;
        ui.press(KeyCode::Enter).await;
        assert_eq!(roster_numbers(&ui), (1..=14).collect::<Vec<_>>());
        // back on the matches
        assert_eq!(ui.rows_containing(&["Arezzo"]).len(), 1, "{}", ui.screen());
    }

    /// In the smallest terminal the list scrolls to the player selected.
    #[tokio::test]
    async fn the_roster_scrolls_to_the_player_selected() {
        let mut ui = Ui::start_with_sets(FIFTEEN).await;
        ui.resize(80, 24);
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Char('e')])
            .await;
        ui.press(KeyCode::Enter).await;
        assert!(ui.screen().contains(&title(15)), "{}", ui.screen());
        for _ in 0..14 {
            ui.press(KeyCode::Down).await;
        }
        assert!(
            ui.selected().is_some_and(|row| row.contains("Sara")),
            "{}",
            ui.screen()
        );
    }
}
