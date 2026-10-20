//! Editing the match info from the match list: the opponent, the date, home
//! and the substitutions per set change, the sets scouted stay, and the
//! substitutions can't go below the ones already made in a set.

#[cfg(test)]
mod tests {
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;
    use scout4all_core::localization::current_labels;
    use std::path::{Path, PathBuf};

    /// Six on court, the libero and two on the bench.
    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &[
            "Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina", "Ilaria", "Laura",
        ],
        opponents: &["Cortona Volley"],
    }];

    fn chars(s: &str) -> Vec<KeyCode> {
        s.chars().map(KeyCode::Char).collect()
    }

    fn substitutions_field(n: &str) -> String {
        format!("{}: {}", current_labels().max_substitutions, n)
    }

    /// The folders in `path`.
    fn dirs(path: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(path)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_dir())
            .collect()
    }

    /// Two substitutions in the first set: the players 2 and 3 out for the
    /// two on the bench.
    fn two_substitutions(base: &Path) {
        let team = &dirs(base)[0];
        let content = std::fs::read_to_string(team.join("team.json")).unwrap();
        let team_json: serde_json::Value = serde_json::from_str(&content).unwrap();
        let mut players = team_json["players"].as_array().unwrap().clone();
        players.sort_by_key(|p| p["number"].as_u64());
        let id = |i: usize| players[i]["id"].as_str().unwrap().to_string();
        let rows: String = [(1, 7), (2, 8)]
            .iter()
            .map(|(out, r#in)| {
                format!(
                    "2025-09-01T18:00:00.000000Z,R,{},,{},\n",
                    id(*out),
                    id(*r#in)
                )
            })
            .collect();
        std::fs::write(dirs(team)[0].join("set_1.csv"), rows).unwrap();
    }

    /// Opens the edit screen on the only match.
    async fn edit_match(ui: &mut Ui) {
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Char('e')])
            .await;
        assert!(
            ui.screen().contains(current_labels().edit_match),
            "{}",
            ui.screen()
        );
    }

    /// From the opponent, moves to the substitutions and types `n`.
    async fn set_substitutions(ui: &mut Ui, n: &str) {
        ui.type_keys(&[KeyCode::Tab, KeyCode::Tab, KeyCode::Tab])
            .await;
        ui.type_keys(&[KeyCode::Backspace, KeyCode::Backspace])
            .await;
        ui.type_keys(&chars(n)).await;
    }

    #[tokio::test]
    async fn edit_starts_with_the_match_info() {
        let mut ui = Ui::start_with_sets(TEAM).await;
        edit_match(&mut ui).await;
        let screen = ui.screen();
        assert!(screen.contains("Cortona Volley"), "{screen}");
        assert!(screen.contains("2025-09-01"), "{screen}");
        assert!(screen.contains("[X]"), "{screen}");
        assert!(screen.contains(&substitutions_field("8")), "{screen}");
    }

    #[tokio::test]
    async fn edit_saves_the_match_info_and_keeps_the_sets() {
        let mut ui = Ui::start_with_files(TEAM, two_substitutions).await;
        let events = ui.saved_first_set_events();
        edit_match(&mut ui).await;
        ui.type_keys(&vec![KeyCode::Backspace; "Cortona Volley".len()])
            .await;
        ui.type_keys(&chars("Arezzo")).await;
        ui.press(KeyCode::Tab).await;
        ui.type_keys(&[KeyCode::Backspace; 8]).await;
        ui.type_keys(&chars("20250915")).await;
        ui.type_keys(&[KeyCode::Tab, KeyCode::Char(' ')]).await;
        ui.type_keys(&[KeyCode::Tab, KeyCode::Backspace, KeyCode::Char('6')])
            .await;
        ui.press(KeyCode::Enter).await;
        let saved = ui.saved_matches();
        assert_eq!(saved.len(), 1, "{}", ui.screen());
        assert_eq!(saved[0]["opponent"], "Arezzo");
        assert_eq!(saved[0]["date"], "2025-09-15T00:00:00Z");
        assert_eq!(saved[0]["home"], false);
        assert_eq!(saved[0]["max_substitutions"], 6);
        assert_eq!(ui.saved_first_set_events(), events);
        // back on the list, the match readable with the new info
        let screen = ui.screen();
        assert!(!screen.contains(current_labels().edit_match), "{screen}");
        assert_eq!(ui.rows_containing(&["Arezzo"]).len(), 1, "{screen}");
        assert!(!screen.contains(current_labels().unreadable), "{screen}");
    }

    #[tokio::test]
    async fn edit_keeps_the_substitutions_already_made() {
        let mut ui = Ui::start_with_files(TEAM, two_substitutions).await;
        edit_match(&mut ui).await;
        set_substitutions(&mut ui, "1").await;
        ui.press(KeyCode::Enter).await;
        // the banner cuts the message
        let error: String = current_labels()
            .max_substitutions_below_existing_substitutions
            .chars()
            .take(40)
            .collect();
        assert!(ui.screen().contains(&error), "{}", ui.screen());
        assert_eq!(ui.saved_matches()[0]["max_substitutions"], 8);
        // as many as the ones made
        ui.press(KeyCode::Esc).await;
        ui.type_keys(&[KeyCode::Backspace, KeyCode::Char('2')])
            .await;
        ui.press(KeyCode::Enter).await;
        assert_eq!(
            ui.saved_matches()[0]["max_substitutions"],
            2,
            "{}",
            ui.screen()
        );
    }

    #[tokio::test]
    async fn edit_can_be_left_without_saving() {
        let mut ui = Ui::start_with_sets(TEAM).await;
        let before = ui.saved_matches();
        edit_match(&mut ui).await;
        ui.type_keys(&chars("xyz")).await;
        ui.press(KeyCode::Esc).await;
        assert!(
            !ui.screen().contains(current_labels().edit_match),
            "{}",
            ui.screen()
        );
        assert_eq!(ui.saved_matches(), before);
    }
}
