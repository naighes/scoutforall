//! The gyms for the apps (TODO/021 §13): where sessions and matches are
//! held, common to every team.
use super::{MobileError, Scout4All, parse_id};
use scout4all_core::{providers::sqlite::venues::SqliteVenues, training::venue::Venue};

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
