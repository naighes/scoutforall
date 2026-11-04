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
            court_zone::{CourtCell, CourtSide, CourtTarget, CourtZoneEnum, SubzoneEnum},
            enums::{EvalEnum, EventTypeEnum, RoleEnum, TeamSideEnum},
            flags::ReceptionSchemeEnum,
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

    /// The roles of the team: the starting six, the libero and a middle
    /// blocker on the bench.
    const ROLES: [RoleEnum; 8] = [
        RoleEnum::Setter,
        RoleEnum::OutsideHitter,
        RoleEnum::MiddleBlocker,
        RoleEnum::OppositeHitter,
        RoleEnum::OutsideHitter,
        RoleEnum::MiddleBlocker,
        RoleEnum::Libero,
        RoleEnum::MiddleBlocker,
    ];

    /// A set in memory where we serve.
    async fn scouting() -> Scouting {
        scouting_of(&ROLES, ROLES.len()).await
    }

    /// A set in memory where we serve, for a team with `roles` and a match
    /// with its first `roster` players on the roster: without a roster for
    /// the whole team.
    async fn scouting_of(roles: &[RoleEnum], roster: usize) -> Scouting {
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let players: Vec<PlayerEntry> = roles
            .iter()
            .copied()
            .enumerate()
            .map(|(i, role)| PlayerEntry {
                id: Uuid::new_v4(),
                name: format!("player {}", i + 1),
                role: Some(role),
                number: i as u8 + 1,
                deleted: false,
                birth_date: None,
                height_cm: None,
                notes: String::new(),
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
                country: None,
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
                String::new(),
                match roster == roles.len() {
                    true => vec![],
                    false => ids[..roster].to_vec(),
                },
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

    /// Where our serve went is told along with its evaluation, or not:
    fn cell(zone: u8, subzone: SubzoneEnum) -> CourtCell {
        CourtCell::new(zone, subzone).expect("a cell")
    }

    fn z43() -> CourtTarget {
        CourtTarget::pair(cell(3, SubzoneEnum::D), cell(4, SubzoneEnum::A)).expect("side by side")
    }

    /// saved in the flags, none when not told; on their court.
    #[tokio::test]
    async fn the_serve_target_is_optional() {
        let mut s = scouting().await;
        assert!(!s.session.targets_apply());
        assert_eq!(
            s.session.choose_target(Some(CourtTarget::Zone(1))),
            Err(ScoutingError::InvalidSelection)
        );
        s.session.choose_event(S).await.unwrap();
        assert!(s.session.targets_apply());
        assert_eq!(s.session.target_side(), Some(CourtSide::Theirs));
        assert_eq!(s.session.target(), None);
        // 3D and 4A: on the line between 3 and 4
        assert_eq!(s.session.touch_target(cell(3, SubzoneEnum::D)), Ok(()));
        assert_eq!(s.session.touch_target(cell(4, SubzoneEnum::A)), Ok(()));
        assert_eq!(s.session.target(), Some(z43()));
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        // not told: their dig and their attack, our dig, our serve again
        s.session.choose_event(OE).await.ok();
        assert_eq!(s.session.target(), None);
        s.session.choose_event(S).await.unwrap();
        s.session.touch_target(cell(6, SubzoneEnum::A)).unwrap();
        s.session.choose_target(None).unwrap();
        s.session.choose_eval(EvalEnum::Error).await.unwrap();
        // their serve: our reception, not told
        s.session.choose_event(P).await.unwrap();
        s.session.choose_player(s.players[6]).await.unwrap();
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        let saved = saved_events(&s).await;
        assert_eq!(
            saved.iter().map(|e| e.flags.target()).collect::<Vec<_>>(),
            vec![Some(z43()), None, None, None]
        );
    }

    /// Where we received their serve is told along with the evaluation of
    /// the reception, on our court, with its formation; not before the
    /// receiver. The attack after it goes to their court.
    #[tokio::test]
    async fn the_reception_target_is_on_our_court() {
        let mut s = scouting().await;
        s.session.choose_event(S).await.unwrap();
        s.session.choose_eval(EvalEnum::Error).await.unwrap();
        s.session.next_reception().unwrap();
        s.session.choose_event(P).await.unwrap();
        assert!(!s.session.targets_apply());
        s.session.choose_player(s.players[6]).await.unwrap();
        assert_eq!(s.session.target_side(), Some(CourtSide::Ours));
        // 1D and 6A: on the line between 1 and 6
        s.session.touch_target(cell(1, SubzoneEnum::D)).unwrap();
        s.session.touch_target(cell(6, SubzoneEnum::A)).unwrap();
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        s.session.choose_event(A).await.unwrap();
        s.session.choose_player(s.players[3]).await.unwrap();
        assert_eq!(s.session.target_side(), Some(CourtSide::Theirs));
        let saved = saved_events(&s).await;
        let reception = saved
            .iter()
            .find(|e| e.event_type == P)
            .expect("the reception");
        assert_eq!(
            (reception.flags.target(), reception.flags.reception()),
            (
                CourtTarget::pair(cell(1, SubzoneEnum::D), cell(6, SubzoneEnum::A)),
                ReceptionSchemeEnum::Four
            )
        );
        // undo resumes it with where it was received
        for _ in 0..3 {
            if s.session.event() != Some(P) {
                s.session.undo().await.unwrap();
            }
        }
        assert_eq!(
            (
                s.session.step(),
                s.session.target().and_then(|t| t.conflict())
            ),
            (ScoutingStep::Eval, Some(CourtZoneEnum::Z61))
        );
    }

    /// Our attack: where it went on their court, saved with its evaluation;
    /// not before the attacker.
    #[tokio::test]
    async fn the_attack_target_is_on_their_court() {
        let mut s = scouting().await;
        s.session.choose_event(S).await.unwrap();
        s.session.choose_eval(EvalEnum::Error).await.unwrap();
        s.session.choose_event(P).await.unwrap();
        s.session.choose_player(s.players[6]).await.unwrap();
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        s.session.choose_event(A).await.unwrap();
        assert!(!s.session.targets_apply());
        s.session.choose_player(s.players[3]).await.unwrap();
        assert_eq!(s.session.target_side(), Some(CourtSide::Theirs));
        // 5A, then 5D next to it: one zone
        s.session.touch_target(cell(5, SubzoneEnum::A)).unwrap();
        s.session.touch_target(cell(5, SubzoneEnum::D)).unwrap();
        s.session.choose_eval(EvalEnum::Perfect).await.unwrap();
        let saved = saved_events(&s).await;
        let attack = saved
            .iter()
            .find(|e| e.event_type == A)
            .expect("the attack");
        assert_eq!(
            attack.flags.target(),
            CourtTarget::pair(cell(5, SubzoneEnum::A), cell(5, SubzoneEnum::D))
        );
    }

    /// Undo resumes the serve with where it went; going back from the
    /// evaluation forgets it.
    #[tokio::test]
    async fn undo_resumes_the_serve_with_its_target() {
        let mut s = scouting().await;
        s.session.choose_event(S).await.unwrap();
        s.session.touch_target(cell(1, SubzoneEnum::B)).unwrap();
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        assert_eq!(s.session.target(), None);
        s.session.undo().await.unwrap();
        assert_eq!(
            (s.session.step(), s.session.target()),
            (
                ScoutingStep::Eval,
                Some(CourtTarget::Cell(cell(1, SubzoneEnum::B)))
            )
        );
        s.session.undo().await.unwrap();
        assert_eq!(s.session.target(), None);
    }

    /// The reception is told before their serve, for that serve only: it
    /// goes on the reception, or on their ace, then it's three again.
    #[tokio::test]
    async fn the_reception_is_told_for_their_next_serve() {
        use ReceptionSchemeEnum::*;
        let mut s = scouting().await;
        // we serve
        assert_eq!(
            s.session.next_reception(),
            Err(ScoutingError::ReceptionNotNow)
        );
        s.session.choose_event(S).await.unwrap();
        s.session.choose_eval(EvalEnum::Error).await.unwrap();
        // they serve: three, four, two
        assert!(s.session.can_tell_reception());
        assert_eq!(s.session.reception(), Three);
        assert_eq!(s.session.next_reception(), Ok(Four));
        assert_eq!(s.session.next_reception(), Ok(Two));
        s.session.choose_event(P).await.unwrap();
        s.session.choose_player(s.players[6]).await.unwrap();
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        // the rally goes on: three, and not to be told
        assert_eq!(s.session.reception(), Three);
        assert!(!s.session.can_tell_reception());
        s.session.choose_event(OE).await.unwrap();
        // their ace, with four
        s.session.choose_event(S).await.unwrap();
        s.session.choose_eval(EvalEnum::Error).await.unwrap();
        assert_eq!(s.session.next_reception(), Ok(Four));
        s.session.choose_event(OS).await.unwrap();
        assert_eq!(s.session.reception(), Three);
        // their serve error: not written, three again
        assert_eq!(s.session.next_reception(), Ok(Four));
        s.session.choose_event(OE).await.unwrap();
        assert_eq!(s.session.reception(), Three);
        let receptions: Vec<_> = saved_events(&s)
            .await
            .iter()
            .map(|e| (e.event_type, e.flags.reception()))
            .collect();
        assert_eq!(
            receptions,
            [
                (S, Three),
                (P, Two),
                (OE, Three),
                (S, Three),
                (OS, Four),
                (OE, Three),
            ]
        );
        assert_eq!(saved_events(&s).await[4].flags.to_string(), "2");
        // undone, their ace gives back its reception
        s.session.undo().await.unwrap();
        assert_eq!(s.session.reception(), Three);
        s.session.undo().await.unwrap();
        assert_eq!(s.session.reception(), Four);
    }

    /// The inverted P1 is a state: every event saved from when it's set
    /// says so, until it's unset; a session opened again on the set, or on
    /// the next one, goes on as the last event says.
    #[tokio::test]
    async fn the_inverted_p1_goes_on_until_changed() {
        let mut s = scouting().await;
        assert!(!s.session.inverted_p1());
        s.session.choose_event(S).await.unwrap();
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        s.session.set_inverted_p1(true);
        // our libero digs their attack, they err
        s.session.choose_event(D).await.unwrap();
        s.session.choose_player(s.players[6]).await.unwrap();
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        s.session.choose_event(OE).await.unwrap();
        let inverted = |events: &[EventEntry]| {
            events
                .iter()
                .map(|e| e.flags.inverted_p1())
                .collect::<Vec<_>>()
        };
        assert_eq!(inverted(&saved_events(&s).await), [false, true, true]);
        // opened again: still inverted
        let mut m = s.session.current_match().clone();
        let set = s.session.set().clone();
        let snapshot = set.compute_snapshot(m.max_substitutions).unwrap();
        let set_writer = Arc::new(StoreSetWriter::new(
            Arc::clone(&s.store),
            Language::default(),
        ));
        let mut again = ScoutingSession::new(m.clone(), set.clone(), snapshot, set_writer.clone());
        assert!(again.inverted_p1());
        again.set_inverted_p1(false);
        again.choose_event(OE).await.unwrap();
        assert_eq!(
            inverted(&saved_events(&s).await),
            [false, true, true, false]
        );
        // the next set as the last event of the one before
        m.sets.push(again.set().clone());
        let next = set_writer
            .create(
                &m,
                2,
                TeamSideEnum::Us,
                s.players[..6].try_into().unwrap(),
                s.players[6],
                None,
                s.players[0],
                vec![],
            )
            .await
            .unwrap();
        let snapshot = next.compute_snapshot(m.max_substitutions).unwrap();
        assert!(
            !ScoutingSession::new(
                m.clone(),
                next.clone(),
                snapshot.clone(),
                set_writer.clone()
            )
            .inverted_p1()
        );
        if let Some(last) = m.sets[0].events.last_mut() {
            last.flags = last.flags.with_inverted_p1(true);
        }
        assert!(ScoutingSession::new(m, next, snapshot, set_writer).inverted_p1());
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

    /// Our fault: the player making it (e.g. a setter's double), or the
    /// team, no player (e.g. a rotation fault). Only the fault can be the
    /// team's: the events requiring the player refuse it.
    #[tokio::test]
    async fn a_fault_is_a_players_or_the_teams() {
        let mut s = scouting().await;
        let refused = Err(ScoutingError::InvalidPlayerSelection);
        // nothing chosen yet, and the serve: no team
        assert!(!s.session.team_choice());
        assert_eq!(s.session.choose_team().await, refused);
        s.session.choose_event(S).await.unwrap();
        assert!(!s.session.team_choice());
        assert_eq!(s.session.choose_team().await, refused);
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        for event in [D, B, A] {
            s.session.choose_event(event).await.unwrap();
            assert_eq!(s.session.step(), ScoutingStep::Player, "{event}");
            assert!(!s.session.team_choice(), "{event}");
            assert_eq!(s.session.choose_team().await, refused, "{event}");
            s.session.undo().await.unwrap();
        }
        assert_eq!(event_types(&saved_events(&s).await), vec![S]);
        // the fault: the team, or the players on court
        s.session.choose_event(F).await.unwrap();
        assert_eq!(s.session.step(), ScoutingStep::Player);
        assert!(s.session.team_choice());
        assert_eq!(s.session.player_choices().len(), 6);
        assert_eq!(s.session.choose_team().await, Ok(Progress::Saved));
        assert!(!s.session.team_choice());
        assert_eq!(s.session.step(), ScoutingStep::Event);
        // they serve: a fault of the setter, named
        s.session.choose_event(F).await.unwrap();
        assert!(s.session.team_choice());
        let setter = s.players[0];
        assert_eq!(s.session.choose_player(setter).await, Ok(Progress::Saved));
        let saved = saved_events(&s).await;
        assert_eq!(event_types(&saved), vec![S, F, F]);
        assert_eq!(
            saved.iter().map(|e| e.player).collect::<Vec<_>>(),
            vec![Some(setter), None, Some(setter)]
        );
        assert_eq!(s.session.snapshot().score_them, 2);
        // the team's fault undone, back to the team choice
        s.session.choose_event(F).await.unwrap();
        s.session.undo().await.unwrap();
        assert_eq!(s.session.step(), ScoutingStep::Event);
        assert!(!s.session.team_choice());
    }

    /// With the player first, the fault comes before its player: the team
    /// can make it too.
    #[tokio::test]
    async fn with_the_player_first_the_fault_can_be_the_teams() {
        let mut s = player_first().await;
        assert!(s.session.other_events().contains(&F));
        s.session.choose_event(F).await.unwrap();
        assert!(s.session.team_choice());
        assert_eq!(s.session.choose_team().await, Ok(Progress::Saved));
        let saved = saved_events(&s).await;
        assert_eq!(
            saved
                .iter()
                .map(|e| (e.event_type, e.player))
                .collect::<Vec<_>>(),
            vec![(F, None)]
        );
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

    /// The libero out, then back in: saved at once, no player, the scout
    /// still choosing the event; the middle blocker back on court meanwhile.
    #[tokio::test]
    async fn the_libero_goes_out_and_back_in() {
        let mut s = scouting().await;
        let (middle, libero) = (s.players[5], s.players[6]);
        let on_court = |s: &Scouting, id: Uuid| {
            s.session
                .snapshot()
                .current_lineup
                .find_position(&id)
                .is_some()
        };
        assert!(on_court(&s, libero) && !on_court(&s, middle));
        assert_eq!(s.session.choose_event(LR).await, Ok(Progress::Saved));
        assert_eq!(s.session.step(), ScoutingStep::Event);
        assert!(!on_court(&s, libero) && on_court(&s, middle));
        assert!(s.session.available_events().contains(&LR));
        assert_eq!(s.session.choose_event(LR).await, Ok(Progress::Saved));
        assert!(on_court(&s, libero) && !on_court(&s, middle));
        let saved = saved_events(&s).await;
        assert_eq!(event_types(&saved), vec![LR, LR]);
        assert!(saved.iter().all(|e| e.player.is_none() && e.eval.is_none()));
        // within the rally, never
        s.session.choose_event(S).await.unwrap();
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        assert_eq!(
            s.session.choose_event(LR).await,
            Err(ScoutingError::EventNotAvailable(LR))
        );
    }

    /// A player of the team who isn't on the roster of the match can't come
    /// in: the score sheet is the roster.
    #[tokio::test]
    async fn only_the_roster_comes_in() {
        let roles = [&ROLES[..], &[RoleEnum::OutsideHitter]].concat();
        let mut s = scouting_of(&roles, ROLES.len()).await;
        let off_the_sheet = s.players[8];
        s.session.choose_event(R).await.unwrap();
        let replaced = s.session.player_choices()[0].id;
        s.session.choose_player(replaced).await.unwrap();
        assert_eq!(
            s.session
                .replacement_choices()
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            vec![s.players[7]]
        );
        assert_eq!(
            s.session.choose_replacement(off_the_sheet).await,
            Err(ScoutingError::InvalidPlayerSelection)
        );
        assert!(saved_events(&s).await.is_empty());
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
            flags: Default::default(),
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
        assert_eq!(s.session.other_events(), vec![OE, F, R, LR, CS, LF]);
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

    /// Undoes, checking that it removes a saved event only when the session
    /// isn't halfway.
    async fn undo_as_said(s: &mut Scouting) {
        let halfway = s.session.is_halfway();
        let before = s.session.set().events.len();
        s.session.undo().await.unwrap();
        assert_eq!(
            s.session.set().events.len() < before,
            !halfway && before > 0,
            "halfway {halfway}, {before} events before"
        );
    }

    /// The serve chosen by itself isn't halfway: undoing it removes the last
    /// event (up to v0.10 the back of the app undid the saved substitutions
    /// one after another, see `is_halfway`); a choice of the scout is.
    #[tokio::test]
    async fn halfway_only_after_a_choice_of_the_scout() {
        let mut s = player_first().await;
        assert!(!s.session.is_halfway());
        // a substitution: the player out, then the one in
        s.session.choose_event(R).await.unwrap();
        assert!(s.session.is_halfway());
        let out = s.players[1];
        s.session.choose_player(out).await.unwrap();
        assert!(s.session.is_halfway());
        let replacement = s.session.replacement_choices()[0].id;
        assert_eq!(
            s.session.choose_replacement(replacement).await,
            Ok(Progress::Saved)
        );
        // serving again, by itself
        assert_eq!(
            (s.session.step(), s.session.event()),
            (ScoutingStep::Eval, Some(S))
        );
        assert!(!s.session.is_halfway());
        // the substitution undone, back to its last choice
        undo_as_said(&mut s).await;
        assert_eq!(s.session.step(), ScoutingStep::Replacement);
        undo_as_said(&mut s).await;
        undo_as_said(&mut s).await;
        assert!(!s.session.is_halfway());
        // a serve, then a player chosen first
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        s.session.choose_player_first(s.players[2]).await.unwrap();
        assert!(s.session.is_halfway());
        for _ in 0..3 {
            undo_as_said(&mut s).await;
        }
        assert!(saved_events(&s).await.is_empty());
    }

    /// Only the player on the diagonal of the setter can become the setter:
    /// anyone else would move the libero. Up to v0.10 any player on court
    /// but the libero could.
    #[tokio::test]
    async fn only_the_diagonal_becomes_the_setter() {
        let mut s = scouting().await;
        let ids = |choices: Vec<crate::scouting::PlayerChoice>| -> Vec<Uuid> {
            choices.iter().map(|c| c.id).collect()
        };
        s.session.choose_event(CS).await.unwrap();
        // the setter in position 1, the opposite in 4
        assert_eq!(ids(s.session.player_choices()), vec![s.players[3]]);
        s.session.choose_player(s.players[3]).await.unwrap();
        assert_eq!(event_types(&saved_events(&s).await), vec![CS]);
        // and back
        s.session.choose_event(CS).await.unwrap();
        assert_eq!(ids(s.session.player_choices()), vec![s.players[0]]);
    }

    /// With the player first, the event is chosen only when the player can
    /// play more than one.
    #[tokio::test]
    async fn with_the_player_first_the_event_comes_if_more_than_one() {
        let mut s = player_first().await;
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();
        // they receive and attack: the setter is in the back row, a middle
        // blocker in the front row, both can attack (our dig not scouted);
        // the libero only digs. Their freeball after their reception, a
        // team event; ours too, after our dig not scouted
        let setter = s.players[0];
        let blocker = s.players[2];
        let libero = s.players[6];
        assert_eq!(s.session.player_events(&setter), vec![A, D]);
        assert_eq!(s.session.player_events(&blocker), vec![A, D, B]);
        assert_eq!(s.session.player_events(&libero), vec![D]);
        assert_eq!(s.session.other_events(), vec![OS, OE, F, FB, OF]);

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
            s.session.choose_player_first(libero).await,
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
        s.session.choose_player_first(libero).await.unwrap();
        s.session.choose_eval(EvalEnum::Positive).await.unwrap();

        // a player not on court plays nothing
        assert_eq!(
            s.session.choose_player_first(s.players[7]).await,
            Err(ScoutingError::InvalidPlayerSelection)
        );
        // the setter can attack; our freeball is a team event, among the
        // other events, saved at once with no player
        assert_eq!(s.session.player_events(&setter), vec![A]);
        assert_eq!(s.session.other_events(), vec![OE, F, FB]);
        assert_eq!(s.session.choose_event(FB).await, Ok(Progress::Saved));
        let saved = saved_events(&s).await;
        assert_eq!(event_types(&saved), vec![S, D, FB]);
        assert_eq!(saved.last().and_then(|e| e.player), None);
        // undone, back to the choice of the player
        s.session.undo().await.unwrap();
        assert_eq!(
            (s.session.step(), s.session.event(), s.session.player()),
            (ScoutingStep::Event, None, None)
        );
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
