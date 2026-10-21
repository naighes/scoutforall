//! The sequence of the points while scouting: a row per team below the
//! score, the home team on top, with the score reached by the team scoring
//! each point in its row, the latest points as many as fit.

#[cfg(test)]
mod tests {
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;
    use scout4all_core::localization::current_labels;

    // we play at home and serve: Gina the libero (7) receives
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

    /// The rows of the point sequence, the team name first: the two rows
    /// below its title.
    fn point_rows(ui: &mut Ui) -> (String, String) {
        let lines = ui.lines();
        let title = lines
            .iter()
            .position(|l| l.contains(current_labels().point_sequence))
            .expect("expected the point sequence");
        let row = |i: usize| lines[title + i].trim_matches(['│', ' ']).to_string();
        (row(1), row(2))
    }

    /// The points scored in a row, in order.
    fn scores(row: &str) -> Vec<u8> {
        row.split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect()
    }

    /// The column of each point scored in a row.
    fn columns(row: &str) -> Vec<usize> {
        row.char_indices()
            .filter(|(i, c)| {
                c.is_ascii_digit()
                    && (*i == 0 || !row[..*i].ends_with(|p: char| p.is_ascii_digit()))
            })
            .map(|(i, _)| i)
            .collect()
    }

    #[tokio::test]
    async fn no_points_before_the_first_rally() {
        let mut ui = scouting().await;
        let (home, away) = point_rows(&mut ui);
        assert_eq!(home, "Volley Club");
        assert_eq!(away, "Cortona Volley");
    }

    #[tokio::test]
    async fn the_points_are_in_the_row_of_the_team_scoring_them() {
        let mut ui = scouting().await;
        // our ace, our ace, our serve error, our reception error
        keys(&mut ui, "s#s#s=p7=").await;
        let (home, away) = point_rows(&mut ui);
        assert!(home.starts_with("Volley Club"), "{home}");
        assert!(away.starts_with("Cortona Volley"), "{away}");
        assert_eq!(scores(&home), [1, 2]);
        assert_eq!(scores(&away), [1, 2]);
        // a column per point, in order: ours first, then theirs
        let (home_columns, away_columns) = (columns(&home), columns(&away));
        assert!(home_columns[1] < away_columns[0], "{home}\n{away}");
        assert!(away_columns[0] < away_columns[1], "{home}\n{away}");
    }

    #[tokio::test]
    async fn undoing_the_point_removes_it() {
        let mut ui = scouting().await;
        keys(&mut ui, "s#s=").await;
        // back to the evaluation of their point, our serve error
        keys(&mut ui, "U").await;
        let (home, away) = point_rows(&mut ui);
        assert_eq!(scores(&home), [1], "{home}");
        assert_eq!(scores(&away), Vec::<u8>::new(), "{away}");
    }

    #[tokio::test]
    async fn only_the_latest_points_that_fit_are_shown() {
        let mut ui = scouting().await;
        keys(&mut ui, &"s#".repeat(20)).await;
        // a narrow terminal: room for some of the points only
        ui.resize(60, 40);
        let (home, _) = point_rows(&mut ui);
        let shown = scores(&home);
        let first = *shown.first().expect("expected some points");
        assert!(first > 1, "{home}");
        assert_eq!(shown, (first..=20).collect::<Vec<_>>(), "{home}");
    }

    #[tokio::test]
    async fn widening_the_terminal_shows_the_first_points_again() {
        let mut ui = scouting().await;
        keys(&mut ui, &"s#".repeat(20)).await;
        ui.resize(60, 40);
        let (home, _) = point_rows(&mut ui);
        assert!(!scores(&home).contains(&1), "{home}");
        // back to a wide terminal: room for all the points
        ui.resize(120, 40);
        let (home, _) = point_rows(&mut ui);
        assert_eq!(scores(&home), (1..=20).collect::<Vec<_>>(), "{home}");
    }
}
