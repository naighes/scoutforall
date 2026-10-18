//! Data saved by older versions: after an update, teams, matches and
//! settings load as they were, and sets replay as they were scouted.

#[cfg(test)]
mod tests {
    use crate::{
        constants::LEGACY_MAX_SUBSTITUTIONS,
        providers::{
            fs::{
                match_reader::FileSystemMatchReader, path::get_config_file_path,
                set_reader::FileSystemSetReader, set_writer::FileSystemSetWriter,
                settings_reader::FileSystemSettingsReader, team_reader::FileSystemTeamReader,
            },
            match_reader::MatchReader,
            set_writer::SetWriter,
            settings_reader::SettingsReader,
            team_reader::TeamReader,
        },
        shapes::{
            enums::{EventTypeEnum, LanguageEnum, LineupRulesEnum, RoleEnum, TeamSideEnum},
            r#match::MatchEntry,
            settings::Settings,
            team::TeamEntry,
        },
    };
    use serde_json::json;
    use std::path::Path;
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
        let team = FileSystemTeamReader::new(base)
            .read_single(&Uuid::parse_str(TEAM_ID).unwrap())
            .await
            .expect("expected the team");
        let reader =
            FileSystemMatchReader::new(base, std::sync::Arc::new(FileSystemSetReader::new(base)));
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
        let teams = FileSystemTeamReader::new(dir.path())
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

    /// Up to v0.0.7 the settings were the language only.
    #[tokio::test]
    async fn settings_saved_by_v0_0_x_load() {
        let dir = TempDir::new().unwrap();
        std::fs::write(get_config_file_path(dir.path()), r#"{ "language": "it" }"#).unwrap();
        let settings = FileSystemSettingsReader::new(dir.path())
            .read()
            .await
            .expect("expected the settings");
        let defaults = Settings::default();
        assert_eq!(settings.language, LanguageEnum::It);
        assert!(settings.analytics_enabled);
        assert_eq!(
            settings.scouting_keybindings.key_for(&EventTypeEnum::S),
            defaults.scouting_keybindings.key_for(&EventTypeEnum::S)
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
        FileSystemSetWriter::new(dir.path())
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
}
