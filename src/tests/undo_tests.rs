//! The undo while scouting: shift-u (not u, too easy to press by mistake)
//! steps back one choice at a time, from the evaluation to the player to
//! the event, and from the event choice it removes the last event saved.

#[cfg(test)]
mod tests {
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::{KeyCode, KeyModifiers};
    use scout4all_core::localization::current_labels;

    // the lineup: the first six on court, Gina the libero, Hana on the
    // bench. We serve.
    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &[
            "Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina", "Hana",
        ],
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

    fn saved_count(ui: &Ui) -> usize {
        ui.saved_first_set_events().len()
    }

    /// The event of the last event saved.
    fn last_saved_event(ui: &Ui) -> Option<String> {
        ui.saved_first_set_events()
            .last()
            .map(|e| e.split(',').nth(1).unwrap_or_default().to_string())
    }

    fn choosing_the_event(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().choose_the_event)
    }

    fn choosing_the_player(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().player_selection)
    }

    fn choosing_the_evaluation(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().choose_the_evaluation)
    }

    fn choosing_the_replacement(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().select_replacement)
    }

    fn score(ui: &mut Ui) -> String {
        ui.lines()
            .iter()
            .find(|l| l.contains("Volley Club") && l.contains('–'))
            .map(|l| {
                l.split_whitespace()
                    .skip_while(|w| !w.chars().all(|c| c.is_ascii_digit()))
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_else(|| panic!("no score: {}", ui.screen()))
    }

    /// u alone does nothing: it's no undo anymore.
    #[tokio::test]
    async fn u_alone_does_not_undo() {
        let mut ui = scouting().await;
        keys(&mut ui, "s+").await;
        assert_eq!(saved_count(&ui), 1);
        keys(&mut ui, "u").await;
        assert_eq!(saved_count(&ui), 1);
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        // nor on the evaluation, the player or the replacement
        keys(&mut ui, "du").await;
        assert!(choosing_the_player(&mut ui), "{}", ui.screen());
        // the error on the player choice goes away with the first key
        keys(&mut ui, "11u").await;
        assert!(choosing_the_evaluation(&mut ui), "{}", ui.screen());
        assert_eq!(saved_count(&ui), 1);
    }

    /// Shift-u, as the terminals report it: an uppercase U, with or without
    /// shift (with caps lock).
    #[tokio::test]
    async fn shift_u_undoes() {
        for modifiers in [KeyModifiers::SHIFT, KeyModifiers::NONE] {
            let mut ui = scouting().await;
            keys(&mut ui, "s+").await;
            ui.press_with(KeyCode::Char('U'), modifiers).await;
            assert_eq!(saved_count(&ui), 0, "{modifiers:?}");
            assert!(choosing_the_evaluation(&mut ui), "{}", ui.screen());
        }
    }

    #[tokio::test]
    async fn footer_shows_the_undo_only_with_something_to_undo() {
        let mut ui = scouting().await;
        let undo = format!("⇧-u = {}", current_labels().undo);
        assert!(!ui.footer().contains(&undo), "{}", ui.footer());
        keys(&mut ui, "s").await;
        assert!(ui.footer().contains(&undo), "{}", ui.footer());
    }

    /// At the start of the set there's nothing to undo: nothing happens.
    #[tokio::test]
    async fn nothing_to_undo() {
        let mut ui = scouting().await;
        let before = ui.screen();
        keys(&mut ui, "U").await;
        assert_eq!(ui.screen(), before);
        assert_eq!(saved_count(&ui), 0);
        // the scouting goes on
        keys(&mut ui, "s#").await;
        assert_eq!(saved_count(&ui), 1);
        assert_eq!(score(&mut ui), "1 – 0");
    }

    /// Each undo steps back one choice: from the evaluation to the player,
    /// from the player to the event. Nothing saved is touched.
    #[tokio::test]
    async fn undo_steps_back_the_choices() {
        let mut ui = scouting().await;
        keys(&mut ui, "s+d").await;
        keys(&mut ui, "3").await;
        assert!(choosing_the_evaluation(&mut ui), "{}", ui.screen());
        keys(&mut ui, "U").await;
        assert!(choosing_the_player(&mut ui), "{}", ui.screen());
        keys(&mut ui, "U").await;
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        assert_eq!(saved_count(&ui), 1);
        // then the dig chosen again
        keys(&mut ui, "d3+").await;
        assert_eq!(saved_count(&ui), 2);
        assert_eq!(last_saved_event(&ui).as_deref(), Some("D"));
    }

    /// The undo of a point takes it back, and the serve with it.
    #[tokio::test]
    async fn undo_of_a_point() {
        let mut ui = scouting().await;
        keys(&mut ui, "s+E").await;
        assert_eq!(score(&mut ui), "1 – 0");
        assert_eq!(last_saved_event(&ui).as_deref(), Some("OE"));
        // an event without player nor evaluation: back to the events
        keys(&mut ui, "U").await;
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        assert_eq!(score(&mut ui), "0 – 0");
        assert_eq!(saved_count(&ui), 1);
        // then their point instead
        keys(&mut ui, "S").await;
        assert_eq!(score(&mut ui), "0 – 1");
        assert_eq!(last_saved_event(&ui).as_deref(), Some("OS"));
    }

    /// The undo of a substitution goes back to the replacement of the same
    /// player, then to the player, then to the events.
    #[tokio::test]
    async fn undo_of_a_substitution() {
        let mut ui = scouting().await;
        keys(&mut ui, "r2").await;
        assert!(choosing_the_replacement(&mut ui), "{}", ui.screen());
        keys(&mut ui, "1").await;
        assert_eq!(last_saved_event(&ui).as_deref(), Some("R"));
        keys(&mut ui, "U").await;
        assert_eq!(saved_count(&ui), 0);
        assert!(choosing_the_replacement(&mut ui), "{}", ui.screen());
        assert!(ui.screen().contains("Hana"), "{}", ui.screen());
        keys(&mut ui, "U").await;
        assert!(choosing_the_player(&mut ui), "{}", ui.screen());
        keys(&mut ui, "U").await;
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        // the substitution again
        keys(&mut ui, "r21").await;
        assert_eq!(saved_count(&ui), 1);
        assert_eq!(last_saved_event(&ui).as_deref(), Some("R"));
    }

    /// More undos in a row remove more events, back to the start.
    #[tokio::test]
    async fn undo_back_to_the_start() {
        let mut ui = scouting().await;
        keys(&mut ui, "s+d3+a2#").await;
        assert_eq!(score(&mut ui), "1 – 0");
        assert_eq!(saved_count(&ui), 3);
        // each saved event: removed, then its evaluation and player undone
        keys(&mut ui, "UUU").await;
        assert_eq!(saved_count(&ui), 2);
        assert_eq!(score(&mut ui), "0 – 0");
        keys(&mut ui, "UUU").await;
        assert_eq!(saved_count(&ui), 1);
        // the serve: its player is the server, no player choice
        keys(&mut ui, "UU").await;
        assert_eq!(saved_count(&ui), 0);
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        keys(&mut ui, "U").await;
        assert_eq!(saved_count(&ui), 0);
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
    }
}
