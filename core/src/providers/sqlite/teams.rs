use crate::{
    domain::sql::Sql,
    errors::{AppError, IOError},
    logging::logger::log_error,
    player_form::removed,
    providers::{
        sqlite::{
            Database, not_found,
            rows::{team_ids, team_rows, tells_of, write_team},
            sqlite,
            summary::forget_team,
        },
        team_reader::TeamReader,
        team_writer::{PlayerInput, TeamInput, TeamWriter},
    },
    shapes::{
        listed::{Listed, Unreadable},
        player::PlayerEntry,
        team::TeamEntry,
    },
};
use async_trait::async_trait;
use rusqlite::{Transaction, params};
use uuid::Uuid;

pub struct SqliteTeamReader(Database);

impl SqliteTeamReader {
    pub fn new(db: Database) -> Self {
        Self(db)
    }
}

/// The team not removed, or an error.
pub(crate) fn read_team(tx: &Transaction, team_id: &Uuid) -> Result<TeamEntry, AppError> {
    team_rows(tx, *team_id)?
        .ok_or_else(|| not_found(&format!("team {team_id}")))?
        .entry()
}

#[async_trait]
impl TeamReader for SqliteTeamReader {
    async fn read_all(&self) -> Result<Vec<Listed<TeamEntry>>, AppError> {
        self.0.read(|tx| {
            Ok(team_ids(tx)?
                .into_iter()
                .map(|id| {
                    Uuid::parse_str(&id)
                        .map_err(|_| AppError::IO(IOError::Msg(format!("team id {id:?}"))))
                        .and_then(|team| read_team(tx, &team))
                        .map_err(|e| {
                            log_error(&format!("team {id:?} is unreadable: {e}"));
                            Unreadable { id, reason: None }
                        })
                })
                .collect())
        })
    }

    async fn read_single(&self, team_id: &Uuid) -> Result<TeamEntry, AppError> {
        self.0.read(|tx| read_team(tx, team_id))
    }

    async fn exists(&self, team_id: &Uuid) -> Result<bool, AppError> {
        self.0.read(|tx| {
            tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM teams WHERE id = ?1 AND deleted = 0)",
                [Sql(*team_id)],
                |row| row.get(0),
            )
            .map_err(sqlite)
        })
    }
}

pub struct SqliteTeamWriter(Database);

impl SqliteTeamWriter {
    pub fn new(db: Database) -> Self {
        Self(db)
    }
}

#[async_trait]
impl TeamWriter for SqliteTeamWriter {
    async fn save(&self, input: TeamInput) -> Result<TeamEntry, AppError> {
        let team = match input {
            TeamInput::New {
                id,
                name,
                classification,
                gender,
                country,
                year,
                players,
            } => TeamEntry {
                id: id.unwrap_or_else(Uuid::new_v4),
                name,
                classification,
                gender,
                country,
                year,
                players,
            },
            TeamInput::Existing(team) => team,
        };
        self.0.write(|tx| write_team(tx, &team))?;
        Ok(team)
    }

    async fn save_player(
        &self,
        input: PlayerInput,
        team: &mut TeamEntry,
    ) -> Result<PlayerEntry, AppError> {
        let player = match input {
            PlayerInput::New {
                name,
                role,
                number,
                details,
            } => PlayerEntry {
                name,
                role: Some(role),
                number,
                birth_date: details.birth_date,
                height_cm: details.height_cm,
                notes: details.notes,
                ..Default::default()
            },
            PlayerInput::Existing(existing) => existing,
        };
        let mut saved = team.clone();
        match saved.players.iter_mut().find(|p| p.id == player.id) {
            Some(existing) => *existing = player.clone(),
            None => saved.players.push(player.clone()),
        }
        self.0.write(|tx| write_team(tx, &saved))?;
        *team = saved;
        Ok(player)
    }

    /// The membership is removed, never the person: a player a match tells
    /// of stays in the team, removed, the others are no longer read.
    async fn remove_player(&self, player_id: Uuid, team: &mut TeamEntry) -> Result<(), AppError> {
        let saved = self.0.write(|tx| {
            let told = tells_of(tx, team.id, player_id)?;
            let saved = TeamEntry {
                players: team
                    .players
                    .iter()
                    .filter_map(|p| match (p.id == player_id, told) {
                        (false, _) => Some(p.clone()),
                        (true, true) => Some(removed(p)),
                        (true, false) => None,
                    })
                    .collect(),
                ..team.clone()
            };
            write_team(tx, &saved)?;
            Ok(saved)
        })?;
        *team = saved;
        Ok(())
    }

    /// The team and its matches marked removed (D16 at step 16): the
    /// persons stay.
    async fn remove(&self, team_id: &Uuid) -> Result<(), AppError> {
        self.0.write(|tx| {
            tx.execute(
                "UPDATE matches SET deleted = 1 WHERE team_a = ?1",
                [Sql(*team_id)],
            )
            .and_then(|_| {
                tx.execute(
                    "UPDATE teams SET deleted = 1 WHERE id = ?1",
                    params![Sql(*team_id)],
                )
            })
            .map_err(sqlite)?;
            forget_team(tx, *team_id)
        })
    }
}
