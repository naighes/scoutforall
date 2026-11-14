//! Generator of the legacy fixtures (TODO/001, step 1), for v0.18.0: a data
//! folder and the archives exported by this version (`archive::export_*`,
//! as the apps do), with the matches scouted through its own rules
//! (`Snapshot::add_event` validates every event, as the scouting screen
//! does).
//!
//!     LEGACY_OUT=<folder> cargo test -p scout4all-core gen_legacy -- --ignored --nocapture

#[cfg(test)]
mod tests {
    use crate::{
        analytics::{
            finished::{FinishedMatches, QueuedFinishedMatches},
            queue_manager::QueueManager,
        },
        archive::{export_match, export_team},
        dvw::{
            self,
            import::{TeamChoice, match_date, match_plan, save_import, team_plan},
            model::Side,
        },
        localization::Language,
        shapes::enums::LanguageEnum,
        storage::{Store, fs::FsStore},
        video::{Anchor, EventRef, VideoFile, VideoProfile, video_midnight, write_profile},
        providers::{
            store::{
                match_reader::StoreMatchReader, match_writer::StoreMatchWriter,
                queue_reader::StoreQueueReader, queue_writer::StoreQueueWriter,
                set_reader::StoreSetReader, set_writer::StoreSetWriter,
                team_writer::StoreTeamWriter,
            },
            match_reader::MatchReader,
            match_writer::MatchWriter,
            set_writer::SetWriter,
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{
            country::CountryCode,
            enums::{EvalEnum, EventTypeEnum, GenderEnum, RoleEnum, TeamClassificationEnum, TeamSideEnum},
            r#match::{MatchEntry, MatchSource},
            player::PlayerEntry,
            snapshot::{EventEntry, Snapshot},
            team::TeamEntry,
            court_zone::{CourtCell, CourtTarget, ServeStartEnum, SubzoneEnum, TargetKind},
            flags::{BlockersEnum, EventFlags, ReceptionSchemeEnum},
            technique::{ServeTypeEnum, TechniqueEnum},
        },
    };
    use chrono::{DateTime, Duration, FixedOffset, NaiveDate, Utc};
    use std::{
        path::PathBuf,
        sync::{Arc, atomic::AtomicBool},
    };
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

    /// The roster of a match (v0.11.0): the players of the team but `out`.
    fn roster(team: &TeamEntry, out: &[u8]) -> Vec<Uuid> {
        team.players
            .iter()
            .filter(|p| !p.deleted && !out.contains(&p.number))
            .map(|p| p.id)
            .collect()
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
        fn target(&mut self, event_type: EventTypeEnum) -> Option<CourtTarget> {
            let subzones = SubzoneEnum::ALL.map(|s| (s, 1.0));
            match (TargetKind::of(event_type), self.rng.f()) {
                (None, _) => None,
                (Some(_), r) if r < 0.3 => None,
                (Some(TargetKind::Net), _) => {
                    let zone = self.rng.pick(&[(2, 1.0), (3, 1.0), (4, 1.0)]);
                    CourtTarget::of_zone(zone, None)
                }
                (Some(TargetKind::Cells), r) if r < 0.8 => {
                    let zone = (self.rng.f() * 9.0) as u8 + 1;
                    let subzone = self.rng.pick(&subzones);
                    CourtTarget::of_zone(zone, Some(subzone))
                }
                (Some(TargetKind::Cells), r) if r < 0.9 => {
                    let zone = (self.rng.f() * 9.0) as u8 + 1;
                    CourtTarget::of_zone(zone, None)
                }
                (Some(TargetKind::Cells), _) => (0..50).find_map(|_| {
                    let a = CourtCell::new((self.rng.f() * 9.0) as u8 + 1, self.rng.pick(&subzones))?;
                    let b = CourtCell::new((self.rng.f() * 9.0) as u8 + 1, self.rng.pick(&subzones))?;
                    CourtTarget::pair(a, b)
                }),
            }
        }

        fn blockers(&mut self, event_type: EventTypeEnum) -> Option<BlockersEnum> {
            let all: Vec<(Option<BlockersEnum>, f64)> = BlockersEnum::of(event_type)
                .iter()
                .map(|b| (Some(*b), 1.0))
                .chain([(None, 1.5)])
                .collect();
            self.rng.pick(&all)
        }

        /// Plays the set until it's won, or until `stop` events.
        async fn play(
            &mut self,
            m: &MatchEntry,
            set_number: u8,
            serving: TeamSideEnum,
            lineup: [u8; 6],
            stop: Option<usize>,
            inverted_p1: bool,
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
            // the players of the match (v0.11.0): the roster, or the team
            let active: Vec<&PlayerEntry> = team
                .players
                .iter()
                .filter(|p| !p.deleted && (m.roster.is_empty() || m.roster.contains(&p.id)))
                .collect();
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
                    // the flags (v0.11.0), as the scouting sets them: the
                    // inverted P1 on every event while told, the reception
                    // on the reception
                    let flags = EventFlags::default().with_inverted_p1(inverted_p1);
                    let flags = match event_type {
                        EventTypeEnum::P => flags.with_reception(self.rng.pick(&[
                            (ReceptionSchemeEnum::Three, 0.8),
                            (ReceptionSchemeEnum::Four, 0.1),
                            (ReceptionSchemeEnum::Two, 0.1),
                        ])),
                        _ => flags,
                    };
                    // where the ball went and the block (v0.13.0), when told
                    let flags = flags
                        .with_target(self.target(event_type))
                        .with_blockers(self.blockers(event_type))
                        // where our serve starts from (v0.16.0), when told
                        .with_serve_start(match (event_type, self.rng.f()) {
                            (EventTypeEnum::S, r) if r < 0.6 => {
                                Some(self.rng.pick(&ServeStartEnum::ALL.map(|z| (z, 1.0))))
                            }
                            _ => None,
                        });
                    let event = EventEntry {
                        timestamp: self.clock,
                        event_type,
                        player,
                        eval,
                        target_player,
                        technique,
                        flags,
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
                // the country of the team (v0.12.0)
                country: CountryCode::known("IT"),
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
            .create(&team, "Rivali Pisa".into(), date("2025-10-04T18:00:00+02:00"), true, 8, "notes of the match: rientro dopo l'infortunio".into(), roster(&team, &[11]))
            .await
            .unwrap();
        finished.team = team.clone();
        let lineups = [[1, 2, 3, 4, 5, 6], [4, 5, 6, 1, 2, 3], [2, 3, 4, 5, 6, 1], [5, 6, 1, 2, 3, 4], [1, 2, 3, 4, 5, 6]];
        let mut serving = TeamSideEnum::Us;
        let (mut us, mut them) = (0, 0);
        for n in 1..=5u8 {
            match scout.play(&finished, n, serving, lineups[(n - 1) as usize], None, n == 2).await {
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
            .create(&team, "Polisportiva Lucca".into(), date("2025-10-11T17:00:00+02:00"), false, 6, String::new(), roster(&team, &[]))
            .await
            .unwrap();
        in_progress.team = team.clone();
        scout.play(&in_progress, 1, TeamSideEnum::Them, [9, 10, 11, 12, 5, 6], None, false).await;
        scout.play(&in_progress, 2, TeamSideEnum::Us, [1, 2, 3, 4, 5, 6], Some(40), false).await;

        // a match without sets
        let empty = match_writer
            .create(&team, "Volley Siena".into(), date("2025-10-18T21:00:00+02:00"), true, 6, String::new(), roster(&team, &[10, 12]))
            .await
            .unwrap();

        // a match scouted on its video (v0.15.0): the times of the events
        // are those of the video, the file and the anchor in its profile
        let video = match_writer
            .create(
                &team,
                "Pallavolo Prato".into(),
                date("2025-11-08T18:30:00+01:00"),
                false,
                6,
                String::new(),
                roster(&team, &[]),
            )
            .await
            .unwrap();
        let mut video = match_writer
            .save_source(&video, MatchSource::Scout4all, true)
            .await
            .unwrap();
        video.team = team.clone();
        scout.clock = video_midnight(&video) + Duration::seconds(95);
        scout.play(&video, 1, TeamSideEnum::Us, [1, 2, 3, 4, 5, 6], None, false).await;
        scout.play(&video, 2, TeamSideEnum::Them, [4, 5, 6, 1, 2, 3], Some(25), false).await;
        let first = EventRef { set_number: 1, index: 0 };
        let profile = VideoProfile {
            files: vec![
                VideoFile {
                    path: "videos/20251108 182500.mp4".into(),
                    start: Some(date("2025-11-08T18:25:00+01:00")),
                },
                VideoFile { path: "/Volumes/camera/parte 2.mp4".into(), start: None },
            ],
            anchor: Some(Anchor {
                at: video_midnight(&video) + Duration::seconds(95),
                offset_ms: 312_000,
                event: Some(first),
            }),
            ..Default::default()
        }
        .corrected(EventRef { set_number: 1, index: 7 }, -1500);
        write_profile(&*store, &team.id, &video.id, &profile).await.unwrap();

        // the same players in two teams: the core doesn't do it by itself,
        // but the archives may (the team of an archive keeps the ids of its
        // players, `teams_of_match`); the migration must not merge them
        // (TODO/001, D5)
        let mut shared = team_writer
            .save(TeamInput::New {
                id: None,
                name: "Fixture Volley U19".to_string(),
                year: 2025,
                classification: None,
                gender: None,
                country: CountryCode::known("IT"),
                players: team
                    .players
                    .iter()
                    .filter(|p| p.number <= 8)
                    .cloned()
                    .chain([PlayerEntry {
                        name: "Viola Marchi".to_string(),
                        role: Some(RoleEnum::OppositeHitter),
                        number: 14,
                        ..Default::default()
                    }])
                    .collect(),
            })
            .await
            .unwrap();
        shared.players.sort_by_key(|p| p.number);
        let mut shared_match = match_writer
            .create(
                &shared,
                "Rivali Pisa U19".into(),
                date("2025-11-15T15:00:00+01:00"),
                true,
                6,
                String::new(),
                // only the players of both teams: the match of either
                roster(&shared, &[14]),
            )
            .await
            .unwrap();
        shared_match.team = shared.clone();
        scout.clock = "2025-11-15T14:00:00Z".parse().unwrap();
        scout.play(&shared_match, 1, TeamSideEnum::Us, [1, 2, 3, 4, 5, 6], None, false).await;

        // the finished matches in the upload queue, as the apps do with the
        // statistics allowed
        let finished_matches = QueuedFinishedMatches {
            queue: Arc::new(QueueManager::new(
                Arc::new(StoreQueueReader::new(store.clone())),
                Arc::new(StoreQueueWriter::new(store.clone())),
            )),
            enabled: Arc::new(AtomicBool::new(true)),
        };
        finished_matches.finished(team.id, finished.id.clone()).await;

        // a match imported from DataVolley (v0.12.0), into a new team with
        // the players of the file
        let language = Language::new(LanguageEnum::En);
        let set_reader = Arc::new(StoreSetReader::new(store.clone(), language.clone()));
        let match_reader = StoreMatchReader::new(store.clone(), set_reader, language.clone());
        // the first public file (fixtures/dvw, MIT) this version imports
        let candidates: [(&str, &[u8]); 5] = [
            ("example_data.dvw", include_bytes!("fixtures/dvw/example_data.dvw")),
            ("PM06.dvw", include_bytes!("fixtures/dvw/PM06.dvw")),
            ("DCup-7.dvw", include_bytes!("fixtures/dvw/DCup-7.dvw")),
            ("example-click-scout.dvw", include_bytes!("fixtures/dvw/example-click-scout.dvw")),
            // synthetic, of the tests of the apps
            ("three_sets.dvw", include_bytes!("fixtures/three_sets.dvw")),
        ];
        let day = NaiveDate::from_ymd_opt(2025, 10, 25).unwrap();
        let offset = FixedOffset::east_opt(2 * 3600).unwrap();
        let (plan, mplan) = candidates
            .iter()
            .flat_map(|(name, bytes)| [(name, bytes, Side::Home), (name, bytes, Side::Visiting)])
            .find_map(|(name, bytes, side)| {
                let file = dvw::parse(bytes).ok()?;
                let plan = team_plan(
                    &file,
                    side,
                    TeamChoice::New {
                        classification: Some(TeamClassificationEnum::TopInternational),
                        gender: Some(GenderEnum::Women),
                        country: CountryCode::known("FR"),
                    },
                )
                .map_err(|e| println!("{name} {side:?}: team: {e:?}"))
                .ok()?;
                let mplan = match_plan(&file, side, &plan, match_date(&file, day, offset), 6)
                    .map_err(|e| println!("{name} {side:?}: match: {e:?}"))
                    .ok()?;
                println!("{name} {side:?}: imported");
                Some((plan, mplan))
            })
            .expect("a file to import");
        let imported = save_import(
            &plan,
            &mplan,
            false,
            &team_writer,
            &match_reader,
            &match_writer,
            &set_writer,
            &finished_matches,
        )
        .await
        .expect("import");
        println!("imported {} into {}", imported.id, plan.team.name);

        // the matches listed, as the apps do: the index of the summaries
        // (v0.16.0, `.index.json`)
        for t in [&team, &plan.team, &shared] {
            match_reader.read_summaries(t).await.unwrap();
        }

        let zip = export_team(&*store, &team.id).await.unwrap();
        std::fs::write(export.join(format!("{}.zip", team.id)), zip).unwrap();
        for m in [&finished, &in_progress, &empty, &video] {
            let zip = export_match(&*store, &team.id, &m.id).await.unwrap();
            std::fs::write(export.join(format!("{}.zip", m.id)), zip).unwrap();
        }
        let zip = export_team(&*store, &shared.id).await.unwrap();
        std::fs::write(export.join(format!("{}.zip", shared.id)), zip).unwrap();
        let zip = export_match(&*store, &shared.id, &shared_match.id).await.unwrap();
        std::fs::write(export.join(format!("{}.zip", shared_match.id)), zip).unwrap();
        let zip = export_team(&*store, &plan.team.id).await.unwrap();
        std::fs::write(export.join(format!("{}.zip", plan.team.id)), zip).unwrap();
        let zip = export_match(&*store, &plan.team.id, &imported.id).await.unwrap();
        std::fs::write(export.join(format!("{}.zip", imported.id)), zip).unwrap();
    }
}
