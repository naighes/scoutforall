//! The form of a match on the database (TODO/001, step 19): team B a team,
//! a new one or a name, home, away or neutral, friendly, the competition
//! chosen or new, its stage and round; saved and read back for the form.

#[cfg(test)]
mod tests {
    use crate::{
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        domain::{CompetitionFormat, Home, ScoutingMode, Stage, TeamRef},
        match_form::{CompetitionInput, MatchForm, MatchFormError, MatchInput, Opponent},
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            sqlite::{Database, competitions::SqliteCompetitions, rows::match_rows},
            team_reader::TeamReader,
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, RoleEnum, TeamClassificationEnum},
            r#match::MatchEntry,
            player::PlayerEntry,
            team::TeamEntry,
        },
        tests::Sqlite,
    };
    use chrono::{DateTime, FixedOffset};
    use uuid::Uuid;

    fn date() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-18T18:00:00+02:00").unwrap_or_default()
    }

    fn players() -> Vec<PlayerEntry> {
        (1..=7)
            .map(|n| PlayerEntry {
                id: Uuid::new_v4(),
                name: format!("player {n}"),
                number: n,
                role: Some(match n {
                    7 => RoleEnum::Libero,
                    1 => RoleEnum::Setter,
                    _ => RoleEnum::OutsideHitter,
                }),
                ..PlayerEntry::default()
            })
            .collect()
    }

    async fn team(p: &Sqlite, name: &str, players: Vec<PlayerEntry>) -> TeamEntry {
        p.team_writer
            .save(TeamInput::New {
                id: None,
                name: name.into(),
                year: 2026,
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Men),
                country: CountryCode::known("IT"),
                players,
            })
            .await
            .unwrap()
    }

    /// Cortona with its players, Cascina without.
    async fn two_teams(p: &Sqlite) -> (TeamEntry, TeamEntry) {
        (
            team(p, "Cortona Volley", players()).await,
            team(p, "Cascina 2", vec![]).await,
        )
    }

    fn input(opponent: Opponent) -> MatchInput {
        MatchInput {
            opponent,
            ..MatchInput::against("", date(), true, DEFAULT_MAX_SUBSTITUTIONS, "", vec![])
        }
    }

    fn count(db: &Database, sql: &str) -> i64 {
        db.read(|tx| {
            tx.query_row(sql, [], |row| row.get(0))
                .map_err(crate::providers::sqlite::sqlite)
        })
        .unwrap()
    }

    fn row(db: &Database, m: &MatchEntry) -> crate::domain::Match {
        let id = Uuid::parse_str(&m.id).unwrap();
        db.read(|tx| match_rows(tx, id)).unwrap().unwrap().row
    }

    /// The form of the match of `team` as the apps read it.
    fn form_of(p: &Sqlite, team: &TeamEntry, m: &MatchEntry) -> MatchForm {
        let (row, competition) = p.match_reader.details(team, &m.id).unwrap();
        MatchForm::of(m, &row, competition.as_ref())
    }

    #[tokio::test]
    async fn team_b_can_be_a_team() {
        let p = Sqlite::new();
        let (a, b) = two_teams(&p).await;
        let m = p
            .match_writer
            .create(&a, input(Opponent::Team(b.id)))
            .await
            .unwrap();
        assert_eq!(m.opponent, "Cascina 2");
        assert_eq!(row(&p.db, &m).team_b, TeamRef::Team(b.id));
        let read = p.match_reader.read_single(&a, &m.id).await.unwrap();
        assert_eq!(read.opponent, "Cascina 2");
        let form = form_of(&p, &a, &read);
        assert_eq!(
            (form.opponent.as_str(), form.opponent_team),
            ("Cascina 2", Some(b.id))
        );
        assert!(!form.create_opponent);
    }

    /// A new team B has no players, and A's year, classification, gender
    /// and country.
    #[tokio::test]
    async fn team_b_can_be_created_from_the_form() {
        let p = Sqlite::new();
        let (a, _) = two_teams(&p).await;
        let m = p
            .match_writer
            .create(&a, input(Opponent::NewTeam("Volley Pisa".into())))
            .await
            .unwrap();
        let TeamRef::Team(id) = row(&p.db, &m).team_b else {
            unreachable!("team B should be a team");
        };
        let b = p.team_reader.read_single(&id).await.unwrap();
        assert_eq!(
            (
                b.name.as_str(),
                b.year,
                b.classification,
                b.gender,
                b.country
            ),
            ("Volley Pisa", a.year, a.classification, a.gender, a.country)
        );
        assert!(b.players.is_empty());
        assert_eq!(m.opponent, "Volley Pisa");
        assert_eq!(p.team_reader.read_all().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn team_b_can_be_only_a_name() {
        let p = Sqlite::new();
        let (a, _) = two_teams(&p).await;
        let m = p
            .match_writer
            .create(&a, input(Opponent::Name("Volley Pisa".into())))
            .await
            .unwrap();
        assert_eq!(row(&p.db, &m).team_b, TeamRef::Name("Volley Pisa".into()));
        assert_eq!(p.team_reader.read_all().await.unwrap().len(), 2);
        assert_eq!(form_of(&p, &a, &m).opponent_team, None);
    }

    /// A removed team, or a team that isn't there, can't be B: nothing is
    /// saved.
    #[tokio::test]
    async fn team_b_is_a_team_not_removed() {
        let p = Sqlite::new();
        let (a, b) = two_teams(&p).await;
        p.db.write(|tx| {
            tx.execute("UPDATE teams SET deleted = 1 WHERE name = 'Cascina 2'", [])
                .map_err(crate::providers::sqlite::sqlite)
        })
        .unwrap();
        for missing in [b.id, Uuid::new_v4(), a.id] {
            assert!(
                p.match_writer
                    .create(&a, input(Opponent::Team(missing)))
                    .await
                    .is_err()
            );
        }
        assert_eq!(count(&p.db, "SELECT count(*) FROM matches"), 0);
    }

    #[test]
    fn the_form_refuses_team_a_as_team_b() {
        let a = TeamEntry {
            id: Uuid::new_v4(),
            players: players(),
            ..TeamEntry::default()
        };
        let form = MatchForm {
            opponent: "Cortona Volley".into(),
            opponent_team: Some(a.id),
            ..MatchForm::new(date())
        };
        assert_eq!(
            form.validate(&a, None),
            Err(MatchFormError::OpponentIsTeamA)
        );
    }

    #[tokio::test]
    async fn the_court_is_home_away_or_neutral() {
        let p = Sqlite::new();
        let (a, _) = two_teams(&p).await;
        for (home, neutral, saved, column) in [
            (true, false, Home::A, "a"),
            (false, false, Home::B, "b"),
            (true, true, Home::Neutral, "neutral"),
            (false, true, Home::Neutral, "neutral"),
        ] {
            let form = MatchForm {
                opponent: "Volley Pisa".into(),
                home,
                neutral,
                ..MatchForm::new(date())
            };
            let valid = form.validate(&a, None).unwrap();
            assert_eq!(valid.home, saved);
            let m = p.match_writer.create(&a, valid).await.unwrap();
            assert_eq!(row(&p.db, &m).home, saved, "{column}");
            assert_eq!(m.home, saved == Home::A);
            let back = form_of(&p, &a, &m);
            assert_eq!(
                (back.home, back.neutral),
                (saved == Home::A, saved == Home::Neutral)
            );
        }
        assert_eq!(
            count(&p.db, "SELECT count(*) FROM matches WHERE home IS NULL"),
            2
        );
    }

    /// A new competition is a league unless told, of the season of A, its
    /// gender and country; chosen again, it's the same one.
    #[tokio::test]
    async fn a_competition_is_created_from_the_form_and_chosen_again() {
        let p = Sqlite::new();
        let (a, b) = two_teams(&p).await;
        let form = MatchForm {
            opponent: "Cascina 2".into(),
            opponent_team: Some(b.id),
            competition: "  Serie C Maschile ".into(),
            stage: Some(Stage::Regular),
            round: " Andata – giornata 1 ".into(),
            ..MatchForm::new(date())
        };
        let first = p
            .match_writer
            .create(&a, form.clone().validate(&a, None).unwrap())
            .await
            .unwrap();
        let competitions = SqliteCompetitions::new(p.db.clone()).read_all().unwrap();
        let [c] = competitions.as_slice() else {
            unreachable!("one competition: {competitions:?}");
        };
        assert_eq!(
            (
                c.name.as_str(),
                c.format,
                c.season.as_deref(),
                c.gender,
                c.country
            ),
            (
                "Serie C Maschile",
                CompetitionFormat::League,
                Some("2026/2027"),
                a.gender,
                a.country
            )
        );
        let saved = row(&p.db, &first);
        assert_eq!(
            (saved.competition_id, saved.stage, saved.round.as_deref()),
            (
                Some(c.id),
                Some(Stage::Regular),
                Some("Andata – giornata 1")
            )
        );
        // chosen for the second match: still one
        let second = MatchForm {
            competition_id: Some(c.id),
            round: "Andata – giornata 2".into(),
            friendly: true,
            ..form
        };
        let second = p
            .match_writer
            .create(&a, second.validate(&a, None).unwrap())
            .await
            .unwrap();
        assert_eq!(count(&p.db, "SELECT count(*) FROM competitions"), 1);
        let back = form_of(&p, &a, &second);
        assert_eq!(
            (
                back.competition.as_str(),
                back.competition_id,
                back.friendly,
                back.stage,
                back.round.as_str()
            ),
            (
                "Serie C Maschile",
                Some(c.id),
                true,
                Some(Stage::Regular),
                "Andata – giornata 2"
            )
        );
    }

    #[tokio::test]
    async fn a_new_competition_has_the_format_chosen() {
        let p = Sqlite::new();
        let (a, _) = two_teams(&p).await;
        let m = p
            .match_writer
            .create(
                &a,
                MatchInput {
                    competition: Some(CompetitionInput::New {
                        name: "Torneo di Natale".into(),
                        format: CompetitionFormat::Tournament,
                    }),
                    ..input(Opponent::Name("Volley Pisa".into()))
                },
            )
            .await
            .unwrap();
        let c = form_of(&p, &a, &m);
        assert_eq!(c.competition_format, Some(CompetitionFormat::Tournament));
        assert_eq!(
            MatchForm {
                competition_id: None,
                competition: "  ".into(),
                ..c
            }
            .validate(&a, Some(&m))
            .unwrap()
            .competition,
            None
        );
    }

    /// The form of a saved match validates back to what it is: saved again,
    /// nothing changes.
    #[tokio::test]
    async fn the_form_of_a_saved_match_saves_it_as_it_is() {
        let p = Sqlite::new();
        let (a, b) = two_teams(&p).await;
        let form = MatchForm {
            opponent: "Cascina 2".into(),
            opponent_team: Some(b.id),
            neutral: true,
            friendly: true,
            competition: "Coppa Toscana".into(),
            competition_format: Some(CompetitionFormat::Cup),
            stage: Some(Stage::FinalRound),
            round: "Finale".into(),
            notes: "windy".into(),
            ..MatchForm::new(date())
        };
        let m = p
            .match_writer
            .create(&a, form.validate(&a, None).unwrap())
            .await
            .unwrap();
        let before = row(&p.db, &m);
        let again = form_of(&p, &a, &m).validate(&a, Some(&m)).unwrap();
        p.match_writer.update(&m, again).await.unwrap();
        assert_eq!(row(&p.db, &m), before);
        assert_eq!(count(&p.db, "SELECT count(*) FROM competitions"), 1);
        assert_eq!(count(&p.db, "SELECT count(*) FROM teams"), 2);
    }

    /// Edited, B goes from a name to a team and back, the competition away.
    #[tokio::test]
    async fn editing_changes_team_b_and_the_competition() {
        let p = Sqlite::new();
        let (a, b) = two_teams(&p).await;
        let m = p
            .match_writer
            .create(
                &a,
                MatchInput {
                    competition: Some(CompetitionInput::New {
                        name: "Serie C".into(),
                        format: CompetitionFormat::League,
                    }),
                    ..input(Opponent::Name("Cascina".into()))
                },
            )
            .await
            .unwrap();
        p.match_writer
            .update(&m, input(Opponent::Team(b.id)))
            .await
            .unwrap();
        let saved = row(&p.db, &m);
        assert_eq!(saved.team_b, TeamRef::Team(b.id));
        assert_eq!(saved.competition_id, None);
        let read = p.match_reader.read_single(&a, &m.id).await.unwrap();
        assert_eq!(read.opponent, "Cascina 2");
        p.match_writer
            .update(&m, input(Opponent::Name("Cascina".into())))
            .await
            .unwrap();
        assert_eq!(row(&p.db, &m).team_b, TeamRef::Name("Cascina".into()));
        // the competition stays, unused
        assert_eq!(count(&p.db, "SELECT count(*) FROM competitions"), 1);
    }

    /// Both teams scouted: B can't change (its lineups, its events).
    #[tokio::test]
    async fn team_b_of_a_match_scouted_on_both_sides_stays() {
        let p = Sqlite::new();
        let (a, b) = two_teams(&p).await;
        let c = team(&p, "Volley Pisa", vec![]).await;
        let m = p
            .match_writer
            .create(&a, input(Opponent::Team(b.id)))
            .await
            .unwrap();
        p.db.write(|tx| {
            tx.execute("UPDATE matches SET scouting = 'both'", [])
                .map_err(crate::providers::sqlite::sqlite)
        })
        .unwrap();
        assert_eq!(row(&p.db, &m).scouting, ScoutingMode::Both);
        for other in [
            Opponent::Team(c.id),
            Opponent::Name("Cascina 2".into()),
            Opponent::NewTeam("Cascina 3".into()),
        ] {
            assert!(p.match_writer.update(&m, input(other)).await.is_err());
        }
        assert_eq!(row(&p.db, &m).team_b, TeamRef::Team(b.id));
        assert_eq!(count(&p.db, "SELECT count(*) FROM teams"), 3);
        p.match_writer
            .update(
                &m,
                MatchInput {
                    notes: "same B".into(),
                    ..input(Opponent::Team(b.id))
                },
            )
            .await
            .unwrap();
        assert_eq!(row(&p.db, &m).notes, "same B");
    }

    /// An import over a match (`replace_details`) with only the name of B
    /// and no competition keeps team B a team, the competition, the stage,
    /// the round, friendly, the notes.
    #[tokio::test]
    async fn saved_again_as_before_the_database_keeps_what_the_files_lacked() {
        let p = Sqlite::new();
        let (a, b) = two_teams(&p).await;
        let m = p
            .match_writer
            .create(
                &a,
                MatchInput {
                    friendly: true,
                    competition: Some(CompetitionInput::New {
                        name: "Serie C".into(),
                        format: CompetitionFormat::League,
                    }),
                    stage: Some(Stage::Playoff),
                    round: Some("Semifinale".into()),
                    ..input(Opponent::Team(b.id))
                },
            )
            .await
            .unwrap();
        let before = row(&p.db, &m);
        p.match_writer
            .replace_details(
                &m,
                MatchInput::against(
                    "CASCINA",
                    m.date,
                    m.home,
                    m.max_substitutions,
                    "imported",
                    m.roster.clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(row(&p.db, &m), before);
    }
}
