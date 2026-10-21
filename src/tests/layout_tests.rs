//! The screens adapt to the size of the terminal: whatever the size, the
//! footer lists all the keys of the screen, and the scouting shows all the
//! entries to choose from, the point sequence only when they still fit.

#[cfg(test)]
mod tests {
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;
    use scout4all_core::localization::current_labels;

    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &[
            "Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina", "Ilaria",
        ],
        opponents: &["Cortona Volley"],
    }];

    /// From the smallest terminal the app is usable in to a large one: the
    /// default of the macOS Terminal is 80x24.
    const SIZES: [(u16, u16); 5] = [(60, 20), (80, 24), (100, 30), (120, 40), (200, 60)];

    /// Some rallies: the match list, the stats and the events have
    /// something to show.
    const SCOUTED: &str = "s#s#s=p7=s=p7#a2#";

    /// The keys typed, a char each: `\n` is Enter, `\x1b` Esc, `\x02` F2,
    /// `\t` Tab, `>` the right arrow.
    fn code(c: char) -> KeyCode {
        match c {
            '\n' => KeyCode::Enter,
            '\x1b' => KeyCode::Esc,
            '\x02' => KeyCode::F(2),
            '\t' => KeyCode::Tab,
            '>' => KeyCode::Right,
            c => KeyCode::Char(c),
        }
    }

    /// Every screen, with the keys reaching it from the team list and
    /// whether the match is already started (its first set).
    fn screens() -> Vec<(&'static str, bool, String)> {
        vec![
            ("team list", true, "".into()),
            ("new team", true, "n".into()),
            ("import team", true, "i".into()),
            ("settings", true, "\x02".into()),
            ("settings shortcuts", true, "\x02\t".into()),
            ("report an issue", true, "\x02i".into()),
            ("team details", true, "\n".into()),
            ("edit team", true, "\ne".into()),
            ("edit player", true, "\n\n".into()),
            ("new player", true, "\nn".into()),
            ("match list", true, "\nm".into()),
            ("new match", true, "\nmn".into()),
            ("edit match", true, "\nme".into()),
            ("export match", true, "\nms".into()),
            ("import match", true, "\nmi".into()),
            ("serving team", false, "\nm\n".into()),
            ("lineup", false, "\nm\n>\n".into()),
            ("scouting", true, format!("\nm\n{SCOUTED}")),
            ("scouting evaluation", true, format!("\nm\n{SCOUTED}s")),
            ("match stats", true, format!("\nm\n{SCOUTED}\x1b ")),
            ("set list", true, format!("\nm\n{SCOUTED}\x1bv")),
            ("set events", true, format!("\nm\n{SCOUTED}\x1bv\n")),
            ("set event form", true, format!("\nm\n{SCOUTED}\x1bv\n\n")),
        ]
    }

    async fn open(started: bool, keys: &str) -> Ui {
        let mut ui = match started {
            true => Ui::start_with_sets(TEAM).await,
            false => Ui::start(TEAM, &["folder"]).await,
        };
        for c in keys.chars() {
            ui.press(code(c)).await;
        }
        ui
    }

    #[tokio::test]
    async fn the_footer_lists_all_the_keys_at_every_size() {
        for (name, started, keys) in screens() {
            let mut ui = open(started, &keys).await;
            let entries = ui.footer_entries();
            assert!(!entries.is_empty(), "{name}: no keys\n{}", ui.screen());
            for (width, height) in SIZES {
                ui.resize(width, height);
                let screen = ui.screen();
                for (key, description) in &entries {
                    let entry = format!("{key} = {description}");
                    assert!(
                        screen.contains(&entry),
                        "{name} at {width}x{height}: \"{entry}\" not shown\n{screen}"
                    );
                }
            }
        }
    }

    /// The entries of the panel with the title, a line each, if it's
    /// shown: the text between its borders, down to its bottom border.
    fn panel(ui: &mut Ui, title: &str) -> Option<Vec<String>> {
        let lines: Vec<Vec<char>> = ui.lines().iter().map(|l| l.chars().collect()).collect();
        let top = format!("┌{title}");
        let (row, x) = lines.iter().enumerate().find_map(|(row, line)| {
            String::from_iter(line)
                .find(&top)
                .map(|byte| (row, String::from_iter(line)[..byte].chars().count()))
        })?;
        Some(
            lines[row + 1..]
                .iter()
                .take_while(|line| line.get(x) != Some(&'└'))
                .map(|line| {
                    String::from_iter(line[x + 1..].iter().take_while(|c| **c != '│'))
                        .trim()
                        .to_string()
                })
                .filter(|entry| !entry.is_empty())
                .collect(),
        )
    }

    fn shows_point_sequence(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().point_sequence)
    }

    /// After our serve error they serve, and our reception goes straight
    /// over: the longest list of events, 7 (their point or error, our
    /// fault, attack, dig or block, their freeball).
    const SEVEN_EVENTS: &str = "s=p7/";
    /// Carla, the middle blocker in the front row, attacks after the dig of
    /// Gina the libero: the tallest panel, 6 evaluations and 8 sets.
    const MIDDLE_BLOCKER_ATTACK: &str = "s-d7+a3";

    #[tokio::test]
    async fn all_the_events_are_listed_at_every_height() {
        let mut ui = open(true, &format!("\nm\n{SEVEN_EVENTS}")).await;
        for height in 20..=60 {
            ui.resize(100, height);
            let events = panel(&mut ui, current_labels().choose_the_event);
            assert_eq!(
                events.as_ref().map(Vec::len),
                Some(7),
                "at 100x{height}: {events:?}\n{}",
                ui.screen()
            );
        }
    }

    /// The evaluations and the sets are all listed from the height where
    /// they fit (with the footer, the score and the borders: 27), and
    /// whenever the point sequence is shown.
    #[tokio::test]
    async fn the_tallest_panel_is_listed_whenever_it_fits() {
        let mut ui = open(true, &format!("\nm\n{MIDDLE_BLOCKER_ATTACK}")).await;
        for height in 27..=60 {
            ui.resize(100, height);
            let evals = panel(&mut ui, current_labels().choose_the_evaluation);
            let sets = panel(&mut ui, current_labels().set_type);
            assert_eq!(
                (evals.as_ref().map(Vec::len), sets.as_ref().map(Vec::len)),
                (Some(6), Some(8)),
                "at 100x{height}: {evals:?} {sets:?}\n{}",
                ui.screen()
            );
        }
    }

    #[tokio::test]
    async fn the_point_sequence_is_shown_only_when_the_lists_still_fit() {
        for keys in [SEVEN_EVENTS, MIDDLE_BLOCKER_ATTACK] {
            let mut ui = open(true, &format!("\nm\n{keys}")).await;
            let shown: Vec<u16> = (20..=60)
                .filter(|height| {
                    ui.resize(100, *height);
                    shows_point_sequence(&mut ui)
                })
                .collect();
            // from a height on, and the same whatever is being chosen
            assert_eq!(shown, (31..=60).collect::<Vec<_>>(), "{keys}");
        }
    }

    #[tokio::test]
    async fn the_point_sequence_comes_back_when_the_terminal_grows() {
        let mut ui = open(true, "\nm\n").await;
        ui.resize(80, 24);
        assert!(!shows_point_sequence(&mut ui), "{}", ui.screen());
        ui.resize(120, 40);
        assert!(shows_point_sequence(&mut ui), "{}", ui.screen());
    }
}
