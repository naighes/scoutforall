//! What the scout tells about an event besides its type, player, evaluation
//! and technique, saved as one number in the last column of the events:
//! bit 0 the inverted P1, bits 1 and 2 the reception formation, bits 3 to 7
//! where the ball went (the code of a `CourtZoneEnum`, 0 when not told).
//! Empty or missing (the sets scouted up to v0.10.1) is 0: the canonical
//! P1, the reception with three players, no zone.

use crate::errors::{AppError, IOError};
use crate::shapes::court_zone::CourtZoneEnum;
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

    /// How many receive.
    pub fn players(&self) -> u8 {
        match self {
            ReceptionSchemeEnum::Three => 3,
            ReceptionSchemeEnum::Four => 4,
            ReceptionSchemeEnum::Two => 2,
        }
    }
}

/// The flags of an event. The bits this version doesn't know (written by a
/// later one) are kept as they were.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct EventFlags(u16);

const INVERTED_P1: u16 = 1;
const RECEPTION: u16 = 0b110;
const RECEPTION_FOUR: u16 = 0b010;
const RECEPTION_TWO: u16 = 0b100;
const TARGET_SHIFT: u16 = 3;
const TARGET: u16 = 0b1_1111 << TARGET_SHIFT;

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

    /// Where the ball went, if the scout told it (and told it in a way
    /// this version knows).
    pub fn target(&self) -> Option<CourtZoneEnum> {
        CourtZoneEnum::from_code((self.0 & TARGET) >> TARGET_SHIFT)
    }

    pub fn with_target(self, target: Option<CourtZoneEnum>) -> Self {
        let bits = target.map_or(0, |zone| zone.code() << TARGET_SHIFT);
        Self(self.0 & !TARGET | bits)
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
