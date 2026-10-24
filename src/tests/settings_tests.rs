//! The settings screen: general settings on a panel, the scouting keyboard
//! shortcuts on the other one, every change saved right away. The sets and
//! the serves are chosen along with the evaluation: their keys can't be the
//! ones of the evaluations.

#[cfg(test)]
mod tests {
    use crate::localization::{Describe, current_labels};
    use crate::{
        providers::{
            settings_reader::SettingsReader,
            store::{CONFIG, settings_reader::StoreSettingsReader},
        },
        shapes::{
            actions::ScreenActionEnum,
            keybinding::{KeyBindings, SETTINGS_KEY, is_reserved_key, navigation_keybindings},
            settings::Settings,
        },
        tests::fs_store,
        tests::ui::{TeamFixture, Ui},
    };
    use crokey::{KeyCombination, crossterm::event::KeyCode, key};
    use scout4all_core::shapes::{
        enums::{EvalEnum, EventTypeEnum, LanguageEnum},
        technique::{ServeTypeEnum, SetTypeEnum},
    };
    use std::collections::HashSet;
    use tempfile::TempDir;

    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &[],
        opponents: &[],
    }];

    async fn settings() -> Ui {
        let mut ui = Ui::start(TEAM, &[]).await;
        ui.press(KeyCode::F(2)).await;
        ui
    }

    /// The settings on the shortcuts panel, on the shortcut at `index`.
    async fn shortcut(index: usize) -> Ui {
        let mut ui = settings().await;
        ui.press(KeyCode::Tab).await;
        for _ in 0..index {
            ui.press(KeyCode::Down).await;
        }
        ui
    }

    /// The key screen of the shortcut at `index`, ready for its new key.
    async fn change_key_for(index: usize) -> Ui {
        let mut ui = shortcut(index).await;
        ui.press(KeyCode::Enter).await;
        ui
    }

    fn eval_index(eval: EvalEnum) -> usize {
        EventTypeEnum::ALL.len() + EvalEnum::ALL.iter().position(|e| *e == eval).unwrap()
    }

    fn set_index(set: SetTypeEnum) -> usize {
        EventTypeEnum::ALL.len()
            + EvalEnum::ALL.len()
            + SetTypeEnum::ALL.iter().position(|s| *s == set).unwrap()
    }

    fn serve_index(serve: ServeTypeEnum) -> usize {
        EventTypeEnum::ALL.len()
            + EvalEnum::ALL.len()
            + SetTypeEnum::ALL.len()
            + ServeTypeEnum::ALL.iter().position(|s| *s == serve).unwrap()
    }

    fn assigned_message(key: &str, other: &str) -> String {
        current_labels()
            .shortcut_already_assigned
            .replacen("{}", key, 1)
            .replacen("{}", other, 1)
    }

    async fn saved(ui: &Ui) -> Settings {
        ui.saved_settings().await.expect("settings not saved")
    }

    fn keys(keys: &[KeyCombination]) -> HashSet<KeyCombination> {
        keys.iter().cloned().collect()
    }

    #[tokio::test]
    async fn home_opens_the_settings_on_f2_only() {
        let mut ui = Ui::start(TEAM, &[]).await;
        let footer = ui.footer();
        assert!(
            footer.contains(&format!("F2 = {}", current_labels().settings)),
            "{footer}"
        );
        assert!(!footer.contains("s = "), "{footer}");
        assert!(!footer.contains("b = "), "{footer}");
        let home = ui.screen();
        ui.press(KeyCode::Char('s')).await;
        ui.press(KeyCode::Char('b')).await;
        assert_eq!(ui.screen(), home);
        ui.press(KeyCode::F(2)).await;
        let selected = ui.selected().unwrap_or_default();
        assert!(
            selected.contains(current_labels().language),
            "{selected}: {}",
            ui.screen()
        );
    }

    #[tokio::test]
    async fn both_panels_are_shown() {
        let mut ui = settings().await;
        let screen = ui.screen();
        let labels = current_labels();
        for text in [
            labels.general_settings,
            labels.key_bindings,
            labels.language,
            labels.enable_send_analytics,
            labels.events,
            labels.evaluations,
            labels.fixed_shortcuts,
        ] {
            assert!(screen.contains(text), "{text}: {screen}");
        }
        for event in EventTypeEnum::ALL {
            assert!(screen.contains(&event.describe().1), "{event}: {screen}");
        }
        for eval in EvalEnum::ALL {
            assert!(screen.contains(&eval.describe().1), "{eval}: {screen}");
        }
        assert!(!screen.contains("Hyphen"), "{screen}");
    }

    #[tokio::test]
    async fn fixed_keys_are_listed_but_not_selectable() {
        let mut ui = settings().await;
        let screen = ui.screen();
        let fixed = current_labels().fixed;
        for key in ["F2", "⇧-u", "Esc", "F3", "0-9"] {
            assert!(
                screen.contains(&format!("{key} ({fixed})")),
                "{key}: {screen}"
            );
        }
        ui.press(KeyCode::Tab).await;
        for _ in 0..50 {
            ui.press(KeyCode::Down).await;
        }
        let selected = ui.selected().unwrap_or_default();
        assert!(
            selected.contains(&ServeTypeEnum::ALL.last().unwrap().describe().1),
            "{selected}"
        );
        // still shown at the end of the list
        let screen = ui.screen();
        assert!(screen.contains(&format!("u ({fixed})")), "{screen}");
    }

    /// The sets and the serves follow the evaluations, each with its key:
    /// the list scrolls to them.
    #[tokio::test]
    async fn set_and_serve_types_are_listed() {
        let mut ui = shortcut(0).await;
        let mut seen = String::new();
        for _ in 0..serve_index(ServeTypeEnum::Spin) {
            seen.push_str(&ui.screen());
            ui.press(KeyCode::Down).await;
        }
        seen.push_str(&ui.screen());
        let labels = current_labels();
        for title in [labels.set_types, labels.serve_types] {
            assert!(seen.contains(title), "{title}: {seen}");
        }
        let rows: Vec<String> = seen
            .lines()
            .flat_map(|l| {
                l.split('│')
                    .map(|c| c.split_whitespace().collect::<Vec<_>>().join(" "))
            })
            .collect();
        for set in SetTypeEnum::ALL {
            let expected = format!("{} {}", set.describe().1, set.to_string().to_lowercase());
            assert!(rows.contains(&expected), "{expected}: {seen}");
        }
        for serve in ServeTypeEnum::ALL {
            let expected = format!(
                "{} {}",
                serve.describe().1,
                serve.to_string().to_lowercase()
            );
            assert!(rows.contains(&expected), "{expected}: {seen}");
        }
    }

    #[tokio::test]
    async fn tab_switches_panel() {
        let mut ui = settings().await;
        ui.press(KeyCode::Tab).await;
        let selected = ui.selected().unwrap_or_default();
        assert!(
            selected.contains(&EventTypeEnum::S.describe().1),
            "{selected}"
        );
        let footer = ui.footer();
        assert!(
            footer.contains(&format!("e = {}", current_labels().change)),
            "{footer}"
        );
        assert!(footer.contains("r = "), "{footer}");
        assert!(
            footer.contains(&format!("⇥ = {}", current_labels().switch_panel)),
            "{footer}"
        );
        ui.press(KeyCode::BackTab).await;
        let selected = ui.selected().unwrap_or_default();
        assert!(selected.contains(current_labels().language), "{selected}");
        let footer = ui.footer();
        assert!(!footer.contains("r = "), "{footer}");
        assert!(
            footer.contains(&format!("↵ = {}", current_labels().change)),
            "{footer}"
        );
    }

    #[tokio::test]
    async fn language_is_saved_right_away() {
        let mut ui = settings().await;
        ui.press(KeyCode::Right).await;
        assert_eq!(saved(&ui).await.language, LanguageEnum::It);
        ui.press(KeyCode::Left).await;
        assert_eq!(saved(&ui).await.language, LanguageEnum::En);
        ui.press(KeyCode::Enter).await;
        assert_eq!(saved(&ui).await.language, LanguageEnum::It);
    }

    #[tokio::test]
    async fn analytics_are_saved_right_away() {
        let mut ui = settings().await;
        ui.press(KeyCode::Down).await;
        ui.press(KeyCode::Char(' ')).await;
        assert!(!saved(&ui).await.consent.analytics_enabled);
        assert!(ui.selected().unwrap_or_default().contains("[ ]"));
        ui.press(KeyCode::Enter).await;
        assert!(saved(&ui).await.consent.analytics_enabled);
        assert!(ui.selected().unwrap_or_default().contains("[X]"));
    }

    #[tokio::test]
    async fn changing_a_setting_keeps_the_others() {
        let mut ui = change_key_for(0).await;
        ui.type_keys(&[KeyCode::Char('w'), KeyCode::Enter]).await;
        ui.press(KeyCode::BackTab).await;
        ui.press(KeyCode::Right).await;
        let settings = saved(&ui).await;
        assert_eq!(settings.language, LanguageEnum::It);
        assert_eq!(
            settings
                .scouting_keybindings
                .keybindings_for(&EventTypeEnum::S),
            keys(&[key!(w)])
        );
    }

    /// One key per shortcut, always: it can be changed, not added nor
    /// removed.
    #[tokio::test]
    async fn event_key_is_changed() {
        let mut ui = change_key_for(0).await;
        let screen = ui.screen();
        assert!(
            screen.contains(current_labels().press_the_shortcut),
            "{screen}"
        );
        let footer = ui.footer();
        for action in [ScreenActionEnum::New, ScreenActionEnum::Delete] {
            let (_, desc) = action.describe();
            assert!(!footer.contains(&desc), "{desc}: {footer}");
        }
        ui.type_keys(&[KeyCode::Char('w'), KeyCode::Enter]).await;
        assert_eq!(
            saved(&ui)
                .await
                .scouting_keybindings
                .keybindings_for(&EventTypeEnum::S),
            keys(&[key!(w)])
        );
        // straight back on the settings
        let selected = ui.selected().unwrap_or_default();
        assert!(
            selected.contains(&EventTypeEnum::S.describe().1),
            "{selected}"
        );
        assert!(selected.trim_end().ends_with('w'), "{selected}");
    }

    #[tokio::test]
    async fn current_key_is_shown() {
        let mut ui = change_key_for(eval_index(EvalEnum::Negative)).await;
        let lines = ui.lines();
        let prompt = lines
            .iter()
            .position(|l| l.contains(current_labels().press_the_shortcut))
            .unwrap();
        assert!(lines[prompt + 1].contains('-'), "{}", lines.join("\n"));
    }

    #[tokio::test]
    async fn eval_key_is_changed() {
        let mut ui = change_key_for(eval_index(EvalEnum::Negative)).await;
        ui.type_keys(&[KeyCode::Char('m'), KeyCode::Enter]).await;
        assert_eq!(
            saved(&ui)
                .await
                .eval_keybindings
                .keybindings_for(&EvalEnum::Negative),
            keys(&[key!(m)])
        );
    }

    #[tokio::test]
    async fn symbol_key_is_added() {
        // "-" and "+" couldn't be typed as shortcuts before
        let mut ui = change_key_for(0).await;
        ui.type_keys(&[KeyCode::Char('.'), KeyCode::Enter]).await;
        assert_eq!(
            saved(&ui)
                .await
                .scouting_keybindings
                .keybindings_for(&EventTypeEnum::S),
            keys(&[key!('.')])
        );
    }

    /// A shortcut can't be left without a key.
    #[tokio::test]
    async fn backspace_clears_the_key() {
        let mut ui = change_key_for(0).await;
        ui.type_keys(&[KeyCode::Char('x'), KeyCode::Backspace, KeyCode::Enter])
            .await;
        assert!(
            ui.screen().contains(current_labels().press_the_shortcut),
            "{}",
            ui.screen()
        );
        assert!(ui.saved_settings().await.is_none());
    }

    #[tokio::test]
    async fn fixed_keys_are_reserved() {
        for (code, shown) in [
            (KeyCode::Char('U'), "⇧-u"),
            (KeyCode::F(2), "F2"),
            (KeyCode::Char('0'), "0"),
            (KeyCode::Char('7'), "7"),
        ] {
            for index in [0, eval_index(EvalEnum::Perfect)] {
                let mut ui = change_key_for(index).await;
                ui.press(code).await;
                let message = current_labels().reserved_shortcut.replace("{}", shown);
                assert!(ui.screen().contains(&message), "{shown}: {}", ui.screen());
                ui.press(KeyCode::Enter).await;
                assert!(ui.saved_settings().await.is_none(), "{shown}");
            }
        }
    }

    #[tokio::test]
    async fn event_keys_do_not_conflict() {
        let mut ui = change_key_for(0).await;
        ui.press(KeyCode::Char('b')).await;
        let message = current_labels()
            .shortcut_already_assigned
            .replacen("{}", "b", 1)
            .replacen("{}", &EventTypeEnum::B.describe().1, 1);
        assert!(ui.screen().contains(&message), "{}", ui.screen());
        ui.press(KeyCode::Enter).await;
        assert!(ui.saved_settings().await.is_none());
    }

    #[tokio::test]
    async fn eval_keys_do_not_conflict() {
        let mut ui = change_key_for(eval_index(EvalEnum::Positive)).await;
        ui.press(KeyCode::Char('#')).await;
        let message = current_labels()
            .shortcut_already_assigned
            .replacen("{}", "#", 1)
            .replacen("{}", &EvalEnum::Perfect.describe().1, 1);
        assert!(ui.screen().contains(&message), "{}", ui.screen());
        ui.press(KeyCode::Enter).await;
        assert!(ui.saved_settings().await.is_none());
    }

    /// Keys the user assigned earlier count as much as the default ones.
    #[tokio::test]
    async fn custom_keys_do_not_conflict() {
        let attack = EventTypeEnum::ALL
            .iter()
            .position(|e| *e == EventTypeEnum::A)
            .unwrap();
        let negative = eval_index(EvalEnum::Negative);
        let mut ui = change_key_for(0).await;
        ui.type_keys(&[KeyCode::Char('w'), KeyCode::Enter]).await;
        for _ in 0..eval_index(EvalEnum::Perfect) {
            ui.press(KeyCode::Down).await;
        }
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('z'), KeyCode::Enter])
            .await;
        let before = saved(&ui).await;

        // "w" is now the serve: not for the attack
        for _ in attack..eval_index(EvalEnum::Perfect) {
            ui.press(KeyCode::Up).await;
        }
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('w')]).await;
        let message = current_labels()
            .shortcut_already_assigned
            .replacen("{}", "w", 1)
            .replacen("{}", &EventTypeEnum::S.describe().1, 1);
        assert!(ui.screen().contains(&message), "{}", ui.screen());
        ui.type_keys(&[KeyCode::Enter, KeyCode::Esc]).await;

        // "z" is now the perfect evaluation: not for the negative one
        for _ in attack..negative {
            ui.press(KeyCode::Down).await;
        }
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('z')]).await;
        let message = current_labels()
            .shortcut_already_assigned
            .replacen("{}", "z", 1)
            .replacen("{}", &EvalEnum::Perfect.describe().1, 1);
        assert!(ui.screen().contains(&message), "{}", ui.screen());
        ui.press(KeyCode::Enter).await;

        let after = saved(&ui).await;
        for event in EventTypeEnum::ALL {
            assert_eq!(
                after.scouting_keybindings.keybindings_for(&event),
                before.scouting_keybindings.keybindings_for(&event)
            );
        }
        for eval in EvalEnum::ALL {
            assert_eq!(
                after.eval_keybindings.keybindings_for(&eval),
                before.eval_keybindings.keybindings_for(&eval)
            );
        }
    }

    #[tokio::test]
    async fn own_key_again_is_not_a_conflict() {
        let mut ui = change_key_for(0).await;
        ui.type_keys(&[KeyCode::Char('s'), KeyCode::Enter]).await;
        assert!(ui.saved_settings().await.is_none());
        let selected = ui.selected().unwrap_or_default();
        assert!(
            selected.contains(&EventTypeEnum::S.describe().1),
            "{selected}"
        );
    }

    #[tokio::test]
    async fn reset_to_defaults_asks_for_confirmation() {
        let mut ui = change_key_for(0).await;
        ui.type_keys(&[KeyCode::Char('w'), KeyCode::Enter]).await;
        for _ in 0..eval_index(EvalEnum::Perfect) {
            ui.press(KeyCode::Down).await;
        }
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('y'), KeyCode::Enter])
            .await;
        let customized = saved(&ui).await;
        assert_eq!(
            customized
                .eval_keybindings
                .keybindings_for(&EvalEnum::Perfect),
            keys(&[key!(y)])
        );

        // the whole question fits a narrow terminal
        ui.resize(80, 40);
        ui.press(KeyCode::Char('r')).await;
        assert!(
            ui.screen()
                .contains(current_labels().reset_to_defaults_confirmation),
            "{}",
            ui.screen()
        );
        ui.press(KeyCode::Char('n')).await;
        let after_no = saved(&ui).await;
        assert_eq!(
            after_no
                .scouting_keybindings
                .keybindings_for(&EventTypeEnum::S),
            keys(&[key!(w)])
        );
        // the dialog is gone: "y" alone does nothing
        ui.press(KeyCode::Char('y')).await;
        assert!(
            saved(&ui)
                .await
                .scouting_keybindings
                .keybindings_for(&EventTypeEnum::S)
                .contains(&key!(w))
        );

        ui.type_keys(&[KeyCode::Char('r'), KeyCode::Char('y')])
            .await;
        let reset = saved(&ui).await;
        let defaults = Settings::default();
        for event in EventTypeEnum::ALL {
            assert_eq!(
                reset.scouting_keybindings.keybindings_for(&event),
                defaults.scouting_keybindings.keybindings_for(&event)
            );
        }
        for eval in EvalEnum::ALL {
            assert_eq!(
                reset.eval_keybindings.keybindings_for(&eval),
                defaults.eval_keybindings.keybindings_for(&eval)
            );
        }
    }

    #[tokio::test]
    async fn reset_to_defaults_restores_sets_and_serves() {
        let mut ui = change_key_for(set_index(SetTypeEnum::High)).await;
        ui.type_keys(&[KeyCode::Char('x'), KeyCode::Enter]).await;
        for _ in set_index(SetTypeEnum::High)..serve_index(ServeTypeEnum::Spin) {
            ui.press(KeyCode::Down).await;
        }
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('z'), KeyCode::Enter])
            .await;
        let customized = saved(&ui).await;
        assert_eq!(
            customized.set_type_keybindings.key_for(&SetTypeEnum::High),
            Some(key!(x))
        );
        assert_eq!(
            customized
                .serve_type_keybindings
                .key_for(&ServeTypeEnum::Spin),
            Some(key!(z))
        );
        ui.type_keys(&[KeyCode::Char('r'), KeyCode::Char('y')])
            .await;
        let reset = saved(&ui).await;
        let defaults = Settings::default();
        for set in SetTypeEnum::ALL {
            assert_eq!(
                reset.set_type_keybindings.keybindings_for(&set),
                defaults.set_type_keybindings.keybindings_for(&set)
            );
        }
        for serve in ServeTypeEnum::ALL {
            assert_eq!(
                reset.serve_type_keybindings.keybindings_for(&serve),
                defaults.serve_type_keybindings.keybindings_for(&serve)
            );
        }
    }

    #[tokio::test]
    async fn set_key_is_changed() {
        let mut ui = change_key_for(set_index(SetTypeEnum::High)).await;
        ui.type_keys(&[KeyCode::Char('x'), KeyCode::Enter]).await;
        let settings = saved(&ui).await;
        assert_eq!(
            settings
                .set_type_keybindings
                .keybindings_for(&SetTypeEnum::High),
            keys(&[key!(x)])
        );
        // the others are the same
        assert_eq!(
            settings.set_type_keybindings.key_for(&SetTypeEnum::Five),
            Some(key!('5'))
        );
        let selected = ui.selected().unwrap_or_default();
        assert!(
            selected.contains(&SetTypeEnum::High.describe().1) && selected.ends_with('x'),
            "{selected}"
        );
    }

    /// The digits choose the players, but sets are chosen once the player
    /// is: they can have digits. Settings, undo and back stay reserved.
    #[tokio::test]
    async fn sets_and_serves_can_have_digits_not_fixed_keys() {
        // the seven, then its digit for the high set
        let mut ui = change_key_for(set_index(SetTypeEnum::Seven)).await;
        ui.type_keys(&[KeyCode::Char('x'), KeyCode::Enter]).await;
        for _ in set_index(SetTypeEnum::Seven)..set_index(SetTypeEnum::High) {
            ui.press(KeyCode::Down).await;
        }
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('7'), KeyCode::Enter])
            .await;
        assert_eq!(
            saved(&ui)
                .await
                .set_type_keybindings
                .key_for(&SetTypeEnum::High),
            Some(key!('7'))
        );
        let mut ui = change_key_for(serve_index(ServeTypeEnum::Float)).await;
        ui.type_keys(&[KeyCode::Char('3'), KeyCode::Enter]).await;
        assert_eq!(
            saved(&ui)
                .await
                .serve_type_keybindings
                .key_for(&ServeTypeEnum::Float),
            Some(key!('3'))
        );
        for (code, shown) in [(KeyCode::Char('U'), "⇧-u"), (KeyCode::F(2), "F2")] {
            for index in [
                set_index(SetTypeEnum::High),
                serve_index(ServeTypeEnum::Spin),
            ] {
                let mut ui = change_key_for(index).await;
                ui.press(code).await;
                let message = current_labels().reserved_shortcut.replace("{}", shown);
                assert!(ui.screen().contains(&message), "{shown}: {}", ui.screen());
                ui.press(KeyCode::Enter).await;
                assert!(ui.saved_settings().await.is_none(), "{shown}");
            }
        }
    }

    #[tokio::test]
    async fn set_keys_do_not_conflict() {
        let mut ui = change_key_for(set_index(SetTypeEnum::High)).await;
        ui.press(KeyCode::Char('5')).await;
        let message = assigned_message("5", &SetTypeEnum::Five.describe().1);
        assert!(ui.screen().contains(&message), "{}", ui.screen());
        ui.press(KeyCode::Enter).await;
        assert!(ui.saved_settings().await.is_none());
    }

    #[tokio::test]
    async fn serve_keys_do_not_conflict() {
        let mut ui = change_key_for(serve_index(ServeTypeEnum::Float)).await;
        ui.press(KeyCode::Char('s')).await;
        let message = assigned_message("s", &ServeTypeEnum::Spin.describe().1);
        assert!(ui.screen().contains(&message), "{}", ui.screen());
        ui.press(KeyCode::Enter).await;
        assert!(ui.saved_settings().await.is_none());
    }

    /// Sets and serves are chosen along with the evaluation: a key can't be
    /// both an evaluation and a set or a serve.
    #[tokio::test]
    async fn sets_and_serves_do_not_conflict_with_evaluations() {
        let cases = [
            (
                set_index(SetTypeEnum::High),
                '+',
                EvalEnum::Positive.describe().1,
            ),
            (
                serve_index(ServeTypeEnum::Spin),
                '#',
                EvalEnum::Perfect.describe().1,
            ),
            (
                eval_index(EvalEnum::Perfect),
                'a',
                SetTypeEnum::High.describe().1,
            ),
            (
                eval_index(EvalEnum::Negative),
                'q',
                SetTypeEnum::Quick.describe().1,
            ),
            // a key of the serves, not of the sets
            (
                eval_index(EvalEnum::Positive),
                'f',
                SetTypeEnum::Fast.describe().1,
            ),
        ];
        for (index, key, other) in cases {
            let mut ui = change_key_for(index).await;
            ui.press(KeyCode::Char(key)).await;
            let message = assigned_message(&key.to_string(), &other);
            assert!(ui.screen().contains(&message), "{key}: {}", ui.screen());
            ui.press(KeyCode::Enter).await;
            assert!(ui.saved_settings().await.is_none(), "{key}");
        }
        // a key of a serve only
        let mut ui = change_key_for(serve_index(ServeTypeEnum::Spin)).await;
        ui.type_keys(&[KeyCode::Char('y'), KeyCode::Enter]).await;
        for _ in eval_index(EvalEnum::Perfect)..serve_index(ServeTypeEnum::Spin) {
            ui.press(KeyCode::Up).await;
        }
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('y')]).await;
        let message = assigned_message("y", &ServeTypeEnum::Spin.describe().1);
        assert!(ui.screen().contains(&message), "{}", ui.screen());
    }

    /// Sets and serves are never chosen together, nor along with the events:
    /// they can share their keys.
    #[tokio::test]
    async fn sets_serves_and_events_share_keys() {
        let mut ui = change_key_for(serve_index(ServeTypeEnum::Float)).await;
        ui.type_keys(&[KeyCode::Char('a'), KeyCode::Enter]).await;
        assert_eq!(
            saved(&ui)
                .await
                .serve_type_keybindings
                .key_for(&ServeTypeEnum::Float),
            Some(key!(a))
        );
        // the serve event on the key of the quick set
        let mut ui = change_key_for(0).await;
        ui.type_keys(&[KeyCode::Char('q'), KeyCode::Enter]).await;
        assert_eq!(
            saved(&ui)
                .await
                .scouting_keybindings
                .key_for(&EventTypeEnum::S),
            Some(key!(q))
        );
        // ...and the high set on the key of the block
        let mut ui = change_key_for(set_index(SetTypeEnum::High)).await;
        ui.type_keys(&[KeyCode::Char('d'), KeyCode::Enter]).await;
        assert_eq!(
            saved(&ui)
                .await
                .set_type_keybindings
                .key_for(&SetTypeEnum::High),
            Some(key!(d))
        );
    }

    #[tokio::test]
    async fn panels_side_by_side_or_stacked() {
        fn title_rows(ui: &mut Ui) -> (usize, usize) {
            let lines = ui.lines();
            let row = |title: &str| {
                lines
                    .iter()
                    .position(|l| l.contains(title))
                    .unwrap_or_else(|| panic!("{title} not shown: {}", lines.join("\n")))
            };
            (
                row(current_labels().general_settings),
                row(current_labels().key_bindings),
            )
        }
        let mut ui = settings().await;
        let (general, shortcuts) = title_rows(&mut ui);
        assert_eq!(general, shortcuts);
        ui.resize(80, 40);
        let (general, shortcuts) = title_rows(&mut ui);
        assert!(general < shortcuts);
    }

    #[test]
    fn default_shortcuts_avoid_fixed_keys_and_each_other() {
        use crate::shapes::keybinding::is_fixed_key;
        fn check<T: std::hash::Hash + Eq + Copy + std::fmt::Debug>(kb: &KeyBindings<T>, all: &[T]) {
            let mut seen = HashSet::new();
            for action in all {
                let cks = kb.keybindings_for(action);
                assert_eq!(cks.len(), 1, "{action:?} not on one key");
                for ck in cks {
                    assert!(!is_fixed_key(&ck), "{action:?} on fixed {ck}");
                    assert!(seen.insert(ck), "{ck} bound twice");
                }
            }
        }
        let settings = Settings::default();
        check(&settings.scouting_keybindings, &EventTypeEnum::ALL);
        check(&settings.eval_keybindings, &EvalEnum::ALL);
        // sets and serves on their codes, apart from the evaluations: the
        // digits are theirs too
        let codes = |codes: Vec<(String, Option<KeyCombination>)>| {
            for (code, ck) in codes {
                let ck = ck.unwrap_or_else(|| panic!("{code} not bound"));
                assert_eq!(ck.to_string(), code.to_lowercase(), "{code}");
                assert!(!is_reserved_key(&ck), "{code} on reserved {ck}");
                assert_eq!(settings.eval_keybindings.action_for(&ck), None, "{code}");
            }
        };
        codes(
            SetTypeEnum::ALL
                .iter()
                .map(|s| (s.to_string(), settings.set_type_keybindings.key_for(s)))
                .collect(),
        );
        codes(
            ServeTypeEnum::ALL
                .iter()
                .map(|s| (s.to_string(), settings.serve_type_keybindings.key_for(s)))
                .collect(),
        );
        let set_keys: HashSet<_> = SetTypeEnum::ALL
            .iter()
            .filter_map(|s| settings.set_type_keybindings.key_for(s))
            .collect();
        assert_eq!(set_keys.len(), SetTypeEnum::ALL.len());
        assert_eq!(
            navigation_keybindings().keybindings_for(&ScreenActionEnum::Settings),
            keys(&[SETTINGS_KEY])
        );
    }

    /// Only the scouting shortcuts are saved: navigation keys aren't settings.
    #[tokio::test]
    async fn navigation_keys_are_not_saved() {
        let mut ui = settings().await;
        ui.press(KeyCode::Right).await;
        let saved = ui.saved_settings_json().await;
        let fields: Vec<_> = saved.as_object().unwrap().keys().cloned().collect();
        assert!(!fields.contains(&"keybindings".to_string()), "{fields:?}");
        assert!(
            fields.contains(&"scouting_keybindings".to_string()),
            "{fields:?}"
        );
        for field in [
            "eval_keybindings",
            "set_type_keybindings",
            "serve_type_keybindings",
        ] {
            assert!(fields.contains(&field.to_string()), "{field}: {fields:?}");
        }
    }

    /// A settings file as saved up to v0.2.0: navigation keys customized
    /// (keybinding settings on "b"), events on a key now fixed, on more keys
    /// or on none, no evaluation keys.
    #[tokio::test]
    async fn settings_saved_by_older_versions() {
        let dir = TempDir::new().unwrap();
        let old = r#"{
            "language": "it",
            "analytics_enabled": false,
            "keybindings": {
                "keybindingsettings": ["b"],
                "languagesettings": ["s"],
                "quit": ["x"]
            },
            "scouting_keybindings": {
                "S": ["x", "s"],
                "P": [],
                "A": ["a", "shift-u"],
                "B": ["b", "F2"]
            }
        }"#;
        std::fs::write(dir.path().join(CONFIG), old).unwrap();
        let settings = StoreSettingsReader::new(fs_store(dir.path()))
            .read()
            .await
            .unwrap();
        let defaults = Settings::default();
        assert_eq!(settings.language, LanguageEnum::It);
        assert!(!settings.consent.analytics_enabled);
        assert_eq!(
            settings
                .scouting_keybindings
                .keybindings_for(&EventTypeEnum::S),
            keys(&[key!(s)])
        );
        assert_eq!(
            settings
                .scouting_keybindings
                .keybindings_for(&EventTypeEnum::P),
            keys(&[key!(p)])
        );
        assert_eq!(
            settings
                .scouting_keybindings
                .keybindings_for(&EventTypeEnum::A),
            keys(&[key!(a)])
        );
        assert_eq!(
            settings
                .scouting_keybindings
                .keybindings_for(&EventTypeEnum::B),
            keys(&[key!(b)])
        );
        for eval in EvalEnum::ALL {
            assert_eq!(
                settings.eval_keybindings.keybindings_for(&eval),
                defaults.eval_keybindings.keybindings_for(&eval)
            );
        }
        for set in SetTypeEnum::ALL {
            assert_eq!(
                settings.set_type_keybindings.keybindings_for(&set),
                defaults.set_type_keybindings.keybindings_for(&set)
            );
        }
        for serve in ServeTypeEnum::ALL {
            assert_eq!(
                settings.serve_type_keybindings.keybindings_for(&serve),
                defaults.serve_type_keybindings.keybindings_for(&serve)
            );
        }
    }

    /// Up to v0.4.3 an evaluation could be on a letter that's now the key
    /// of a set or a serve: the evaluation keeps it, the set has no key
    /// until the scout gives it one.
    #[tokio::test]
    async fn evaluation_keys_of_v0_4_3_win_over_the_sets() {
        let dir = TempDir::new().unwrap();
        let old = r##"{
            "language": "en",
            "eval_keybindings": { "#": ["a"], "+": ["f"] }
        }"##;
        std::fs::write(dir.path().join(CONFIG), old).unwrap();
        let settings = StoreSettingsReader::new(fs_store(dir.path()))
            .read()
            .await
            .unwrap();
        assert_eq!(
            settings.eval_keybindings.key_for(&EvalEnum::Perfect),
            Some(key!(a))
        );
        assert_eq!(
            settings.set_type_keybindings.key_for(&SetTypeEnum::High),
            None
        );
        assert_eq!(
            settings.set_type_keybindings.key_for(&SetTypeEnum::Fast),
            None
        );
        assert_eq!(
            settings
                .serve_type_keybindings
                .key_for(&ServeTypeEnum::Float),
            None
        );
        // the others on their codes
        assert_eq!(
            settings.set_type_keybindings.key_for(&SetTypeEnum::Super),
            Some(key!(s))
        );
        assert_eq!(
            settings
                .serve_type_keybindings
                .key_for(&ServeTypeEnum::Spin),
            Some(key!(s))
        );
    }

    // --- the freeballs, added after v0.5.0 -------------------------------------

    fn event_index(event: EventTypeEnum) -> usize {
        EventTypeEnum::ALL.iter().position(|e| *e == event).unwrap()
    }

    /// Listed among the events, on "x" and Shift-x.
    #[tokio::test]
    async fn freeballs_listed_on_x() {
        for (event, key) in [(EventTypeEnum::FB, "x"), (EventTypeEnum::OF, "⇧-x")] {
            let mut ui = shortcut(event_index(event)).await;
            let selected = ui.selected().unwrap_or_default();
            assert!(selected.contains(&event.describe().1), "{selected}");
            assert!(selected.trim_end().ends_with(key), "{selected}");
        }
    }

    /// The settings of v0.5.0 have no freeballs: they get their default
    /// keys, unless another event took them.
    #[tokio::test]
    async fn settings_of_v0_5_0_get_the_freeball_keys() {
        let read = |json: &'static str| async move {
            let dir = TempDir::new().unwrap();
            std::fs::write(dir.path().join(CONFIG), json).unwrap();
            StoreSettingsReader::new(fs_store(dir.path()))
                .read()
                .await
                .unwrap()
        };
        let settings =
            read(r##"{ "language": "en", "scouting_keybindings": { "S": ["s"] } }"##).await;
        let events = &settings.scouting_keybindings;
        assert_eq!(events.key_for(&EventTypeEnum::FB), Some(key!(x)));
        assert_eq!(events.key_for(&EventTypeEnum::OF), Some(key!(shift - x)));
        // the attack on "x", the opponent score on Shift-x: they keep them
        let settings = read(
            r##"{ "language": "en", "scouting_keybindings": { "A": ["x"], "OS": ["shift-x"] } }"##,
        )
        .await;
        let events = &settings.scouting_keybindings;
        assert_eq!(events.key_for(&EventTypeEnum::A), Some(key!(x)));
        assert_eq!(events.key_for(&EventTypeEnum::OS), Some(key!(shift - x)));
        assert_eq!(events.key_for(&EventTypeEnum::FB), None);
        assert_eq!(events.key_for(&EventTypeEnum::OF), None);
        // an evaluation or a set on "x" doesn't take it from the freeball
        let settings = read(
            r##"{ "language": "en", "eval_keybindings": { "+": ["x"] }, "set_type_keybindings": { "high": ["x"] } }"##,
        )
        .await;
        assert_eq!(
            settings.scouting_keybindings.key_for(&EventTypeEnum::FB),
            Some(key!(x))
        );
    }

    /// The freeball keys are events': not for another event, and the
    /// freeballs don't take another event's key.
    #[tokio::test]
    async fn freeball_keys_conflict_with_the_events() {
        let conflict = |key: &str, event: EventTypeEnum| {
            current_labels()
                .shortcut_already_assigned
                .replacen("{}", key, 1)
                .replacen("{}", &event.describe().1, 1)
        };
        let cases = [
            (0, KeyCode::Char('x'), conflict("x", EventTypeEnum::FB)),
            (0, KeyCode::Char('X'), conflict("⇧-x", EventTypeEnum::OF)),
            (
                event_index(EventTypeEnum::FB),
                KeyCode::Char('a'),
                conflict("a", EventTypeEnum::A),
            ),
            (
                event_index(EventTypeEnum::OF),
                KeyCode::Char('S'),
                conflict("⇧-s", EventTypeEnum::OS),
            ),
            (
                event_index(EventTypeEnum::FB),
                KeyCode::Char('X'),
                conflict("⇧-x", EventTypeEnum::OF),
            ),
        ];
        for (index, code, message) in cases {
            let mut ui = change_key_for(index).await;
            ui.press(code).await;
            assert!(ui.screen().contains(&message), "{message}: {}", ui.screen());
            ui.press(KeyCode::Enter).await;
            assert!(ui.saved_settings().await.is_none(), "{message}");
        }
        // nor a reserved key
        let mut ui = change_key_for(event_index(EventTypeEnum::FB)).await;
        ui.press(KeyCode::Char('U')).await;
        let message = current_labels().reserved_shortcut.replace("{}", "⇧-u");
        assert!(ui.screen().contains(&message), "{}", ui.screen());
    }

    /// The reset gives them back their keys.
    #[tokio::test]
    async fn reset_gives_the_freeballs_their_keys() {
        let mut ui = change_key_for(event_index(EventTypeEnum::FB)).await;
        ui.type_keys(&[KeyCode::Char('w'), KeyCode::Enter]).await;
        assert_eq!(
            saved(&ui)
                .await
                .scouting_keybindings
                .key_for(&EventTypeEnum::FB),
            Some(key!(w))
        );
        ui.type_keys(&[KeyCode::Char('r'), KeyCode::Char('y')])
            .await;
        let settings = saved(&ui).await;
        assert_eq!(
            settings.scouting_keybindings.key_for(&EventTypeEnum::FB),
            Some(key!(x))
        );
        assert_eq!(
            settings.scouting_keybindings.key_for(&EventTypeEnum::OF),
            Some(key!(shift - x))
        );
    }

    /// Hand-edited sets and serves: reserved keys and the keys of the
    /// evaluations are dropped, digits and more keys handled as for events.
    #[tokio::test]
    async fn hand_edited_sets_and_serves() {
        let dir = TempDir::new().unwrap();
        let old = r##"{
            "language": "en",
            "set_type_keybindings": {
                "high": ["shift-u"],
                "five": ["+"],
                "super": ["x", "w"],
                "pipe": ["F2"],
                "quick": []
            },
            "serve_type_keybindings": { "float": ["#"], "spin": ["9"] }
        }"##;
        std::fs::write(dir.path().join(CONFIG), old).unwrap();
        let settings = StoreSettingsReader::new(fs_store(dir.path()))
            .read()
            .await
            .unwrap();
        let sets = &settings.set_type_keybindings;
        // back to their codes
        assert_eq!(sets.key_for(&SetTypeEnum::High), Some(key!(a)));
        assert_eq!(sets.key_for(&SetTypeEnum::Five), Some(key!('5')));
        assert_eq!(sets.key_for(&SetTypeEnum::Pipe), Some(key!('0')));
        assert_eq!(sets.key_for(&SetTypeEnum::Quick), Some(key!(q)));
        // one key only
        assert_eq!(sets.keybindings_for(&SetTypeEnum::Super).len(), 1);
        let serves = &settings.serve_type_keybindings;
        assert_eq!(serves.key_for(&ServeTypeEnum::Float), Some(key!(f)));
        assert_eq!(serves.key_for(&ServeTypeEnum::Spin), Some(key!('9')));
    }

    /// Up to v0.0.7 the settings were the language only.
    #[tokio::test]
    async fn settings_saved_by_v0_0_x_load() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(CONFIG), r#"{ "language": "it" }"#).unwrap();
        let settings = StoreSettingsReader::new(fs_store(dir.path()))
            .read()
            .await
            .expect("expected the settings");
        let defaults = Settings::default();
        assert_eq!(settings.language, LanguageEnum::It);
        assert!(settings.consent.analytics_enabled);
        assert_eq!(
            settings.scouting_keybindings.key_for(&EventTypeEnum::S),
            defaults.scouting_keybindings.key_for(&EventTypeEnum::S)
        );
    }
}
