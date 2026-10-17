//! Substitutions per set: 8 from the 2026 rules, 6 before. New matches ask
//! for the number (8 by default); the matches saved without it, or imported
//! from files saved without it, keep the old 6.

#[cfg(test)]
mod tests {
    use crate::{
        constants::{DEFAULT_MAX_SUBSTITUTIONS, LEGACY_MAX_SUBSTITUTIONS},
        localization::current_labels,
        providers::{
            fs::{
                match_reader::FileSystemMatchReader, match_writer::FileSystemMatchWriter,
                set_reader::FileSystemSetReader, set_writer::FileSystemSetWriter,
                team_writer::FileSystemTeamWriter,
            },
            match_reader::MatchReader,
            team_writer::{PlayerInput, TeamInput, TeamWriter},
        },
        screens::{file_system_screen::FileSystemAction, import_match_screen::ImportMatchAction},
        shapes::{
            enums::{EventTypeEnum, RoleEnum, TeamSideEnum},
            r#match::MatchEntry,
            set::SetEntry,
            snapshot::{EventEntry, Snapshot},
            team::TeamEntry,
        },
        tests::ui::{TeamFixture, Ui},
    };
    use chrono::Utc;
    use crokey::crossterm::event::KeyCode;
    use std::{io::Write, path::Path, sync::Arc};
    use tempfile::TempDir;
    use uuid::Uuid;
    use zip::{ZipWriter, write::SimpleFileOptions};

    /* the match descriptor */

    const LEGACY_DESCRIPTOR: &str =
        r#"{"opponent": "Cortona Volley", "date": "2025-09-01T18:00:00+02:00", "home": true}"#;

    #[test]
    fn match_saved_without_the_number_allows_6() {
        let m: MatchEntry = serde_json::from_str(LEGACY_DESCRIPTOR).unwrap();
        assert_eq!(m.max_substitutions, LEGACY_MAX_SUBSTITUTIONS);
        assert_eq!(LEGACY_MAX_SUBSTITUTIONS, 6);
    }

    #[test]
    fn match_saved_with_the_number() {
        let mut value: serde_json::Value = serde_json::from_str(LEGACY_DESCRIPTOR).unwrap();
        value["max_substitutions"] = 8.into();
        let m: MatchEntry = serde_json::from_value(value).unwrap();
        assert_eq!(m.max_substitutions, 8);
        let saved = serde_json::to_value(&m).unwrap();
        assert_eq!(saved["max_substitutions"], 8);
    }

    /* the lineup */

    /// Starting six, libero and four players on the bench.
    fn lineup(max_substitutions: u8) -> (Snapshot, [Uuid; 6], [Uuid; 4]) {
        let court: [Uuid; 6] = std::array::from_fn(|_| Uuid::new_v4());
        let bench: [Uuid; 4] = std::array::from_fn(|_| Uuid::new_v4());
        let set = SetEntry::new(1, TeamSideEnum::Them, court, Uuid::new_v4(), None, court[0])
            .expect("expected a valid set");
        let snapshot = Snapshot::new(&set, max_substitutions).expect("expected a snapshot");
        (snapshot, court, bench)
    }

    /// Four closed changes: the first four of the court out and back.
    fn substitutions() -> impl Fn(&[Uuid; 6], &[Uuid; 4]) -> Vec<(Uuid, Uuid)> {
        |court, bench| {
            (0..4)
                .flat_map(|i| [(court[i], bench[i]), (bench[i], court[i])])
                .collect()
        }
    }

    #[test]
    fn eight_substitutions_with_the_new_rules() {
        let (mut snapshot, court, bench) = lineup(DEFAULT_MAX_SUBSTITUTIONS);
        let lineup = &mut snapshot.current_lineup;
        assert_eq!(lineup.get_max_substitutions(), 8);
        for (i, (out, r#in)) in substitutions()(&court, &bench).iter().enumerate() {
            assert!(!lineup.was_max_number_of_substitutions_reached());
            lineup
                .add_substitution(out, r#in)
                .unwrap_or_else(|e| panic!("substitution {}: {e:?}", i + 1));
        }
        assert!(lineup.was_max_number_of_substitutions_reached());
        assert!(lineup.get_replaceable_lineup_choices().is_empty());
        assert!(lineup.add_substitution(&court[4], &Uuid::new_v4()).is_err());
    }

    #[test]
    fn six_substitutions_with_the_old_rules() {
        let (mut snapshot, court, bench) = lineup(LEGACY_MAX_SUBSTITUTIONS);
        let lineup = &mut snapshot.current_lineup;
        let subs = substitutions()(&court, &bench);
        for (out, r#in) in &subs[..6] {
            lineup
                .add_substitution(out, r#in)
                .expect("expected a change");
        }
        assert!(lineup.was_max_number_of_substitutions_reached());
        assert!(lineup.get_replaceable_lineup_choices().is_empty());
        let (out, r#in) = subs[6];
        assert!(lineup.add_substitution(&out, &r#in).is_err());
    }

    /// The sets of a match follow its number of substitutions.
    #[test]
    fn sets_follow_the_match() {
        let (_, court, bench) = lineup(DEFAULT_MAX_SUBSTITUTIONS);
        let mut set = SetEntry::new(1, TeamSideEnum::Them, court, Uuid::new_v4(), None, court[0])
            .expect("expected a valid set");
        set.events = substitutions()(&court, &bench)[..6]
            .iter()
            .map(|(out, r#in)| EventEntry {
                timestamp: Utc::now(),
                event_type: EventTypeEnum::R,
                player: Some(*out),
                eval: None,
                target_player: Some(*r#in),
            })
            .collect();
        let mut m: MatchEntry = serde_json::from_str(LEGACY_DESCRIPTOR).unwrap();
        m.sets = vec![set];
        let reached = |m: &MatchEntry| {
            let (snapshot, _) = m.get_set_snapshot(1).unwrap().unwrap();
            snapshot
                .current_lineup
                .was_max_number_of_substitutions_reached()
        };
        assert!(reached(&m));
        m.max_substitutions = DEFAULT_MAX_SUBSTITUTIONS;
        assert!(!reached(&m));
    }

    /* importing */

    async fn team(base: &Path) -> TeamEntry {
        let writer = FileSystemTeamWriter::new(base);
        let mut team = writer
            .save(TeamInput::New {
                id: None,
                name: "Volley Club".to_string(),
                year: 2025,
                classification: None,
                gender: None,
                players: vec![],
            })
            .await
            .unwrap();
        for i in 0..7 {
            let input = PlayerInput::New {
                name: format!("player {}", i + 1),
                role: RoleEnum::OutsideHitter,
                number: i + 1,
            };
            writer.save_player(input, &mut team).await.unwrap();
        }
        team
    }

    /// A match exported to `zip_path`, with `descriptor` as match.json.
    fn exported_match(zip_path: &Path, team: &TeamEntry, descriptor: &str) {
        let ids: Vec<Uuid> = team.players.iter().map(|p| p.id).collect();
        let positions: [Uuid; 6] = ids[..6].try_into().unwrap();
        let set =
            SetEntry::new(1, TeamSideEnum::Us, positions, ids[6], None, positions[0]).unwrap();
        let mut zip = ZipWriter::new(std::fs::File::create(zip_path).unwrap());
        for (name, content) in [
            ("match.json", descriptor.to_string()),
            ("set_1.json", serde_json::to_string(&set).unwrap()),
            ("set_1.csv", String::new()),
        ] {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    /// The name the export gives to the file.
    const EXPORTED: &str = "2025-09-01_cortona_volley.zip";

    async fn import(file_name: &str, descriptor: &str) -> MatchEntry {
        let dir = TempDir::new().unwrap();
        let base = dir.path().join("data");
        std::fs::create_dir_all(&base).unwrap();
        let team = team(&base).await;
        let zip_path = dir.path().join(file_name);
        exported_match(&zip_path, &team, descriptor);
        let reader = Arc::new(FileSystemMatchReader::new(
            &base,
            Arc::new(FileSystemSetReader::new(&base)),
        ));
        let mut action = ImportMatchAction::new(
            team.clone(),
            reader.clone(),
            Arc::new(FileSystemMatchWriter::new(&base)),
            Arc::new(FileSystemSetWriter::new(&base)),
        );
        action
            .on_selected(&zip_path)
            .await
            .expect("expected an import");
        let all = reader.read_all(&team).await.unwrap();
        assert_eq!(all.len(), 1);
        all.into_iter()
            .next()
            .unwrap()
            .expect("expected a readable match")
    }

    #[tokio::test]
    async fn imported_match_without_the_number_allows_6() {
        let m = import(EXPORTED, LEGACY_DESCRIPTOR).await;
        assert_eq!(m.max_substitutions, 6);
        let (snapshot, _) = m.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!(snapshot.current_lineup.get_max_substitutions(), 6);
    }

    #[tokio::test]
    async fn imported_match_keeps_its_number() {
        let mut value: serde_json::Value = serde_json::from_str(LEGACY_DESCRIPTOR).unwrap();
        value["max_substitutions"] = 8.into();
        let m = import(EXPORTED, &value.to_string()).await;
        assert_eq!(m.max_substitutions, 8);
    }

    /// The file was renamed after the export, e.g. downloaded twice: the
    /// sets are saved with the match, which keeps loading.
    #[tokio::test]
    async fn renamed_export_is_imported() {
        let m = import("2025-09-01_cortona_volley (1).zip", LEGACY_DESCRIPTOR).await;
        assert_eq!(m.id, "2025-09-01_cortona_volley");
        assert_eq!(m.sets.len(), 1);
    }

    /* new match screen */

    const NO_MATCHES: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &["Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina"],
        opponents: &[],
    }];

    fn chars(s: &str) -> Vec<KeyCode> {
        s.chars().map(KeyCode::Char).collect()
    }

    async fn new_match() -> Ui {
        let mut ui = Ui::start(NO_MATCHES, &[]).await;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Char('n')])
            .await;
        assert!(
            ui.screen().contains(current_labels().new_match),
            "{}",
            ui.screen()
        );
        ui
    }

    /// Fills the opponent and the date, then moves to the substitutions.
    async fn fill_new_match(ui: &mut Ui, opponent: &str) {
        ui.type_keys(&chars(opponent)).await;
        ui.press(KeyCode::Tab).await;
        ui.type_keys(&chars("20260920")).await;
        ui.type_keys(&[KeyCode::Tab, KeyCode::Tab]).await;
    }

    fn substitutions_field(n: &str) -> String {
        format!("{}: {}", current_labels().max_substitutions, n)
    }

    #[tokio::test]
    async fn new_match_allows_8_substitutions_by_default() {
        let mut ui = new_match().await;
        assert!(
            ui.screen().contains(&substitutions_field("8")),
            "{}",
            ui.screen()
        );
        fill_new_match(&mut ui, "Cortona Volley").await;
        ui.press(KeyCode::Enter).await;
        let saved = ui.saved_matches();
        assert_eq!(saved.len(), 1, "{}", ui.screen());
        assert_eq!(saved[0]["max_substitutions"], 8);
        assert_eq!(saved[0]["opponent"], "Cortona Volley");
    }

    #[tokio::test]
    async fn new_match_with_another_number_of_substitutions() {
        let mut ui = new_match().await;
        fill_new_match(&mut ui, "Cortona Volley").await;
        // digits only, two at most
        ui.type_keys(&[KeyCode::Backspace]).await;
        ui.type_keys(&chars("1x06")).await;
        assert!(
            ui.screen().contains(&substitutions_field("10")),
            "{}",
            ui.screen()
        );
        ui.press(KeyCode::Enter).await;
        let saved = ui.saved_matches();
        assert_eq!(saved.len(), 1, "{}", ui.screen());
        assert_eq!(saved[0]["max_substitutions"], 10);
    }

    #[tokio::test]
    async fn new_match_needs_some_substitutions() {
        for value in ["", "0"] {
            let mut ui = new_match().await;
            fill_new_match(&mut ui, "Cortona Volley").await;
            ui.press(KeyCode::Backspace).await;
            ui.type_keys(&chars(value)).await;
            ui.press(KeyCode::Enter).await;
            assert!(
                ui.screen()
                    .contains(current_labels().invalid_max_substitutions),
                "{value:?}: {}",
                ui.screen()
            );
            assert!(ui.saved_matches().is_empty());
        }
    }

    #[tokio::test]
    async fn new_match_needs_the_opponent() {
        let mut ui = new_match().await;
        fill_new_match(&mut ui, "  ").await;
        ui.press(KeyCode::Enter).await;
        assert!(
            ui.screen()
                .contains(current_labels().opponent_cannot_be_empty),
            "{}",
            ui.screen()
        );
        assert!(ui.saved_matches().is_empty());
    }

    /* scouting */

    /// Six on court, the libero and four on the bench.
    const SET_IN_PROGRESS: &[TeamFixture] = &[TeamFixture {
        name: "Volley Club",
        players: &[
            "Anna", "Bea", "Carla", "Dora", "Elsa", "Fede", "Gina", "Ilaria", "Laura", "Marta",
            "Nina",
        ],
        opponents: &["Cortona Volley"],
    }];

    async fn scouting(legacy: bool) -> Ui {
        let ui = Ui::start_with_sets(SET_IN_PROGRESS).await;
        if legacy {
            ui.edit_saved_matches(|m| {
                m.as_object_mut().unwrap().remove("max_substitutions");
            });
        }
        let mut ui = ui;
        ui.type_keys(&[KeyCode::Enter, KeyCode::Char('m'), KeyCode::Enter])
            .await;
        assert!(
            ui.screen().contains(current_labels().choose_the_event),
            "not scouting: {}",
            ui.screen()
        );
        ui
    }

    fn counter(used: u8, max: u8) -> String {
        format!("{} {}/{}", current_labels().substitutions, used, max)
    }

    /// The player at `replaced` in the lineup choices out, the first
    /// available replacement in.
    async fn substitute(ui: &mut Ui, replaced: char) {
        ui.type_keys(&[KeyCode::Char('r'), KeyCode::Char(replaced)])
            .await;
        assert!(
            ui.screen().contains(current_labels().select_replacement),
            "{}",
            ui.screen()
        );
        ui.press(KeyCode::Char('1')).await;
        assert!(
            ui.screen().contains(current_labels().choose_the_event),
            "{}",
            ui.screen()
        );
    }

    /// Setter, outside hitter and opposite out and back.
    async fn three_closed_changes(ui: &mut Ui) {
        for replaced in ['1', '1', '2', '2', '4', '4'] {
            substitute(ui, replaced).await;
        }
    }

    #[tokio::test]
    async fn substitutions_left_with_the_new_rules() {
        let mut ui = scouting(false).await;
        assert!(ui.screen().contains(&counter(0, 8)), "{}", ui.screen());
        three_closed_changes(&mut ui).await;
        assert!(ui.screen().contains(&counter(6, 8)), "{}", ui.screen());
        // a seventh: the other outside hitter
        substitute(&mut ui, '5').await;
        assert!(ui.screen().contains(&counter(7, 8)), "{}", ui.screen());
    }

    #[tokio::test]
    async fn no_substitutions_left_with_the_old_rules() {
        let mut ui = scouting(true).await;
        assert!(ui.screen().contains(&counter(0, 6)), "{}", ui.screen());
        three_closed_changes(&mut ui).await;
        assert!(ui.screen().contains(&counter(6, 6)), "{}", ui.screen());
        ui.press(KeyCode::Char('r')).await;
        let screen = ui.screen();
        assert!(
            screen.contains(current_labels().max_substitutions_reached),
            "{screen}"
        );
        assert!(
            !screen.contains(current_labels().player_selection),
            "{screen}"
        );
        // the banner goes away, still choosing the event
        ui.press(KeyCode::Char('r')).await;
        assert!(ui.screen().contains(current_labels().choose_the_event));
    }
}
