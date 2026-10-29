//! The team a new installation starts with.

#[cfg(test)]
mod tests {
    use crate::{
        example::{
            add_example_team_if_new, example_team, is_new_installation, tutorial_lineup,
            tutorial_match,
        },
        localization::{Language, labels},
        providers::team_writer::TeamInput,
        providers::{
            store::{team_reader::StoreTeamReader, team_writer::StoreTeamWriter},
            team_reader::TeamReader,
        },
        set_start::SetStartForm,
        shapes::enums::TeamSideEnum,
        shapes::enums::{GenderEnum, LanguageEnum, RoleEnum, TeamClassificationEnum},
        storage::{Store, layout, memory::MemoryStore},
        tests::EN,
    };
    use chrono::{Datelike, Local};
    use std::{collections::HashSet, sync::Arc};

    fn store() -> Arc<dyn Store> {
        Arc::new(MemoryStore::new())
    }

    #[tokio::test]
    async fn an_empty_folder_or_one_with_the_log_only_is_new() {
        let store = store();
        assert!(is_new_installation(store.as_ref()).await.unwrap());
        store.write("scout4all.log", b"started").await.unwrap();
        assert!(is_new_installation(store.as_ref()).await.unwrap());
    }

    #[tokio::test]
    async fn the_settings_or_a_team_make_an_update() {
        let with_settings = store();
        with_settings.write(layout::CONFIG, b"{}").await.unwrap();
        assert!(!is_new_installation(with_settings.as_ref()).await.unwrap());
        let with_team = store();
        with_team.write("some-team/team.json", b"{}").await.unwrap();
        assert!(!is_new_installation(with_team.as_ref()).await.unwrap());
    }

    #[tokio::test]
    async fn a_new_installation_gets_the_national_team_once() {
        let store = store();
        let writer = StoreTeamWriter::new(Arc::clone(&store));
        let added = add_example_team_if_new(store.as_ref(), &writer, EN)
            .await
            .unwrap();
        assert!(added.is_some());
        let again = add_example_team_if_new(store.as_ref(), &writer, EN)
            .await
            .unwrap();
        assert!(again.is_none());

        let teams = StoreTeamReader::new(Arc::clone(&store), Language::default())
            .read_all()
            .await
            .unwrap();
        assert_eq!(teams.len(), 1);
        let team = match teams.into_iter().next() {
            Some(Ok(team)) => team,
            _ => unreachable!("the example team should be readable"),
        };
        assert_eq!(team.name, "Italy");
        assert_eq!(i32::from(team.year), Local::now().year());
        assert_eq!(
            team.classification,
            Some(TeamClassificationEnum::TopInternational)
        );
        assert_eq!(team.gender, Some(GenderEnum::Women));
        assert_eq!(team.players.len(), 14);
        let role = |number: u8| {
            team.players
                .iter()
                .find(|p| p.number == number)
                .and_then(|p| p.role)
        };
        assert_eq!(role(8), Some(RoleEnum::Setter));
        assert_eq!(role(5), Some(RoleEnum::Libero));
        assert_eq!(role(18), Some(RoleEnum::OppositeHitter));
        assert_eq!(role(17), Some(RoleEnum::OutsideHitter));
        assert_eq!(role(11), Some(RoleEnum::MiddleBlocker));
        let numbers: HashSet<u8> = team.players.iter().map(|p| p.number).collect();
        assert_eq!(numbers.len(), 14);
    }

    #[tokio::test]
    async fn an_update_gets_nothing() {
        let store = store();
        store.write(layout::CONFIG, b"{}").await.unwrap();
        let writer = StoreTeamWriter::new(Arc::clone(&store));
        let added = add_example_team_if_new(store.as_ref(), &writer, EN)
            .await
            .unwrap();
        assert!(added.is_none());
        assert_eq!(store.list(layout::ROOT).await.unwrap().len(), 1);
    }

    /// The statistics uploaded from two installations don't mix.
    #[tokio::test]
    async fn each_installation_has_its_own_ids() {
        let add = || async {
            let store = store();
            let writer = StoreTeamWriter::new(Arc::clone(&store));
            add_example_team_if_new(store.as_ref(), &writer, labels(LanguageEnum::It))
                .await
                .unwrap()
                .unwrap()
        };
        let (one, other) = (add().await, add().await);
        assert_eq!(one.name, "Italia");
        assert_ne!(one.id, other.id);
        let ids = |team: &crate::shapes::team::TeamEntry| -> HashSet<_> {
            team.players.iter().map(|p| p.id).collect()
        };
        assert!(ids(&one).is_disjoint(&ids(&other)));
    }

    fn team() -> crate::shapes::team::TeamEntry {
        match example_team(EN, 2026) {
            TeamInput::New {
                players,
                name,
                year,
                classification,
                gender,
                ..
            } => crate::shapes::team::TeamEntry {
                name,
                year,
                players,
                classification,
                gender,
                ..Default::default()
            },
            TeamInput::Existing(team) => team,
        }
    }

    /// We receive in P6: the setter in 6, the outside hitter in 4 receives.
    #[test]
    fn the_lineup_of_the_tutorial_is_complete_in_p6() {
        let team = team();
        let mut form = SetStartForm::new(1, None);
        tutorial_lineup(&mut form, &team);
        let number = |id: uuid::Uuid| team.find_player(id).map(|p| p.number);
        let input = form.validate(&team).unwrap();
        assert_eq!(input.serving_team, TeamSideEnum::Them);
        assert_eq!(
            input.positions.map(|id| number(id).unwrap()),
            [17, 19, 18, 16, 11, 8]
        );
        assert_eq!(number(input.setter), Some(8));
        assert_eq!(input.positions[5], input.setter);
        assert_eq!(number(input.libero), Some(5));
        assert_eq!(input.fallback_libero.and_then(number), Some(7));
    }

    #[test]
    fn the_match_of_the_tutorial_is_at_home_against_brazil() {
        let date = chrono::Utc::now().fixed_offset();
        let form = tutorial_match(labels(LanguageEnum::It), date, &[]);
        assert_eq!(form.opponent, "Brasile");
        assert!(form.home);
        assert_eq!(form.date, Some(date));
        assert!(form.validate(&team(), None).is_ok());
    }

    /// Seen again the same day: another match, not one already there.
    #[test]
    fn the_match_of_the_tutorial_seen_again_is_numbered() {
        let date = chrono::Utc::now().fixed_offset();
        let id = |opponent: &str| crate::shapes::r#match::MatchEntry::id_of(opponent, &date);
        let taken = vec![id("Brazil"), id("Brazil 2")];
        assert_eq!(tutorial_match(EN, date, &taken).opponent, "Brazil 3");
        let yesterday = date - chrono::Duration::days(1);
        assert_eq!(tutorial_match(EN, yesterday, &taken).opponent, "Brazil");
    }
}
