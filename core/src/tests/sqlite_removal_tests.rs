//! Removals on the database (TODO/001, step 16, D16): a team takes with it
//! only its matches against a name, the others stay to the other team (B
//! becoming A when the removed one was A); a person removed from the
//! registry keeps only its id, and the matches played keep them as an
//! anonymous player; a match is removed by either team, with its video.

#[cfg(test)]
mod tests {
    use crate::{
        analytics::queue::{PendingUpload, UploadQueue},
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        domain::sql::Sql,
        legacy::migrate::unzip,
        localization::Language,
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            queue_reader::QueueReader,
            queue_writer::QueueWriter,
            set_writer::SetWriter,
            sqlite::{
                Database,
                archive::{Subject, export},
                matches::{SqliteMatchReader, SqliteMatchWriter},
                persons::SqlitePersons,
                queue::SqliteQueue,
                sets::SqliteSetWriter,
                sqlite,
                teams::{SqliteTeamReader, SqliteTeamWriter, TeamRemoval},
                video::SqliteVideoProfiles,
            },
            team_reader::TeamReader,
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{
            enums::{EvalEnum, EventTypeEnum, RoleEnum, TeamSideEnum},
            r#match::MatchEntry,
            player::PlayerEntry,
            snapshot::EventEntry,
            team::TeamEntry,
            technique::{ServeTypeEnum, TechniqueEnum},
        },
        video::{EventRef, StoredProfile, VideoFile, VideoProfile, VideoProfiles},
    };
    use chrono::Utc;
    use rusqlite::params;
    use serde_json::Value as Json;
    use uuid::Uuid;

    struct P {
        db: Database,
        teams: SqliteTeamReader,
        team_writer: SqliteTeamWriter,
        matches: SqliteMatchReader,
        match_writer: SqliteMatchWriter,
        set_writer: SqliteSetWriter,
        queue: SqliteQueue,
        videos: SqliteVideoProfiles,
        persons: SqlitePersons,
    }

    fn providers() -> P {
        let db = Database::new(crate::db::open_in_memory().unwrap());
        let language = Language::default();
        P {
            teams: SqliteTeamReader::new(db.clone()),
            team_writer: SqliteTeamWriter::new(db.clone()),
            matches: SqliteMatchReader::new(db.clone(), language.clone()),
            match_writer: SqliteMatchWriter::new(db.clone()),
            set_writer: SqliteSetWriter::new(db.clone(), language),
            queue: SqliteQueue::new(db.clone()),
            videos: SqliteVideoProfiles(db.clone()),
            persons: SqlitePersons::new(db.clone()),
            db,
        }
    }

    fn players(team: &str) -> Vec<PlayerEntry> {
        use RoleEnum::*;
        [
            Setter,
            OutsideHitter,
            MiddleBlocker,
            OppositeHitter,
            OutsideHitter,
            MiddleBlocker,
            Libero,
        ]
        .into_iter()
        .zip(1..)
        .map(|(role, n)| PlayerEntry {
            id: Uuid::new_v4(),
            name: format!("{team} {n}"),
            role: Some(role),
            number: n,
            deleted: false,
            birth_date: chrono::NaiveDate::from_ymd_opt(2008, 3, n.into()),
            height_cm: Some(180),
            notes: "notes".into(),
        })
        .collect()
    }

    async fn team(p: &P, name: &str, players: Vec<PlayerEntry>) -> TeamEntry {
        p.team_writer
            .save(TeamInput::New {
                id: None,
                name: name.into(),
                year: 2026,
                classification: None,
                gender: None,
                country: None,
                players,
            })
            .await
            .unwrap()
    }

    fn serve(player: Uuid) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type: EventTypeEnum::S,
            player: Some(player),
            eval: Some(EvalEnum::Positive),
            target_player: None,
            technique: Some(TechniqueEnum::Serve(ServeTypeEnum::Float)),
            flags: Default::default(),
        }
    }

    fn their_error() -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type: EventTypeEnum::OE,
            player: None,
            eval: None,
            target_player: None,
            technique: None,
            flags: Default::default(),
        }
    }

    /// A match of `team` against a name, at home, with a set: A serves and
    /// B misses.
    async fn scouted(p: &P, team: &TeamEntry) -> MatchEntry {
        let ids: Vec<Uuid> = team.players.iter().map(|p| p.id).collect();
        let m = p
            .match_writer
            .create(
                team,
                "Cortona Volley".into(),
                Utc::now().fixed_offset(),
                true,
                DEFAULT_MAX_SUBSTITUTIONS,
                String::new(),
                vec![],
            )
            .await
            .unwrap();
        p.set_writer
            .create(
                &m,
                1,
                TeamSideEnum::Us,
                ids[..6].try_into().unwrap(),
                ids[6],
                None,
                ids[0],
                vec![serve(ids[0]), their_error()],
            )
            .await
            .unwrap();
        p.matches.read_single(team, &m.id).await.unwrap()
    }

    fn execute(db: &Database, sql: &str) {
        db.write(|tx| tx.execute_batch(sql).map_err(sqlite))
            .unwrap()
    }

    fn count(db: &Database, sql: &str) -> i64 {
        db.read(|tx| tx.query_row(sql, [], |r| r.get(0)).map_err(sqlite))
            .unwrap()
    }

    /// Team `b` plays the match as B; with `both`, B is scouted too: its
    /// roster, its lineup of the set and its reception error (B's touches
    /// written by hand: the projection of D8 doesn't exist yet).
    fn against(db: &Database, m: &MatchEntry, b: &TeamEntry, both: bool) {
        let scouting = match both {
            true => "both",
            false => "a",
        };
        execute(
            db,
            &format!(
                "UPDATE matches SET team_b = '{}', team_b_name = NULL, scouting = '{scouting}'
                 WHERE id = '{}'",
                b.id, m.id
            ),
        );
        if !both {
            return;
        }
        let ids: Vec<Uuid> = b.players.iter().map(|p| p.id).collect();
        db.write(|tx| {
            let run = |r: rusqlite::Result<usize>| r.map(|_| ()).map_err(sqlite);
            b.players.iter().try_for_each(|player| {
                run(tx.execute(
                    "INSERT INTO match_roster (match_id, team_id, person_id, number, role)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![m.id, Sql(b.id), Sql(player.id), player.number, player.role],
                ))
            })?;
            run(tx.execute(
                "INSERT INTO set_lineups (match_id, set_number, side, p1, p2, p3, p4, p5, p6,
                     libero, fallback_libero, setter)
                 VALUES (?1, 1, 'b', ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?2)",
                params![
                    m.id,
                    Sql(ids[0]),
                    Sql(ids[1]),
                    Sql(ids[2]),
                    Sql(ids[3]),
                    Sql(ids[4]),
                    Sql(ids[5]),
                    Sql(ids[6])
                ],
            ))?;
            run(tx.execute(
                "INSERT INTO events (id, match_id, set_number, seq, side, timestamp,
                     event_type, player, eval)
                 VALUES (?1, ?2, 1, 2, 'b', ?3, 'P', ?4, '=')",
                params![Sql(Uuid::new_v4()), m.id, Sql(Utc::now()), Sql(ids[1])],
            ))
        })
        .unwrap()
    }

    fn video() -> VideoProfile {
        VideoProfile {
            files: vec![VideoFile {
                path: "videos/match.mp4".into(),
                start: None,
            }],
            ..Default::default()
        }
        .corrected(
            EventRef {
                set_number: 1,
                index: 1,
            },
            500,
        )
    }

    fn video_rows(db: &Database) -> (i64, i64, i64) {
        (
            count(db, "SELECT count(*) FROM match_videos"),
            count(db, "SELECT count(*) FROM match_video_files"),
            count(db, "SELECT count(*) FROM match_video_corrections"),
        )
    }

    async fn queue(p: &P, matches: &[&MatchEntry], team: &TeamEntry) {
        p.queue
            .save(&UploadQueue::of(
                matches
                    .iter()
                    .map(|m| PendingUpload {
                        team_id: team.id,
                        match_id: m.id.clone(),
                        queued_at: 1,
                        retry_count: 0,
                    })
                    .collect(),
            ))
            .await
            .unwrap();
    }

    /// The set as replayed: the events, the score, who served first.
    fn replayed(m: &MatchEntry) -> Vec<(Vec<EventTypeEnum>, u8, u8, TeamSideEnum)> {
        m.sets
            .iter()
            .map(|set| {
                let snapshot = set.compute_snapshot(m.max_substitutions).unwrap();
                (
                    set.events.iter().map(|e| e.event_type).collect(),
                    snapshot.score_us,
                    snapshot.score_them,
                    set.serving_team,
                )
            })
            .collect()
    }

    /// What a removal may change, table by table.
    fn state(db: &Database) -> Vec<String> {
        [
            "SELECT id, name, deleted FROM teams ORDER BY id",
            "SELECT id, name, birth_date, height_cm, notes, deleted FROM persons ORDER BY id",
            "SELECT team_id, person_id, deleted FROM team_members ORDER BY team_id, person_id",
            "SELECT id, team_a, team_b, team_b_name, home, scouting, deleted FROM matches
             ORDER BY id",
            "SELECT match_id, team_id, person_id, number FROM match_roster
             ORDER BY match_id, team_id, person_id",
            "SELECT match_id, number, serving, deleted FROM sets ORDER BY match_id, number",
            "SELECT match_id, set_number, side, p1 FROM set_lineups
             ORDER BY match_id, set_number, side",
            "SELECT id, side, deleted FROM events ORDER BY id",
            "SELECT match_id FROM upload_queue ORDER BY position",
            "SELECT match_id FROM match_videos ORDER BY match_id",
            "SELECT match_id FROM match_summary ORDER BY match_id",
        ]
        .iter()
        .map(|sql| {
            db.read(|tx| {
                let mut statement = tx.prepare(sql).map_err(sqlite)?;
                let columns = statement.column_count();
                statement
                    .query_map([], |row| {
                        (0..columns)
                            .map(|i| {
                                row.get::<_, rusqlite::types::Value>(i)
                                    .map(|v| format!("{v:?}"))
                            })
                            .collect::<Result<Vec<_>, _>>()
                            .map(|v| v.join("|"))
                    })
                    .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
                    .map(|rows| format!("{sql}\n{}", rows.join("\n")))
                    .map_err(sqlite)
            })
            .unwrap()
        })
        .collect()
    }

    fn ids(matches: &[crate::shapes::listed::Listed<MatchEntry>]) -> Vec<String> {
        matches
            .iter()
            .map(|m| m.as_ref().unwrap().id.clone())
            .collect()
    }

    /// The confirmation tells how many matches stay to the other team and
    /// how many go: those against a name (or a removed team), not those
    /// removed already, not those of other teams.
    #[tokio::test]
    async fn the_confirmation_counts_the_matches() {
        let p = providers();
        let a = team(&p, "A", players("a")).await;
        let b = team(&p, "B", players("b")).await;
        let c = team(&p, "C", players("c")).await;
        assert_eq!(p.teams.removal(&a.id).unwrap(), TeamRemoval::default());
        let _against_a_name = scouted(&p, &a).await;
        let removed_already = scouted(&p, &a).await;
        p.match_writer
            .remove(&a.id, &removed_already.id)
            .await
            .unwrap();
        let against_b = scouted(&p, &a).await;
        against(&p.db, &against_b, &b, false);
        let as_b = scouted(&p, &c).await;
        against(&p.db, &as_b, &a, true);
        let against_a_removed_team = scouted(&p, &a).await;
        let gone = team(&p, "gone", vec![]).await;
        against(&p.db, &against_a_removed_team, &gone, false);
        execute(
            &p.db,
            &format!("UPDATE teams SET deleted = 1 WHERE id = '{}'", gone.id),
        );
        let _of_c = scouted(&p, &c).await;

        assert_eq!(
            p.teams.removal(&a.id).unwrap(),
            TeamRemoval {
                kept: 2,
                removed: 2
            }
        );
        assert_eq!(
            p.teams.removal(&b.id).unwrap(),
            TeamRemoval {
                kept: 1,
                removed: 0
            }
        );
        assert_eq!(
            p.teams.removal(&c.id).unwrap(),
            TeamRemoval {
                kept: 1,
                removed: 1
            }
        );
        assert_eq!(
            p.teams.removal(&Uuid::new_v4()).unwrap(),
            TeamRemoval::default()
        );

        // and the removal does what the confirmation said
        p.team_writer.remove(&a.id).await.unwrap();
        assert_eq!(
            count(&p.db, "SELECT count(*) FROM matches WHERE deleted = 0"),
            3
        );
        assert_eq!(p.matches.read_all(&b).await.unwrap().len(), 1);
        assert_eq!(p.matches.read_all(&c).await.unwrap().len(), 2);
    }

    /// The matches against a name go with the team, their video with them;
    /// the queue lets them drop when uploading, as a match removed.
    #[tokio::test]
    async fn the_matches_against_a_name_go_with_the_team() {
        let p = providers();
        let a = team(&p, "A", players("a")).await;
        let other = team(&p, "other", players("o")).await;
        let m = scouted(&p, &a).await;
        let theirs = scouted(&p, &other).await;
        p.videos.write(&a.id, &m.id, &video()).await.unwrap();
        p.videos
            .write(&other.id, &theirs.id, &video())
            .await
            .unwrap();
        assert_eq!(video_rows(&p.db), (2, 2, 2));

        p.team_writer.remove(&a.id).await.unwrap();
        assert!(!p.teams.exists(&a.id).await.unwrap());
        assert_eq!(
            count(
                &p.db,
                &format!("SELECT deleted FROM matches WHERE id = '{}'", m.id)
            ),
            1
        );
        assert_eq!(video_rows(&p.db), (1, 1, 1));
        assert_eq!(
            count(
                &p.db,
                &format!(
                    "SELECT count(*) FROM match_summary WHERE match_id = '{}'",
                    m.id
                )
            ),
            0
        );
        let left = p.matches.read_all(&other).await.unwrap();
        assert_eq!(ids(&left), vec![theirs.id.clone()]);
        assert_eq!(
            p.videos.read(&other.id, &theirs.id).await.unwrap(),
            StoredProfile::Profile(video())
        );
        // removed again, or a team that isn't there: nothing happens
        let before = state(&p.db);
        p.team_writer.remove(&a.id).await.unwrap();
        p.team_writer.remove(&Uuid::new_v4()).await.unwrap();
        assert_eq!(state(&p.db), before);
    }

    /// Team B removed: the match stays to A against B's name, scouted
    /// only on A's side: B's touches, lineups and roster are lost, A's
    /// replay doesn't change, nor its video or its upload.
    #[tokio::test]
    async fn a_team_b_removed_becomes_its_name() {
        let p = providers();
        let a = team(&p, "A", players("a")).await;
        let b = team(&p, "Volley Pisa", players("b")).await;
        let m = scouted(&p, &a).await;
        against(&p.db, &m, &b, true);
        p.videos.write(&a.id, &m.id, &video()).await.unwrap();
        queue(&p, &[&m], &a).await;
        let before = p.matches.read_single(&a, &m.id).await.unwrap();
        assert_eq!(before.opponent, "Volley Pisa");
        let summaries = p.matches.read_summaries(&a).await.unwrap();

        p.team_writer.remove(&b.id).await.unwrap();
        let after = p.matches.read_single(&a, &m.id).await.unwrap();
        assert_eq!(after, before);
        assert_eq!(
            replayed(&after),
            vec![(
                vec![EventTypeEnum::S, EventTypeEnum::OE],
                1,
                0,
                TeamSideEnum::Us
            )]
        );
        assert_eq!(p.matches.read_summaries(&a).await.unwrap(), summaries);
        let id = &m.id;
        for (sql, expected) in [
            (
                format!(
                    "SELECT count(*) FROM matches WHERE id = '{id}' AND team_a = '{}'
                     AND team_b IS NULL AND team_b_name = 'Volley Pisa' AND scouting = 'a'
                     AND home = 'a' AND deleted = 0",
                    a.id
                ),
                1,
            ),
            (
                format!("SELECT count(*) FROM events WHERE match_id = '{id}' AND side = 'b'"),
                1,
            ),
            (
                format!(
                    "SELECT count(*) FROM events WHERE match_id = '{id}' AND side = 'b'
                     AND deleted = 0"
                ),
                0,
            ),
            (
                format!("SELECT count(*) FROM events WHERE match_id = '{id}' AND deleted = 0"),
                2,
            ),
            (
                format!("SELECT count(*) FROM set_lineups WHERE match_id = '{id}'"),
                1,
            ),
            (
                format!(
                    "SELECT count(*) FROM match_roster WHERE match_id = '{id}'
                     AND team_id = '{}'",
                    b.id
                ),
                0,
            ),
            (
                format!("SELECT count(*) FROM match_roster WHERE match_id = '{id}'"),
                7,
            ),
            (
                format!("SELECT count(*) FROM match_summary WHERE match_id = '{id}'"),
                1,
            ),
        ] {
            assert_eq!(count(&p.db, &sql), expected, "{sql}");
        }
        assert_eq!(video_rows(&p.db), (1, 1, 1));
        let queued = p.queue.load().await.unwrap();
        assert_eq!(queued.pending().len(), 1);
        assert_eq!(queued.pending()[0].team_id, a.id);
        // B's persons stay in the registry
        assert_eq!(
            count(
                &p.db,
                "SELECT count(*) FROM persons WHERE name LIKE 'b %' AND deleted = 0"
            ),
            7
        );
    }

    /// Team A removed, only A scouted: B becomes A against A's name, the
    /// court seen from B; what A scouted is lost (D16, decided at step
    /// 16), so the match is played and not scouted, no longer waiting to
    /// be uploaded. The video stays with the match.
    #[tokio::test]
    async fn a_team_a_removed_leaves_the_match_to_b() {
        let p = providers();
        let a = team(&p, "Cortona", players("a")).await;
        let b = team(&p, "B", players("b")).await;
        let m = scouted(&p, &a).await;
        let other = scouted(&p, &b).await;
        against(&p.db, &m, &b, false);
        p.videos.write(&a.id, &m.id, &video()).await.unwrap();
        queue(&p, &[&m, &other], &a).await;
        assert!(p.matches.read_all(&b).await.unwrap().len() == 1);

        p.team_writer.remove(&a.id).await.unwrap();
        let left = p.matches.read_all(&b).await.unwrap();
        let mut expected = vec![m.id.clone(), other.id.clone()];
        expected.sort_by_key(|id| (id != &m.id, id.clone()));
        let mut found = ids(&left);
        found.sort_by_key(|id| (id != &m.id, id.clone()));
        assert_eq!(found, expected);
        let back = p.matches.read_single(&b, &m.id).await.unwrap();
        assert_eq!(back.opponent, "Cortona");
        assert!(!back.home);
        assert!(back.sets.is_empty());
        assert_eq!(back.date, m.date);
        assert!(back.team.players.iter().all(|p| p.name.starts_with("b ")));
        let summary = p
            .matches
            .read_summaries(&b)
            .await
            .unwrap()
            .into_iter()
            .flatten()
            .find(|s| s.id == m.id)
            .unwrap();
        assert_eq!(
            (
                summary.sets,
                summary.us_wins,
                summary.them_wins,
                summary.opponent.as_str()
            ),
            (0, 0, 0, "Cortona")
        );
        let id = &m.id;
        for (sql, expected) in [
            (
                format!(
                    "SELECT count(*) FROM matches WHERE id = '{id}' AND team_a = '{}'
                     AND team_b IS NULL AND team_b_name = 'Cortona' AND home = 'b'
                     AND scouting = 'a'",
                    b.id
                ),
                1,
            ),
            // A's touches, now on side b, lost
            (
                format!(
                    "SELECT count(*) FROM events WHERE match_id = '{id}' AND side = 'b'
                     AND deleted = 1"
                ),
                2,
            ),
            (
                format!("SELECT count(*) FROM events WHERE match_id = '{id}'"),
                2,
            ),
            (
                format!(
                    "SELECT count(*) FROM sets WHERE match_id = '{id}' AND deleted = 1
                     AND serving = 'b'"
                ),
                1,
            ),
            (
                format!("SELECT count(*) FROM set_lineups WHERE match_id = '{id}'"),
                0,
            ),
            (
                format!("SELECT count(*) FROM match_roster WHERE match_id = '{id}'"),
                0,
            ),
            (
                format!("SELECT legacy_id IS NULL FROM matches WHERE id = '{id}'"),
                1,
            ),
        ] {
            assert_eq!(count(&p.db, &sql), expected, "{sql}");
        }
        let queued = p.queue.load().await.unwrap();
        let queued: Vec<&str> = queued
            .pending()
            .iter()
            .map(|q| q.match_id.as_str())
            .collect();
        assert_eq!(queued, vec![other.id.as_str()]);
        assert_eq!(video_rows(&p.db), (1, 1, 1));
        // the correction was of an event of A: no longer read
        assert_eq!(
            p.videos.read(&b.id, &m.id).await.unwrap(),
            StoredProfile::Profile(VideoProfile {
                corrections: vec![],
                ..video()
            })
        );
        // the persons of A stay, with their memberships
        assert_eq!(
            count(
                &p.db,
                &format!(
                    "SELECT count(*) FROM team_members WHERE team_id = '{}' AND deleted = 0",
                    a.id
                )
            ),
            7
        );
        assert_eq!(
            count(
                &p.db,
                "SELECT count(*) FROM persons WHERE name LIKE 'a %' AND deleted = 0
                 AND notes = 'notes'"
            ),
            7
        );
    }

    /// Team A removed, both scouted: B's touches become A's, with its
    /// lineup and its roster, and replay from B's side.
    #[tokio::test]
    async fn with_both_scouted_b_keeps_its_touches() {
        let p = providers();
        let a = team(&p, "A", players("a")).await;
        let b = team(&p, "B", players("b")).await;
        let m = scouted(&p, &a).await;
        against(&p.db, &m, &b, true);
        // a second set without B's lineup: nothing of B, lost
        let ids: Vec<Uuid> = a.players.iter().map(|p| p.id).collect();
        let first = p.matches.read_single(&a, &m.id).await.unwrap();
        p.set_writer
            .create(
                &first,
                2,
                TeamSideEnum::Them,
                ids[..6].try_into().unwrap(),
                ids[6],
                None,
                ids[0],
                vec![their_error()],
            )
            .await
            .unwrap();

        p.team_writer.remove(&a.id).await.unwrap();
        let back = p.matches.read_single(&b, &m.id).await.unwrap();
        assert_eq!(
            replayed(&back),
            vec![(vec![EventTypeEnum::P], 0, 1, TeamSideEnum::Them)]
        );
        let lineup = &back.sets[0].initial_positions;
        let b_ids: Vec<Uuid> = b.players.iter().map(|p| p.id).collect();
        assert_eq!(lineup.as_slice(), &b_ids[..6]);
        assert_eq!(back.roster, b_ids);
        assert_eq!(back.opponent, "A");
        let id = &m.id;
        for (sql, expected) in [
            (
                format!(
                    "SELECT count(*) FROM matches WHERE id = '{id}' AND scouting = 'a'
                     AND home = 'b'"
                ),
                1,
            ),
            (
                format!(
                    "SELECT count(*) FROM events WHERE match_id = '{id}' AND side = 'a'
                     AND deleted = 0"
                ),
                1,
            ),
            (
                format!(
                    "SELECT count(*) FROM events WHERE match_id = '{id}' AND side = 'b'
                     AND deleted = 1"
                ),
                3,
            ),
            (
                format!("SELECT count(*) FROM set_lineups WHERE match_id = '{id}' AND side = 'a'"),
                1,
            ),
            (
                format!(
                    "SELECT count(*) FROM sets WHERE match_id = '{id}' AND number = 2
                     AND deleted = 1 AND serving = 'a'"
                ),
                1,
            ),
            (
                format!(
                    "SELECT count(*) FROM sets WHERE match_id = '{id}' AND number = 1
                     AND deleted = 0 AND serving = 'b'"
                ),
                1,
            ),
        ] {
            assert_eq!(count(&p.db, &sql), expected, "{sql}");
        }
    }

    /// The court seen from B: neutral stays neutral, B's home is A's.
    #[tokio::test]
    async fn the_court_is_seen_from_the_new_a() {
        let p = providers();
        let b = team(&p, "B", players("b")).await;
        for (home, seen) in [("a", "b"), ("b", "a")] {
            let a = team(&p, "A", players("a")).await;
            let m = scouted(&p, &a).await;
            against(&p.db, &m, &b, false);
            execute(
                &p.db,
                &format!("UPDATE matches SET home = '{home}' WHERE id = '{}'", m.id),
            );
            p.team_writer.remove(&a.id).await.unwrap();
            assert_eq!(
                count(
                    &p.db,
                    &format!(
                        "SELECT count(*) FROM matches WHERE id = '{}' AND home = '{seen}'",
                        m.id
                    )
                ),
                1,
                "{home}"
            );
        }
        let a = team(&p, "A", players("a")).await;
        let m = scouted(&p, &a).await;
        against(&p.db, &m, &b, false);
        execute(
            &p.db,
            &format!("UPDATE matches SET home = NULL WHERE id = '{}'", m.id),
        );
        p.team_writer.remove(&a.id).await.unwrap();
        assert_eq!(
            count(
                &p.db,
                &format!(
                    "SELECT count(*) FROM matches WHERE id = '{}' AND home IS NULL",
                    m.id
                )
            ),
            1
        );
    }

    /// A removal fails as a whole: nothing changes.
    #[tokio::test]
    async fn a_removal_is_all_or_nothing() {
        let p = providers();
        let a = team(&p, "A", players("a")).await;
        let b = team(&p, "B", players("b")).await;
        let _against_a_name = scouted(&p, &a).await;
        let m = scouted(&p, &a).await;
        against(&p.db, &m, &b, true);
        let as_b = scouted(&p, &b).await;
        against(&p.db, &as_b, &a, false);
        p.videos.write(&a.id, &m.id, &video()).await.unwrap();
        queue(&p, &[&m], &a).await;
        p.matches.read_summaries(&a).await.unwrap();
        let before = state(&p.db);
        execute(
            &p.db,
            "CREATE TEMP TRIGGER no_removal BEFORE UPDATE OF deleted ON teams
             BEGIN SELECT RAISE(ABORT, 'no'); END;",
        );
        assert!(p.team_writer.remove(&a.id).await.is_err());
        assert_eq!(state(&p.db), before);
        execute(&p.db, "DROP TRIGGER no_removal");
        p.team_writer.remove(&a.id).await.unwrap();
        assert_ne!(state(&p.db), before);
    }

    /// A match is one for both teams (D2): either removes it, with its
    /// video; a team that doesn't play it doesn't.
    #[tokio::test]
    async fn either_team_removes_a_match() {
        let p = providers();
        let a = team(&p, "A", players("a")).await;
        let b = team(&p, "B", players("b")).await;
        let c = team(&p, "C", players("c")).await;
        let m = scouted(&p, &a).await;
        against(&p.db, &m, &b, false);
        p.videos.write(&a.id, &m.id, &video()).await.unwrap();
        let before = state(&p.db);
        p.match_writer.remove(&c.id, &m.id).await.unwrap();
        assert_eq!(state(&p.db), before);
        p.match_writer.remove(&b.id, &m.id).await.unwrap();
        assert!(p.matches.read_all(&a).await.unwrap().is_empty());
        assert_eq!(video_rows(&p.db), (0, 0, 0));
        // removed again: nothing more
        let before = state(&p.db);
        p.match_writer.remove(&a.id, &m.id).await.unwrap();
        assert_eq!(state(&p.db), before);
    }

    /// The persons of a removed team stay in the registry with their
    /// details, in the other teams too.
    #[tokio::test]
    async fn the_persons_stay_with_their_team_removed() {
        let p = providers();
        let a = team(&p, "A", players("a")).await;
        let c = team(&p, "C", a.players.clone()).await;
        scouted(&p, &a).await;
        let persons = state(&p.db)[1].clone();
        p.team_writer.remove(&a.id).await.unwrap();
        assert_eq!(state(&p.db)[1], persons);
        assert_eq!(p.teams.read_single(&c.id).await.unwrap(), c);
    }

    fn person_row(db: &Database, id: Uuid) -> String {
        db.read(|tx| {
            tx.query_row(
                "SELECT name, birth_date, height_cm, notes, deleted FROM persons WHERE id = ?1",
                [Sql(id)],
                |r| {
                    Ok(format!(
                        "{:?}|{:?}|{:?}|{:?}|{:?}",
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, i64>(4)?
                    ))
                },
            )
            .map_err(sqlite)
        })
        .unwrap()
    }

    /// A person removed from the registry: only id and flag are left, out
    /// of every team; a match keeps them as an anonymous player with the
    /// number of its roster, and its replay doesn't change.
    #[tokio::test]
    async fn a_person_removed_keeps_only_the_id() {
        let p = providers();
        let a = team(&p, "A", players("a")).await;
        let c = team(&p, "C", a.players.clone()).await;
        let m = scouted(&p, &a).await;
        let gone = a.players[1].id;
        execute(
            &p.db,
            &format!("UPDATE match_roster SET number = 77 WHERE person_id = '{gone}'"),
        );
        let before = p.matches.read_single(&a, &m.id).await.unwrap();
        let snapshots: Vec<String> = before
            .sets
            .iter()
            .map(|s| format!("{:?}", s.compute_snapshot(before.max_substitutions)))
            .collect();
        let summaries = p.matches.read_summaries(&a).await.unwrap();
        let persons = count(&p.db, "SELECT count(*) FROM persons");

        p.persons.remove(&gone).unwrap();
        assert_eq!(person_row(&p.db, gone), r#"""|None|None|""|1"#);
        assert_eq!(count(&p.db, "SELECT count(*) FROM persons"), persons);
        assert_eq!(
            count(
                &p.db,
                &format!(
                    "SELECT count(*) FROM team_members WHERE person_id = '{gone}' AND deleted = 1"
                )
            ),
            2
        );
        // the others keep their details
        assert_eq!(
            count(
                &p.db,
                "SELECT count(*) FROM persons WHERE deleted = 0 AND notes = 'notes'"
            ),
            6
        );
        // the lists read the match again
        assert_eq!(count(&p.db, "SELECT count(*) FROM match_summary"), 0);
        assert_eq!(p.matches.read_summaries(&a).await.unwrap(), summaries);

        // the team as it reads now: the match takes it from the caller
        let fresh = p.teams.read_single(&a.id).await.unwrap();
        let after = p.matches.read_single(&fresh, &m.id).await.unwrap();
        // in the roster of the match (`deleted` there is "out of the
        // roster"): anonymous, with the number of the match
        let player = after.team.find_player(gone).unwrap();
        assert_eq!(
            (
                player.deleted,
                player.name.as_str(),
                player.number,
                player.notes.as_str()
            ),
            (false, "", 77, "")
        );
        assert_eq!((player.birth_date, player.height_cm), (None, None));
        assert_eq!(after.sets, before.sets);
        assert_eq!(
            after
                .sets
                .iter()
                .map(|s| format!("{:?}", s.compute_snapshot(after.max_substitutions)))
                .collect::<Vec<_>>(),
            snapshots
        );
        // in the team the match tells of them, removed; not in the other
        let team_a = p.teams.read_single(&a.id).await.unwrap();
        let in_a = team_a.find_player(gone).unwrap();
        assert_eq!((in_a.deleted, in_a.name.as_str()), (true, ""));
        assert_eq!(team_a.active_players().len(), 6);
        let team_c = p.teams.read_single(&c.id).await.unwrap();
        assert!(team_c.find_player(gone).is_none());
        assert_eq!(team_c.players.len(), 6);
    }

    /// Removed again: nothing more; a person who isn't there: an error.
    #[tokio::test]
    async fn a_person_is_removed_once() {
        let p = providers();
        let a = team(&p, "A", players("a")).await;
        let gone = a.players[0].id;
        p.persons.remove(&gone).unwrap();
        let before = state(&p.db);
        p.persons.remove(&gone).unwrap();
        assert_eq!(state(&p.db), before);
        assert!(p.persons.remove(&Uuid::new_v4()).is_err());
        assert_eq!(state(&p.db), before);
    }

    /// A team saved as it was read before the removal doesn't bring the
    /// details back.
    #[tokio::test]
    async fn an_old_copy_of_the_team_doesnt_bring_them_back() {
        let p = providers();
        let a = team(&p, "A", players("a")).await;
        let m = scouted(&p, &a).await;
        let gone = a.players[1].id;
        p.persons.remove(&gone).unwrap();
        p.team_writer
            .save(TeamInput::Existing(a.clone()))
            .await
            .unwrap();
        assert_eq!(person_row(&p.db, gone), r#"""|None|None|""|1"#);
        assert_eq!(
            count(
                &p.db,
                &format!("SELECT deleted FROM team_members WHERE person_id = '{gone}'")
            ),
            1
        );
        // the team as it reads now: the match takes it from the caller
        let fresh = p.teams.read_single(&a.id).await.unwrap();
        let read = p.matches.read_single(&fresh, &m.id).await.unwrap();
        let player = read.team.find_player(gone).unwrap();
        assert_eq!(player.name, "");
        let player = fresh.find_player(gone).unwrap();
        assert_eq!((player.deleted, player.name.as_str()), (true, ""));
    }

    /// An archive of the match carries the person without details.
    #[tokio::test]
    async fn an_archive_carries_only_the_id() {
        let p = providers();
        let a = team(&p, "A", players("a")).await;
        let m = scouted(&p, &a).await;
        let gone = a.players[1].id;
        p.persons.remove(&gone).unwrap();
        let zip = export(&p.db, Subject::Match(Uuid::parse_str(&m.id).unwrap())).unwrap();
        let (_, persons) = unzip(&zip)
            .unwrap()
            .into_iter()
            .find(|(name, _)| name == "persons.json")
            .unwrap();
        let persons: Vec<Json> = serde_json::from_slice(&persons).unwrap();
        let row = persons
            .iter()
            .find(|r| r["id"] == gone.to_string())
            .unwrap();
        assert_eq!(
            (
                &row["name"],
                &row["birth_date"],
                &row["height_cm"],
                &row["notes"],
                &row["deleted"]
            ),
            (
                &Json::from(""),
                &Json::Null,
                &Json::Null,
                &Json::from(""),
                &Json::from(1)
            )
        );
    }
}
