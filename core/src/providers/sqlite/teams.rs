use crate::{
    domain::{Team, sql::Sql},
    errors::{AppError, IOError},
    logging::logger::log_error,
    player_form::removed,
    providers::{
        sqlite::{
            Database, not_found,
            rows::{team_ids, team_row, team_rows, tells_of, write_team},
            sqlite,
            summary::{forget_team, refresh},
            video::forget_video,
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

    /// Whether a team was ever saved here, a removed one too: the data of
    /// an installation used before.
    pub fn any_team_ever(&self) -> Result<bool, AppError> {
        self.0.read(|tx| {
            tx.query_row("SELECT EXISTS (SELECT 1 FROM teams)", [], |row| row.get(0))
                .map_err(sqlite)
        })
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

    /// D16: the matches against a name are removed with the team, those
    /// against a team stay to it, the removed team becoming a name (when
    /// it was A, B becomes A). What the removed team scouted of them is
    /// lost. The persons stay, and so do its memberships.
    async fn remove(&self, team_id: &Uuid) -> Result<(), AppError> {
        self.0.write(|tx| {
            let Some(team) = team_row(tx, *team_id)?.filter(|t| !t.deleted) else {
                return Ok(());
            };
            match_ids_where(tx, *team_id, AGAINST_A_NAME)?
                .into_iter()
                .try_for_each(|m| {
                    // their rows of the cache go with `forget_team` below
                    tx.execute("UPDATE matches SET deleted = 1 WHERE id = ?1", [Sql(m)])
                        .map_err(sqlite)?;
                    forget_video(tx, m)
                })?;
            // A's status doesn't come from B: its row of the cache holds
            match_ids_where(tx, *team_id, AS_B)?
                .into_iter()
                .try_for_each(|m| b_lost(tx, m, &team))?;
            match_ids_where(tx, *team_id, AS_A_AGAINST_A_TEAM)?
                .into_iter()
                .try_for_each(|m| {
                    a_lost(tx, m, &team)?;
                    refresh(tx, m)
                })?;
            tx.execute(
                "UPDATE teams SET deleted = 1 WHERE id = ?1",
                [Sql(*team_id)],
            )
            .map_err(sqlite)?;
            forget_team(tx, *team_id)
        })
    }
}

/// The matches of team A against a name (or a removed team).
const AGAINST_A_NAME: &str = "team_a = ?1 AND NOT EXISTS (SELECT 1 FROM teams b
                                    WHERE b.id = matches.team_b AND b.deleted = 0)";
/// The matches of team A against a team.
const AS_A_AGAINST_A_TEAM: &str = "team_a = ?1 AND EXISTS (SELECT 1 FROM teams b
                                         WHERE b.id = matches.team_b AND b.deleted = 0)";
/// The matches of team B.
const AS_B: &str = "team_b = ?1";

/// The matches not removed of team `team` chosen by `filter`.
fn match_ids_where(tx: &Transaction, team: Uuid, filter: &str) -> Result<Vec<Uuid>, AppError> {
    tx.prepare(&format!(
        "SELECT id FROM matches WHERE deleted = 0 AND ({filter}) ORDER BY start, id"
    ))
    .and_then(|mut statement| {
        statement
            .query_map([Sql(team)], |row| row.get::<_, Sql<Uuid>>(0).map(|id| id.0))?
            .collect::<Result<_, _>>()
    })
    .map_err(sqlite)
}

/// What removing a team does to its matches (D16), for the confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TeamRemoval {
    /// against a team: they stay to it
    pub kept: usize,
    /// against a name: removed with the team
    pub removed: usize,
}

impl SqliteTeamReader {
    /// What `TeamWriter::remove` would do to the matches of the team.
    pub fn removal(&self, team_id: &Uuid) -> Result<TeamRemoval, AppError> {
        self.0.read(|tx| {
            let count = |filter: &str| match_ids_where(tx, *team_id, filter).map(|m| m.len());
            Ok(TeamRemoval {
                kept: count(AS_A_AGAINST_A_TEAM)? + count(AS_B)?,
                removed: count(AGAINST_A_NAME)?,
            })
        })
    }
}

/// Team B `team` is removed: B becomes its name, the match is scouted
/// only on A's side, what was scouted of B is lost.
fn b_lost(tx: &Transaction, m: Uuid, team: &Team) -> Result<(), AppError> {
    tx.execute(
        "UPDATE events SET deleted = 1 WHERE match_id = ?1 AND side = 'b'",
        [Sql(m)],
    )
    .and_then(|_| {
        tx.execute(
            "DELETE FROM set_lineups WHERE match_id = ?1 AND side = 'b'",
            [Sql(m)],
        )
    })
    .and_then(|_| {
        tx.execute(
            "DELETE FROM match_roster WHERE match_id = ?1 AND team_id = ?2",
            params![Sql(m), Sql(team.id)],
        )
    })
    .and_then(|_| {
        tx.execute(
            "UPDATE matches SET team_b = NULL, team_b_name = ?2, scouting = 'a' WHERE id = ?1",
            params![Sql(m), team.name],
        )
    })
    .map(|_| ())
    .map_err(sqlite)
}

/// Team A `team` is removed: B becomes A, against the name of the removed
/// team. The sides of everything swap; what was scouted of the removed
/// team is lost, and so is a set left without the lineup of the new A
/// (all of them when only A was scouted). The match no longer waits to be
/// uploaded: what A scouted is gone.
fn a_lost(tx: &Transaction, m: Uuid, team: &Team) -> Result<(), AppError> {
    tx.execute(
        "UPDATE events SET side = CASE side WHEN 'a' THEN 'b' ELSE 'a' END,
             deleted = CASE side WHEN 'a' THEN 1 ELSE deleted END
         WHERE match_id = ?1",
        [Sql(m)],
    )
    .and_then(|_| {
        tx.execute(
            "DELETE FROM set_lineups WHERE match_id = ?1 AND side = 'a'",
            [Sql(m)],
        )
    })
    .and_then(|_| {
        tx.execute(
            "UPDATE set_lineups SET side = 'a' WHERE match_id = ?1",
            [Sql(m)],
        )
    })
    .and_then(|_| {
        tx.execute(
            "UPDATE sets SET serving = CASE serving WHEN 'a' THEN 'b' ELSE 'a' END,
                 deleted = CASE WHEN EXISTS (SELECT 1 FROM set_lineups l
                                             WHERE l.match_id = sets.match_id
                                             AND l.set_number = sets.number)
                           THEN deleted ELSE 1 END
             WHERE match_id = ?1",
            [Sql(m)],
        )
    })
    .and_then(|_| {
        tx.execute(
            "DELETE FROM match_roster WHERE match_id = ?1 AND team_id = ?2",
            params![Sql(m), Sql(team.id)],
        )
    })
    .and_then(|_| {
        tx.execute(
            "UPDATE matches SET team_a = team_b, team_b = NULL, team_b_name = ?2,
                 home = CASE home WHEN 'a' THEN 'b' WHEN 'b' THEN 'a' END, scouting = 'a'
             WHERE id = ?1",
            params![Sql(m), team.name],
        )
    })
    .and_then(|_| tx.execute("DELETE FROM upload_queue WHERE match_id = ?1", [Sql(m)]))
    .map(|_| ())
    .map_err(sqlite)
}
