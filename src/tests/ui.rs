//! A tiny harness driving the whole app like `run_app` does: real file system
//! providers over a temporary directory, key events sent to `App` and frames
//! rendered on ratatui's `TestBackend`, so tests assert on what users see.

use crate::{
    app::App,
    providers::{
        fs::{
            match_reader::FileSystemMatchReader, match_writer::FileSystemMatchWriter,
            set_reader::FileSystemSetReader, set_writer::FileSystemSetWriter,
            settings_reader::FileSystemSettingsReader, settings_writer::FileSystemSettingsWriter,
            team_reader::FileSystemTeamReader, team_writer::FileSystemTeamWriter,
        },
        match_writer::MatchWriter,
        team_reader::TeamReader,
        team_writer::{PlayerInput, TeamInput, TeamWriter},
    },
    shapes::{enums::RoleEnum, settings::Settings},
};
use chrono::{Duration, TimeZone, Utc};
use crokey::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Modifier};
use std::{path::PathBuf, sync::Arc};
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
    _dir: TempDir,
}

impl Ui {
    /// Starts the app on the team list. `browse_dirs` are created in the
    /// folder the file system screens open on.
    pub async fn start(teams: &[TeamFixture], browse_dirs: &[&str]) -> Self {
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
                match_writer
                    .create(&team, o.to_string(), date, true)
                    .await
                    .unwrap();
            }
        }

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
            _ => KeyModifiers::NONE,
        };
        let quit = self.app.handle_key(KeyEvent::new(code, modifiers)).await;
        self.render();
        quit.is_some()
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
