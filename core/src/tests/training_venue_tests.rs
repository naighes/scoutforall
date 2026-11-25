//! The gyms (TODO/021 §13): the coordinates in the links of the maps, the
//! form, the choice in the forms, the pin, the removal, and the warnings
//! when two teams hold the same gym at the same time.

#[cfg(test)]
mod tests {
    use crate::{
        db::{self, MIGRATIONS},
        match_form::MatchForm,
        providers::{
            match_writer::MatchWriter,
            sqlite::{teams::SqliteTeamWriter, training::SqliteTraining, venues::SqliteVenues},
            team_writer::{TeamInput, TeamWriter},
            training_agenda::TrainingAgenda,
            training_reader::TrainingReader,
            training_writer::TrainingWriter,
        },
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, TeamClassificationEnum},
            team::TeamEntry,
        },
        tests::{EN, Sqlite},
        training::{
            SessionStatus,
            agenda::Period,
            duplicate::{DuplicateCycleInput, DuplicateInput},
            form::SessionForm,
            overlap::SlotKind,
            series::{Band, SeriesInput},
            venue::{
                LinkError, Venue, VenueForm, VenueFormError, VenueInput, coordinates_from_link,
                geo_uri, web_url,
            },
        },
    };
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Weekday};
    use uuid::Uuid;

    // the coordinates

    #[test]
    fn the_coordinates_come_from_every_kind_of_link() {
        let places = (45.0716, 7.6869);
        for link in [
            // Google: the view, the place, the query
            "https://www.google.com/maps/place/Palasport/@45.0716,7.6869,17z/data=!3m1",
            "https://www.google.com/maps/place/X/data=!4m2!3m1!1s0x0:0x0!3d45.0716!4d7.6869",
            "https://www.google.com/maps?q=45.0716,7.6869",
            "https://www.google.com/maps?ll=45.0716,7.6869&z=15",
            "https://www.google.com/maps/search/?api=1&query=45.0716%2C7.6869",
            // Apple
            "https://maps.apple.com/?ll=45.0716,7.6869",
            "https://maps.apple.com/?q=45.0716,7.6869",
            "https://maps.apple.com/place?coordinate=45.0716,7.6869&name=Palasport",
            // Bing
            "https://www.bing.com/maps?cp=45.0716~7.6869&lvl=16",
            "https://www.bing.com/maps?where1=45.0716,7.6869",
            // OpenStreetMap
            "https://www.openstreetmap.org/#map=17/45.0716/7.6869",
            "https://www.openstreetmap.org/?mlat=45.0716&mlon=7.6869#map=17/45.5/7.5",
            // the phone
            "geo:45.0716,7.6869",
            "geo:45.0716,7.6869?q=Palasport",
        ] {
            assert_eq!(coordinates_from_link(link), Ok(places), "{link}");
        }
    }

    #[test]
    fn the_place_wins_over_the_view_in_a_google_link() {
        assert_eq!(
            coordinates_from_link(
                "https://www.google.com/maps/place/X/@45.0,7.0,17z/data=!3d45.5!4d7.5"
            ),
            Ok((45.5, 7.5))
        );
    }

    #[test]
    fn the_southern_and_western_coordinates_are_negative() {
        assert_eq!(
            coordinates_from_link("https://www.google.com/maps/@-33.8688,151.2093,12z"),
            Ok((-33.8688, 151.2093))
        );
        assert_eq!(
            coordinates_from_link("https://www.openstreetmap.org/#map=10/40.7/-74.0"),
            Ok((40.7, -74.0))
        );
    }

    #[test]
    fn a_short_link_has_no_coordinates() {
        for link in [
            "https://maps.app.goo.gl/abcDEF123",
            "https://goo.gl/maps/abcDEF123",
            "https://g.co/kgs/abc",
        ] {
            assert_eq!(coordinates_from_link(link), Err(LinkError::Short), "{link}");
        }
    }

    #[test]
    fn a_link_with_no_place_or_a_place_out_of_the_earth_is_refused() {
        for link in [
            "",
            "not a link",
            "https://www.google.com/maps/place/Palasport",
            "https://www.google.com/maps?q=Palasport",
        ] {
            assert_eq!(
                coordinates_from_link(link),
                Err(LinkError::NoCoordinates),
                "{link}"
            );
        }
        for link in [
            "geo:91.0,7.0",
            "geo:45.0,181.0",
            "https://www.google.com/maps/@95.0,7.0,17z",
            "https://maps.apple.com/?ll=-91,0",
        ] {
            assert_eq!(
                coordinates_from_link(link),
                Err(LinkError::OutOfRange),
                "{link}"
            );
        }
        // never a value that is not a number
        assert!(coordinates_from_link("geo:NaN,7.0").is_err());
        assert!(coordinates_from_link("geo:inf,7.0").is_err());
    }

    // the form

    fn form(name: &str) -> VenueForm {
        VenueForm {
            name: name.into(),
            ..VenueForm::default()
        }
    }

    #[test]
    fn a_gym_needs_a_name_and_nothing_more() {
        assert_eq!(form("  ").validate(), Err(VenueFormError::EmptyName));
        let input = form("  Palasport ").validate().unwrap();
        assert_eq!(input.name, "Palasport");
        assert_eq!(input.coordinates, None);
        assert!(!input.pinned);
    }

    #[test]
    fn the_coordinates_come_together_or_not_at_all() {
        for (lat, lon) in [
            ("45", ""),
            ("", "7"),
            ("x", "7"),
            ("91", "7"),
            ("45", "181"),
            ("nan", "7"),
        ] {
            let f = VenueForm {
                latitude: lat.into(),
                longitude: lon.into(),
                ..form("G")
            };
            assert_eq!(
                f.validate(),
                Err(VenueFormError::InvalidCoordinates),
                "{lat},{lon}"
            );
        }
        let f = VenueForm {
            latitude: " 45.5 ".into(),
            longitude: "-7.25".into(),
            ..form("G")
        };
        assert_eq!(f.validate().unwrap().coordinates, Some((45.5, -7.25)));
    }

    #[test]
    fn the_link_gives_the_coordinates_unless_they_are_written() {
        let link = "https://www.google.com/maps/@45.0716,7.6869,17z";
        let from_link = VenueForm {
            map_url: format!(" {link} "),
            ..form("G")
        };
        let input = from_link.clone().validate().unwrap();
        assert_eq!(input.coordinates, Some((45.0716, 7.6869)));
        assert_eq!(input.map_url, link);
        let written = VenueForm {
            latitude: "10".into(),
            longitude: "20".into(),
            ..from_link
        };
        assert_eq!(written.validate().unwrap().coordinates, Some((10.0, 20.0)));
        // a short link is kept, with no coordinates, and the form can say why
        let short = VenueForm {
            map_url: "https://maps.app.goo.gl/x".into(),
            ..form("G")
        };
        assert_eq!(short.link_status(), Err(LinkError::Short));
        let input = short.validate().unwrap();
        assert_eq!(input.coordinates, None);
        assert_eq!(input.map_url, "https://maps.app.goo.gl/x");
    }

    #[test]
    fn every_error_has_a_message_in_every_language() {
        for language in crate::shapes::enums::LanguageEnum::ALL {
            let l = crate::localization::labels(language);
            for error in [
                VenueFormError::EmptyName,
                VenueFormError::InvalidCoordinates,
            ] {
                assert!(!error.message(l).is_empty());
            }
            for error in [
                LinkError::Short,
                LinkError::NoCoordinates,
                LinkError::OutOfRange,
            ] {
                assert!(!error.message(l).is_empty());
            }
            assert!(!l.deleted_venue.is_empty());
            assert!(l.venue_busy.matches("{}").count() == 4);
        }
    }

    fn venue(coordinates: Option<(f64, f64)>, map_url: &str) -> Venue {
        Venue {
            id: Uuid::nil(),
            name: "Palasport".into(),
            address: String::new(),
            map_url: map_url.into(),
            coordinates,
            notes: String::new(),
            pinned: false,
            deleted: false,
        }
    }

    #[test]
    fn a_gym_opens_in_the_map() {
        let with = venue(Some((45.5, 7.5)), "");
        assert_eq!(
            geo_uri(&with).unwrap(),
            "geo:45.5,7.5?q=45.5,7.5(Palasport)"
        );
        assert!(web_url(&with).unwrap().contains("mlat=45.5&mlon=7.5"));
        let linked = venue(Some((45.5, 7.5)), "https://example.org/m");
        assert_eq!(web_url(&linked).unwrap(), "https://example.org/m");
        let nothing = venue(None, "");
        assert_eq!((geo_uri(&nothing), web_url(&nothing)), (None, None));
        // a link and no coordinates: the browser still has it
        assert!(web_url(&venue(None, "https://maps.app.goo.gl/x")).is_some());
    }

    // the provider

    struct W {
        p: Sqlite,
        venues: SqliteVenues,
        training: SqliteTraining,
        teams: SqliteTeamWriter,
    }

    fn world() -> W {
        let p = Sqlite::new();
        W {
            venues: SqliteVenues::new(p.db.clone()),
            training: SqliteTraining::new(p.db.clone()),
            teams: SqliteTeamWriter::new(p.db.clone()),
            p,
        }
    }

    async fn team(w: &W, name: &str) -> TeamEntry {
        w.teams
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

    fn gym(w: &W, name: &str) -> Venue {
        w.venues.create(VenueInput::named(name).unwrap()).unwrap()
    }

    fn at(day: u32, hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, day)
            .and_then(|d| d.and_hms_opt(hour, minute, 0))
            .unwrap()
    }

    async fn session(
        w: &W,
        team: &TeamEntry,
        start: NaiveDateTime,
        minutes: u16,
        venue: Option<Uuid>,
    ) -> Uuid {
        w.training
            .create_session(
                &team.id,
                SessionForm {
                    start: Some(start),
                    duration: minutes.to_string(),
                    venue,
                    ..SessionForm::default()
                }
                .validate()
                .unwrap(),
            )
            .await
            .unwrap()
            .id
    }

    #[test]
    fn a_gym_is_saved_changed_and_removed() {
        let w = world();
        let made = w
            .venues
            .create(VenueInput {
                coordinates: Some((45.5, 7.5)),
                address: "Via Roma 1".into(),
                notes: "key at the desk".into(),
                ..form("Palasport").validate().unwrap()
            })
            .unwrap();
        assert_eq!(w.venues.venue(&made.id).unwrap(), made);
        assert_eq!(made.coordinates, Some((45.5, 7.5)));
        let changed = w
            .venues
            .update(&made.id, form("Palasport Nord").validate().unwrap())
            .unwrap();
        assert_eq!(
            (changed.name.as_str(), changed.coordinates),
            ("Palasport Nord", None)
        );
        assert_eq!(w.venues.venues().unwrap(), std::slice::from_ref(&changed));
        w.venues.remove(&made.id).unwrap();
        assert!(w.venues.venues().unwrap().is_empty());
        // removed, it is still there to be read, and cannot change
        assert!(w.venues.venue(&made.id).unwrap().deleted);
        assert!(
            w.venues
                .update(&made.id, VenueInput::named("x").unwrap())
                .is_err()
        );
        assert!(w.venues.remove(&made.id).is_err());
        assert!(w.venues.venue(&Uuid::new_v4()).is_err());
    }

    #[test]
    fn the_pin_is_in_the_database_for_every_team() {
        let w = world();
        let a = gym(&w, "A gym");
        let b = gym(&w, "B gym");
        assert!(w.venues.set_pinned(&b.id, true).unwrap().pinned);
        // another connection on the same data sees it
        let other = SqliteVenues::new(w.p.db.clone());
        assert_eq!(
            other
                .venues()
                .unwrap()
                .iter()
                .map(|v| (v.id, v.pinned))
                .collect::<Vec<_>>(),
            [(b.id, true), (a.id, false)]
        );
        w.venues.set_pinned(&b.id, false).unwrap();
        assert!(w.venues.venues().unwrap().iter().all(|v| !v.pinned));
        assert!(w.venues.set_pinned(&Uuid::new_v4(), true).is_err());
    }

    #[tokio::test]
    async fn the_choice_puts_the_pinned_first_then_the_used_then_the_others() {
        let w = world();
        let a = team(&w, "A").await;
        let b = team(&w, "B").await;
        let (alpha, beta, gamma, delta, epsilon) = (
            gym(&w, "Alpha"),
            gym(&w, "Beta"),
            gym(&w, "Gamma"),
            gym(&w, "delta"),
            gym(&w, "Epsilon"),
        );
        w.venues.set_pinned(&epsilon.id, true).unwrap();
        // A used Gamma twice and Beta once; B used Delta
        session(&w, &a, at(2, 18, 0), 60, Some(gamma.id)).await;
        session(&w, &a, at(3, 18, 0), 60, Some(gamma.id)).await;
        session(&w, &a, at(4, 18, 0), 60, Some(beta.id)).await;
        session(&w, &b, at(2, 18, 0), 60, Some(delta.id)).await;
        let names = |v: Vec<Venue>| v.into_iter().map(|v| v.name).collect::<Vec<_>>();
        assert_eq!(
            names(w.venues.choices(&a.id, "").unwrap()),
            ["Epsilon", "Gamma", "Beta", "Alpha", "delta"]
        );
        assert_eq!(
            names(w.venues.choices(&b.id, "").unwrap()),
            ["Epsilon", "delta", "Alpha", "Beta", "Gamma"]
        );
        // by name, whatever the case
        assert_eq!(names(w.venues.choices(&a.id, "ETA").unwrap()), ["Beta"]);
        assert_eq!(names(w.venues.choices(&a.id, " a").unwrap()).len(), 4);
        // a removed one is not offered, the sessions keep it
        w.venues.remove(&gamma.id).unwrap();
        assert_eq!(
            names(w.venues.choices(&a.id, "").unwrap()),
            ["Epsilon", "Beta", "Alpha", "delta"]
        );
        assert_eq!(
            w.training
                .sessions(&a.id)
                .await
                .unwrap()
                .iter()
                .filter(|s| s.venue_id == Some(gamma.id))
                .count(),
            2
        );
        let _ = alpha;
    }

    // the warnings

    #[tokio::test]
    async fn two_teams_in_the_same_gym_at_the_same_time_are_warned_not_refused() {
        let w = world();
        let (a, b) = (team(&w, "U19F").await, team(&w, "U17M").await);
        let g = gym(&w, "Terontola");
        session(&w, &a, at(23, 18, 30), 90, Some(g.id)).await;
        // the other team asks for 19:00–20:00 in the same gym
        let clashes = w
            .venues
            .clashes(&g.id, &[(at(23, 19, 0), 60)], &[b.id])
            .unwrap();
        assert_eq!(clashes.len(), 1);
        assert_eq!(clashes[0].team_id, a.id);
        assert_eq!(clashes[0].team_name, "U19F");
        assert_eq!(clashes[0].kind, SlotKind::Session);
        assert_eq!(clashes[0].start, at(23, 18, 30));
        assert_eq!(
            clashes[0].message(EN, "Terontola"),
            "Terontola is busy: U19F 18:30–20:00"
        );
        // and it is saved all the same
        session(&w, &b, at(23, 19, 0), 60, Some(g.id)).await;
        // touching is not overlapping; another gym, another day, no
        assert!(
            w.venues
                .clashes(&g.id, &[(at(23, 20, 0), 60)], &[b.id])
                .unwrap()
                .is_empty()
        );
        assert!(
            w.venues
                .clashes(&g.id, &[(at(23, 17, 30), 60)], &[b.id])
                .unwrap()
                .is_empty()
        );
        assert!(
            w.venues
                .clashes(&g.id, &[(at(24, 19, 0), 60)], &[b.id])
                .unwrap()
                .is_empty()
        );
        let other = gym(&w, "Montecchio");
        assert!(
            w.venues
                .clashes(&other.id, &[(at(23, 19, 0), 60)], &[b.id])
                .unwrap()
                .is_empty()
        );
        // the sessions of the team itself are another matter (a conflict)
        assert!(
            w.venues
                .clashes(&g.id, &[(at(23, 19, 0), 60)], &[a.id, b.id])
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn a_cancelled_session_holds_nothing() {
        let w = world();
        let (a, b) = (team(&w, "A").await, team(&w, "B").await);
        let g = gym(&w, "G");
        let s = session(&w, &a, at(23, 18, 0), 90, Some(g.id)).await;
        let busy = |w: &W| {
            w.venues
                .clashes(&g.id, &[(at(23, 18, 30), 30)], &[b.id])
                .unwrap()
        };
        assert_eq!(busy(&w).len(), 1);
        w.training
            .set_session_status(&s, SessionStatus::Cancelled)
            .await
            .unwrap();
        assert!(busy(&w).is_empty());
        w.training
            .set_session_status(&s, SessionStatus::Planned)
            .await
            .unwrap();
        assert_eq!(busy(&w).len(), 1);
        w.training.remove_session(&s).await.unwrap();
        assert!(busy(&w).is_empty());
    }

    fn match_input(
        _w: &W,
        _team: &TeamEntry,
        day: u32,
        venue: Option<Uuid>,
    ) -> crate::match_form::MatchInput {
        let date =
            chrono::DateTime::parse_from_rfc3339(&format!("2026-11-{day:02}T19:00:00+01:00"))
                .unwrap();
        crate::match_form::MatchInput {
            venue_id: venue,
            ..crate::match_form::MatchInput::against("Them", date, true, 6, "", vec![])
        }
    }

    #[tokio::test]
    async fn a_match_in_programma_holds_the_gym_a_played_one_does_not() {
        let w = world();
        let (a, b) = (team(&w, "A").await, team(&w, "B").await);
        let g = gym(&w, "G");
        let m =
            w.p.match_writer
                .create(&a, match_input(&w, &a, 23, Some(g.id)))
                .await
                .unwrap();
        let busy = |w: &W| {
            w.venues
                .clashes(&g.id, &[(at(23, 19, 30), 30)], &[b.id])
                .unwrap()
        };
        let clashes = busy(&w);
        assert_eq!(clashes.len(), 1);
        assert_eq!(
            (clashes[0].kind, clashes[0].team_id),
            (SlotKind::Match, a.id)
        );
        // 120 minutes when nothing says: until 21:00
        assert_eq!(clashes[0].minutes, 120);
        // a match with a set was played: it holds nothing
        w.p.db
            .write(|tx| {
                tx.execute(
                    "INSERT INTO sets (match_id, number, serving, lineup_rules, scouting_rules)
                     VALUES (?1, 1, 'a', 'v2', 'v4')",
                    [&m.id],
                )
                .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        assert!(busy(&w).is_empty());
    }

    #[tokio::test]
    async fn the_gym_of_a_match_is_saved_and_changed_with_the_form() {
        let w = world();
        let a = team(&w, "A").await;
        let (g, h) = (gym(&w, "G"), gym(&w, "H"));
        let m =
            w.p.match_writer
                .create(&a, match_input(&w, &a, 23, Some(g.id)))
                .await
                .unwrap();
        let read = |w: &W, id: &str| -> Option<String> {
            w.p.db
                .read(|tx| {
                    tx.query_row("SELECT venue_id FROM matches WHERE id = ?1", [id], |r| {
                        r.get(0)
                    })
                    .map_err(crate::providers::sqlite::sqlite)
                })
                .unwrap()
        };
        assert_eq!(read(&w, &m.id), Some(g.id.to_string()));
        w.p.match_writer
            .update(&m, match_input(&w, &a, 23, Some(h.id)))
            .await
            .unwrap();
        assert_eq!(read(&w, &m.id), Some(h.id.to_string()));
        w.p.match_writer
            .update(&m, match_input(&w, &a, 23, None))
            .await
            .unwrap();
        assert_eq!(read(&w, &m.id), None);
        // the form of the match gives it back
        w.p.match_writer
            .update(&m, match_input(&w, &a, 23, Some(g.id)))
            .await
            .unwrap();
        let rows =
            w.p.db
                .read(|tx| {
                    crate::providers::sqlite::rows::match_rows(tx, Uuid::parse_str(&m.id).unwrap())
                })
                .unwrap()
                .unwrap();
        assert_eq!(rows.row.venue_id, Some(g.id));
        let entry = rows.entry(&a, None).unwrap();
        assert_eq!(MatchForm::of(&entry, &rows.row, None).venue, Some(g.id));
    }

    fn band(weekday: Weekday, venue: Option<Uuid>) -> Band {
        Band {
            weekday,
            time: NaiveTime::from_hms_opt(18, 30, 0).unwrap(),
            duration_min: 90,
            venue_id: venue,
        }
    }

    #[tokio::test]
    async fn a_series_lists_the_dates_where_its_gym_is_held() {
        let w = world();
        let (a, b) = (team(&w, "A").await, team(&w, "B").await);
        let (terontola, montecchio) = (gym(&w, "Terontola"), gym(&w, "Montecchio"));
        // B holds Terontola on Wednesday the 25th and Montecchio on Thursday the 26th
        session(&w, &b, at(25, 18, 0), 90, Some(terontola.id)).await;
        session(&w, &b, at(26, 18, 0), 90, Some(montecchio.id)).await;
        let input = SeriesInput {
            from: NaiveDate::from_ymd_opt(2026, 11, 23).unwrap(),
            to: NaiveDate::from_ymd_opt(2026, 11, 29).unwrap(),
            bands: vec![
                band(Weekday::Mon, Some(terontola.id)),
                band(Weekday::Wed, Some(terontola.id)),
                // the gym is of the band: Thursday is somewhere else
                band(Weekday::Thu, Some(terontola.id)),
                band(Weekday::Fri, None),
            ],
        };
        let found = w
            .venues
            .series_clashes(&a.id, std::slice::from_ref(&input))
            .unwrap();
        assert_eq!(
            found
                .iter()
                .map(|c| (c.window, c.team_name.as_str()))
                .collect::<Vec<_>>(),
            [(at(25, 18, 30), "B")]
        );
        // and the series is created all the same, each session in the gym of its band
        let outcome = w
            .training
            .create_series(&a.id, vec![input], false)
            .await
            .unwrap();
        assert_eq!(outcome.created.len(), 4);
        let venues: Vec<Option<Uuid>> = outcome.created.iter().map(|s| s.venue_id).collect();
        assert_eq!(
            venues,
            [
                Some(terontola.id),
                Some(terontola.id),
                Some(terontola.id),
                None
            ]
        );
    }

    #[tokio::test]
    async fn a_copy_lists_where_its_gym_is_held() {
        let w = world();
        let (a, b) = (team(&w, "A").await, team(&w, "B").await);
        let g = gym(&w, "G");
        let original = session(&w, &a, at(23, 18, 0), 90, Some(g.id)).await;
        session(&w, &b, at(30, 18, 30), 60, Some(g.id)).await;
        let found = w
            .venues
            .duplicate_clashes(&original, &DuplicateInput::proposed(at(23, 18, 0), None))
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].window, at(30, 18, 0));
        // the copy keeps the gym
        let copy = w
            .training
            .duplicate_session(&original, DuplicateInput::proposed(at(23, 18, 0), None))
            .await
            .unwrap();
        assert_eq!(copy.venue_id, Some(g.id));
    }

    #[tokio::test]
    async fn the_copies_of_a_microcycle_list_where_their_gym_is_held() {
        use crate::{
            providers::training_cycles::TrainingCycles,
            training::cycles::{CycleForm, Level},
        };
        let w = world();
        let (a, b) = (team(&w, "A").await, team(&w, "B").await);
        let g = gym(&w, "G");
        let micro = w
            .training
            .create_cycle(
                &a.id,
                Level::Microcycle,
                CycleForm {
                    start: NaiveDate::from_ymd_opt(2026, 11, 23),
                    weeks: "1".into(),
                    ..CycleForm::default()
                }
                .validate(Level::Microcycle)
                .unwrap(),
                vec![],
            )
            .await
            .unwrap();
        w.training
            .create_session(
                &a.id,
                SessionForm {
                    start: Some(at(24, 18, 0)),
                    microcycle: Some(micro.id),
                    venue: Some(g.id),
                    ..SessionForm::default()
                }
                .validate()
                .unwrap(),
            )
            .await
            .unwrap();
        // the second copy lands on December 1st
        session(
            &w,
            &b,
            at(24, 18, 0) + chrono::Duration::weeks(2),
            90,
            Some(g.id),
        )
        .await;
        let found = w
            .venues
            .microcycle_clashes(
                &micro.id,
                &DuplicateCycleInput {
                    shift_weeks: 1,
                    copies: 2,
                    copy_objectives: false,
                    copy_activities: false,
                },
            )
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].window, at(24, 18, 0) + chrono::Duration::weeks(2));
    }

    // what the agenda and the gym show

    #[tokio::test]
    async fn the_agenda_names_the_gym_and_marks_the_ones_shared() {
        let w = world();
        let (a, b) = (team(&w, "A").await, team(&w, "B").await);
        let g = gym(&w, "Terontola");
        let h = gym(&w, "Montecchio");
        let one = session(&w, &a, at(23, 18, 0), 90, Some(g.id)).await;
        let two = session(&w, &b, at(23, 19, 0), 90, Some(g.id)).await;
        let three = session(&w, &b, at(24, 19, 0), 90, Some(h.id)).await;
        let four = session(&w, &a, at(24, 19, 30), 60, None).await;
        let period = Period::week(NaiveDate::from_ymd_opt(2026, 11, 23).unwrap());
        let events = w.training.agenda(None, period, at(1, 0, 0)).await.unwrap();
        let find = |id: Uuid| events.iter().find(|e| e.item.id() == id).unwrap();
        assert!(find(one).shared_venue && find(two).shared_venue);
        assert!(!find(three).shared_venue && !find(four).shared_venue);
        assert_eq!(
            find(one).venue.as_ref().map(|v| v.name.as_str()),
            Some("Terontola")
        );
        assert!(find(four).venue.is_none());
        // seen from one team it is shared all the same
        let mine = w
            .training
            .agenda(Some(&a.id), period, at(1, 0, 0))
            .await
            .unwrap();
        assert!(
            mine.iter()
                .find(|e| e.item.id() == one)
                .unwrap()
                .shared_venue
        );
        assert_eq!(mine.len(), 2);
        // a gym removed is named, and marked
        w.venues.remove(&h.id).unwrap();
        let events = w.training.agenda(None, period, at(1, 0, 0)).await.unwrap();
        assert!(
            events
                .iter()
                .find(|e| e.item.id() == three)
                .unwrap()
                .venue
                .as_ref()
                .unwrap()
                .deleted
        );
        // cancelled, it holds nothing: the other is no longer shared
        w.training
            .set_session_status(&two, SessionStatus::Cancelled)
            .await
            .unwrap();
        let events = w.training.agenda(None, period, at(1, 0, 0)).await.unwrap();
        assert!(
            !events
                .iter()
                .find(|e| e.item.id() == one)
                .unwrap()
                .shared_venue
        );
        assert!(
            !events
                .iter()
                .find(|e| e.item.id() == two)
                .unwrap()
                .shared_venue
        );
    }

    #[tokio::test]
    async fn the_week_of_a_gym_has_every_team_and_marks_the_overlaps() {
        let w = world();
        let (a, b) = (team(&w, "A").await, team(&w, "B").await);
        let g = gym(&w, "G");
        let h = gym(&w, "H");
        session(&w, &a, at(23, 18, 0), 90, Some(g.id)).await;
        session(&w, &b, at(23, 19, 0), 90, Some(g.id)).await;
        session(&w, &b, at(25, 19, 0), 90, Some(g.id)).await;
        session(&w, &b, at(24, 19, 0), 90, Some(h.id)).await;
        session(&w, &b, at(30, 19, 0), 90, Some(g.id)).await;
        let week = w
            .venues
            .week(
                &g.id,
                &Period::week(NaiveDate::from_ymd_opt(2026, 11, 23).unwrap()),
            )
            .unwrap();
        assert_eq!(
            week.iter()
                .map(|(o, overlapping)| (o.slot.start, *overlapping))
                .collect::<Vec<_>>(),
            [
                (at(23, 18, 0), true),
                (at(23, 19, 0), true),
                (at(25, 19, 0), false)
            ]
        );
    }

    #[tokio::test]
    async fn the_gym_of_a_dvw_file_is_found_by_its_name_or_made_not_pinned() {
        let w = world();
        let a = team(&w, "A").await;
        let known = w
            .venues
            .create(VenueInput {
                pinned: true,
                ..VenueInput::named("Palasport Terontola").unwrap()
            })
            .unwrap();
        let m1 =
            w.p.match_writer
                .create(&a, match_input(&w, &a, 23, None))
                .await
                .unwrap();
        let m2 =
            w.p.match_writer
                .create(&a, match_input(&w, &a, 24, None))
                .await
                .unwrap();
        let m3 =
            w.p.match_writer
                .create(&a, match_input(&w, &a, 25, None))
                .await
                .unwrap();
        // the same name, whatever the case, the accents and the spaces
        w.p.match_writer
            .save_venue_named(&m1, "  PALASPORT   terontola ", "Cortona")
            .await
            .unwrap();
        // a name nobody has: a new gym with the city for address, not pinned
        w.p.match_writer
            .save_venue_named(&m2, "OŠ Braslovče", "Braslovče")
            .await
            .unwrap();
        // the same one again: the one made, not another
        w.p.match_writer
            .save_venue_named(&m3, "os braslovce", "")
            .await
            .unwrap();
        let all = w.venues.venues().unwrap();
        assert_eq!(all.len(), 2);
        let made = all.iter().find(|v| v.name == "OŠ Braslovče").unwrap();
        assert_eq!((made.address.as_str(), made.pinned), ("Braslovče", false));
        // the one known stays as it was
        let still = w.venues.venue(&known.id).unwrap();
        assert_eq!(
            (still.name.as_str(), still.address.as_str(), still.pinned),
            ("Palasport Terontola", "", true)
        );
        let of = |id: &str| -> Option<String> {
            w.p.db
                .read(|tx| {
                    tx.query_row("SELECT venue_id FROM matches WHERE id = ?1", [id], |r| {
                        r.get(0)
                    })
                    .map_err(crate::providers::sqlite::sqlite)
                })
                .unwrap()
        };
        assert_eq!(of(&m1.id), Some(known.id.to_string()));
        assert_eq!(of(&m2.id), Some(made.id.to_string()));
        assert_eq!(of(&m3.id), Some(made.id.to_string()));
        // a gym removed is not found: a new one is made
        w.venues.remove(&known.id).unwrap();
        w.p.match_writer
            .save_venue_named(&m1, "Palasport Terontola", "")
            .await
            .unwrap();
        assert_ne!(of(&m1.id), Some(known.id.to_string()));
    }

    // the archives

    #[tokio::test]
    async fn the_gym_of_a_match_goes_with_the_archive_of_the_team_and_comes_back() {
        use crate::providers::sqlite::archive::{Subject, export, import};
        let w = world();
        let a = team(&w, "A").await;
        let g = w
            .venues
            .create(VenueInput {
                pinned: true,
                address: "Via Roma 1".into(),
                ..VenueInput::named("Terontola").unwrap()
            })
            .unwrap();
        let other = gym(&w, "Never used");
        let m =
            w.p.match_writer
                .create(&a, match_input(&w, &a, 23, Some(g.id)))
                .await
                .unwrap();
        let archive = export(&w.p.db, Subject::TeamWithMatches(a.id)).unwrap();
        let there = crate::tests::sqlite_archive_tests::tests::database();
        import(&there, &archive, &crate::localization::Language::default()).unwrap();
        let rows: Vec<(String, i64)> = there
            .read(|tx| {
                tx.prepare("SELECT id, pinned FROM venues")
                    .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
                    .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        // only the gym the match used, pinned as it was
        assert_eq!(rows, [(g.id.to_string(), 1)]);
        assert_ne!(other.id, g.id);
        let venue: Option<String> = there
            .read(|tx| {
                tx.query_row("SELECT venue_id FROM matches WHERE id = ?1", [&m.id], |r| {
                    r.get(0)
                })
                .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        assert_eq!(venue, Some(g.id.to_string()));
        // the gym of a match of the archive is the local one, when it is here with other data
        crate::tests::sqlite_archive_tests::tests::execute(
            &there,
            &format!(
                "UPDATE venues SET name = 'mine', pinned = 0 WHERE id = '{}'",
                g.id
            ),
        );
        let again =
            w.p.match_writer
                .create(&a, match_input(&w, &a, 24, Some(g.id)))
                .await
                .unwrap();
        let archive = export(&w.p.db, Subject::TeamWithMatches(a.id)).unwrap();
        import(&there, &archive, &crate::localization::Language::default()).unwrap();
        let (name, pinned): (String, i64) = there
            .read(|tx| {
                tx.query_row("SELECT name, pinned FROM venues", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        assert_eq!((name.as_str(), pinned), ("mine", 0));
        let count: i64 = there
            .read(|tx| {
                tx.query_row(
                    "SELECT count(*) FROM matches WHERE id IN (?1, ?2) AND venue_id = ?3",
                    [&m.id, &again.id, &g.id.to_string()],
                    |r| r.get(0),
                )
                .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        assert_eq!(count, 2);
    }

    /// The versions before the gyms read the archive of a match with none:
    /// no file, and no column in the matches.
    #[tokio::test]
    async fn a_match_in_no_gym_is_archived_as_before_the_gyms() {
        use crate::providers::sqlite::archive::{Subject, export};
        let w = world();
        let a = team(&w, "A").await;
        w.p.match_writer
            .create(&a, match_input(&w, &a, 23, None))
            .await
            .unwrap();
        let archive = export(&w.p.db, Subject::TeamWithMatches(a.id)).unwrap();
        let files = crate::legacy::migrate::unzip(&archive).unwrap();
        assert!(files.iter().all(|(name, _)| name != "venues.json"));
        let matches = files
            .iter()
            .find(|(name, _)| name == "matches.json")
            .unwrap();
        let text = String::from_utf8(matches.1.clone()).unwrap();
        assert!(!text.contains("venue_id"), "{text}");
        // and with today's schema, because all person columns are exported
        // even when there is no training or venue
        let manifest = files
            .iter()
            .find(|(name, _)| name == "manifest.json")
            .unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&manifest.1).unwrap();
        assert_eq!(manifest["schema"], 8);
    }

    /// A match in a gym needs the schema of the gyms: the versions before
    /// ask to update instead of losing the gym.
    #[tokio::test]
    async fn a_match_in_a_gym_is_archived_with_the_schema_of_the_gyms() {
        use crate::providers::sqlite::archive::{Subject, export};
        let w = world();
        let a = team(&w, "A").await;
        let gym = w
            .venues
            .create(VenueInput::named("Terontola").unwrap())
            .unwrap();
        w.p.match_writer
            .create(&a, match_input(&w, &a, 23, Some(gym.id)))
            .await
            .unwrap();
        let archive = export(&w.p.db, Subject::TeamWithMatches(a.id)).unwrap();
        let files = crate::legacy::migrate::unzip(&archive).unwrap();
        let manifest = files
            .iter()
            .find(|(name, _)| name == "manifest.json")
            .unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(&manifest.1).unwrap();
        assert_eq!(manifest["schema"], crate::db::schema_version());
    }

    // the schema and the migration

    #[test]
    fn the_schema_refuses_a_gym_without_a_name_or_with_half_the_coordinates() {
        let db = db::open_in_memory().unwrap();
        let id = "11111111-0000-4000-8000-000000000001";
        let fails = |sql: &str| assert!(db.execute_batch(sql).is_err(), "accepted: {sql}");
        let passes = |sql: &str| {
            db.execute_batch(sql)
                .unwrap_or_else(|e| panic!("{sql}: {e}"))
        };
        fails(&format!(
            "INSERT INTO venues (id, name) VALUES ('{id}', '')"
        ));
        fails(&format!(
            "INSERT INTO venues (id, name, latitude) VALUES ('{id}', 'g', 45)"
        ));
        fails(&format!(
            "INSERT INTO venues (id, name, longitude) VALUES ('{id}', 'g', 7)"
        ));
        fails(&format!(
            "INSERT INTO venues (id, name, latitude, longitude) VALUES ('{id}', 'g', 91, 7)"
        ));
        fails(&format!(
            "INSERT INTO venues (id, name, latitude, longitude) VALUES ('{id}', 'g', 45, -181)"
        ));
        fails(&format!(
            "INSERT INTO venues (id, name, pinned) VALUES ('{id}', 'g', 2)"
        ));
        passes(&format!(
            "INSERT INTO venues (id, name, latitude, longitude) VALUES ('{id}', 'g', 45, 7)"
        ));
        // a session or a match refers to a gym that exists
        fails(&format!(
            "INSERT INTO training_sessions (id, team_id, start_local, duration_min, venue_id)
             VALUES ('{id}', '{id}', '2026-11-23T18:00', 90, '{id}')"
        ));
    }

    #[test]
    fn a_database_of_v5_migrates_with_its_matches_in_no_gym() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.sqlite");
        let (a, b) = (
            "aaaaaaaa-0000-4000-8000-000000000001",
            "aaaaaaaa-0000-4000-8000-000000000002",
        );
        db::open_with(&path, &MIGRATIONS[..5])
            .unwrap()
            .execute_batch(&format!(
                "INSERT INTO teams (id, name, year) VALUES ('{a}', 'A', 2026), ('{b}', 'B', 2026);
                 INSERT INTO matches (id, team_a, team_b, home, scouting, start, max_substitutions)
                     VALUES ('dddddddd-0000-4000-8000-000000000004', '{a}', '{b}', 'a', 'both',
                             '2026-11-12T18:00:00+01:00', 6);
                 INSERT INTO training_sessions (id, team_id, start_local, duration_min)
                     VALUES ('bbbbbbbb-0000-4000-8000-000000000001', '{a}', '2026-11-13T18:00', 90);"
            ))
            .unwrap();
        let db = db::open(&path).unwrap();
        assert_eq!(db::user_version(&db).unwrap(), db::schema_version());
        let venue: Option<String> = db
            .query_row("SELECT venue_id FROM matches", [], |r| r.get(0))
            .unwrap();
        assert_eq!(venue, None);
        let sessions: i64 = db
            .query_row("SELECT count(*) FROM training_sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sessions, 1);
        // and the gyms are there to be used
        db.execute_batch(
            "INSERT INTO venues (id, name) VALUES ('11111111-0000-4000-8000-000000000001', 'g');
             UPDATE matches SET venue_id = '11111111-0000-4000-8000-000000000001';",
        )
        .unwrap();
    }
}
