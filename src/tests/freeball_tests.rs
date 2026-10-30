//! The freeballs while scouting: ours (`x`) and theirs (`X`), team events
//! with nothing else to choose, listed and accepted only where they can be,
//! saved as events of their own, undone like the others.

#[cfg(test)]
mod tests {
    use crate::localization::{Describe, current_labels};
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;
    use scout4all_core::shapes::{
        enums::{EvalEnum, EventTypeEnum},
        technique::SetTypeEnum,
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
        let name = event.describe().1;
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

    // the rallies so far, by the keys typed: after "s=" (our serve error)
    // they serve, and the libero receives

    /// The ball on our side to play: our freeball, not theirs.
    const BALL_OURS_TO_PLAY: [(&str, &str); 12] = [
        ("s/", "their reception came back"),
        ("s=p7#", "reception #"),
        ("s=p7+", "reception +"),
        ("s=p7!", "reception !"),
        ("s=p7-", "reception -"),
        ("s-d7#", "dig #"),
        ("s-d7+", "dig +"),
        ("s-d7!", "dig !"),
        ("s-d7-", "dig -"),
        ("s-d7+a2!", "attack !"),
        ("s-b2+X", "their freeball"),
        ("s-b2+XxX", "their freeball, after ours"),
    ];

    /// Our attack or block they couldn't counter: we play the ball again,
    /// after their freeball or not (both freeballs).
    const BALL_OURS_AFTER_THEIR_FREEBALL_OR_NOT: [(&str, &str); 2] =
        [("s-d7+a2+", "attack +"), ("s-b2+", "block +")];

    /// The ball on their side, sent over without an attack: their freeball;
    /// ours too, after their attack and our dig not scouted.
    const BALL_THEIRS: [(&str, &str); 5] = [
        ("s=p7/", "our reception over"),
        ("s-d7/", "our dig over"),
        ("s-d7+x", "our freeball"),
        ("s-b2+Xx", "our freeball, after theirs"),
        ("s=p7+x", "our freeball, after the reception"),
    ];

    /// The ball on their side, to attack: not their freeball (it would make
    /// our serve `/`, our attack or block `+`); ours, after their attack and
    /// our dig not scouted.
    const BALL_THEIRS_TO_ATTACK: [(&str, &str); 5] = [
        ("s+", "our serve +"),
        ("s!", "our serve !"),
        ("s-", "our serve -"),
        ("s-d7+a2-", "our attack dug"),
        ("s-b2-", "block -"),
    ];

    fn not_available(event: EventTypeEnum) -> String {
        current_labels()
            .domain
            .event_is_not_available
            .replace("{}", &event.to_string())
    }

    // --- our freeball -------------------------------------------------------------

    /// Wherever the ball is on our side to play, "x" saves our freeball at
    /// once: a team event, no player, no evaluation. On their side too, our
    /// dig being optional: they attacked, we dug without scouting it.
    #[tokio::test]
    async fn our_freeball_is_saved_at_once() {
        let cases = BALL_OURS_TO_PLAY
            .iter()
            .chain(&BALL_OURS_AFTER_THEIR_FREEBALL_OR_NOT)
            .chain(&BALL_THEIRS)
            .chain(&BALL_THEIRS_TO_ATTACK);
        for (before, case) in cases {
            let mut ui = scouting().await;
            keys(&mut ui, before).await;
            let row = event_row(&mut ui, EventTypeEnum::FB)
                .unwrap_or_else(|| panic!("{case}: not listed: {}", ui.screen()));
            assert!(row.ends_with('x'), "{case}: {row}");
            let saved = saved_count(&ui);
            keys(&mut ui, "x").await;
            assert!(choosing_the_event(&mut ui), "{case}: {}", ui.screen());
            assert_eq!(saved_count(&ui), saved + 1, "{case}");
            assert_eq!(
                last_saved(&ui),
                ("FB".to_string(), String::new(), String::new()),
                "{case}"
            );
            // the ball is theirs now: they can send it back
            assert!(
                event_row(&mut ui, EventTypeEnum::OF).is_some(),
                "{case}: {}",
                ui.screen()
            );
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

    /// Wherever they can send it, "X" saves their freeball at once: no
    /// player, no evaluation.
    #[tokio::test]
    async fn their_freeball_is_saved_at_once() {
        let cases = BALL_THEIRS
            .iter()
            .chain(&BALL_OURS_AFTER_THEIR_FREEBALL_OR_NOT);
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

    /// Where the ball is ours to play, or theirs to attack.
    #[tokio::test]
    async fn no_freeball_of_theirs_where_the_ball_is_ours_or_they_attack() {
        let cases = BALL_OURS_TO_PLAY.iter().chain(&BALL_THEIRS_TO_ATTACK);
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
        keys(&mut ui, "s-b2+Xa2").await;
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
            selected.ends_with(&SetTypeEnum::Super.describe().1),
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
        keys(&mut ui, "s-b2+X").await;
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

    /// The undo of either freeball goes back to the event choice.
    #[tokio::test]
    async fn undo_of_the_freeballs() {
        let mut ui = scouting().await;
        keys(&mut ui, "s-d7+x").await;
        let saved = saved_count(&ui);
        keys(&mut ui, "U").await;
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        assert_eq!(saved_count(&ui), saved - 1);
        assert!(event_row(&mut ui, EventTypeEnum::FB).is_some());
        keys(&mut ui, "x").await;
        assert_eq!(last_saved(&ui).0, "FB");
        assert_eq!(saved_count(&ui), saved);
        keys(&mut ui, "X").await;
        keys(&mut ui, "U").await;
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        assert_eq!(saved_count(&ui), saved);
        assert!(event_row(&mut ui, EventTypeEnum::OF).is_some());
    }

    #[tokio::test]
    async fn latest_events_show_the_freeballs() {
        let mut ui = scouting().await;
        keys(&mut ui, "s-d7+xX").await;
        let labels = current_labels();
        let lines = ui.lines();
        assert!(
            lines.iter().any(|l| l.contains(labels.domain.freeball)
                && !l.contains(labels.domain.opponent_freeball)),
            "{}",
            ui.screen()
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains(labels.domain.opponent_freeball)),
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
        keys(&mut ui, "s-b2+").await;
        let row = event_row(&mut ui, EventTypeEnum::OF).unwrap();
        assert!(row.ends_with("Shift-z"), "{row}");
        // the old key is gone
        keys(&mut ui, "X").await;
        assert_eq!(saved_count(&ui), 2);
        keys(&mut ui, "Z").await;
        assert_eq!(last_saved(&ui).0, "OF");
        let row = event_row(&mut ui, EventTypeEnum::FB).unwrap();
        assert!(row.ends_with('z'), "{row}");
        keys(&mut ui, "x").await;
        assert!(choosing_the_event(&mut ui), "{}", ui.screen());
        keys(&mut ui, "z").await;
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
        keys(&mut ui, "d7#x").await;
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
        keys(&mut ui, "x").await;
        assert_eq!(last_saved(&ui).0, "FB");
    }

    // --- the statistics -------------------------------------------------------------

    /// The match statistics list the freeballs: the ones given, the ones
    /// received and how many of these we exploited; team events, none by
    /// player.
    #[tokio::test]
    async fn match_stats_show_the_freeballs() {
        let mut ui = scouting().await;
        // their freeball exploited: our kill
        keys(&mut ui, "s-b2+Xa2#").await;
        // their freeball not exploited: they dig and attack again; then our
        // freeball, and their point
        keys(&mut ui, "s-b2+Xa2-d7+xS").await;
        let labels = current_labels();
        while !ui.screen().contains(labels.match_list) {
            ui.press(KeyCode::Esc).await;
        }
        ui.press(KeyCode::Char(' ')).await;
        // the stat filter, its last entry
        ui.type_keys(&[KeyCode::Tab, KeyCode::Tab, KeyCode::Tab, KeyCode::Up])
            .await;
        // the rows of the freeball table, label and value
        let rows = |ui: &mut Ui| {
            let names = [
                labels.domain.freeballs_given,
                labels.domain.freeballs_received,
                labels.domain.freeballs_exploited,
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
                [labels.domain.freeballs_given, "1"],
                [labels.domain.freeballs_received, "2"],
                [labels.domain.freeballs_exploited, "50%(1/2)"],
            ],
            "{}",
            ui.screen()
        );
        // with a player: none given, the others not shown
        ui.press(KeyCode::Tab).await;
        for (player, given) in [("Anna", "-"), ("Bea", "-")] {
            for _ in 0..8 {
                // the player selected: the highlighted one in the list
                let selected = ui
                    .lines()
                    .into_iter()
                    .skip_while(|l| !l.contains(&format!("┌{}", labels.domain.player)))
                    .find(|l| l.contains(">>"));
                match selected {
                    Some(p) if p.contains(player) => break,
                    _ => ui.press(KeyCode::Up).await,
                };
            }
            assert_eq!(
                rows(&mut ui),
                [[labels.domain.freeballs_given, given]],
                "{player}: {}",
                ui.screen()
            );
        }
    }
}
