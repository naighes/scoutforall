use crate::{
    domain::{
        Competition, Home, Match, RosterEntry, ScoutingMode, Team, TeamRef, adapters::MatchRows,
        sql::Sql,
    },
    errors::{AppError, IOError, MatchError},
    localization::Language,
    logging::logger::log_error,
    match_form::{CompetitionInput, MatchInput, Opponent},
    providers::{
        match_reader::MatchReader,
        match_writer::MatchWriter,
        sqlite::{
            Database,
            competitions::{competition_row, insert_competition},
            not_found,
            rows::{
                insert_match, insert_roster_entry, match_ids, match_rows, team_row, write_team,
            },
            sets::numbered,
            sqlite,
            summary::{cached, forget, refresh, save},
            teams::read_team,
            video::forget_video,
        },
    },
    shapes::{
        listed::{Listed, Unreadable},
        r#match::{MatchEntry, MatchSource, MatchStatus, MatchSummary},
        team::TeamEntry,
    },
};
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset};
use rusqlite::{Transaction, params};
use std::cmp::Ordering;
use uuid::Uuid;

/// The id of a match, its UUID as text.
pub(crate) fn match_uuid(match_id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(match_id)
        .ok()
        .filter(|id| id.to_string() == match_id)
        .ok_or_else(|| AppError::IO(IOError::Msg(format!("invalid match: {match_id:?}"))))
}

/// The match of team A `team`, with the status its sets replay to.
pub(crate) fn replayed(
    tx: &Transaction,
    team: &TeamEntry,
    match_id: &str,
    language: &Language,
) -> Result<(MatchEntry, MatchStatus), AppError> {
    let id = match_uuid(match_id)?;
    let rows = match_rows(tx, id)?
        .filter(|rows| rows.row.team_a == team.id)
        .ok_or_else(|| not_found(&format!("match {match_id} of team {}", team.id)))?;
    let team_b = match &rows.row.team_b {
        TeamRef::Team(b) => team_row(tx, *b)?,
        TeamRef::Name(_) => None,
    };
    let entry = rows.entry(team, team_b.as_ref())?;
    let entry = MatchEntry {
        sets: numbered(entry.sets, language, match_id)?,
        ..entry
    };
    let status = replay_status(&entry, language)?;
    Ok((entry, status))
}

pub struct SqliteMatchReader {
    db: Database,
    language: Language,
}

impl SqliteMatchReader {
    pub fn new(db: Database, language: Language) -> Self {
        Self { db, language }
    }

    /// The match of team A `team`, with the status its sets replay to.
    fn entry(
        &self,
        tx: &Transaction,
        team: &TeamEntry,
        match_id: &str,
    ) -> Result<(MatchEntry, MatchStatus), AppError> {
        replayed(tx, team, match_id, &self.language)
    }

    /// The row of the match of team A `team`, for its form, with the
    /// competition it's played in.
    pub fn details(
        &self,
        team: &TeamEntry,
        match_id: &str,
    ) -> Result<(Match, Option<Competition>), AppError> {
        let id = match_uuid(match_id)?;
        self.db.read(|tx| {
            let row = match_rows(tx, id)?
                .map(|rows| rows.row)
                .filter(|row| row.team_a == team.id)
                .ok_or_else(|| not_found(&format!("match {match_id} of team {}", team.id)))?;
            let competition = match row.competition_id {
                Some(c) => competition_row(tx, c)?,
                None => None,
            };
            Ok((row, competition))
        })
    }

    fn listed(
        &self,
        tx: &Transaction,
        team: &TeamEntry,
        match_id: &str,
    ) -> Listed<(MatchEntry, MatchStatus)> {
        self.entry(tx, team, match_id)
            .map_err(|e| unreadable(match_id, team, e))
    }
}

#[async_trait]
impl MatchReader for SqliteMatchReader {
    async fn read_all(&self, team: &TeamEntry) -> Result<Vec<Listed<MatchEntry>>, AppError> {
        Ok(self
            .read_all_with_status(team)
            .await?
            .into_iter()
            .map(|listed| listed.map(|(entry, _)| entry))
            .collect())
    }

    async fn read_all_with_status(
        &self,
        team: &TeamEntry,
    ) -> Result<Vec<Listed<(MatchEntry, MatchStatus)>>, AppError> {
        let mut entries: Vec<_> = self.db.read(|tx| {
            Ok(match_ids(tx, team.id)?
                .iter()
                .map(|id| self.listed(tx, team, id))
                .collect())
        })?;
        entries.sort_by(|a, b| recent_first(a, b, |(m, _)| m.date));
        Ok(entries)
    }

    async fn read_some(&self, team: &TeamEntry, ids: &[String]) -> Vec<Listed<MatchEntry>> {
        let read = self.db.read(|tx| {
            Ok(ids
                .iter()
                .map(|id| self.listed(tx, team, id).map(|(entry, _)| entry))
                .collect::<Vec<_>>())
        });
        match read {
            Ok(read) => read,
            // the database itself can't be read: none of them
            Err(e) => ids
                .iter()
                .map(|id| {
                    Err(unreadable(
                        id,
                        team,
                        AppError::IO(IOError::Msg(e.to_string())),
                    ))
                })
                .collect(),
        }
    }

    /// From the cache (`summary`): the matches without a row of this
    /// version are replayed, and their rows written.
    async fn read_summaries(
        &self,
        team: &TeamEntry,
    ) -> Result<Vec<Listed<MatchSummary>>, AppError> {
        let (mut summaries, missing) = self.db.read(|tx| {
            let mut summaries = vec![];
            let mut missing = vec![];
            for id in match_ids(tx, team.id)? {
                match match_uuid(&id).and_then(|uuid| cached(tx, team, uuid)) {
                    Ok(Some(summary)) => summaries.push(Ok(summary)),
                    Ok(None) => missing.push(id),
                    Err(e) => summaries.push(Err(unreadable(&id, team, e))),
                }
            }
            Ok((summaries, missing))
        })?;
        let rebuilt = match missing.as_slice() {
            [] => vec![],
            ids => self.db.write(|tx| {
                ids.iter()
                    .map(|id| match self.listed(tx, team, id) {
                        Ok((m, status)) => {
                            let summary = MatchSummary::of(&m, &status);
                            save(tx, match_uuid(id)?, &summary).map(|()| Ok(summary))
                        }
                        Err(unreadable) => Ok(Err(unreadable)),
                    })
                    .collect::<Result<Vec<_>, _>>()
            })?,
        };
        summaries.extend(rebuilt);
        summaries.sort_by(|a, b| recent_first(a, b, |s| s.date));
        Ok(summaries)
    }

    async fn read_single(&self, team: &TeamEntry, match_id: &str) -> Result<MatchEntry, AppError> {
        self.db
            .read(|tx| self.entry(tx, team, match_id))
            .map(|(entry, _)| entry)
    }

    /// A match of team A `team` not removed that was in that folder
    /// (`legacy_id`, migrated or imported from an archive of then), or
    /// whose opponent and day make that id now, the first by start.
    async fn saved_as(&self, team: &TeamEntry, file_id: &str) -> Result<Option<String>, AppError> {
        self.db.read(|tx| {
            let mut statement = tx
                .prepare(
                    "SELECT m.id, m.legacy_id = ?2 AND m.legacy_team = ?1,
                            coalesce(m.team_b_name, b.name), m.start
                     FROM matches m LEFT JOIN teams b ON b.id = m.team_b
                     WHERE m.team_a = ?1 AND m.deleted = 0
                     ORDER BY m.start, m.id",
                )
                .map_err(sqlite)?;
            let rows = statement
                .query_map(params![Sql(team.id), file_id], |row| {
                    Ok((
                        row.get::<_, Sql<Uuid>>(0)?.0,
                        row.get::<_, Option<bool>>(1)?.unwrap_or(false),
                        row.get::<_, String>(2)?,
                        row.get::<_, Sql<DateTime<FixedOffset>>>(3)?.0,
                    ))
                })
                .map_err(sqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sqlite)?;
            let was_there = rows.iter().find(|(_, legacy, _, _)| *legacy);
            let same_now = || {
                rows.iter()
                    .find(|(_, _, opponent, start)| MatchEntry::id_of(opponent, start) == file_id)
            };
            Ok(was_there.or_else(same_now).map(|(id, ..)| id.to_string()))
        })
    }

    async fn exists(&self, team: &TeamEntry, match_id: &str) -> Result<bool, AppError> {
        let Ok(id) = match_uuid(match_id) else {
            return Ok(false);
        };
        self.db.read(|tx| {
            tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM matches WHERE id = ?1 AND team_a = ?2
                                AND deleted = 0)",
                params![Sql(id), Sql(team.id)],
                |row| row.get(0),
            )
            .map_err(sqlite)
        })
    }
}

pub struct SqliteMatchWriter(Database);

impl SqliteMatchWriter {
    pub fn new(db: Database) -> Self {
        Self(db)
    }
}

/// Team B as saved for a match of team A `team`: a team not removed, a
/// new one without players (the year, classification, gender and country
/// of A: likely of the same league),
/// or a name; the team, for its name.
fn saved_team_b(
    tx: &Transaction,
    team: &TeamEntry,
    opponent: &Opponent,
) -> Result<(TeamRef, Option<Team>), AppError> {
    match opponent {
        Opponent::Name(name) => Ok((TeamRef::Name(name.clone()), None)),
        Opponent::Team(id) => team_row(tx, *id)?
            .filter(|b| !b.deleted && b.id != team.id)
            .map(|b| (TeamRef::Team(b.id), Some(b)))
            .ok_or_else(|| not_found(&format!("team {id}"))),
        Opponent::NewTeam(name) => {
            let b = TeamEntry {
                id: Uuid::new_v4(),
                name: name.clone(),
                year: team.year,
                classification: team.classification,
                gender: team.gender,
                country: team.country,
                players: vec![],
            };
            write_team(tx, &b)?;
            team_row(tx, b.id)?
                .map(|row| (TeamRef::Team(b.id), Some(row)))
                .ok_or_else(|| not_found(&format!("team {}", b.id)))
        }
    }
}

/// The competition as saved for a match of team A `team`: one not
/// removed, or a new one of the season of A (its year and the next), its
/// gender and country.
fn saved_competition(
    tx: &Transaction,
    team: &TeamEntry,
    competition: Option<&CompetitionInput>,
) -> Result<Option<Uuid>, AppError> {
    match competition {
        None => Ok(None),
        Some(CompetitionInput::Existing(id)) => competition_row(tx, *id)?
            .filter(|c| !c.deleted)
            .map(|c| Some(c.id))
            .ok_or_else(|| not_found(&format!("competition {id}"))),
        Some(CompetitionInput::New { name, format }) => {
            let c = Competition {
                id: Uuid::new_v4(),
                name: name.clone(),
                format: *format,
                season: Some(format!("{}/{}", team.year, team.year.saturating_add(1))),
                gender: team.gender,
                country: team.country,
                friendly: false,
                deleted: false,
            };
            insert_competition(tx, &c)?;
            Ok(Some(c.id))
        }
    }
}

/// Updates the match `id` with `input`: team B changes, unless both teams
/// are scouted. The roster: the players left out go, the ones added copy
/// number and role from the team (D9), the others keep those of the
/// match; empty, the active team.
fn update_match(tx: &Transaction, id: Uuid, input: MatchInput) -> Result<(), AppError> {
    let MatchInput {
        opponent,
        date,
        home,
        max_substitutions,
        notes,
        roster,
        friendly,
        competition,
        stage,
        round,
    } = input;
    let rows = match_rows(tx, id)?.ok_or_else(|| not_found(&format!("match {id}")))?;
    let team = read_team(tx, &rows.row.team_a)?;
    let (team_b, _) = saved_team_b(tx, &team, &opponent)?;
    if rows.row.scouting == ScoutingMode::Both && team_b != rows.row.team_b {
        return Err(AppError::IO(IOError::Msg(format!(
            "match {id}: team B of a match scouted on both sides can't change"
        ))));
    }
    let competition_id = saved_competition(tx, &team, competition.as_ref())?;
    let (team_b, team_b_name) = team_b.columns();
    tx.execute(
        "UPDATE matches SET team_b = ?2, team_b_name = ?3, start = ?4, home = ?5,
                 max_substitutions = ?6, notes = ?7, friendly = ?8, competition_id = ?9,
                 stage = ?10, round = ?11
             WHERE id = ?1",
        params![
            Sql(id),
            team_b.map(Sql),
            team_b_name,
            Sql(date),
            home,
            max_substitutions,
            notes,
            friendly,
            competition_id.map(Sql),
            stage,
            round
        ],
    )
    .map_err(sqlite)?;
    let team_a = rows.row.team_a;
    let roster = match roster.as_slice() {
        [] => read_team(tx, &team_a)?
            .active_players()
            .iter()
            .map(|p| p.id)
            .collect(),
        ids => ids.to_vec(),
    };
    let saved: Vec<Uuid> = rows
        .roster
        .iter()
        .filter(|r| r.team_id == team_a)
        .map(|r| r.person_id)
        .collect();
    for gone in saved.iter().filter(|p| !roster.contains(p)) {
        tx.execute(
            "DELETE FROM match_roster WHERE match_id = ?1 AND team_id = ?2
                 AND person_id = ?3",
            params![Sql(id), Sql(team_a), Sql(*gone)],
        )
        .map_err(sqlite)?;
    }
    let team = read_team(tx, &team_a)?;
    roster
        .iter()
        .filter(|p| !saved.contains(p))
        .try_for_each(|person_id| {
            let player = team
                .find_player(*person_id)
                .ok_or_else(|| not_found(&format!("player {person_id} of team {team_a}")))?;
            insert_roster_entry(
                tx,
                &RosterEntry {
                    match_id: id,
                    team_id: team_a,
                    person_id: player.id,
                    number: player.number,
                    role: player.role,
                },
            )
        })?;
    // the substitutions decide how the sets replay
    // the substitutions decide how the sets replay
    refresh(tx, id)
}

#[async_trait]
impl MatchWriter for SqliteMatchWriter {
    /// A match with a new id: two matches against the same opponent on the
    /// same day are two matches (no `MatchAlreadyExists`). Its roster
    /// copies number and role from the team (D9), the whole active team
    /// when empty.
    async fn create(&self, team: &TeamEntry, input: MatchInput) -> Result<MatchEntry, AppError> {
        let id = Uuid::new_v4();
        let m = MatchEntry {
            source: Default::default(),
            video_clock: false,
            opponent: String::new(),
            date: input.date,
            id: id.to_string(),
            team: team.with_roster(&input.roster),
            home: input.home.is_a(),
            max_substitutions: input.max_substitutions,
            notes: input.notes,
            roster: input.roster,
            sets: vec![],
        };
        self.0.write(|tx| {
            let (team_b, b) = saved_team_b(tx, team, &input.opponent)?;
            let competition_id = saved_competition(tx, team, input.competition.as_ref())?;
            let rows = MatchRows::of(&m, id, team_b, None)?;
            let rows = MatchRows {
                row: Match {
                    home: input.home,
                    friendly: input.friendly,
                    competition_id,
                    stage: input.stage,
                    round: input.round,
                    ..rows.row
                },
                ..rows
            };
            insert_match(tx, &rows)?;
            refresh(tx, id)?;
            rows.entry(team, b.as_ref())
        })
    }

    /// See `update_match`.
    async fn update(&self, m: &MatchEntry, input: MatchInput) -> Result<(), AppError> {
        let id = match_uuid(&m.id)?;
        self.0.write(|tx| update_match(tx, id, input))
    }

    /// Team B changes only when it's a name; a neutral court stays neutral
    /// when A doesn't play at home; the competition, its stage and round,
    /// friendly or not, stay.
    async fn replace_details(&self, m: &MatchEntry) -> Result<(), AppError> {
        let id = match_uuid(&m.id)?;
        self.0.write(|tx| {
            let row = match_rows(tx, id)?
                .ok_or_else(|| not_found(&format!("match {id}")))?
                .row;
            let input = MatchInput {
                opponent: match row.team_b {
                    TeamRef::Team(b) => Opponent::Team(b),
                    TeamRef::Name(_) => Opponent::Name(m.opponent.clone()),
                },
                home: match (m.home, row.home) {
                    (true, _) => Home::A,
                    (false, Home::Neutral) => Home::Neutral,
                    (false, _) => Home::B,
                },
                friendly: row.friendly,
                competition: row.competition_id.map(CompetitionInput::Existing),
                stage: row.stage,
                round: row.round,
                ..MatchInput::against(
                    &m.opponent,
                    m.date,
                    m.home,
                    m.max_substitutions,
                    &m.notes,
                    m.roster.clone(),
                )
            };
            update_match(tx, id, input)
        })
    }

    async fn save_source(
        &self,
        m: &MatchEntry,
        source: MatchSource,
        video_clock: bool,
    ) -> Result<MatchEntry, AppError> {
        let id = match_uuid(&m.id)?;
        let changed = self.0.write(|tx| {
            tx.execute(
                "UPDATE matches SET source = ?2, video_clock = ?3 WHERE id = ?1 AND deleted = 0",
                params![Sql(id), source, video_clock],
            )
            .map_err(sqlite)
        })?;
        match changed {
            0 => Err(not_found(&format!("match {}", m.id))),
            _ => Ok(MatchEntry {
                source,
                video_clock,
                ..m.clone()
            }),
        }
    }

    /// Marked removed, with what it has (D7), by either team: one match
    /// for both (D2). Its video, of this device, goes with it.
    async fn remove(&self, team_id: &Uuid, match_id: &str) -> Result<(), AppError> {
        let id = match_uuid(match_id)?;
        self.0.write(|tx| {
            let removed = tx
                .execute(
                    "UPDATE matches SET deleted = 1
                     WHERE id = ?1 AND ?2 IN (team_a, team_b) AND deleted = 0",
                    params![Sql(id), Sql(*team_id)],
                )
                .map_err(sqlite)?;
            match removed {
                0 => Ok(()),
                _ => forget(tx, id).and_then(|()| forget_video(tx, id)),
            }
        })
    }
}

/// The status of the match, as its sets replay: a match whose sets don't
/// replay can't be shown nor scouted, the first event that doesn't tells
/// why.
fn replay_status(entry: &MatchEntry, language: &Language) -> Result<MatchStatus, AppError> {
    entry.get_status().map_err(|e| match entry.replay_issue() {
        Some(issue) => AppError::Match(MatchError::SetNotValid(issue.describe(language.labels()))),
        None => e,
    })
}

/// Why the match `match_id` of the team can't be read (logged).
fn unreadable(match_id: &str, team: &TeamEntry, e: AppError) -> Unreadable {
    log_error(&format!(
        "match {:?} of team {} is unreadable: {}",
        match_id, team.id, e
    ));
    Unreadable {
        id: match_id.to_string(),
        reason: match e {
            AppError::Match(MatchError::SetNotValid(reason)) => Some(reason),
            _ => None,
        },
    }
}

/// The most recent first, then the unreadable ones by name.
fn recent_first<T>(
    a: &Listed<T>,
    b: &Listed<T>,
    date: impl Fn(&T) -> DateTime<FixedOffset>,
) -> Ordering {
    match (a, b) {
        (Ok(a), Ok(b)) => date(b).cmp(&date(a)),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        (Err(a), Err(b)) => a.id.cmp(&b.id),
    }
}
