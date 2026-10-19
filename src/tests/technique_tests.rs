//! The set received by the attacker and the serve, chosen along with the
//! evaluation: a box lists the ones available, one is always chosen (the
//! preselected one until the scout presses another key), and the evaluation
//! saves it in the last column of the event.

#[cfg(test)]
mod tests {
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;
    use scout4all_core::{
        localization::current_labels,
        shapes::{
            enums::{EvalEnum, EventTypeEnum, WithDesc},
            technique::{ServeTypeEnum, SetTypeEnum},
        },
    };

    // the lineup: Anna the setter (1 in the player choice), Bea outside
    // hitter 1 (2), Carla middle blocker 2 (3), Dora the opposite (4), Elsa
    // outside hitter 2 (5), Fede middle blocker 1 (6), Gina the libero (7).
    // We serve, the setter in position 1: Bea, Carla and Dora in the front
    // row, Elsa in the back row.
    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &["Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina"],
        opponents: &["Cortona Volley"],
    }];

    async fn scouting() -> Ui {
        scouting_with(Ui::start_with_sets(TEAM).await).await
    }

    /// Opens the scouting screen on the set in progress.
    async fn scouting_with(mut ui: Ui) -> Ui {
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

    /// The rows of the box: "[X] a => A (high)" for the chosen one.
    fn box_rows(ui: &mut Ui) -> Vec<String> {
        ui.lines()
            .iter()
            .flat_map(|l| {
                l.split('│')
                    .map(str::trim)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|c| c.starts_with("[X]") || c.starts_with("[ ]"))
            .collect()
    }

    fn row(checked: bool, key: &str, description: &str) -> String {
        let check = match checked {
            true => "[X]",
            false => "[ ]",
        };
        format!("{check} {key} => {description}")
    }

    fn set_rows(sets: &[SetTypeEnum], chosen: SetTypeEnum) -> Vec<String> {
        sets.iter()
            .map(|set| {
                let (_, description) = set.with_desc();
                row(
                    *set == chosen,
                    &set.to_string().to_lowercase(),
                    &description,
                )
            })
            .collect()
    }

    /// The set or serve chosen in the box, if there's one.
    fn chosen(ui: &mut Ui) -> Option<String> {
        box_rows(ui).into_iter().find(|r| r.starts_with("[X]"))
    }

    fn assert_chosen(ui: &mut Ui, description: &str) {
        let chosen = chosen(ui).unwrap_or_else(|| panic!("nothing chosen: {}", ui.screen()));
        assert!(
            chosen.ends_with(&format!("=> {description}")),
            "{chosen}: {}",
            ui.screen()
        );
    }

    fn set_desc(set: SetTypeEnum) -> String {
        set.with_desc().1
    }

    fn serve_desc(serve: ServeTypeEnum) -> String {
        serve.with_desc().1
    }

    /// The technique column of the last event saved.
    fn last_technique(ui: &Ui) -> String {
        let events = ui.saved_first_set_events();
        let last = events.last().expect("no event saved");
        last.rsplit(',').next().unwrap().to_string()
    }

    fn is_choosing_the_evaluation(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().choose_the_evaluation)
    }

    // --- serves -----------------------------------------------------------

    #[tokio::test]
    async fn float_serve_preselected_and_saved() {
        let mut ui = scouting().await;
        keys(&mut ui, "s").await;
        assert!(
            ui.screen().contains(current_labels().serve_type),
            "{}",
            ui.screen()
        );
        assert_eq!(
            box_rows(&mut ui),
            vec![
                row(true, "f", &serve_desc(ServeTypeEnum::Float)),
                row(false, "s", &serve_desc(ServeTypeEnum::Spin)),
            ]
        );
        keys(&mut ui, "+").await;
        assert_eq!(last_technique(&ui), "F");
    }

    /// Pressing the chosen one again keeps it: one is always chosen.
    #[tokio::test]
    async fn spin_serve_chosen_and_kept() {
        let mut ui = scouting().await;
        keys(&mut ui, "ss").await;
        assert_chosen(&mut ui, &serve_desc(ServeTypeEnum::Spin));
        keys(&mut ui, "s").await;
        assert_chosen(&mut ui, &serve_desc(ServeTypeEnum::Spin));
        assert!(is_choosing_the_evaluation(&mut ui));
        keys(&mut ui, "f").await;
        assert_chosen(&mut ui, &serve_desc(ServeTypeEnum::Float));
        keys(&mut ui, "s+").await;
        assert_eq!(last_technique(&ui), "S");
    }

    /// The server serves again after an ace: the same serve is chosen.
    #[tokio::test]
    async fn serve_of_the_server_remembered() {
        let mut ui = scouting().await;
        keys(&mut ui, "ss#").await;
        assert_eq!(last_technique(&ui), "S");
        keys(&mut ui, "s").await;
        assert_chosen(&mut ui, &serve_desc(ServeTypeEnum::Spin));
    }

    // --- sets ---------------------------------------------------------------

    /// Each attacker gets the sets of their role and row, the one told by
    /// the touch before chosen: after a positive dig, 5 for the outside
    /// hitter in the front row, 6 for the opposite, 1 for the middle
    /// blocker, the pipe in the back row.
    #[tokio::test]
    async fn sets_of_each_attacker_after_a_positive_dig() {
        use SetTypeEnum::*;
        let cases = [
            ('2', vec![High, Five, Super, Nine, Quick], Five),
            ('4', vec![High, Six, Super, Quick], Six),
            ('3', vec![One, Two, Three, Four, C, B, Seven, Fast], One),
            ('5', vec![High, Pipe], Pipe),
        ];
        for (attacker, sets, preselected) in cases {
            let mut ui = scouting().await;
            // a negative serve, a positive dig of the libero
            keys(&mut ui, &format!("s-d7+a{attacker}")).await;
            assert!(is_choosing_the_evaluation(&mut ui), "{}", ui.screen());
            assert!(
                ui.screen().contains(current_labels().set_type),
                "{}",
                ui.screen()
            );
            assert_eq!(
                box_rows(&mut ui),
                set_rows(&sets, preselected),
                "attacker {attacker}"
            );
            keys(&mut ui, "#").await;
            assert_eq!(last_technique(&ui), preselected.to_string());
        }
    }

    /// The setter attacks on the second touch only: no sets to choose.
    #[tokio::test]
    async fn no_sets_for_the_setter() {
        let mut ui = scouting().await;
        keys(&mut ui, "s-d7+a1").await;
        assert!(is_choosing_the_evaluation(&mut ui), "{}", ui.screen());
        assert!(box_rows(&mut ui).is_empty(), "{}", ui.screen());
        assert!(
            !ui.screen().contains(current_labels().set_type),
            "{}",
            ui.screen()
        );
        keys(&mut ui, "#").await;
        assert_eq!(last_technique(&ui), "");
    }

    /// The touch before tells the set of the hitters in the front row.
    #[tokio::test]
    async fn sets_told_by_the_touch_before() {
        use SetTypeEnum::*;
        let cases = [
            // their serve after our serve error, our reception
            ("s=p7#", '2', Super),
            ("s=p7#", '4', Super),
            ("s=p7-", '2', High),
            ("s=p7!", '4', High),
            ("s=p7+", '4', Six),
            // a free ball: their reception comes straight back
            ("s/", '2', Super),
            ("s/", '4', Super),
            // our attack covered, then attacked again
            ("s-d7+a2!", '4', Super),
            ("s-d7+a2+", '2', Super),
        ];
        for (before, attacker, expected) in cases {
            let mut ui = scouting().await;
            keys(&mut ui, before).await;
            keys(&mut ui, &format!("a{attacker}")).await;
            assert!(
                is_choosing_the_evaluation(&mut ui),
                "{before}: {}",
                ui.screen()
            );
            assert_chosen(&mut ui, &set_desc(expected));
        }
    }

    /// The key of a set changes the choice and leaves the evaluation to
    /// come; the keys of the sets the attacker can't receive do nothing.
    #[tokio::test]
    async fn set_changed_with_its_key() {
        let mut ui = scouting().await;
        keys(&mut ui, "s-d7+a2q").await;
        assert_chosen(&mut ui, &set_desc(SetTypeEnum::Quick));
        assert!(is_choosing_the_evaluation(&mut ui));
        // the chosen one again
        keys(&mut ui, "q").await;
        assert_chosen(&mut ui, &set_desc(SetTypeEnum::Quick));
        // a set of the middle blockers, the pipe of the back row
        keys(&mut ui, "10").await;
        assert_chosen(&mut ui, &set_desc(SetTypeEnum::Quick));
        assert!(!ui.screen().contains(current_labels().player_selection));
        keys(&mut ui, "#").await;
        assert_eq!(last_technique(&ui), "Q");
    }

    /// The digits choose the players, but not along with the evaluation:
    /// there they're sets.
    #[tokio::test]
    async fn digit_sets_of_the_middle_blockers() {
        let mut ui = scouting().await;
        keys(&mut ui, "s-d7+a3").await;
        for (key, set) in [
            ('7', SetTypeEnum::Seven),
            ('c', SetTypeEnum::C),
            ('b', SetTypeEnum::B),
            ('f', SetTypeEnum::Fast),
            ('2', SetTypeEnum::Two),
        ] {
            keys(&mut ui, &key.to_string()).await;
            assert_chosen(&mut ui, &set_desc(set));
            assert!(is_choosing_the_evaluation(&mut ui), "{key}");
        }
        keys(&mut ui, "+").await;
        assert_eq!(last_technique(&ui), "2");
    }

    /// The undo brings back the set saved, not the preselected one; going
    /// back to the player choice forgets it.
    #[tokio::test]
    async fn undo_brings_back_the_set() {
        let mut ui = scouting().await;
        keys(&mut ui, "s-d7+a2q#").await;
        assert!(!is_choosing_the_evaluation(&mut ui));
        keys(&mut ui, "u").await;
        assert!(is_choosing_the_evaluation(&mut ui), "{}", ui.screen());
        assert_chosen(&mut ui, &set_desc(SetTypeEnum::Quick));
        // back to the player, then the same attacker again
        keys(&mut ui, "u2").await;
        assert_chosen(&mut ui, &set_desc(SetTypeEnum::Five));
        // another attacker
        keys(&mut ui, "u4").await;
        assert_chosen(&mut ui, &set_desc(SetTypeEnum::Six));
    }

    #[tokio::test]
    async fn undo_brings_back_the_serve() {
        let mut ui = scouting().await;
        keys(&mut ui, "ss+").await;
        keys(&mut ui, "u").await;
        assert!(is_choosing_the_evaluation(&mut ui), "{}", ui.screen());
        assert_chosen(&mut ui, &serve_desc(ServeTypeEnum::Spin));
        // back to the events, then the serve again: nothing to remember
        keys(&mut ui, "us").await;
        assert_chosen(&mut ui, &serve_desc(ServeTypeEnum::Float));
    }

    /// The latest events show the set or the serve after the evaluation.
    #[tokio::test]
    async fn latest_events_show_the_technique() {
        let mut ui = scouting().await;
        keys(&mut ui, "s-d7+a2q+").await;
        let labels = current_labels();
        let attack = ui
            .lines()
            .into_iter()
            .find(|l| l.contains(labels.attack) && l.contains("Bea"))
            .unwrap_or_else(|| panic!("{}", ui.screen()));
        let positive = EvalEnum::Positive.friendly_name(EventTypeEnum::A, labels);
        assert!(attack.contains(&format!("{positive} Q")), "{attack}");
        let serve = ui
            .lines()
            .into_iter()
            .find(|l| l.contains(labels.serve))
            .unwrap_or_else(|| panic!("{}", ui.screen()));
        assert!(serve.contains(" F"), "{serve}");
    }

    // --- key bindings ---------------------------------------------------------

    /// From the scouting, changes the key of the shortcut at `index` of the
    /// shortcuts list and goes back.
    async fn bind_from_scouting(ui: &mut Ui, index: usize, key: KeyCode) {
        ui.press(KeyCode::F(2)).await;
        for _ in 0..index {
            ui.press(KeyCode::Down).await;
        }
        ui.type_keys(&[KeyCode::Enter, key, KeyCode::Enter]).await;
        ui.press(KeyCode::Esc).await;
        assert!(
            ui.screen().contains(current_labels().choose_the_event),
            "not back to scouting: {}",
            ui.screen()
        );
    }

    fn set_index(set: SetTypeEnum) -> usize {
        EventTypeEnum::ALL.len()
            + EvalEnum::ALL.len()
            + SetTypeEnum::ALL.iter().position(|s| *s == set).unwrap()
    }

    fn serve_index(serve: ServeTypeEnum) -> usize {
        set_index(*SetTypeEnum::ALL.last().unwrap())
            + 1
            + ServeTypeEnum::ALL.iter().position(|s| *s == serve).unwrap()
    }

    #[tokio::test]
    async fn rebound_set_key_while_scouting() {
        let mut ui = scouting().await;
        bind_from_scouting(&mut ui, set_index(SetTypeEnum::High), KeyCode::Char('x')).await;
        keys(&mut ui, "s-d7+a2").await;
        assert!(
            box_rows(&mut ui).contains(&row(false, "x", &set_desc(SetTypeEnum::High))),
            "{}",
            ui.screen()
        );
        // the previous key is gone
        keys(&mut ui, "a").await;
        assert_chosen(&mut ui, &set_desc(SetTypeEnum::Five));
        keys(&mut ui, "x").await;
        assert_chosen(&mut ui, &set_desc(SetTypeEnum::High));
        keys(&mut ui, "-").await;
        assert_eq!(last_technique(&ui), "A");
    }

    #[tokio::test]
    async fn rebound_serve_key_while_scouting() {
        let mut ui = scouting().await;
        bind_from_scouting(
            &mut ui,
            serve_index(ServeTypeEnum::Spin),
            KeyCode::Char('z'),
        )
        .await;
        keys(&mut ui, "s").await;
        assert!(
            box_rows(&mut ui).contains(&row(false, "z", &serve_desc(ServeTypeEnum::Spin))),
            "{}",
            ui.screen()
        );
        keys(&mut ui, "z+").await;
        assert_eq!(last_technique(&ui), "S");
    }

    // --- sets scouted up to v0.4.3 ------------------------------------------

    /// A set scouted up to v0.4.3 goes on: its rows get the technique
    /// column (empty), so that v0.4.3 still reads the file, and the undo of
    /// an event without technique chooses the preselected one.
    #[tokio::test]
    async fn set_scouted_up_to_v0_4_3_goes_on() {
        let ui = Ui::start_with_files(TEAM, |base| {
            let set = std::fs::read_dir(base)
                .unwrap()
                .map(|e| e.unwrap().path())
                .filter(|p| p.is_dir())
                .flat_map(|team| std::fs::read_dir(team).unwrap().map(|e| e.unwrap().path()))
                .find(|p| p.is_dir())
                .unwrap();
            let descriptor: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(set.join("set_1.json")).unwrap())
                    .unwrap();
            let setter = descriptor["setter"].as_str().unwrap();
            std::fs::write(
                set.join("set_1.csv"),
                format!("2025-09-01T18:00:00Z,S,{setter},-,\n"),
            )
            .unwrap();
        })
        .await;
        let mut ui = scouting_with(ui).await;
        // the dig of the libero after the negative serve
        keys(&mut ui, "d7+").await;
        let events = ui.saved_first_set_events();
        assert_eq!(events.len(), 2, "{events:?}");
        for event in &events {
            assert_eq!(event.split(',').count(), 6, "{event}");
        }
        assert!(events[0].ends_with(",-,,"), "{events:?}");
        // undo the dig (back to its evaluation, its player, the events),
        // then the serve saved without technique: it gets the float
        keys(&mut ui, "uuuu").await;
        assert!(is_choosing_the_evaluation(&mut ui), "{}", ui.screen());
        assert_chosen(&mut ui, &serve_desc(ServeTypeEnum::Float));
        keys(&mut ui, "-").await;
        assert_eq!(last_technique(&ui), "F");
    }
}
