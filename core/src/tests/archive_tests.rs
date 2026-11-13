//! The archives exported by the versions before the database (up to
//! v0.9.0 in the fixtures, with the data saved by the older versions),
//! imported into it (`legacy_archive`): the team takes its id, the match
//! replays as scouted, what isn't of the team or doesn't replay is refused.
//! The archives of every legacy version are in `legacy_archive_tests`.

#[cfg(test)]
mod tests {
    use crate::{
        errors::{AppError, IOError},
        legacy::archive::{Archive, file_team_id, read_archive},
        localization::Language,
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            sqlite::{
                archive::{self as archives, Subject},
                legacy_archive::{Opened, import_legacy_match, import_legacy_team, open},
            },
            team_reader::TeamReader,
            team_writer::{PlayerInput, TeamInput, TeamWriter},
        },
        shapes::{
            enums::LineupRulesEnum,
            r#match::{MatchEntry, MatchSource},
            team::TeamEntry,
        },
        tests::{EN, Sqlite},
    };
    use std::io::Write;
    use uuid::Uuid;

    const TEAM_ID: &str = "45a589d5-004a-4a50-99f5-6d78fd052a3c";
    const MATCH_ID: &str = "2025-09-20_club_arezzo";
    /// Volley Club, saved by v0.0.1.
    const TEAM_UP_TO_V0_9_0: &[u8] =
        include_bytes!("fixtures/45a589d5-004a-4a50-99f5-6d78fd052a3c.zip");
    /// The first set of the match against Club Arezzo, scouted up to v0.2.0:
    /// it replays only with the rules it was scouted with.
    const MATCH_UP_TO_V0_9_0: &[u8] = include_bytes!("fixtures/2025-09-20_club_arezzo.zip");
    /// Middle blocker 2, who serves in the set of the match.
    const MB2: &str = "00000000-0000-4000-8000-000000000003";

    struct Providers(Sqlite);

    impl std::ops::Deref for Providers {
        type Target = Sqlite;
        fn deref(&self) -> &Sqlite {
            &self.0
        }
    }

    impl Providers {
        fn new() -> Self {
            Providers(Sqlite::new())
        }

        async fn import_team(
            &self,
            archive: &[u8],
            file_id: Option<Uuid>,
        ) -> Result<TeamEntry, AppError> {
            let imported = import_legacy_team(&self.db, archive, file_id, &Language::default())?;
            self.team_reader.read_single(&imported.id).await
        }

        async fn import_match(
            &self,
            archive: &[u8],
            team: &TeamEntry,
        ) -> Result<MatchEntry, AppError> {
            let imported = import_legacy_match(&self.db, archive, team.id, &Language::default())?;
            let team = self.team_reader.read_single(&team.id).await?;
            self.match_reader
                .read_single(&team, &imported.id.to_string())
                .await
        }

        /// The team and the match of the fixtures.
        async fn legacy_match(&self) -> (TeamEntry, MatchEntry) {
            let team = self
                .import_team(TEAM_UP_TO_V0_9_0, Uuid::parse_str(TEAM_ID).ok())
                .await
                .expect("expected the team");
            let m = self
                .import_match(MATCH_UP_TO_V0_9_0, &team)
                .await
                .expect("expected the match");
            (team, m)
        }

        /// The matches of the team.
        async fn matches(&self, team: &TeamEntry) -> Vec<MatchEntry> {
            self.match_reader
                .read_all(team)
                .await
                .unwrap()
                .into_iter()
                .map(|m| m.unwrap())
                .collect()
        }

        /// Another team, with players of its own.
        async fn other_team(&self) -> TeamEntry {
            let id = Uuid::parse_str(TEAM_ID).unwrap();
            let team = self.team_reader.read_single(&id).await;
            let players = team.map(|t| t.players).unwrap_or_default();
            self.team_writer
                .save(TeamInput::New {
                    id: None,
                    name: "Other Club".into(),
                    year: 2025,
                    classification: None,
                    gender: None,
                    country: None,
                    players: players
                        .into_iter()
                        .map(|mut p| {
                            p.id = Uuid::new_v4();
                            p
                        })
                        .collect(),
                })
                .await
                .unwrap()
        }
    }

    /// The match `m` exported from `from` (an archive of the database) and
    /// imported into a new database.
    async fn moved(from: &Providers, m: &MatchEntry) -> (Providers, TeamEntry, MatchEntry) {
        let id = Uuid::parse_str(&m.id).unwrap();
        let archive = archives::export(&from.db, Subject::Match(id)).unwrap();
        let to = Providers::new();
        archives::import(&to.db, &archive, &Language::default()).unwrap();
        let team = to.team_reader.read_single(&m.team.id).await.unwrap();
        let again = to.match_reader.read_single(&team, &m.id).await.unwrap();
        (to, team, again)
    }

    /// The player with `number` in the fixtures.
    fn numbered(number: u8) -> Uuid {
        Uuid::from_u128(0x00000000_0000_4000_8000_000000000000 + u128::from(number))
    }

    /// The archive with the file `name` changed by `edit`, as by hand.
    fn edited(archive: &[u8], name: &str, edit: impl Fn(String) -> String) -> Vec<u8> {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).unwrap();
        let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for i in 0..zip.len() {
            let file = zip.by_index(i).unwrap();
            let file_name = file.name().unwrap().to_string();
            let content = std::io::read_to_string(file).unwrap();
            let content = match file_name == name {
                true => edit(content),
                false => content,
            };
            out.start_file::<_, ()>(file_name.as_str(), zip::write::FileOptions::default())
                .unwrap();
            out.write_all(content.as_bytes()).unwrap();
        }
        out.finish().unwrap().into_inner()
    }

    /// The match descriptor with `roster`.
    fn with_roster(json: &str, roster: &[Uuid]) -> String {
        let mut descriptor: serde_json::Value = serde_json::from_str(json).unwrap();
        descriptor["roster"] = serde_json::json!(roster);
        descriptor.to_string()
    }

    fn message(result: Result<impl std::fmt::Debug, AppError>) -> String {
        match result {
            Err(AppError::IO(IOError::Msg(message))) => message,
            other => panic!("expected a message, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn team_archive_up_to_v0_9_0_takes_the_id_of_its_name() {
        let p = Providers::new();
        let team = p
            .import_team(TEAM_UP_TO_V0_9_0, Uuid::parse_str(TEAM_ID).ok())
            .await
            .unwrap();
        assert_eq!(team.id.to_string(), TEAM_ID);
        assert_eq!(team.name, "Volley Club");
        assert_eq!(team.players.len(), 8);
        assert!(p.team_reader.exists(&team.id).await.unwrap());
    }

    #[tokio::test]
    async fn team_archive_up_to_v0_9_0_renamed_is_refused() {
        let p = Providers::new();
        let result = p.import_team(TEAM_UP_TO_V0_9_0, None).await;
        assert_eq!(message(result), EN.team_archive_without_id);
    }

    /// From v0.10.1 the id is in the archive: renamed on the way, e.g.
    /// received twice, it keeps it; again, it's the same team.
    #[tokio::test]
    async fn team_archive_keeps_its_id_whatever_its_name() {
        let archive = edited(TEAM_UP_TO_V0_9_0, "team.json", |json| {
            let mut team: serde_json::Value = serde_json::from_str(&json).unwrap();
            team["id"] = TEAM_ID.into();
            team.to_string()
        });
        let p = Providers::new();
        let imported = p.import_team(&archive, Some(Uuid::new_v4())).await.unwrap();
        assert_eq!(imported.id.to_string(), TEAM_ID);
        let again = p.import_team(&archive, None).await.unwrap();
        assert_eq!(again, imported);
        assert_eq!(p.team_reader.read_all().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn match_archive_up_to_v0_9_0_replays_as_scouted() {
        let (_, m) = Providers::new().legacy_match().await;
        // saved without its rules, as from its folder: those it replays with
        assert_eq!(m.sets[0].lineup_rules, None);
        assert_eq!(
            m.sets[0].find_lineup_rules(m.max_substitutions).unwrap(),
            LineupRulesEnum::V1
        );
        // no roster up to v0.10.1: the active team
        let active: Vec<Uuid> = m.team.active_players().iter().map(|p| p.id).collect();
        assert_eq!(m.roster, active);
        let snapshot = m.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!((snapshot.score_us, snapshot.score_them), (3, 1));
        assert_eq!(
            snapshot.current_lineup.get_setter(),
            Uuid::parse_str(MB2).ok()
        );
    }

    /// A match created with v0.3.0, which saved the substitutions but not
    /// the rules of the sets, as exported up to v0.9.0: its set replays with
    /// the rules of v0.3.0, as from its folder, though those of v0.2.0
    /// replay it too (with middle blocker 2 serving).
    #[tokio::test]
    async fn match_archive_of_v0_3_0_replays_as_scouted() {
        let p = Providers::new();
        let team = p
            .import_team(TEAM_UP_TO_V0_9_0, Uuid::parse_str(TEAM_ID).ok())
            .await
            .unwrap();
        let descriptor = edited(MATCH_UP_TO_V0_9_0, "match.json", |json| {
            let mut descriptor: serde_json::Value = serde_json::from_str(&json).unwrap();
            descriptor["max_substitutions"] = 6.into();
            descriptor.to_string()
        });
        let archive = edited(&descriptor, "set_1.csv", |csv| {
            csv.lines().take(4).map(|row| format!("{row}\n")).collect()
        });
        let Ok(Archive::Match(read)) = read_archive(&archive, EN) else {
            panic!("expected a match");
        };
        assert_eq!(read.sets[0].lineup_rules, Some(LineupRulesEnum::V2));
        let m = p.import_match(&archive, &team).await.unwrap();
        assert_eq!(m.sets[0].lineup_rules, Some(LineupRulesEnum::V2));
        let snapshot = m.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!(
            snapshot.current_lineup.get_serving_player(),
            Some(numbered(8))
        );
    }

    /// A match from DataVolley on the video clock (from v0.15.0) stays so,
    /// and so it moves on from the database.
    #[tokio::test]
    async fn match_archive_on_its_clock() {
        let p = Providers::new();
        let team = p
            .import_team(TEAM_UP_TO_V0_9_0, Uuid::parse_str(TEAM_ID).ok())
            .await
            .unwrap();
        let archive = edited(MATCH_UP_TO_V0_9_0, "match.json", |json| {
            let mut descriptor: serde_json::Value = serde_json::from_str(&json).unwrap();
            descriptor["source"] = "dvw".into();
            descriptor["video_clock"] = true.into();
            descriptor.to_string()
        });
        let m = p.import_match(&archive, &team).await.unwrap();
        assert_eq!((m.source, m.video_clock), (MatchSource::Dvw, true));
        let (_, _, again) = moved(&p, &m).await;
        assert_eq!((again.source, again.video_clock), (MatchSource::Dvw, true));
    }

    /// The match moves on from the database with its rules, once.
    #[tokio::test]
    async fn match_archive_moves_on_with_its_rules() {
        let from = Providers::new();
        let (_, m) = from.legacy_match().await;
        let (to, team, again) = moved(&from, &m).await;
        assert_eq!(again.sets, m.sets);
        assert_eq!(
            again.sets[0]
                .find_lineup_rules(m.max_substitutions)
                .unwrap(),
            LineupRulesEnum::V1
        );
        let snapshot = again.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!((snapshot.score_us, snapshot.score_them), (3, 1));
        let twice = to.import_match(MATCH_UP_TO_V0_9_0, &team).await;
        assert_eq!(message(twice), EN.match_already_exists);
        assert_eq!(to.matches(&team).await.len(), 1);
    }

    /// The details of the players and the notes of the match go with the
    /// archives.
    #[tokio::test]
    async fn archives_carry_the_details_and_the_notes() {
        let from = Providers::new();
        let (mut team, m) = from.legacy_match().await;
        let mut player = team.players[0].clone();
        player.birth_date = chrono::NaiveDate::from_ymd_opt(2001, 12, 3);
        player.height_cm = Some(182);
        player.notes = "captain".into();
        from.team_writer
            .save_player(PlayerInput::Existing(player.clone()), &mut team)
            .await
            .unwrap();
        from.match_writer
            .update(
                &m,
                m.opponent.clone(),
                m.date,
                m.home,
                m.max_substitutions,
                "windy".into(),
                (1..=8).map(numbered).collect(),
            )
            .await
            .unwrap();
        let (_, team, again) = moved(&from, &m).await;
        let imported = team.find_player(player.id).unwrap();
        assert_eq!(
            (
                imported.birth_date,
                imported.height_cm,
                imported.notes.as_str()
            ),
            (player.birth_date, Some(182), "captain")
        );
        assert_eq!(again.notes, "windy");
        assert_eq!(again.roster, (1..=8).map(numbered).collect::<Vec<_>>());
    }

    /// v0.4.3 added an event, without the technique, to a set scouted
    /// later.
    #[tokio::test]
    async fn match_archive_with_rows_of_v0_4_3_is_imported() {
        let p = Providers::new();
        let team = p
            .import_team(TEAM_UP_TO_V0_9_0, Uuid::parse_str(TEAM_ID).ok())
            .await
            .unwrap();
        let ace = format!("2025-09-20T20:01:00.000000Z,S,{MB2},#,\n");
        let archive = edited(MATCH_UP_TO_V0_9_0, "set_1.csv", |csv| csv + &ace);
        let imported = p.import_match(&archive, &team).await.unwrap();
        let snapshot = imported.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!((snapshot.score_us, snapshot.score_them), (4, 1));
    }

    /// A set this version doesn't replay with its rules, e.g. saved by a
    /// version whose scouting allowed an event no more: the import says
    /// which and writes nothing.
    #[tokio::test]
    async fn match_archive_with_a_set_that_does_not_replay_is_refused() {
        let p = Providers::new();
        let team = p
            .import_team(TEAM_UP_TO_V0_9_0, Uuid::parse_str(TEAM_ID).ok())
            .await
            .unwrap();
        let Ok(Archive::Match(read)) = read_archive(MATCH_UP_TO_V0_9_0, EN) else {
            panic!("expected a match");
        };
        let rows = read.sets[0].events.len();
        // their error gives us the serve: they can't score an ace
        let rows_added = "2025-09-20T20:01:00.000000Z,OE,,,\n2025-09-20T20:01:01.000000Z,OS,,,\n";
        let archive = edited(MATCH_UP_TO_V0_9_0, "set_1.csv", |csv| csv + rows_added);
        // the rules in the set, as saved from v0.4.0
        let archive = edited(&archive, "set_1.json", |json| {
            let mut set: serde_json::Value = serde_json::from_str(&json).unwrap();
            set["lineup_rules"] = "v1".into();
            set.to_string()
        });
        let result = p.import_match(&archive, &team).await;
        let why = EN
            .set_event_not_valid
            .replace("{set}", "1")
            .replace("{row}", &(rows + 2).to_string())
            .replace("{event}", "OS")
            .replace("{previous}", "OE");
        assert_eq!(message(result), why);
        assert!(p.matches(&team).await.is_empty());
    }

    #[tokio::test]
    async fn match_of_another_team_is_refused() {
        let p = Providers::new();
        p.import_team(TEAM_UP_TO_V0_9_0, Uuid::parse_str(TEAM_ID).ok())
            .await
            .unwrap();
        let other = p.other_team().await;
        let result = p.import_match(MATCH_UP_TO_V0_9_0, &other).await;
        assert_eq!(message(result), EN.match_of_another_team);
        assert!(p.matches(&other).await.is_empty());
    }

    /// A substitution that brings in a player who isn't of the team, e.g.
    /// written by hand: the match is of another team.
    #[tokio::test]
    async fn a_match_bringing_in_a_stranger_is_of_another_team() {
        let p = Providers::new();
        let team = p
            .import_team(TEAM_UP_TO_V0_9_0, Uuid::parse_str(TEAM_ID).ok())
            .await
            .unwrap();
        let stranger = Uuid::new_v4().to_string();
        let archive = edited(MATCH_UP_TO_V0_9_0, "set_1.csv", |csv| {
            csv.replace(&numbered(8).to_string(), &stranger)
        });
        assert!(read_archive(&archive, EN).is_ok());
        let result = p.import_match(&archive, &team).await;
        assert_eq!(message(result), EN.match_of_another_team);
        assert!(p.matches(&team).await.is_empty());
    }

    /// A roster written by hand, as no version saves it: without who came
    /// in and with a player of another team. The match keeps the players of
    /// the team and who played, as the match form does.
    #[tokio::test]
    async fn the_roster_of_the_archive_keeps_who_played() {
        let p = Providers::new();
        let team = p
            .import_team(TEAM_UP_TO_V0_9_0, Uuid::parse_str(TEAM_ID).ok())
            .await
            .unwrap();
        let roster: Vec<Uuid> = (1..=7).map(numbered).chain([Uuid::new_v4()]).collect();
        let archive = edited(MATCH_UP_TO_V0_9_0, "match.json", |json| {
            with_roster(&json, &roster)
        });
        let m = p.import_match(&archive, &team).await.unwrap();
        assert_eq!(m.roster, (1..=8).map(numbered).collect::<Vec<_>>());
        assert_eq!(m.team.active_players().len(), 8);
    }

    /// Before importing, e.g. to show the archive opened from outside: the
    /// match goes to its team only, until imported.
    #[tokio::test]
    async fn the_match_finds_its_team_until_imported() {
        let p = Providers::new();
        p.other_team().await;
        let none = open(&p.db, MATCH_UP_TO_V0_9_0, EN);
        assert_eq!(message(none), EN.match_of_another_team);
        let team = p
            .import_team(TEAM_UP_TO_V0_9_0, Uuid::parse_str(TEAM_ID).ok())
            .await
            .unwrap();
        let Ok(Opened::LegacyMatch { archived, teams }) = open(&p.db, MATCH_UP_TO_V0_9_0, EN)
        else {
            panic!("expected a match");
        };
        assert_eq!(archived.id, MATCH_ID);
        assert_eq!(archived.opponent, "Club Arezzo");
        assert_eq!(teams.iter().map(|t| t.id).collect::<Vec<_>>(), [team.id]);
        p.import_match(MATCH_UP_TO_V0_9_0, &team).await.unwrap();
        let again = open(&p.db, MATCH_UP_TO_V0_9_0, EN);
        assert_eq!(message(again), EN.match_already_exists);
    }

    #[test]
    fn the_id_of_a_team_in_the_name_of_its_file() {
        assert_eq!(
            file_team_id(&format!("{TEAM_ID}.zip")),
            Uuid::parse_str(TEAM_ID).ok()
        );
        assert_eq!(file_team_id(TEAM_ID), Uuid::parse_str(TEAM_ID).ok());
        assert_eq!(file_team_id(&format!("{TEAM_ID} (1).zip")), None);
        assert_eq!(file_team_id("Volley Club.zip"), None);
    }

    #[tokio::test]
    async fn an_archive_of_another_kind_is_refused() {
        let p = Providers::new();
        let team = p.import_team(TEAM_UP_TO_V0_9_0, Uuid::parse_str(TEAM_ID).ok());
        let team = team.await.unwrap();
        let result = p.import_match(TEAM_UP_TO_V0_9_0, &team).await;
        assert_eq!(message(result), EN.invalid_archive);
        let result = p.import_team(MATCH_UP_TO_V0_9_0, None).await;
        assert_eq!(message(result), EN.invalid_archive);
        let result = p.import_team(b"not a zip", None).await;
        assert_eq!(message(result), EN.invalid_archive);
    }
}
