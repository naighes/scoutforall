//! The gyms for the apps (TODO/021 §13): where sessions and matches are
//! held, common to every team.
use super::{
    MatchDate, MobileError, Scout4All, parse_id,
    training::{
        AgendaSpan, DuplicateCycleDraft, DuplicateDraft, GuestChoice, SeriesDraft, TrainingField,
        day, moment, parse_day, parse_moment, series_input,
    },
};
use scout4all_core::{
    errors::AppError,
    providers::sqlite::{persons::SqlitePersons, venues::SqliteVenues},
    training::{
        agenda::Period,
        duplicate::{DuplicateCycleInput, DuplicateInput},
        overlap::{DEFAULT_MATCH_MINUTES, SlotKind},
        venue::{
            Occupation, Venue, VenueClash, VenueForm, VenueFormError, VenueInput, geo_uri, web_url,
        },
    },
};
use uuid::Uuid;

/// A gym. The coordinates come together.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct VenueRecord {
    pub id: String,
    pub name: String,
    pub address: String,
    pub map_url: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub notes: String,
    pub pinned: bool,
    /// removed: sessions and matches keep it, the choice doesn't offer it
    pub deleted: bool,
}

pub(crate) fn venue_record(v: &Venue) -> VenueRecord {
    VenueRecord {
        id: v.id.to_string(),
        name: v.name.clone(),
        address: v.address.clone(),
        map_url: v.map_url.clone(),
        latitude: v.coordinates.map(|(lat, _)| lat),
        longitude: v.coordinates.map(|(_, lon)| lon),
        notes: v.notes.clone(),
        pinned: v.pinned,
        deleted: v.deleted,
    }
}

#[uniffi::export]
impl Scout4All {
    /// The gyms not removed: the pinned first, then by name.
    pub fn venues(&self) -> Result<Vec<VenueRecord>, MobileError> {
        Ok(SqliteVenues::new(self.db.clone())
            .venues()?
            .iter()
            .map(venue_record)
            .collect())
    }

    /// The gym, removed or not.
    pub fn venue(&self, id: String) -> Result<VenueRecord, MobileError> {
        Ok(venue_record(
            &SqliteVenues::new(self.db.clone()).venue(&parse_id(&id)?)?,
        ))
    }

    /// What a form of the team offers whose name holds `query`: the pinned
    /// first, then the ones the team used (the most used first), then the
    /// others.
    pub fn venue_choices(
        &self,
        team_id: String,
        query: String,
    ) -> Result<Vec<VenueRecord>, MobileError> {
        Ok(SqliteVenues::new(self.db.clone())
            .choices(&parse_id(&team_id)?, &query)?
            .iter()
            .map(venue_record)
            .collect())
    }
}

/// What the user typed for a gym: the coordinates are text, as typed.
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct VenueDraft {
    pub name: String,
    pub address: String,
    pub map_url: String,
    pub latitude: String,
    pub longitude: String,
    pub notes: String,
    pub pinned: bool,
}

impl From<VenueDraft> for VenueForm {
    fn from(d: VenueDraft) -> Self {
        Self {
            name: d.name,
            address: d.address,
            map_url: d.map_url,
            latitude: d.latitude,
            longitude: d.longitude,
            notes: d.notes,
            pinned: d.pinned,
        }
    }
}

impl From<VenueForm> for VenueDraft {
    fn from(f: VenueForm) -> Self {
        Self {
            name: f.name,
            address: f.address,
            map_url: f.map_url,
            latitude: f.latitude,
            longitude: f.longitude,
            notes: f.notes,
            pinned: f.pinned,
        }
    }
}

/// The coordinates a link of a map gives.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct VenueCoordinates {
    pub latitude: f64,
    pub longitude: f64,
}

/// Another team holding a gym at the same time as what is saved: allowed,
/// the user is told first.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VenueClashRecord {
    pub venue_id: String,
    /// the start of what is saved that clashes
    pub window: String,
    pub team_id: String,
    pub team_name: String,
    pub is_match: bool,
    pub start: String,
    pub minutes: u16,
    /// "Palestra X è occupata: U19F 18:30–20:00", in the language of the app
    pub message: String,
}

/// A session or a match holding a gym.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OccupationRecord {
    /// the id of the session or of the match
    pub item_id: String,
    pub is_match: bool,
    /// the teams that are there: one for a session, both for a match of two
    /// teams of the user
    pub teams: Vec<VenueTeamRecord>,
    pub start: String,
    pub minutes: u16,
    /// another team holds the gym at the same time
    pub overlapping: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VenueTeamRecord {
    pub id: String,
    pub name: String,
}

/// A day of the week of a gym, empty too.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VenueDay {
    pub date: String,
    pub occupations: Vec<OccupationRecord>,
}

fn venue_field(e: VenueFormError) -> TrainingField {
    match e {
        VenueFormError::EmptyName => TrainingField::VenueName,
        VenueFormError::InvalidCoordinates => TrainingField::Coordinates,
    }
}

fn occupation_record(o: &Occupation, overlapping: bool) -> OccupationRecord {
    OccupationRecord {
        item_id: o.item.to_string(),
        is_match: o.slot.kind == SlotKind::Match,
        teams: o
            .teams
            .iter()
            .map(|(id, name)| VenueTeamRecord {
                id: id.to_string(),
                name: name.clone(),
            })
            .collect(),
        start: moment(o.slot.start),
        minutes: o.slot.minutes,
        overlapping,
    }
}

impl Scout4All {
    fn venue_provider(&self) -> SqliteVenues {
        SqliteVenues::new(self.db.clone())
    }

    /// The warnings of the clashes, each with the name of its gym.
    fn clash_records(&self, clashes: &[VenueClash]) -> Result<Vec<VenueClashRecord>, AppError> {
        let venues = self.venue_provider();
        clashes
            .iter()
            .map(|c| {
                let name = venues.venue(&c.venue)?.name;
                Ok(VenueClashRecord {
                    venue_id: c.venue.to_string(),
                    window: moment(c.window),
                    team_id: c.team_id.to_string(),
                    team_name: c.team_name.clone(),
                    is_match: c.kind == SlotKind::Match,
                    start: moment(c.start),
                    minutes: c.minutes,
                    message: c.message(self.language.labels(), &name),
                })
            })
            .collect()
    }
}

#[uniffi::export]
impl Scout4All {
    /// An empty form of a gym.
    pub fn new_venue_draft(&self) -> VenueDraft {
        VenueDraft::default()
    }

    /// The form of the gym being edited.
    pub fn venue_draft(&self, id: String) -> Result<VenueDraft, MobileError> {
        Ok(VenueForm::of(&self.venue_provider().venue(&parse_id(&id)?)?).into())
    }

    /// Creates the gym (no `id`) or updates the one with `id`. The name is
    /// needed; the coordinates come together, written or from the link.
    pub fn save_venue(
        &self,
        id: Option<String>,
        draft: VenueDraft,
    ) -> Result<VenueRecord, MobileError> {
        let input =
            VenueForm::from(draft)
                .validate()
                .map_err(|e| MobileError::InvalidTraining {
                    field: venue_field(e),
                    reason: e.message(self.language.labels()),
                })?;
        let venues = self.venue_provider();
        let venue = match id {
            Some(id) => venues.update(&parse_id(&id)?, input)?,
            None => venues.create(input)?,
        };
        Ok(venue_record(&venue))
    }

    /// A gym made on the fly in a form, with the name only.
    pub fn create_venue_named(&self, name: String) -> Result<VenueRecord, MobileError> {
        let input = VenueInput::named(&name).map_err(|e| MobileError::InvalidTraining {
            field: venue_field(e),
            reason: e.message(self.language.labels()),
        })?;
        Ok(venue_record(&self.venue_provider().create(input)?))
    }

    /// The coordinates the link gives, for the form to fill the two fields:
    /// none for an empty link, `InvalidTraining` on `MapLink` for one that
    /// gives none (a short one asks for the full link).
    pub fn venue_link_coordinates(
        &self,
        link: String,
    ) -> Result<Option<VenueCoordinates>, MobileError> {
        VenueForm {
            map_url: link,
            ..VenueForm::default()
        }
        .link_status()
        .map(|found| {
            found.map(|(latitude, longitude)| VenueCoordinates {
                latitude,
                longitude,
            })
        })
        .map_err(|e| MobileError::InvalidTraining {
            field: TrainingField::MapLink,
            reason: e.message(self.language.labels()),
        })
    }

    /// The pin, the same on every device.
    pub fn set_venue_pinned(&self, id: String, pinned: bool) -> Result<VenueRecord, MobileError> {
        Ok(venue_record(
            &self.venue_provider().set_pinned(&parse_id(&id)?, pinned)?,
        ))
    }

    /// Removes the gym: sessions and matches keep it.
    pub fn remove_venue(&self, id: String) -> Result<(), MobileError> {
        Ok(self.venue_provider().remove(&parse_id(&id)?)?)
    }

    /// The `geo:` address of the gym for the map of the phone, if it has
    /// coordinates.
    pub fn venue_geo_uri(&self, id: String) -> Result<Option<String>, MobileError> {
        Ok(geo_uri(&self.venue_provider().venue(&parse_id(&id)?)?))
    }

    /// The address of the web page of the gym on a map: its link, or
    /// OpenStreetMap with the coordinates.
    pub fn venue_web_url(&self, id: String) -> Result<Option<String>, MobileError> {
        Ok(web_url(&self.venue_provider().venue(&parse_id(&id)?)?))
    }

    /// What holds the gym in the days of the period that has `date`, every
    /// day told, with the overlaps of other teams marked.
    pub fn venue_agenda(
        &self,
        venue_id: String,
        span: AgendaSpan,
        date: String,
    ) -> Result<Vec<VenueDay>, MobileError> {
        let date = parse_day(&date, TrainingField::Start)?;
        let period = match span {
            AgendaSpan::Day => Period::day(date),
            AgendaSpan::Week => Period::week(date),
            AgendaSpan::Month => Period::month(date),
        };
        let found = self.venue_provider().week(&parse_id(&venue_id)?, &period)?;
        Ok(period
            .days()
            .into_iter()
            .map(|d| VenueDay {
                date: day(d),
                occupations: found
                    .iter()
                    .filter(|(o, _)| o.slot.start.date() == d)
                    .map(|(o, overlapping)| occupation_record(o, *overlapping))
                    .collect(),
            })
            .collect())
    }

    // ---- warnings: another team holds the gym then (a save asks first)

    /// For a session of the team in the gym at `start`; `minutes` long.
    pub fn session_venue_clashes(
        &self,
        team_id: String,
        venue_id: String,
        start: String,
        minutes: u16,
    ) -> Result<Vec<VenueClashRecord>, MobileError> {
        let clashes = self.venue_provider().clashes(
            &parse_id(&venue_id)?,
            &[(parse_moment(&start)?, minutes)],
            &[parse_id(&team_id)?],
        )?;
        Ok(self.clash_records(&clashes)?)
    }

    /// For a match of the team (and the opponent, if one of the user's) in
    /// the gym on `date`.
    pub fn match_venue_clashes(
        &self,
        team_id: String,
        opponent_team: Option<String>,
        venue_id: String,
        date: MatchDate,
    ) -> Result<Vec<VenueClashRecord>, MobileError> {
        let own: Vec<Uuid> = std::iter::once(parse_id(&team_id))
            .chain(opponent_team.as_deref().map(parse_id))
            .collect::<Result<_, _>>()?;
        let clashes = self.venue_provider().clashes(
            &parse_id(&venue_id)?,
            &[(date.naive_local(), DEFAULT_MATCH_MINUTES)],
            &own,
        )?;
        Ok(self.clash_records(&clashes)?)
    }

    /// For the sessions a series would make (all its entries).
    pub fn series_venue_clashes(
        &self,
        team_id: String,
        drafts: Vec<SeriesDraft>,
    ) -> Result<Vec<VenueClashRecord>, MobileError> {
        let inputs = drafts
            .iter()
            .map(series_input)
            .collect::<Result<Vec<_>, _>>()?;
        let clashes = self
            .venue_provider()
            .series_clashes(&parse_id(&team_id)?, &inputs)?;
        Ok(self.clash_records(&clashes)?)
    }

    /// For the copy of a session.
    pub fn duplicate_venue_clashes(
        &self,
        session_id: String,
        draft: DuplicateDraft,
    ) -> Result<Vec<VenueClashRecord>, MobileError> {
        let input = DuplicateInput {
            start: parse_moment(&draft.start)?,
            microcycle_id: None,
            copy_objectives: draft.copy_objectives,
            copy_activities: draft.copy_activities,
        };
        let clashes = self
            .venue_provider()
            .duplicate_clashes(&parse_id(&session_id)?, &input)?;
        Ok(self.clash_records(&clashes)?)
    }

    /// For the copies of a microcycle.
    pub fn microcycle_venue_clashes(
        &self,
        id: String,
        draft: DuplicateCycleDraft,
    ) -> Result<Vec<VenueClashRecord>, MobileError> {
        let clashes = self.venue_provider().microcycle_clashes(
            &parse_id(&id)?,
            &DuplicateCycleInput {
                shift_weeks: draft.shift_weeks,
                copies: draft.copies,
                copy_objectives: draft.copy_objectives,
                copy_activities: draft.copy_activities,
            },
        )?;
        Ok(self.clash_records(&clashes)?)
    }

    // ---- guests

    /// A person with the name only, to be a guest of a session when they
    /// aren't in any team: as the search of the guests offers them (no
    /// team), for `save_attendance` (guest row).
    pub fn create_guest(&self, name: String) -> Result<GuestChoice, MobileError> {
        let person = SqlitePersons::new(self.db.clone()).create(&name)?;
        Ok(GuestChoice {
            person_id: person.id.to_string(),
            name: person.name,
            teams: vec![],
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::unreachable)]
mod tests {
    use super::*;
    use crate::training::{SeriesBand, SessionDraft};
    use scout4all_core::{
        shapes::{
            country::CountryCode,
            enums::{GenderEnum, TeamClassificationEnum},
        },
        team_form::TeamForm,
    };
    use std::sync::Arc;

    fn app(dir: &tempfile::TempDir) -> Arc<Scout4All> {
        Scout4All::for_tests(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap()
    }

    fn team(app: &Scout4All, name: &str) -> String {
        app.save_team(
            None,
            TeamForm {
                name: name.into(),
                classification: Some(TeamClassificationEnum::Regional),
                gender: Some(GenderEnum::Women),
                country: CountryCode::known("IT"),
                year: "2026".into(),
            },
        )
        .unwrap()
    }

    fn gym(app: &Scout4All, name: &str) -> String {
        app.create_venue_named(name.into()).unwrap().id
    }

    fn session_in(app: &Scout4All, team: &str, gym: &str, start: &str) {
        app.save_session(
            team.into(),
            None,
            SessionDraft {
                start: Some(start.into()),
                title: "x".into(),
                venue: Some(gym.into()),
                ..app.new_session_draft()
            },
        )
        .unwrap();
    }

    #[test]
    fn a_gym_is_made_edited_pinned_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let saved = app
            .save_venue(
                None,
                VenueDraft {
                    name: " Terontola ".into(),
                    map_url: "geo:43.1,12.2".into(),
                    ..app.new_venue_draft()
                },
            )
            .unwrap();
        assert_eq!(saved.name, "Terontola");
        // the coordinates of the link
        assert_eq!(saved.latitude, Some(43.1));
        let draft = app.venue_draft(saved.id.clone()).unwrap();
        assert_eq!(draft.longitude, "12.2");
        let edited = app
            .save_venue(
                Some(saved.id.clone()),
                VenueDraft {
                    address: "Via Roma".into(),
                    ..draft
                },
            )
            .unwrap();
        assert_eq!(edited.address, "Via Roma");
        let other = gym(&app, "Alfa");
        assert_eq!(app.venues().unwrap()[0].id, other);
        app.set_venue_pinned(saved.id.clone(), true).unwrap();
        assert_eq!(app.venues().unwrap()[0].id, saved.id);
        assert_eq!(
            app.venue_geo_uri(saved.id.clone()).unwrap().as_deref(),
            Some("geo:43.1,12.2?q=43.1,12.2(Terontola)")
        );
        app.remove_venue(saved.id.clone()).unwrap();
        assert_eq!(app.venues().unwrap().len(), 1);
        assert!(app.venue(saved.id).unwrap().deleted);
    }

    #[test]
    fn a_bad_gym_form_names_its_field() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let field = |draft: VenueDraft| match app.save_venue(None, draft) {
            Err(MobileError::InvalidTraining { field, .. }) => field,
            other => unreachable!("{other:?}"),
        };
        assert_eq!(field(app.new_venue_draft()), TrainingField::VenueName);
        assert_eq!(
            field(VenueDraft {
                name: "x".into(),
                latitude: "43".into(),
                ..app.new_venue_draft()
            }),
            TrainingField::Coordinates
        );
        assert!(matches!(
            app.create_venue_named("  ".into()),
            Err(MobileError::InvalidTraining { .. })
        ));
    }

    #[test]
    fn the_link_gives_the_coordinates_and_a_short_one_asks_the_full() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        assert_eq!(app.venue_link_coordinates(String::new()).unwrap(), None);
        let found = app
            .venue_link_coordinates("https://www.google.com/maps/@43.1,12.2,15z".into())
            .unwrap()
            .unwrap();
        assert_eq!((found.latitude, found.longitude), (43.1, 12.2));
        match app.venue_link_coordinates("https://maps.app.goo.gl/abc".into()) {
            Err(MobileError::InvalidTraining { field, reason }) => {
                assert_eq!(field, TrainingField::MapLink);
                assert!(!reason.is_empty());
            }
            other => unreachable!("{other:?}"),
        }
    }

    #[test]
    fn the_choice_puts_the_pinned_then_the_used_ones_first() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        let (a, b, c) = (gym(&app, "A"), gym(&app, "B"), gym(&app, "C"));
        session_in(&app, &team, &b, "2030-03-04T18:30");
        app.set_venue_pinned(c.clone(), true).unwrap();
        let ids = |q: &str| -> Vec<String> {
            app.venue_choices(team.clone(), q.into())
                .unwrap()
                .into_iter()
                .map(|v| v.id)
                .collect()
        };
        assert_eq!(ids(""), vec![c.clone(), b.clone(), a]);
        assert_eq!(ids("b"), vec![b]);
    }

    #[test]
    fn another_team_in_the_gym_at_the_time_is_a_warning_naming_the_gym() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let (u17, u19) = (team(&app, "U17M"), team(&app, "U19F"));
        let gym = gym(&app, "Terontola");
        session_in(&app, &u19, &gym, "2030-03-04T18:30");
        // the same team never: it is the conflict of the core
        assert!(
            app.session_venue_clashes(u19.clone(), gym.clone(), "2030-03-04T18:30".into(), 90)
                .unwrap()
                .is_empty()
        );
        let found = app
            .session_venue_clashes(u17.clone(), gym.clone(), "2030-03-04T19:00".into(), 90)
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].team_name, "U19F");
        assert!(found[0].message.contains("Terontola"));
        assert!(found[0].message.contains("18:30"));
        // touching is not clashing
        assert!(
            app.session_venue_clashes(u17.clone(), gym.clone(), "2030-03-04T20:30".into(), 60)
                .unwrap()
                .is_empty()
        );
        // a series, a copy of a session and of a microcycle say it too
        let series = SeriesDraft {
            from: "2030-03-04".into(),
            to: "2030-03-10".into(),
            bands: vec![SeriesBand {
                weekday: 1,
                time: "18:30".into(),
                duration_min: 90,
                venue: Some(gym.clone()),
            }],
        };
        assert_eq!(
            app.series_venue_clashes(u17.clone(), vec![series])
                .unwrap()
                .len(),
            1
        );
        let mine = app
            .save_session(
                u17.clone(),
                None,
                SessionDraft {
                    start: Some("2030-02-25T18:30".into()),
                    title: "x".into(),
                    venue: Some(gym.clone()),
                    ..app.new_session_draft()
                },
            )
            .unwrap();
        let copy = DuplicateDraft {
            start: "2030-03-04T18:30".into(),
            microcycle_id: None,
            copy_objectives: false,
            copy_activities: false,
        };
        assert_eq!(app.duplicate_venue_clashes(mine.id, copy).unwrap().len(), 1);
    }

    #[test]
    fn the_week_of_a_gym_marks_the_overlaps_of_other_teams() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let (u17, u19) = (team(&app, "U17M"), team(&app, "U19F"));
        let gym = gym(&app, "Terontola");
        session_in(&app, &u17, &gym, "2030-03-04T18:30");
        session_in(&app, &u19, &gym, "2030-03-04T19:00");
        session_in(&app, &u19, &gym, "2030-03-06T18:30");
        let week = app
            .venue_agenda(gym, AgendaSpan::Week, "2030-03-05".into())
            .unwrap();
        assert_eq!(week.len(), 7);
        assert_eq!(week[0].date, "2030-03-04");
        assert_eq!(week[0].occupations.len(), 2);
        assert!(week[0].occupations.iter().all(|o| o.overlapping));
        assert_eq!(week[1].occupations.len(), 0);
        assert!(!week[2].occupations[0].overlapping);
    }

    #[test]
    fn a_guest_without_a_team_is_a_person_of_the_registry() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let team = team(&app, "Cortona");
        let guest = app.create_guest(" Anna ".into()).unwrap();
        assert_eq!(guest.name, "Anna");
        assert!(app.create_guest(" ".into()).is_err());
        let s = app
            .save_session(
                team.clone(),
                None,
                SessionDraft {
                    start: Some("2030-03-04T18:30".into()),
                    title: "x".into(),
                    ..app.new_session_draft()
                },
            )
            .unwrap();
        app.save_attendance(
            s.id.clone(),
            vec![crate::training::AttendanceRowRecord {
                person_id: guest.person_id.clone(),
                name: String::new(),
                number: None,
                status: scout4all_core::training::AttendanceStatus::Present,
                note: String::new(),
                guest: true,
                makeup: false,
            }],
        )
        .unwrap();
        let sheet = app.session_attendance(s.id).unwrap();
        assert_eq!(sheet.rows.len(), 1);
        assert_eq!(sheet.rows[0].name, "Anna");
    }

    #[test]
    fn a_match_of_another_team_in_the_gym_is_a_warning_too() {
        let dir = tempfile::tempdir().unwrap();
        let app = app(&dir);
        let (u17, u19) = (team(&app, "U17M"), team(&app, "U19F"));
        let gym = gym(&app, "Terontola");
        (1..=7).for_each(|n| {
            app.save_player(
                u19.clone(),
                None,
                scout4all_core::player_form::PlayerForm {
                    name: format!("P{n}"),
                    role: Some(scout4all_core::shapes::enums::RoleEnum::Setter),
                    number: n.to_string(),
                    birth_date: String::new(),
                    height_cm: String::new(),
                    notes: String::new(),
                    nationality: None,
                    dominant_hand: None,
                    federation_id: String::new(),
                    person: None,
                },
                false,
            )
            .unwrap();
        });
        let date = chrono::DateTime::parse_from_rfc3339("2030-03-04T18:30:00+01:00").unwrap();
        let played = app
            .save_match(
                u19.clone(),
                None,
                scout4all_core::match_form::MatchForm {
                    opponent: "Arezzo".into(),
                    date: Some(date),
                    home: true,
                    max_substitutions: "8".into(),
                    venue: Some(Uuid::parse_str(&gym).unwrap()),
                    ..Default::default()
                },
            )
            .unwrap();
        // the details of the match tell its gym
        assert_eq!(
            app.match_details(u19.clone(), played).unwrap().venue_id,
            Some(gym.clone())
        );
        let found = app
            .match_venue_clashes(u17.clone(), None, gym.clone(), date)
            .unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].is_match);
        assert_eq!(found[0].team_name, "U19F");
        // the team of the match is not warned of its own match
        assert!(
            app.match_venue_clashes(u19, None, gym, date)
                .unwrap()
                .is_empty()
        );
    }
}
