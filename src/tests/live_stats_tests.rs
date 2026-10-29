//! The statistics while scouting, opened and closed with F3: the figures of
//! the set, of another set or of the match, from the core, colored by the
//! band of the level of the team.

#[cfg(test)]
mod tests {
    use crate::localization::current_labels;
    use crate::tests::ui::{TeamFixture, Ui};
    use crokey::crossterm::event::KeyCode;
    use ratatui::style::Color;
    use scout4all_core::shapes::enums::{
        EventTypeEnum, FriendlyName, GenderEnum, PhaseEnum, TeamClassificationEnum,
    };
    use std::path::Path;

    const TEAM: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &["Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina"],
        opponents: &["Cortona Volley"],
    }];

    /// Our two aces (2-0) and serve error (2-1); a perfect reception of
    /// Gina the libero and Bea's kill from zone 2 (3-1); our serve error
    /// (3-2) and their ace on Gina (3-3).
    const RALLIES: &str = "s#s#s=p7#a2#s=p7=";

    async fn keys(ui: &mut Ui, keys: &str) {
        for c in keys.chars() {
            ui.press(KeyCode::Char(c)).await;
        }
    }

    /// The scouting of the rallies, from a team saved by `edit`.
    async fn scouted_with(rallies: &str, edit: impl FnOnce(&Path)) -> Ui {
        let mut ui = Ui::start_with_files(TEAM, edit).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Enter])
            .await;
        keys(&mut ui, rallies).await;
        assert!(is_scouting(&mut ui), "{}", ui.screen());
        ui
    }

    async fn scouted(rallies: &str) -> Ui {
        scouted_with(rallies, |_| {}).await
    }

    /// The team a men's regional one.
    fn regional_men(base: &Path) {
        let team = std::fs::read_dir(base)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.is_dir())
            .unwrap()
            .join("team.json");
        let mut json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&team).unwrap()).unwrap();
        json["classification"] = serde_json::to_value(TeamClassificationEnum::Regional).unwrap();
        json["gender"] = serde_json::to_value(GenderEnum::Men).unwrap();
        std::fs::write(&team, json.to_string()).unwrap();
    }

    fn is_scouting(ui: &mut Ui) -> bool {
        ui.screen().contains(current_labels().choose_the_event)
    }

    fn is_stats(ui: &mut Ui) -> bool {
        ui.screen()
            .contains(&format!("┌{} ◂", current_labels().live_stats))
    }

    /// The words of the row of the panel starting with `name`.
    fn row(ui: &mut Ui, panel: &str, name: &str) -> Vec<String> {
        let rows = ui
            .panel(panel)
            .unwrap_or_else(|| panic!("no {panel}: {}", ui.screen()));
        let row = rows
            .iter()
            .find(|r| r.starts_with(name))
            .unwrap_or_else(|| panic!("no {name} in {panel}: {rows:?}"));
        row[name.len()..]
            .split_whitespace()
            .map(str::to_string)
            .collect()
    }

    fn fundamentals(ui: &mut Ui, event: EventTypeEnum) -> Vec<String> {
        let title = format!("{} ◂", current_labels().live_stats);
        row(ui, &title, event.friendly_name(current_labels().domain))
    }

    fn points(ui: &mut Ui, name: &str) -> Vec<String> {
        row(ui, current_labels().points_and_errors, name)
    }

    /// The color of the first cell showing `text` on the row with `name`.
    fn color(ui: &mut Ui, name: &str, text: &str) -> Option<Color> {
        let buffer = ui.render();
        let lines = ui.lines();
        lines.iter().enumerate().find_map(|(y, line)| {
            let chars: Vec<char> = line.chars().collect();
            let start = line.find(name)?;
            let from = line[..start].chars().count();
            let x = (from..chars.len())
                .find(|x| chars[*x..].iter().collect::<String>().starts_with(text))?;
            Some(buffer[(x as u16, y as u16)].fg)
        })
    }

    #[tokio::test]
    async fn f3_opens_and_closes_the_stats() {
        let mut ui = scouted(RALLIES).await;
        ui.press(KeyCode::F(3)).await;
        assert!(is_stats(&mut ui), "{}", ui.screen());
        ui.press(KeyCode::F(3)).await;
        assert!(is_scouting(&mut ui), "{}", ui.screen());
        ui.press(KeyCode::F(3)).await;
        ui.press(KeyCode::Esc).await;
        assert!(is_scouting(&mut ui), "{}", ui.screen());
        // the scouting goes on where it was: their serve, our reception
        keys(&mut ui, "p7#a2#").await;
        assert!(is_scouting(&mut ui), "{}", ui.screen());
        ui.press(KeyCode::F(3)).await;
        assert_eq!(fundamentals(&mut ui, EventTypeEnum::P)[0], "3");
    }

    /// The figures of the rallies, counted by hand.
    #[tokio::test]
    async fn the_figures_of_the_set() {
        use EventTypeEnum::*;
        let mut ui = scouted(RALLIES).await;
        ui.press(KeyCode::F(3)).await;
        let labels = current_labels();
        let domain = labels.domain;
        // tot, #, errors, efficiency
        assert_eq!(fundamentals(&mut ui, P), ["2", "1", "1", "0%"]);
        assert_eq!(fundamentals(&mut ui, A), ["1", "1", "0", "100%"]);
        // (# - =) / tot: (2 - 2) / 4
        assert_eq!(fundamentals(&mut ui, S), ["4", "2", "2", "0%"]);
        assert_eq!(fundamentals(&mut ui, B), ["0", "0", "0", "-"]);
        let title = format!("{} ◂", labels.live_stats);
        assert_eq!(
            row(&mut ui, &title, domain.after_reception),
            ["1", "1", "0", "100%"]
        );
        assert_eq!(
            row(&mut ui, &title, domain.counter_attack),
            ["0", "0", "0", "-"]
        );
        // side out: 1 of the 2 rallies they served; break: 2 of our 4
        assert_eq!(
            points(&mut ui, PhaseEnum::SideOut.friendly_name(domain)),
            ["50%", "1/2"]
        );
        assert_eq!(
            points(&mut ui, PhaseEnum::Break.friendly_name(domain)),
            ["50%", "2/4"]
        );
        // both on the first ball: one won, one lost
        assert_eq!(
            points(&mut ui, &domain.possessions.join("·")),
            ["50%", "-", "·", "-"]
        );
        assert_eq!(points(&mut ui, labels.our_aces), ["2"]);
        // the reception error is their ace
        assert_eq!(points(&mut ui, labels.their_aces), ["1"]);
        let serve = S.friendly_name(domain);
        assert_eq!(
            points(&mut ui, labels.unforced_errors).join(" "),
            format!("2 {serve} 2")
        );
        let reception = P.friendly_name(domain);
        assert_eq!(
            points(&mut ui, labels.forced_errors).join(" "),
            format!("1 {reception} 1")
        );
        assert_eq!(points(&mut ui, domain.opponent_errors), ["0"]);
        // Bea's kill from zone 2, after reception: all of the attacks
        assert_eq!(
            row(&mut ui, domain.distribution, "2"),
            ["100%", "1", "100%", "100%", "-"]
        );
        assert_eq!(
            row(&mut ui, domain.distribution, "4"),
            ["-", "0", "-", "-", "-"]
        );
        // the trend: the set, the match and the last touches
        let trend =
            |ui: &mut Ui, event: EventTypeEnum| row(ui, labels.trend, event.friendly_name(domain));
        assert_eq!(trend(&mut ui, S), ["0%", "0%", "0%"]);
        assert_eq!(trend(&mut ui, P), ["0%", "0%", "0%"]);
    }

    /// The match statistics show the side out by possession with the
    /// reception, not with a player selected.
    #[tokio::test]
    async fn match_stats_show_the_side_out_by_possession() {
        let mut ui = scouted(RALLIES).await;
        let labels = current_labels();
        let domain = labels.domain;
        while !ui.screen().contains(labels.match_list) {
            ui.press(KeyCode::Esc).await;
        }
        ui.press(KeyCode::Char(' ')).await;
        // the stat filter, its second entry: the reception
        ui.type_keys(&[KeyCode::Tab, KeyCode::Tab, KeyCode::Tab, KeyCode::Down])
            .await;
        let [first, second, later] = domain.possessions;
        assert_eq!(
            row(&mut ui, domain.sideout_by_possession, first),
            ["2", "1", "50%"]
        );
        assert_eq!(
            row(&mut ui, domain.sideout_by_possession, second),
            ["-", "-", "-"]
        );
        assert_eq!(
            row(&mut ui, domain.sideout_by_possession, later),
            ["-", "-", "-"]
        );
        // the serve: none
        ui.press(KeyCode::Up).await;
        assert_eq!(ui.panel(domain.sideout_by_possession), None);
        // a player: none
        ui.type_keys(&[KeyCode::Down, KeyCode::Tab, KeyCode::Down])
            .await;
        assert_eq!(
            ui.panel(domain.sideout_by_possession),
            None,
            "{}",
            ui.screen()
        );
    }

    /// Left and right go through the sets and the match, the set being
    /// played first.
    #[tokio::test]
    async fn the_set_or_the_match() {
        let mut ui = scouted(RALLIES).await;
        ui.press(KeyCode::F(3)).await;
        let labels = current_labels();
        let title = |scope: &str| format!("┌{} ◂ {scope} ▸", labels.live_stats);
        assert!(ui.screen().contains(&title("set 1")), "{}", ui.screen());
        ui.press(KeyCode::Right).await;
        assert!(
            ui.screen().contains(&title(labels.match_word)),
            "{}",
            ui.screen()
        );
        // the match is the only set so far: the same figures
        assert_eq!(
            fundamentals(&mut ui, EventTypeEnum::S),
            ["4", "2", "2", "0%"]
        );
        ui.press(KeyCode::Right).await;
        assert!(ui.screen().contains(&title("set 1")), "{}", ui.screen());
        ui.press(KeyCode::Left).await;
        assert!(
            ui.screen().contains(&title(labels.match_word)),
            "{}",
            ui.screen()
        );
        ui.press(KeyCode::Tab).await;
        assert!(ui.screen().contains(&title("set 1")), "{}", ui.screen());
    }

    /// The figures by the band of a men's regional team: red below it,
    /// yellow in it, green above it.
    #[tokio::test]
    async fn the_colors_of_the_level() {
        let mut ui = scouted_with(RALLIES, regional_men).await;
        ui.press(KeyCode::F(3)).await;
        let domain = current_labels().domain;
        let name = |e: EventTypeEnum| e.friendly_name(domain);
        // attack 100%, band 3-13
        assert_eq!(
            color(&mut ui, name(EventTypeEnum::A), "100%"),
            Some(Color::Green)
        );
        // serve 0%, band -7 to 11
        assert_eq!(
            color(&mut ui, name(EventTypeEnum::S), "0%"),
            Some(Color::Yellow)
        );
        // reception 0%: band 5-22, below it by less than half of it
        assert_eq!(
            color(&mut ui, name(EventTypeEnum::P), "0%"),
            Some(Color::Red)
        );
        // side out 50%, band 61-73
        let side_out = PhaseEnum::SideOut.friendly_name(domain);
        assert_eq!(color(&mut ui, side_out, "50%"), Some(Color::Red));
        // break point 50%, band 27-39
        let break_point = PhaseEnum::Break.friendly_name(domain);
        assert_eq!(color(&mut ui, break_point, "50%"), Some(Color::Green));
        assert!(
            !ui.screen().contains(current_labels().set_team_level),
            "{}",
            ui.screen()
        );
    }

    /// Without the level and the gender of the team no colors, and a hint
    /// to set them.
    #[tokio::test]
    async fn no_colors_without_the_level() {
        let mut ui = scouted(RALLIES).await;
        ui.resize(200, 60);
        ui.press(KeyCode::F(3)).await;
        let attack = EventTypeEnum::A.friendly_name(current_labels().domain);
        assert_eq!(color(&mut ui, attack, "100%"), Some(Color::Reset));
        assert!(
            ui.screen().contains(current_labels().set_team_level),
            "{}",
            ui.screen()
        );
    }
}
