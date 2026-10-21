//! The freeballs while scouting: ours (`x`, then the player sending it) and
//! theirs (`X`, nothing else to choose), listed and accepted only where
//! they can be, saved as events of their own, undone like the others.

#[cfg(test)]
mod tests {
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;
    use scout4all_core::{
        localization::current_labels,
        shapes::{
            enums::{EvalEnum, EventTypeEnum, WithDesc},
            technique::SetTypeEnum,
        },
    };

    // the lineup: Anna the setter (1 in the player choice), Bea outside
    // hitter 1 (2), Carla middle blocker 2 (3), Dora the opposite (4), Elsa
    // outside hitter 2 (5), Fede middle blocker 1 (6), Gina the libero (7).
    // We serve, the setter in position 1: Bea, Carla and Dora in the front
    // row.
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
            "not scouting: {}",
            ui.screen()
        );
        ui
    }

    async fn keys(ui: &mut Ui, keys: &str) {
        for c in keys.chars() {
            ui.press(KeyCode::Char(c)).await;
        }
    }

    /// The row of the events panel listing the event, if it's listed: the
    /// panel is above the latest events, which list them too.
    fn event_row(ui: &mut Ui, event: EventTypeEnum) -> Option<String> {
        let name = event.with_desc().1;
        ui.lines()
            .iter()
            .take_while(|l| !l.contains(current_labels().latest_events))
            .flat_map(|l| {
                l.split('│')
                    .map(str::trim)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .find(|c| c.starts_with(&name))
    }

    /// The last event saved, split in its columns: event, player, eval.
    fn last_saved(ui: &Ui) -> (String, String, String) {
        let events = ui.saved_first_set_events();
        let last = events.last().expect("no event saved");
        let columns: Vec<_> = last.split(',').map(str::to_string).collect();
        (columns[1].clone(), columns[2].clone(), columns[3].clone())
    }

    fn saved_count(ui: &Ui) -> usize {
        ui.saved_first_set_events().len()
    }

    fn choosing_the_event(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().choose_the_event)
    }

    fn choosing_the_player(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().player_selection)
    }

    // the rallies so far, by the keys typed: after "s=" (our serve error)
    // they serve, and the libero receives

    /// The ball on our side to play: our freeball, not theirs.
    const BALL_OURS_TO_PLAY: [(&str, &str); 14] = [
        ("s/", "their reception came back"),
        ("s=p7#", "reception #"),
        ("s=p7+", "reception +"),
        ("s=p7!", "reception !"),
        ("s=p7-", "reception -"),
        ("s-d7#", "dig #"),
        ("s-d7+", "dig +"),
        ("s-d7!", "dig !"),
        ("s-d7-", "dig -"),
        ("s-d7+a2+", "attack +"),
        ("s-d7+a2!", "attack !"),
        ("s-b2+", "block +"),
        ("s-X", "their freeball"),
        ("s-Xx1X", "their freeball, after ours"),
    ];

    /// The ball on their side: their freeball, not ours.
    const BALL_THEIRS: [(&str, &str); 9] = [
        ("s+", "our serve +"),
        ("s-", "our serve -"),
        ("s=p7/", "our reception over"),
        ("s-d7/", "our dig over"),
        ("s-d7+a2-", "our attack dug"),
        ("s-b2-", "block -"),
        ("s-d7+x1", "our freeball"),
        ("s-Xx1", "our freeball, after theirs"),
        ("s=p7+x1", "our freeball, after the reception"),
    ];

    fn not_available(event: EventTypeEnum) -> String {
        current_labels()
            .event_is_not_available
            .replace("{}", &event.to_string())
    }

    // --- our freeball -------------------------------------------------------------

    /// Wherever the ball is on our side to play, "x" asks who sent it, and
    /// saves it with no evaluation.
    #[tokio::test]
    async fn our_freeball_asks_the_player() {
        let cases = BALL_OURS_TO_PLAY;
        for (before, case) in cases {
            let mut ui = scouting().await;
            keys(&mut ui, before).await;
            let row = event_row(&mut ui, EventTypeEnum::FB)
                .unwrap_or_else(|| panic!("{case}: not listed: {}", ui.screen()));
            assert!(row.ends_with('x'), "{case}: {row}");
            let saved = saved_count(&ui);
            keys(&mut ui, "x").await;
            assert!(choosing_the_player(&mut ui), "{case}: {}", ui.screen());
            assert_eq!(saved_count(&ui), saved, "{case}: saved before the player");
            // the libero too
            keys(&mut ui, "7").await;
            assert!(choosing_the_event(&mut ui), "{case}: {}", ui.screen());
            let (event, player, eval) = last_saved(&ui);
            assert_eq!(event, "FB", "{case}");
            assert!(!player.is_empty(), "{case}");
            assert_eq!(eval, "", "{case}");
            // the ball is theirs now: they can send it back
            assert!(
                event_row(&mut ui, EventTypeEnum::OF).is_some(),
                "{case}: {}",
                ui.screen()
            );
        }
    }

    /// Where the ball is on their side or just came over, our freeball
    /// isn't listed, and its key says so.
    #[tokio::test]
    async fn no_freeball_of_ours_where_the_ball_is_theirs() {
        let cases = BALL_THEIRS;
        for (before, case) in cases {
            let mut ui = scouting().await;
            keys(&mut ui, before).await;
            assert_eq!(event_row(&mut ui, EventTypeEnum::FB), None, "{case}");
            let saved = saved_count(&ui);
            keys(&mut ui, "x").await;
            assert!(
                ui.screen().contains(&not_available(EventTypeEnum::FB)),
                "{case}: {}",
                ui.screen()
            );
            assert!(choosing_the_event(&mut ui), "{case}");
            assert_eq!(saved_count(&ui), saved, "{case}");
        }
    }

    /// Before the serve neither freeball.
    #[tokio::test]
    async fn no_freeball_before_the_serve() {
        let mut ui = scouting().await;
        assert_eq!(event_row(&mut ui, EventTypeEnum::FB), None);
        assert_eq!(event_row(&mut ui, EventTypeEnum::OF), None);
        for key in ["x", "X"] {
            keys(&mut ui, key).await;
            assert!(choosing_the_event(&mut ui), "{}", ui.screen());
            assert_eq!(saved_count(&ui), 0);
        }
    }

    // --- their freeball -----------------------------------------------------------

    /// Wherever the ball is on their side, "X" saves their freeball at
    /// once: no player, no evaluation.
    #[tokio::test]
    async fn their_freeball_is_saved_at_once() {
        let cases = BALL_THEIRS;
        for (before, case) in cases {
            let mut ui = scouting().await;
            keys(&mut ui, before).await;
            let row = event_row(&mut ui, EventTypeEnum::OF)
                .unwrap_or_else(|| panic!("{case}: not listed: {}", ui.screen()));
            assert!(row.ends_with("Shift-x"), "{case}: {row}");
            let saved = saved_count(&ui);
            keys(&mut ui, "X").await;
            assert!(choosing_the_event(&mut ui), "{case}: {}", ui.screen());
            assert_eq!(saved_count(&ui), saved + 1, "{case}");
            assert_eq!(
                last_saved(&ui),
                ("OF".to_string(), String::new(), String::new()),
                "{case}"
            );
            // the ball is ours: no dig nor block on it, an attack or our
            // freeball
            assert_eq!(event_row(&mut ui, EventTypeEnum::D), None, "{case}");
            assert_eq!(event_row(&mut ui, EventTypeEnum::B), None, "{case}");
            assert!(event_row(&mut ui, EventTypeEnum::A).is_some(), "{case}");
            assert!(event_row(&mut ui, EventTypeEnum::FB).is_some(), "{case}");
        }
    }

    #[tokio::test]
    async fn no_freeball_of_theirs_where_the_ball_is_ours() {
        let cases = BALL_OURS_TO_PLAY;
        for (before, case) in cases {
            let mut ui = scouting().await;
            keys(&mut ui, before).await;
            assert_eq!(event_row(&mut ui, EventTypeEnum::OF), None, "{case}");
            let saved = saved_count(&ui);
            keys(&mut ui, "X").await;
            assert!(
                ui.screen().contains(&not_available(EventTypeEnum::OF)),
                "{case}: {}",
                ui.screen()
            );
            assert_eq!(saved_count(&ui), saved, "{case}");
        }
    }

    /// The attack on their freeball: the super for the outside hitter,
    /// as after their reception came back.
    #[tokio::test]
    async fn super_chosen_for_the_attack_on_their_freeball() {
        let mut ui = scouting().await;
        keys(&mut ui, "s-Xa2").await;
        let set_type = current_labels().set_type;
        assert!(ui.screen().contains(set_type), "{}", ui.screen());
        let selected = ui
            .lines()
            .iter()
            .flat_map(|l| {
                l.split('│')
                    .map(str::trim)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .find(|c| c.starts_with("[X]"))
            .unwrap_or_else(|| panic!("{}", ui.screen()));
        assert!(
            selected.ends_with(&SetTypeEnum::Super.with_desc().1),
            "{selected}"
        );
        keys(&mut ui, "#").await;
        let (event, _, eval) = last_saved(&ui);
        assert_eq!((event.as_str(), eval.as_str()), ("A", "#"));
    }

    /// Their freeball falling on our court: their point.
    #[tokio::test]
    async fn their_freeball_untouched_is_their_point() {
        let mut ui = scouting().await;
        keys(&mut ui, "s-X").await;
        ui.press(KeyCode::Char('S')).await;
        assert_eq!(last_saved(&ui).0, "OS");
        // they serve now: our reception
        assert!(
            event_row(&mut ui, EventTypeEnum::P).is_some(),
            "{}",
            ui.screen()
        );
    }

    // --- undo and the latest events -----------------------------------------------

    /// The undo of our freeball goes back to the player choice, of theirs
    /// to the event choice.
    #[tokio::test]
    async fn undo_of_the_freeballs() {
        let mut ui = scouting().await;
        keys(&mut ui, "s-d7+x1").await;
        let saved = saved_count(&ui);
        keys(&mut ui, "U").await;
        assert!(choosing_the_player(&mut ui), "{}", ui.screen());
        assert_eq!(saved_count(&ui), saved - 1);
        // another player sent it
        keys(&mut ui, "2").await;
        assert_eq!(last_saved(&ui).0, "FB");
        assert_eq!(saved_count(&ui), saved);
        keys(&mut ui, "X").await;
        keys(&mut ui, "U").await;
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        assert_eq!(saved_count(&ui), saved);
        assert!(event_row(&mut ui, EventTypeEnum::OF).is_some());
        // back to the event choice from the player choice
        let mut ui = scouting().await;
        keys(&mut ui, "s-d7+x").await;
        keys(&mut ui, "U").await;
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        assert!(event_row(&mut ui, EventTypeEnum::FB).is_some());
    }

    #[tokio::test]
    async fn latest_events_show_the_freeballs() {
        let mut ui = scouting().await;
        keys(&mut ui, "s-d7+x2X").await;
        let labels = current_labels();
        let lines = ui.lines();
        let ours = lines
            .iter()
            .find(|l| l.contains(labels.freeball) && l.contains("Bea"))
            .unwrap_or_else(|| panic!("{}", ui.screen()));
        assert!(!ours.contains(labels.opponent_freeball), "{ours}");
        assert!(
            lines.iter().any(|l| l.contains(labels.opponent_freeball)),
            "{}",
            ui.screen()
        );
    }

    // --- the key bindings -----------------------------------------------------------

    fn event_index(event: EventTypeEnum) -> usize {
        EventTypeEnum::ALL.iter().position(|e| *e == event).unwrap()
    }

    /// Both keys can be changed while scouting.
    #[tokio::test]
    async fn rebound_freeball_keys_while_scouting() {
        let mut ui = scouting().await;
        for (event, key) in [(EventTypeEnum::FB, 'z'), (EventTypeEnum::OF, 'Z')] {
            ui.press(KeyCode::F(2)).await;
            for _ in 0..event_index(event) {
                ui.press(KeyCode::Down).await;
            }
            ui.type_keys(&[KeyCode::Enter, KeyCode::Char(key), KeyCode::Enter])
                .await;
            ui.press(KeyCode::Esc).await;
            assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        }
        keys(&mut ui, "s-").await;
        let row = event_row(&mut ui, EventTypeEnum::OF).unwrap();
        assert!(row.ends_with("Shift-z"), "{row}");
        // the old key is gone
        keys(&mut ui, "X").await;
        assert_eq!(saved_count(&ui), 1);
        keys(&mut ui, "Z").await;
        assert_eq!(last_saved(&ui).0, "OF");
        let row = event_row(&mut ui, EventTypeEnum::FB).unwrap();
        assert!(row.ends_with('z'), "{row}");
        keys(&mut ui, "x").await;
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        keys(&mut ui, "z1").await;
        assert_eq!(last_saved(&ui).0, "FB");
    }

    /// Bind the shortcut at `index` of the settings list to "x".
    async fn bind_to_x(ui: &mut Ui, index: usize) {
        ui.press(KeyCode::F(2)).await;
        for _ in 0..index {
            ui.press(KeyCode::Down).await;
        }
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('x'), KeyCode::Enter])
            .await;
        assert!(
            !ui.screen().contains(current_labels().press_the_shortcut),
            "x refused: {}",
            ui.screen()
        );
        ui.press(KeyCode::Esc).await;
    }

    /// Our freeball's key can be an evaluation's or a set's too: they're
    /// never expected at the same time.
    #[tokio::test]
    async fn freeball_key_shared_with_an_evaluation() {
        let mut ui = scouting().await;
        let positive = EvalEnum::ALL
            .iter()
            .position(|e| *e == EvalEnum::Positive)
            .unwrap();
        bind_to_x(&mut ui, EventTypeEnum::ALL.len() + positive).await;
        // "x" is "+" for the serve, our freeball after the dig
        keys(&mut ui, "sx").await;
        let (event, _, eval) = last_saved(&ui);
        assert_eq!((event.as_str(), eval.as_str()), ("S", "+"));
        keys(&mut ui, "d7#x1").await;
        assert_eq!(last_saved(&ui).0, "FB");
    }

    #[tokio::test]
    async fn freeball_key_shared_with_a_set() {
        let mut ui = scouting().await;
        let high = SetTypeEnum::ALL
            .iter()
            .position(|s| *s == SetTypeEnum::High)
            .unwrap();
        bind_to_x(
            &mut ui,
            EventTypeEnum::ALL.len() + EvalEnum::ALL.len() + high,
        )
        .await;
        // "x" is the high set for the attack, then our freeball
        keys(&mut ui, "s-d7#a2x+").await;
        let events = ui.saved_first_set_events();
        let attack = events.last().unwrap();
        assert_eq!(
            attack.split(',').nth(5),
            Some(SetTypeEnum::High.to_string().as_str()),
            "{attack}"
        );
        keys(&mut ui, "x1").await;
        assert_eq!(last_saved(&ui).0, "FB");
    }

    // --- the statistics -------------------------------------------------------------

    /// The match statistics list the freeballs: the ones given, by player
    /// too, the ones received and how many of these we exploited.
    #[tokio::test]
    async fn match_stats_show_the_freeballs() {
        let mut ui = scouting().await;
        // their freeball exploited: our kill
        keys(&mut ui, "s-Xa2#").await;
        // their freeball not exploited: they dig and attack again; then our
        // freeball, and their point
        keys(&mut ui, "s-Xa2-d7+x1S").await;
        let labels = current_labels();
        while !ui.screen().contains(labels.match_list) {
            ui.press(KeyCode::Esc).await;
        }
        ui.press(KeyCode::Char(' ')).await;
        // the stat filter, its last entry
        ui.type_keys(&[KeyCode::Tab, KeyCode::Tab, KeyCode::Tab, KeyCode::Down])
            .await;
        // the rows of the freeball table, label and value
        let rows = |ui: &mut Ui| {
            let names = [
                labels.freeballs_given,
                labels.freeballs_received,
                labels.freeballs_exploited,
            ];
            ui.lines()
                .iter()
                .flat_map(|l| {
                    l.split('│')
                        .map(str::trim)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .filter(|c| names.iter().any(|n| c.starts_with(n)))
                .map(|c| c.split_whitespace().map(str::to_string).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            rows(&mut ui),
            [
                [labels.freeballs_given, "1"],
                [labels.freeballs_received, "2"],
                [labels.freeballs_exploited, "50%(1/2)"],
            ],
            "{}",
            ui.screen()
        );
        // with a player: only the ones given, theirs have no player. Anna
        // sent it, Bea didn't
        ui.press(KeyCode::Tab).await;
        for (player, given) in [("Anna", "1"), ("Bea", "-")] {
            for _ in 0..8 {
                // the player selected: the highlighted one in the list
                let selected = ui
                    .lines()
                    .into_iter()
                    .skip_while(|l| !l.contains(&format!("┌{}", labels.player)))
                    .find(|l| l.contains(">>"));
                match selected {
                    Some(p) if p.contains(player) => break,
                    _ => ui.press(KeyCode::Up).await,
                };
            }
            assert_eq!(
                rows(&mut ui),
                [[labels.freeballs_given, given]],
                "{player}: {}",
                ui.screen()
            );
        }
    }
}
