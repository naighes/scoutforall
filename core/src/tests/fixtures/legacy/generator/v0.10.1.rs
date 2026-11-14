//! Generator of the legacy fixtures (TODO/001, step 1), for v0.10.1: a data
//! folder and the archives exported by this version (`archive::export_*`,
//! as the apps do), with the matches scouted through its own rules
//! (`Snapshot::add_event` validates every event, as the scouting screen
//! does).
//!
//!     LEGACY_OUT=<folder> cargo test -p scout4all-core gen_legacy -- --ignored --nocapture

#[cfg(test)]
mod tests {
    use crate::{
        archive::{export_match, export_team},
        localization::Language,
        shapes::enums::LanguageEnum,
        storage::{Store, fs::FsStore},
        providers::{
            store::{
                match_writer::StoreMatchWriter, set_writer::StoreSetWriter,
                team_writer::StoreTeamWriter,
            },
            match_writer::MatchWriter,
            set_writer::SetWriter,
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{
            enums::{EvalEnum, EventTypeEnum, RoleEnum, TeamSideEnum},
            r#match::MatchEntry,
            player::PlayerEntry,
            snapshot::{EventEntry, Snapshot},
            team::TeamEntry,
            technique::{ServeTypeEnum, TechniqueEnum},
        },
    };
    use chrono::{DateTime, Duration, FixedOffset, NaiveDate, Utc};
    use std::{path::PathBuf, sync::Arc};
    use uuid::Uuid;

    /// xorshift64*: the same choices every run.
    struct Rng(u64);

    impl Rng {
        fn new(seed: u64) -> Self {
            Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
        }
        fn f(&mut self) -> f64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
        }
        fn pick<T: Copy>(&mut self, choices: &[(T, f64)]) -> T {
            let total: f64 = choices.iter().map(|(_, w)| w.max(0.0)).sum();
            let mut r = self.f() * total;
            for (c, w) in choices {
                r -= w.max(0.0);
                if r < 0.0 {
                    return *c;
                }
            }
            choices[0].0
        }
    }

    const PLAYERS: [(u8, &str, RoleEnum); 13] = [
        (1, "Giulia Neri", RoleEnum::Setter),
        (2, "Marta Villa", RoleEnum::OutsideHitter),
        (3, "Sara Conti", RoleEnum::MiddleBlocker),
        (4, "Elena Ferri", RoleEnum::OppositeHitter),
        (5, "Chiara Galli", RoleEnum::OutsideHitter),
        (6, "Laura Fabbri", RoleEnum::MiddleBlocker),
        (7, "Paola Riva", RoleEnum::Libero),
        (8, "Anna Longo", RoleEnum::Libero),
        (9, "Irene Sala", RoleEnum::Setter),
        (10, "Silvia Moro", RoleEnum::OutsideHitter),
        (11, "Beatrice Gatti", RoleEnum::MiddleBlocker),
        (12, "Federica Testa", RoleEnum::OppositeHitter),
        // removed from the team: still in the descriptor
        (13, "Noemi Pace", RoleEnum::OutsideHitter),
    ];

    fn by_number(team: &TeamEntry, number: u8) -> Uuid {
        team.players
            .iter()
            .find(|p| p.number == number)
            .map(|p| p.id)
            .expect("player")
    }

    fn weight(event: EventTypeEnum) -> f64 {
        use EventTypeEnum::*;
        match event {
            R => 0.1,
            CL => 0.02,
            CS => 0.01,
            F => 0.04,
            FB => 0.15,
            OF => 0.2,
            D => 0.6,
            OS => 0.12,
            OE => 0.07,
            _ => 1.0,
        }
    }

    fn eval_weight(eval: EvalEnum) -> f64 {
        use EvalEnum::*;
        match eval {
            Perfect => 0.18,
            Error => 0.12,
            Over => 0.15,
            _ => 1.0,
        }
    }

    struct Scout<'a> {
        writer: &'a StoreSetWriter,
        rng: Rng,
        clock: DateTime<Utc>,
    }

    impl Scout<'_> {
        /// Plays the set until it's won, or until `stop` events.
        async fn play(
            &mut self,
            m: &MatchEntry,
            set_number: u8,
            serving: TeamSideEnum,
            lineup: [u8; 6],
            stop: Option<usize>,
        ) -> Option<TeamSideEnum> {
            let team = &m.team;
            let positions = lineup.map(|n| by_number(team, n));
            let setter = positions
                .iter()
                .copied()
                .find(|id| {
                    team.players
                        .iter()
                        .any(|p| p.id == *id && p.role == Some(RoleEnum::Setter))
                })
                .expect("setter");
            let set = self
                .writer
                .create(
                    m,
                    set_number,
                    serving,
                    positions,
                    by_number(team, 7),
                    Some(by_number(team, 8)),
                    setter,
                    vec![],
                )
                .await
                .expect("set");
            let mut snapshot = Snapshot::new(&set, m.max_substitutions).expect("snapshot");
            let active: Vec<&PlayerEntry> = team.players.iter().filter(|p| !p.deleted).collect();
            let mut count = 0;
            while snapshot.get_set_winner(set_number).is_none() && Some(count) != stop {
                let options: Vec<(EventTypeEnum, f64)> = snapshot
                    .get_available_options()
                    .iter()
                    .map(|e| (*e, weight(*e)))
                    .collect();
                let mut done = false;
                for _ in 0..500 {
                    let event_type = self.rng.pick(&options);
                    let players: Vec<(Uuid, f64)> = active
                        .iter()
                        .filter(|p| snapshot.can_play(event_type, &p.id))
                        .map(|p| (p.id, 1.0))
                        .collect();
                    let player = match (event_type.requires_player(), players.is_empty()) {
                        (true, true) => continue,
                        (true, false) => Some(self.rng.pick(&players)),
                        (false, _) => None,
                    };
                    let target_player = match event_type {
                        EventTypeEnum::R => {
                            let bench: Vec<(Uuid, f64)> = active
                                .iter()
                                .filter(|p| Some(p.id) != player)
                                .map(|p| (p.id, 1.0))
                                .collect();
                            Some(self.rng.pick(&bench))
                        }
                        _ => None,
                    };
                    let evals: Vec<(EvalEnum, f64)> = event_type
                        .available_evals()
                        .into_iter()
                        .map(|e| (e, eval_weight(e)))
                        .collect();
                    let eval = match evals.is_empty() {
                        true => None,
                        false => Some(self.rng.pick(&evals)),
                    };
                    let technique = match (event_type, player) {
                        (EventTypeEnum::S, Some(_)) => Some(TechniqueEnum::Serve(
                            self.rng
                                .pick(&[(ServeTypeEnum::Float, 1.0), (ServeTypeEnum::Spin, 1.0)]),
                        )),
                        (EventTypeEnum::A, Some(attacker)) => {
                            let types: Vec<_> = snapshot
                                .available_set_types(&attacker)
                                .into_iter()
                                .map(|t| (Some(t), 1.0))
                                .chain([(None, 0.5)])
                                .collect();
                            self.rng.pick(&types).map(TechniqueEnum::Set)
                        }
                        _ => None,
                    };
                    let event = EventEntry {
                        timestamp: self.clock,
                        event_type,
                        player,
                        eval,
                        target_player,
                        technique,
                    };
                    let mut next = snapshot.clone();
                    if next.add_event(&event).is_ok() {
                        self.writer
                            .append_event(m, set_number, &event)
                            .await
                            .expect("append");
                        snapshot = next;
                        self.clock += Duration::seconds(6);
                        done = true;
                        break;
                    }
                }
                assert!(done, "no valid event after {:?}", snapshot.last_event);
                count += 1;
            }
            println!(
                "{} set {set_number}: {}-{} ({count} events)",
                m.id, snapshot.score_us, snapshot.score_them
            );
            snapshot.get_set_winner(set_number)
        }
    }

    fn date(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).expect("date")
    }

    #[tokio::test]
    #[ignore]
    async fn gen_legacy() {
        let out = PathBuf::from(std::env::var("LEGACY_OUT").expect("LEGACY_OUT"));
        let data = out.join("data");
        let export = out.join("export");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&export).unwrap();

        let store: Arc<dyn Store> = Arc::new(FsStore::new(&data));
        let team_writer = StoreTeamWriter::new(store.clone());
        let mut team = team_writer
            .save(TeamInput::New {
                id: None,
                name: "Fixture Volley".to_string(),
                year: 2025,
                classification: None,
                gender: None,
                players: PLAYERS
                    .iter()
                    .map(|(number, name, role)| PlayerEntry {
                        name: name.to_string(),
                        role: Some(*role),
                        number: *number,
                        deleted: *number == 13,
                        // the details of the players (v0.10.0)
                        birth_date: (*number <= 3)
                            .then(|| NaiveDate::from_ymd_opt(2004, 3, *number as u32))
                            .flatten(),
                        height_cm: (*number <= 4).then_some(170 + *number as u16),
                        notes: match number {
                            1 => "capitana, mancina".to_string(),
                            _ => String::new(),
                        },
                        ..Default::default()
                    })
                    .collect(),
            })
            .await
            .unwrap();
        team.players.sort_by_key(|p| p.number);

        let match_writer = StoreMatchWriter::new(store.clone());
        let set_writer = StoreSetWriter::new(store.clone(), Language::new(LanguageEnum::En));
        let mut scout = Scout {
            writer: &set_writer,
            rng: Rng::new(9),
            clock: "2025-10-04T16:00:00Z".parse().unwrap(),
        };

        // a finished match, at home
        let mut finished = match_writer
            .create(&team, "Rivali Pisa".into(), date("2025-10-04T18:00:00+02:00"), true, 8, "notes of the match: rientro dopo l'infortunio".into())
            .await
            .unwrap();
        finished.team = team.clone();
        let lineups = [[1, 2, 3, 4, 5, 6], [4, 5, 6, 1, 2, 3], [2, 3, 4, 5, 6, 1], [5, 6, 1, 2, 3, 4], [1, 2, 3, 4, 5, 6]];
        let mut serving = TeamSideEnum::Us;
        let (mut us, mut them) = (0, 0);
        for n in 1..=5u8 {
            match scout.play(&finished, n, serving, lineups[(n - 1) as usize], None).await {
                Some(TeamSideEnum::Us) => us += 1,
                Some(TeamSideEnum::Them) => them += 1,
                None => panic!("set not finished"),
            }
            serving = match serving {
                TeamSideEnum::Us => TeamSideEnum::Them,
                TeamSideEnum::Them => TeamSideEnum::Us,
            };
            if us == 3 || them == 3 {
                break;
            }
        }

        // a match in progress: one set and part of the second, away
        scout.clock = "2025-10-11T15:00:00Z".parse().unwrap();
        let mut in_progress = match_writer
            .create(&team, "Polisportiva Lucca".into(), date("2025-10-11T17:00:00+02:00"), false, 6, String::new())
            .await
            .unwrap();
        in_progress.team = team.clone();
        scout.play(&in_progress, 1, TeamSideEnum::Them, [9, 10, 11, 12, 5, 6], None).await;
        scout.play(&in_progress, 2, TeamSideEnum::Us, [1, 2, 3, 4, 5, 6], Some(40)).await;

        // a match without sets
        let empty = match_writer
            .create(&team, "Volley Siena".into(), date("2025-10-18T21:00:00+02:00"), true, 6, String::new())
            .await
            .unwrap();

        let zip = export_team(&*store, &team.id).await.unwrap();
        std::fs::write(export.join(format!("{}.zip", team.id)), zip).unwrap();
        for m in [&finished, &in_progress, &empty] {
            let zip = export_match(&*store, &team.id, &m.id).await.unwrap();
            std::fs::write(export.join(format!("{}.zip", m.id)), zip).unwrap();
        }
    }
}
