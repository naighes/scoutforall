//! Where our serve went, told along with its evaluation on the court of
//! the opponent: the arrows move the cursor, the space bar chooses the zone
//! or takes it back; never required. Saved in the flags of the serve.

#[cfg(test)]
mod tests {
    use crate::localization::current_labels;
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::{KeyCode, KeyModifiers};

    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &["Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina"],
        opponents: &["Cortona Volley"],
    }];

    /// Scouting, our serve to evaluate.
    async fn serving() -> Ui {
        let mut ui = Ui::start_with_sets(TEAM).await;
        ui.type_keys(&[
            KeyCode::Enter,
            KeyCode::Char('m'),
            KeyCode::Enter,
            KeyCode::Char('s'),
        ])
        .await;
        ui
    }

    /// The title of the court: the zone told, if any.
    fn court_title(zone: &str) -> String {
        format!("{}: {zone}", current_labels().serve_target)
    }

    fn shows_the_court(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().serve_target)
    }

    /// The flags column of the last event saved.
    fn last_flags(ui: &Ui) -> String {
        let events = ui.saved_first_set_events();
        let last = events.last().expect("no event saved");
        last.split(',').nth(6).unwrap_or_default().to_string()
    }

    #[tokio::test]
    async fn the_court_is_shown_with_no_zone_told() {
        let mut ui = serving().await;
        assert!(
            ui.screen().contains(&court_title(current_labels().no_zone)),
            "{}",
            ui.screen()
        );
        let footer = ui.footer_entries();
        let labels = current_labels();
        assert!(
            footer.iter().any(|(_, d)| d == labels.target_zone),
            "{footer:?}"
        );
        assert!(
            footer.iter().any(|(_, d)| d == labels.choose_zone),
            "{footer:?}"
        );
    }

    /// Not required: the evaluation saves the serve without a zone.
    #[tokio::test]
    async fn the_serve_is_saved_without_a_zone() {
        let mut ui = serving().await;
        ui.press(KeyCode::Char('+')).await;
        assert_eq!(last_flags(&ui), "");
        assert!(!shows_the_court(&mut ui), "{}", ui.screen());
    }

    /// From the middle (8A): left to 9D, chosen, then 8A next to it: two
    /// cells across 9 and 8, on the line between them (8/9), saved.
    #[tokio::test]
    async fn two_cells_across_two_zones_are_saved() {
        let mut ui = serving().await;
        ui.press(KeyCode::Left).await;
        assert!(ui.screen().contains(&court_title(current_labels().no_zone)));
        ui.press(KeyCode::Char(' ')).await;
        assert!(ui.screen().contains(&court_title("9D")), "{}", ui.screen());
        ui.type_keys(&[KeyCode::Right, KeyCode::Char(' ')]).await;
        assert!(
            ui.screen().contains(&court_title("9D-8A (8/9)")),
            "{}",
            ui.screen()
        );
        let footer = ui.footer_entries();
        assert!(
            footer
                .iter()
                .any(|(_, d)| d == current_labels().remove_zone),
            "{footer:?}"
        );
        // moving on doesn't change it
        ui.press(KeyCode::Down).await;
        assert!(ui.screen().contains(&court_title("9D-8A (8/9)")));
        ui.press(KeyCode::Char('+')).await;
        // the pair across in the third row, second column: 46 + 2 * 5 + 1,
        // in bits 3 to 9
        assert_eq!(last_flags(&ui), (57 * 8).to_string());
    }

    /// Two cells of a zone are the zone; the space bar on a cell chosen
    /// takes it back.
    #[tokio::test]
    async fn the_space_bar_takes_a_cell_back() {
        let mut ui = serving().await;
        ui.press(KeyCode::Char(' ')).await;
        assert!(ui.screen().contains(&court_title("8A")));
        ui.type_keys(&[KeyCode::Right, KeyCode::Char(' ')]).await;
        assert!(
            ui.screen().contains(&court_title("8A-8D")),
            "{}",
            ui.screen()
        );
        ui.press(KeyCode::Char(' ')).await;
        assert!(ui.screen().contains(&court_title("8A")), "{}", ui.screen());
        ui.type_keys(&[KeyCode::Left, KeyCode::Char(' ')]).await;
        assert!(ui.screen().contains(&court_title(current_labels().no_zone)));
        ui.press(KeyCode::Char('-')).await;
        assert_eq!(last_flags(&ui), "");
    }

    /// A cell on the corner of the one chosen isn't side by side: it's
    /// chosen alone.
    #[tokio::test]
    async fn a_cell_on_the_corner_starts_over() {
        let mut ui = serving().await;
        ui.press(KeyCode::Char(' ')).await;
        ui.type_keys(&[KeyCode::Right, KeyCode::Down, KeyCode::Char(' ')])
            .await;
        assert!(ui.screen().contains(&court_title("8C")), "{}", ui.screen());
    }

    /// Undo resumes the serve with its cells; the next serve starts without
    /// any, the cursor in the middle.
    #[tokio::test]
    async fn undo_resumes_the_serve_with_its_cells() {
        let mut ui = serving().await;
        ui.type_keys(&[KeyCode::Down, KeyCode::Down, KeyCode::Right])
            .await;
        ui.press(KeyCode::Char(' ')).await;
        assert!(ui.screen().contains(&court_title("3D")), "{}", ui.screen());
        ui.type_keys(&[KeyCode::Up, KeyCode::Char(' ')]).await;
        assert!(
            ui.screen().contains(&court_title("8C-3D (3/8)")),
            "{}",
            ui.screen()
        );
        ui.press(KeyCode::Char('#')).await;
        // the pair down from the fourth row, fourth column: 46 + 30 + 3 *
        // 6 + 3
        assert_eq!(last_flags(&ui), (97 * 8).to_string());
        ui.press(KeyCode::Char('s')).await;
        assert!(ui.screen().contains(&court_title(current_labels().no_zone)));
        ui.press(KeyCode::Char(' ')).await;
        assert!(ui.screen().contains(&court_title("8A")));
        // back to the events, then the ace again
        ui.press_with(KeyCode::Char('U'), KeyModifiers::SHIFT).await;
        ui.press_with(KeyCode::Char('U'), KeyModifiers::SHIFT).await;
        assert!(
            ui.screen().contains(&court_title("8C-3D (3/8)")),
            "{}",
            ui.screen()
        );
    }

    /// Their serve: no court for our reception, the arrows do nothing.
    #[tokio::test]
    async fn no_court_for_the_reception() {
        let mut ui = serving().await;
        ui.press(KeyCode::Char('=')).await;
        ui.type_keys(&[KeyCode::Char('p'), KeyCode::Char('7')])
            .await;
        assert!(
            ui.screen().contains(current_labels().choose_the_evaluation),
            "{}",
            ui.screen()
        );
        assert!(!shows_the_court(&mut ui));
        ui.type_keys(&[KeyCode::Up, KeyCode::Char(' '), KeyCode::Char('+')])
            .await;
        assert_eq!(last_flags(&ui), "");
    }

    /// The stats of the serve: how many went to each zone and their
    /// efficiency there, of the serves whose zone was told; the efficiency
    /// only from three serves on.
    #[tokio::test]
    async fn the_serve_stats_show_the_zones() {
        use KeyCode::*;
        let mut ui = serving().await;
        // 4A; 3D and 4A, on the line between 3 and 4
        let to_4 = [Down, Down, Right, Right, Char(' ')];
        let to_43 = [Down, Down, Right, Char(' '), Right, Char(' ')];
        // aces to 4 (twice) and to 4/3, one without a zone, an error to 4
        ui.type_keys(&[&to_4[..], &[Char('#'), Char('s')]].concat())
            .await;
        ui.type_keys(&[&to_4[..], &[Char('#'), Char('s')]].concat())
            .await;
        ui.type_keys(&[&to_43[..], &[Char('#'), Char('s')]].concat())
            .await;
        ui.type_keys(&[Char('#'), Char('s')]).await;
        ui.type_keys(&[&to_4[..], &[Char('=')]].concat()).await;
        ui.resize(120, 40);
        // the stats of the match, the serve chosen
        ui.type_keys(&[Esc, Char(' '), Tab, Tab, Tab]).await;
        let domain = current_labels().domain;
        let told = domain
            .zones_told
            .replace("{told}", "4")
            .replace("{total}", "5");
        let density = ui
            .panel(&format!("{} · {told}", domain.serves_by_zone))
            .unwrap_or_else(|| panic!("{}", ui.screen()));
        assert!(
            density
                .iter()
                .any(|l| l.contains("75%") && l.contains("25%")),
            "{density:?}"
        );
        assert!(
            density
                .iter()
                .any(|l| l.contains("(3)") && l.contains("(1)")),
            "{density:?}"
        );
        let efficiency = ui.panel(domain.efficiency_by_zone).unwrap();
        // (2 aces - 1 error) / 3; 4/3 has too few
        assert!(
            efficiency
                .iter()
                .any(|l| l.contains("+33%") && l.contains("(1)")),
            "{efficiency:?}"
        );
        assert!(
            efficiency.iter().any(|l| l.contains("n=3")),
            "{efficiency:?}"
        );
        // green, a positive efficiency
        let buffer = ui.render();
        let green = (0..buffer.area.height).any(|y| {
            (0..buffer.area.width).any(|x| {
                buffer[(x, y)].symbol() == "+"
                    && buffer[(x, y)].bg == ratatui::style::Color::Indexed(71)
            })
        });
        assert!(green, "{}", ui.screen());
    }
}
