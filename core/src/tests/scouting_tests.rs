//! Scouting a set without a user interface: the choices, the events saved
//! and the undo.

#[cfg(test)]
mod tests {
    use crate::{
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        localization::Language,
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            set_writer::SetWriter,
            store::{
                match_reader::StoreMatchReader, match_writer::StoreMatchWriter,
                set_reader::StoreSetReader, set_writer::StoreSetWriter,
                team_writer::StoreTeamWriter,
            },
            team_writer::{TeamInput, TeamWriter},
        },
        scouting::{ChoiceOrder, Progress, ScoutingError, ScoutingSession, ScoutingStep},
        shapes::{
            enums::{EvalEnum, EventTypeEnum, RoleEnum, TeamSideEnum},
            player::PlayerEntry,
            snapshot::EventEntry,
            stats::{Metric, StatsFilter},
            technique::{ServeTypeEnum, TechniqueEnum},
        },
        storage::{Store, memory::MemoryStore},
        tests::EN,
    };
    use EventTypeEnum::*;
    use chrono::Utc;
    use std::sync::Arc;
    use uuid::Uuid;

    struct Scouting {
        session: ScoutingSession,
        store: Arc<dyn Store>,
        /// the starting six, then the libero and a player on the bench
        players: Vec<Uuid>,
    }

    /// A set in memory where we serve.
    async fn scouting() -> Scouting {
        use RoleEnum::*;
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let players: Vec<PlayerEntry> = [
            Setter,
            OutsideHitter,
            MiddleBlocker,
            OppositeHitter,
            OutsideHitter,
            MiddleBlocker,
            Libero,
            MiddleBlocker,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, role)| PlayerEntry {
            id: Uuid::new_v4(),
            name: format!("player {}", i + 1),
            role: Some(role),
            number: i as u8 + 1,
            deleted: false,
        })
        .collect();
        let ids: Vec<Uuid> = players.iter().map(|p| p.id).collect();
        let team = StoreTeamWriter::new(Arc::clone(&store))
            .save(TeamInput::New {
                id: None,
                name: "Volley Club".into(),
                year: 2025,
                classification: None,
                gender: None,
                players,
            })
            .await
            .unwrap();
        let m = StoreMatchWriter::new(Arc::clone(&store))
            .create(
                &team,
                "Cortona Volley".into(),
                Utc::now().fixed_offset(),
                true,
                DEFAULT_MAX_SUBSTITUTIONS,
            )
            .await
            .unwrap();
        let set_writer = Arc::new(StoreSetWriter::new(Arc::clone(&store), Language::default()));
        let set = set_writer
            .create(
                &m,
                1,
                TeamSideEnum::Us,
                ids[..6].try_into().unwrap(),
                ids[6],
                None,
                ids[0],
                vec![],
            )
            .await
            .unwrap();
        let snapshot = set.compute_snapshot(m.max_substitutions).unwrap();
        Scouting {
            session: ScoutingSession::new(m, set, snapshot, set_writer),
            store,
            players: ids,
        }
    }

    /// The events as they load from the store.
    async fn saved_events(s: &Scouting) -> Vec<EventEntry> {
        let m = s.session.current_match();
        StoreMatchReader::new(
            Arc::clone(&s.store),
            Arc::new(StoreSetReader::new(
                Arc::clone(&s.store),
                Language::default(),
            )),
            Language::default(),
        )
        .read_single(&m.team, &m.id)
        .await
        .unwrap()
        .sets
        .remove(0)
        .events
    }

    fn event_types(events: &[EventEntry]) -> Vec<EventTypeEnum> {
        events.iter().map(|e| e.event_type).collect()
    }

    /// The server is known: the serve goes straight to the evaluation,
    /// with the serve preselected.
    #[tokio::test]
    async fn a_serve_is_evaluated_with_its_serve_type() {
        let mut s = scouting().await;
        assert_eq!(s.session.choose_event(S).await, Ok(Progress::Choosing));
        assert_eq!(s.session.step(), ScoutingStep::Eval);
        assert!(s.session.player().is_some());
        let float = TechniqueEnum::Serve(ServeTypeEnum::Float);
        let spin = TechniqueEnum::Serve(ServeTypeEnum::Spin);
        assert_eq!(s.session.technique(), Some(float));
        assert_eq!(s.session.choose_technique(spin), Ok(()));
        assert_eq!(
            s.session.choose_eval(EvalEnum::Positive).await,
            Ok(Progress::Saved)
        );
        assert_eq!(s.session.step(), ScoutingStep::Event);
        assert_eq!(s.session.event(), None);
        let saved = saved_events(&s).await;
        assert_eq!(event_types(&saved), vec![S]);
        assert_eq!(saved[0].technique, Some(spin));
    }

    /// A dig needs the player, then the evaluation; a fault the player only.
    #[tokio::test]
    async fn the_player_comes_before_the_evaluation() {
        let mut s = scouting().await;
        s.session.choose_event(S).await.unwrap();
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        assert_eq!(s.session.choose_event(D).await, Ok(Progress::Choosing));
        assert_eq!(s.session.step(), ScoutingStep::Player);
        let digger = s.session.player_choices()[0].id;
        assert_eq!(
            s.session.choose_player(digger).await,
            Ok(Progress::Choosing)
        );
        assert_eq!(s.session.step(), ScoutingStep::Eval);
        // not a serve: no technique
        assert_eq!(s.session.available_techniques(), vec![]);
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        s.session.choose_event(F).await.unwrap();
        let player = s.session.player_choices()[0].id;
        assert_eq!(s.session.choose_player(player).await, Ok(Progress::Saved));
        assert_eq!(event_types(&saved_events(&s).await), vec![S, D, F]);
        assert_eq!(s.session.snapshot().score_them, 1);
    }

    /// A substitution: the player who leaves, then the one who comes in.
    #[tokio::test]
    async fn a_substitution_needs_the_replacement() {
        let mut s = scouting().await;
        s.session.choose_event(R).await.unwrap();
        let replaced = s.session.player_choices()[0].id;
        s.session.choose_player(replaced).await.unwrap();
        assert_eq!(s.session.step(), ScoutingStep::Replacement);
        let bench = s.players[7];
        assert_eq!(
            s.session
                .replacement_choices()
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            vec![bench]
        );
        assert_eq!(
            s.session.choose_replacement(Uuid::new_v4()).await,
            Err(ScoutingError::InvalidPlayerSelection)
        );
        assert_eq!(
            s.session.choose_replacement(bench).await,
            Ok(Progress::Saved)
        );
        let saved = saved_events(&s).await;
        assert_eq!(saved[0].player, Some(replaced));
        assert_eq!(saved[0].target_player, Some(bench));
    }

    /// The choices that don't fit are refused, and nothing is saved.
    #[tokio::test]
    async fn choices_that_dont_fit_are_refused() {
        let mut s = scouting().await;
        // we serve: no reception
        assert_eq!(
            s.session.choose_event(P).await,
            Err(ScoutingError::EventNotAvailable(P))
        );
        assert_eq!(
            s.session.choose_player(s.players[0]).await,
            Err(ScoutingError::InvalidPlayerSelection)
        );
        assert_eq!(
            s.session.choose_eval(EvalEnum::Perfect).await,
            Err(ScoutingError::InvalidSelection)
        );
        s.session.choose_event(S).await.unwrap();
        assert_eq!(
            s.session.choose_technique(TechniqueEnum::Set(
                crate::shapes::technique::SetTypeEnum::High
            )),
            Err(ScoutingError::InvalidSelection)
        );
        assert!(saved_events(&s).await.is_empty());
        assert_eq!(
            ScoutingError::EventNotAvailable(P).message(EN),
            "event P is not available"
        );
    }

    /// An event that doesn't apply isn't saved: up to v0.3.0 it was, and the
    /// set didn't open anymore.
    #[tokio::test]
    async fn event_that_does_not_apply_is_not_saved() {
        let mut s = scouting().await;
        let event = |event_type, player, target_player| EventEntry {
            timestamp: Utc::now(),
            event_type,
            player,
            eval: None,
            target_player,
            technique: None,
        };
        // a player who isn't on court is substituted
        assert_eq!(
            s.session
                .save(event(R, Some(Uuid::new_v4()), Some(Uuid::new_v4())))
                .await,
            Err(ScoutingError::CouldNotAddEvent)
        );
        assert!(s.session.set().events.is_empty());
        s.session.save(event(OE, None, None)).await.unwrap();
        assert_eq!(s.session.snapshot().score_us, 1);
        // the set opens with the event that applied only
        assert_eq!(event_types(&saved_events(&s).await), vec![OE]);
    }

    /// Undo goes back a choice; before the first one it removes the last
    /// event and resumes it, with its serve type.
    #[tokio::test]
    async fn undo_goes_back_a_choice_then_an_event() {
        let mut s = scouting().await;
        s.session.choose_event(S).await.unwrap();
        let spin = TechniqueEnum::Serve(ServeTypeEnum::Spin);
        s.session.choose_technique(spin).unwrap();
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        s.session.choose_event(D).await.unwrap();
        let digger = s.session.player_choices()[0].id;
        s.session.choose_player(digger).await.unwrap();

        s.session.undo().await.unwrap();
        assert_eq!(
            (s.session.step(), s.session.player()),
            (ScoutingStep::Player, None)
        );
        s.session.undo().await.unwrap();
        assert_eq!(
            (s.session.step(), s.session.event()),
            (ScoutingStep::Event, None)
        );
        // the serve again, to evaluate
        s.session.undo().await.unwrap();
        assert_eq!(
            (s.session.step(), s.session.event(), s.session.technique()),
            (ScoutingStep::Eval, Some(S), Some(spin))
        );
        assert!(s.session.set().events.is_empty());
        assert!(saved_events(&s).await.is_empty());
        // the server isn't chosen: back to the events
        s.session.undo().await.unwrap();
        assert_eq!(
            (s.session.step(), s.session.event()),
            (ScoutingStep::Event, None)
        );
        // nothing left to undo
        assert_eq!(s.session.undo().await, Ok(()));
    }

    /// The point that wins the set ends it: the set joins the match.
    #[tokio::test]
    async fn the_winning_point_ends_the_set() {
        let mut s = scouting().await;
        for _ in 0..24 {
            assert_eq!(s.session.choose_event(OE).await, Ok(Progress::Saved));
        }
        assert!(s.session.current_match().sets.is_empty());
        assert_eq!(s.session.choose_event(OE).await, Ok(Progress::SetOver));
        assert_eq!(s.session.current_match().sets.len(), 1);
        assert_eq!(s.session.current_match().sets[0].events.len(), 25);
        // undone, the set is open again
        s.session.undo().await.unwrap();
        assert_eq!(s.session.current_match().sets[0].events.len(), 24);
        assert!(
            s.session
                .current_match()
                .get_status()
                .is_ok_and(|status| status.us_wins == 0)
        );
    }

    /// A set where we serve, the player chosen first.
    async fn player_first() -> Scouting {
        let s = scouting().await;
        Scouting {
            session: s.session.with_order(ChoiceOrder::PlayerFirst),
            ..s
        }
    }

    /// With the player first, our serve waits for its evaluation: the
    /// server is known. The other events are there too.
    #[tokio::test]
    async fn with_the_player_first_the_serve_needs_its_evaluation_only() {
        let mut s = player_first().await;
        assert_eq!(
            (s.session.step(), s.session.event(), s.session.player()),
            (ScoutingStep::Eval, Some(S), Some(s.players[0]))
        );
        assert_eq!(s.session.other_events(), vec![OE, F, R, CS]);
        assert_eq!(s.session.player_events(&s.players[0]), vec![]);
        assert_eq!(
            s.session.choose_eval(EvalEnum::Perfect).await,
            Ok(Progress::Saved)
        );
        // still serving, again to evaluate
        assert_eq!(
            (s.session.step(), s.session.event()),
            (ScoutingStep::Eval, Some(S))
        );
        // another event takes the place of the serve
        assert_eq!(s.session.choose_event(OE).await, Ok(Progress::Saved));
        assert_eq!(s.session.snapshot().score_us, 2);
        assert_eq!(s.session.choose_event(F).await, Ok(Progress::Choosing));
        assert_eq!(s.session.step(), ScoutingStep::Player);
        // back to the serve
        s.session.undo().await.unwrap();
        assert_eq!(
            (s.session.step(), s.session.event()),
            (ScoutingStep::Eval, Some(S))
        );
        // an event not possible leaves the serve as it was
        assert_eq!(
            s.session.choose_event(P).await,
            Err(ScoutingError::EventNotAvailable(P))
        );
        assert_eq!(s.session.event(), Some(S));
        // nothing to go back to: the undo removes the last event, and we
        // serve again
        s.session.undo().await.unwrap();
        assert_eq!(event_types(&saved_events(&s).await), vec![S]);
        assert_eq!(
            (s.session.step(), s.session.event()),
            (ScoutingStep::Eval, Some(S))
        );
        // the ace is removed, to evaluate again
        s.session.undo().await.unwrap();
        assert!(saved_events(&s).await.is_empty());
        assert_eq!(
            (s.session.step(), s.session.event()),
            (ScoutingStep::Eval, Some(S))
        );
        // nothing left to undo
        assert_eq!(s.session.undo().await, Ok(()));
    }

    /// With the player first, the event is chosen only when the player can
    /// play more than one.
    #[tokio::test]
    async fn with_the_player_first_the_event_comes_if_more_than_one() {
        let mut s = player_first().await;
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        // they receive and attack: the setter is in the back row, a middle
        // blocker in the front row
        let setter = s.players[0];
        let blocker = s.players[2];
        assert_eq!(s.session.player_events(&setter), vec![D]);
        assert_eq!(s.session.player_events(&blocker), vec![D, B]);
        assert_eq!(s.session.other_events(), vec![OS, OE, F, OF]);

        assert_eq!(
            s.session.choose_player_first(blocker).await,
            Ok(Progress::Choosing)
        );
        assert_eq!(
            (s.session.step(), s.session.player()),
            (ScoutingStep::Event, Some(blocker))
        );
        assert_eq!(s.session.other_events(), vec![]);
        s.session.choose_event(B).await.unwrap();
        assert_eq!(
            (s.session.step(), s.session.event()),
            (ScoutingStep::Eval, Some(B))
        );
        // back to the events of the player, then to the players
        s.session.undo().await.unwrap();
        assert_eq!(
            (s.session.step(), s.session.event(), s.session.player()),
            (ScoutingStep::Event, None, Some(blocker))
        );
        s.session.undo().await.unwrap();
        assert_eq!(s.session.player(), None);

        // one event: straight to the evaluation
        assert_eq!(
            s.session.choose_player_first(setter).await,
            Ok(Progress::Choosing)
        );
        assert_eq!(
            (s.session.step(), s.session.event()),
            (ScoutingStep::Eval, Some(D))
        );
        s.session.undo().await.unwrap();
        assert_eq!(
            (s.session.step(), s.session.player()),
            (ScoutingStep::Event, None)
        );
        s.session.choose_player_first(setter).await.unwrap();
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();

        // a player not on court plays nothing
        assert_eq!(
            s.session.choose_player_first(s.players[7]).await,
            Err(ScoutingError::InvalidPlayerSelection)
        );
        // the setter can attack or send a freeball: the freeball is saved
        // with the player
        assert_eq!(s.session.player_events(&setter), vec![A, FB]);
        s.session.choose_player_first(setter).await.unwrap();
        assert_eq!(s.session.choose_event(FB).await, Ok(Progress::Saved));
        // undone, back to the events of the player
        s.session.undo().await.unwrap();
        assert_eq!(
            (s.session.step(), s.session.player()),
            (ScoutingStep::Event, Some(setter))
        );
        // another event drops the player
        assert_eq!(s.session.choose_event(OE).await, Ok(Progress::Saved));
        assert_eq!(event_types(&saved_events(&s).await), vec![S, D, OE]);
    }

    /// The statistics of the match count the sets before and this one so
    /// far.
    #[tokio::test]
    async fn the_statistics_of_the_match_count_every_set() {
        let mut s = player_first().await;
        let server = s.players[0];
        // an ace in set 1
        assert_eq!(
            s.session.choose_eval(EvalEnum::Perfect).await,
            Ok(Progress::Saved)
        );
        let mut m = s.session.current_match().clone();
        m.sets.push(s.session.set().clone());
        // set 2, we serve again: a serve error
        let set_writer = Arc::new(StoreSetWriter::new(
            Arc::clone(&s.store),
            Language::default(),
        ));
        let set = set_writer
            .create(
                &m,
                2,
                TeamSideEnum::Us,
                s.players[..6].try_into().unwrap(),
                s.players[6],
                None,
                server,
                vec![],
            )
            .await
            .unwrap();
        let snapshot = set.compute_snapshot(m.max_substitutions).unwrap();
        let mut session =
            ScoutingSession::new(m, set, snapshot, set_writer).with_order(ChoiceOrder::PlayerFirst);
        let efficiency = |session: &ScoutingSession| {
            session.match_stats().metric_percentage(
                S,
                Metric::Efficiency,
                &StatsFilter::new(None, None, Some(server)),
            )
        };
        assert_eq!(efficiency(&session), Some(100.0));
        assert_eq!(
            session.choose_eval(EvalEnum::Error).await,
            Ok(Progress::Saved)
        );
        assert_eq!(efficiency(&session), Some(0.0));
        // the sets up to this one, this one as scouted so far
        let sets: Vec<_> = session
            .sets()
            .iter()
            .map(|s| (s.set_number, s.events.len()))
            .collect();
        assert_eq!(sets, vec![(1, 1), (2, 1)]);
    }
}
