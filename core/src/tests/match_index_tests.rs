//! The index of the matches (`providers::store::match_index`): the lists
//! read from it what they would read from the matches, and never what the
//! files no longer say.

#[cfg(test)]
mod tests {
    use crate::{
        archive::import_team,
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        localization::Language,
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            set_writer::SetWriter,
            store::{
                match_reader::StoreMatchReader, match_writer::StoreMatchWriter,
                set_reader::StoreSetReader, set_writer::StoreSetWriter,
                team_reader::StoreTeamReader, team_writer::StoreTeamWriter,
            },
        },
        shapes::{
            enums::{EventTypeEnum, TeamSideEnum},
            r#match::MatchSummary,
            snapshot::EventEntry,
            team::TeamEntry,
        },
        storage::{Store, fs::FsStore, layout, memory::MemoryStore},
        tests::EN,
    };
    use chrono::Utc;
    use serde_json::Value;
    use std::{
        path::Path,
        sync::Arc,
        time::{Duration, SystemTime},
    };
    use uuid::Uuid;

    const TEAM_ID: &str = "45a589d5-004a-4a50-99f5-6d78fd052a3c";

    fn their_errors(n: usize) -> Vec<EventEntry> {
        (0..n)
            .map(|_| EventEntry {
                timestamp: Utc::now(),
                event_type: EventTypeEnum::OE,
                player: None,
                eval: None,
                target_player: None,
                technique: None,
                flags: Default::default(),
            })
            .collect()
    }

    fn reader(store: &Arc<dyn Store>) -> StoreMatchReader {
        StoreMatchReader::new(
            store.clone(),
            Arc::new(StoreSetReader::new(store.clone(), Language::default())),
            Language::default(),
        )
    }

    /// A team with two matches, its sets of their errors: one of a set
    /// going on (24-0), one of two sets won; and a match that can't be read.
    async fn team_with_matches(store: &Arc<dyn Store>) -> TeamEntry {
        let team = import_team(
            include_bytes!("fixtures/45a589d5-004a-4a50-99f5-6d78fd052a3c.zip"),
            Uuid::parse_str(TEAM_ID).ok(),
            &StoreTeamReader::new(store.clone(), Language::default()),
            &StoreTeamWriter::new(store.clone()),
            EN,
        )
        .await
        .unwrap();
        let ids: Vec<Uuid> = team.players.iter().map(|p| p.id).collect();
        let set_writer = StoreSetWriter::new(store.clone(), Language::default());
        for (opponent, date, sets) in [
            ("Arezzo", "2026-10-25T18:00:00+02:00", vec![24]),
            ("Siena", "2026-10-18T18:00:00+02:00", vec![25, 25]),
        ] {
            let m = StoreMatchWriter::new(store.clone())
                .create(
                    &team,
                    opponent.into(),
                    date.parse().unwrap(),
                    true,
                    DEFAULT_MAX_SUBSTITUTIONS,
                    String::new(),
                    vec![],
                )
                .await
                .unwrap();
            for (number, points) in (1..).zip(sets) {
                set_writer
                    .create(
                        &m,
                        number,
                        TeamSideEnum::Us,
                        ids[..6].try_into().unwrap(),
                        ids[6],
                        None,
                        ids[0],
                        their_errors(points),
                    )
                    .await
                    .unwrap();
            }
        }
        store
            .write(&format!("{TEAM_ID}/broken/match.json"), b"{")
            .await
            .unwrap();
        team
    }

    /// The summaries as the matches read in full say them.
    async fn read_in_full(
        store: &Arc<dyn Store>,
        team: &TeamEntry,
    ) -> Vec<Result<MatchSummary, String>> {
        reader(store)
            .read_all_with_status(team)
            .await
            .unwrap()
            .into_iter()
            .map(|listed| {
                listed
                    .map(|(m, status)| MatchSummary::of(&m, &status))
                    .map_err(|u| u.id)
            })
            .collect()
    }

    async fn summaries(
        reader: &StoreMatchReader,
        team: &TeamEntry,
    ) -> Vec<Result<MatchSummary, String>> {
        reader
            .read_summaries(team)
            .await
            .unwrap()
            .into_iter()
            .map(|listed| listed.map_err(|u| u.id))
            .collect()
    }

    async fn index(store: &dyn Store) -> Value {
        serde_json::from_slice(&store.read(layout::MATCH_INDEX).await.unwrap().unwrap()).unwrap()
    }

    fn opponents(listed: &[Result<MatchSummary, String>]) -> Vec<String> {
        listed
            .iter()
            .map(|l| match l {
                Ok(m) => m.opponent.clone(),
                Err(id) => id.clone(),
            })
            .collect()
    }

    /// The summaries, the matches in progress and over, the unreadable one
    /// last: as read from the matches, and indexed but the unreadable one.
    #[tokio::test]
    async fn the_summaries_are_the_matches_read_in_full() {
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let team = team_with_matches(&store).await;
        let expected = read_in_full(&store, &team).await;
        assert_eq!(opponents(&expected), vec!["Arezzo", "Siena", "broken"]);
        let arezzo = expected[0].as_ref().unwrap();
        assert_eq!(
            (arezzo.sets, arezzo.us_wins, arezzo.finished, arezzo.resume),
            (1, 0, false, Some(1))
        );
        let siena = expected[1].as_ref().unwrap();
        assert_eq!(
            (siena.us_wins, siena.resume, siena.next_set_number),
            (2, None, Some(3))
        );
        let reader = reader(&store);
        assert_eq!(summaries(&reader, &team).await, expected);
        // again, from the index in memory
        assert_eq!(summaries(&reader, &team).await, expected);
        let indexed = index(&*store).await;
        let matches = indexed["teams"][TEAM_ID].as_object().unwrap();
        let mut ids: Vec<&String> = matches.keys().collect();
        ids.sort();
        assert_eq!(ids, vec!["2026-10-18_siena", "2026-10-25_arezzo"]);
    }

    /// The index is what the lists read when the files didn't change: an
    /// opponent changed in the index shows; the match changed since, read
    /// again, doesn't.
    #[tokio::test]
    async fn the_index_holds_until_the_files_change() {
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let team = team_with_matches(&store).await;
        summaries(&reader(&store), &team).await;
        let mut indexed = index(&*store).await;
        indexed["teams"][TEAM_ID]["2026-10-25_arezzo"]["summary"]["opponent"] =
            Value::from("From the index");
        store
            .write(layout::MATCH_INDEX, indexed.to_string().as_bytes())
            .await
            .unwrap();
        // the app opened again: the index read from its file
        let listed = summaries(&reader(&store), &team).await;
        assert_eq!(opponents(&listed)[0], "From the index");
        // an event more: the match read again, the set is over
        let csv = format!("{TEAM_ID}/2026-10-25_arezzo/set_1.csv");
        let event = store.read(&csv).await.unwrap().unwrap();
        let last = event
            .split(|b| *b == b'\n')
            .rfind(|l| !l.is_empty())
            .unwrap();
        store.append(&csv, &[last, b"\n"].concat()).await.unwrap();
        let listed = summaries(&reader(&store), &team).await;
        let arezzo = listed[0].as_ref().unwrap();
        assert_eq!(
            (arezzo.opponent.as_str(), arezzo.us_wins, arezzo.resume),
            ("Arezzo", 1, None)
        );
        assert_eq!(listed, read_in_full(&store, &team).await);
    }

    /// The team changed: every match of it read again.
    #[tokio::test]
    async fn a_team_changed_reads_its_matches_again() {
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let team = team_with_matches(&store).await;
        summaries(&reader(&store), &team).await;
        let mut indexed = index(&*store).await;
        indexed["teams"][TEAM_ID]["2026-10-18_siena"]["summary"]["opponent"] =
            Value::from("From the index");
        store
            .write(layout::MATCH_INDEX, indexed.to_string().as_bytes())
            .await
            .unwrap();
        let descriptor = layout::team_descriptor(&team.id);
        let json = store.read(&descriptor).await.unwrap().unwrap();
        store.write(&descriptor, &json).await.unwrap();
        assert_eq!(
            summaries(&reader(&store), &team).await,
            read_in_full(&store, &team).await
        );
    }

    /// An index that can't be read, or of another version: as none, and
    /// written again.
    #[tokio::test]
    async fn an_index_unreadable_or_of_another_version_is_left_aside() {
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let team = team_with_matches(&store).await;
        let expected = read_in_full(&store, &team).await;
        summaries(&reader(&store), &team).await;
        let mut indexed = index(&*store).await;
        indexed["teams"][TEAM_ID]["2026-10-25_arezzo"]["summary"]["opponent"] =
            Value::from("From the index");
        indexed["version"] = Value::from("0/0.0.0");
        for written in [indexed.to_string().into_bytes(), b"{".to_vec()] {
            store.write(layout::MATCH_INDEX, &written).await.unwrap();
            assert_eq!(summaries(&reader(&store), &team).await, expected);
            assert_eq!(
                index(&*store).await["teams"][TEAM_ID]["2026-10-25_arezzo"]["summary"]["opponent"],
                "Arezzo"
            );
        }
    }

    /// A team whose folder is gone (removed by hand, or by a restore)
    /// leaves the index with the next listing, even when it changes
    /// nothing else; an entry that isn't a team neither stays.
    #[tokio::test]
    async fn the_teams_gone_leave_the_index() {
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let team = team_with_matches(&store).await;
        summaries(&reader(&store), &team).await;
        let mut indexed = index(&*store).await;
        let ours = indexed["teams"][TEAM_ID].clone();
        for gone in ["2c1e0d3a-5b7f-4e7a-9d6c-0f8e1a2b3c4d", "not a team"] {
            indexed["teams"][gone] = ours.clone();
        }
        store
            .write(layout::MATCH_INDEX, indexed.to_string().as_bytes())
            .await
            .unwrap();
        let reader = reader(&store);
        summaries(&reader, &team).await;
        let teams: Vec<String> = index(&*store).await["teams"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(teams, vec![TEAM_ID.to_string()]);
        // written once: the next listing finds it as it is
        store.write(layout::MATCH_INDEX, b"{}").await.unwrap();
        summaries(&reader, &team).await;
        assert_eq!(
            &*store.read(layout::MATCH_INDEX).await.unwrap().unwrap(),
            b"{}"
        );
    }

    /// On the file system, a file just written isn't indexed (its time may
    /// be the one of the write before); once settled, it is.
    #[tokio::test]
    async fn the_files_just_written_wait_to_be_indexed() {
        let dir = tempfile::tempdir().unwrap();
        let store: Arc<dyn Store> = Arc::new(FsStore::new(dir.path()));
        let team = team_with_matches(&store).await;
        let expected = read_in_full(&store, &team).await;
        assert_eq!(summaries(&reader(&store), &team).await, expected);
        let unsettled = index(&*store).await;
        assert_eq!(
            unsettled["teams"][TEAM_ID].as_object().map(|m| m.len()),
            Some(0)
        );
        age(dir.path(), Duration::from_secs(10));
        assert_eq!(summaries(&reader(&store), &team).await, expected);
        let settled = index(&*store).await;
        assert_eq!(
            settled["teams"][TEAM_ID].as_object().map(|m| m.len()),
            Some(2)
        );
    }

    /// Every file under the folder modified `ago`.
    fn age(folder: &Path, ago: Duration) {
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            match path.is_dir() {
                true => age(&path, ago),
                false => std::fs::File::options()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_modified(SystemTime::now() - ago)
                    .unwrap(),
            }
        }
    }
}
