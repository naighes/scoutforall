//! The filter of the statistics by competition (TODO/033, step 1): the
//! summary of a match carries its competition, the period chooses the
//! matches of one, and the competitions on offer are those of the matches
//! scouted.

#[cfg(test)]
mod tests {
    use crate::{
        analysis::{Period, Played, competitions_of},
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        domain::{CompetitionFormat, Stage},
        match_form::{CompetitionInput, MatchInput},
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, TeamClassificationEnum},
            r#match::{MatchEntry, MatchSummary},
            team::TeamEntry,
        },
        tests::Sqlite,
    };
    use chrono::{DateTime, NaiveDate};
    use uuid::Uuid;

    fn day(d: u32) -> DateTime<chrono::FixedOffset> {
        DateTime::parse_from_rfc3339(&format!("2026-10-{d:02}T18:00:00+02:00")).unwrap_or_default()
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 19).unwrap_or_default()
    }

    /// A summary of the 10th..28th of October; `sets` 0 is not scouted.
    fn summary(id: &str, d: u32, sets: u8, competition: Option<Uuid>) -> MatchSummary {
        MatchSummary {
            id: id.into(),
            opponent: id.into(),
            date: day(d),
            home: true,
            sets,
            us_wins: 0,
            them_wins: 0,
            finished: false,
            resume: None,
            next_set_number: None,
            competition_id: competition,
            opponent_team: None,
        }
    }

    fn ids(chosen: Vec<&MatchSummary>) -> Vec<&str> {
        chosen.into_iter().map(|m| m.id.as_str()).collect()
    }

    fn matches(league: Uuid, cup: Uuid) -> Vec<MatchSummary> {
        vec![
            summary("l1", 1, 3, Some(league)),
            summary("c1", 2, 3, Some(cup)),
            summary("f1", 3, 3, None),
            summary("l2", 4, 3, Some(league)),
            summary("c2", 5, 3, Some(cup)),
            summary("l3", 6, 3, Some(league)),
        ]
    }

    #[test]
    fn no_competition_chooses_every_match_as_before() {
        let all = matches(Uuid::new_v4(), Uuid::new_v4());
        for period in [
            Period::LastMonth,
            Period::LastMatches(4),
            Period::Between {
                from: NaiveDate::from_ymd_opt(2026, 10, 2).unwrap_or_default(),
                to: NaiveDate::from_ymd_opt(2026, 10, 5).unwrap_or_default(),
            },
        ] {
            assert_eq!(
                ids(period.select_in(&all, today(), None)),
                ids(period.select(&all, today())),
            );
        }
        assert_eq!(ids(Period::LastMatches(9).select(&all, today())).len(), 6);
    }

    #[test]
    fn a_competition_chooses_only_its_matches() {
        let (league, cup) = (Uuid::new_v4(), Uuid::new_v4());
        let all = matches(league, cup);
        let month = Period::Between {
            from: NaiveDate::from_ymd_opt(2026, 10, 1).unwrap_or_default(),
            to: NaiveDate::from_ymd_opt(2026, 10, 31).unwrap_or_default(),
        };
        assert_eq!(
            ids(month.select_in(&all, today(), Some(league))),
            ["l1", "l2", "l3"]
        );
        assert_eq!(ids(month.select_in(&all, today(), Some(cup))), ["c1", "c2"]);
        // a competition with no matches: none, not all
        assert!(
            month
                .select_in(&all, today(), Some(Uuid::new_v4()))
                .is_empty()
        );
    }

    /// The competition narrows the matches before the period counts them:
    /// the last two of the league, not the league ones among the last two.
    #[test]
    fn the_last_matches_are_the_last_of_the_competition() {
        let (league, cup) = (Uuid::new_v4(), Uuid::new_v4());
        let all = matches(league, cup);
        assert_eq!(
            ids(Period::LastMatches(2).select_in(&all, today(), Some(league))),
            ["l2", "l3"]
        );
        assert_eq!(
            ids(Period::LastMatches(2).select_in(&all, today(), Some(cup))),
            ["c1", "c2"]
        );
        assert_eq!(
            ids(Period::LastMatches(2).select_in(&all, today(), None)),
            ["c2", "l3"]
        );
    }

    #[test]
    fn the_days_of_the_period_follow_the_matches_of_the_competition() {
        let (league, cup) = (Uuid::new_v4(), Uuid::new_v4());
        let all = matches(league, cup);
        let chosen = Period::LastMatches(2).select_in(&all, today(), Some(cup));
        assert_eq!(
            Period::LastMatches(2).days(&chosen, today()),
            Some((
                NaiveDate::from_ymd_opt(2026, 10, 2).unwrap_or_default(),
                NaiveDate::from_ymd_opt(2026, 10, 5).unwrap_or_default()
            ))
        );
    }

    #[test]
    fn the_matches_not_scouted_or_to_come_stay_out_of_the_competition() {
        let league = Uuid::new_v4();
        let all = vec![
            summary("done", 1, 3, Some(league)),
            summary("empty", 2, 0, Some(league)),
            // after today
            MatchSummary {
                date: DateTime::parse_from_rfc3339("2026-12-01T18:00:00+01:00").unwrap_or_default(),
                ..summary("later", 3, 3, Some(league))
            },
        ];
        let year = Period::Between {
            from: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap_or_default(),
            to: NaiveDate::from_ymd_opt(2026, 12, 31).unwrap_or_default(),
        };
        assert_eq!(ids(year.select_in(&all, today(), Some(league))), ["done"]);
    }

    #[test]
    fn the_competitions_on_offer_are_those_of_the_matches_scouted() {
        let (league, cup, unplayed) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let mut all = matches(league, cup);
        // no sets, and to come: their competitions are not on offer
        all.push(summary("empty", 7, 0, Some(unplayed)));
        all.push(MatchSummary {
            date: DateTime::parse_from_rfc3339("2026-12-01T18:00:00+01:00").unwrap_or_default(),
            ..summary("later", 8, 3, Some(unplayed))
        });
        let offered = competitions_of(&all, today());
        assert_eq!(offered.len(), 2);
        assert!(offered.contains(&league) && offered.contains(&cup));
        // none at all
        assert!(competitions_of(&[summary("f", 1, 3, None)], today()).is_empty());
        assert!(competitions_of::<MatchSummary>(&[], today()).is_empty());
    }

    #[test]
    fn the_summary_seen_by_b_keeps_the_competition() {
        let league = Uuid::new_v4();
        let seen = summary("m", 1, 3, Some(league)).seen_by_b(Uuid::nil(), "A".into(), false);
        assert_eq!(seen.competition_id, Some(league));
        assert_eq!(seen.competition(), Some(league));
    }

    #[test]
    fn an_entry_and_a_reference_answer_as_the_summary_does() {
        let league = Uuid::new_v4();
        let s = summary("m", 1, 3, Some(league));
        let by_ref = &s;
        assert_eq!(Played::competition(&by_ref), Some(league));
        let entry = MatchEntry {
            source: Default::default(),
            video_clock: false,
            opponent: "x".into(),
            date: day(1),
            id: "x".into(),
            team: TeamEntry::default(),
            home: true,
            roster: vec![],
            max_substitutions: DEFAULT_MAX_SUBSTITUTIONS,
            sets: vec![],
            notes: String::new(),
        };
        // the entry doesn't carry it: the filter reads the summaries
        assert_eq!(entry.competition(), None);
    }

    #[test]
    fn a_summary_saved_without_the_competition_reads_as_none() {
        let s = summary("m", 1, 3, Some(Uuid::new_v4()));
        let mut json = serde_json::to_value(&s).unwrap();
        json.as_object_mut().unwrap().remove("competition_id");
        let read: MatchSummary = serde_json::from_value(json).unwrap();
        assert_eq!(read.competition_id, None);
    }

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

    fn input(opponent: &str, d: u32, competition: Option<CompetitionInput>) -> MatchInput {
        MatchInput {
            competition,
            stage: Some(Stage::Regular),
            ..MatchInput::against(
                opponent,
                day(d),
                true,
                DEFAULT_MAX_SUBSTITUTIONS,
                "",
                vec![],
            )
        }
    }

    fn new(name: &str) -> Option<CompetitionInput> {
        Some(CompetitionInput::New {
            name: name.into(),
            format: CompetitionFormat::League,
        })
    }

    async fn competition_of_each(p: &Sqlite, team: &TeamEntry) -> Vec<(String, Option<Uuid>)> {
        let mut found: Vec<_> = p
            .match_reader
            .read_summaries(team)
            .await
            .unwrap()
            .into_iter()
            .map(|l| l.unwrap())
            .map(|s| (s.opponent, s.competition_id))
            .collect();
        found.sort();
        found
    }

    /// The listing of the database carries the competition of each match:
    /// when the row is rebuilt, from the cache, and after the match moves
    /// to another competition (the cache keeps only the status).
    #[tokio::test]
    async fn the_listing_carries_the_competition_of_each_match() {
        let p = Sqlite::new();
        let a = team(&p).await;
        let first = p
            .match_writer
            .create(&a, input("Cascina", 4, new("Serie C")))
            .await
            .unwrap();
        let serie_c = competition_of_each(&p, &a).await[0].1.unwrap();
        p.match_writer
            .create(
                &a,
                input("Arezzo", 11, Some(CompetitionInput::Existing(serie_c))),
            )
            .await
            .unwrap();
        p.match_writer
            .create(&a, input("Pisa", 18, new("Coppa Toscana")))
            .await
            .unwrap();
        p.match_writer
            .create(&a, input("Siena", 25, None))
            .await
            .unwrap();
        let read = competition_of_each(&p, &a).await;
        let cup = read.iter().find(|(o, _)| o == "Pisa").unwrap().1.unwrap();
        assert_ne!(cup, serie_c);
        let expected = vec![
            ("Arezzo".to_string(), Some(serie_c)),
            ("Cascina".to_string(), Some(serie_c)),
            ("Pisa".to_string(), Some(cup)),
            ("Siena".to_string(), None),
        ];
        assert_eq!(read, expected);
        // from the cache, the same
        assert_eq!(competition_of_each(&p, &a).await, expected);
        // an empty cache is rebuilt with it
        p.db.write(|tx| {
            tx.execute_batch("DELETE FROM match_summary")
                .map_err(crate::providers::sqlite::sqlite)
        })
        .unwrap();
        assert_eq!(competition_of_each(&p, &a).await, expected);
        // the first moves to the cup: the listing follows at once
        p.match_writer
            .update(
                &first,
                input("Cascina", 4, Some(CompetitionInput::Existing(cup))),
            )
            .await
            .unwrap();
        let moved = competition_of_each(&p, &a).await;
        assert_eq!(moved[1], ("Cascina".to_string(), Some(cup)));
        assert_eq!(moved[0], ("Arezzo".to_string(), Some(serie_c)));
    }

    /// The filter over the summaries of the database: the matches of one
    /// competition among those of a team (none scouted yet: the period
    /// keeps only the scouted ones, so a set is added to the ones chosen).
    #[tokio::test]
    async fn the_competitions_offered_are_those_of_the_team() {
        let p = Sqlite::new();
        let a = team(&p).await;
        for (opponent, d, c) in [
            ("Cascina", 4, new("Serie C")),
            ("Pisa", 11, new("Coppa Toscana")),
            ("Siena", 18, None),
        ] {
            p.match_writer
                .create(&a, input(opponent, d, c))
                .await
                .unwrap();
        }
        let all: Vec<MatchSummary> = p
            .match_reader
            .read_summaries(&a)
            .await
            .unwrap()
            .into_iter()
            .map(|l| MatchSummary {
                sets: 1,
                ..l.unwrap()
            })
            .collect();
        let offered = competitions_of(&all, today());
        assert_eq!(offered.len(), 2);
        let period = Period::LastMatches(10);
        for c in &offered {
            let chosen = period.select_in(&all, today(), Some(*c));
            assert_eq!(chosen.len(), 1);
            assert_eq!(chosen[0].competition_id, Some(*c));
        }
        assert_eq!(period.select_in(&all, today(), None).len(), 3);
    }
}
