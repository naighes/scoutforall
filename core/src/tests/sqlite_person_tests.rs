//! The registry of the persons in the teams (TODO/001, step 22, D5, D15,
//! D16): a person of the registry added to one more team, the same person
//! there; the new season of a team, with the same persons; a person
//! removed from the registry named as a removed player.

#[cfg(test)]
mod tests {
    use crate::{
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        localization::{Language, labels},
        match_form::MatchInput,
        player_form::{PlayerForm, name_the_removed},
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            sqlite::{
                Database,
                matches::{SqliteMatchReader, SqliteMatchWriter},
                persons::{PersonTeam, SqlitePersons},
                sqlite,
                teams::{SqliteTeamReader, SqliteTeamWriter},
            },
            team_reader::TeamReader,
            team_writer::{PlayerInput, TeamInput, TeamWriter},
        },
        season::{self, SeasonError, SeasonMember},
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, LanguageEnum, RoleEnum, TeamClassificationEnum},
            player::PlayerEntry,
            team::TeamEntry,
        },
        team_form::TeamForm,
    };
    use chrono::{NaiveDate, Utc};
    use uuid::Uuid;

    struct P {
        db: Database,
        teams: SqliteTeamReader,
        team_writer: SqliteTeamWriter,
        matches: SqliteMatchReader,
        match_writer: SqliteMatchWriter,
        persons: SqlitePersons,
    }

    fn providers() -> P {
        let db = Database::new(crate::db::open_in_memory().unwrap());
        P {
            teams: SqliteTeamReader::new(db.clone()),
            team_writer: SqliteTeamWriter::new(db.clone()),
            matches: SqliteMatchReader::new(db.clone(), Language::default()),
            match_writer: SqliteMatchWriter::new(db.clone()),
            persons: SqlitePersons::new(db.clone()),
            db,
        }
    }

    fn player(name: &str, number: u8, role: RoleEnum) -> PlayerEntry {
        PlayerEntry {
            id: Uuid::new_v4(),
            name: name.into(),
            role: Some(role),
            number,
            deleted: false,
            birth_date: NaiveDate::from_ymd_opt(2008, 3, 14),
            height_cm: Some(180),
            notes: format!("{name} notes"),
        }
    }

    async fn team(p: &P, name: &str, year: u16, players: Vec<PlayerEntry>) -> TeamEntry {
        p.team_writer
            .save(TeamInput::New {
                id: None,
                name: name.into(),
                year,
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Women),
                country: CountryCode::known("IT"),
                players,
            })
            .await
            .unwrap()
    }

    async fn read(p: &P, team: &TeamEntry) -> TeamEntry {
        p.teams.read_single(&team.id).await.unwrap()
    }

    fn count(db: &Database, sql: &str) -> i64 {
        db.read(|tx| tx.query_row(sql, [], |r| r.get(0)).map_err(sqlite))
            .unwrap()
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 13).unwrap()
    }

    /// The form of a person chosen in the registry, as the apps fill it.
    fn chosen(person: &PlayerEntry, number: &str) -> PlayerForm {
        PlayerForm {
            number: number.into(),
            person: Some(person.id),
            ..PlayerForm::of(person)
        }
    }

    async fn add(p: &P, team: &TeamEntry, form: PlayerForm) -> PlayerEntry {
        let mut team = read(p, team).await;
        let existing = form.person.and_then(|id| team.find_player(id).cloned());
        let input = form.validate(&team, existing.as_ref(), today()).unwrap();
        p.team_writer.save_player(input, &mut team).await.unwrap()
    }

    /// The persons offered to a team are those of the registry not playing
    /// in it, by name, with the teams they play in, the most recent first:
    /// also one removed from it, or whose only team was removed; never one
    /// removed from the registry.
    #[tokio::test]
    async fn the_persons_not_in_the_team_are_offered() {
        let p = providers();
        let anna = player("anna Rossi", 9, RoleEnum::Setter);
        let bea = player("Bea Verdi", 4, RoleEnum::Libero);
        let carla = player("Carla Neri", 7, RoleEnum::OutsideHitter);
        let dora = player("Dora Blu", 1, RoleEnum::MiddleBlocker);
        let elsa = player("Elsa Gialli", 2, RoleEnum::OppositeHitter);
        let senior = team(&p, "Cortona", 2025, vec![anna.clone(), bea.clone()]).await;
        let under = team(&p, "Cortona U19", 2026, vec![carla.clone()]).await;
        let gone = team(&p, "Arezzo", 2024, vec![dora.clone(), elsa.clone()]).await;
        // Anna in two teams, Carla removed from hers, Arezzo removed, Elsa
        // removed from the registry
        add(&p, &under, chosen(&anna, "10")).await;
        let mut u = read(&p, &under).await;
        p.team_writer.remove_player(carla.id, &mut u).await.unwrap();
        p.team_writer.remove(&gone.id).await.unwrap();
        p.persons.remove(&elsa.id).unwrap();

        let offered = p.persons.choices(&under.id).unwrap();
        let names: Vec<&str> = offered.iter().map(|c| c.person.name.as_str()).collect();
        assert_eq!(names, ["Bea Verdi", "Carla Neri", "Dora Blu"]);
        let bea_offered = &offered[0];
        assert_eq!(bea_offered.person.birth_date, bea.birth_date);
        assert_eq!(bea_offered.person.height_cm, Some(180));
        assert_eq!(bea_offered.person.notes, "Bea Verdi notes");
        assert_eq!(
            bea_offered.teams,
            [PersonTeam {
                team: senior.id,
                name: "Cortona".into(),
                year: 2025,
                number: 4,
                role: Some(RoleEnum::Libero),
            }]
        );
        // in no team any more: still found
        assert!(offered[1].teams.is_empty());
        assert!(offered[2].teams.is_empty());

        let to_senior = p.persons.choices(&senior.id).unwrap();
        let names: Vec<&str> = to_senior.iter().map(|c| c.person.name.as_str()).collect();
        assert_eq!(names, ["Carla Neri", "Dora Blu"]);
        // Anna's teams: the most recent first
        let teams: Vec<(String, u16, u8)> = p
            .persons
            .teams_of(&anna.id)
            .unwrap()
            .into_iter()
            .map(|t| (t.name, t.year, t.number))
            .collect();
        assert_eq!(
            teams,
            [
                ("Cortona U19".into(), 2026, 10),
                ("Cortona".into(), 2025, 9)
            ]
        );
        assert!(p.persons.teams_of(&elsa.id).unwrap().is_empty());
        assert!(p.persons.teams_of(&dora.id).unwrap().is_empty());
    }

    /// A person added to one more team is the same person (D5): one row,
    /// two memberships, each with its number and role; their details
    /// edited in one team are theirs in the other too.
    #[tokio::test]
    async fn a_person_added_to_another_team_is_the_same_person() {
        let p = providers();
        let anna = player("Anna Rossi", 9, RoleEnum::Setter);
        let senior = team(&p, "Cortona", 2025, vec![anna.clone()]).await;
        let under = team(
            &p,
            "Cortona U19",
            2026,
            vec![player("Bea Verdi", 4, RoleEnum::Libero)],
        )
        .await;
        let persons = count(&p.db, "SELECT count(*) FROM persons");

        let added = add(
            &p,
            &under,
            PlayerForm {
                role: Some(RoleEnum::OppositeHitter),
                ..chosen(&anna, "12")
            },
        )
        .await;
        assert_eq!(added.id, anna.id);
        assert_eq!(count(&p.db, "SELECT count(*) FROM persons"), persons);
        let u = read(&p, &under).await;
        let there = u.find_player(anna.id).unwrap();
        assert_eq!(
            (there.number, there.role, there.deleted),
            (12, Some(RoleEnum::OppositeHitter), false)
        );
        // after the players already there
        assert_eq!(u.players.last().map(|p| p.id), Some(anna.id));
        let s = read(&p, &senior).await;
        let here = s.find_player(anna.id).unwrap();
        assert_eq!((here.number, here.role), (9, Some(RoleEnum::Setter)));

        // edited in the U19: the same details in the senior team
        let mut u = read(&p, &under).await;
        let input = PlayerForm {
            name: "Anna Rossi Bianchi".into(),
            height_cm: "182".into(),
            ..PlayerForm::of(there)
        }
        .validate(&u, Some(there), today())
        .unwrap();
        p.team_writer.save_player(input, &mut u).await.unwrap();
        let here = read(&p, &senior).await;
        let here = here.find_player(anna.id).unwrap();
        assert_eq!(
            (here.name.as_str(), here.height_cm, here.number),
            ("Anna Rossi Bianchi", Some(182), 9)
        );
    }

    /// A player removed from the team, chosen again, comes back: the same
    /// membership, in its place, with the number given now.
    #[tokio::test]
    async fn a_player_removed_from_the_team_comes_back() {
        let p = providers();
        let players: Vec<PlayerEntry> = (1..=7)
            .map(|n| player(&format!("P{n}"), n, RoleEnum::OutsideHitter))
            .collect();
        let t = team(&p, "Cortona", 2026, players.clone()).await;
        // told of by a match: kept in the team, removed
        p.match_writer
            .create(
                &t,
                MatchInput::against(
                    "Arezzo",
                    Utc::now().fixed_offset(),
                    true,
                    DEFAULT_MAX_SUBSTITUTIONS,
                    "",
                    vec![],
                ),
            )
            .await
            .unwrap();
        let mut read_back = read(&p, &t).await;
        for gone in [players[1].id, players[2].id] {
            p.team_writer
                .remove_player(gone, &mut read_back)
                .await
                .unwrap();
        }
        let removed = read(&p, &t).await;
        assert!(
            removed
                .find_player(players[1].id)
                .is_some_and(|p| p.deleted)
        );
        let offered: Vec<Uuid> = p
            .persons
            .choices(&t.id)
            .unwrap()
            .iter()
            .map(|c| c.person.id)
            .collect();
        assert!(offered.contains(&players[1].id));

        // the number of a player of the team: not valid
        let taken = chosen(&players[1], "1")
            .validate(&removed, removed.find_player(players[1].id), today())
            .err();
        assert_eq!(
            taken,
            Some(crate::player_form::PlayerFormError::NumberInUse)
        );
        add(&p, &t, chosen(&players[1], "22")).await;
        add(&p, &t, chosen(&players[2], "3")).await;
        let back = read(&p, &t).await;
        let ids: Vec<Uuid> = back.players.iter().map(|p| p.id).collect();
        assert_eq!(ids, players.iter().map(|p| p.id).collect::<Vec<_>>());
        let numbers: Vec<(u8, bool)> = back.players.iter().map(|p| (p.number, p.deleted)).collect();
        assert_eq!(numbers[1], (22, false));
        assert_eq!(numbers[2], (3, false));
        // the details come back from the form (a removed one shows none)
        assert_eq!(
            back.find_player(players[1].id).and_then(|p| p.height_cm),
            Some(180)
        );
        assert_eq!(
            count(&p.db, "SELECT count(*) FROM team_members WHERE deleted = 1"),
            0
        );
    }

    /// The person chosen isn't a namesake of themselves; another player
    /// of the team with their name is.
    #[test]
    fn the_person_chosen_isnt_their_own_namesake() {
        let anna = player("Anna Rossi", 9, RoleEnum::Setter);
        let other = player("Anna Rossi", 3, RoleEnum::Libero);
        let removed = crate::player_form::removed(&anna);
        let team = TeamEntry {
            players: vec![removed.clone()],
            ..TeamEntry::default()
        };
        let form = chosen(&anna, "9");
        assert!(crate::player_form::namesake(&form, &team, Some(&removed)).is_none());
        // chosen while in another team only: another player with the name
        let team = TeamEntry {
            players: vec![other.clone()],
            ..TeamEntry::default()
        };
        assert_eq!(
            crate::player_form::namesake(&form, &team, None).map(|p| p.id),
            Some(other.id)
        );
        let team = TeamEntry {
            players: vec![anna.clone()],
            ..TeamEntry::default()
        };
        assert!(crate::player_form::namesake(&form, &team, None).is_none());
        // a new player with no person: a new person
        let input = PlayerForm {
            person: None,
            ..chosen(&anna, "1")
        }
        .validate(&TeamEntry::default(), None, today())
        .unwrap();
        assert!(matches!(input, PlayerInput::New { .. }));
        // with a person: that person, active
        let input = chosen(&anna, "1")
            .validate(&TeamEntry::default(), None, today())
            .unwrap();
        assert!(matches!(
            input,
            PlayerInput::Existing(PlayerEntry { id, deleted: false, number: 1, .. }) if id == anna.id
        ));
    }

    fn member(player: &PlayerEntry, chosen: bool) -> SeasonMember {
        SeasonMember {
            player: player.id,
            number: player.number,
            role: player.role,
            chosen,
        }
    }

    /// The new season starts from the team, the year after, with its active
    /// players all chosen, by number.
    #[tokio::test]
    async fn the_new_season_offers_the_active_players() {
        let p = providers();
        let players = vec![
            player("Anna", 9, RoleEnum::Setter),
            player("Bea", 4, RoleEnum::Libero),
            player("Carla", 7, RoleEnum::OutsideHitter),
        ];
        let t = team(&p, "Cortona", 2025, players.clone()).await;
        let mut r = read(&p, &t).await;
        p.team_writer
            .remove_player(players[2].id, &mut r)
            .await
            .unwrap();
        let t = read(&p, &t).await;
        assert_eq!(
            season::team_form(&t),
            TeamForm {
                year: "2026".into(),
                ..TeamForm::of(&t)
            }
        );
        assert_eq!(
            season::members(&t),
            [member(&players[1], true), member(&players[0], true)]
        );
    }

    /// A number is corrected only from 0 to 99 and to one no other player
    /// chosen has; the players by number again.
    #[test]
    fn a_number_of_the_new_season_is_corrected() {
        let a = player("Anna", 9, RoleEnum::Setter);
        let b = player("Bea", 4, RoleEnum::Libero);
        let c = player("Carla", 7, RoleEnum::OutsideHitter);
        let members = vec![member(&b, true), member(&c, false), member(&a, true)];
        assert_eq!(
            season::correct(&members, a.id, "100", None),
            Err(SeasonError::InvalidNumber)
        );
        assert_eq!(
            season::correct(&members, a.id, "4", None),
            Err(SeasonError::NumberInUse)
        );
        // the number of one not chosen is free, and their own too
        let corrected =
            season::correct(&members, a.id, "7", Some(RoleEnum::OppositeHitter)).unwrap();
        assert_eq!(
            corrected
                .iter()
                .map(|m| (m.number, m.player))
                .collect::<Vec<_>>(),
            [(4, b.id), (7, c.id), (7, a.id)]
        );
        assert_eq!(corrected[2].role, Some(RoleEnum::OppositeHitter));
        assert!(season::correct(&members, a.id, " 9 ", Some(RoleEnum::Setter)).is_ok());
        assert_eq!(
            season::correct(&members, a.id, "", None),
            Err(SeasonError::InvalidNumber)
        );
    }

    /// The new team: the year after, the players chosen with the numbers
    /// and roles given, the same persons with their details; the old team
    /// as it was. Two chosen with one number aren't valid.
    #[tokio::test]
    async fn the_new_season_is_a_new_team_with_the_same_persons() {
        let p = providers();
        let players = vec![
            player("Anna", 9, RoleEnum::Setter),
            player("Bea", 4, RoleEnum::Libero),
            player("Carla", 7, RoleEnum::OutsideHitter),
        ];
        let old = team(&p, "Cortona", 2025, players.clone()).await;
        let persons = count(&p.db, "SELECT count(*) FROM persons");
        let members = vec![
            member(&players[1], true),
            member(&players[2], false),
            member(&players[0], true),
        ];
        let members = season::correct(
            &members,
            players[0].id,
            "10",
            Some(RoleEnum::OppositeHitter),
        )
        .unwrap();
        let input = || season::team_form(&old).validate(None).unwrap();

        // Carla taken back with Anna's number
        let doubled: Vec<SeasonMember> = members
            .iter()
            .map(|m| match m.player == players[2].id {
                true => SeasonMember {
                    chosen: true,
                    number: 10,
                    ..m.clone()
                },
                false => m.clone(),
            })
            .collect();
        assert!(matches!(
            season::new_team(&old, input(), &doubled),
            Err(SeasonError::NumberInUse)
        ));

        let input = season::new_team(&old, input(), &members).unwrap();
        let new = p.team_writer.save(input).await.unwrap();
        assert_ne!(new.id, old.id);
        let new = read(&p, &new).await;
        assert_eq!((new.name.as_str(), new.year), ("Cortona", 2026));
        let got: Vec<(Uuid, u8, Option<RoleEnum>)> = new
            .players
            .iter()
            .map(|p| (p.id, p.number, p.role))
            .collect();
        assert_eq!(
            got,
            [
                (players[1].id, 4, Some(RoleEnum::Libero)),
                (players[0].id, 10, Some(RoleEnum::OppositeHitter)),
            ]
        );
        assert_eq!(new.players[1].height_cm, Some(180));
        assert_eq!(new.players[1].notes, "Anna notes");
        assert_eq!(count(&p.db, "SELECT count(*) FROM persons"), persons);
        // the old team as it was
        assert_eq!(read(&p, &old).await, old);
        let teams: Vec<u16> = p
            .persons
            .teams_of(&players[0].id)
            .unwrap()
            .iter()
            .map(|t| t.year)
            .collect();
        assert_eq!(teams, [2026, 2025]);
    }

    /// A person removed from the registry reads as a removed player in the
    /// matches played, with their number; a name is left as it is.
    #[tokio::test]
    async fn a_person_removed_reads_as_a_removed_player() {
        let p = providers();
        let players: Vec<PlayerEntry> = (1..=7)
            .map(|n| player(&format!("P{n}"), n, RoleEnum::OutsideHitter))
            .collect();
        let t = team(&p, "Cortona", 2026, players.clone()).await;
        let m = p
            .match_writer
            .create(
                &t,
                MatchInput::against(
                    "Arezzo",
                    Utc::now().fixed_offset(),
                    true,
                    DEFAULT_MAX_SUBSTITUTIONS,
                    "",
                    vec![],
                ),
            )
            .await
            .unwrap();
        p.persons.remove(&players[6].id).unwrap();
        let t = read(&p, &t).await;
        let mut seen = p.matches.read_single(&t, &m.id).await.unwrap();
        let labels = labels(LanguageEnum::It);
        name_the_removed(&mut seen.team, labels);
        let gone = seen.team.find_player(players[6].id).unwrap();
        assert_eq!((gone.name.as_str(), gone.number), ("giocatore rimosso", 7));
        assert_eq!(
            seen.team
                .find_player(players[0].id)
                .map(|p| p.name.as_str()),
            Some("P1")
        );
    }
}
