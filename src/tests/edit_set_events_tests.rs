//! Editing the events of a set from the match list: the set is chosen
//! first, then an event's type, evaluation and technique change, and the
//! edit is saved only if the events after it still fit.

#[cfg(test)]
mod tests {
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;
    use scout4all_core::localization::current_labels;

    // Anna serves (1 in the player choice), Bea (2) is in the front row,
    // Ilaria is on the bench.
    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &[
            "Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina", "Ilaria",
        ],
        opponents: &["Cortona Volley"],
    }];

    async fn keys(ui: &mut Ui, keys: &str) {
        for c in keys.chars() {
            ui.press(KeyCode::Char(c)).await;
        }
    }

    /// Scouts the first set: our serve kept by them (`S-`), Bea's dig
    /// (`D+`) and winning attack (`A#`), then our serve again and their
    /// error (`OE`): 1-0, then 2-0.
    async fn scouted() -> Ui {
        let mut ui = Ui::start_with_sets(TEAM).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Enter])
            .await;
        keys(&mut ui, "s-d2+a2#s-E").await;
        assert_eq!(ui.saved_first_set_events().len(), 5, "{}", ui.screen());
        ui.press(KeyCode::Esc).await;
        ui
    }

    /// From the match list, opens the events of the first set.
    async fn edit_events(ui: &mut Ui) {
        ui.press(KeyCode::Char('v')).await;
        assert!(
            ui.screen().contains(current_labels().choose_the_set),
            "{}",
            ui.screen()
        );
        assert_eq!(ui.rows_containing(&["set 1"]).len(), 1, "{}", ui.screen());
        ui.press(KeyCode::Enter).await;
        assert!(
            ui.screen().contains(current_labels().events),
            "{}",
            ui.screen()
        );
    }

    /// The columns of a saved event: event, player, eval.
    fn saved(ui: &Ui, i: usize) -> (String, String) {
        let events = ui.saved_first_set_events();
        let columns: Vec<_> = events[i].split(',').map(str::to_string).collect();
        (columns[1].clone(), columns[3].clone())
    }

    /// Whether the form is open: its values are between arrows.
    fn editing(ui: &mut Ui) -> bool {
        ui.screen().contains('◀')
    }

    #[tokio::test]
    async fn edit_events_lists_the_events_and_the_score_before_them() {
        let mut ui = scouted().await;
        edit_events(&mut ui).await;
        assert!(ui.screen().contains("0 – 0"), "{}", ui.screen());
        ui.type_keys(&[KeyCode::Down, KeyCode::Down, KeyCode::Down])
            .await;
        // before our second serve
        assert!(ui.screen().contains("1 – 0"), "{}", ui.screen());
        ui.press(KeyCode::Down).await;
        assert!(ui.screen().contains("1 – 0"), "{}", ui.screen());
        // the last event stays selected
        ui.press(KeyCode::Down).await;
        assert!(ui.screen().contains("1 – 0"), "{}", ui.screen());
    }

    #[tokio::test]
    async fn edit_is_saved_when_the_events_after_it_fit() {
        let mut ui = scouted().await;
        let before = ui.saved_first_set_events();
        edit_events(&mut ui).await;
        // the serve kept by them becomes positive: still kept
        ui.type_keys(&[KeyCode::Enter, KeyCode::Down, KeyCode::Left, KeyCode::Left])
            .await;
        assert!(editing(&mut ui), "{}", ui.screen());
        ui.press(KeyCode::Enter).await;
        assert!(!editing(&mut ui), "{}", ui.screen());
        assert_eq!(saved(&ui, 0), ("S".into(), "+".into()));
        // the others untouched, their timestamps included
        assert_eq!(ui.saved_first_set_events()[1..], before[1..]);
    }

    #[tokio::test]
    async fn edit_is_rejected_when_the_events_after_it_do_not_fit() {
        let mut ui = scouted().await;
        let before = ui.saved_first_set_events();
        edit_events(&mut ui).await;
        // a serve error: their serve next, not Bea's dig
        ui.type_keys(&[KeyCode::Enter, KeyCode::Down, KeyCode::Right])
            .await;
        ui.press(KeyCode::Enter).await;
        let error: String = current_labels()
            .edit_does_not_fit_the_set
            .chars()
            .take(30)
            .collect();
        assert!(ui.screen().contains(&error), "{}", ui.screen());
        assert_eq!(ui.saved_first_set_events(), before);
        // a key clears the message: the edit is still there, to be changed
        ui.press(KeyCode::Left).await;
        assert!(!ui.screen().contains(&error), "{}", ui.screen());
        assert_eq!(
            ui.rows_containing(&["◀ error ▶"]).len(),
            1,
            "{}",
            ui.screen()
        );
        ui.type_keys(&[KeyCode::Left, KeyCode::Left, KeyCode::Left])
            .await;
        ui.press(KeyCode::Enter).await;
        assert_eq!(saved(&ui, 0), ("S".into(), "+".into()), "{}", ui.screen());
    }

    #[tokio::test]
    async fn event_type_changes_among_the_ones_the_player_can_play() {
        let mut ui = scouted().await;
        edit_events(&mut ui).await;
        // Bea's dig becomes a block, keeping its evaluation
        ui.type_keys(&[KeyCode::Down, KeyCode::Enter, KeyCode::Right])
            .await;
        ui.press(KeyCode::Enter).await;
        assert_eq!(saved(&ui, 1), ("B".into(), "+".into()), "{}", ui.screen());
    }

    #[tokio::test]
    async fn edit_can_be_cancelled() {
        let mut ui = scouted().await;
        let before = ui.saved_first_set_events();
        edit_events(&mut ui).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Down, KeyCode::Left, KeyCode::Left])
            .await;
        ui.press(KeyCode::Esc).await;
        assert!(!editing(&mut ui), "{}", ui.screen());
        assert_eq!(ui.saved_first_set_events(), before);
        // back to the sets, then to the matches
        ui.press(KeyCode::Esc).await;
        assert!(
            ui.screen().contains(current_labels().choose_the_set),
            "{}",
            ui.screen()
        );
        ui.press(KeyCode::Esc).await;
        assert!(
            ui.screen().contains(current_labels().match_list),
            "{}",
            ui.screen()
        );
    }

    /// A set saved without the lineup rules (up to v0.3.0) gets the ones it
    /// replays with, and the match stays readable.
    #[tokio::test]
    async fn edit_of_a_legacy_set_saves_its_lineup_rules() {
        let mut ui = scouted().await;
        ui.edit_saved_matches(|m| {
            m.as_object_mut().unwrap().remove("max_substitutions");
        });
        ui.edit_saved_first_sets(|s| {
            s.as_object_mut().unwrap().remove("lineup_rules");
        });
        // the matches read again
        ui.type_keys(&[KeyCode::Esc, KeyCode::Char('m')]).await;
        edit_events(&mut ui).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Down, KeyCode::Left, KeyCode::Left])
            .await;
        ui.press(KeyCode::Enter).await;
        assert_eq!(saved(&ui, 0), ("S".into(), "+".into()), "{}", ui.screen());
        assert!(
            ui.saved_first_sets()[0].get("lineup_rules").is_some(),
            "{:?}",
            ui.saved_first_sets()
        );
        ui.type_keys(&[KeyCode::Esc, KeyCode::Esc, KeyCode::Esc, KeyCode::Char('m')])
            .await;
        let screen = ui.screen();
        assert!(!screen.contains(current_labels().unreadable), "{screen}");
        assert_eq!(ui.rows_containing(&["Cortona Volley"]).len(), 1, "{screen}");
    }

    #[tokio::test]
    async fn substitution_cannot_be_edited() {
        let mut ui = Ui::start_with_sets(TEAM).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Enter])
            .await;
        // Bea out for Ilaria
        keys(&mut ui, "r21").await;
        assert_eq!(ui.saved_first_set_events().len(), 1, "{}", ui.screen());
        ui.press(KeyCode::Esc).await;
        edit_events(&mut ui).await;
        ui.press(KeyCode::Enter).await;
        let error: String = current_labels()
            .event_cannot_be_edited
            .chars()
            .take(30)
            .collect();
        assert!(ui.screen().contains(&error), "{}", ui.screen());
        assert!(!editing(&mut ui), "{}", ui.screen());
    }

    #[tokio::test]
    async fn edit_events_is_offered_only_for_matches_with_sets() {
        let mut ui = Ui::start(TEAM, &[]).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m')]).await;
        assert!(
            !ui.footer().contains(current_labels().edit_events),
            "{}",
            ui.footer()
        );
        let mut ui = scouted().await;
        assert!(
            ui.footer().contains(current_labels().edit_events),
            "{}",
            ui.footer()
        );
    }
}
