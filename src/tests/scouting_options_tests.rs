//! The options told while choosing the event: the inverted P1, kept until
//! changed, shown by the rotation and saved with every event.

#[cfg(test)]
mod tests {
    use crate::localization::current_labels;
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;

    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &["Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina"],
        opponents: &["Cortona Volley"],
    }];

    async fn scouting() -> Ui {
        let mut ui = Ui::start_with_sets(TEAM).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Enter])
            .await;
        assert!(
            ui.screen().contains(current_labels().choose_the_event),
            "{}",
            ui.screen()
        );
        ui
    }

    async fn keys(ui: &mut Ui, keys: &str) {
        for c in keys.chars() {
            ui.press(KeyCode::Char(c)).await;
        }
    }

    /// The flags column of the saved events.
    fn flags(ui: &Ui) -> Vec<String> {
        ui.saved_first_set_events()
            .iter()
            .map(|e| e.rsplit(',').next().unwrap_or_default().to_string())
            .collect()
    }

    fn inverted_row(ui: &mut Ui) -> Option<String> {
        let name = current_labels().inverted_p1;
        ui.panel(current_labels().choose_the_event)
            .and_then(|rows| rows.into_iter().find(|r| r.contains(name)))
    }

    /// "i" inverts the P1 and back: the row of the option and the rotation
    /// tell it, and the events saved meanwhile say so.
    #[tokio::test]
    async fn i_inverts_the_p1_until_pressed_again() {
        let mut ui = scouting().await;
        let short = current_labels().inverted_p1_short;
        assert!(inverted_row(&mut ui).is_some_and(|r| r.starts_with("[ ]")));
        assert!(!ui.screen().contains(short), "{}", ui.screen());
        ui.press(KeyCode::Char('i')).await;
        assert!(
            inverted_row(&mut ui).is_some_and(|r| r.starts_with("[x]")),
            "{}",
            ui.screen()
        );
        assert!(
            ui.screen().contains(&format!("S1 · {short}")),
            "{}",
            ui.screen()
        );
        // our serve error, then their serve error
        keys(&mut ui, "s=E").await;
        // nothing else was chosen: still choosing the event
        ui.press(KeyCode::Char('i')).await;
        keys(&mut ui, "s=").await;
        assert_eq!(flags(&ui), ["1", "1", ""]);
        assert!(!ui.screen().contains(short), "{}", ui.screen());
    }

    /// Opened again, the set goes on with the P1 as its last event says.
    #[tokio::test]
    async fn the_inverted_p1_is_kept_when_the_set_is_opened_again() {
        let mut ui = scouting().await;
        keys(&mut ui, "is=").await;
        ui.press(KeyCode::Esc).await;
        ui.press(KeyCode::Enter).await;
        assert!(
            inverted_row(&mut ui).is_some_and(|r| r.starts_with("[x]")),
            "{}",
            ui.screen()
        );
    }
}
