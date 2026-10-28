//! The screens adapt to the size of the terminal: whatever the size, the
//! footer lists all the keys of the screen, the scouting shows all the
//! entries to choose from (the point sequence only when they still fit),
//! and the tables size their columns on their texts. The titles of the
//! panels look the same on every screen.

#[cfg(test)]
mod tests {
    use crate::localization::current_labels;
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;
    use ratatui::style::{Color, Modifier};

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
    /// `\x03` F3, `\t` Tab, `>` the right arrow.
    fn code(c: char) -> KeyCode {
        match c {
            '\n' => KeyCode::Enter,
            '\x1b' => KeyCode::Esc,
            '\x02' => KeyCode::F(2),
            '\x03' => KeyCode::F(3),
            '\t' => KeyCode::Tab,
            '>' => KeyCode::Right,
            c => KeyCode::Char(c),
        }
    }

    /// Every screen, with the keys reaching it from the team list and
    /// whether the match is already started (its first set).
    fn screens() -> Vec<(&'static str, bool, String)> {
        vec![
            (TERMS, false, "".into()),
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
            ("match roster", true, "\nme\n".into()),
            ("export match", true, "\nms".into()),
            ("import match", true, "\nmi".into()),
            ("serving team", false, "\nm\n".into()),
            ("lineup", false, "\nm\n>\n".into()),
            ("scouting", true, format!("\nm\n{SCOUTED}")),
            ("scouting evaluation", true, format!("\nm\n{SCOUTED}s")),
            ("live stats", true, format!("\nm\n{SCOUTED}\x03")),
            ("match stats", true, format!("\nm\n{SCOUTED}\x1b ")),
            ("set list", true, format!("\nm\n{SCOUTED}\x1bv")),
            ("set events", true, format!("\nm\n{SCOUTED}\x1bv\n")),
            ("set event form", true, format!("\nm\n{SCOUTED}\x1bv\n\n")),
        ]
    }

    /// The terms of use, shown at the first startup.
    const TERMS: &str = "terms of use";

    /// The screen of `screens()` with the name.
    async fn open_screen(name: &str, started: bool, keys: &str) -> Ui {
        match name {
            TERMS => Ui::first_startup(TEAM).await,
            _ => open(started, keys).await,
        }
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
            let mut ui = open_screen(name, started, &keys).await;
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

    /// The titles of the panels, the text on their top border after the
    /// corner, each with whether it's in bold and whether it's in yellow.
    fn titles(ui: &mut Ui) -> Vec<(String, bool, bool)> {
        let buffer = ui.render();
        let area = buffer.area;
        (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buffer[(x, y)].symbol() == "┌")
            .map(|(x, y)| {
                let cells: Vec<_> = (x + 1..area.width)
                    .map(|x| &buffer[(x, y)])
                    .take_while(|cell| !matches!(cell.symbol(), "─" | "┐"))
                    .collect();
                let letters: Vec<_> = cells
                    .iter()
                    .filter(|cell| !cell.symbol().trim().is_empty())
                    .collect();
                (
                    cells.iter().map(|cell| cell.symbol()).collect::<String>(),
                    letters
                        .iter()
                        .all(|cell| cell.modifier.contains(Modifier::BOLD)),
                    letters.iter().all(|cell| cell.fg == Color::Yellow),
                )
            })
            .filter(|(title, _, _)| !title.is_empty())
            .collect()
    }

    #[tokio::test]
    async fn every_panel_title_is_in_bold() {
        for (name, started, keys) in screens() {
            let mut ui = open_screen(name, started, &keys).await;
            for (title, bold, _) in titles(&mut ui) {
                assert!(bold, "{name}: \"{title}\" not in bold\n{}", ui.screen());
            }
        }
    }

    /// The panel naming the screen, the only one in yellow: the stats are
    /// a set of charts with nothing naming them, the import of a team
    /// opens on an empty folder.
    #[tokio::test]
    async fn every_screen_has_its_title_in_yellow() {
        let untitled = ["match stats", "import team"];
        for (name, started, keys) in screens() {
            let mut ui = open_screen(name, started, &keys).await;
            let yellow: Vec<String> = titles(&mut ui)
                .into_iter()
                .filter(|(_, _, yellow)| *yellow)
                .map(|(title, _, _)| title)
                .collect();
            let expected = match untitled.contains(&name) {
                true => 0,
                false => 1,
            };
            assert_eq!(
                yellow.len(),
                expected,
                "{name}: {yellow:?}\n{}",
                ui.screen()
            );
        }
    }

    /// The entries of the panel with the title, a line each, if it's
    /// shown: the text between its borders, down to its bottom border.
    fn panel(ui: &mut Ui, title: &str) -> Option<Vec<String>> {
        ui.panel(title)
    }

    fn shows_point_sequence(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().domain.point_sequence)
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

    /// The first row of the panel with the title.
    fn first_row(ui: &mut Ui, title: &str) -> String {
        let lines = ui.lines();
        lines
            .iter()
            .position(|l| l.contains(&format!("┌{title}")))
            .map(|row| lines[row + 1].clone())
            .unwrap_or_else(|| panic!("\"{title}\" not shown\n{}", ui.screen()))
    }

    #[tokio::test]
    async fn the_match_list_shows_the_score_the_date_and_the_status_in_full() {
        let mut ui = open(true, "\nm").await;
        for (width, height) in SIZES {
            ui.resize(width, height);
            let row = first_row(&mut ui, current_labels().match_list);
            for text in ["0-0", "Mon Sep 01, 2025", current_labels().in_progress] {
                assert!(row.contains(text), "{width}x{height}: \"{text}\" in {row}");
            }
        }
        // the teams in full when there's room for them, the score not far
        // from them on a wide screen
        for (width, height) in SIZES.into_iter().filter(|(width, _)| *width >= 80) {
            ui.resize(width, height);
            let row = first_row(&mut ui, current_labels().match_list);
            let (team, score) = (row.find("Cortona Volley"), row.find("0-0"));
            assert!(
                matches!((team, score), (Some(team), Some(score)) if score - team <= 31),
                "{width}x{height}: {row}"
            );
        }
    }

    #[tokio::test]
    async fn the_set_events_show_their_texts_in_full() {
        let mut ui = open(true, &format!("\nm\n{SCOUTED}\x1bv\n")).await;
        // the events of SCOUTED, a row each: number, event, player,
        // evaluation, serve or set
        let expected = [
            "1 serve Anna ace F",
            "2 serve Anna ace F",
            "3 serve Anna error F",
            "4 reception Gina error -",
            "5 reception Gina perfect -",
        ];
        for (width, height) in SIZES.into_iter().filter(|(width, _)| *width >= 80) {
            ui.resize(width, height);
            let rows: Vec<String> = panel(&mut ui, current_labels().events)
                .expect("expected the events")
                .iter()
                .map(|row| row.split_whitespace().collect::<Vec<_>>().join(" "))
                .collect();
            assert!(
                rows.starts_with(&expected.map(String::from)),
                "{width}x{height}: {rows:?}"
            );
        }
    }

    #[tokio::test]
    async fn the_substitutions_are_shown_in_full() {
        let substitutions = format!("{} 0/8", current_labels().domain.substitutions);
        for keys in ["\nm\n".to_string(), format!("\nm\n{SCOUTED}\x1bv\n")] {
            let mut ui = open(true, &keys).await;
            for (width, height) in SIZES {
                ui.resize(width, height);
                assert!(
                    ui.screen().contains(&substitutions),
                    "{keys:?} at {width}x{height}\n{}",
                    ui.screen()
                );
            }
        }
    }

    /// Wherever the footer lists the keys moving to the previous and the
    /// next row, field or panel, it lists tab and shift tab too, which move
    /// the same way, next to the arrows. Not on the settings, where tab
    /// switches panel, nor on the lineup, where it skips the fallback
    /// libero.
    #[tokio::test]
    async fn tab_and_shift_tab_are_listed_with_the_arrows() {
        let labels = current_labels();
        for (name, started, keys) in screens() {
            let mut ui = open_screen(name, started, &keys).await;
            let entries = ui.footer_entries();
            let key_of = |description: &str| {
                entries
                    .iter()
                    .find(|(_, d)| d == description)
                    .map(|(k, _)| k.clone())
            };
            let exception = matches!(name, "settings" | "settings shortcuts" | "lineup");
            match (key_of(labels.previous), key_of(labels.next), exception) {
                (_, _, true) | (None, None, _) => {}
                (Some(previous), Some(next), false) => {
                    let keys = |k: &str| k.split('/').map(str::to_string).collect::<Vec<_>>();
                    let (previous, next) = (keys(&previous), keys(&next));
                    assert!(
                        previous.contains(&"⇧-⇥".to_string()),
                        "{name}: {previous:?}"
                    );
                    assert!(next.contains(&"⇥".to_string()), "{name}: {next:?}");
                    assert_eq!((previous.len(), next.len()), (2, 2), "{name}: {entries:?}");
                }
                (previous, next, false) => {
                    panic!("{name}: previous {previous:?}, next {next:?}")
                }
            }
        }
    }
}
