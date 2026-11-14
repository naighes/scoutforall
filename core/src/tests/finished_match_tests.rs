//! A finished match goes to the upload queue of the scouting, if the user
//! allows the statistics: not to the global one, so that each data folder
//! of a process (as in the tests of `mobile`) keeps its own.

#[cfg(test)]
mod tests {
    use crate::{
        analytics::{finished::QueuedFinishedMatches, queue_manager::QueueManager},
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        localization::Language,
        providers::{
            match_writer::MatchWriter,
            set_writer::SetWriter,
            sqlite::{
                legacy_archive::import_legacy_team, queue::SqliteQueue, sets::SqliteSetWriter,
            },
            team_reader::TeamReader,
        },
        set_start,
        shapes::{
            enums::{EventTypeEnum, TeamSideEnum},
            snapshot::EventEntry,
        },
        tests::Sqlite,
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
    /// match is over, the matches in the queue of the scouting, and the id
    /// of the match.
    async fn match_point(enabled: bool) -> (bool, Vec<String>, String) {
        let p = Sqlite::new();
        let imported = import_legacy_team(
            &p.db,
            include_bytes!("fixtures/45a589d5-004a-4a50-99f5-6d78fd052a3c.zip"),
            Uuid::parse_str(TEAM_ID).ok(),
            &Language::default(),
        )
        .unwrap();
        let team = p.team_reader.read_single(&imported.id).await.unwrap();
        let ids: Vec<Uuid> = team.players.iter().map(|p| p.id).collect();
        let mut m = p
            .match_writer
            .create(
                &team,
                crate::match_form::MatchInput::against(
                    "Arezzo",
                    "2026-10-25T18:00:00+02:00".parse().unwrap(),
                    true,
                    DEFAULT_MAX_SUBSTITUTIONS,
                    "",
                    vec![],
                ),
            )
            .await
            .unwrap();
        let set_writer = Arc::new(SqliteSetWriter::new(p.db.clone(), Language::default()));
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
        let id = m.id.clone();
        let queue = Arc::new(QueueManager::new(
            Arc::new(SqliteQueue::new(p.db.clone())),
            Arc::new(SqliteQueue::new(p.db.clone())),
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
            id,
        )
    }

    #[tokio::test]
    async fn the_finished_match_goes_to_the_queue_of_the_scouting() {
        let (over, queued, id) = match_point(true).await;
        assert_eq!((over, queued), (true, vec![id]));
    }

    #[tokio::test]
    async fn not_when_the_user_turned_the_statistics_off() {
        let (over, queued, _) = match_point(false).await;
        assert_eq!((over, queued), (true, vec![]));
    }
}
