//! The gyms (TODO/021 §13): where sessions and matches are held. A gym is
//! common to every team; only its name is needed. The coordinates are read
//! from the link of a map, without the network, or written by hand. Pure:
//! the provider reads and writes.

use crate::{
    localization::Labels,
    training::overlap::{Slot, SlotKind},
};
use chrono::NaiveDateTime;
use std::collections::HashSet;
use uuid::Uuid;

/// A gym, as saved.
#[derive(Debug, Clone, PartialEq)]
pub struct Venue {
    pub id: Uuid,
    pub name: String,
    pub address: String,
    pub map_url: String,
    /// latitude and longitude, together
    pub coordinates: Option<(f64, f64)>,
    pub notes: String,
    /// the gyms the user trains in, in evidence; the same on every device
    pub pinned: bool,
    /// removed: sessions and matches keep it, the choice doesn't offer it
    pub deleted: bool,
}

/// The gym of an event, as the calendar shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VenueRef {
    pub id: Uuid,
    pub name: String,
    /// removed: shown as such
    pub deleted: bool,
}

/// A valid form, the arguments of `VenueWriter::create_venue` and
/// `update_venue`.
#[derive(Debug, Clone, PartialEq)]
pub struct VenueInput {
    pub name: String,
    pub address: String,
    pub map_url: String,
    pub coordinates: Option<(f64, f64)>,
    pub notes: String,
    pub pinned: bool,
}

impl VenueInput {
    /// A gym made on the fly, from a form that only has the name.
    pub fn named(name: &str) -> Result<Self, VenueFormError> {
        VenueForm {
            name: name.to_string(),
            ..VenueForm::default()
        }
        .validate()
    }
}

/// What the user filled in: the coordinates are text, as typed; all but the
/// name may be empty.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VenueForm {
    pub name: String,
    pub address: String,
    pub map_url: String,
    pub latitude: String,
    pub longitude: String,
    pub notes: String,
    pub pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VenueFormError {
    EmptyName,
    /// one of the two missing, not a number or out of range
    InvalidCoordinates,
}

impl VenueFormError {
    pub fn message(&self, labels: &Labels) -> String {
        match self {
            VenueFormError::EmptyName => labels.name_cannot_be_empty,
            VenueFormError::InvalidCoordinates => labels.invalid_coordinates,
        }
        .to_string()
    }
}

/// Why a link gives no coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkError {
    /// a short link (`maps.app.goo.gl`): it hides them, and there is no
    /// network to follow it
    Short,
    /// nothing of a place in it
    NoCoordinates,
    /// found, but not a place of the Earth
    OutOfRange,
}

impl LinkError {
    pub fn message(&self, labels: &Labels) -> String {
        match self {
            LinkError::Short => labels.short_map_link,
            LinkError::NoCoordinates | LinkError::OutOfRange => labels.invalid_coordinates,
        }
        .to_string()
    }
}

impl VenueForm {
    /// The form of a gym being edited.
    pub fn of(venue: &Venue) -> Self {
        Self {
            name: venue.name.clone(),
            address: venue.address.clone(),
            map_url: venue.map_url.clone(),
            latitude: venue
                .coordinates
                .map(|(lat, _)| lat.to_string())
                .unwrap_or_default(),
            longitude: venue
                .coordinates
                .map(|(_, lon)| lon.to_string())
                .unwrap_or_default(),
            notes: venue.notes.clone(),
            pinned: venue.pinned,
        }
    }

    /// The coordinates the link of the form gives, for the user interface to
    /// say what is wrong with it (nothing when the link is empty).
    pub fn link_status(&self) -> Result<Option<(f64, f64)>, LinkError> {
        match self.map_url.trim() {
            "" => Ok(None),
            link => coordinates_from_link(link).map(Some),
        }
    }

    /// The input; the texts are trimmed. The coordinates written win over
    /// those of the link, which are taken when none is written (a link
    /// without any, a short one for instance, just gives none).
    pub fn validate(self) -> Result<VenueInput, VenueFormError> {
        let written = parse_pair(&self.latitude, &self.longitude);
        let from_link = coordinates_from_link(self.map_url.trim()).ok();
        match (self.name.trim().is_empty(), written) {
            (true, _) => Err(VenueFormError::EmptyName),
            (false, Err(e)) => Err(e),
            (false, Ok(written)) => Ok(VenueInput {
                name: self.name.trim().to_string(),
                address: self.address.trim().to_string(),
                map_url: self.map_url.trim().to_string(),
                coordinates: written.or(from_link),
                notes: self.notes.trim().to_string(),
                pinned: self.pinned,
            }),
        }
    }
}

/// Both texts empty: none; both a valid latitude and longitude: the pair;
/// anything else is an error.
fn parse_pair(latitude: &str, longitude: &str) -> Result<Option<(f64, f64)>, VenueFormError> {
    match (latitude.trim(), longitude.trim()) {
        ("", "") => Ok(None),
        (lat, lon) => match (number(lat), number(lon)) {
            (Some(lat), Some(lon)) if in_range(lat, lon) => Ok(Some((lat, lon))),
            _ => Err(VenueFormError::InvalidCoordinates),
        },
    }
}

fn number(text: &str) -> Option<f64> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn in_range(lat: f64, lon: f64) -> bool {
    (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon)
}

/// The coordinates a link of a map holds, without the network. Google
/// (`@lat,lon`, `!3d…!4d…`, `q=`, `ll=`, `query=`), Apple (`ll=`, `q=`,
/// `coordinate=`), Bing (`cp=lat~lon`, `where1=`), OpenStreetMap
/// (`#map=z/lat/lon`, `mlat=`/`mlon=`) and `geo:lat,lon`. A short link has
/// none.
pub fn coordinates_from_link(link: &str) -> Result<(f64, f64), LinkError> {
    let link = decoded(link.trim());
    let lower = link.to_lowercase();
    match (is_short(&lower), found(&link)) {
        (_, Some((lat, lon))) if in_range(lat, lon) => Ok((lat, lon)),
        (_, Some(_)) => Err(LinkError::OutOfRange),
        (true, None) => Err(LinkError::Short),
        (false, None) => Err(LinkError::NoCoordinates),
    }
}

fn is_short(lower: &str) -> bool {
    ["maps.app.goo.gl", "goo.gl/maps", "g.co/kgs", "share.google"]
        .iter()
        .any(|short| lower.contains(short))
}

/// `%2C` and `%7E` are what a comma and a tilde look like in a link; the
/// plus is a space and the coordinates have none.
fn decoded(link: &str) -> String {
    link.replace("%2C", ",")
        .replace("%2c", ",")
        .replace("%7E", "~")
        .replace("%7e", "~")
        .replace("%20", "")
        .replace('+', "")
}

/// The first place the link says, the most precise first.
fn found(link: &str) -> Option<(f64, f64)> {
    geo(link)
        .or_else(|| google_place(link))
        .or_else(|| at_sign(link))
        .or_else(|| osm_marker(link))
        .or_else(|| osm_map(link))
        .or_else(|| bing_center(link))
        .or_else(|| {
            [
                "ll",
                "sll",
                "q",
                "query",
                "coordinate",
                "center",
                "where1",
                "destination",
            ]
            .iter()
            .find_map(|key| param(link, key).and_then(|value| pair(&value)))
        })
}

/// `geo:45.1,9.2` (and `?q=…`).
fn geo(link: &str) -> Option<(f64, f64)> {
    link.strip_prefix("geo:")
        .map(|rest| rest.split(['?', ';']).next().unwrap_or_default())
        .and_then(pair)
}

/// `!3d45.1!4d9.2`: the place itself, where `@` is only the view.
fn google_place(link: &str) -> Option<(f64, f64)> {
    let lat = after(link, "!3d").and_then(leading_number)?;
    let lon = after(link, "!4d").and_then(leading_number)?;
    Some((lat, lon))
}

/// `@45.1,9.2,17z`.
fn at_sign(link: &str) -> Option<(f64, f64)> {
    after(link, "@").and_then(|rest| {
        let mut parts = rest.split(',');
        let lat = parts.next().and_then(number)?;
        let lon = parts.next().and_then(number)?;
        Some((lat, lon))
    })
}

/// `mlat=45.1&mlon=9.2`.
fn osm_marker(link: &str) -> Option<(f64, f64)> {
    let lat = param(link, "mlat").and_then(|v| number(&v))?;
    let lon = param(link, "mlon").and_then(|v| number(&v))?;
    Some((lat, lon))
}

/// `#map=17/45.1/9.2`.
fn osm_map(link: &str) -> Option<(f64, f64)> {
    after(link, "map=").and_then(|rest| {
        let mut parts = rest
            .split(['&', '#', '?'])
            .next()
            .unwrap_or_default()
            .split('/');
        let _zoom = parts.next()?;
        let lat = parts.next().and_then(number)?;
        let lon = parts.next().and_then(number)?;
        Some((lat, lon))
    })
}

/// `cp=45.1~9.2`.
fn bing_center(link: &str) -> Option<(f64, f64)> {
    param(link, "cp").and_then(|value| pair(&value))
}

/// What follows the first `marker`.
fn after<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    text.find(marker).map(|at| &text[at + marker.len()..])
}

/// The value of a parameter of the query or of the fragment.
fn param(link: &str, key: &str) -> Option<String> {
    link.split(['?', '&', '#', ';']).skip(1).find_map(|part| {
        part.split_once('=')
            .filter(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.to_string())
    })
}

/// `lat,lon` or `lat~lon`.
fn pair(text: &str) -> Option<(f64, f64)> {
    let mut parts = text.split([',', '~']);
    let lat = parts.next().and_then(number)?;
    let lon = parts.next().and_then(number)?;
    Some((lat, lon))
}

/// The number a text begins with.
fn leading_number(text: &str) -> Option<f64> {
    let end = text
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-')))
        .unwrap_or(text.len());
    number(&text[..end])
}

/// The geo URI to open a gym in the map of the phone.
pub fn geo_uri(venue: &Venue) -> Option<String> {
    venue
        .coordinates
        .map(|(lat, lon)| format!("geo:{lat},{lon}?q={lat},{lon}({})", venue.name))
}

/// What to open in the browser of a computer: the link of the gym, else
/// OpenStreetMap on its coordinates.
pub fn web_url(venue: &Venue) -> Option<String> {
    match (venue.map_url.trim(), venue.coordinates) {
        ("", Some((lat, lon))) => Some(format!(
            "https://www.openstreetmap.org/?mlat={lat}&mlon={lon}#map=17/{lat}/{lon}"
        )),
        ("", None) => None,
        (link, _) => Some(link.to_string()),
    }
}

/// A session or a match of another team in the same gym at the same time:
/// allowed, but the user is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VenueClash {
    /// the gym that is busy
    pub venue: Uuid,
    /// the start of the window that was asked about
    pub window: NaiveDateTime,
    pub team_id: Uuid,
    pub team_name: String,
    pub kind: SlotKind,
    pub start: NaiveDateTime,
    pub minutes: u16,
}

impl VenueClash {
    /// "Gym X is busy: U19F 18:30–20:00".
    pub fn message(&self, labels: &Labels, venue: &str) -> String {
        let end = self.start + chrono::Duration::minutes(i64::from(self.minutes));
        labels
            .venue_busy
            .replacen("{}", venue, 1)
            .replacen("{}", &self.team_name, 1)
            .replacen("{}", &self.start.format("%H:%M").to_string(), 1)
            .replacen("{}", &end.format("%H:%M").to_string(), 1)
    }
}

/// A session (not cancelled) or a match in programma holding a gym for a
/// while: what two teams may want at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occupation {
    pub venue: Uuid,
    /// the id of the session or of the match
    pub item: Uuid,
    /// the teams that are there: one for a session, A and B (when a team of
    /// the user) for a match
    pub teams: Vec<(Uuid, String)>,
    pub slot: Slot,
}

impl Occupation {
    /// The names of the teams that aren't in `own`, joined.
    fn others(&self, own: &[Uuid]) -> Vec<&(Uuid, String)> {
        self.teams
            .iter()
            .filter(|(id, _)| !own.contains(id))
            .collect()
    }
}

/// The clashes of the windows `windows` in `venue` with the occupations of
/// teams that aren't `own`: a match one of whose teams is `own` is theirs.
pub fn clashes_in(
    venue: &Uuid,
    windows: &[(NaiveDateTime, u16)],
    own: &[Uuid],
    occupations: &[Occupation],
) -> Vec<VenueClash> {
    windows
        .iter()
        .flat_map(|(start, minutes)| {
            let window = Slot::session(*start, *minutes);
            occupations
                .iter()
                .filter(|o| o.venue == *venue && o.teams.iter().all(|(id, _)| !own.contains(id)))
                .filter(move |o| !crate::training::overlap::clashes(&window, &[o.slot]).is_empty())
                .flat_map(move |o| {
                    o.others(own)
                        .into_iter()
                        .map(|(team_id, team_name)| VenueClash {
                            venue: *venue,
                            window: *start,
                            team_id: *team_id,
                            team_name: team_name.clone(),
                            kind: o.slot.kind,
                            start: o.slot.start,
                            minutes: o.slot.minutes,
                        })
                        .take(1)
                        .collect::<Vec<_>>()
                })
        })
        .collect()
}

/// The items that hold a gym at the same time as an item of other teams:
/// the signs of the calendar. Items of the same team aren't here: they are
/// a conflict the core refuses.
pub fn shared(occupations: &[Occupation]) -> HashSet<Uuid> {
    occupations
        .iter()
        .enumerate()
        .flat_map(|(i, a)| {
            occupations[i + 1..]
                .iter()
                .filter(move |b| {
                    a.venue == b.venue
                        && a.item != b.item
                        && a.teams
                            .iter()
                            .all(|(x, _)| b.teams.iter().all(|(y, _)| x != y))
                        && !crate::training::overlap::clashes(&a.slot, &[b.slot]).is_empty()
                })
                .flat_map(move |b| [a.item, b.item])
        })
        .collect()
}
