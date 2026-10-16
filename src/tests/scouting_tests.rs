//! Scouting keys as users press them. Up to v0.1.1 "b" was both "block" and
//! "keybinding settings": scouting a block opened the settings instead. Now
//! events and evaluations are customizable, and the settings, undo and back
//! keys are fixed and can't be assigned to them.

#[cfg(test)]
mod tests {
    use crate::{
        localization::current_labels,
        shapes::enums::{EvalEnum, EventTypeEnum, WithDesc},
        tests::ui::{TeamFixture, Ui},
    };
    use crokey::crossterm::event::{KeyCode, KeyModifiers};

    pub const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &["Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina"],
        opponents: &["Cortona Volley"],
    }];

    /// Opens the scouting screen on the set in progress.
    async fn scouting() -> Ui {
        let mut ui = Ui::start_with_sets(TEAM).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Enter])
            .await;
        assert!(
            ui.screen().contains(current_labels().choose_the_event),
            "not scouting: {}",
            ui.screen()
        );
        ui
    }

    /// From the scouting, adds `key` to the shortcut at `index` of the
    /// shortcuts list (events first, then evaluations) and goes back.
    async fn bind_from_scouting(ui: &mut Ui, index: usize, key: KeyCode) {
        ui.press(KeyCode::F(2)).await;
        for _ in 0..index {
            ui.press(KeyCode::Down).await;
        }
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('n'), key, KeyCode::Enter])
            .await;
        // shortcut keys, settings, scouting
        ui.type_keys(&[KeyCode::Esc, KeyCode::Esc]).await;
        assert!(
            ui.screen().contains(current_labels().choose_the_event),
            "not back to scouting: {}",
            ui.screen()
        );
    }

    fn eval_index(eval: EvalEnum) -> usize {
        EventTypeEnum::ALL.len() + EvalEnum::ALL.iter().position(|e| *e == eval).unwrap()
    }

    fn is_choosing_the_evaluation(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().choose_the_evaluation)
    }

    #[tokio::test]
    async fn block_after_positive_serve() {
        let mut ui = scouting().await;
        ui.type_keys(&[KeyCode::Char('s'), KeyCode::Char('+'), KeyCode::Char('b')])
            .await;
        let screen = ui.screen();
        assert!(
            screen.contains(current_labels().player_selection),
            "block did not ask for the player: {screen}"
        );
        assert!(screen.contains(current_labels().block), "{screen}");
    }

    #[tokio::test]
    async fn footer_advertises_fixed_keys() {
        let mut ui = scouting().await;
        ui.type_keys(&[KeyCode::Char('s'), KeyCode::Char('+')])
            .await;
        let footer = ui.footer();
        let labels = current_labels();
        assert!(
            footer.contains(&format!("F2 = {}", labels.settings)),
            "{footer}"
        );
        assert!(footer.contains(&format!("u = {}", labels.undo)), "{footer}");
        assert!(
            footer.contains(&format!("Esc = {}", labels.back)),
            "{footer}"
        );
        assert!(!footer.contains("b = "), "{footer}");
    }

    #[tokio::test]
    async fn settings_open_on_the_shortcuts() {
        let mut ui = scouting().await;
        ui.press(KeyCode::F(2)).await;
        let selected = ui.selected().unwrap_or_default();
        assert!(
            selected.contains(&EventTypeEnum::S.with_desc().1),
            "{selected}: {}",
            ui.screen()
        );
        ui.press(KeyCode::Esc).await;
        assert!(ui.screen().contains(current_labels().choose_the_event));
    }

    #[tokio::test]
    async fn rebound_event_key_while_scouting() {
        let mut ui = scouting().await;
        bind_from_scouting(&mut ui, 0, KeyCode::Char('x')).await;
        ui.press(KeyCode::Char('x')).await;
        assert!(is_choosing_the_evaluation(&mut ui), "{}", ui.screen());
        // the previous key still works too
        ui.type_keys(&[KeyCode::Char('u'), KeyCode::Char('s')])
            .await;
        assert!(is_choosing_the_evaluation(&mut ui), "{}", ui.screen());
    }

    #[tokio::test]
    async fn rebound_eval_key_while_scouting() {
        let mut ui = scouting().await;
        bind_from_scouting(&mut ui, eval_index(EvalEnum::Positive), KeyCode::Char('w')).await;
        ui.press(KeyCode::Char('s')).await;
        assert!(
            ui.screen().contains("+, w => "),
            "evaluation keys not listed: {}",
            ui.screen()
        );
        ui.press(KeyCode::Char('w')).await;
        assert!(!is_choosing_the_evaluation(&mut ui), "{}", ui.screen());
        assert!(ui.screen().contains(current_labels().choose_the_event));
    }

    /// The same key can be an event and an evaluation: they're never
    /// expected at the same time.
    #[tokio::test]
    async fn event_and_evaluation_sharing_a_key() {
        let mut ui = scouting().await;
        bind_from_scouting(&mut ui, eval_index(EvalEnum::Positive), KeyCode::Char('d')).await;
        // positive serve, then a dig
        ui.type_keys(&[KeyCode::Char('s'), KeyCode::Char('d')])
            .await;
        assert!(ui.screen().contains(current_labels().choose_the_event));
        ui.press(KeyCode::Char('d')).await;
        assert!(
            ui.screen().contains(current_labels().player_selection),
            "{}",
            ui.screen()
        );
    }

    /// Back from the settings, the events becoming available later in the
    /// rally still answer to their keys.
    #[tokio::test]
    async fn events_after_visiting_the_settings() {
        let mut ui = scouting().await;
        // analytics toggled twice: the settings are saved, then read back
        ui.type_keys(&[
            KeyCode::F(2),
            KeyCode::BackTab,
            KeyCode::Down,
            KeyCode::Char(' '),
            KeyCode::Char(' '),
            KeyCode::Esc,
        ])
        .await;
        assert!(ui.saved_settings().await.is_some());
        // a positive serve, then a dig
        ui.type_keys(&[KeyCode::Char('s'), KeyCode::Char('+'), KeyCode::Char('d')])
            .await;
        let screen = ui.screen();
        assert!(
            screen.contains(current_labels().player_selection),
            "{screen}"
        );
        assert!(screen.contains(current_labels().defense), "{screen}");
    }

    #[tokio::test]
    async fn evaluation_with_shift() {
        let mut ui = scouting().await;
        ui.press(KeyCode::Char('s')).await;
        ui.press_with(KeyCode::Char('+'), KeyModifiers::SHIFT).await;
        assert!(!is_choosing_the_evaluation(&mut ui), "{}", ui.screen());
    }

    #[tokio::test]
    async fn evaluation_not_available_for_the_event() {
        let mut ui = scouting().await;
        // "!" only evaluates passes and digs
        ui.type_keys(&[KeyCode::Char('s'), KeyCode::Char('!')])
            .await;
        assert!(is_choosing_the_evaluation(&mut ui), "{}", ui.screen());
    }
}
