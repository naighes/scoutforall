//! A tiny harness driving the whole app like `run_app` does: real file system
//! providers over a temporary directory, key events sent to `App` and frames
//! rendered on ratatui's `TestBackend`, so tests assert on what users see.

use crate::{
    app::App,
    providers::{
        fs::{
            settings_reader::FileSystemSettingsReader, settings_writer::FileSystemSettingsWriter,
        },
        settings_reader::SettingsReader,
    },
    shapes::settings::Settings,
};
use chrono::{Duration, TimeZone, Utc};
use crokey::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Modifier};
use scout4all_core::{
    constants::DEFAULT_MAX_SUBSTITUTIONS,
    providers::{
        fs::{
            match_reader::FileSystemMatchReader, match_writer::FileSystemMatchWriter,
            set_reader::FileSystemSetReader, set_writer::FileSystemSetWriter,
            team_reader::FileSystemTeamReader, team_writer::FileSystemTeamWriter,
        },
        match_writer::MatchWriter,
        set_writer::SetWriter,
        team_reader::TeamReader,
        team_writer::{PlayerInput, TeamInput, TeamWriter},
    },
    shapes::enums::{RoleEnum, TeamSideEnum},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tempfile::TempDir;

const WIDTH: u16 = 120;
const HEIGHT: u16 = 40;

pub struct TeamFixture {
    pub name: &'static str,
    pub players: &'static [&'static str],
    pub opponents: &'static [&'static str],
}

pub struct Ui {
    app: App,
    terminal: Terminal<TestBackend>,
    base: PathBuf,
    _dir: TempDir,
}

impl Ui {
    /// Starts the app on the team list. `browse_dirs` are created in the
    /// folder the file system screens open on.
    pub async fn start(teams: &[TeamFixture], browse_dirs: &[&str]) -> Self {
        Self::start_with(teams, browse_dirs, false, |_| {}).await
    }

    /// Starts the app on the team list, with the first set of every match
    /// already started and serving: the first six players of the team are on
    /// court, the seventh is the libero.
    pub async fn start_with_sets(teams: &[TeamFixture]) -> Self {
        Self::start_with(teams, &[], true, |_| {}).await
    }

    /// Like `start_with_sets`, changing the saved files before the app
    /// starts, e.g. to damage them: `edit` gets the data folder.
    pub async fn start_with_files(teams: &[TeamFixture], edit: impl FnOnce(&Path)) -> Self {
        Self::start_with(teams, &[], true, edit).await
    }

    async fn start_with(
        teams: &[TeamFixture],
        browse_dirs: &[&str],
        start_sets: bool,
        edit: impl FnOnce(&Path),
    ) -> Self {
        let dir = TempDir::new().unwrap();
        let base = dir.path().join("data");
        std::fs::create_dir_all(&base).unwrap();
        let browse: PathBuf = dir.path().join("browse");
        for d in browse_dirs {
            std::fs::create_dir_all(browse.join(d)).unwrap();
        }

        let team_writer = FileSystemTeamWriter::new(&base);
        let match_writer = FileSystemMatchWriter::new(&base);
        let first_match = Utc
            .with_ymd_and_hms(2025, 9, 1, 18, 0, 0)
            .unwrap()
            .fixed_offset();
        for t in teams {
            let mut team = team_writer
                .save(TeamInput::New {
                    id: None,
                    name: t.name.to_string(),
                    year: 2025,
                    classification: None,
                    gender: None,
                    players: vec![],
                })
                .await
                .unwrap();
            for (i, p) in t.players.iter().enumerate() {
                let input = PlayerInput::New {
                    name: p.to_string(),
                    role: RoleEnum::OutsideHitter,
                    number: i as u8 + 1,
                };
                team_writer.save_player(input, &mut team).await.unwrap();
            }
            for (i, o) in t.opponents.iter().enumerate() {
                let date = first_match + Duration::days(i as i64);
                let m = match_writer
                    .create(&team, o.to_string(), date, true, DEFAULT_MAX_SUBSTITUTIONS)
                    .await
                    .unwrap();
                if start_sets {
                    let ids: Vec<_> = team.players.iter().map(|p| p.id).collect();
                    let positions: [_; 6] = ids[..6].try_into().unwrap();
                    FileSystemSetWriter::new(&base)
                        .create(
                            &m,
                            1,
                            TeamSideEnum::Us,
                            positions,
                            ids[6],
                            None,
                            positions[0],
                            vec![],
                        )
                        .await
                        .unwrap();
                }
            }
        }

        edit(&base);
        let team_reader = FileSystemTeamReader::new(&base);
        let teams = team_reader.read_all().await.unwrap();
        let settings = Settings {
            last_used_dir: Some(browse),
            ..Settings::default()
        };
        let app = App::new(
            settings,
            teams,
            base.clone(),
            Arc::new(team_reader),
            Arc::new(team_writer),
            Arc::new(FileSystemSettingsWriter::new(&base)),
            Arc::new(FileSystemSetWriter::new(&base)),
            Arc::new(FileSystemMatchReader::new(
                &base,
                Arc::new(FileSystemSetReader::new(&base)),
            )),
            Arc::new(match_writer),
            Arc::new(FileSystemSettingsReader::new(&base)),
        );
        let terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
        let mut ui = Self {
            app,
            terminal,
            base,
            _dir: dir,
        };
        ui.render();
        ui
    }

    pub fn render(&mut self) -> Buffer {
        let app = &mut self.app;
        self.terminal.draw(|f| app.draw(f)).unwrap().buffer.clone()
    }

    /// Sends a key, then renders like the main loop does. Returns true when
    /// the key made the app quit.
    pub async fn press(&mut self, code: KeyCode) -> bool {
        let modifiers = match code {
            KeyCode::BackTab => KeyModifiers::SHIFT,
            KeyCode::Char(c) if c.is_ascii_uppercase() => KeyModifiers::SHIFT,
            _ => KeyModifiers::NONE,
        };
        self.press_with(code, modifiers).await
    }

    pub async fn press_with(&mut self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        let quit = self.app.handle_key(KeyEvent::new(code, modifiers)).await;
        self.render();
        quit.is_some()
    }

    pub async fn type_keys(&mut self, codes: &[KeyCode]) {
        for code in codes {
            self.press(*code).await;
        }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.terminal.backend_mut().resize(width, height);
        self.render();
    }

    /// The settings file as saved on disk.
    pub async fn saved_settings_json(&self) -> serde_json::Value {
        let path = scout4all_core::providers::fs::path::get_config_file_path(&self.base);
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    /// The match descriptors as saved on disk.
    pub fn saved_matches(&self) -> Vec<serde_json::Value> {
        self.match_descriptor_paths()
            .iter()
            .map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap())
            .collect()
    }

    /// Changes the match descriptors on disk, e.g. to save them as an older
    /// version did.
    pub fn edit_saved_matches(&self, edit: impl Fn(&mut serde_json::Value)) {
        for path in self.match_descriptor_paths() {
            let mut value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            edit(&mut value);
            std::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        }
    }

    /// The descriptors of the first set of each match, as saved on disk.
    pub fn saved_first_sets(&self) -> Vec<serde_json::Value> {
        self.match_descriptor_paths()
            .iter()
            .filter_map(|p| std::fs::read_to_string(p.with_file_name("set_1.json")).ok())
            .map(|json| serde_json::from_str(&json).unwrap())
            .collect()
    }

    /// Changes the descriptors of the first sets on disk, e.g. to save them
    /// as an older version did.
    pub fn edit_saved_first_sets(&self, edit: impl Fn(&mut serde_json::Value)) {
        for path in self.match_descriptor_paths() {
            let path = path.with_file_name("set_1.json");
            let mut value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            edit(&mut value);
            std::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        }
    }

    /// The events saved for the first set of each match, as CSV rows.
    pub fn saved_first_set_events(&self) -> Vec<String> {
        self.match_descriptor_paths()
            .iter()
            .filter_map(|p| std::fs::read_to_string(p.with_file_name("set_1.csv")).ok())
            .flat_map(|csv| csv.lines().map(str::to_string).collect::<Vec<_>>())
            .collect()
    }

    fn match_descriptor_paths(&self) -> Vec<PathBuf> {
        let dirs = |p: &PathBuf| -> Vec<PathBuf> {
            std::fs::read_dir(p)
                .unwrap()
                .map(|e| e.unwrap().path())
                .filter(|p| p.is_dir())
                .collect()
        };
        dirs(&self.base)
            .iter()
            .flat_map(dirs)
            .map(|m| m.join(scout4all_core::constants::MATCH_DESCRIPTOR_FILE_NAME))
            .filter(|p| p.exists())
            .collect()
    }

    /// The settings as saved on disk; None until something saves them.
    pub async fn saved_settings(&self) -> Option<Settings> {
        FileSystemSettingsReader::new(&self.base).read().await.ok()
    }

    pub fn lines(&mut self) -> Vec<String> {
        let buffer = self.render();
        (0..buffer.area.height)
            .map(|y| row_text(&buffer, y))
            .collect()
    }

    /// The text of the selected list row: marked either by the `>>` highlight
    /// symbol or by reversed colors, depending on the screen.
    pub fn selected(&mut self) -> Option<String> {
        let buffer = self.render();
        (0..buffer.area.height)
            .find(|&y| {
                row_text(&buffer, y).contains(">> ")
                    || (0..buffer.area.width).any(|x| {
                        let cell = &buffer[(x, y)];
                        cell.modifier.contains(Modifier::REVERSED)
                            && !cell.symbol().trim().is_empty()
                    })
            })
            .map(|y| clean(&row_text(&buffer, y)))
    }

    /// The rows of the body, in display order, without borders.
    pub fn rows_containing(&mut self, needles: &[&str]) -> Vec<String> {
        self.lines()
            .iter()
            .map(|l| clean(l))
            .filter(|l| needles.iter().any(|n| l.contains(n)))
            .collect()
    }

    /// The keys the footer of the current screen lists, `(key, description)`.
    pub fn footer_entries(&mut self) -> Vec<(String, String)> {
        self.app
            .current_screen()
            .map(|screen| screen.footer_entries())
            .unwrap_or_default()
    }

    pub fn footer(&mut self) -> String {
        let lines = self.lines();
        lines[lines.len() - 5..].join("\n")
    }

    pub fn screen(&mut self) -> String {
        self.lines().join("\n")
    }
}

fn row_text(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn clean(line: &str) -> String {
    line.replace(">> ", "")
        .trim_matches(|c: char| c == '│' || c.is_whitespace())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
