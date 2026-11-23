//! The agenda (TODO/021 step 11): week, month, today, one team or all, the
//! matches with their duration and score, a match between two teams once.

#[cfg(test)]
mod tests {
    use crate::{
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        domain::sql::Sql,
        match_form::{MatchInput, Opponent},
        providers::{
            match_writer::MatchWriter,
            sqlite::{sqlite, training::SqliteTraining},
            team_writer::{TeamInput, TeamWriter},
            training_agenda::TrainingAgenda,
            training_writer::TrainingWriter,
        },
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, TeamClassificationEnum},
            team::TeamEntry,
        },
        tests::Sqlite,
        training::{
            SessionInput, SessionStatus,
            agenda::{AgendaEvent, AgendaItem, MatchState, Period, by_day, hours, lanes},
            form::SessionForm,
        },
    };
    use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime};
    use uuid::Uuid;

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn at(d: u32, hour: u32, minute: u32) -> NaiveDateTime {
        day(d).and_hms_opt(hour, minute, 0).unwrap()
    }

    fn session(start: NaiveDateTime, minutes: u16) -> SessionInput {
        SessionForm {
            start: Some(start),
            duration: minutes.to_string(),
            title: "Serve".into(),
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

    fn when(d: u32, hour: u32) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(&format!("2026-11-{d:02}T{hour:02}:00:00+09:00")).unwrap()
    }

    async fn the_match(
        p: &Sqlite,
        a: &TeamEntry,
        b: Option<&TeamEntry>,
        date: DateTime<FixedOffset>,
    ) -> Uuid {
        let m = p
            .match_writer
            .create(
                a,
                MatchInput {
                    opponent: b.map_or(Opponent::Name("Others".into()), |b| Opponent::Team(b.id)),
                    ..MatchInput::against("", date, true, DEFAULT_MAX_SUBSTITUTIONS, "", vec![])
                },
            )
            .await
            .unwrap();
        Uuid::parse_str(&m.id).unwrap()
    }

    /// The match has a set with events at the given clock minutes, and a
    /// summary (when `wins`; the one of the new match is replaced).
    fn play(p: &Sqlite, m: Uuid, first: &str, last: &str, wins: Option<(u8, u8)>) {
        p.db.write(|tx| {
            tx.execute(
                "INSERT INTO sets (match_id, number, serving) VALUES (?1, 1, 'a')",
                [Sql(m)],
            )
            .map_err(sqlite)?;
            [(0, first), (1, last)].iter().try_for_each(|(seq, ts)| {
                tx.execute(
                    "INSERT INTO events (id, match_id, set_number, seq, side, timestamp, event_type)
                     VALUES (?1, ?2, 1, ?3, 'a', ?4, 'serve')",
                    rusqlite::params![Sql(Uuid::new_v4()), Sql(m), seq, ts],
                )
                .map(|_| ())
                .map_err(sqlite)
            })?;
            tx.execute("DELETE FROM match_summary WHERE match_id = ?1", [Sql(m)])
                .map_err(sqlite)?;
            if let Some((a, b)) = wins {
                tx.execute(
                    "INSERT INTO match_summary (match_id, sets, a_wins, b_wins, finished, version)
                     VALUES (?1, 3, ?2, ?3, 1, 'x')",
                    rusqlite::params![Sql(m), a, b],
                )
                .map_err(sqlite)?;
            }
            Ok(())
        })
        .unwrap();
    }

    fn world() -> (Sqlite, SqliteTraining) {
        let p = Sqlite::new();
        let training = SqliteTraining::new(p.db.clone());
        (p, training)
    }

    async fn agenda(t: &SqliteTraining, team: Option<&Uuid>, period: Period) -> Vec<AgendaEvent> {
        t.agenda(team, period, at(25, 12, 0)).await.unwrap()
    }

    #[test]
    fn periods_cover_whole_weeks_and_months() {
        let week = Period::week(day(25));
        assert_eq!((week.from, week.to), (day(23), day(30)));
        assert_eq!(week.days().len(), 7);
        let month = Period::month(day(25));
        assert_eq!((month.from, month.days().len()), (day(1), 30));
        assert_eq!(Period::day(day(25)).days(), [day(25)]);
        assert!(week.contains(day(29)) && !week.contains(day(30)));
    }

    /// The week by the hour shows 7–24, earlier for an event before; an
    /// event past midnight stops at it.
    #[test]
    fn the_hours_of_the_week_hold_its_events() {
        assert_eq!(hours(&[]), (7, 24));
        assert_eq!(hours(&[(18 * 60, 90)]), (7, 24));
        assert_eq!(hours(&[(6 * 60 + 30, 60), (21 * 60 + 30, 60)]), (6, 24));
        assert_eq!(hours(&[(23 * 60, 120)]), (7, 24));
    }

    /// Events at the same time go side by side, each in the first lane
    /// free; one starting as they end takes the whole column again; a
    /// short one counts as half an hour.
    #[test]
    fn events_at_the_same_time_go_side_by_side() {
        let events = [(1080, 90), (1110, 60), (1170, 60), (1260, 60)];
        assert_eq!(lanes(&events), vec![(0, 2), (1, 2), (0, 1), (0, 1)]);
        assert_eq!(lanes(&[(600, 10), (620, 60)]), vec![(0, 2), (1, 2)]);
        assert_eq!(lanes(&[]), vec![]);
    }

    #[tokio::test]
    async fn no_training_and_no_matches_is_an_empty_week_of_seven_days() {
        let (p, t) = world();
        let a = team(&p, "A").await;
        let period = Period::week(day(25));
        let events = agenda(&t, Some(&a.id), period).await;
        assert!(events.is_empty());
        let days = by_day(&period, &events);
        assert_eq!(days.len(), 7);
        assert!(days.iter().all(|(_, e)| e.is_empty()));
    }

    #[tokio::test]
    async fn the_week_holds_sessions_and_matches_in_order_and_nothing_else() {
        let (p, t) = world();
        let (a, b) = (team(&p, "A").await, team(&p, "B").await);
        t.create_session(&a.id, session(at(23, 18, 0), 90))
            .await
            .unwrap();
        t.create_session(&a.id, session(at(22, 18, 0), 90))
            .await
            .unwrap();
        t.create_session(&a.id, session(at(30, 18, 0), 90))
            .await
            .unwrap();
        t.create_session(&b.id, session(at(24, 18, 0), 90))
            .await
            .unwrap();
        let m = the_match(&p, &a, None, when(26, 20)).await;
        let events = agenda(&t, Some(&a.id), Period::week(day(25))).await;
        let starts: Vec<_> = events.iter().map(|e| e.start).collect();
        assert_eq!(starts, [at(23, 18, 0), at(26, 20, 0)]);
        assert!(matches!(
            &events[1].item,
            AgendaItem::Match { id, opponent, state: MatchState::Scheduled, .. }
                if *id == m && opponent == "Others"
        ));
        assert_eq!(events[1].minutes, 120, "the usual duration");
        let days = by_day(&Period::week(day(25)), &events);
        assert_eq!(days[0].1.len(), 1);
        assert_eq!(days[3].1.len(), 1);
    }

    #[tokio::test]
    async fn a_planned_session_that_is_over_is_to_close() {
        let (p, t) = world();
        let a = team(&p, "A").await;
        t.create_session(&a.id, session(at(24, 18, 0), 90))
            .await
            .unwrap();
        let late = t
            .create_session(&a.id, session(at(26, 18, 0), 90))
            .await
            .unwrap();
        let done = t
            .create_session(&a.id, session(at(23, 18, 0), 90))
            .await
            .unwrap();
        t.set_session_status(&done.id, SessionStatus::Cancelled)
            .await
            .unwrap();
        let events = agenda(&t, Some(&a.id), Period::week(day(25))).await;
        let closing: Vec<_> = events
            .iter()
            .map(|e| match &e.item {
                AgendaItem::Session { to_close, .. } => *to_close,
                AgendaItem::Match { .. } => false,
            })
            .collect();
        assert_eq!(closing, [false, true, false], "cancelled and future aren't");
        assert_ne!(late.id, done.id);
    }

    #[tokio::test]
    async fn a_removed_session_or_match_is_not_there() {
        let (p, t) = world();
        let a = team(&p, "A").await;
        let s = t
            .create_session(&a.id, session(at(24, 18, 0), 90))
            .await
            .unwrap();
        let m = the_match(&p, &a, None, when(25, 20)).await;
        p.db.write(|tx| {
            tx.execute("UPDATE matches SET deleted = 1 WHERE id = ?1", [Sql(m)])
                .map_err(sqlite)?;
            tx.execute(
                "UPDATE training_sessions SET deleted = 1 WHERE id = ?1",
                [Sql(s.id)],
            )
            .map_err(sqlite)
        })
        .unwrap();
        assert!(agenda(&t, None, Period::month(day(25))).await.is_empty());
    }

    #[tokio::test]
    async fn a_match_between_two_teams_is_once_for_all_and_once_for_each() {
        let (p, t) = world();
        let (a, b) = (team(&p, "A").await, team(&p, "B").await);
        let m = the_match(&p, &a, Some(&b), when(26, 20)).await;
        let all = agenda(&t, None, Period::week(day(25))).await;
        assert_eq!(all.len(), 1);
        assert_eq!((all[0].team_id, all[0].team_name.as_str()), (a.id, "A"));
        let for_b = agenda(&t, Some(&b.id), Period::week(day(25))).await;
        assert_eq!(for_b.len(), 1);
        assert!(matches!(
            &for_b[0].item,
            AgendaItem::Match { id, opponent, opponent_team: Some(o), .. }
                if *id == m && opponent == "A" && *o == a.id
        ));
        assert_eq!(for_b[0].team_name, "B");
    }

    #[tokio::test]
    async fn all_the_teams_show_their_name_on_each_event() {
        let (p, t) = world();
        let (a, b) = (team(&p, "A").await, team(&p, "B").await);
        t.create_session(&b.id, session(at(24, 18, 0), 90))
            .await
            .unwrap();
        t.create_session(&a.id, session(at(24, 19, 0), 90))
            .await
            .unwrap();
        let names: Vec<_> = agenda(&t, None, Period::day(day(24)))
            .await
            .into_iter()
            .map(|e| e.team_name)
            .collect();
        assert_eq!(names, ["B", "A"]);
    }

    #[tokio::test]
    async fn a_played_match_has_the_score_of_the_team_and_the_span_of_its_events() {
        let (p, t) = world();
        let (a, b) = (team(&p, "A").await, team(&p, "B").await);
        let m = the_match(&p, &a, Some(&b), when(24, 20)).await;
        play(
            &p,
            m,
            "2026-11-24T20:05:00+09:00",
            "2026-11-24T21:35:30+09:00",
            Some((3, 1)),
        );
        let state = |events: &[AgendaEvent]| match &events[0].item {
            AgendaItem::Match { state, .. } => *state,
            AgendaItem::Session { .. } => unreachable!(),
        };
        let for_a = agenda(&t, Some(&a.id), Period::week(day(25))).await;
        assert_eq!(state(&for_a), MatchState::Played(Some((3, 1))));
        assert_eq!(for_a[0].minutes, 90);
        let for_b = agenda(&t, Some(&b.id), Period::week(day(25))).await;
        assert_eq!(state(&for_b), MatchState::Played(Some((1, 3))));
    }

    #[tokio::test]
    async fn a_played_match_without_summary_has_no_score() {
        let (p, t) = world();
        let a = team(&p, "A").await;
        let m = the_match(&p, &a, None, when(24, 20)).await;
        play(
            &p,
            m,
            "2026-11-24T20:00:00+09:00",
            "2026-11-24T20:00:00+09:00",
            None,
        );
        let events = agenda(&t, Some(&a.id), Period::day(day(24))).await;
        assert!(matches!(
            events[0].item,
            AgendaItem::Match {
                state: MatchState::Played(None),
                ..
            }
        ));
        assert_eq!(
            events[0].minutes, 1,
            "a single instant still lasts a minute"
        );
    }

    #[tokio::test]
    async fn the_day_of_a_match_is_the_clock_day_written() {
        let (p, t) = world();
        let a = team(&p, "A").await;
        the_match(&p, &a, None, when(24, 1)).await;
        assert_eq!(agenda(&t, None, Period::day(day(24))).await.len(), 1);
        assert!(agenda(&t, None, Period::day(day(23))).await.is_empty());
    }
}
