//! The archives of a team or a match move between the versions, both ways:
//! the fixtures are archives as exported up to v0.9.0, with the data saved
//! by the older versions.

#[cfg(test)]
mod tests {
    use crate::{
        archive::{
            Archive, export_match, export_team, file_team_id, import_match, import_team,
            match_to_import, read_archive, teams_of_match,
        },
        errors::{AppError, IOError},
        localization::Language,
        providers::{
            match_reader::MatchReader,
            store::{
                match_reader::StoreMatchReader, match_writer::StoreMatchWriter,
                set_reader::StoreSetReader, set_writer::StoreSetWriter,
                team_reader::StoreTeamReader, team_writer::StoreTeamWriter,
            },
            team_reader::TeamReader,
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{enums::LineupRulesEnum, r#match::MatchEntry, team::TeamEntry},
        storage::{Store, layout, memory::MemoryStore},
        tests::EN,
    };
    use std::sync::Arc;
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

    struct Providers {
        store: Arc<dyn Store>,
        team_reader: StoreTeamReader,
        team_writer: StoreTeamWriter,
        match_reader: StoreMatchReader,
        match_writer: StoreMatchWriter,
        set_writer: StoreSetWriter,
    }

    impl Providers {
        fn new() -> Self {
            let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
            Self {
                team_reader: StoreTeamReader::new(Arc::clone(&store), Language::default()),
                team_writer: StoreTeamWriter::new(Arc::clone(&store)),
                match_reader: StoreMatchReader::new(
                    Arc::clone(&store),
                    Arc::new(StoreSetReader::new(Arc::clone(&store), Language::default())),
                    Language::default(),
                ),
                match_writer: StoreMatchWriter::new(Arc::clone(&store)),
                set_writer: StoreSetWriter::new(Arc::clone(&store), Language::default()),
                store,
            }
        }

        async fn import_team(
            &self,
            archive: &[u8],
            file_id: Option<Uuid>,
        ) -> Result<TeamEntry, AppError> {
            import_team(archive, file_id, &self.team_reader, &self.team_writer, EN).await
        }

        async fn import_match(
            &self,
            archive: &[u8],
            team: &TeamEntry,
        ) -> Result<MatchEntry, AppError> {
            import_match(
                archive,
                team,
                &self.match_reader,
                &self.match_writer,
                &self.set_writer,
                EN,
            )
            .await
        }

        /// The team and the match of the fixtures.
        async fn legacy_match(&self) -> (TeamEntry, MatchEntry) {
            let team = self
                .import_team(TEAM_UP_TO_V0_9_0, Uuid::parse_str(TEAM_ID).ok())
                .await
                .expect("expected the team");
            self.import_match(MATCH_UP_TO_V0_9_0, &team)
                .await
                .expect("expected the match");
            let m = self
                .match_reader
                .read_single(&team, MATCH_ID)
                .await
                .expect("expected a readable match");
            (team, m)
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

    /// The archive is renamed on the way, e.g. received twice.
    #[tokio::test]
    async fn team_archive_keeps_its_id_whatever_its_name() {
        let from = Providers::new();
        let (team, _) = from.legacy_match().await;
        let archive = export_team(&*from.store, &team.id).await.unwrap();
        let to = Providers::new();
        let imported = to
            .import_team(&archive, Some(Uuid::new_v4()))
            .await
            .unwrap();
        assert_eq!(imported.id, team.id);
        let again = to.import_team(&archive, None).await;
        assert_eq!(message(again), EN.team_already_exists);
    }

    /// Up to v0.9.0 the descriptor is read as a team, the id ignored.
    #[tokio::test]
    async fn team_archive_is_read_up_to_v0_9_0() {
        let p = Providers::new();
        let (team, _) = p.legacy_match().await;
        let archive = export_team(&*p.store, &team.id).await.unwrap();
        let Ok(Archive::Team { team: read, id }) = read_archive(&archive, EN) else {
            panic!("expected a team");
        };
        assert_eq!(id, Some(team.id));
        assert_eq!(read.players.len(), team.players.len());
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).unwrap();
        let names: Vec<_> = zip.file_names().map(|n| n.unwrap().to_string()).collect();
        assert_eq!(names, ["team.json"]);
        let json = std::io::read_to_string(zip.by_index(0).unwrap()).unwrap();
        let old: TeamEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(old.name, team.name);
    }

    #[tokio::test]
    async fn match_archive_up_to_v0_9_0_replays_as_scouted() {
        let (_, m) = Providers::new().legacy_match().await;
        assert_eq!(m.sets[0].lineup_rules, Some(LineupRulesEnum::V1));
        let snapshot = m.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!((snapshot.score_us, snapshot.score_them), (3, 1));
        assert_eq!(
            snapshot.current_lineup.get_setter(),
            Uuid::parse_str(MB2).ok()
        );
    }

    /// The match moves again, from this version.
    #[tokio::test]
    async fn match_archive_moves_on_with_its_rules() {
        let from = Providers::new();
        let (team, m) = from.legacy_match().await;
        let archive = export_match(&*from.store, &team.id, &m.id).await.unwrap();
        let to = Providers::new();
        let team_archive = export_team(&*from.store, &team.id).await.unwrap();
        let team = to.import_team(&team_archive, None).await.unwrap();
        to.import_match(&archive, &team).await.unwrap();
        let again = to.match_reader.read_single(&team, MATCH_ID).await.unwrap();
        assert_eq!(again.sets[0].lineup_rules, Some(LineupRulesEnum::V1));
        let snapshot = again.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!((snapshot.score_us, snapshot.score_them), (3, 1));
        let twice = to.import_match(&archive, &team).await;
        assert_eq!(message(twice), EN.match_already_exists);
    }

    /// v0.4.3 added an event, without the technique, to a set scouted
    /// later.
    #[tokio::test]
    async fn match_archive_with_rows_of_v0_4_3_is_imported() {
        let from = Providers::new();
        let (team, m) = from.legacy_match().await;
        let events = layout::set_events(&team.id, &m.id, 1);
        let ace = format!("2025-09-20T20:01:00.000000Z,S,{MB2},#,\n");
        from.store.append(&events, ace.as_bytes()).await.unwrap();
        let archive = export_match(&*from.store, &team.id, &m.id).await.unwrap();
        let to = Providers::new();
        let team_archive = export_team(&*from.store, &team.id).await.unwrap();
        let team = to.import_team(&team_archive, None).await.unwrap();
        to.import_match(&archive, &team).await.unwrap();
        let imported = to.match_reader.read_single(&team, MATCH_ID).await.unwrap();
        let snapshot = imported.get_set_snapshot(1).unwrap().unwrap();
        assert_eq!((snapshot.score_us, snapshot.score_them), (4, 1));
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
        assert!(!p.match_reader.exists(&other, MATCH_ID).await.unwrap());
    }

    #[tokio::test]
    async fn the_match_finds_its_team() {
        let p = Providers::new();
        let team = p
            .import_team(TEAM_UP_TO_V0_9_0, Uuid::parse_str(TEAM_ID).ok())
            .await
            .unwrap();
        let teams = [p.other_team().await, team];
        let Ok(Archive::Match(m)) = read_archive(MATCH_UP_TO_V0_9_0, EN) else {
            panic!("expected a match");
        };
        assert_eq!(m.id, MATCH_ID);
        assert_eq!(m.opponent, "Club Arezzo");
        let found: Vec<_> = teams_of_match(&m, &teams).iter().map(|t| t.id).collect();
        assert_eq!(found, [teams[1].id]);
    }

    /// Before importing, e.g. to show the archive opened from outside.
    #[tokio::test]
    async fn the_match_to_import_goes_to_its_team_until_imported() {
        let p = Providers::new();
        let other = p.other_team().await;
        let only_other = [other.clone()];
        let none = match_to_import(MATCH_UP_TO_V0_9_0, &only_other, &p.match_reader, EN);
        assert_eq!(message(none.await), EN.match_of_another_team);
        let team = p
            .import_team(TEAM_UP_TO_V0_9_0, Uuid::parse_str(TEAM_ID).ok())
            .await
            .unwrap();
        let teams = [other, team];
        let (m, found) = match_to_import(MATCH_UP_TO_V0_9_0, &teams, &p.match_reader, EN)
            .await
            .unwrap();
        assert_eq!(m.opponent, "Club Arezzo");
        assert_eq!(
            found.iter().map(|t| t.id).collect::<Vec<_>>(),
            [teams[1].id]
        );
        p.import_match(MATCH_UP_TO_V0_9_0, &teams[1]).await.unwrap();
        let again = match_to_import(MATCH_UP_TO_V0_9_0, &teams, &p.match_reader, EN);
        assert_eq!(message(again.await), EN.match_already_exists);
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
