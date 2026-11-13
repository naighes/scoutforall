//! The competitions in their list (TODO/001, step 23): counted with their
//! matches, edited through their form, removed leaving their matches.

#[cfg(test)]
mod tests {
    use crate::{
        competition_form::{CompetitionForm, CompetitionFormError},
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        domain::{Competition, CompetitionFormat, Stage},
        match_form::{CompetitionInput, MatchForm, MatchInput},
        providers::{
            match_writer::MatchWriter,
            sqlite::{competitions::SqliteCompetitions, rows::match_rows},
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, TeamClassificationEnum},
            r#match::MatchEntry,
            team::TeamEntry,
        },
        tests::Sqlite,
    };
    use chrono::DateTime;
    use uuid::Uuid;

    async fn team(p: &Sqlite) -> TeamEntry {
        p.team_writer
            .save(TeamInput::New {
                id: None,
                name: "Cortona Volley".into(),
                year: 2026,
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Men),
                country: CountryCode::known("IT"),
                players: vec![],
            })
            .await
            .unwrap()
    }

    /// A match against `opponent` in the competition.
    async fn play(
        p: &Sqlite,
        team: &TeamEntry,
        opponent: &str,
        competition: CompetitionInput,
    ) -> MatchEntry {
        let date = DateTime::parse_from_rfc3339("2026-10-18T18:00:00+02:00").unwrap_or_default();
        p.match_writer
            .create(
                team,
                MatchInput {
                    competition: Some(competition),
                    stage: Some(Stage::Regular),
                    round: Some("Andata – giornata 1".into()),
                    ..MatchInput::against(opponent, date, true, DEFAULT_MAX_SUBSTITUTIONS, "", vec![])
                },
            )
            .await
            .unwrap()
    }

    fn new(name: &str) -> CompetitionInput {
        CompetitionInput::New {
            name: name.into(),
            format: CompetitionFormat::League,
        }
    }

    fn competitions(p: &Sqlite) -> SqliteCompetitions {
        SqliteCompetitions::new(p.db.clone())
    }

    fn named(p: &Sqlite, name: &str) -> Competition {
        competitions(p)
            .read_all()
            .unwrap()
            .into_iter()
            .find(|c| c.name == name)
            .unwrap()
    }

    fn row(p: &Sqlite, m: &MatchEntry) -> crate::domain::Match {
        let id = Uuid::parse_str(&m.id).unwrap();
        p.db.read(|tx| match_rows(tx, id)).unwrap().unwrap().row
    }

    /// Each competition with its matches not removed; one without matches
    /// too.
    #[tokio::test]
    async fn a_competition_is_listed_with_its_matches() {
        let p = Sqlite::new();
        let a = team(&p).await;
        play(&p, &a, "Cascina", new("Serie C")).await;
        let serie_c = named(&p, "Serie C");
        let second = play(&p, &a, "Arezzo", CompetitionInput::Existing(serie_c.id)).await;
        play(&p, &a, "Pisa", new("Coppa Toscana")).await;
        play(&p, &a, "Siena", new("Torneo di Natale")).await;
        let counted = |p: &Sqlite| {
            competitions(p)
                .read_counted()
                .unwrap()
                .into_iter()
                .map(|(c, n)| (c.name, n))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            counted(&p),
            vec![
                ("Coppa Toscana".to_string(), 1),
                ("Serie C".to_string(), 2),
                ("Torneo di Natale".to_string(), 1),
            ]
        );
        p.match_writer.remove(&a.id, &second.id).await.unwrap();
        assert_eq!(counted(&p).get(1), Some(&("Serie C".to_string(), 1)));
    }

    /// A new competition with the name of one of the season of team A (in
    /// any case) is that one, not a twin; in another season it's new.
    #[tokio::test]
    async fn a_new_competition_named_as_one_there_is_that_one() {
        let p = Sqlite::new();
        let a = team(&p).await;
        play(&p, &a, "Cascina", new("Serie C")).await;
        play(&p, &a, "Arezzo", new(" serie c ")).await;
        let counted = competitions(&p).read_counted().unwrap();
        assert_eq!(
            counted
                .iter()
                .map(|(c, n)| (c.name.as_str(), *n))
                .collect::<Vec<_>>(),
            vec![("Serie C", 2)]
        );
        let next = p
            .team_writer
            .save(TeamInput::New {
                id: None,
                name: "Cortona Volley".into(),
                year: 2027,
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Men),
                country: CountryCode::known("IT"),
                players: vec![],
            })
            .await
            .unwrap();
        play(&p, &next, "Pisa", new("Serie C")).await;
        let seasons: Vec<_> = competitions(&p)
            .read_all()
            .unwrap()
            .into_iter()
            .map(|c| c.season)
            .collect();
        assert_eq!(
            seasons,
            vec![Some("2027/2028".to_string()), Some("2026/2027".to_string())]
        );
    }

    /// The form of a competition: trimmed, an empty season none, written
    /// as edited; its matches keep their own friendly and show the new
    /// name.
    #[tokio::test]
    async fn a_competition_is_edited() {
        let p = Sqlite::new();
        let a = team(&p).await;
        let m = play(&p, &a, "Cascina", new("Serie C")).await;
        let before = named(&p, "Serie C");
        assert_eq!(before.season.as_deref(), Some("2026/2027"));
        let form = CompetitionForm {
            name: "  Serie C maschile – girone A ".into(),
            format: CompetitionFormat::Cup,
            season: "  ".into(),
            gender: Some(GenderEnum::Women),
            country: CountryCode::known("PL"),
            friendly: true,
        };
        let all = competitions(&p).read_all().unwrap();
        let edited = form.validate(&before, &all).unwrap();
        competitions(&p).update(&edited).unwrap();
        let read = competitions(&p).read(before.id).unwrap().unwrap();
        assert_eq!(
            read,
            Competition {
                id: before.id,
                name: "Serie C maschile – girone A".into(),
                format: CompetitionFormat::Cup,
                season: None,
                gender: Some(GenderEnum::Women),
                country: CountryCode::known("PL"),
                friendly: true,
                deleted: false,
            }
        );
        assert_eq!(CompetitionForm::of(&read).season, "");
        // the match keeps what it was saved with
        assert!(!row(&p, &m).friendly);
        let (match_row, competition) = p.match_reader.details(&a, &m.id).unwrap();
        let form = MatchForm::of(&m, &match_row, competition.as_ref());
        assert_eq!(
            (form.competition.as_str(), form.competition_id, form.friendly),
            ("Serie C maschile – girone A", Some(before.id), false)
        );
    }

    /// No name, or the name and the season of another competition (in
    /// any case, with spaces around): not valid; the same name in another
    /// season, or its own, is.
    #[tokio::test]
    async fn the_form_refuses_an_empty_name_or_one_taken() {
        let p = Sqlite::new();
        let a = team(&p).await;
        play(&p, &a, "Cascina", new("Serie C")).await;
        play(&p, &a, "Arezzo", new("Serie D")).await;
        let serie_c = named(&p, "Serie C");
        let serie_d = named(&p, "Serie D");
        let all = competitions(&p).read_all().unwrap();
        let validate = |c: &Competition, name: &str, season: &str| {
            CompetitionForm {
                name: name.into(),
                season: season.into(),
                ..CompetitionForm::of(c)
            }
            .validate(c, &all)
            .map(|c| (c.name, c.season))
        };
        assert_eq!(
            validate(&serie_d, "  ", "2026/2027"),
            Err(CompetitionFormError::EmptyName)
        );
        assert_eq!(
            validate(&serie_d, " serie c ", " 2026/2027 "),
            Err(CompetitionFormError::AlreadyThere)
        );
        assert_eq!(
            validate(&serie_d, "Serie C", "2025/2026"),
            Ok(("Serie C".to_string(), Some("2025/2026".to_string())))
        );
        assert_eq!(
            validate(&serie_c, "SERIE C", "2026/2027"),
            Ok(("SERIE C".to_string(), Some("2026/2027".to_string())))
        );
        // a removed one doesn't count
        competitions(&p).remove(serie_c.id).unwrap();
        let all = competitions(&p).read_all().unwrap();
        let removed_too = [all, vec![Competition { deleted: true, ..serie_c }]].concat();
        assert!(
            CompetitionForm {
                name: "Serie C".into(),
                ..CompetitionForm::of(&serie_d)
            }
            .validate(&serie_d, &removed_too)
            .is_ok()
        );
    }

    /// Removed: out of the list, only its flag set; its matches stay,
    /// without a competition, their stage and round kept, saved again as
    /// they are. Not removed twice nor edited once removed.
    #[tokio::test]
    async fn a_removed_competition_leaves_its_matches() {
        let p = Sqlite::new();
        let a = team(&p).await;
        let m = play(&p, &a, "Cascina", new("Serie C")).await;
        let other = play(&p, &a, "Arezzo", new("Serie D")).await;
        let serie_c = named(&p, "Serie C");
        competitions(&p).remove(serie_c.id).unwrap();

        let names: Vec<_> = competitions(&p)
            .read_all()
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, vec!["Serie D".to_string()]);
        assert_eq!(
            competitions(&p).read(serie_c.id).unwrap(),
            Some(Competition {
                deleted: true,
                ..serie_c.clone()
            })
        );
        let left = row(&p, &m);
        assert_eq!(left.competition_id, None);
        assert_eq!(
            (left.stage, left.round.as_deref()),
            (Some(Stage::Regular), Some("Andata – giornata 1"))
        );
        // the other match keeps its own
        assert!(row(&p, &other).competition_id.is_some());

        let (match_row, competition) = p.match_reader.details(&a, &m.id).unwrap();
        assert_eq!(competition, None);
        let form = MatchForm::of(&m, &match_row, competition.as_ref());
        assert_eq!((form.competition.as_str(), form.competition_id), ("", None));
        let input = form.validate(&a, Some(&m)).unwrap();
        p.match_writer.update(&m, input).await.unwrap();

        assert!(competitions(&p).remove(serie_c.id).is_err());
        assert!(competitions(&p).update(&serie_c).is_err());
        assert_eq!(competitions(&p).read(serie_c.id).unwrap().map(|c| c.name), Some("Serie C".into()));
    }
}
