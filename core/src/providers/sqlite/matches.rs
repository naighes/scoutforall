use crate::{
    domain::{
        Competition, Match, RosterEntry, ScoutingMode, Side, Team, TeamRef, adapters::MatchRows,
        sql::Sql,
    },
    errors::{AppError, IOError, MatchError},
    localization::Language,
    logging::logger::log_error,
    match_form::{CompetitionInput, MatchInput, Opponent, RosterNumber},
    providers::{
        match_reader::{MatchReader, PlayedOn},
        match_writer::MatchWriter,
        sqlite::{
            Database,
            competitions::{competition_row, competition_rows, insert_competition},
            not_found,
            rows::{
                insert_match, insert_roster_entry, match_ids, match_rows, matches_as_b,
                matches_both, matches_scouted_as_b, team_row, write_team,
            },
            sets::numbered,
            sqlite,
            summary::{cached, competition_of, forget, opponent_of, refresh, save},
            teams::read_team,
            training::check_match,
            venues::find_or_create,
            video::forget_video,
        },
    },
    shapes::{
        listed::{Listed, Unreadable},
        r#match::{
            MatchEntry, MatchSource, MatchStatus, MatchSummary, PlayedMatch, ScoutedSummary,
        },
        team::TeamEntry,
    },
};
use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate};
use rusqlite::{OptionalExtension, Transaction, params};
use std::cmp::Ordering;
use std::collections::HashSet;
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

/// A match scouted on both sides, as the analysis of its team B picks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BothMatch {
    pub id: String,
    pub team_b: Uuid,
    pub name_b: String,
    pub home_b: bool,
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

    /// The match of team A `team` from B's side, scouted on both sides
    /// (`MatchRows::entry_of_b`): its sets first checked to replay alike
    /// on both sides (`SetRows::sides`), an error naming the touch that
    /// doesn't. `None` scouting only A.
    pub fn read_of_b(
        &self,
        team: &TeamEntry,
        match_id: &str,
    ) -> Result<Option<MatchEntry>, AppError> {
        let id = match_uuid(match_id)?;
        self.db.read(|tx| {
            let rows = match_rows(tx, id)?
                .filter(|rows| rows.row.team_a == team.id)
                .ok_or_else(|| not_found(&format!("match {match_id} of team {}", team.id)))?;
            match (rows.row.scouting, &rows.row.team_b) {
                (ScoutingMode::Both, TeamRef::Team(b)) => {
                    rows.sets
                        .iter()
                        .filter(|s| !s.row.deleted)
                        .try_for_each(|s| s.sides(rows.row.max_substitutions).map(|_| ()))?;
                    let team_b = read_team(tx, b)?;
                    rows.entry_of_b(team, &team_b)?
                        .map(|entry| {
                            numbered(entry.sets.clone(), &self.language, match_id)
                                .map(|sets| MatchEntry { sets, ..entry })
                        })
                        .transpose()
                }
                _ => Ok(None),
            }
        })
    }

    /// The roster of B in the last match of team A `team` against the same
    /// team B before the match `match_id` (by date) that has one, with the
    /// numbers and roles of that match: matches scouting only A write none
    /// and are skipped. `None` when there is no such match, or B is a name.
    pub fn previous_roster_of_b(
        &self,
        team: &TeamEntry,
        match_id: &str,
    ) -> Result<Option<Vec<RosterNumber>>, AppError> {
        let id = match_uuid(match_id)?;
        self.db.read(|tx| {
            let Some(rows) = match_rows(tx, id)?.filter(|rows| rows.row.team_a == team.id) else {
                return Err(not_found(&format!("match {match_id} of team {}", team.id)));
            };
            let TeamRef::Team(b) = rows.row.team_b else {
                return Ok(None);
            };
            let previous: Option<Uuid> = tx
                .query_row(
                    "SELECT m.id FROM matches m
                     WHERE m.team_a = ?1 AND m.team_b = ?2 AND m.deleted = 0 AND m.id != ?3
                       AND julianday(m.start) < julianday(?4)
                       AND EXISTS (SELECT 1 FROM match_roster r
                                   WHERE r.match_id = m.id AND r.team_id = ?2)
                     ORDER BY julianday(m.start) DESC, m.id DESC LIMIT 1",
                    params![Sql(team.id), Sql(b), Sql(id), Sql(rows.row.start)],
                    |r| r.get::<_, Sql<Uuid>>(0).map(|s| s.0),
                )
                .optional()
                .map_err(sqlite)?;
            previous
                .map(|previous| {
                    Ok(match_rows(tx, previous)?
                        .map(|rows| {
                            rows.roster
                                .iter()
                                .filter(|r| r.team_id == b)
                                .map(|r| RosterNumber {
                                    player: r.person_id,
                                    number: r.number,
                                    role: r.role,
                                })
                                .collect()
                        })
                        .unwrap_or_default())
                })
                .transpose()
        })
    }

    /// For each set of the match scouted on both sides, and each event of
    /// B's side (`read_of_b`), the index of the event of A's side coming
    /// from the same touch, if any (`MatchRows::origins_of_b`). Empty
    /// scouting only A.
    pub fn origins_of_b(
        &self,
        team: &TeamEntry,
        match_id: &str,
    ) -> Result<Vec<(u8, Vec<Option<usize>>)>, AppError> {
        let id = match_uuid(match_id)?;
        self.db.read(|tx| {
            Ok(match_rows(tx, id)?
                .filter(|rows| rows.row.team_a == team.id)
                .ok_or_else(|| not_found(&format!("match {match_id} of team {}", team.id)))?
                .origins_of_b())
        })
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

    /// The summaries of these matches, each of its team A, in their
    /// order: from the cache (`summary`), the matches without a row of
    /// this version replayed and their rows written.
    fn summaries(
        &self,
        matches: &[(TeamEntry, String)],
    ) -> Result<Vec<Listed<MatchSummary>>, AppError> {
        let cached: Vec<Option<Listed<MatchSummary>>> = self.db.read(|tx| {
            Ok(matches
                .iter()
                .map(
                    |(team, id)| match match_uuid(id).and_then(|uuid| cached(tx, team, uuid)) {
                        Ok(Some(summary)) => Some(Ok(summary)),
                        Ok(None) => None,
                        Err(e) => Some(Err(unreadable(id, team, e))),
                    },
                )
                .collect())
        })?;
        let missing: Vec<&(TeamEntry, String)> = matches
            .iter()
            .zip(&cached)
            .filter(|(_, summary)| summary.is_none())
            .map(|(m, _)| m)
            .collect();
        let rebuilt = match missing.as_slice() {
            [] => vec![],
            missing => self.db.write(|tx| {
                missing
                    .iter()
                    .map(|(team, id)| match self.listed(tx, team, id) {
                        Ok((m, status)) => {
                            let summary = MatchSummary {
                                competition_id: competition_of(tx, match_uuid(id)?)?,
                                opponent_team: opponent_of(tx, match_uuid(id)?)?,
                                ..MatchSummary::of(&m, &status)
                            };
                            save(tx, match_uuid(id)?, &summary).map(|()| Ok(summary))
                        }
                        Err(unreadable) => Ok(Err(unreadable)),
                    })
                    .collect::<Result<Vec<_>, _>>()
            })?,
        };
        let mut rebuilt = rebuilt.into_iter();
        Ok(cached
            .into_iter()
            .filter_map(|summary| summary.or_else(|| rebuilt.next()))
            .collect())
    }

    /// Every match the team plays, from its side, the most recent first,
    /// then the unreadable ones: those it's A of (`read_summaries`), and
    /// those it's B of, scouted from A's side only: A's summary seen by B.
    pub async fn played(&self, team: &TeamEntry) -> Result<Vec<Listed<PlayedMatch>>, AppError> {
        let as_a = self.read_summaries(team).await?.into_iter().map(|listed| {
            listed.map(|summary| PlayedMatch {
                summary,
                scouted_by: None,
            })
        });
        let (readable, unreadable): (Vec<_>, Vec<_>) = self.db.read(|tx| {
            Ok(matches_as_b(tx, team.id)?
                .into_iter()
                .map(|(id, a, home)| match read_team(tx, &a) {
                    Ok(a) => Ok((a, id, home)),
                    Err(e) => Err(unreadable(&id, team, e)),
                })
                .partition(Result::is_ok))
        })?;
        let readable: Vec<_> = readable.into_iter().flatten().collect();
        let matches: Vec<_> = readable
            .iter()
            .map(|(a, id, _)| (a.clone(), id.clone()))
            .collect();
        let as_b =
            self.summaries(&matches)?
                .into_iter()
                .zip(&readable)
                .map(|(listed, (a, _, home))| {
                    listed.map(|summary| PlayedMatch {
                        summary: summary.seen_by_b(a.id, a.name.clone(), home.is_b()),
                        scouted_by: Some(a.id),
                    })
                });
        let unreadable = unreadable.into_iter().filter_map(Result::err).map(Err);
        let mut played: Vec<_> = as_a.chain(as_b).chain(unreadable).collect();
        played.sort_by(|a, b| recent_first(a, b, |m| m.summary.date));
        Ok(played)
    }

    /// The summaries of the matches that scouted the players of `team`
    /// (as `read_scouted` reads them), each from its side, in order of
    /// date. The unreadable ones are left out.
    pub async fn scouted_summaries(
        &self,
        team: &TeamEntry,
    ) -> Result<Vec<ScoutedSummary>, AppError> {
        let as_a = self
            .read_summaries(team)
            .await?
            .into_iter()
            .filter_map(Result::ok)
            .map(|summary| ScoutedSummary {
                summary,
                team_a: None,
            });
        let as_b: Vec<_> = self.db.read(|tx| {
            Ok(matches_scouted_as_b(tx, team.id)?
                .into_iter()
                .filter_map(|(id, a, home)| read_team(tx, &a).ok().map(|a| (a, id, home)))
                .collect())
        })?;
        let matches: Vec<_> = as_b
            .iter()
            .map(|(a, id, _)| (a.clone(), id.clone()))
            .collect();
        let as_b =
            self.summaries(&matches)?
                .into_iter()
                .zip(as_b)
                .filter_map(|(listed, (a, _, home))| {
                    listed.ok().map(|summary| ScoutedSummary {
                        summary: summary.seen_by_b(a.id, a.name.clone(), home.is_b()),
                        team_a: Some(a),
                    })
                });
        let mut scouted: Vec<_> = as_a.chain(as_b).collect();
        scouted.sort_by_key(|m| m.summary.date);
        Ok(scouted)
    }

    /// The matches of team A `team` scouted on both sides, with the team B
    /// of each and whether B plays at home, in order of date: B's side of
    /// each is `read_of_b`. A team B that can't be read is left out.
    pub fn both_matches(&self, team: &TeamEntry) -> Result<Vec<BothMatch>, AppError> {
        self.db.read(|tx| {
            Ok(matches_both(tx, team.id)?
                .into_iter()
                .filter_map(|(id, b, home)| {
                    read_team(tx, &b).ok().map(|b| BothMatch {
                        id,
                        team_b: b.id,
                        name_b: b.name,
                        home_b: home.is_b(),
                    })
                })
                .collect())
        })
    }

    /// The matches that scouted the players of `team`, each from its side:
    /// those it's A of, and those scouted on both sides it's B of. The
    /// unreadable ones are left out, as in the analysis.
    pub async fn read_scouted(&self, team: &TeamEntry) -> Result<Vec<MatchEntry>, AppError> {
        let as_a = self.read_all(team).await?;
        let as_b = self.db.read(|tx| matches_scouted_as_b(tx, team.id))?;
        let as_b = as_b.into_iter().filter_map(|(id, a, _)| {
            self.db
                .read(|tx| read_team(tx, &a))
                .and_then(|a| self.read_of_b(&a, &id))
                .ok()
                .flatten()
        });
        Ok(as_a
            .into_iter()
            .filter_map(Result::ok)
            .chain(as_b)
            .collect())
    }

    /// The match in which `team` is B, as B sees it: team A, and whether
    /// B plays at home; `None` when the team isn't B of that match, or the
    /// match was removed.
    pub fn played_as_b(
        &self,
        team: &TeamEntry,
        match_id: &str,
    ) -> Result<Option<(TeamEntry, bool)>, AppError> {
        self.db.read(|tx| {
            matches_as_b(tx, team.id)?
                .into_iter()
                .find(|(id, ..)| id == match_id)
                .map(|(_, a, home)| read_team(tx, &a).map(|a| (a, home.is_b())))
                .transpose()
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
        let ids = self.db.read(|tx| match_ids(tx, team.id))?;
        let matches: Vec<_> = ids.into_iter().map(|id| (team.clone(), id)).collect();
        let mut summaries = self.summaries(&matches)?;
        summaries.sort_by(|a, b| recent_first(a, b, |s| s.date));
        Ok(summaries)
    }

    async fn read_single(&self, team: &TeamEntry, match_id: &str) -> Result<MatchEntry, AppError> {
        self.db
            .read(|tx| self.entry(tx, team, match_id))
            .map(|(entry, _)| entry)
    }

    async fn played_on(&self, teams: &[Uuid], day: NaiveDate) -> Result<Vec<PlayedOn>, AppError> {
        self.db.read(|tx| {
            let mut statement = tx
                .prepare(
                    "SELECT id, team_a, team_b, start FROM matches
                     WHERE deleted = 0 AND (team_a = ?1 OR team_b = ?1)",
                )
                .map_err(sqlite)?;
            let mut rows = vec![];
            for team in teams {
                let of_team = statement
                    .query_map(params![Sql(*team)], |row| {
                        Ok((
                            row.get::<_, Sql<Uuid>>(0)?.0,
                            row.get::<_, Sql<Uuid>>(1)?.0,
                            row.get::<_, Option<Sql<Uuid>>>(2)?.map(|b| b.0),
                            row.get::<_, Sql<DateTime<FixedOffset>>>(3)?.0,
                        ))
                    })
                    .map_err(sqlite)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(sqlite)?;
                rows.extend(of_team);
            }
            rows.sort_by_key(|(id, _, _, start)| (*start, *id));
            rows.dedup_by_key(|(id, ..)| *id);
            rows.into_iter()
                .filter(|(.., start)| start.date_naive() == day)
                .map(|(id, a, b, _)| {
                    let team = read_team(tx, &a)?;
                    Ok(self
                        .entry(tx, &team, &id.to_string())
                        .ok()
                        .and_then(|(m, _)| {
                            m.set_scores().ok().map(|scores| PlayedOn {
                                id: m.id.clone(),
                                team_a: a,
                                team_b: b,
                                names: (team.name.clone(), m.opponent.clone()),
                                scores,
                            })
                        }))
                })
                .filter_map(Result::transpose)
                .collect()
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
/// gender and country; a new one with the name of one of that season is
/// that one.
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
            let season = format!("{}/{}", team.year, team.year.saturating_add(1));
            // the one of the name and the season if there's one already
            // (the form of a competition keeps them apart): no twin
            let same = |a: &str, b: &str| a.trim().to_lowercase() == b.trim().to_lowercase();
            let there = competition_rows(tx)?.into_iter().find(|c| {
                same(&c.name, name) && same(c.season.as_deref().unwrap_or_default(), &season)
            });
            match there {
                Some(c) => Ok(Some(c.id)),
                None => {
                    let c = Competition {
                        id: Uuid::new_v4(),
                        name: name.clone(),
                        format: *format,
                        season: Some(season),
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
    }
}

/// The match `id` without the lineups of B, its touches removed: scouted
/// only on A's side.
fn without_side_b(tx: &Transaction, id: Uuid) -> Result<(), AppError> {
    tx.execute(
        "DELETE FROM set_lineups WHERE match_id = ?1 AND side = ?2",
        params![Sql(id), Side::B],
    )
    .and_then(|_| {
        tx.execute(
            "UPDATE events SET deleted = 1 WHERE match_id = ?1 AND side = ?2",
            params![Sql(id), Side::B],
        )
    })
    .map_err(sqlite)?;
    refresh(tx, id)
}

/// Updates the match `id` with `input`: team B changes, unless both teams
/// are scouted. The roster: the players left out go; number and role as in
/// `numbers`, else the ones added copy them from the team (D9) and the
/// others keep those of the match; empty, the active team.
fn update_match(tx: &Transaction, id: Uuid, input: MatchInput) -> Result<(), AppError> {
    let MatchInput {
        opponent,
        date,
        home,
        max_substitutions,
        notes,
        roster,
        numbers,
        friendly,
        competition,
        stage,
        round,
        // chosen at the creation, never changed
        scouting: _,
        venue_id,
    } = input;
    let rows = match_rows(tx, id)?.ok_or_else(|| not_found(&format!("match {id}")))?;
    let team = read_team(tx, &rows.row.team_a)?;
    let (team_b, _) = saved_team_b(tx, &team, &opponent)?;
    if rows.row.scouting == ScoutingMode::Both && team_b != rows.row.team_b {
        return Err(AppError::IO(IOError::Msg(format!(
            "match {id}: team B of a match scouted on both sides can't change"
        ))));
    }
    let played = rows.sets.iter().any(|s| !s.row.deleted);
    if !played {
        let teams: Vec<Uuid> = [Some(rows.row.team_a), team_b.team_id()]
            .into_iter()
            .flatten()
            .collect();
        check_match(tx, &teams, date, rows.row.duration_min)?;
    }
    let competition_id = saved_competition(tx, &team, competition.as_ref())?;
    let (team_b, team_b_name) = team_b.columns();
    tx.execute(
        "UPDATE matches SET team_b = ?2, team_b_name = ?3, start = ?4, home = ?5,
                 max_substitutions = ?6, notes = ?7, friendly = ?8, competition_id = ?9,
                 stage = ?10, round = ?11, venue_id = ?12
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
            round,
            venue_id.map(Sql)
        ],
    )
    .map_err(sqlite)?;
    let team_a = rows.row.team_a;
    let roster = match roster.as_slice() {
        [] => team.active_players().iter().map(|p| p.id).collect(),
        ids => ids.to_vec(),
    };
    let entries = roster
        .iter()
        .map(|person_id| {
            let given = numbers.iter().find(|n| n.player == *person_id);
            let saved = rows
                .roster
                .iter()
                .find(|r| r.team_id == team_a && r.person_id == *person_id);
            let player = team.find_player(*person_id);
            let (number, role) = match (given, saved, player) {
                (Some(n), _, _) => (n.number, n.role),
                (None, Some(r), _) => (r.number, r.role),
                (None, None, Some(p)) => (p.number, p.role),
                (None, None, None) => {
                    return Err(not_found(&format!("player {person_id} of team {team_a}")));
                }
            };
            Ok(RosterEntry {
                match_id: id,
                team_id: team_a,
                person_id: *person_id,
                number,
                role,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    // all written again: two players may swap their numbers, unique in
    // the roster
    tx.execute(
        "DELETE FROM match_roster WHERE match_id = ?1 AND team_id = ?2",
        params![Sql(id), Sql(team_a)],
    )
    .map_err(sqlite)?;
    entries
        .iter()
        .try_for_each(|entry| insert_roster_entry(tx, entry))?;
    // the roster of a team B no more in the match goes with it
    tx.execute(
        "DELETE FROM match_roster WHERE match_id = ?1 AND team_id NOT IN
             (SELECT team_a FROM matches WHERE id = ?1
              UNION SELECT team_b FROM matches WHERE id = ?1 AND team_b IS NOT NULL)",
        params![Sql(id)],
    )
    .map_err(sqlite)?;
    // the substitutions decide how the sets replay
    refresh(tx, id)
}

#[async_trait]
impl MatchWriter for SqliteMatchWriter {
    /// A match with a new id: two matches against the same opponent on the
    /// same day are two matches (no `MatchAlreadyExists`). Its roster
    /// takes number and role from `numbers`, else copies them from the
    /// team (D9); the whole active team when empty.
    async fn create(&self, team: &TeamEntry, input: MatchInput) -> Result<MatchEntry, AppError> {
        let id = Uuid::new_v4();
        let m = MatchEntry {
            source: Default::default(),
            video_clock: false,
            opponent: String::new(),
            date: input.date,
            id: id.to_string(),
            team: team
                .with_match_numbers(&input.numbers)
                .with_roster(&input.roster),
            home: input.home.is_a(),
            max_substitutions: input.max_substitutions,
            notes: input.notes,
            roster: input.roster,
            sets: vec![],
        };
        self.0.write(|tx| {
            let (team_b, b) = saved_team_b(tx, team, &input.opponent)?;
            let competition_id = saved_competition(tx, team, input.competition.as_ref())?;
            let teams: Vec<Uuid> = [Some(team.id), team_b.team_id()]
                .into_iter()
                .flatten()
                .collect();
            check_match(tx, &teams, input.date, None)?;
            let rows = MatchRows::of(&m, id, team_b, None)?;
            let rows = MatchRows {
                row: Match {
                    home: input.home,
                    friendly: input.friendly,
                    competition_id,
                    stage: input.stage,
                    round: input.round,
                    scouting: input.scouting,
                    venue_id: input.venue_id,
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

    async fn save_roster_of_b(
        &self,
        m: &MatchEntry,
        numbers: &[RosterNumber],
    ) -> Result<(), AppError> {
        let id = match_uuid(&m.id)?;
        self.0.write(|tx| {
            let rows = match_rows(tx, id)?.ok_or_else(|| not_found(&format!("match {id}")))?;
            let TeamRef::Team(b) = rows.row.team_b else {
                return Err(AppError::IO(IOError::Msg(format!(
                    "match {id}: team B is a name, without a roster"
                ))));
            };
            // whoever B has put on court, on the bench as libero or in a
            // touch stays on the roster: the check is made here, in the
            // transaction, whatever asks for it
            let used: HashSet<Uuid> = rows
                .sets
                .iter()
                .filter(|s| !s.row.deleted)
                .flat_map(|s| {
                    s.lineups
                        .iter()
                        .filter(|l| l.side == Side::B)
                        .flat_map(|l| {
                            l.positions
                                .iter()
                                .copied()
                                .chain([l.libero, l.setter])
                                .chain(l.fallback_libero)
                        })
                        .chain(
                            s.events
                                .iter()
                                .filter(|e| e.side == Side::B && !e.deleted)
                                .flat_map(|e| {
                                    e.entry.player.into_iter().chain(e.entry.target_player)
                                }),
                        )
                })
                .collect();
            if let Some(missing) = used
                .iter()
                .find(|p| !numbers.iter().any(|n| n.player == **p))
            {
                return Err(AppError::IO(IOError::Msg(format!(
                    "match {id}: player {missing} of team B played, it stays on the roster"
                ))));
            }
            tx.execute(
                "DELETE FROM match_roster WHERE match_id = ?1 AND team_id = ?2",
                params![Sql(id), Sql(b)],
            )
            .map_err(sqlite)?;
            numbers.iter().try_for_each(|n| {
                insert_roster_entry(
                    tx,
                    &RosterEntry {
                        match_id: id,
                        team_id: b,
                        person_id: n.player,
                        number: n.number,
                        role: n.role,
                    },
                )
            })
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

    async fn save_scouting(&self, m: &MatchEntry, scouting: ScoutingMode) -> Result<(), AppError> {
        let id = match_uuid(&m.id)?;
        self.0.write(|tx| {
            let changed = tx
                .execute(
                    "UPDATE matches SET scouting = ?2 WHERE id = ?1 AND deleted = 0",
                    params![Sql(id), scouting],
                )
                .map_err(sqlite)?;
            match (changed, scouting) {
                (0, _) => Err(not_found(&format!("match {}", m.id))),
                (_, ScoutingMode::A) => without_side_b(tx, id),
                (_, ScoutingMode::Both) => Ok(()),
            }
        })
    }

    async fn save_venue_named(
        &self,
        m: &MatchEntry,
        name: &str,
        city: &str,
    ) -> Result<(), AppError> {
        let id = match_uuid(&m.id)?;
        self.0.write(|tx| {
            let venue = find_or_create(tx, name, city)?;
            match tx
                .execute(
                    "UPDATE matches SET venue_id = ?2 WHERE id = ?1 AND deleted = 0",
                    params![Sql(id), Sql(venue.id)],
                )
                .map_err(sqlite)?
            {
                0 => Err(not_found(&format!("match {}", m.id))),
                _ => Ok(()),
            }
        })
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
