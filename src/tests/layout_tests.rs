//! The screens adapt to the size of the terminal: whatever the size, the
//! footer lists all the keys of the screen.

#[cfg(test)]
mod tests {
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;

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
}
