//! Starting a set without a user interface: which set is next, the choices
//! of the lineup and its checks.

#[cfg(test)]
mod tests {
    use crate::{
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        localization::Language,
        providers::{
            match_writer::MatchWriter,
            set_writer::SetWriter,
            store::{match_writer::StoreMatchWriter, set_writer::StoreSetWriter},
        },
        scouting::ScoutingStep,
        set_start::{self, NextSet, SetStartError, SetStartForm, next_set},
        shapes::{
            enums::{RoleEnum, TeamSideEnum},
            r#match::MatchStatus,
            player::PlayerEntry,
            team::TeamEntry,
        },
        storage::{Store, memory::MemoryStore},
        tests::EN,
    };
    use chrono::Utc;
    use std::sync::Arc;
    use uuid::Uuid;

    /// A team of `active` players, numbered from 1, the seventh a libero,
    /// then a removed one.
    fn team(active: usize) -> TeamEntry {
        let players = (0..=active)
            .map(|i| PlayerEntry {
                id: Uuid::new_v4(),
                name: format!("player {}", i + 1),
                role: Some(match i {
                    6 => RoleEnum::Libero,
                    _ => RoleEnum::OutsideHitter,
                }),
                number: i as u8 + 1,
                deleted: i == active,
            })
            .collect();
        TeamEntry {
            name: "Volley Club".into(),
            year: 2025,
            players,
            id: Uuid::new_v4(),
            classification: None,
            gender: None,
        }
    }

    fn ids(players: &[&PlayerEntry]) -> Vec<Uuid> {
        players.iter().map(|p| p.id).collect()
    }

    fn id(team: &TeamEntry, i: usize) -> Uuid {
        team.players[i].id
    }

    fn removed(team: &TeamEntry) -> Uuid {
        team.players
            .iter()
            .find(|p| p.deleted)
            .map(|p| p.id)
            .unwrap()
    }

    /// The first six on court, the first the setter, the seventh the
    /// libero.
    fn complete(team: &TeamEntry) -> SetStartForm {
        let mut form = SetStartForm::new(1, None);
        form.serving_team = Some(TeamSideEnum::Us);
        for i in 0..6 {
            form.place(i, Some(id(team, i)));
        }
        form.setter = Some(id(team, 0));
        form.libero = Some(id(team, 6));
        form
    }

    fn status(next_set_number: u8, last_serving_team: Option<TeamSideEnum>) -> MatchStatus {
        MatchStatus {
            us_wins: next_set_number - 1,
            them_wins: 0,
            next_set_number: Some(next_set_number),
            last_incomplete_set: None,
            match_finished: false,
            last_serving_team,
        }
    }

    #[test]
    fn the_toss_decides_who_serves_in_the_first_set_and_in_the_tie_break() {
        let first = SetStartForm::new(1, None);
        let tie_break = SetStartForm::new(5, Some(TeamSideEnum::Us));
        assert!(first.chooses_serving_team() && first.serving_team.is_none());
        assert!(tie_break.chooses_serving_team() && tie_break.serving_team.is_none());
    }

    #[test]
    fn the_team_that_received_first_serves_first_in_the_next_set() {
        let form = |last| match next_set(&status(2, Some(last))) {
            NextSet::Start(form) => Some(form),
            _ => None,
        };
        let after_us = form(TeamSideEnum::Us).unwrap();
        assert_eq!(after_us.set_number, 2);
        assert!(!after_us.chooses_serving_team());
        assert_eq!(after_us.serving_team, Some(TeamSideEnum::Them));
        assert_eq!(
            form(TeamSideEnum::Them).unwrap().serving_team,
            Some(TeamSideEnum::Us)
        );
    }

    #[test]
    fn nothing_to_start_when_the_match_is_over() {
        let over = MatchStatus {
            us_wins: 3,
            next_set_number: None,
            match_finished: true,
            ..status(4, Some(TeamSideEnum::Us))
        };
        assert!(matches!(next_set(&over), NextSet::Over));
    }

    #[test]
    fn removed_players_are_never_offered() {
        let team = team(8);
        let form = SetStartForm::new(1, None);
        assert!(!ids(&form.position_choices(&team, 0)).contains(&removed(&team)));
        let form = complete(&team);
        assert!(!ids(&form.libero_choices(&team)).contains(&removed(&team)));
        assert!(!ids(&form.fallback_libero_choices(&team)).contains(&removed(&team)));
    }

    #[test]
    fn a_position_offers_the_players_neither_elsewhere_nor_liberos() {
        let team = team(8);
        let mut form = complete(&team);
        form.fallback_libero = Some(id(&team, 7));
        assert_eq!(ids(&form.position_choices(&team, 0)), vec![id(&team, 0)]);
    }

    #[test]
    fn the_setter_is_one_on_court_by_position() {
        let team = team(8);
        let mut form = complete(&team);
        form.place(0, Some(id(&team, 7)));
        let expected: Vec<Uuid> = [7, 1, 2, 3, 4, 5].iter().map(|i| id(&team, *i)).collect();
        assert_eq!(ids(&form.setter_choices(&team)), expected);
    }

    #[test]
    fn replacing_the_setter_on_court_drops_it() {
        let team = team(8);
        let mut form = complete(&team);
        form.place(0, Some(id(&team, 7)));
        assert_eq!(form.setter, None);
    }

    #[test]
    fn putting_the_libero_on_court_drops_it() {
        let team = team(8);
        let mut form = complete(&team);
        form.place(1, Some(id(&team, 6)));
        assert_eq!(form.libero, None);
    }

    #[test]
    fn no_fallback_libero_below_eight_active_players() {
        // eight players, one removed
        let seven = team(7);
        assert_eq!(seven.players.len(), 8);
        assert!(complete(&seven).fallback_libero_choices(&seven).is_empty());
        let eight = team(8);
        assert_eq!(
            ids(&complete(&eight).fallback_libero_choices(&eight)),
            vec![id(&eight, 7)]
        );
    }

    #[test]
    fn the_first_missing_choice_in_the_order_of_the_fields() {
        let team = team(8);
        let missing = |edit: fn(&mut SetStartForm)| {
            let mut form = complete(&team);
            edit(&mut form);
            form.validate(&team).err()
        };
        assert_eq!(
            missing(|f| {
                f.serving_team = None;
                f.place(2, None);
            }),
            Some(SetStartError::MissingServingTeam)
        );
        assert_eq!(
            missing(|f| {
                f.place(2, None);
                f.place(4, None);
            }),
            Some(SetStartError::MissingPlayer(3))
        );
        assert_eq!(
            missing(|f| {
                f.setter = None;
                f.libero = None;
            }),
            Some(SetStartError::MissingSetter)
        );
        assert_eq!(
            missing(|f| f.libero = None),
            Some(SetStartError::MissingLibero)
        );
    }

    #[test]
    fn an_invalid_lineup_is_rejected() {
        let team = team(8);
        let invalid = |edit: &dyn Fn(&mut SetStartForm)| {
            let mut form = complete(&team);
            edit(&mut form);
            form.validate(&team).err()
        };
        let removed = removed(&team);
        let libero = id(&team, 6);
        let other = id(&team, 7);
        let cases: [(&str, &dyn Fn(&mut SetStartForm)); 4] = [
            ("removed on court", &|f| f.positions[0] = Some(removed)),
            ("twice on court", &|f| f.positions[1] = f.positions[0]),
            ("setter off court", &|f| f.setter = Some(other)),
            ("fallback libero as the libero", &|f| {
                f.fallback_libero = Some(libero)
            }),
        ];
        for (case, edit) in cases {
            assert_eq!(invalid(edit), Some(SetStartError::InvalidPlayer), "{case}");
        }
    }

    #[test]
    fn a_complete_form_starts_the_set() {
        let team = team(8);
        let mut form = complete(&team);
        form.fallback_libero = Some(id(&team, 7));
        let input = form.validate(&team).unwrap();
        assert_eq!(input.set_number, 1);
        assert_eq!(input.serving_team, TeamSideEnum::Us);
        assert_eq!(
            input.positions.to_vec(),
            (0..6).map(|i| id(&team, i)).collect::<Vec<_>>()
        );
        assert_eq!(input.setter, id(&team, 0));
        assert_eq!(input.libero, id(&team, 6));
        assert_eq!(input.fallback_libero, Some(id(&team, 7)));
    }

    #[test]
    fn the_message_names_the_empty_position() {
        assert_eq!(
            SetStartError::MissingPlayer(3).message(EN),
            "player in position 3 is required"
        );
    }

    #[tokio::test]
    async fn starting_saves_the_set_and_scouts_it() {
        let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
        let team = team(8);
        let m = StoreMatchWriter::new(Arc::clone(&store))
            .create(
                &team,
                "Cortona Volley".into(),
                Utc::now().fixed_offset(),
                true,
                DEFAULT_MAX_SUBSTITUTIONS,
            )
            .await
            .unwrap();
        let set_writer: Arc<dyn SetWriter + Send + Sync> =
            Arc::new(StoreSetWriter::new(Arc::clone(&store), Language::default()));
        let input = complete(&team).validate(&team).unwrap();
        let session = set_start::start(m.clone(), input, Arc::clone(&set_writer))
            .await
            .unwrap();
        assert_eq!(session.set().set_number, 1);
        assert_eq!(session.set().serving_team, TeamSideEnum::Us);
        assert_eq!(session.step(), ScoutingStep::Event);
        let resumed = set_start::resume(m, session.set().clone(), set_writer).unwrap();
        assert_eq!(resumed.set().setter, id(&team, 0));
        assert_eq!(resumed.step(), ScoutingStep::Event);
    }
}
