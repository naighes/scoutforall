//! What the scout tells about an event besides its type, player, evaluation
//! and technique, saved as one number in the last column of the events:
//! bit 0 the inverted P1, bits 1 and 2 the reception formation. Empty or
//! missing (the sets scouted up to v0.10.1) is 0: the canonical P1, the
//! reception with three players.

use crate::errors::{AppError, IOError};
use std::fmt;

/// How many receive the serve: three by default, four or two when the
/// scout tells so for a rally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ReceptionSchemeEnum {
    #[default]
    Three,
    Four,
    Two,
}

impl ReceptionSchemeEnum {
    pub const ALL: [ReceptionSchemeEnum; 3] = [
        ReceptionSchemeEnum::Three,
        ReceptionSchemeEnum::Four,
        ReceptionSchemeEnum::Two,
    ];
}

/// The flags of an event. The bits this version doesn't know (written by a
/// later one) are kept as they were.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EventFlags(u16);

const INVERTED_P1: u16 = 1;
const RECEPTION: u16 = 0b110;
const RECEPTION_FOUR: u16 = 0b010;
const RECEPTION_TWO: u16 = 0b100;

impl EventFlags {
    /// In P1 in side out, the outside hitter attacks from 4 and the
    /// opposite from 2, as in break point.
    pub fn inverted_p1(&self) -> bool {
        self.0 & INVERTED_P1 != 0
    }

    pub fn with_inverted_p1(self, inverted: bool) -> Self {
        match inverted {
            true => Self(self.0 | INVERTED_P1),
            false => Self(self.0 & !INVERTED_P1),
        }
    }

    /// The reception formation, on the reception or on their ace with no
    /// reception; three when not told, or told in a way this version
    /// doesn't know.
    pub fn reception(&self) -> ReceptionSchemeEnum {
        match self.0 & RECEPTION {
            RECEPTION_FOUR => ReceptionSchemeEnum::Four,
            RECEPTION_TWO => ReceptionSchemeEnum::Two,
            _ => ReceptionSchemeEnum::Three,
        }
    }

    pub fn with_reception(self, scheme: ReceptionSchemeEnum) -> Self {
        let bits = match scheme {
            ReceptionSchemeEnum::Three => 0,
            ReceptionSchemeEnum::Four => RECEPTION_FOUR,
            ReceptionSchemeEnum::Two => RECEPTION_TWO,
        };
        Self(self.0 & !RECEPTION | bits)
    }

    /// The flags of the column: empty is none.
    pub fn parse(column: &str) -> Result<Self, AppError> {
        match column {
            "" => Ok(Self::default()),
            n => n
                .parse()
                .map(Self)
                .map_err(|_| AppError::IO(IOError::Msg(format!("invalid flags {n}")))),
        }
    }
}

/// As saved: empty when none, as the technique.
impl fmt::Display for EventFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            0 => Ok(()),
            n => write!(f, "{n}"),
        }
    }
}
