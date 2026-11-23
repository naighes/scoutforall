//! The repetition of sessions (TODO/021 step 7): generation by weekday
//! bands, conflicts, group edit and removal. The times are local and naive,
//! so the cases with daylight saving time, leap days and months of different
//! length must come out the same in every time zone (`scripts/test.sh`).

#[cfg(test)]
mod tests {
    use crate::{
        constants::DEFAULT_MAX_SUBSTITUTIONS,
        errors::{AppError, TrainingError},
        match_form::{MatchInput, Opponent},
        providers::{
            match_writer::MatchWriter,
            sqlite::{training::SqliteTraining, venues::SqliteVenues},
            team_writer::{TeamInput, TeamWriter},
            training_reader::TrainingReader,
            training_writer::TrainingWriter,
        },
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, TeamClassificationEnum},
            team::TeamEntry,
        },
        tests::Sqlite,
        training::{
            SessionInput, SessionStatus, TrainingSession,
            form::SessionForm,
            overlap::{Slot, SlotKind},
            series::{Band, SeriesEdit, SeriesError, SeriesInput, SeriesScope, select},
            venue::VenueInput,
        },
    };
    use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
    use uuid::Uuid;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        day(y, m, d).and_hms_opt(h, min, 0).unwrap()
    }

    fn time(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn band(weekday: Weekday, h: u32, m: u32, minutes: u16) -> Band {
        Band {
            weekday,
            time: time(h, m),
            duration_min: minutes,
            venue_id: None,
        }
    }

    fn input(from: NaiveDate, to: NaiveDate, bands: Vec<Band>) -> SeriesInput {
        SeriesInput { from, to, bands }
    }

    /// The bands in the gym.
    fn in_gym(bands: Vec<Band>, venue: Uuid) -> Vec<Band> {
        bands
            .into_iter()
            .map(|b| Band {
                venue_id: Some(venue),
                ..b
            })
            .collect()
    }

    fn mon_wed() -> Vec<Band> {
        vec![
            band(Weekday::Mon, 20, 30, 120),
            band(Weekday::Wed, 18, 30, 120),
        ]
    }

    // 2026-11-23 is a Monday

    #[test]
    fn generates_a_band_for_each_chosen_weekday() {
        let found = input(day(2026, 11, 23), day(2026, 12, 6), mon_wed())
            .generate()
            .unwrap();
        assert_eq!(
            found,
            [
                (at(2026, 11, 23, 20, 30), 120),
                (at(2026, 11, 25, 18, 30), 120),
                (at(2026, 11, 30, 20, 30), 120),
                (at(2026, 12, 2, 18, 30), 120),
            ]
        );
    }

    #[test]
    fn both_ends_are_included() {
        let found = input(day(2026, 11, 23), day(2026, 11, 25), mon_wed())
            .generate()
            .unwrap();
        assert_eq!(found.len(), 2);
        let one = input(day(2026, 11, 23), day(2026, 11, 23), mon_wed())
            .generate()
            .unwrap();
        assert_eq!(one, [(at(2026, 11, 23, 20, 30), 120)]);
    }

    #[test]
    fn daylight_saving_time_moves_nothing() {
        // Europe/Rome leaves summer time on 2026-10-25, a Sunday
        let found = input(
            day(2026, 10, 18),
            day(2026, 11, 1),
            vec![band(Weekday::Sun, 20, 30, 90)],
        )
        .generate()
        .unwrap();
        assert_eq!(
            found,
            [
                (at(2026, 10, 18, 20, 30), 90),
                (at(2026, 10, 25, 20, 30), 90),
                (at(2026, 11, 1, 20, 30), 90),
            ]
        );
        // and a time that does not exist in Rome on 2026-03-29 at 02:30
        let spring = input(
            day(2026, 3, 29),
            day(2026, 3, 29),
            vec![band(Weekday::Sun, 2, 30, 60)],
        )
        .generate()
        .unwrap();
        assert_eq!(spring, [(at(2026, 3, 29, 2, 30), 60)]);
    }

    #[test]
    fn leap_day_and_months_of_different_length() {
        let leap = input(
            day(2028, 2, 22),
            day(2028, 3, 7),
            vec![band(Weekday::Tue, 19, 0, 60)],
        )
        .generate()
        .unwrap();
        assert_eq!(
            leap.iter().map(|(s, _)| s.date()).collect::<Vec<_>>(),
            [day(2028, 2, 22), day(2028, 2, 29), day(2028, 3, 7)]
        );
        let plain = input(
            day(2027, 2, 23),
            day(2027, 3, 9),
            vec![band(Weekday::Tue, 19, 0, 60)],
        )
        .generate()
        .unwrap();
        assert_eq!(
            plain.iter().map(|(s, _)| s.date()).collect::<Vec<_>>(),
            [day(2027, 2, 23), day(2027, 3, 2), day(2027, 3, 9)]
        );
        // from the 31st on: no month is skipped
        let long = input(
            day(2026, 12, 31),
            day(2027, 3, 1),
            vec![band(Weekday::Sun, 10, 0, 60)],
        )
        .generate()
        .unwrap();
        assert_eq!(long.len(), 9);
    }

    #[test]
    fn refuses_what_makes_no_sense() {
        let go = |from, to, bands| input(from, to, bands).generate().unwrap_err();
        let (a, b) = (day(2026, 11, 23), day(2026, 12, 6));
        assert_eq!(go(b, a, mon_wed()), SeriesError::InvalidRange);
        assert_eq!(go(a, b, vec![]), SeriesError::NoBands);
        assert_eq!(
            go(
                a,
                b,
                vec![band(Weekday::Mon, 20, 0, 60), band(Weekday::Mon, 18, 0, 60)]
            ),
            SeriesError::DuplicateWeekday
        );
        assert_eq!(
            go(a, b, vec![band(Weekday::Mon, 20, 0, 0)]),
            SeriesError::InvalidDuration
        );
        assert_eq!(go(a, day(2036, 12, 6), mon_wed()), SeriesError::TooMany);
        // 4 years of Mondays and Wednesdays are more than the cap
        assert_eq!(go(a, day(2030, 11, 23), mon_wed()), SeriesError::TooMany);
    }

    fn session_of(
        series: Option<Uuid>,
        start: NaiveDateTime,
        status: SessionStatus,
    ) -> TrainingSession {
        TrainingSession {
            id: Uuid::new_v4(),
            team_id: Uuid::nil(),
            microcycle_id: None,
            series_id: series,
            start,
            duration_min: 90,
            actual_duration_min: None,
            venue_id: None,
            title: String::new(),
            objectives: String::new(),
            notes: String::new(),
            final_remarks: String::new(),
            status,
        }
    }

    #[test]
    fn the_scope_picks_only_planned_sessions_of_the_series() {
        let s = Uuid::new_v4();
        let list = [
            session_of(Some(s), at(2026, 11, 23, 20, 0), SessionStatus::Planned),
            session_of(Some(s), at(2026, 11, 30, 20, 0), SessionStatus::Completed),
            session_of(Some(s), at(2026, 12, 7, 20, 0), SessionStatus::Planned),
            session_of(Some(s), at(2026, 12, 14, 20, 0), SessionStatus::Cancelled),
            session_of(Some(s), at(2026, 12, 21, 20, 0), SessionStatus::Planned),
            session_of(
                Some(Uuid::new_v4()),
                at(2026, 12, 8, 20, 0),
                SessionStatus::Planned,
            ),
        ];
        let starts = |scope, anchor: usize| {
            select(scope, &list[anchor], &list)
                .iter()
                .map(|s| s.start)
                .collect::<Vec<_>>()
        };
        assert_eq!(starts(SeriesScope::This, 2), [list[2].start]);
        assert_eq!(
            starts(SeriesScope::ThisAndFollowing, 2),
            [list[2].start, list[4].start]
        );
        assert_eq!(
            starts(SeriesScope::All, 2),
            [list[0].start, list[2].start, list[4].start]
        );
        // a session already completed is not touched, even if chosen
        assert!(starts(SeriesScope::This, 1).is_empty());
        // a session in no series is its own group
        let alone = session_of(None, at(2026, 12, 1, 20, 0), SessionStatus::Planned);
        assert_eq!(select(SeriesScope::All, &alone, &list).len(), 1);
    }

    // the provider

    struct World {
        p: Sqlite,
        training: SqliteTraining,
        gym: Uuid,
        arena: Uuid,
        hall: Uuid,
    }

    fn world() -> World {
        let p = Sqlite::new();
        let venues = SqliteVenues::new(p.db.clone());
        let venue = |name: &str| venues.create(VenueInput::named(name).unwrap()).unwrap().id;
        World {
            training: SqliteTraining::new(p.db.clone()),
            gym: venue("Gym"),
            arena: venue("Arena"),
            hall: venue("Hall"),
            p,
        }
    }

    async fn team(p: &Sqlite) -> TeamEntry {
        p.team_writer
            .save(TeamInput::New {
                id: None,
                name: "A".into(),
                year: 2026,
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Women),
                country: CountryCode::known("IT"),
                players: vec![],
            })
            .await
            .unwrap()
    }

    async fn plain_session(w: &World, team: &Uuid, start: NaiveDateTime, minutes: u16) {
        w.training
            .create_session(
                team,
                SessionForm {
                    start: Some(start),
                    duration: minutes.to_string(),
                    ..SessionForm::default()
                }
                .validate()
                .unwrap(),
            )
            .await
            .unwrap();
    }

    fn overlaps(e: &AppError) -> Vec<Slot> {
        match e {
            AppError::Training(TrainingError::Overlap(slots)) => slots.clone(),
            other => panic!("not an overlap: {other}"),
        }
    }

    #[tokio::test]
    async fn creates_real_planned_sessions_with_a_series_in_common() {
        let w = world();
        let t = team(&w.p).await;
        let out = w
            .training
            .create_series(
                &t.id,
                vec![input(
                    day(2026, 11, 23),
                    day(2026, 12, 6),
                    in_gym(mon_wed(), w.gym),
                )],
                false,
            )
            .await
            .unwrap();
        assert!(out.conflicts.is_empty());
        assert_eq!(out.created.len(), 4);
        let series = out.created[0].series_id.unwrap();
        for s in &out.created {
            assert_eq!(s.series_id, Some(series));
            assert_eq!(s.status, SessionStatus::Planned);
            assert_eq!(s.venue_id, Some(w.gym));
            assert_eq!(s.microcycle_id, None);
            assert_eq!((s.title.as_str(), s.objectives.as_str()), ("", ""));
            assert!(w.training.activities(&s.id).await.unwrap().is_empty());
            assert!(!w.training.attendance(&s.id).await.unwrap().recorded);
        }
        assert_eq!(w.training.sessions(&t.id).await.unwrap(), out.created);
        // the second repetition is another series
        let again = w
            .training
            .create_series(
                &t.id,
                vec![input(day(2026, 12, 14), day(2026, 12, 14), mon_wed())],
                false,
            )
            .await
            .unwrap();
        assert_ne!(again.created[0].series_id, Some(series));
    }

    #[tokio::test]
    async fn a_conflict_stops_everything_or_leaves_out_only_that_date() {
        let w = world();
        let t = team(&w.p).await;
        plain_session(&w, &t.id, at(2026, 11, 25, 19, 30), 60).await;
        let range = input(day(2026, 11, 23), day(2026, 12, 6), mon_wed());
        let stopped = w
            .training
            .create_series(&t.id, vec![range.clone()], false)
            .await
            .unwrap();
        assert!(stopped.created.is_empty());
        assert_eq!(stopped.conflicts.len(), 1);
        assert_eq!(stopped.conflicts[0].start, at(2026, 11, 25, 18, 30));
        assert_eq!(
            stopped.conflicts[0].with,
            [Slot::session(at(2026, 11, 25, 19, 30), 60)]
        );
        assert_eq!(w.training.sessions(&t.id).await.unwrap().len(), 1);

        let others = w
            .training
            .create_series(&t.id, vec![range], true)
            .await
            .unwrap();
        assert_eq!(others.created.len(), 3);
        assert_eq!(others.conflicts, stopped.conflicts);
        assert_eq!(w.training.sessions(&t.id).await.unwrap().len(), 4);
    }

    #[tokio::test]
    async fn a_match_in_programma_is_a_conflict() {
        let w = world();
        let t = team(&w.p).await;
        w.p.match_writer
            .create(
                &t,
                MatchInput {
                    opponent: Opponent::Name("Others".into()),
                    ..MatchInput::against(
                        "",
                        DateTime::parse_from_rfc3339("2026-11-23T19:30:00+09:00").unwrap(),
                        true,
                        DEFAULT_MAX_SUBSTITUTIONS,
                        "",
                        vec![],
                    )
                },
            )
            .await
            .unwrap();
        let out = w
            .training
            .create_series(
                &t.id,
                vec![input(day(2026, 11, 23), day(2026, 11, 25), mon_wed())],
                true,
            )
            .await
            .unwrap();
        assert_eq!(out.created.len(), 1);
        assert_eq!(out.conflicts[0].with[0].kind, SlotKind::Match);
    }

    #[tokio::test]
    async fn two_sessions_of_the_series_across_midnight_do_not_overlap_each_other() {
        let w = world();
        let t = team(&w.p).await;
        // Mon 23:00-01:00 and Tue 00:30-01:30
        let out = w
            .training
            .create_series(
                &t.id,
                vec![input(
                    day(2026, 11, 23),
                    day(2026, 11, 24),
                    vec![
                        band(Weekday::Mon, 23, 0, 120),
                        band(Weekday::Tue, 0, 30, 60),
                    ],
                )],
                true,
            )
            .await
            .unwrap();
        assert_eq!(out.created.len(), 1);
        assert_eq!(out.conflicts[0].start, at(2026, 11, 24, 0, 30));
    }

    #[tokio::test]
    async fn a_team_that_is_not_there_is_not_found() {
        let w = world();
        assert!(
            w.training
                .create_series(
                    &Uuid::new_v4(),
                    vec![input(day(2026, 11, 23), day(2026, 11, 25), mon_wed())],
                    true,
                )
                .await
                .is_err()
        );
        // and a refused input writes nothing
        let t = team(&w.p).await;
        assert!(
            w.training
                .create_series(
                    &t.id,
                    vec![input(day(2026, 11, 25), day(2026, 11, 23), mon_wed())],
                    true
                )
                .await
                .is_err()
        );
        assert!(w.training.sessions(&t.id).await.unwrap().is_empty());
    }

    async fn four(w: &World) -> (TeamEntry, Vec<TrainingSession>) {
        let t = team(&w.p).await;
        let out = w
            .training
            .create_series(
                &t.id,
                vec![input(
                    day(2026, 11, 23),
                    day(2026, 12, 6),
                    in_gym(mon_wed(), w.gym),
                )],
                false,
            )
            .await
            .unwrap();
        (t, out.created)
    }

    #[tokio::test]
    async fn the_group_edit_reaches_the_scope_and_never_the_date() {
        let w = world();
        let (_, list) = four(&w).await;
        let edit = SeriesEdit {
            venue: Some(Some(w.arena)),
            time: None,
            duration_min: Some(75),
        };
        let done = w
            .training
            .update_series(&list[1].id, SeriesScope::ThisAndFollowing, edit)
            .await
            .unwrap();
        assert_eq!(done.len(), 3);
        let now = w.training.sessions(&list[0].team_id).await.unwrap();
        // the first is as it was; the others changed place and length, and
        // keep their date and their time (it was not touched)
        assert_eq!(now[0], list[0]);
        for (before, after) in list[1..].iter().zip(&now[1..]) {
            assert_eq!(after.venue_id, Some(w.arena));
            assert_eq!(after.duration_min, 75);
            assert_eq!(after.start, before.start);
            assert_eq!(after.series_id, before.series_id);
        }
    }

    #[tokio::test]
    async fn the_time_is_rewritten_only_when_it_is_changed() {
        let w = world();
        let (t, list) = four(&w).await;
        w.training
            .update_series(
                &list[0].id,
                SeriesScope::All,
                SeriesEdit {
                    time: Some(time(21, 0)),
                    ..SeriesEdit::default()
                },
            )
            .await
            .unwrap();
        let now = w.training.sessions(&t.id).await.unwrap();
        for (before, after) in list.iter().zip(&now) {
            assert_eq!(after.start, before.start.date().and_time(time(21, 0)));
            assert_eq!(after.duration_min, before.duration_min);
            assert_eq!(after.venue_id, before.venue_id);
        }
        // "this" only
        w.training
            .update_series(
                &list[2].id,
                SeriesScope::This,
                SeriesEdit {
                    venue: Some(Some(w.hall)),
                    ..SeriesEdit::default()
                },
            )
            .await
            .unwrap();
        let now = w.training.sessions(&t.id).await.unwrap();
        assert_eq!(
            now.iter().map(|s| s.venue_id).collect::<Vec<_>>(),
            [Some(w.gym), Some(w.gym), Some(w.hall), Some(w.gym)]
        );
    }

    #[tokio::test]
    async fn only_the_planned_sessions_are_touched() {
        let w = world();
        let (t, list) = four(&w).await;
        w.training
            .set_session_status(&list[1].id, SessionStatus::Cancelled)
            .await
            .unwrap();
        w.training
            .complete_session(&list[2].id, vec![])
            .await
            .unwrap();
        let done = w
            .training
            .update_series(
                &list[0].id,
                SeriesScope::All,
                SeriesEdit {
                    venue: Some(Some(w.arena)),
                    ..SeriesEdit::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(done.len(), 2);
        let now = w.training.sessions(&t.id).await.unwrap();
        assert_eq!(
            now.iter().map(|s| s.venue_id).collect::<Vec<_>>(),
            [Some(w.arena), Some(w.gym), Some(w.gym), Some(w.arena)]
        );
        // removing too: the cancelled and the completed stay
        let removed = w
            .training
            .remove_series(&list[0].id, SeriesScope::All)
            .await
            .unwrap();
        assert_eq!(removed, 2);
        assert_eq!(w.training.sessions(&t.id).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn an_overlap_stops_the_whole_group_and_says_which() {
        let w = world();
        let (t, list) = four(&w).await;
        // a session right where the third one would end up if it started at 18:30
        plain_session(&w, &t.id, at(2026, 11, 30, 18, 30), 60).await;
        let e = w
            .training
            .update_series(
                &list[0].id,
                SeriesScope::All,
                SeriesEdit {
                    time: Some(time(18, 30)),
                    ..SeriesEdit::default()
                },
            )
            .await
            .unwrap_err();
        assert_eq!(overlaps(&e), [Slot::session(at(2026, 11, 30, 18, 30), 60)]);
        // nothing was written, not even the sessions before the conflict
        let now = w.training.sessions(&t.id).await.unwrap();
        for before in &list {
            assert!(now.contains(before));
        }
    }

    #[tokio::test]
    async fn the_group_overlaps_are_checked_against_the_moved_ones_too() {
        let w = world();
        let (t, list) = four(&w).await;
        // a duration of 4000 minutes (2.7 days) makes Monday reach Wednesday
        let e = w
            .training
            .update_series(
                &list[0].id,
                SeriesScope::All,
                SeriesEdit {
                    duration_min: Some(4000),
                    ..SeriesEdit::default()
                },
            )
            .await
            .unwrap_err();
        assert!(!overlaps(&e).is_empty());
        assert_eq!(w.training.sessions(&t.id).await.unwrap(), list);
    }

    #[tokio::test]
    async fn an_empty_duration_and_a_missing_session_are_refused() {
        let w = world();
        let (_, list) = four(&w).await;
        assert!(
            w.training
                .update_series(
                    &list[0].id,
                    SeriesScope::All,
                    SeriesEdit {
                        duration_min: Some(0),
                        ..SeriesEdit::default()
                    },
                )
                .await
                .is_err()
        );
        assert!(
            w.training
                .remove_series(&Uuid::new_v4(), SeriesScope::All)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_session_in_no_series_is_its_own_group() {
        let w = world();
        let t = team(&w.p).await;
        plain_session(&w, &t.id, at(2026, 11, 23, 20, 0), 90).await;
        plain_session(&w, &t.id, at(2026, 11, 30, 20, 0), 90).await;
        let list = w.training.sessions(&t.id).await.unwrap();
        let removed = w
            .training
            .remove_series(&list[0].id, SeriesScope::All)
            .await
            .unwrap();
        assert_eq!(removed, 1);
        assert_eq!(w.training.sessions(&t.id).await.unwrap().len(), 1);
    }

    /// Monday, Wednesday and Thursday at 20:30 for two weeks, each entry of
    /// the dialog its own series (decided 2026-11-23).
    async fn three_entries(w: &World) -> (TeamEntry, Vec<TrainingSession>) {
        let t = team(&w.p).await;
        let entry =
            |weekday, first| input(first, day(2026, 12, 6), vec![band(weekday, 20, 30, 120)]);
        let out = w
            .training
            .create_series(
                &t.id,
                vec![
                    entry(Weekday::Mon, day(2026, 11, 23)),
                    entry(Weekday::Wed, day(2026, 11, 25)),
                    entry(Weekday::Thu, day(2026, 11, 26)),
                ],
                false,
            )
            .await
            .unwrap();
        (t, out.created)
    }

    #[tokio::test]
    async fn each_entry_of_the_dialog_is_a_series_of_its_own() {
        let w = world();
        let (t, made) = three_entries(&w).await;
        assert_eq!(made.len(), 6);
        let series_of = |weekday: Weekday| {
            made.iter()
                .filter(|s| chrono::Datelike::weekday(&s.start) == weekday)
                .map(|s| s.series_id.unwrap())
                .collect::<std::collections::HashSet<_>>()
        };
        let (mon, wed, thu) = (
            series_of(Weekday::Mon),
            series_of(Weekday::Wed),
            series_of(Weekday::Thu),
        );
        assert_eq!((mon.len(), wed.len(), thu.len()), (1, 1, 1));
        assert!(mon.is_disjoint(&wed) && wed.is_disjoint(&thu) && mon.is_disjoint(&thu));
        assert_eq!(w.training.sessions(&t.id).await.unwrap().len(), 6);
    }

    #[tokio::test]
    async fn entries_that_overlap_each_other_are_conflicts() {
        let w = world();
        let t = team(&w.p).await;
        let monday = |h| {
            input(
                day(2026, 11, 23),
                day(2026, 11, 23),
                vec![band(Weekday::Mon, h, 0, 120)],
            )
        };
        let out = w
            .training
            .create_series(&t.id, vec![monday(18), monday(19)], false)
            .await
            .unwrap();
        assert!(out.created.is_empty());
        assert_eq!(out.conflicts.len(), 1);
        assert_eq!(out.conflicts[0].start, at(2026, 11, 23, 19, 0));
        // all the entries together are capped
        let long = |weekday| {
            input(
                day(2026, 11, 23),
                day(2030, 11, 23),
                vec![band(weekday, 8, 0, 60)],
            )
        };
        assert!(
            w.training
                .create_series(&t.id, vec![long(Weekday::Mon), long(Weekday::Tue)], true)
                .await
                .is_err()
        );
        assert!(w.training.sessions(&t.id).await.unwrap().is_empty());
    }

    fn form_of(s: &TrainingSession) -> SessionInput {
        SessionInput {
            microcycle_id: s.microcycle_id,
            start: s.start,
            duration_min: s.duration_min,
            actual_duration_min: s.actual_duration_min,
            venue_id: s.venue_id,
            title: s.title.clone(),
            objectives: s.objectives.clone(),
            notes: s.notes.clone(),
            final_remarks: s.final_remarks.clone(),
            status: s.status,
        }
    }

    #[tokio::test]
    async fn the_thursday_is_moved_alone_with_all_its_series() {
        let w = world();
        let (t, made) = three_entries(&w).await;
        let thursday = made
            .iter()
            .find(|s| s.start == at(2026, 11, 26, 20, 30))
            .unwrap();
        let input = SessionInput {
            start: at(2026, 11, 26, 19, 30),
            title: "Ricezione".into(),
            ..form_of(thursday)
        };
        let done = w
            .training
            .update_session_in_series(&thursday.id, input, SeriesScope::All)
            .await
            .unwrap();
        assert_eq!(done.len(), 2);
        let now = w.training.sessions(&t.id).await.unwrap();
        let starts: Vec<NaiveDateTime> = now.iter().map(|s| s.start).collect();
        assert_eq!(
            starts,
            [
                at(2026, 11, 23, 20, 30),
                at(2026, 11, 25, 20, 30),
                at(2026, 11, 26, 19, 30),
                at(2026, 11, 30, 20, 30),
                at(2026, 12, 2, 20, 30),
                at(2026, 12, 3, 19, 30),
            ]
        );
        // the title is of the session edited only
        let titles: Vec<&str> = now.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["", "", "Ricezione", "", "", ""]);
    }

    #[tokio::test]
    async fn this_and_following_and_the_date_of_the_one_edited() {
        let w = world();
        let (t, list) = four(&w).await;
        // the second (Wed 25th 18:30) moves to Thursday at 19:00, 90 minutes
        let input = SessionInput {
            start: at(2026, 11, 26, 19, 0),
            duration_min: 90,
            ..form_of(&list[1])
        };
        w.training
            .update_session_in_series(&list[1].id, input, SeriesScope::ThisAndFollowing)
            .await
            .unwrap();
        let now = w.training.sessions(&t.id).await.unwrap();
        let windows: Vec<(NaiveDateTime, u16)> =
            now.iter().map(|s| (s.start, s.duration_min)).collect();
        // the first is as it was; the date goes to the edited one only
        assert_eq!(
            windows,
            [
                (at(2026, 11, 23, 20, 30), 120),
                (at(2026, 11, 26, 19, 0), 90),
                (at(2026, 11, 30, 19, 0), 90),
                (at(2026, 12, 2, 19, 0), 90),
            ]
        );
    }

    #[tokio::test]
    async fn the_one_edited_changes_even_if_completed_the_others_only_if_planned() {
        let w = world();
        let (t, list) = four(&w).await;
        w.training
            .complete_session(&list[0].id, vec![])
            .await
            .unwrap();
        w.training
            .set_session_status(&list[2].id, SessionStatus::Cancelled)
            .await
            .unwrap();
        let input = SessionInput {
            venue_id: Some(w.arena),
            ..form_of(&w.training.session(&list[0].id).await.unwrap())
        };
        let done = w
            .training
            .update_session_in_series(&list[0].id, input, SeriesScope::All)
            .await
            .unwrap();
        assert_eq!(done.len(), 3);
        let now = w.training.sessions(&t.id).await.unwrap();
        assert_eq!(
            now.iter().map(|s| s.venue_id).collect::<Vec<_>>(),
            [Some(w.arena), Some(w.arena), Some(w.gym), Some(w.arena)]
        );
        assert_eq!(now[0].status, SessionStatus::Completed);
    }

    #[tokio::test]
    async fn an_overlap_of_the_others_stops_the_edit_too() {
        let w = world();
        let (t, list) = four(&w).await;
        plain_session(&w, &t.id, at(2026, 12, 2, 20, 30), 60).await;
        let input = SessionInput {
            start: at(2026, 11, 25, 20, 0),
            title: "x".into(),
            ..form_of(&list[1])
        };
        let e = w
            .training
            .update_session_in_series(&list[1].id, input, SeriesScope::All)
            .await
            .unwrap_err();
        assert_eq!(overlaps(&e), [Slot::session(at(2026, 12, 2, 20, 30), 60)]);
        let now = w.training.sessions(&t.id).await.unwrap();
        for before in &list {
            assert!(now.contains(before));
        }
    }

    #[tokio::test]
    async fn removing_takes_the_one_chosen_even_if_completed() {
        let w = world();
        let (t, list) = four(&w).await;
        w.training
            .complete_session(&list[1].id, vec![])
            .await
            .unwrap();
        let removed = w
            .training
            .remove_series(&list[1].id, SeriesScope::ThisAndFollowing)
            .await
            .unwrap();
        assert_eq!(removed, 3);
        let now = w.training.sessions(&t.id).await.unwrap();
        assert_eq!(now, [list[0].clone()]);
    }
}
