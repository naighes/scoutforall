//! The birth date, the height and the notes of a player, the notes of a
//! match: typed in the forms, saved, shown again when editing; every field
//! of the forms in sight at every size.

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

    const SIZES: [(u16, u16); 5] = [(60, 20), (80, 24), (100, 30), (120, 40), (200, 60)];

    fn chars(s: &str) -> Vec<KeyCode> {
        s.chars().map(KeyCode::Char).collect()
    }

    /// Tab, then `text`.
    async fn next(ui: &mut Ui, text: &str) {
        ui.press(KeyCode::Tab).await;
        ui.type_keys(&chars(text)).await;
    }

    fn player<'a>(team: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
        team["players"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == name)
            .unwrap()
    }

    #[tokio::test]
    async fn a_new_player_is_saved_with_birth_date_height_and_notes() {
        let mut ui = Ui::start(TEAM, &[]).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('n')]).await;
        ui.type_keys(&chars("Zoe")).await;
        ui.press(KeyCode::Tab).await;
        ui.press(KeyCode::Right).await;
        next(&mut ui, "12").await;
        next(&mut ui, "2001-12-03").await;
        next(&mut ui, "182").await;
        next(&mut ui, "captain").await;
        ui.press(KeyCode::Enter).await;
        let screen = ui.screen();
        let teams = ui.saved_teams();
        assert!(
            teams[0]["players"]
                .as_array()
                .unwrap()
                .iter()
                .any(|p| p["name"] == "Zoe"),
            "{screen}"
        );
        let zoe = player(&teams[0], "Zoe");
        assert_eq!(zoe["birth_date"], "2001-12-03", "{}", ui.screen());
        assert_eq!(zoe["height_cm"], 182);
        assert_eq!(zoe["notes"], "captain");
    }

    #[tokio::test]
    async fn a_birth_date_in_the_future_is_refused() {
        let mut ui = Ui::start(TEAM, &[]).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('n')]).await;
        ui.type_keys(&chars("Zoe")).await;
        ui.press(KeyCode::Tab).await;
        ui.press(KeyCode::Right).await;
        next(&mut ui, "12").await;
        next(&mut ui, "2999-01-01").await;
        ui.press(KeyCode::Enter).await;
        assert!(
            ui.screen()
                .contains(current_labels().domain.invalid_birth_date),
            "{}",
            ui.screen()
        );
        assert!(
            ui.saved_teams()[0]["players"]
                .as_array()
                .unwrap()
                .iter()
                .all(|p| p["name"] != "Zoe")
        );
    }

    #[tokio::test]
    async fn editing_a_player_shows_and_keeps_the_details() {
        let mut ui = Ui::start_editing(TEAM, |base| {
            let path = std::fs::read_dir(base)
                .unwrap()
                .map(|e| e.unwrap().path().join("team.json"))
                .find(|p| p.exists())
                .unwrap();
            let mut team: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            for p in team["players"].as_array_mut().unwrap() {
                p["birth_date"] = "2001-12-03".into();
                p["height_cm"] = 182.into();
                p["notes"] = "captain".into();
            }
            std::fs::write(&path, serde_json::to_vec_pretty(&team).unwrap()).unwrap();
        })
        .await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Enter]).await;
        let screen = ui.screen();
        let labels = current_labels();
        for shown in [
            format!("{}: 2001-12-03", labels.birth_date),
            format!("{}: 182", labels.height),
            format!("{}: captain", labels.notes),
        ] {
            assert!(screen.contains(&shown), "no {shown:?}\n{screen}");
        }
        ui.press(KeyCode::Enter).await;
        for p in ui.saved_teams()[0]["players"].as_array().unwrap() {
            assert_eq!(
                (&p["birth_date"], &p["height_cm"], &p["notes"]),
                (&"2001-12-03".into(), &182.into(), &"captain".into())
            );
        }
    }

    #[tokio::test]
    async fn the_notes_of_a_match_are_saved_and_shown_again() {
        let mut ui = Ui::start_with_sets(TEAM).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Char('e')])
            .await;
        for _ in 0..4 {
            ui.press(KeyCode::Tab).await;
        }
        ui.type_keys(&chars("windy")).await;
        // the form, then the roster as it is
        ui.type_keys(&[KeyCode::Enter, KeyCode::Enter]).await;
        assert_eq!(ui.saved_matches()[0]["notes"], "windy", "{}", ui.screen());
        // back on the match list
        ui.type_keys(&[KeyCode::Char('e')]).await;
        let shown = format!("{}: windy", current_labels().notes);
        assert!(ui.screen().contains(&shown), "{}", ui.screen());
    }

    #[tokio::test]
    async fn every_field_of_the_forms_is_in_sight_at_every_size() {
        let labels = current_labels();
        let forms: [(&str, &[KeyCode], Vec<&str>); 2] = [
            (
                "player",
                &[KeyCode::Enter, KeyCode::Char('n')],
                vec![
                    labels.name,
                    labels.role,
                    labels.number,
                    labels.birth_date,
                    labels.height,
                    labels.notes,
                ],
            ),
            (
                "match",
                &[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Char('n')],
                vec![
                    labels.opponent,
                    labels.date,
                    labels.home,
                    labels.max_substitutions,
                    labels.notes,
                ],
            ),
        ];
        for (form, keys, fields) in forms {
            let mut ui = Ui::start(TEAM, &[]).await;
            ui.type_keys(keys).await;
            for (width, height) in SIZES {
                ui.resize(width, height);
                let screen = ui.screen();
                for field in &fields {
                    // the date says its format before the colon
                    assert!(
                        screen.contains(&format!("{field}:"))
                            || screen.contains(&format!("{field} (")),
                        "{form} at {width}x{height}: no {field}\n{screen}"
                    );
                }
            }
        }
    }
}
