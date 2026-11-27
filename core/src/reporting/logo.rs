//! The logo of a team next to its name in the PDFs; a team without one has
//! just the name. The templates lay it out with `logo.typ`.

use crate::assets::{Image, LogoBackdrop, logo_tone};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Logo {
    pub data: Vec<u8>,
    /// a light logo would vanish on the paper: it goes on a dark patch
    pub dark_backdrop: bool,
}

impl From<Image> for Logo {
    fn from(image: Image) -> Self {
        Logo {
            dark_backdrop: logo_tone(&image.data).backdrop(false) == LogoBackdrop::Dark,
            data: image.data,
        }
    }
}

/// The logos of the two teams of a match, as the team of the report sees
/// them.
#[derive(Debug, Clone, Default)]
pub struct MatchLogos {
    pub team: Option<Logo>,
    pub opponent: Option<Logo>,
}

impl MatchLogos {
    /// As the other team sees them.
    pub fn swapped(self) -> Self {
        MatchLogos {
            team: self.opponent,
            opponent: self.team,
        }
    }
}
