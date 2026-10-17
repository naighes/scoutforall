//! The settings screen: general settings on a panel, the scouting keyboard
//! shortcuts on the other one, every change saved right away.

#[cfg(test)]
mod tests {
    use crate::{
        localization::current_labels,
        providers::{
            fs::{path::get_config_file_path, settings_reader::FileSystemSettingsReader},
            settings_reader::SettingsReader,
        },
        shapes::{
            enums::{EvalEnum, EventTypeEnum, LanguageEnum, ScreenActionEnum, WithDesc},
            keybinding::{KeyBindings, SETTINGS_KEY, navigation_keybindings},
            settings::Settings,
        },
        tests::ui::{TeamFixture, Ui},
    };
    use crokey::{KeyCombination, crossterm::event::KeyCode, key};
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
            assert!(screen.contains(&event.with_desc().1), "{event}: {screen}");
        }
        for eval in EvalEnum::ALL {
            assert!(screen.contains(&eval.with_desc().1), "{eval}: {screen}");
        }
        assert!(!screen.contains("Hyphen"), "{screen}");
    }

    #[tokio::test]
    async fn fixed_keys_are_listed_but_not_selectable() {
        let mut ui = settings().await;
        let screen = ui.screen();
        let fixed = current_labels().fixed;
        for key in ["F2", "u", "Esc", "0-9"] {
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
            selected.contains(&EvalEnum::ALL.last().unwrap().with_desc().1),
            "{selected}"
        );
    }

    #[tokio::test]
    async fn tab_switches_panel() {
        let mut ui = settings().await;
        ui.press(KeyCode::Tab).await;
        let selected = ui.selected().unwrap_or_default();
        assert!(
            selected.contains(&EventTypeEnum::S.with_desc().1),
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
        assert!(!saved(&ui).await.analytics_enabled);
        assert!(ui.selected().unwrap_or_default().contains("[ ]"));
        ui.press(KeyCode::Enter).await;
        assert!(saved(&ui).await.analytics_enabled);
        assert!(ui.selected().unwrap_or_default().contains("[X]"));
    }

    #[tokio::test]
    async fn changing_a_setting_keeps_the_others() {
        let mut ui = change_key_for(0).await;
        ui.type_keys(&[KeyCode::Char('x'), KeyCode::Enter]).await;
        ui.press(KeyCode::BackTab).await;
        ui.press(KeyCode::Right).await;
        let settings = saved(&ui).await;
        assert_eq!(settings.language, LanguageEnum::It);
        assert_eq!(
            settings
                .scouting_keybindings
                .keybindings_for(&EventTypeEnum::S),
            keys(&[key!(x)])
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
            let (_, desc) = action.with_desc();
            assert!(!footer.contains(&desc), "{desc}: {footer}");
        }
        ui.type_keys(&[KeyCode::Char('x'), KeyCode::Enter]).await;
        assert_eq!(
            saved(&ui)
                .await
                .scouting_keybindings
                .keybindings_for(&EventTypeEnum::S),
            keys(&[key!(x)])
        );
        // straight back on the settings
        let selected = ui.selected().unwrap_or_default();
        assert!(
            selected.contains(&EventTypeEnum::S.with_desc().1),
            "{selected}"
        );
        assert!(selected.trim_end().ends_with('x'), "{selected}");
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
            (KeyCode::Char('u'), "u"),
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
            .replacen("{}", &EventTypeEnum::B.with_desc().1, 1);
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
            .replacen("{}", &EvalEnum::Perfect.with_desc().1, 1);
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
        ui.type_keys(&[KeyCode::Char('x'), KeyCode::Enter]).await;
        for _ in 0..eval_index(EvalEnum::Perfect) {
            ui.press(KeyCode::Down).await;
        }
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('z'), KeyCode::Enter])
            .await;
        let before = saved(&ui).await;

        // "x" is now the serve: not for the attack
        for _ in attack..eval_index(EvalEnum::Perfect) {
            ui.press(KeyCode::Up).await;
        }
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('x')]).await;
        let message = current_labels()
            .shortcut_already_assigned
            .replacen("{}", "x", 1)
            .replacen("{}", &EventTypeEnum::S.with_desc().1, 1);
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
            .replacen("{}", &EvalEnum::Perfect.with_desc().1, 1);
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
            selected.contains(&EventTypeEnum::S.with_desc().1),
            "{selected}"
        );
    }

    #[tokio::test]
    async fn reset_to_defaults_asks_for_confirmation() {
        let mut ui = change_key_for(0).await;
        ui.type_keys(&[KeyCode::Char('x'), KeyCode::Enter]).await;
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
            keys(&[key!(x)])
        );
        // the dialog is gone: "y" alone does nothing
        ui.press(KeyCode::Char('y')).await;
        assert!(
            saved(&ui)
                .await
                .scouting_keybindings
                .keybindings_for(&EventTypeEnum::S)
                .contains(&key!(x))
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
        assert!(
            fields.contains(&"eval_keybindings".to_string()),
            "{fields:?}"
        );
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
                "A": ["a", "u"],
                "B": ["b", "F2"]
            }
        }"#;
        std::fs::write(get_config_file_path(dir.path()), old).unwrap();
        let settings = FileSystemSettingsReader::new(dir.path())
            .read()
            .await
            .unwrap();
        let defaults = Settings::default();
        assert_eq!(settings.language, LanguageEnum::It);
        assert!(!settings.analytics_enabled);
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
    }
}
