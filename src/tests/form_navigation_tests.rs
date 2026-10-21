//! The forms move between their fields as the other screens move between
//! rows: up and shift tab to the previous field, down and tab to the next
//! one. Left and right change the value of a field (a choice, a checkbox),
//! never the field, and the footers list all of them.

#[cfg(test)]
mod tests {
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;
    use scout4all_core::{
        localization::current_labels,
        shapes::enums::{FriendlyName, RoleEnum, TeamClassificationEnum},
    };

    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &["Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina"],
        opponents: &[],
    }];

    fn chars(s: &str) -> Vec<KeyCode> {
        s.chars().map(KeyCode::Char).collect()
    }

    /// The footer lists both keys of each direction, and the change.
    fn assert_form_footer(ui: &mut Ui) {
        let footer = ui.footer();
        for entry in [
            format!("⬆/⇧-⇥ = {}", current_labels().previous),
            format!("⬇/⇥ = {}", current_labels().next),
            format!("⬅/⮕ = {}", current_labels().change),
        ] {
            assert!(footer.contains(&entry), "{entry} not in {footer}");
        }
    }

    #[tokio::test]
    async fn new_match_moves_between_fields_up_and_down() {
        let mut ui = Ui::start(TEAM, &[]).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Char('n')])
            .await;
        assert_form_footer(&mut ui);
        ui.type_keys(&chars("Arezzo")).await;
        // left and right don't leave the opponent
        ui.type_keys(&[KeyCode::Right, KeyCode::Left]).await;
        ui.type_keys(&chars("x")).await;
        assert!(ui.screen().contains("Arezzox"), "{}", ui.screen());
        // down to the date and back up
        ui.press(KeyCode::Down).await;
        ui.type_keys(&chars("y")).await;
        assert!(!ui.screen().contains("Arezzoxy"), "{}", ui.screen());
        ui.press(KeyCode::Up).await;
        ui.type_keys(&chars("y")).await;
        assert!(ui.screen().contains("Arezzoxy"), "{}", ui.screen());
        // tab and shift tab as well
        ui.type_keys(&[KeyCode::Tab, KeyCode::BackTab]).await;
        ui.type_keys(&chars("z")).await;
        assert!(ui.screen().contains("Arezzoxyz"), "{}", ui.screen());
        // home changes with left and right
        assert!(ui.screen().contains("[ ]"), "{}", ui.screen());
        ui.type_keys(&[KeyCode::Down, KeyCode::Down, KeyCode::Right])
            .await;
        assert!(ui.screen().contains("[X]"), "{}", ui.screen());
    }

    #[tokio::test]
    async fn new_player_role_changes_with_left_and_right() {
        let mut ui = Ui::start(TEAM, &[]).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('n')]).await;
        assert_form_footer(&mut ui);
        ui.type_keys(&chars("Zoe")).await;
        ui.type_keys(&[KeyCode::Down, KeyCode::Right]).await;
        ui.type_keys(&[KeyCode::Down]).await;
        ui.type_keys(&chars("42")).await;
        ui.press(KeyCode::Enter).await;
        assert!(
            ui.screen().contains(current_labels().operation_successful),
            "{}",
            ui.screen()
        );
        // the message closes the form
        ui.press(KeyCode::Enter).await;
        let rows = ui.rows_containing(&["Zoe"]);
        assert_eq!(rows.len(), 1, "{}", ui.screen());
        assert!(
            rows[0].contains(&RoleEnum::ALL[1].to_string()),
            "{}",
            ui.screen()
        );
    }

    #[tokio::test]
    async fn edit_team_choice_changes_with_left_and_right() {
        let mut ui = Ui::start(TEAM, &[]).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('e')]).await;
        assert_form_footer(&mut ui);
        ui.press(KeyCode::Down).await;
        let first = TeamClassificationEnum::ALL[0].friendly_name(current_labels());
        let second = TeamClassificationEnum::ALL[1].friendly_name(current_labels());
        assert_eq!(ui.selected().as_deref(), Some(first));
        ui.press(KeyCode::Right).await;
        assert_eq!(ui.selected().as_deref(), Some(second));
        ui.press(KeyCode::Left).await;
        assert_eq!(ui.selected().as_deref(), Some(first));
    }

    #[tokio::test]
    async fn report_an_issue_moves_between_fields_up_and_down() {
        let mut ui = Ui::start(TEAM, &[]).await;
        ui.press(KeyCode::F(2)).await;
        ui.press(KeyCode::Char('i')).await;
        let footer = ui.footer();
        assert!(footer.contains("⬇/⇥"), "{footer}");
        ui.type_keys(&chars("Bug")).await;
        ui.type_keys(&[KeyCode::Right]).await;
        ui.type_keys(&chars("s")).await;
        assert!(ui.screen().contains("Bugs"), "{}", ui.screen());
        ui.type_keys(&[KeyCode::Down]).await;
        ui.type_keys(&chars("q")).await;
        assert!(!ui.screen().contains("Bugsq"), "{}", ui.screen());
    }
}
