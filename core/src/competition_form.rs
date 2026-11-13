//! Editing a competition (D12), whatever the user interface: the form
//! holds what the user typed and chose, and turns into the competition to
//! save once it's valid. Competitions are created in the form of a match,
//! not here.

use crate::{
    domain::{Competition, CompetitionFormat},
    localization::Labels,
    shapes::{country::CountryCode, enums::GenderEnum},
};

/// What the user filled in: the season is text, as typed ("2026/2027").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompetitionForm {
    pub name: String,
    pub format: CompetitionFormat,
    pub season: String,
    pub gender: Option<GenderEnum>,
    pub country: Option<CountryCode>,
    /// the default of its matches: the ones saved keep theirs
    pub friendly: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompetitionFormError {
    EmptyName,
    /// another competition has the name and the season: the form of a
    /// match couldn't tell them apart
    AlreadyThere,
}

impl CompetitionFormError {
    pub fn message(&self, labels: &Labels) -> String {
        match self {
            CompetitionFormError::EmptyName => labels.name_cannot_be_empty,
            CompetitionFormError::AlreadyThere => labels.competition_already_there,
        }
        .to_string()
    }
}

/// A name or a season as written, to compare.
fn same(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

impl CompetitionForm {
    /// The form of a competition being edited.
    pub fn of(c: &Competition) -> Self {
        Self {
            name: c.name.clone(),
            format: c.format,
            season: c.season.clone().unwrap_or_default(),
            gender: c.gender,
            country: c.country,
            friendly: c.friendly,
        }
    }

    /// `existing` as the form has it, name and season trimmed (an empty
    /// season is none), or the first error in the order of the fields.
    /// `others`: the competitions not removed, `existing` among them or
    /// not.
    pub fn validate(
        self,
        existing: &Competition,
        others: &[Competition],
    ) -> Result<Competition, CompetitionFormError> {
        let name = self.name.trim().to_string();
        let season = Some(self.season.trim().to_string()).filter(|s| !s.is_empty());
        let taken = others.iter().any(|c| {
            c.id != existing.id
                && !c.deleted
                && same(&c.name, &name)
                && same(
                    c.season.as_deref().unwrap_or_default(),
                    season.as_deref().unwrap_or_default(),
                )
        });
        match (name.is_empty(), taken) {
            (true, _) => Err(CompetitionFormError::EmptyName),
            (false, true) => Err(CompetitionFormError::AlreadyThere),
            (false, false) => Ok(Competition {
                name,
                format: self.format,
                season,
                gender: self.gender,
                country: self.country,
                friendly: self.friendly,
                ..existing.clone()
            }),
        }
    }
}
