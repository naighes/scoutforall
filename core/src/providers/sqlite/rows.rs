//! The rows of `domain` read from and written to the tables.

use crate::{
    domain::{
        Event, Home, LegacyId, Match, MatchSet, Membership, Person, RosterEntry, SetLineup, Team,
        TeamRef,
        adapters::{MatchRows, SetRows, TeamRows},
        sql::{Sql, technique_from_db},
    },
    errors::AppError,
    providers::sqlite::{sqlite, summary::forget_team},
    shapes::{enums::EventTypeEnum, snapshot::EventEntry, team::TeamEntry},
};
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use rusqlite::{OptionalExtension, Row, Transaction, params};
use uuid::Uuid;

fn id(row: &Row, i: usize) -> rusqlite::Result<Uuid> {
    row.get::<_, Sql<Uuid>>(i).map(|id| id.0)
}

fn some_id(row: &Row, i: usize) -> rusqlite::Result<Option<Uuid>> {
    row.get::<_, Option<Sql<Uuid>>>(i)
        .map(|id| id.map(|id| id.0))
}

/// Whether a match of team `?1`, as A or as B, tells of the person in
/// `person`: its roster, a lineup of a set, an event of its side. The removed matches, sets and
/// events don't.
fn tells_of_sql(person: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM match_roster r JOIN matches m ON m.id = r.match_id
                 WHERE r.team_id = ?1 AND r.person_id = {person} AND m.deleted = 0)
         OR EXISTS (SELECT 1 FROM set_lineups l
                    JOIN matches m ON m.id = l.match_id
                    JOIN sets s ON s.match_id = l.match_id AND s.number = l.set_number
                    WHERE ((m.team_a = ?1 AND l.side = 'a') OR (m.team_b = ?1 AND l.side = 'b'))
                    AND m.deleted = 0 AND s.deleted = 0
                    AND {person} IN (l.p1, l.p2, l.p3, l.p4, l.p5, l.p6, l.libero,
                                     l.fallback_libero, l.setter))
         OR EXISTS (SELECT 1 FROM events e JOIN matches m ON m.id = e.match_id
                    WHERE ((m.team_a = ?1 AND e.side = 'a') OR (m.team_b = ?1 AND e.side = 'b'))
                    AND m.deleted = 0 AND e.deleted = 0
                    AND {person} IN (e.player, e.target_player))"
    )
}

pub(crate) fn tells_of(tx: &Transaction, team: Uuid, person: Uuid) -> Result<bool, AppError> {
    tx.query_row(
        &format!("SELECT {}", tells_of_sql("?2")),
        params![Sql(team), Sql(person)],
        |row| row.get(0),
    )
    .map_err(sqlite)
}

/// The team, deleted or not.
pub(crate) fn team_row(tx: &Transaction, team: Uuid) -> Result<Option<Team>, AppError> {
    tx.query_row(
        "SELECT id, name, year, classification, gender, country, deleted FROM teams
         WHERE id = ?1",
        [Sql(team)],
        |row| {
            Ok(Team {
                id: id(row, 0)?,
                name: row.get(1)?,
                year: row.get(2)?,
                classification: row.get(3)?,
                gender: row.get(4)?,
                country: row.get(5)?,
                deleted: row.get(6)?,
            })
        },
    )
    .optional()
    .map_err(sqlite)
}

/// The ids of the teams not removed, as written (an id that can't be read
/// is an unreadable team): by name, year, id.
pub(crate) fn team_ids(tx: &Transaction) -> Result<Vec<String>, AppError> {
    tx.prepare("SELECT id FROM teams WHERE deleted = 0 ORDER BY name, year, id")
        .and_then(|mut statement| {
            statement
                .query_map([], |row| row.get(0))?
                .collect::<Result<_, _>>()
        })
        .map_err(sqlite)
}

/// The team not removed, with its members in their order: the removed
/// ones only when a match tells of them.
pub(crate) fn team_rows(tx: &Transaction, team: Uuid) -> Result<Option<TeamRows>, AppError> {
    team_rows_where(
        tx,
        team,
        &format!("tm.deleted = 0 OR {}", tells_of_sql("tm.person_id")),
    )
}

/// The team not removed, with all its members in their order, also the
/// removed ones no match here tells of (one in an archive may).
pub(crate) fn whole_team_rows(tx: &Transaction, team: Uuid) -> Result<Option<TeamRows>, AppError> {
    team_rows_where(tx, team, "1")
}

/// The team not removed, with its members chosen by `members`.
fn team_rows_where(
    tx: &Transaction,
    team: Uuid,
    members: &str,
) -> Result<Option<TeamRows>, AppError> {
    let Some(row) = team_row(tx, team)?.filter(|t| !t.deleted) else {
        return Ok(None);
    };
    let sql = format!(
        "SELECT tm.person_id, tm.position, tm.number, tm.role, tm.deleted,
                p.name, p.birth_date, p.height_cm, p.notes, p.deleted
         FROM team_members tm JOIN persons p ON p.id = tm.person_id
         WHERE tm.team_id = ?1 AND ({members})
         ORDER BY tm.position"
    );
    let members: Vec<(Person, Membership)> = tx
        .prepare(&sql)
        .and_then(|mut statement| {
            statement
                .query_map([Sql(team)], |r| {
                    let person_id = id(r, 0)?;
                    Ok((
                        Person {
                            id: person_id,
                            name: r.get(5)?,
                            birth_date: r.get::<_, Option<Sql<NaiveDate>>>(6)?.map(|d| d.0),
                            height_cm: r.get(7)?,
                            notes: r.get(8)?,
                            deleted: r.get(9)?,
                        },
                        Membership {
                            team_id: team,
                            person_id,
                            position: r.get(1)?,
                            number: r.get(2)?,
                            role: r.get(3)?,
                            deleted: r.get(4)?,
                        },
                    ))
                })?
                .collect::<Result<_, _>>()
        })
        .map_err(sqlite)?;
    let (persons, members) = members.into_iter().unzip();
    Ok(Some(TeamRows {
        row,
        persons,
        members,
    }))
}

/// The team as `team` has it: its row, its players (persons and
/// memberships). A new member goes after the last one, a member keeps its
/// place; the members missing from `team` are removed. A removed player's
/// personal details aren't the team's to change: the person keeps them.
pub(crate) fn write_team(tx: &Transaction, team: &TeamEntry) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO teams (id, name, year, classification, gender, country)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (id) DO UPDATE SET name = excluded.name, year = excluded.year,
             classification = excluded.classification, gender = excluded.gender,
             country = excluded.country, deleted = 0",
        params![
            Sql(team.id),
            team.name,
            team.year,
            team.classification,
            team.gender,
            team.country
        ],
    )
    .map_err(sqlite)?;
    let saved: Vec<Uuid> = tx
        .prepare("SELECT person_id FROM team_members WHERE team_id = ?1")
        .and_then(|mut statement| {
            statement
                .query_map([Sql(team.id)], |row| id(row, 0))?
                .collect::<Result<_, _>>()
        })
        .map_err(sqlite)?;
    for player in &team.players {
        let person = params![
            Sql(player.id),
            player.name,
            player.birth_date.map(Sql),
            player.height_cm,
            player.notes
        ];
        let sql = match player.deleted {
            false => {
                "INSERT INTO persons (id, name, birth_date, height_cm, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (id) DO UPDATE SET name = excluded.name,
                     birth_date = excluded.birth_date, height_cm = excluded.height_cm,
                     notes = excluded.notes
                 WHERE persons.deleted = 0"
            }
            true => {
                "INSERT INTO persons (id, name, birth_date, height_cm, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT (id) DO NOTHING"
            }
        };
        tx.execute(sql, person).map_err(sqlite)?;
        // a person removed from the registry (D16) is in no team
        tx.execute(
            "INSERT INTO team_members (team_id, person_id, position, number, role, deleted)
             VALUES (?1, ?2,
                     (SELECT coalesce(max(position) + 1, 0) FROM team_members WHERE team_id = ?1),
                     ?3, ?4, ?5 OR (SELECT deleted FROM persons WHERE id = ?2))
             ON CONFLICT (team_id, person_id) DO UPDATE SET number = excluded.number,
                 role = excluded.role, deleted = excluded.deleted",
            params![
                Sql(team.id),
                Sql(player.id),
                player.number,
                player.role,
                player.deleted
            ],
        )
        .map_err(sqlite)?;
    }
    for missing in saved
        .iter()
        .filter(|person| team.find_player(**person).is_none())
    {
        tx.execute(
            "UPDATE team_members SET deleted = 1 WHERE team_id = ?1 AND person_id = ?2",
            params![Sql(team.id), Sql(*missing)],
        )
        .map_err(sqlite)?;
    }
    // as `team.json` did with the index of the files
    forget_team(tx, team.id)
}

/// The ids of the matches of team A not removed, as written.
pub(crate) fn match_ids(tx: &Transaction, team: Uuid) -> Result<Vec<String>, AppError> {
    tx.prepare("SELECT id FROM matches WHERE team_a = ?1 AND deleted = 0 ORDER BY start, id")
        .and_then(|mut statement| {
            statement
                .query_map([Sql(team)], |row| row.get(0))?
                .collect::<Result<_, _>>()
        })
        .map_err(sqlite)
}

/// The matches not removed in which `team` is B: id, team A and court.
pub(crate) fn matches_as_b(
    tx: &Transaction,
    team: Uuid,
) -> Result<Vec<(String, Uuid, Home)>, AppError> {
    tx.prepare(
        "SELECT id, team_a, home FROM matches WHERE team_b = ?1 AND deleted = 0
         ORDER BY start, id",
    )
    .and_then(|mut statement| {
        statement
            .query_map([Sql(team)], |row| {
                Ok((row.get(0)?, id(row, 1)?, row.get::<_, Home>(2)?))
            })?
            .collect::<Result<_, _>>()
    })
    .map_err(sqlite)
}

/// The match not removed, with its roster and its sets not removed.
pub(crate) fn match_rows(tx: &Transaction, match_id: Uuid) -> Result<Option<MatchRows>, AppError> {
    let row = tx
        .query_row(
            "SELECT id, team_a, team_b, team_b_name, home, scouting, start, duration_min,
                    friendly, competition_id, stage, round, max_substitutions, notes, source,
                    video_clock, legacy_id, legacy_team, deleted
             FROM matches WHERE id = ?1 AND deleted = 0",
            [Sql(match_id)],
            |r| {
                let legacy = match (r.get::<_, Option<String>>(16)?, some_id(r, 17)?) {
                    (Some(id), Some(team)) => Some(LegacyId { id, team }),
                    _ => None,
                };
                Ok((
                    some_id(r, 2)?,
                    r.get::<_, Option<String>>(3)?,
                    Match {
                        id: id(r, 0)?,
                        team_a: id(r, 1)?,
                        team_b: TeamRef::Name(String::new()),
                        home: r.get(4)?,
                        scouting: r.get(5)?,
                        start: r.get::<_, Sql<DateTime<FixedOffset>>>(6)?.0,
                        duration_min: r.get(7)?,
                        friendly: r.get(8)?,
                        competition_id: some_id(r, 9)?,
                        stage: r.get(10)?,
                        round: r.get(11)?,
                        max_substitutions: r.get(12)?,
                        notes: r.get(13)?,
                        source: r.get(14)?,
                        video_clock: r.get(15)?,
                        legacy,
                        deleted: r.get(18)?,
                    },
                ))
            },
        )
        .optional()
        .map_err(sqlite)?;
    let Some((team_b, team_b_name, row)) = row else {
        return Ok(None);
    };
    let row = Match {
        team_b: TeamRef::from_columns(team_b, team_b_name)?,
        ..row
    };
    let roster = tx
        .prepare(
            "SELECT team_id, person_id, number, role FROM match_roster WHERE match_id = ?1
             ORDER BY team_id, number",
        )
        .and_then(|mut statement| {
            statement
                .query_map([Sql(match_id)], |r| {
                    Ok(RosterEntry {
                        match_id,
                        team_id: id(r, 0)?,
                        person_id: id(r, 1)?,
                        number: r.get(2)?,
                        role: r.get(3)?,
                    })
                })?
                .collect::<Result<_, _>>()
        })
        .map_err(sqlite)?;
    let sets = set_rows(tx, match_id)?;
    Ok(Some(MatchRows { row, roster, sets }))
}

/// The sets of the match not removed, in order, with their lineups and
/// their events not removed.
pub(crate) fn set_rows(tx: &Transaction, match_id: Uuid) -> Result<Vec<SetRows>, AppError> {
    let sets: Vec<MatchSet> = tx
        .prepare(
            "SELECT number, serving, lineup_rules, deleted FROM sets
             WHERE match_id = ?1 AND deleted = 0 ORDER BY number",
        )
        .and_then(|mut statement| {
            statement
                .query_map([Sql(match_id)], |r| {
                    Ok(MatchSet {
                        match_id,
                        number: r.get(0)?,
                        serving: r.get(1)?,
                        lineup_rules: r.get(2)?,
                        deleted: r.get(3)?,
                    })
                })?
                .collect::<Result<_, _>>()
        })
        .map_err(sqlite)?;
    sets.into_iter()
        .map(|row| {
            Ok(SetRows {
                lineups: lineups(tx, match_id, row.number)?,
                events: events(tx, match_id, row.number)?,
                row,
            })
        })
        .collect()
}

fn lineups(tx: &Transaction, match_id: Uuid, set_number: u8) -> Result<Vec<SetLineup>, AppError> {
    tx.prepare(
        "SELECT side, p1, p2, p3, p4, p5, p6, libero, fallback_libero, setter
         FROM set_lineups WHERE match_id = ?1 AND set_number = ?2 ORDER BY side",
    )
    .and_then(|mut statement| {
        statement
            .query_map(params![Sql(match_id), set_number], |r| {
                Ok(SetLineup {
                    match_id,
                    set_number,
                    side: r.get(0)?,
                    positions: [
                        id(r, 1)?,
                        id(r, 2)?,
                        id(r, 3)?,
                        id(r, 4)?,
                        id(r, 5)?,
                        id(r, 6)?,
                    ],
                    libero: id(r, 7)?,
                    fallback_libero: some_id(r, 8)?,
                    setter: id(r, 9)?,
                })
            })?
            .collect::<Result<_, _>>()
    })
    .map_err(sqlite)
}

/// The events of the set not removed, in their order.
pub(crate) fn events(
    tx: &Transaction,
    match_id: Uuid,
    set_number: u8,
) -> Result<Vec<Event>, AppError> {
    let rows: Vec<(Event, Option<String>)> = tx
        .prepare(
            "SELECT id, seq, side, timestamp, event_type, player, eval, target_player,
                    technique, flags, deleted
             FROM events WHERE match_id = ?1 AND set_number = ?2 AND deleted = 0
             ORDER BY seq",
        )
        .and_then(|mut statement| {
            statement
                .query_map(params![Sql(match_id), set_number], |r| {
                    let event_type: EventTypeEnum = r.get(4)?;
                    Ok((
                        Event {
                            id: id(r, 0)?,
                            match_id,
                            set_number,
                            seq: r.get(1)?,
                            side: r.get(2)?,
                            entry: EventEntry {
                                timestamp: r.get::<_, Sql<DateTime<Utc>>>(3)?.0,
                                event_type,
                                player: some_id(r, 5)?,
                                eval: r.get(6)?,
                                target_player: some_id(r, 7)?,
                                technique: None,
                                flags: r.get(9)?,
                            },
                            deleted: r.get(10)?,
                        },
                        r.get(8)?,
                    ))
                })?
                .collect::<Result<_, _>>()
        })
        .map_err(sqlite)?;
    rows.into_iter()
        .map(|(event, technique)| {
            let technique = technique_from_db(event.entry.event_type, technique.as_deref())?;
            Ok(Event {
                entry: EventEntry {
                    technique,
                    ..event.entry
                },
                ..event
            })
        })
        .collect()
}

/// A new match with its roster and its sets.
pub(crate) fn insert_match(tx: &Transaction, rows: &MatchRows) -> Result<(), AppError> {
    let m = &rows.row;
    let (team_b, team_b_name) = m.team_b.columns();
    tx.execute(
        "INSERT INTO matches (id, team_a, team_b, team_b_name, home, scouting, start,
             duration_min, friendly, competition_id, stage, round, max_substitutions, notes,
             source, video_clock, legacy_id, legacy_team, deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
             ?18, ?19)",
        params![
            Sql(m.id),
            Sql(m.team_a),
            team_b.map(Sql),
            team_b_name,
            m.home,
            m.scouting,
            Sql(m.start),
            m.duration_min,
            m.friendly,
            m.competition_id.map(Sql),
            m.stage,
            m.round,
            m.max_substitutions,
            m.notes,
            m.source,
            m.video_clock,
            m.legacy.as_ref().map(|l| l.id.as_str()),
            m.legacy.as_ref().map(|l| Sql(l.team)),
            m.deleted
        ],
    )
    .map_err(sqlite)?;
    rows.roster
        .iter()
        .try_for_each(|entry| insert_roster_entry(tx, entry))?;
    rows.sets.iter().try_for_each(|set| {
        write_set(tx, set)?;
        set.events
            .iter()
            .try_for_each(|event| insert_event(tx, event))
    })
}

pub(crate) fn insert_roster_entry(tx: &Transaction, entry: &RosterEntry) -> Result<(), AppError> {
    tx.execute(
        "INSERT INTO match_roster (match_id, team_id, person_id, number, role)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            Sql(entry.match_id),
            Sql(entry.team_id),
            Sql(entry.person_id),
            entry.number,
            entry.role
        ],
    )
    .map(|_| ())
    .map_err(sqlite)
}

/// The set and its lineups, in place of those saved (a removed set comes
/// back); not its events.
pub(crate) fn write_set(tx: &Transaction, set: &SetRows) -> Result<(), AppError> {
    let row = &set.row;
    tx.execute(
        "INSERT INTO sets (match_id, number, serving, lineup_rules, deleted)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (match_id, number) DO UPDATE SET serving = excluded.serving,
             lineup_rules = excluded.lineup_rules, deleted = excluded.deleted",
        params![
            Sql(row.match_id),
            row.number,
            row.serving,
            row.lineup_rules,
            row.deleted
        ],
    )
    .map_err(sqlite)?;
    tx.execute(
        "DELETE FROM set_lineups WHERE match_id = ?1 AND set_number = ?2",
        params![Sql(row.match_id), row.number],
    )
    .map_err(sqlite)?;
    set.lineups.iter().try_for_each(|l| {
        let [p1, p2, p3, p4, p5, p6] = l.positions;
        tx.execute(
            "INSERT INTO set_lineups (match_id, set_number, side, p1, p2, p3, p4, p5, p6,
                 libero, fallback_libero, setter)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                Sql(l.match_id),
                l.set_number,
                l.side,
                Sql(p1),
                Sql(p2),
                Sql(p3),
                Sql(p4),
                Sql(p5),
                Sql(p6),
                Sql(l.libero),
                l.fallback_libero.map(Sql),
                Sql(l.setter)
            ],
        )
        .map(|_| ())
        .map_err(sqlite)
    })
}

/// The place after the last event of the set, removed ones included: an
/// undone event keeps its place.
pub(crate) fn next_seq(tx: &Transaction, match_id: Uuid, set_number: u8) -> Result<u32, AppError> {
    tx.query_row(
        "SELECT coalesce(max(seq) + 1, 0) FROM events WHERE match_id = ?1 AND set_number = ?2",
        params![Sql(match_id), set_number],
        |row| row.get(0),
    )
    .map_err(sqlite)
}

pub(crate) fn insert_event(tx: &Transaction, event: &Event) -> Result<(), AppError> {
    let e = &event.entry;
    tx.execute(
        "INSERT INTO events (id, match_id, set_number, seq, side, timestamp, event_type,
             player, eval, target_player, technique, flags, deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            Sql(event.id),
            Sql(event.match_id),
            event.set_number,
            event.seq,
            event.side,
            Sql(e.timestamp),
            e.event_type,
            e.player.map(Sql),
            e.eval,
            e.target_player.map(Sql),
            e.technique,
            e.flags,
            event.deleted
        ],
    )
    .map(|_| ())
    .map_err(sqlite)
}

/// The event as `e` has it now: a correction updates its row (TODO/001,
/// default choices; append-only comes back with the sync, 030).
pub(crate) fn update_event(tx: &Transaction, id: Uuid, e: &EventEntry) -> Result<(), AppError> {
    tx.execute(
        "UPDATE events SET timestamp = ?2, event_type = ?3, player = ?4, eval = ?5,
             target_player = ?6, technique = ?7, flags = ?8
         WHERE id = ?1",
        params![
            Sql(id),
            Sql(e.timestamp),
            e.event_type,
            e.player.map(Sql),
            e.eval,
            e.target_player.map(Sql),
            e.technique,
            e.flags
        ],
    )
    .map(|_| ())
    .map_err(sqlite)
}

pub(crate) fn remove_event(tx: &Transaction, id: Uuid) -> Result<(), AppError> {
    tx.execute("UPDATE events SET deleted = 1 WHERE id = ?1", [Sql(id)])
        .map(|_| ())
        .map_err(sqlite)
}
