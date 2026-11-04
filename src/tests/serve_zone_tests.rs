//! Where our serve went, told along with its evaluation on the court of
//! the opponent, and where we received theirs, on ours: the arrows move
//! the cursor, the space bar chooses the zone or takes it back; never
//! required. Saved in the flags of the event.

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
            ui.screen().contains(&court_title("8C-3D")) && !ui.screen().contains("(3/8)"),
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
            ui.screen().contains(&court_title("8C-3D")) && !ui.screen().contains("(3/8)"),
            "{}",
            ui.screen()
        );
    }

    /// Their serve: where we received it, told along with the evaluation
    /// of the reception on our court, seen from the bench (the net at the
    /// top): the arrows move as shown, from the middle (8A) up to 8B, left
    /// to 8C; two cells of 8, saved.
    #[tokio::test]
    async fn the_reception_is_told_on_our_court() {
        let mut ui = serving().await;
        ui.press(KeyCode::Char('=')).await;
        ui.type_keys(&[KeyCode::Char('p'), KeyCode::Char('7')])
            .await;
        let title = |zone: &str| format!("{}: {zone}", current_labels().reception_target);
        assert!(
            ui.screen().contains(&title(current_labels().no_zone)),
            "{}",
            ui.screen()
        );
        ui.type_keys(&[KeyCode::Up, KeyCode::Char(' ')]).await;
        assert!(ui.screen().contains(&title("8B")), "{}", ui.screen());
        ui.type_keys(&[KeyCode::Left, KeyCode::Char(' ')]).await;
        assert!(ui.screen().contains(&title("8B-8C")), "{}", ui.screen());
        // our court: 4 at the net on the left, 5 at our end line
        let screen = ui.screen();
        let (at_4, at_5) = (screen.find("4C"), screen.find("5D"));
        assert!(at_4 < at_5, "{screen}");
        ui.press(KeyCode::Char('+')).await;
        // the pair across in the fourth row, third column: 46 + 3 * 5 + 2,
        // in bits 3 to 9
        assert_eq!(last_flags(&ui), (63 * 8).to_string());
    }

    /// The stats of the reception: how many were received in each zone of
    /// ours, beside the side outs by possession; and on the conflict zones,
    /// the line between 8 and 9 here (8A and 9D side by side).
    #[tokio::test]
    async fn the_reception_stats_show_the_zones() {
        use KeyCode::*;
        let mut ui = serving().await;
        ui.press(Char('=')).await;
        ui.type_keys(&[Char('p'), Char('7'), Char(' '), Right, Char(' ')])
            .await;
        let title = format!("{}: 9D-8A (8/9)", current_labels().reception_target);
        assert!(ui.screen().contains(&title), "{}", ui.screen());
        ui.press(Char('+')).await;
        ui.resize(120, 40);
        ui.type_keys(&[Esc, Char(' '), Tab, Tab, Tab, Down]).await;
        let domain = current_labels().domain;
        let told = domain
            .zones_told
            .replace("{told}", "1")
            .replace("{total}", "1");
        let conflicts = domain
            .on_conflicts
            .replace("{count}", "1")
            .replace("{share}", "100");
        let density = ui
            .panel(domain.receptions_by_zone)
            .unwrap_or_else(|| panic!("{}", ui.screen()));
        // how many told, at the bottom
        assert!(ui.screen().contains(&told), "{}", ui.screen());
        assert!(
            ui.screen().contains(&format!("{told} · {conflicts}")),
            "{}",
            ui.screen()
        );
        // half in 8, half in 9, all on 8/9
        assert!(
            density
                .iter()
                .any(|l| l.contains("50%") && l.contains("100%")),
            "{density:?}"
        );
        assert!(density.iter().any(|l| l.contains("8/9")), "{density:?}");
    }

    /// Our attack: where it went, told along with its evaluation on their
    /// court; in the stats beside the distribution.
    #[tokio::test]
    async fn the_attack_is_told_on_their_court() {
        use KeyCode::*;
        let mut ui = serving().await;
        ui.press(Char('=')).await;
        ui.type_keys(&[Char('p'), Char('7'), Char('+'), Char('a'), Char('4')])
            .await;
        let title = |zone: &str| format!("{}: {zone}", current_labels().attack_target);
        assert!(
            ui.screen().contains(&title(current_labels().no_zone)),
            "{}",
            ui.screen()
        );
        // from 8A up to 6B
        ui.type_keys(&[Up, Char(' ')]).await;
        assert!(ui.screen().contains(&title("6B")), "{}", ui.screen());
        ui.press(Char('#')).await;
        assert_eq!(last_flags(&ui), ((10 + 5 * 4 + 1) * 8).to_string());
        ui.resize(160, 50);
        ui.type_keys(&[Esc, Char(' '), Tab, Tab, Tab, Down, Down, Down, Down])
            .await;
        let domain = current_labels().domain;
        let told = domain
            .zones_told
            .replace("{told}", "1")
            .replace("{total}", "1");
        let density = ui
            .panel(domain.attacks_by_zone)
            .unwrap_or_else(|| panic!("{}", ui.screen()));
        // how many told, at the bottom
        assert!(ui.screen().contains(&told), "{}", ui.screen());
        assert!(density.iter().any(|l| l.contains("100%")), "{density:?}");
        assert!(ui.panel(domain.distribution).is_some(), "{}", ui.screen());
    }

    /// Our dig: where we dug their attack, on our court; in the stats with
    /// the conflict zones, as the reception.
    #[tokio::test]
    async fn the_dig_is_told_on_our_court() {
        use KeyCode::*;
        let mut ui = serving().await;
        ui.type_keys(&[Char('+'), Char('d'), Char('7')]).await;
        let title = |zone: &str| format!("{}: {zone}", current_labels().dig_target);
        assert!(
            ui.screen().contains(&title(current_labels().no_zone)),
            "{}",
            ui.screen()
        );
        ui.type_keys(&[Char(' '), Right, Char(' ')]).await;
        assert!(
            ui.screen().contains(&title("9D-8A (8/9)")),
            "{}",
            ui.screen()
        );
        ui.press(Char('+')).await;
        ui.resize(120, 40);
        ui.type_keys(&[Esc, Char(' '), Tab, Tab, Tab, Down, Down])
            .await;
        let domain = current_labels().domain;
        let density = ui
            .panel(domain.digs_by_zone)
            .unwrap_or_else(|| panic!("{}", ui.screen()));
        assert!(density.iter().any(|l| l.contains("8/9")), "{density:?}");
        let conflicts = domain
            .on_conflicts
            .replace("{count}", "1")
            .replace("{share}", "100");
        assert!(ui.screen().contains(&conflicts), "{}", ui.screen());
    }

    /// Our block: only the zones at the net, 4 3 2 from the left on our
    /// court, the cursor in the middle; the space bar tells it, again
    /// takes it back. In the stats, the blocks by zone at the net.
    #[tokio::test]
    async fn the_block_is_told_at_the_net() {
        use KeyCode::*;
        let mut ui = serving().await;
        ui.type_keys(&[Char('+'), Char('b'), Char('2')]).await;
        let title = |zone: &str| format!("{}: {zone}", current_labels().block_target);
        assert!(
            ui.screen().contains(&title(current_labels().no_zone)),
            "{}",
            ui.screen()
        );
        ui.type_keys(&[Char(' ')]).await;
        assert!(ui.screen().contains(&title("3")), "{}", ui.screen());
        ui.type_keys(&[Char(' ')]).await;
        assert!(
            ui.screen().contains(&title(current_labels().no_zone)),
            "{}",
            ui.screen()
        );
        // right of 3 on our court, then on to the end: 2
        ui.type_keys(&[Right, Right, Down, Char(' ')]).await;
        assert!(ui.screen().contains(&title("2")), "{}", ui.screen());
        // only across at the net
        assert!(ui.screen().contains("⬅⮕ ="), "{}", ui.screen());
        ui.press(Char('+')).await;
        // the zone 2 in the flags: (2) << 3
        assert_eq!(last_flags(&ui), (2 * 8).to_string());
        ui.resize(120, 40);
        ui.type_keys(&[Esc, Char(' '), Tab, Tab, Tab, Down, Down, Down])
            .await;
        let density = ui
            .panel(current_labels().domain.blocks_by_zone)
            .unwrap_or_else(|| panic!("{}", ui.screen()));
        assert!(density.iter().any(|l| l.contains("100%")), "{density:?}");
        assert!(!density.iter().any(|l| l.contains('8')), "{density:?}");
    }

    /// The stats of the serve: how many went to each zone and their
    /// efficiency there, of the serves whose zone was told, two cells of
    /// two zones half in each; the efficiency only from three serves on.
    #[tokio::test]
    async fn the_serve_stats_show_the_zones() {
        use KeyCode::*;
        let mut ui = serving().await;
        // 4A; 3D and 4A, on the line between 3 and 4
        let to_4 = [Down, Down, Right, Right, Char(' ')];
        let to_43 = [Down, Down, Right, Char(' '), Right, Char(' ')];
        // aces to 4 (twice) and to 4/3 (half to 4, half to 3), one without
        // a zone, an error to 4
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
            .panel(domain.serves_by_zone)
            .unwrap_or_else(|| panic!("{}", ui.screen()));
        // how many told, at the bottom
        assert!(ui.screen().contains(&told), "{}", ui.screen());
        assert!(
            density
                .iter()
                .any(|l| l.contains("12%") && l.contains("88%")),
            "{density:?}"
        );
        assert!(
            density
                .iter()
                .any(|l| l.contains("(0.5)") && l.contains("(3.5)")),
            "{density:?}"
        );
        let efficiency = ui.panel(domain.efficiency_by_zone).unwrap();
        // (2.5 aces - 1 error) / 3.5; 3 has too few
        assert!(
            efficiency
                .iter()
                .any(|l| l.contains("(0.5)") && l.contains("n=3.5")),
            "{efficiency:?}"
        );
        assert!(
            efficiency.iter().any(|l| l.contains("+43%")),
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
