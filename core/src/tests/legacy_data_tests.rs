//! Data saved by older versions: after an update, the data folder migrates
//! into the database, and teams and matches load as they were, sets replay
//! as they were scouted, also when imported from the archives of the older
//! versions; edited and scouted on in the database, they still do (the
//! settings are in the settings tests of the terminal interface).

#[cfg(test)]
mod tests {
    use crate::{
        constants::LEGACY_MAX_SUBSTITUTIONS,
        legacy::migrate::{Backup, migrate},
        localization::Language,
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            set_writer::SetWriter,
            sqlite::{
                archive::{self as archives, Subject},
                legacy_archive::{import_legacy_match, import_legacy_team},
            },
            team_reader::TeamReader,
        },
        reporting::model::build_report,
        shapes::{
            enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, RoleEnum, TeamSideEnum},
            flags::{EventFlags, ReceptionSchemeEnum},
            r#match::MatchEntry,
            snapshot::EventEntry,
            team::TeamEntry,
            technique::{ServeTypeEnum, SetTypeEnum, TechniqueEnum},
        },
        storage::fs::FsStore,
        tests::{EN, Sqlite},
    };
    use chrono::Utc;
    use serde_json::json;
    use std::{io::Write, path::Path};
    use tempfile::TempDir;
    use uuid::Uuid;

    const TEAM_ID: &str = "45a589d5-004a-4a50-99f5-6d78fd052a3c";
    const MATCH_ID: &str = "2025-09-20_club_arezzo";

    struct Players {
        setter: Uuid,
        oh1: Uuid,
        mb2: Uuid,
        opposite: Uuid,
        oh2: Uuid,
        mb1: Uuid,
        libero: Uuid,
        sub: Uuid,
    }

    impl Players {
        fn new() -> Self {
            Players {
                setter: Uuid::new_v4(),
                oh1: Uuid::new_v4(),
                mb2: Uuid::new_v4(),
                opposite: Uuid::new_v4(),
                oh2: Uuid::new_v4(),
                mb1: Uuid::new_v4(),
                libero: Uuid::new_v4(),
                sub: Uuid::new_v4(),
            }
        }

        fn roles(&self) -> [(Uuid, &'static str); 8] {
            [
                (self.setter, "Setter"),
                (self.oh1, "OutsideHitter"),
                (self.mb2, "MiddleBlocker"),
                (self.opposite, "OppositeHitter"),
                (self.oh2, "OutsideHitter"),
                (self.mb1, "MiddleBlocker"),
                (self.libero, "Libero"),
                (self.sub, "MiddleBlocker"),
            ]
        }
    }

    fn write(path: &Path, content: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    /// A team as saved by v0.0.1: a league, no classification nor gender,
    /// players never deleted.
    fn legacy_team(base: &Path, p: &Players) {
        let players: Vec<_> = p
            .roles()
            .iter()
            .enumerate()
            .map(|(i, (id, role))| {
                json!({ "id": id, "name": format!("player {}", i + 1), "role": role, "number": i + 1 })
            })
            .collect();
        let team =
            json!({ "name": "Volley Club", "league": "Serie C", "year": 2025, "players": players });
        write(
            &base.join(TEAM_ID).join("team.json"),
            &serde_json::to_string_pretty(&team).unwrap(),
        );
    }

    /// The first set, as saved up to v0.3.0: no lineup rules, and no
    /// fallback libero up to v0.0.2. They serve, the setter in position 1:
    /// the libero is on court for middle blocker 1, and middle blocker 2
    /// would replace it.
    fn legacy_set(base: &Path, p: &Players, events: &[String]) {
        let set = json!({
            "serving_team": "Them",
            "initial_positions": [p.setter, p.oh1, p.mb2, p.opposite, p.oh2, p.mb1],
            "libero": p.libero,
            "setter": p.setter,
        });
        let folder = base.join(TEAM_ID).join(MATCH_ID);
        write(
            &folder.join("set_1.json"),
            &serde_json::to_string_pretty(&set).unwrap(),
        );
        write(&folder.join("set_1.csv"), &events.join(""));
    }

    fn match_descriptor(base: &Path, max_substitutions: Option<u8>) {
        let mut descriptor =
            json!({ "opponent": "Club Arezzo", "date": "2025-09-20T00:00:00Z", "home": false });
        if let Some(max) = max_substitutions {
            descriptor["max_substitutions"] = max.into();
        }
        write(
            &base.join(TEAM_ID).join(MATCH_ID).join("match.json"),
            &serde_json::to_string_pretty(&descriptor).unwrap(),
        );
    }

    fn row(event_type: &str, player: Option<Uuid>, eval: &str, target: Option<Uuid>) -> String {
        let id = |p: Option<Uuid>| p.map(|p| p.to_string()).unwrap_or_default();
        format!(
            "2025-09-20T20:00:00.000000Z,{},{},{},{}\n",
            event_type,
            id(player),
            eval,
            id(target)
        )
    }

    /// Middle blocker 2 is substituted, then the libero must leave the
    /// court: up to v0.2.0 middle blocker 2 went back on court to serve, and
    /// the scout went on with it.
    fn events_scouted_up_to_v0_2_0(p: &Players) -> Vec<String> {
        vec![
            row("R", Some(p.mb2), "", Some(p.sub)),
            row("OE", None, "", None),
            row("F", Some(p.setter), "", None),
            row("OE", None, "", None),
            // middle blocker 2 serves an ace
            row("S", Some(p.mb2), "#", None),
            // ...and is chosen as setter
            row("CS", Some(p.mb2), "", None),
        ]
    }

    /// The data folder in `base` migrated into a database, as at the start
    /// of the app: its team and its only match, read from there.
    struct Loaded {
        p: Sqlite,
        team: TeamEntry,
        m: MatchEntry,
    }

    impl Loaded {
        /// The match read again from the database.
        async fn reload(&self) -> MatchEntry {
            let team = self.p.team_reader.read_single(&self.team.id).await.unwrap();
            self.p
                .match_reader
                .read_single(&team, &self.m.id)
                .await
                .expect("expected a readable match")
        }
    }

    async fn load(base: &Path) -> Loaded {
        // the archives of the folder before it moves
        let archives = (
            zip_folder(&base.join(TEAM_ID)),
            zip_folder(&base.join(TEAM_ID).join(MATCH_ID)),
        );
        let p = Sqlite::new();
        let backups = TempDir::new().unwrap();
        let backup = Backup {
            dir: backups.path().to_path_buf(),
            prefix: "backup".into(),
        };
        let migrated = migrate(&FsStore::new(base), &p.db, &backup, EN)
            .await
            .expect("expected the folder to migrate");
        assert_eq!(migrated.left, vec![]);
        let team = p
            .team_reader
            .read_single(&Uuid::parse_str(TEAM_ID).unwrap())
            .await
            .expect("expected the team");
        let matches = p
            .match_reader
            .read_all(&team)
            .await
            .expect("expected the matches");
        assert_eq!(matches.len(), 1);
        let m = matches.into_iter().next().unwrap();
        let m = m.expect("expected a readable match");
        imports_as_loaded(&archives, &p, &m).await;
        Loaded { p, team, m }
    }

    /// The files of the folder zipped, as exported up to v0.9.0.
    fn zip_folder(folder: &Path) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                zip.start_file::<_, ()>(name.as_str(), zip::write::FileOptions::default())
                    .unwrap();
                zip.write_all(&std::fs::read(&path).unwrap()).unwrap();
            }
        }
        zip.finish().unwrap().into_inner()
    }

    /// What tells two matches apart once replayed: the rules, the events,
    /// the score and the lineup of each set, and the report.
    fn replayed(m: &MatchEntry) -> String {
        let sets: Vec<String> = m
            .sets
            .iter()
            .map(|s| {
                let snapshot = m.get_set_snapshot(s.set_number).ok().flatten();
                format!(
                    "{:?} {:?} {:?} {:?}",
                    s.find_lineup_rules(m.max_substitutions).ok(),
                    s.events,
                    snapshot.as_ref().map(|s| (s.score_us, s.score_them)),
                    snapshot.map(|s| format!("{:?}", s.current_lineup)),
                )
            })
            .collect();
        let report = build_report(m, EN).map(|r| serde_json::to_string(&r).ok());
        format!("{sets:?} {report:?}")
    }

    /// The team and the match, zipped as exported up to v0.9.0, and the
    /// match exported from the database, are imported into an empty
    /// database: the match replays as migrated from its folder.
    async fn imports_as_loaded(archives: &(Vec<u8>, Vec<u8>), from: &Sqlite, m: &MatchEntry) {
        let language = Language::default();
        let (team_archive, match_archive) = archives;
        let to = Sqlite::new();
        import_legacy_team(
            &to.db,
            team_archive,
            Uuid::parse_str(TEAM_ID).ok(),
            &language,
        )
        .expect("expected the team to import");
        let imported = import_legacy_match(&to.db, match_archive, m.team.id, &language)
            .expect("expected the match to import");
        let team = to.team_reader.read_single(&m.team.id).await.unwrap();
        let again = to
            .match_reader
            .read_single(&team, &imported.id.to_string())
            .await
            .expect("expected the imported match to load");
        assert_eq!(replayed(&again), replayed(m));
        let id = Uuid::parse_str(&m.id).unwrap();
        let archive = archives::export(&from.db, Subject::Match(id)).unwrap();
        let to = Sqlite::new();
        archives::import(&to.db, &archive, &language).expect("expected the match to import");
        let team = to.team_reader.read_single(&m.team.id).await.unwrap();
        let again = to
            .match_reader
            .read_single(&team, &m.id)
            .await
            .expect("expected the imported match to load");
        assert_eq!(replayed(&again), replayed(m));
    }

    #[tokio::test]
    async fn team_saved_by_v0_0_1_loads() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), None);
        let team = load(dir.path()).await.team;
        assert_eq!(team.name, "Volley Club");
        assert_eq!(team.classification, None);
        assert_eq!(team.players.len(), 8);
        assert!(team.players.iter().all(|p| !p.deleted));
        assert_eq!(
            team.players
                .iter()
                .find(|pl| pl.id == p.libero)
                .unwrap()
                .role,
            Some(RoleEnum::Libero)
        );
    }

    /// A set scouted up to v0.2.0 replays with the lineup it was scouted on,
    /// not the one v0.3.0 would have: otherwise the setter change fails, and
    /// the match disappears from the list.
    #[tokio::test]
    async fn set_scouted_up_to_v0_2_0_replays_as_scouted() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), None);
        legacy_set(dir.path(), &p, &events_scouted_up_to_v0_2_0(&p));
        let m = load(dir.path()).await.m;
        assert_eq!(m.max_substitutions, LEGACY_MAX_SUBSTITUTIONS);
        assert_eq!(m.sets[0].lineup_rules, None);
        m.get_status().expect("expected the match status");
        let snapshot = m
            .get_set_snapshot(1)
            .expect("expected the set to replay")
            .unwrap();
        assert_eq!(snapshot.score_us, 3);
        assert_eq!(snapshot.score_them, 1);
        assert_eq!(snapshot.current_lineup.get_serving_player(), Some(p.mb2));
        assert_eq!(snapshot.current_lineup.get_setter(), Some(p.mb2));
    }

    /// Editing the match info of a match created before v0.3.0 saves the
    /// substitutions: its sets still replay with the rules they were scouted
    /// with.
    #[tokio::test]
    async fn editing_a_match_created_before_v0_3_0_keeps_its_sets_replaying() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), None);
        legacy_set(dir.path(), &p, &events_scouted_up_to_v0_2_0(&p));
        let m = edit(dir.path()).await;
        assert_eq!(m.opponent, "Arezzo");
        assert!(m.home);
        assert_eq!(m.max_substitutions, LEGACY_MAX_SUBSTITUTIONS);
        assert_eq!(
            m.sets[0].find_lineup_rules(m.max_substitutions).ok(),
            Some(LineupRulesEnum::V1)
        );
        let snapshot = m
            .get_set_snapshot(1)
            .expect("expected the set to replay")
            .unwrap();
        assert_eq!(snapshot.current_lineup.get_setter(), Some(p.mb2));
    }

    /// The set of a match created before v0.3.0 that only the rules of
    /// v0.3.0 replay keeps them once the match is edited.
    #[tokio::test]
    async fn editing_a_match_created_before_v0_3_0_keeps_the_rules_of_v0_3_0() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), None);
        let mut events = events_scouted_up_to_v0_2_0(&p)[..4].to_vec();
        events.push(row("S", Some(p.sub), "#", None));
        events.push(row("CS", Some(p.sub), "", None));
        legacy_set(dir.path(), &p, &events);
        let m = edit(dir.path()).await;
        assert_eq!(
            m.sets[0].find_lineup_rules(m.max_substitutions).ok(),
            Some(LineupRulesEnum::V2)
        );
        let snapshot = m
            .get_set_snapshot(1)
            .expect("expected the set to replay")
            .unwrap();
        assert_eq!(snapshot.current_lineup.get_setter(), Some(p.sub));
    }

    /// Edits the opponent and home of the only match, then loads it again.
    async fn edit(base: &Path) -> MatchEntry {
        let loaded = load(base).await;
        let m = &loaded.m;
        loaded
            .p
            .match_writer
            .update(
                m,
                "Arezzo".into(),
                m.date,
                true,
                m.max_substitutions,
                String::new(),
                vec![],
            )
            .await
            .expect("expected the match to be updated");
        loaded.reload().await
    }

    /// With the rules of v0.3.0 the substitute serves instead.
    #[tokio::test]
    async fn the_same_events_dont_replay_with_the_rules_of_v0_3_0() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), None);
        legacy_set(dir.path(), &p, &events_scouted_up_to_v0_2_0(&p));
        let mut m = load(dir.path()).await.m;
        m.sets[0].lineup_rules = Some(LineupRulesEnum::V2);
        assert!(m.get_set_snapshot(1).is_err());
        // until the serve, the substitute is the one serving
        m.sets[0].events.truncate(4);
        let snapshot = m.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!(snapshot.current_lineup.get_serving_player(), Some(p.sub));
    }

    /// v0.3.0 didn't save the rules either: its matches are the ones saving
    /// the substitutions, and their sets replay with its rules.
    #[tokio::test]
    async fn set_scouted_with_v0_3_0_replays_as_scouted() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(8));
        let events = events_scouted_up_to_v0_2_0(&p)[..4].to_vec();
        legacy_set(dir.path(), &p, &events);
        let m = load(dir.path()).await.m;
        assert_eq!(m.sets[0].lineup_rules, Some(LineupRulesEnum::V2));
        let snapshot = m.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!(snapshot.current_lineup.get_serving_player(), Some(p.sub));
    }

    /// A set scouted with v0.3.0 in a match created before it: the rules of
    /// v0.2.0 can't replay it, those of v0.3.0 can.
    #[tokio::test]
    async fn set_only_v0_3_0_can_replay_falls_back_to_its_rules() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), None);
        let mut events = events_scouted_up_to_v0_2_0(&p)[..4].to_vec();
        // the substitute serves, as v0.3.0 showed
        events.push(row("S", Some(p.sub), "#", None));
        events.push(row("CS", Some(p.sub), "", None));
        legacy_set(dir.path(), &p, &events);
        let m = load(dir.path()).await.m;
        let snapshot = m
            .get_set_snapshot(1)
            .expect("expected the set to replay")
            .unwrap();
        assert_eq!(snapshot.current_lineup.get_serving_player(), Some(p.sub));
        assert_eq!(snapshot.current_lineup.get_setter(), Some(p.sub));
    }

    /// The first set of a migrated match, created now with the starting six
    /// of `legacy_set` and `events`.
    async fn new_set(loaded: &Loaded, p: &Players, events: Vec<EventEntry>) {
        loaded
            .p
            .set_writer
            .create(
                &loaded.m,
                1,
                TeamSideEnum::Them,
                [p.setter, p.oh1, p.mb2, p.opposite, p.oh2, p.mb1],
                p.libero,
                None,
                p.setter,
                events,
            )
            .await
            .expect("expected a new set");
    }

    /// New sets save the rules they're scouted with, also in a match
    /// created before the rules were saved.
    #[tokio::test]
    async fn new_sets_save_the_lineup_rules() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), None);
        let loaded = load(dir.path()).await;
        new_set(&loaded, &p, vec![]).await;
        let m = loaded.reload().await;
        assert_eq!(m.sets[0].lineup_rules, Some(LineupRulesEnum::V2));
    }

    // --- the technique column, added after v0.4.3 ---------------------------

    /// A positive reception and a kill of outside hitter 1, who then serves.
    fn events_scouted_up_to_v0_4_3(p: &Players) -> Vec<String> {
        vec![
            row("P", Some(p.libero), "+", None),
            row("A", Some(p.oh1), "#", None),
        ]
    }

    fn event(
        event_type: EventTypeEnum,
        player: Uuid,
        eval: EvalEnum,
        technique: Option<TechniqueEnum>,
    ) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type,
            player: Some(player),
            eval: Some(eval),
            target_player: None,
            technique,
            flags: Default::default(),
        }
    }

    fn float_serve(p: &Players) -> EventEntry {
        event(
            EventTypeEnum::S,
            p.oh1,
            EvalEnum::Positive,
            Some(TechniqueEnum::Serve(ServeTypeEnum::Float)),
        )
    }

    /// The fields v0.4.3 knows, to compare events across versions.
    fn known_fields(e: &EventEntry) -> (EventTypeEnum, Option<Uuid>, Option<EvalEnum>) {
        (e.event_type, e.player, e.eval)
    }

    #[tokio::test]
    async fn set_scouted_up_to_v0_4_3_loads_without_techniques() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(6));
        legacy_set(dir.path(), &p, &events_scouted_up_to_v0_4_3(&p));
        let m = load(dir.path()).await.m;
        assert_eq!(m.sets[0].events.len(), 2);
        assert!(m.sets[0].events.iter().all(|e| e.technique.is_none()));
        let snapshot = m.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!((snapshot.score_us, snapshot.score_them), (1, 0));
    }

    /// The events scouted on a set of v0.4.3, after the migration: the
    /// older ones stay as they were, the new ones have their technique.
    #[tokio::test]
    async fn events_added_to_a_set_of_v0_4_3_keep_the_older_ones() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(6));
        legacy_set(dir.path(), &p, &events_scouted_up_to_v0_4_3(&p));
        let loaded = load(dir.path()).await;
        let before = loaded.m.sets[0].events.clone();
        loaded
            .p
            .set_writer
            .append_event(&loaded.m, 1, &float_serve(&p))
            .await
            .expect("expected the event to be saved");
        let m = loaded.reload().await;
        let events = &m.sets[0].events;
        assert_eq!(events.len(), 3);
        for (old, new) in before.iter().zip(events) {
            assert_eq!(known_fields(old), known_fields(new));
            assert_eq!(old.timestamp, new.timestamp);
            assert_eq!(new.technique, None);
        }
        assert_eq!(
            events[2].technique,
            Some(TechniqueEnum::Serve(ServeTypeEnum::Float))
        );
    }

    /// v0.4.3 appends its rows without the technique to a set scouted
    /// later: the set still migrates, with the techniques it has.
    #[tokio::test]
    async fn rows_added_by_v0_4_3_to_a_set_scouted_later_load() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(6));
        // a reception and an attack scouted later, then the older version
        // appends a serve
        let with_technique =
            |row: String, technique: &str| format!("{},{}\n", row.trim_end(), technique);
        legacy_set(
            dir.path(),
            &p,
            &[
                with_technique(row("P", Some(p.libero), "+", None), ""),
                with_technique(row("A", Some(p.oh1), "#", None), "5"),
                row("S", Some(p.oh1), "+", None),
            ],
        );
        let m = load(dir.path()).await.m;
        let techniques: Vec<_> = m.sets[0].events.iter().map(|e| e.technique).collect();
        assert_eq!(
            techniques,
            vec![None, Some(TechniqueEnum::Set(SetTypeEnum::Five)), None]
        );
    }

    // --- the freeball events, added after v0.5.0 ------------------------------

    fn freeball(event_type: EventTypeEnum, player: Option<Uuid>) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type,
            player,
            eval: None,
            target_player: None,
            technique: None,
            flags: Default::default(),
        }
    }

    /// The freeballs, ours with the player sending it, are saved and load
    /// back as they were, in a match of before.
    #[tokio::test]
    async fn freeballs_are_saved_and_load() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), None);
        let loaded = load(dir.path()).await;
        new_set(
            &loaded,
            &p,
            vec![event(EventTypeEnum::P, p.libero, EvalEnum::Positive, None)],
        )
        .await;
        let writer = &loaded.p.set_writer;
        let events = [
            freeball(EventTypeEnum::FB, Some(p.setter)),
            freeball(EventTypeEnum::OF, None),
            event(
                EventTypeEnum::A,
                p.oh1,
                EvalEnum::Perfect,
                Some(TechniqueEnum::Set(SetTypeEnum::Super)),
            ),
        ];
        let m = loaded.reload().await;
        for e in &events {
            writer.append_event(&m, 1, e).await.unwrap();
        }
        let m = loaded.reload().await;
        let read: Vec<_> = m.sets[0].events.iter().map(known_fields).collect();
        assert_eq!(
            read,
            vec![
                (EventTypeEnum::P, Some(p.libero), Some(EvalEnum::Positive)),
                (EventTypeEnum::FB, Some(p.setter), None),
                (EventTypeEnum::OF, None, None),
                (EventTypeEnum::A, Some(p.oh1), Some(EvalEnum::Perfect)),
            ]
        );
        let snapshot = m.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!((snapshot.score_us, snapshot.score_them), (1, 0));
        // the undo takes them back one by one
        let removed = writer.remove_last_event(&m, 1).await.unwrap().unwrap();
        assert_eq!(removed.event_type, EventTypeEnum::A);
        let removed = writer.remove_last_event(&m, 1).await.unwrap().unwrap();
        assert_eq!(known_fields(&removed), (EventTypeEnum::OF, None, None));
        let removed = writer.remove_last_event(&m, 1).await.unwrap().unwrap();
        assert_eq!(
            known_fields(&removed),
            (EventTypeEnum::FB, Some(p.setter), None)
        );
    }

    /// The undo keeps the techniques of the events before; on a migrated
    /// set of v0.4.3 it takes back its rows too.
    #[tokio::test]
    async fn undo_keeps_the_techniques() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(6));
        legacy_set(dir.path(), &p, &events_scouted_up_to_v0_4_3(&p));
        let loaded = load(dir.path()).await;
        let writer = &loaded.p.set_writer;
        writer
            .append_event(&loaded.m, 1, &float_serve(&p))
            .await
            .unwrap();
        let spin = EventEntry {
            technique: Some(TechniqueEnum::Serve(ServeTypeEnum::Spin)),
            eval: Some(EvalEnum::Perfect),
            ..float_serve(&p)
        };
        // the float serve is "+": the rally goes on, so a dig and not a
        // second serve would come; the set writer doesn't care
        writer.append_event(&loaded.m, 1, &spin).await.unwrap();
        let removed = writer
            .remove_last_event(&loaded.m, 1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            removed.technique,
            Some(TechniqueEnum::Serve(ServeTypeEnum::Spin))
        );
        let m = loaded.reload().await;
        assert_eq!(
            m.sets[0].events.last().unwrap().technique,
            Some(TechniqueEnum::Serve(ServeTypeEnum::Float))
        );
        // the undo on a set of v0.4.3 just migrated
        let dir = TempDir::new().unwrap();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(6));
        legacy_set(dir.path(), &p, &events_scouted_up_to_v0_4_3(&p));
        let loaded = load(dir.path()).await;
        let removed = loaded
            .p
            .set_writer
            .remove_last_event(&loaded.m, 1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(removed.event_type, EventTypeEnum::A);
        let m = loaded.reload().await;
        assert_eq!(m.sets[0].events.len(), 1);
    }

    // --- the flags column, added after v0.10.1 -------------------------------

    /// A set scouted up to v0.10.1 has no flags: it loads with none, and the
    /// events scouted on it after the migration have theirs.
    #[tokio::test]
    async fn set_scouted_up_to_v0_10_1_gets_the_flags() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(6));
        let with_technique =
            |row: String, technique: &str| format!("{},{}\n", row.trim_end(), technique);
        legacy_set(
            dir.path(),
            &p,
            &[
                with_technique(row("P", Some(p.libero), "+", None), ""),
                with_technique(row("A", Some(p.oh1), "#", None), "5"),
            ],
        );
        let loaded = load(dir.path()).await;
        assert!(
            loaded.m.sets[0]
                .events
                .iter()
                .all(|e| e.flags == EventFlags::default())
        );
        let flags = EventFlags::default()
            .with_inverted_p1(true)
            .with_reception(ReceptionSchemeEnum::Two);
        loaded
            .p
            .set_writer
            .append_event(
                &loaded.m,
                1,
                &EventEntry {
                    flags,
                    ..float_serve(&p)
                },
            )
            .await
            .unwrap();
        let m = loaded.reload().await;
        let events = &m.sets[0].events;
        assert_eq!(
            events.iter().map(|e| e.flags).collect::<Vec<_>>(),
            [EventFlags::default(), EventFlags::default(), flags]
        );
        assert_eq!(
            events[1].technique,
            Some(TechniqueEnum::Set(SetTypeEnum::Five))
        );
    }

    /// The undo keeps the flags of the events before, also the bits of a
    /// later version.
    #[tokio::test]
    async fn undo_keeps_the_flags() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(6));
        let later = EventFlags::parse("70").unwrap();
        legacy_set(
            dir.path(),
            &p,
            &[
                format!("{},,70\n", row("P", Some(p.libero), "+", None).trim_end()),
                format!("{},5,1\n", row("A", Some(p.oh1), "#", None).trim_end()),
            ],
        );
        let loaded = load(dir.path()).await;
        loaded
            .p
            .set_writer
            .remove_last_event(&loaded.m, 1)
            .await
            .unwrap();
        let m = loaded.reload().await;
        assert_eq!(
            m.sets[0].events.iter().map(|e| e.flags).collect::<Vec<_>>(),
            [later]
        );
    }

    /// The battery ran out while an event was appended: the row cut is left
    /// out, the set migrates without it, and the next event takes its place.
    #[tokio::test]
    async fn an_event_cut_while_saved_is_left_out() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(6));
        let their_error = "2025-09-20T20:00:00.000000Z,OE,,,,\n".to_string();
        legacy_set(
            dir.path(),
            &p,
            &[
                their_error.clone(),
                their_error,
                // all its columns, only the line end missing
                "2025-09-20T20:00:01.000000Z,OE,,,,".into(),
            ],
        );
        let loaded = load(dir.path()).await;
        assert_eq!(loaded.m.sets[0].events.len(), 2);
        let next = loaded.m.sets[0].events[0].clone();
        loaded
            .p
            .set_writer
            .append_event(&loaded.m, 1, &next)
            .await
            .unwrap();
        let m = loaded.reload().await;
        assert_eq!(m.sets[0].events.len(), 3);
    }
}
