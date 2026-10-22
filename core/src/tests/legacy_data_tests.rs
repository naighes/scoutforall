//! Data saved by older versions: after an update, teams and matches load as
//! they were, and sets replay as they were scouted (the settings are in the
//! settings tests of the terminal interface).

#[cfg(test)]
mod tests {
    use crate::{
        constants::LEGACY_MAX_SUBSTITUTIONS,
        localization::Language,
        providers::{
            fs::{
                match_reader::FileSystemMatchReader, match_writer::FileSystemMatchWriter,
                set_reader::FileSystemSetReader, set_writer::FileSystemSetWriter,
                team_reader::FileSystemTeamReader,
            },
            match_reader::MatchReader,
            match_writer::MatchWriter,
            set_writer::SetWriter,
            team_reader::TeamReader,
        },
        shapes::{
            enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, RoleEnum, TeamSideEnum},
            r#match::MatchEntry,
            snapshot::EventEntry,
            team::TeamEntry,
            technique::{ServeTypeEnum, SetTypeEnum, TechniqueEnum},
        },
    };
    use chrono::{DateTime, Utc};
    use csv::ReaderBuilder;
    use serde::Deserialize;
    use serde_json::json;
    use std::path::{Path, PathBuf};
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

    async fn load(base: &Path) -> (TeamEntry, MatchEntry) {
        let team = FileSystemTeamReader::new(base, Language::default())
            .read_single(&Uuid::parse_str(TEAM_ID).unwrap())
            .await
            .expect("expected the team");
        let reader = FileSystemMatchReader::new(
            base,
            std::sync::Arc::new(FileSystemSetReader::new(base, Language::default())),
            Language::default(),
        );
        let matches = reader.read_all(&team).await.expect("expected the matches");
        assert_eq!(matches.len(), 1);
        let m = matches.into_iter().next().unwrap();
        (team, m.expect("expected a readable match"))
    }

    #[tokio::test]
    async fn team_saved_by_v0_0_1_loads() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        let teams = FileSystemTeamReader::new(dir.path(), Language::default())
            .read_all()
            .await
            .expect("expected the teams");
        assert_eq!(teams.len(), 1);
        let team = teams[0].as_ref().expect("expected a readable team");
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
        let (_, m) = load(dir.path()).await;
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
        assert_eq!(m.sets[0].lineup_rules, Some(LineupRulesEnum::V1));
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
        assert_eq!(m.sets[0].lineup_rules, Some(LineupRulesEnum::V2));
        let snapshot = m
            .get_set_snapshot(1)
            .expect("expected the set to replay")
            .unwrap();
        assert_eq!(snapshot.current_lineup.get_setter(), Some(p.sub));
    }

    /// Edits the opponent and home of the only match, then loads it again.
    async fn edit(base: &Path) -> MatchEntry {
        let (_, m) = load(base).await;
        FileSystemMatchWriter::new(base, Language::default())
            .update(&m, "Arezzo".into(), m.date, true, m.max_substitutions)
            .await
            .expect("expected the match to be updated");
        load(base).await.1
    }

    /// With the rules of v0.3.0 the substitute serves instead.
    #[tokio::test]
    async fn the_same_events_dont_replay_with_the_rules_of_v0_3_0() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), None);
        legacy_set(dir.path(), &p, &events_scouted_up_to_v0_2_0(&p));
        let (_, mut m) = load(dir.path()).await;
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
        let (_, m) = load(dir.path()).await;
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
        let (_, m) = load(dir.path()).await;
        let snapshot = m
            .get_set_snapshot(1)
            .expect("expected the set to replay")
            .unwrap();
        assert_eq!(snapshot.current_lineup.get_serving_player(), Some(p.sub));
        assert_eq!(snapshot.current_lineup.get_setter(), Some(p.sub));
    }

    /// New sets save the rules they're scouted with.
    #[tokio::test]
    async fn new_sets_save_the_lineup_rules() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), None);
        let (_, m) = load(dir.path()).await;
        FileSystemSetWriter::new(dir.path(), Language::default())
            .create(
                &m,
                1,
                TeamSideEnum::Them,
                [p.setter, p.oh1, p.mb2, p.opposite, p.oh2, p.mb1],
                p.libero,
                None,
                p.setter,
                vec![],
            )
            .await
            .expect("expected a new set");
        let saved: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(TEAM_ID).join(MATCH_ID).join("set_1.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(saved["lineup_rules"], "v2");
        let (_, m) = load(dir.path()).await;
        assert_eq!(m.sets[0].lineup_rules, Some(LineupRulesEnum::V2));
    }

    // --- the technique column, added after v0.4.3 ---------------------------

    /// An event as v0.4.3 reads it: no technique, and every row of the file
    /// with the same columns.
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct EventUpToV0_4_3 {
        timestamp: DateTime<Utc>,
        event_type: EventTypeEnum,
        player: Option<Uuid>,
        eval: Option<EvalEnum>,
        target_player: Option<Uuid>,
    }

    fn read_as_v0_4_3(path: &Path) -> Result<Vec<EventUpToV0_4_3>, csv::Error> {
        ReaderBuilder::new()
            .has_headers(false)
            .from_path(path)?
            .deserialize()
            .collect()
    }

    fn events_path(base: &Path) -> PathBuf {
        base.join(TEAM_ID).join(MATCH_ID).join("set_1.csv")
    }

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

    fn columns(path: &Path) -> Vec<usize> {
        std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .map(|l| l.split(',').count())
            .collect()
    }

    #[tokio::test]
    async fn set_scouted_up_to_v0_4_3_loads_without_techniques() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(6));
        legacy_set(dir.path(), &p, &events_scouted_up_to_v0_4_3(&p));
        let (_, m) = load(dir.path()).await;
        assert_eq!(m.sets[0].events.len(), 2);
        assert!(m.sets[0].events.iter().all(|e| e.technique.is_none()));
        let snapshot = m.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!((snapshot.score_us, snapshot.score_them), (1, 0));
    }

    /// v0.4.3 reads a file only when all its rows have the same columns: the
    /// first event saved with the technique gives it to the older rows too.
    #[tokio::test]
    async fn events_added_to_a_set_of_v0_4_3_rewrite_its_rows() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(6));
        legacy_set(dir.path(), &p, &events_scouted_up_to_v0_4_3(&p));
        let (_, m) = load(dir.path()).await;
        let before = m.sets[0].events.clone();
        assert_eq!(columns(&events_path(dir.path())), vec![5, 5]);
        FileSystemSetWriter::new(dir.path(), Language::default())
            .append_event(&m, 1, &float_serve(&p))
            .await
            .expect("expected the event to be saved");
        assert_eq!(columns(&events_path(dir.path())), vec![6, 6, 6]);
        // the older rows are the same
        let (_, m) = load(dir.path()).await;
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
        // ...and v0.4.3 still reads them all
        let old_reader = read_as_v0_4_3(&events_path(dir.path())).expect("v0.4.3 can't read");
        assert_eq!(old_reader.len(), 3);
        // the next events are appended
        FileSystemSetWriter::new(dir.path(), Language::default())
            .append_event(
                &m,
                1,
                &event(
                    EventTypeEnum::A,
                    p.opposite,
                    EvalEnum::Perfect,
                    Some(TechniqueEnum::Set(SetTypeEnum::Super)),
                ),
            )
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(columns(&events_path(dir.path())), vec![6, 6, 6, 6]);
        assert_eq!(read_as_v0_4_3(&events_path(dir.path())).unwrap().len(), 4);
    }

    /// A set scouted now opens with v0.4.3: it skips the technique.
    #[tokio::test]
    async fn set_scouted_now_is_read_by_v0_4_3() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), None);
        let (_, m) = load(dir.path()).await;
        let writer = FileSystemSetWriter::new(dir.path(), Language::default());
        let reception = event(EventTypeEnum::P, p.libero, EvalEnum::Positive, None);
        writer
            .create(
                &m,
                1,
                TeamSideEnum::Them,
                [p.setter, p.oh1, p.mb2, p.opposite, p.oh2, p.mb1],
                p.libero,
                None,
                p.setter,
                vec![reception],
            )
            .await
            .expect("expected a new set");
        writer
            .append_event(
                &m,
                1,
                &event(
                    EventTypeEnum::A,
                    p.oh1,
                    EvalEnum::Perfect,
                    Some(TechniqueEnum::Set(SetTypeEnum::Five)),
                ),
            )
            .await
            .unwrap();
        writer.append_event(&m, 1, &float_serve(&p)).await.unwrap();
        let old_reader = read_as_v0_4_3(&events_path(dir.path())).expect("v0.4.3 can't read");
        let kinds: Vec<_> = old_reader.iter().map(|e| e.event_type).collect();
        assert_eq!(
            kinds,
            vec![EventTypeEnum::P, EventTypeEnum::A, EventTypeEnum::S]
        );
        let (_, m) = load(dir.path()).await;
        let techniques: Vec<_> = m.sets[0].events.iter().map(|e| e.technique).collect();
        assert_eq!(
            techniques,
            vec![
                None,
                Some(TechniqueEnum::Set(SetTypeEnum::Five)),
                Some(TechniqueEnum::Serve(ServeTypeEnum::Float)),
            ]
        );
    }

    /// v0.4.3 appends its rows without the technique to a set scouted now:
    /// this version still opens it (v0.4.3 itself doesn't).
    #[tokio::test]
    async fn rows_added_by_v0_4_3_to_a_set_scouted_now_load() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(6));
        // a reception and an attack scouted now, then the older version
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
        assert_eq!(columns(&events_path(dir.path())), vec![6, 6, 5]);
        assert!(read_as_v0_4_3(&events_path(dir.path())).is_err());
        let (_, m) = load(dir.path()).await;
        let techniques: Vec<_> = m.sets[0].events.iter().map(|e| e.technique).collect();
        assert_eq!(
            techniques,
            vec![None, Some(TechniqueEnum::Set(SetTypeEnum::Five)), None]
        );
        // and saving again makes it readable by v0.4.3 too
        FileSystemSetWriter::new(dir.path(), Language::default())
            .append_event(
                &m,
                1,
                &event(EventTypeEnum::D, p.libero, EvalEnum::Positive, None),
            )
            .await
            .unwrap();
        assert_eq!(columns(&events_path(dir.path())), vec![6, 6, 6, 6]);
        assert_eq!(read_as_v0_4_3(&events_path(dir.path())).unwrap().len(), 4);
    }

    // --- the freeball events, added after v0.5.0 ------------------------------

    /// The event types up to v0.5.0: no freeballs.
    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "UPPERCASE")]
    #[allow(dead_code, clippy::upper_case_acronyms)]
    enum EventTypeUpToV0_5_0 {
        S,
        P,
        A,
        D,
        B,
        F,
        OS,
        OE,
        R,
        CL,
        CS,
    }

    /// An event as v0.5.0 reads it: the technique last, the rows with
    /// fewer or more columns accepted.
    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct EventUpToV0_5_0 {
        timestamp: DateTime<Utc>,
        event_type: EventTypeUpToV0_5_0,
        player: Option<Uuid>,
        eval: Option<EvalEnum>,
        target_player: Option<Uuid>,
    }

    fn read_as_v0_5_0(path: &Path) -> Result<Vec<EventUpToV0_5_0>, csv::Error> {
        ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .from_path(path)?
            .deserialize()
            .collect()
    }

    fn freeball(event_type: EventTypeEnum, player: Option<Uuid>) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type,
            player,
            eval: None,
            target_player: None,
            technique: None,
        }
    }

    /// The freeballs are saved with their code in the event column, ours
    /// with the player sending it, and load back as they were.
    #[tokio::test]
    async fn freeballs_are_saved_and_load() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), None);
        let (_, m) = load(dir.path()).await;
        let writer = FileSystemSetWriter::new(dir.path(), Language::default());
        writer
            .create(
                &m,
                1,
                TeamSideEnum::Them,
                [p.setter, p.oh1, p.mb2, p.opposite, p.oh2, p.mb1],
                p.libero,
                None,
                p.setter,
                vec![event(EventTypeEnum::P, p.libero, EvalEnum::Positive, None)],
            )
            .await
            .expect("expected a new set");
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
        for e in &events {
            writer.append_event(&m, 1, e).await.unwrap();
        }
        let rows = std::fs::read_to_string(events_path(dir.path())).unwrap();
        let rows: Vec<_> = rows.lines().collect();
        assert!(
            rows[1].ends_with(&format!(",FB,{},,,", p.setter)),
            "{}",
            rows[1]
        );
        assert!(rows[2].ends_with(",OF,,,,"), "{}", rows[2]);
        assert_eq!(columns(&events_path(dir.path())), vec![6, 6, 6, 6]);
        let (_, m) = load(dir.path()).await;
        let loaded: Vec<_> = m.sets[0].events.iter().map(known_fields).collect();
        assert_eq!(
            loaded,
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

    /// The versions up to v0.5.0 don't know the freeballs: a set with one
    /// is unreadable for them (agreed, they update themselves). A set
    /// without one they still read.
    #[tokio::test]
    async fn set_with_a_freeball_is_unreadable_up_to_v0_5_0() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(6));
        legacy_set(dir.path(), &p, &events_scouted_up_to_v0_4_3(&p));
        let (_, m) = load(dir.path()).await;
        let writer = FileSystemSetWriter::new(dir.path(), Language::default());
        writer.append_event(&m, 1, &float_serve(&p)).await.unwrap();
        assert_eq!(read_as_v0_5_0(&events_path(dir.path())).unwrap().len(), 3);
        assert_eq!(read_as_v0_4_3(&events_path(dir.path())).unwrap().len(), 3);
        writer
            .append_event(&m, 1, &freeball(EventTypeEnum::OF, None))
            .await
            .unwrap();
        // (v0.4.3 knows the same event types as v0.5.0)
        assert!(read_as_v0_5_0(&events_path(dir.path())).is_err());
        // this version reads it, and the undo makes it readable again
        let (_, m) = load(dir.path()).await;
        assert_eq!(m.sets[0].events.len(), 4);
        writer.remove_last_event(&m, 1).await.unwrap();
        assert_eq!(read_as_v0_5_0(&events_path(dir.path())).unwrap().len(), 3);
    }

    /// The undo rewrites the file: the techniques stay, and the rows of
    /// v0.4.3 get the column.
    #[tokio::test]
    async fn undo_keeps_the_techniques() {
        let dir = TempDir::new().unwrap();
        let p = Players::new();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(6));
        legacy_set(dir.path(), &p, &events_scouted_up_to_v0_4_3(&p));
        let (_, m) = load(dir.path()).await;
        let writer = FileSystemSetWriter::new(dir.path(), Language::default());
        writer.append_event(&m, 1, &float_serve(&p)).await.unwrap();
        let spin = EventEntry {
            technique: Some(TechniqueEnum::Serve(ServeTypeEnum::Spin)),
            eval: Some(EvalEnum::Perfect),
            ..float_serve(&p)
        };
        // the float serve is "+": the rally goes on, so a dig and not a
        // second serve would come; the file doesn't care
        writer.append_event(&m, 1, &spin).await.unwrap();
        let removed = writer.remove_last_event(&m, 1).await.unwrap().unwrap();
        assert_eq!(
            removed.technique,
            Some(TechniqueEnum::Serve(ServeTypeEnum::Spin))
        );
        let (_, m) = load(dir.path()).await;
        assert_eq!(
            m.sets[0].events.last().unwrap().technique,
            Some(TechniqueEnum::Serve(ServeTypeEnum::Float))
        );
        assert_eq!(columns(&events_path(dir.path())), vec![6, 6, 6]);
        // the undo of a set of v0.4.3 gives the column to its rows
        let dir = TempDir::new().unwrap();
        legacy_team(dir.path(), &p);
        match_descriptor(dir.path(), Some(6));
        legacy_set(dir.path(), &p, &events_scouted_up_to_v0_4_3(&p));
        let (_, m) = load(dir.path()).await;
        let removed = FileSystemSetWriter::new(dir.path(), Language::default())
            .remove_last_event(&m, 1)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(removed.event_type, EventTypeEnum::A);
        assert_eq!(columns(&events_path(dir.path())), vec![6]);
        assert_eq!(read_as_v0_4_3(&events_path(dir.path())).unwrap().len(), 1);
    }
}
