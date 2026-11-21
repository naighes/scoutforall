//! A team in two places at once (TODO/021 step 3): the sessions among them,
//! the sessions against the matches in programma (as A or as B, not the
//! played ones), and the match form against the sessions.

#[cfg(test)]
mod tests {
    use crate::{
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        errors::{AppError, TrainingError},
        localization::labels,
        match_form::{MatchInput, Opponent},
        providers::{
            match_writer::MatchWriter,
            sqlite::training::SqliteTraining,
            team_writer::{TeamInput, TeamWriter},
            training_reader::TrainingReader,
            training_writer::TrainingWriter,
        },
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, LanguageEnum, TeamClassificationEnum},
            r#match::MatchEntry,
            team::TeamEntry,
        },
        tests::Sqlite,
        training::{
            SessionInput,
            form::SessionForm,
            overlap::{DEFAULT_MATCH_MINUTES, Slot, SlotKind, clashes},
        },
    };
    use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime};
    use uuid::Uuid;

    fn at(day: u32, hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, day)
            .and_then(|d| d.and_hms_opt(hour, minute, 0))
            .unwrap()
    }

    fn session(start: NaiveDateTime, minutes: u16) -> SessionInput {
        SessionForm {
            start: Some(start),
            duration: minutes.to_string(),
            ..SessionForm::default()
        }
        .validate()
        .unwrap()
    }

    async fn team(p: &Sqlite, name: &str) -> TeamEntry {
        p.team_writer
            .save(TeamInput::New {
                id: None,
                name: name.into(),
                year: 2026,
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Women),
                country: CountryCode::known("IT"),
                players: vec![],
            })
            .await
            .unwrap()
    }

    /// 2026-11-<day> <hour>:00 with an offset that is not the one of the
    /// server: the clock time written is what counts.
    fn when(day: u32, hour: u32) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(&format!("2026-11-{day:02}T{hour:02}:00:00+09:00")).unwrap()
    }

    async fn the_match(
        p: &Sqlite,
        a: &TeamEntry,
        b: Option<&TeamEntry>,
        date: DateTime<FixedOffset>,
    ) -> Result<MatchEntry, AppError> {
        p.match_writer
            .create(
                a,
                MatchInput {
                    opponent: b.map_or(Opponent::Name("Others".into()), |b| Opponent::Team(b.id)),
                    ..MatchInput::against("", date, true, DEFAULT_MAX_SUBSTITUTIONS, "", vec![])
                },
            )
            .await
    }

    fn overlaps(e: &AppError) -> Vec<Slot> {
        match e {
            AppError::Training(TrainingError::Overlap(slots)) => slots.clone(),
            other => panic!("not an overlap: {other}"),
        }
    }

    struct World {
        p: Sqlite,
        training: SqliteTraining,
    }

    fn world() -> World {
        let p = Sqlite::new();
        World {
            training: SqliteTraining::new(p.db.clone()),
            p,
        }
    }

    #[test]
    fn slots_that_touch_do_not_overlap() {
        let slot = Slot::session(at(23, 20, 0), 60);
        let others = [
            Slot::session(at(23, 19, 0), 60),
            Slot::session(at(23, 21, 0), 60),
            Slot::session(at(23, 19, 30), 60),
            Slot::session(at(23, 20, 59), 5),
            Slot::session(at(24, 20, 0), 60),
        ];
        assert_eq!(
            clashes(&slot, &others),
            [others[2], others[3]],
            "only the two that share minutes"
        );
    }

    #[test]
    fn a_slot_inside_another_or_around_it_overlaps() {
        let slot = Slot::session(at(23, 20, 0), 60);
        let inside = Slot::session(at(23, 20, 10), 10);
        let around = Slot::session(at(23, 19, 0), 180);
        let same = Slot::session(at(23, 20, 0), 60);
        assert_eq!(clashes(&slot, &[inside, around, same]).len(), 3);
    }

    #[test]
    fn across_midnight_and_the_longest_slot() {
        let late = Slot::session(at(23, 23, 30), 90);
        assert_eq!(clashes(&late, &[Slot::session(at(24, 0, 15), 30)]).len(), 1);
        assert!(clashes(&late, &[Slot::session(at(24, 1, 0), 30)]).is_empty());
        let huge = Slot::session(at(23, 0, 0), u16::MAX);
        assert_eq!(clashes(&huge, &[Slot::session(at(25, 0, 0), 30)]).len(), 1);
    }

    #[test]
    fn a_match_without_duration_lasts_two_hours() {
        let m = Slot::match_of(at(23, 18, 0), None);
        assert_eq!(m.minutes, DEFAULT_MATCH_MINUTES);
        assert_eq!(DEFAULT_MATCH_MINUTES, 120);
        assert_eq!(m.kind, SlotKind::Match);
        assert_eq!(Slot::match_of(at(23, 18, 0), Some(90)).minutes, 90);
    }

    #[tokio::test]
    async fn two_sessions_of_a_team_overlapping_are_refused_naming_the_other() {
        let w = world();
        let t = team(&w.p, "A").await;
        w.training
            .create_session(&t.id, session(at(23, 20, 0), 90))
            .await
            .unwrap();
        let e = w
            .training
            .create_session(&t.id, session(at(23, 21, 0), 90))
            .await
            .unwrap_err();
        assert_eq!(overlaps(&e), [Slot::session(at(23, 20, 0), 90)]);
        assert_eq!(
            e.message(labels(LanguageEnum::En)),
            "overlaps the training of 2026-11-23 20:00"
        );
        assert_eq!(
            e.message(labels(LanguageEnum::It)),
            "si sovrappone all'allenamento del 2026-11-23 20:00"
        );
        assert_eq!(w.training.sessions(&t.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_double_session_and_a_touching_one_are_fine() {
        let w = world();
        let t = team(&w.p, "A").await;
        w.training
            .create_session(&t.id, session(at(23, 10, 0), 90))
            .await
            .unwrap();
        w.training
            .create_session(&t.id, session(at(23, 11, 30), 60))
            .await
            .unwrap();
        w.training
            .create_session(&t.id, session(at(23, 20, 0), 90))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn other_teams_overlap_freely_and_a_removed_session_does_not_count() {
        let w = world();
        let (a, b) = (team(&w.p, "A").await, team(&w.p, "B").await);
        let first = w
            .training
            .create_session(&a.id, session(at(23, 20, 0), 90))
            .await
            .unwrap();
        w.training
            .create_session(&b.id, session(at(23, 20, 0), 90))
            .await
            .unwrap();
        w.training.remove_session(&first.id).await.unwrap();
        w.training
            .create_session(&a.id, session(at(23, 20, 30), 90))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn editing_a_session_is_not_an_overlap_with_itself() {
        let w = world();
        let t = team(&w.p, "A").await;
        let s = w
            .training
            .create_session(&t.id, session(at(23, 20, 0), 90))
            .await
            .unwrap();
        let other = w
            .training
            .create_session(&t.id, session(at(24, 20, 0), 90))
            .await
            .unwrap();
        w.training
            .update_session(&s.id, session(at(23, 20, 30), 120))
            .await
            .unwrap();
        let e = w
            .training
            .update_session(&other.id, session(at(23, 22, 0), 90))
            .await
            .unwrap_err();
        assert_eq!(overlaps(&e), [Slot::session(at(23, 20, 30), 120)]);
    }

    #[tokio::test]
    async fn a_session_against_a_match_in_programma_as_a_or_as_b() {
        let w = world();
        let (a, b) = (team(&w.p, "A").await, team(&w.p, "B").await);
        the_match(&w.p, &a, Some(&b), when(23, 18)).await.unwrap();
        // the clock time of the match is 18:00, whatever its offset
        let clash = Slot::match_of(at(23, 18, 0), None);
        for team in [&a, &b] {
            let e = w
                .training
                .create_session(&team.id, session(at(23, 19, 0), 60))
                .await
                .unwrap_err();
            assert_eq!(overlaps(&e), [clash]);
            assert_eq!(
                e.message(labels(LanguageEnum::En)),
                "overlaps the match of 2026-11-23 18:00"
            );
            w.training
                .create_session(&team.id, session(at(23, 20, 0), 60))
                .await
                .unwrap();
            w.training
                .create_session(&team.id, session(at(23, 16, 30), 90))
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn a_played_match_does_not_count() {
        let w = world();
        let a = team(&w.p, "A").await;
        let m = the_match(&w.p, &a, None, when(23, 18)).await.unwrap();
        w.training
            .create_session(&a.id, session(at(23, 18, 0), 60))
            .await
            .unwrap_err();
        w.p.db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO sets (match_id, number, serving) VALUES (?1, 1, 'a')",
                    [crate::domain::sql::Sql(Uuid::parse_str(&m.id).unwrap())],
                )
                .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        w.training
            .create_session(&a.id, session(at(23, 18, 0), 60))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn the_match_form_refuses_a_session_of_a_or_of_b() {
        let w = world();
        let (a, b) = (team(&w.p, "A").await, team(&w.p, "B").await);
        w.training
            .create_session(&b.id, session(at(23, 17, 0), 120))
            .await
            .unwrap();
        let e = the_match(&w.p, &a, Some(&b), when(23, 18))
            .await
            .unwrap_err();
        assert_eq!(overlaps(&e), [Slot::session(at(23, 17, 0), 120)]);
        assert_eq!(
            e.message(labels(LanguageEnum::Ja)),
            "2026-11-23 17:00の練習と重なっています"
        );
        // against a name B has no sessions: only A's count
        the_match(&w.p, &a, None, when(23, 18)).await.unwrap();
        // nothing was saved by the refused one
        let free = the_match(&w.p, &a, Some(&b), when(23, 20)).await.unwrap();
        // editing it onto the session of B is refused too
        let e =
            w.p.match_writer
                .update(
                    &free,
                    MatchInput {
                        opponent: Opponent::Team(b.id),
                        ..MatchInput::against("", when(23, 18), true, 6, "", vec![])
                    },
                )
                .await
                .unwrap_err();
        assert_eq!(overlaps(&e), [Slot::session(at(23, 17, 0), 120)]);
    }
}
