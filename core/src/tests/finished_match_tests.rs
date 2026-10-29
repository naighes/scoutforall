//! A finished match goes to the upload queue of the scouting, if the user
//! allows the statistics: not to the global one, so that each data folder
//! of a process (as in the tests of `mobile`) keeps its own.

#[cfg(test)]
mod tests {
    use crate::{
        analytics::{finished::QueuedFinishedMatches, queue_manager::QueueManager},
        archive::import_team,
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        localization::Language,
        providers::{
            match_writer::MatchWriter,
            set_writer::SetWriter,
            store::{
                match_writer::StoreMatchWriter, queue_reader::StoreQueueReader,
                queue_writer::StoreQueueWriter, set_writer::StoreSetWriter,
                team_reader::StoreTeamReader, team_writer::StoreTeamWriter,
            },
        },
        set_start,
        shapes::{
            enums::{EventTypeEnum, TeamSideEnum},
            snapshot::EventEntry,
        },
        storage::{Store, memory::MemoryStore},
        tests::EN,
    };
    use chrono::Utc;
    use std::sync::{Arc, atomic::AtomicBool};
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

    /// The match point of the third set, the first two won: whether the
    /// match is over, and the matches in the queue of the scouting.
    async fn match_point(enabled: bool) -> (bool, Vec<String>) {
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
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
        let mut m = StoreMatchWriter::new(store.clone())
            .create(
                &team,
                "Arezzo".into(),
                "2026-10-25T18:00:00+02:00".parse().unwrap(),
                true,
                DEFAULT_MAX_SUBSTITUTIONS,
                String::new(),
                vec![],
            )
            .await
            .unwrap();
        let set_writer = Arc::new(StoreSetWriter::new(store.clone(), Language::default()));
        for (number, points) in [(1, 25), (2, 25), (3, 24)] {
            let set = set_writer
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
            m.sets.push(set);
        }
        let set = m.sets.pop().unwrap();
        let queue = Arc::new(QueueManager::new(
            Arc::new(StoreQueueReader::new(store.clone())),
            Arc::new(StoreQueueWriter::new(store.clone())),
        ));
        let finished = QueuedFinishedMatches {
            queue: queue.clone(),
            enabled: Arc::new(AtomicBool::new(enabled)),
        };
        let mut session = set_start::resume(m, set, set_writer)
            .unwrap()
            .with_finished(Arc::new(finished));
        session.choose_event(EventTypeEnum::OE).await.unwrap();
        let over = session.current_match().get_status().unwrap().match_finished;
        let queued = queue.load().await.unwrap();
        (
            over,
            queued
                .peek()
                .map(|p| p.match_id.clone())
                .into_iter()
                .collect(),
        )
    }

    #[tokio::test]
    async fn the_finished_match_goes_to_the_queue_of_the_scouting() {
        assert_eq!(
            match_point(true).await,
            (true, vec!["2026-10-25_arezzo".to_string()])
        );
    }

    #[tokio::test]
    async fn not_when_the_user_turned_the_statistics_off() {
        assert_eq!(match_point(false).await, (true, vec![]));
    }
}
