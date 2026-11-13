//! The cache `match_summary` (TODO/001, step 8): the lists of the matches
//! from the database are those of the index of the files, the cache is
//! up to date after every write and is what the lists read.

#[cfg(test)]
mod tests {
    use crate::{
        localization::Language,
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            set_reader::SetReader,
            set_writer::SetWriter,
            sqlite::{
                Database,
                matches::{SqliteMatchReader, SqliteMatchWriter},
                sets::{SqliteSetReader, SqliteSetWriter},
                teams::SqliteTeamWriter,
            },
            store::{
                match_reader::StoreMatchReader, match_writer::StoreMatchWriter,
                set_reader::StoreSetReader, set_writer::StoreSetWriter,
                team_reader::StoreTeamReader, team_writer::StoreTeamWriter,
            },
            team_reader::TeamReader,
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{
            listed::Listed,
            r#match::{MatchEntry, MatchSummary},
            set::SetEntry,
            team::TeamEntry,
        },
        storage::{Store, fs::FsStore, memory::MemoryStore},
        tests::{LEGACY_VERSIONS, copy_dir, legacy_fixtures},
    };
    use std::{collections::BTreeMap, sync::Arc};
    use tempfile::TempDir;

    struct Providers {
        team_writer: Box<dyn TeamWriter + Send + Sync>,
        match_reader: Box<dyn MatchReader + Send + Sync>,
        match_writer: Box<dyn MatchWriter + Send + Sync>,
        set_reader: Box<dyn SetReader + Send + Sync>,
        set_writer: Box<dyn SetWriter + Send + Sync>,
    }

    fn files(store: Arc<dyn Store>) -> Providers {
        let language = Language::default();
        let set_reader = Arc::new(StoreSetReader::new(Arc::clone(&store), language.clone()));
        Providers {
            team_writer: Box::new(StoreTeamWriter::new(Arc::clone(&store))),
            match_reader: Box::new(StoreMatchReader::new(
                Arc::clone(&store),
                set_reader,
                language.clone(),
            )),
            match_writer: Box::new(StoreMatchWriter::new(Arc::clone(&store))),
            set_reader: Box::new(StoreSetReader::new(Arc::clone(&store), language.clone())),
            set_writer: Box::new(StoreSetWriter::new(store, language)),
        }
    }

    fn database() -> Database {
        Database::new(crate::db::open_in_memory().unwrap())
    }

    fn sqlite(db: &Database) -> Providers {
        let language = Language::default();
        Providers {
            team_writer: Box::new(SqliteTeamWriter::new(db.clone())),
            match_reader: Box::new(SqliteMatchReader::new(db.clone(), language.clone())),
            match_writer: Box::new(SqliteMatchWriter::new(db.clone())),
            set_reader: Box::new(SqliteSetReader::new(db.clone(), language.clone())),
            set_writer: Box::new(SqliteSetWriter::new(db.clone(), language)),
        }
    }

    fn count(db: &Database, sql: &str) -> i64 {
        db.read(|tx| {
            tx.query_row(sql, [], |row| row.get(0))
                .map_err(crate::providers::sqlite::sqlite)
        })
        .unwrap()
    }

    fn execute(db: &Database, sql: &str) {
        db.write(|tx| {
            tx.execute_batch(sql)
                .map_err(crate::providers::sqlite::sqlite)
        })
        .unwrap()
    }

    fn rows(db: &Database) -> i64 {
        count(db, "SELECT count(*) FROM match_summary")
    }

    /// A listing, comparable: by id, with the ids through `id`.
    fn by_id(
        listed: Vec<Listed<MatchSummary>>,
        id: impl Fn(&str) -> String,
    ) -> BTreeMap<String, Listed<MatchSummary>> {
        listed
            .into_iter()
            .map(|l| match l {
                Ok(s) => (id(&s.id), Ok(MatchSummary { id: id(&s.id), ..s })),
                Err(mut u) => {
                    u.id = id(&u.id);
                    (u.id.clone(), Err(u))
                }
            })
            .collect()
    }

    /// What the listing of a team would be without any cache: its matches
    /// replayed.
    async fn replayed(p: &Providers, team: &TeamEntry) -> Vec<Listed<MatchSummary>> {
        p.match_reader
            .read_all_with_status(team)
            .await
            .unwrap()
            .into_iter()
            .map(|l| l.map(|(m, status)| MatchSummary::of(&m, &status)))
            .collect()
    }

    /// The match `m` of the files, scouted again with `p`: the same team,
    /// form and sets; its new id.
    async fn scout_again(p: &Providers, team: &TeamEntry, m: &MatchEntry) -> MatchEntry {
        let new = p
            .match_writer
            .create(
                team,
                m.opponent.clone(),
                m.date,
                m.home,
                m.max_substitutions,
                m.notes.clone(),
                m.roster.clone(),
            )
            .await
            .unwrap();
        for set in &m.sets {
            create_set(p, &new, set).await;
            p.set_writer.update(&new, set).await.unwrap();
        }
        new
    }

    /// The set `set` created in `m`, without its events.
    async fn create_set(p: &Providers, m: &MatchEntry, set: &SetEntry) {
        p.set_writer
            .create(
                m,
                set.set_number,
                set.serving_team,
                set.initial_positions,
                set.libero,
                set.fallback_libero,
                set.setter,
                vec![],
            )
            .await
            .unwrap();
    }

    /// Every match of every version of the legacy fixtures: the listing of
    /// the database, from the cache and rebuilt from nothing, is the one
    /// of the index of the files, and the one replaying every match.
    #[tokio::test]
    async fn the_listings_are_those_of_the_index_of_the_files() {
        let mut matches = 0;
        let mut finished = 0;
        let mut going_on = 0;
        for version in LEGACY_VERSIONS {
            let dir = TempDir::new().unwrap();
            copy_dir(&legacy_fixtures(version).join("data"), dir.path());
            let store: Arc<dyn Store> = Arc::new(FsStore::new(dir.path()));
            let legacy = files(Arc::clone(&store));
            let teams: Vec<TeamEntry> = StoreTeamReader::new(store, Language::default())
                .read_all()
                .await
                .unwrap()
                .into_iter()
                .map(|t| t.unwrap())
                .collect();
            let db = database();
            let p = sqlite(&db);
            for team in &teams {
                p.team_writer
                    .save(TeamInput::Existing(team.clone()))
                    .await
                    .unwrap();
            }
            for team in &teams {
                // the index of the files: built, then read
                let indexed = legacy.match_reader.read_summaries(team).await.unwrap();
                assert_eq!(
                    legacy.match_reader.read_summaries(team).await.unwrap(),
                    indexed
                );
                let mut legacy_id = BTreeMap::new();
                for m in legacy.match_reader.read_all(team).await.unwrap() {
                    let m = m.unwrap();
                    let new = scout_again(&p, team, &m).await;
                    legacy_id.insert(new.id.clone(), m.id.clone());
                }
                let back = |id: &str| legacy_id[id].clone();
                let expected = by_id(indexed.clone(), str::to_string);
                // up to date after the writes
                let cached = p.match_reader.read_summaries(team).await.unwrap();
                assert_eq!(by_id(cached.clone(), back), expected, "{version}");
                assert_eq!(by_id(replayed(&p, team).await, back), expected);
                // in the same order, the most recent first
                let dates = |l: &[Listed<MatchSummary>]| -> Vec<_> {
                    l.iter().map(|s| s.as_ref().unwrap().date).collect()
                };
                assert_eq!(dates(&cached), dates(&indexed));
                for s in indexed.iter().flatten() {
                    matches += 1;
                    finished += usize::from(s.finished);
                    going_on += usize::from(s.resume.is_some());
                }
            }
            // rebuilt from nothing
            let before = rows(&db);
            execute(&db, "DELETE FROM match_summary");
            for team in &teams {
                let indexed = legacy.match_reader.read_summaries(team).await.unwrap();
                let rebuilt = p.match_reader.read_summaries(team).await.unwrap();
                assert_eq!(rebuilt.len(), indexed.len(), "{version}");
                assert_eq!(
                    by_id(rebuilt, str::to_string),
                    by_id(
                        p.match_reader.read_summaries(team).await.unwrap(),
                        str::to_string
                    )
                );
            }
            assert_eq!(rows(&db), before, "{version}");
        }
        assert!(
            matches >= 30 && finished > 0 && going_on > 0,
            "{matches} {finished} {going_on}"
        );
    }

    /// The finished match of v0.18.0 and its team, from the files.
    async fn finished_match() -> (TeamEntry, MatchEntry) {
        let dir = TempDir::new().unwrap();
        copy_dir(&legacy_fixtures("v0.18.0").join("data"), dir.path());
        let store: Arc<dyn Store> = Arc::new(FsStore::new(dir.path()));
        let p = files(Arc::clone(&store));
        for team in StoreTeamReader::new(store, Language::default())
            .read_all()
            .await
            .unwrap()
        {
            let team = team.unwrap();
            for m in p.match_reader.read_all(&team).await.unwrap() {
                let m = m.unwrap();
                if m.id == "2025-10-04_rivali_pisa" {
                    return (team, m);
                }
            }
        }
        unreachable!("the finished match of v0.18.0")
    }

    /// The listing of the team, as the replay of its matches has it, the
    /// same on the files and on the database: after every write.
    struct Both {
        files: Providers,
        db: Database,
        sqlite: Providers,
        team: TeamEntry,
        /// on the files, on the database
        matches: (MatchEntry, MatchEntry),
    }

    impl Both {
        async fn new(team: &TeamEntry, m: &MatchEntry) -> Self {
            let db = database();
            let both = Both {
                files: files(Arc::new(MemoryStore::new())),
                sqlite: sqlite(&db),
                db,
                team: team.clone(),
                matches: (m.clone(), m.clone()),
            };
            let mut matches = vec![];
            for p in [&both.files, &both.sqlite] {
                p.team_writer
                    .save(TeamInput::Existing(team.clone()))
                    .await
                    .unwrap();
                let created = p
                    .match_writer
                    .create(
                        team,
                        m.opponent.clone(),
                        m.date,
                        m.home,
                        m.max_substitutions,
                        m.notes.clone(),
                        m.roster.clone(),
                    )
                    .await
                    .unwrap();
                matches.push(created);
            }
            let sqlite = matches.pop().unwrap();
            let files = matches.pop().unwrap();
            Both {
                matches: (files, sqlite),
                ..both
            }
        }

        /// The listing of the database: the one replaying, the one of the
        /// files; its only match.
        async fn check(&self, what: &str) -> Listed<MatchSummary> {
            let listed = self
                .sqlite
                .match_reader
                .read_summaries(&self.team)
                .await
                .unwrap();
            let replayed = replayed(&self.sqlite, &self.team).await;
            assert_eq!(listed, replayed, "{what}");
            let same = |id: &str| match id == self.matches.1.id {
                true => self.matches.0.id.clone(),
                false => id.to_string(),
            };
            let files = self
                .files
                .match_reader
                .read_summaries(&self.team)
                .await
                .unwrap();
            assert_eq!(
                by_id(listed.clone(), same),
                by_id(files, str::to_string),
                "{what}"
            );
            assert_eq!(listed.len(), 1);
            listed.into_iter().next().unwrap()
        }

        /// The cache row of the database, whether there is one: written in
        /// the transaction of the write, not by the listing.
        fn cached(&self) -> bool {
            rows(&self.db) == 1
        }

        async fn each(&self, f: impl AsyncFn(&Providers, &MatchEntry)) {
            f(&self.files, &self.matches.0).await;
            f(&self.sqlite, &self.matches.1).await;
        }
    }

    /// A real match scouted again touch by touch, undone, corrected, its
    /// sets removed and created again, its form changed: after each write
    /// the cache is there, and the listing is the replay and the files.
    #[tokio::test]
    async fn the_cache_follows_every_write() {
        let (team, m) = finished_match().await;
        let both = Both::new(&team, &m).await;
        assert!(both.cached());
        let new = both.check("created").await.unwrap();
        assert_eq!(
            (new.sets, new.us_wins, new.them_wins, new.finished),
            (0, 0, 0, false)
        );
        assert_eq!((new.resume, new.next_set_number), (None, Some(1)));

        let mut touches = 0;
        for set in &m.sets {
            both.each(async |p, m| create_set(p, m, set).await).await;
            assert!(both.cached());
            assert_eq!(
                both.check("set").await.unwrap().resume,
                Some(set.set_number)
            );
            for (i, event) in set.events.iter().enumerate() {
                both.each(async |p, m| {
                    p.set_writer
                        .append_event(m, set.set_number, event)
                        .await
                        .unwrap()
                })
                .await;
                assert!(both.cached());
                // every touch of the first set, then the last ones of a set
                match (set.set_number, set.events.len() - i) {
                    (1, _) | (_, 0..=3) => {
                        both.check(&format!("set {} touch {i}", set.set_number))
                            .await
                            .unwrap();
                    }
                    _ => {}
                }
                touches += 1;
            }
            let after = both.check("set over").await.unwrap();
            assert_eq!(after.resume, None);
        }
        assert!(touches > 300, "{touches}");
        let last = m.sets.last().unwrap();
        let over = both.check("match over").await.unwrap();
        assert!(over.finished);
        assert_eq!(over.sets, u8::try_from(m.sets.len()).unwrap());
        assert_eq!(over.next_set_number, None);

        // the last touch undone: the set and the match go on
        both.each(async |p, m| {
            p.set_writer
                .remove_last_event(m, last.set_number)
                .await
                .unwrap();
        })
        .await;
        assert!(both.cached());
        let undone = both.check("undone").await.unwrap();
        assert!(!undone.finished);
        assert_eq!(undone.resume, Some(last.set_number));
        // and done again
        let point = last.events.last().unwrap();
        both.each(async |p, m| {
            p.set_writer
                .append_event(m, last.set_number, point)
                .await
                .unwrap()
        })
        .await;
        assert!(both.check("done again").await.unwrap().finished);

        // the last set corrected to half its touches
        let half = SetEntry {
            events: last.events[..last.events.len() / 2].to_vec(),
            ..last.clone()
        };
        both.each(async |p, m| p.set_writer.update(m, &half).await.unwrap())
            .await;
        assert!(both.cached());
        assert_eq!(
            both.check("corrected").await.unwrap().resume,
            Some(last.set_number)
        );

        // the last set removed: the match is to go on with it
        both.each(async |p, m| p.set_writer.remove(m, last.set_number).await.unwrap())
            .await;
        assert!(both.cached());
        let removed = both.check("removed").await.unwrap();
        assert_eq!(removed.sets, u8::try_from(m.sets.len() - 1).unwrap());
        assert_eq!(removed.next_set_number, Some(last.set_number));
        assert_eq!(removed.resume, None);
        // and scouted again at once
        both.each(async |p, m| {
            create_set(p, m, last).await;
            p.set_writer.update(m, last).await.unwrap();
        })
        .await;
        assert!(both.check("scouted again").await.unwrap().finished);

        // the form: opponent, date and court are read from the match
        let date = m.date + chrono::Duration::days(1);
        both.each(async |p, m| {
            p.match_writer
                .update(
                    m,
                    "Rivali Siena".into(),
                    date,
                    !m.home,
                    m.max_substitutions,
                    String::new(),
                    m.roster.clone(),
                )
                .await
                .unwrap()
        })
        .await;
        assert!(both.cached());
        let changed = both.check("form").await.unwrap();
        assert_eq!(
            (changed.opponent.as_str(), changed.date, changed.home),
            ("Rivali Siena", date, !m.home)
        );
        assert!(changed.finished);
    }

    /// The substitutions decide how the sets replay: fewer than the match
    /// made, it no longer replays, and the cache has no row for it.
    #[tokio::test]
    async fn fewer_substitutions_than_made_leave_no_row() {
        let (team, m) = finished_match().await;
        let both = Both::new(&team, &m).await;
        for set in &m.sets {
            both.each(async |p, m| {
                create_set(p, m, set).await;
                p.set_writer.update(m, set).await.unwrap();
            })
            .await;
        }
        assert!(both.check("scouted").await.unwrap().finished);
        let update = async |max_substitutions: u8| {
            both.each(async |p, m| {
                p.match_writer
                    .update(
                        m,
                        m.opponent.clone(),
                        m.date,
                        m.home,
                        max_substitutions,
                        String::new(),
                        m.roster.clone(),
                    )
                    .await
                    .unwrap()
            })
            .await
        };
        update(0).await;
        assert!(!both.cached());
        let unreadable = both.check("no substitutions").await.unwrap_err();
        assert!(unreadable.reason.is_some());
        // the listing doesn't write it either
        assert!(!both.cached());
        update(m.max_substitutions).await;
        assert!(both.cached());
        assert!(both.check("substitutions back").await.unwrap().finished);
    }

    /// A new team, and a match of it on the database with the first set
    /// of the finished match.
    async fn one_set(db: &Database) -> (Providers, TeamEntry, MatchEntry) {
        let (team, m) = finished_match().await;
        // a team of its own each time, with the same persons
        let team = TeamEntry {
            id: uuid::Uuid::new_v4(),
            ..team
        };
        let p = sqlite(db);
        p.team_writer
            .save(TeamInput::Existing(team.clone()))
            .await
            .unwrap();
        let new = scout_again(
            &p,
            &team,
            &MatchEntry {
                sets: m.sets[..1].to_vec(),
                ..m
            },
        )
        .await;
        (p, team, new)
    }

    /// What the listing reads is the cache, not the events: a change the
    /// writes didn't make shows only when the row is gone, or of another
    /// version of the app.
    #[tokio::test]
    async fn the_listing_reads_the_cache() {
        let db = database();
        let (p, team, m) = one_set(&db).await;
        let won = p.match_reader.read_summaries(&team).await.unwrap();
        let won = won[0].as_ref().unwrap();
        assert_eq!(won.us_wins + won.them_wins, 1);
        // the events of the set gone behind the back of the cache
        execute(
            &db,
            &format!("UPDATE events SET deleted = 1 WHERE match_id = '{}'", m.id),
        );
        let listed = p.match_reader.read_summaries(&team).await.unwrap();
        assert_eq!(listed[0].as_ref().unwrap(), won);
        let replay = replayed(&p, &team).await;
        let replay = replay[0].as_ref().unwrap();
        assert_eq!(replay.us_wins + replay.them_wins, 0);
        // a row of another version is replayed, and written again
        execute(&db, "UPDATE match_summary SET version = '0.0.0'");
        let listed = p.match_reader.read_summaries(&team).await.unwrap();
        assert_eq!(listed[0].as_ref().unwrap(), replay);
        assert_eq!(
            count(
                &db,
                &format!(
                    "SELECT count(*) FROM match_summary WHERE version = '{}'",
                    env!("CARGO_PKG_VERSION")
                )
            ),
            1
        );
        // a missing row too
        execute(
            &db,
            &format!("UPDATE events SET deleted = 0 WHERE match_id = '{}'", m.id),
        );
        execute(&db, "DELETE FROM match_summary");
        let listed = p.match_reader.read_summaries(&team).await.unwrap();
        assert_eq!(listed[0].as_ref().unwrap(), won);
        assert_eq!(rows(&db), 1);
        // read and written the same
        assert_eq!(
            count(
                &db,
                &format!(
                    "SELECT count(*) FROM match_summary WHERE match_id = '{}' AND sets = 1
                     AND a_wins + b_wins = 1 AND finished = 0 AND resume IS NULL
                     AND next_set_number = 2",
                    m.id
                )
            ),
            1
        );
    }

    /// A write of the team forgets the rows of its matches (as `team.json`
    /// did with the index of the files), the next listing rebuilds them; a
    /// removed match or team has no row.
    #[tokio::test]
    async fn a_team_written_or_removed_forgets_its_rows() {
        let db = database();
        let (p, team, m) = one_set(&db).await;
        let (other_p, other, _) = one_set(&db).await;
        assert_eq!(rows(&db), 2);
        let listed = p.match_reader.read_summaries(&team).await.unwrap();
        p.team_writer
            .save(TeamInput::Existing(team.clone()))
            .await
            .unwrap();
        assert_eq!(rows(&db), 1);
        assert_eq!(p.match_reader.read_summaries(&team).await.unwrap(), listed);
        assert_eq!(rows(&db), 2);

        p.match_writer.remove(&team.id, &m.id).await.unwrap();
        assert_eq!(rows(&db), 1);
        assert!(
            p.match_reader
                .read_summaries(&team)
                .await
                .unwrap()
                .is_empty()
        );
        other_p.team_writer.remove(&other.id).await.unwrap();
        assert_eq!(rows(&db), 0);
    }

    /// The opponent is the name of team B as the match reads it: when B is
    /// a team, its name now, also when the team is renamed after the row
    /// was written.
    #[tokio::test]
    async fn the_opponent_is_read_from_the_match() {
        let db = database();
        let (p, team, m) = one_set(&db).await;
        let (_, b, _) = one_set(&db).await;
        execute(
            &db,
            &format!(
                "UPDATE matches SET team_b = '{}', team_b_name = NULL WHERE id = '{}'",
                b.id, m.id
            ),
        );
        let listed = p.match_reader.read_summaries(&team).await.unwrap();
        assert_eq!(listed[0].as_ref().unwrap().opponent, b.name);
        p.team_writer
            .save(TeamInput::Existing(TeamEntry {
                name: "Rivali Arezzo".into(),
                ..b
            }))
            .await
            .unwrap();
        // the row of the match of A stays
        let row = format!(
            "SELECT count(*) FROM match_summary WHERE match_id = '{}'",
            m.id
        );
        assert_eq!(count(&db, &row), 1);
        let renamed = p.match_reader.read_summaries(&team).await.unwrap();
        assert_eq!(renamed[0].as_ref().unwrap().opponent, "Rivali Arezzo");
        assert_eq!(
            MatchSummary {
                opponent: b.name.clone(),
                ..renamed[0].clone().unwrap()
            },
            listed[0].clone().unwrap()
        );
        assert_eq!(p.set_reader.read_all(&m).await.unwrap().len(), 1);
    }
}
