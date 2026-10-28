//! Importing the archives of a team or a match from files, whatever their
//! name: the files received from a phone may be renamed on the way.

#[cfg(test)]
mod tests {
    use crate::localization::language;
    use crate::{
        screens::{
            file_system_screen::FileSystemAction, import_match_screen::ImportMatchAction,
            import_team_screen::ImportTeamAction,
        },
        tests::fs_store,
    };
    use chrono::Utc;
    use scout4all_core::{
        archive::{export_match, export_team},
        errors::{AppError, IOError},
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            set_writer::SetWriter,
            store::{
                match_reader::StoreMatchReader, match_writer::StoreMatchWriter,
                set_reader::StoreSetReader, set_writer::StoreSetWriter,
                team_reader::StoreTeamReader, team_writer::StoreTeamWriter,
            },
            team_reader::TeamReader,
            team_writer::{PlayerInput, TeamInput, TeamWriter},
        },
        shapes::{
            enums::{RoleEnum, TeamSideEnum},
            team::TeamEntry,
        },
    };
    use std::{path::Path, sync::Arc};
    use tempfile::TempDir;
    use uuid::Uuid;

    async fn team(base: &Path, name: &str) -> TeamEntry {
        let writer = StoreTeamWriter::new(fs_store(base));
        let mut team = writer
            .save(TeamInput::New {
                id: None,
                name: name.into(),
                year: 2025,
                classification: None,
                gender: None,
                players: vec![],
            })
            .await
            .unwrap();
        for i in 0..7 {
            let input = PlayerInput::New {
                name: format!("player {}", i + 1),
                role: RoleEnum::OutsideHitter,
                number: i + 1,
                details: Default::default(),
            };
            writer.save_player(input, &mut team).await.unwrap();
        }
        team
    }

    fn match_reader(base: &Path) -> Arc<StoreMatchReader> {
        Arc::new(StoreMatchReader::new(
            fs_store(base),
            Arc::new(StoreSetReader::new(fs_store(base), language())),
            language(),
        ))
    }

    #[tokio::test]
    async fn renamed_team_archive_is_imported() {
        let from = TempDir::new().unwrap();
        let team = team(from.path(), "Volley Club").await;
        let archive = export_team(&*fs_store(from.path()), &team.id)
            .await
            .unwrap();
        let to = TempDir::new().unwrap();
        let zip_path = to.path().join("Volley Club (1).zip");
        std::fs::write(&zip_path, archive).unwrap();
        let reader = Arc::new(StoreTeamReader::new(fs_store(to.path()), language()));
        let mut action = ImportTeamAction::new(
            reader.clone(),
            Arc::new(StoreTeamWriter::new(fs_store(to.path()))),
        );
        action
            .on_selected(&zip_path)
            .await
            .expect("expected an import");
        let imported = reader.read_single(&team.id).await.unwrap();
        assert_eq!(imported.name, "Volley Club");
        assert_eq!(imported.players.len(), 7);
    }

    #[tokio::test]
    async fn match_of_another_team_is_refused() {
        let dir = TempDir::new().unwrap();
        let base = dir.path();
        let ours = team(base, "Volley Club").await;
        let m = StoreMatchWriter::new(fs_store(base))
            .create(
                &ours,
                "Cortona".into(),
                Utc::now().into(),
                true,
                8,
                String::new(),
                vec![],
            )
            .await
            .unwrap();
        let ids: Vec<Uuid> = ours.players.iter().map(|p| p.id).collect();
        let positions: [Uuid; 6] = ids[..6].try_into().unwrap();
        StoreSetWriter::new(fs_store(base), language())
            .create(
                &m,
                1,
                TeamSideEnum::Us,
                positions,
                ids[6],
                None,
                ids[0],
                vec![],
            )
            .await
            .unwrap();
        let archive = export_match(&*fs_store(base), &ours.id, &m.id)
            .await
            .unwrap();
        let zip_path = base.join(format!("{}.zip", m.id));
        std::fs::write(&zip_path, archive).unwrap();

        let other = team(base, "Other Club").await;
        let reader = match_reader(base);
        let mut action = ImportMatchAction::new(
            other.clone(),
            reader.clone(),
            Arc::new(StoreMatchWriter::new(fs_store(base))),
            Arc::new(StoreSetWriter::new(fs_store(base), language())),
        );
        match action.on_selected(&zip_path).await {
            Err(AppError::IO(IOError::Msg(message))) => {
                assert_eq!(message, language().labels().match_of_another_team)
            }
            other => panic!("expected the match to be refused, got {:?}", other.err()),
        }
        assert!(reader.read_all(&other).await.unwrap().is_empty());
    }
}
