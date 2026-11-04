//! The archives, as the app and the CLI make them: a team or a match
//! exported to a zip saved where the user chooses; a zip chosen to import,
//! shown before it's imported. A DataVolley file goes to its own screen
//! (`dvw`). The files are chosen with the dialogs of the system.

use {
    crate::{
        strings::Strings,
        team::day,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, column, container, row, text},
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{ArchivePreview, MobileError, Scout4All},
    std::{path::PathBuf, sync::Arc},
};

/// What is exported.
#[derive(Debug, Clone)]
pub enum Export {
    Team(String),
    /// the team, the match
    Match(String, String),
    /// the report of the match as a PDF, as the TUI prints it
    Report(String, String),
}

/// A file chosen to import: its name and what it holds.
#[derive(Clone)]
pub struct Picked {
    pub name: String,
    pub bytes: Arc<Vec<u8>>,
}

impl std::fmt::Debug for Picked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Picked({}, {} bytes)", self.name, self.bytes.len())
    }
}

/// Whether the file is a DataVolley one: by its name, as the TUI and the
/// app.
pub fn is_dvw(name: &str) -> bool {
    name.to_lowercase().ends_with(".dvw")
}

/// The file of `export` (a zip, or the PDF of the report) and the name to
/// give it.
pub fn archive(core: &Scout4All, export: &Export) -> Result<(String, Vec<u8>), MobileError> {
    let archive = match export {
        Export::Team(team) => core.export_team(team.clone())?,
        Export::Match(team, m) => core.export_match(team.clone(), m.clone())?,
        Export::Report(team, m) => {
            return Ok((format!("{m}.pdf"), core.match_pdf(team.clone(), m.clone())?));
        }
    };
    Ok((archive.file_name, archive.bytes))
}

/// Where to save the file named `name`, of its kind (zip, pdf): none when
/// the user changes their mind.
pub async fn save_to(name: String) -> Option<PathBuf> {
    let extension = name.rsplit_once('.').map_or("zip", |(_, e)| e).to_string();
    rfd::AsyncFileDialog::new()
        .set_file_name(name)
        .add_filter(&extension, &[&extension])
        .save_file()
        .await
        .map(|f| f.path().to_path_buf())
}

/// The file to import: an archive or, with `dvw`, a DataVolley file too.
pub async fn pick(filter: &'static str, dvw: bool) -> Option<Picked> {
    let extensions: &[&str] = match dvw {
        true => &["zip", "dvw", "DVW"],
        false => &["zip"],
    };
    let file = rfd::AsyncFileDialog::new()
        .add_filter(filter, extensions)
        .pick_file()
        .await?;
    Some(Picked {
        name: file.file_name(),
        bytes: Arc::new(file.read().await),
    })
}

#[derive(Debug, Clone)]
pub enum Message {
    Team(usize),
    Import,
    Cancel,
}

pub enum Outcome {
    Stay,
    Cancel,
    Import,
}

/// What was imported: the team, or the match and its team.
pub enum Imported {
    Team(String),
    Match(String, String),
}

/// An archive chosen, shown before it's imported: the team a match goes
/// into is chosen among those of its players.
pub struct Importing {
    picked: Picked,
    /// the team the match goes into, when imported from one
    team_id: Option<String>,
    preview: ArchivePreview,
    team: usize,
}

impl Importing {
    pub fn new(
        core: &Scout4All,
        picked: Picked,
        team_id: Option<String>,
    ) -> Result<Importing, MobileError> {
        let preview = core.preview_archive(
            picked.bytes.to_vec(),
            Some(picked.name.clone()),
            team_id.clone(),
        )?;
        Ok(Importing {
            picked,
            team_id,
            preview,
            team: 0,
        })
    }

    fn teams(&self) -> &[scout4all_mobile::TeamChoice] {
        match &self.preview {
            ArchivePreview::Match { teams, .. } => teams,
            ArchivePreview::Team { .. } => &[],
        }
    }

    /// Imports it, once confirmed.
    pub fn import(&self, core: &Scout4All) -> Result<Imported, MobileError> {
        let bytes = self.picked.bytes.to_vec();
        match &self.preview {
            ArchivePreview::Team { .. } => core
                .import_team(bytes, Some(self.picked.name.clone()))
                .map(Imported::Team),
            ArchivePreview::Match { .. } => {
                let team = self
                    .team_id
                    .clone()
                    .or_else(|| self.teams().get(self.team).map(|t| t.id.clone()))
                    .ok_or_else(|| MobileError::Failed {
                        reason: self.picked.name.clone(),
                    })?;
                core.import_match(bytes, team.clone())
                    .map(|m| Imported::Match(team, m))
            }
        }
    }

    /// Whether the team is to choose: a match outside of a team, of the
    /// players of more than one.
    fn choosing(&self) -> bool {
        self.team_id.is_none() && self.teams().len() > 1
    }

    pub fn key(&mut self, ck: KeyCombination) -> Outcome {
        let count = self.teams().len();
        match ck {
            k if k == BACK_KEY => Outcome::Cancel,
            k if k == key!(enter) => Outcome::Import,
            k if k == key!(down) && count > 0 => {
                self.team = (self.team + 1) % count;
                Outcome::Stay
            }
            k if k == key!(up) && count > 0 => {
                self.team = (self.team + count - 1) % count;
                Outcome::Stay
            }
            _ => Outcome::Stay,
        }
    }

    pub fn update(&mut self, message: Message) -> Outcome {
        match message {
            Message::Team(i) => {
                self.team = i.min(self.teams().len().saturating_sub(1));
                Outcome::Stay
            }
            Message::Import => Outcome::Import,
            Message::Cancel => Outcome::Cancel,
        }
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        let choose = match self.choosing() {
            true => vec![("↑ ↓".to_string(), s.into_team.to_string())],
            false => vec![],
        };
        [
            choose,
            vec![
                ("Enter".to_string(), s.import_archive.to_string()),
                ("Esc".to_string(), s.cancel.to_string()),
            ],
        ]
        .concat()
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let bold = theme::fonts().bold;
        let (title, name, line) = match &self.preview {
            ArchivePreview::Team {
                name,
                year,
                players,
            } => (
                s.import_team_title,
                name.clone(),
                format!("{year} · {players} {}", s.players),
            ),
            ArchivePreview::Match {
                opponent,
                home,
                date,
                won,
                lost,
                ..
            } => (
                s.import_match_title,
                opponent.clone(),
                format!(
                    "{} · {} · {won}–{lost}",
                    day(date),
                    match home {
                        true => s.home_game,
                        false => s.away_game,
                    }
                ),
            ),
        };
        let teams: Element<'a, Message> = match (&self.team_id, self.teams()) {
            (Some(_), _) | (None, []) => column![].into(),
            (None, [team]) => column![ui::section(s.into_team, c), ui::body(team.name.clone())]
                .spacing(6)
                .into(),
            (None, teams) => column![
                ui::section(s.into_team, c),
                Column::with_children(teams.iter().enumerate().map(|(i, t)| {
                    ui::list_row(
                        ui::body(t.name.clone()),
                        i == self.team,
                        Some(Message::Team(i)),
                        c,
                    )
                }))
                .spacing(3)
            ]
            .spacing(6)
            .into(),
        };
        let body = ui::panel(
            Some(title),
            column![
                column![
                    text(name).size(22).font(bold),
                    text(line).size(SIZE_BODY).color(c.muted),
                    text(self.picked.name.clone())
                        .size(SIZE_SMALL)
                        .color(c.muted),
                ]
                .spacing(4),
                teams,
                container(
                    row![
                        ui::pill(
                            &format!("Esc  {}", s.cancel),
                            false,
                            Some(Message::Cancel),
                            c
                        ),
                        ui::pill(
                            &format!("Enter  {}", s.import_archive),
                            true,
                            Some(Message::Import),
                            c
                        ),
                    ]
                    .spacing(10)
                )
                .width(Length::Fill)
                .align_x(Alignment::End),
            ]
            .spacing(18),
            c,
        );
        container(container(body).max_width(560))
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }
}
