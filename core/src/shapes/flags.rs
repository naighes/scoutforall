//! What the scout tells about an event besides its type, player, evaluation
//! and technique, saved as one number in the last column of the events:
//! bit 0 the inverted P1, bits 1 and 2 the reception formation, bits 3 to 9
//! where the ball went (the code of a `CourtTarget`, 0 when not told), bits
//! 10 to 12 how many blocked (the code of a `BlockersEnum`, 0 when not
//! told), bits 13 to 15 where our serve started from (the code of a
//! `ServeStartEnum`, 0 when not told).
//! Empty or missing (the sets scouted up to v0.10.1) is 0: the canonical
//! P1, the reception with three players, no zone.

use crate::errors::{AppError, IOError};
use crate::shapes::court_zone::{CourtTarget, ServeStartEnum};
use crate::shapes::enums::EventTypeEnum;
use serde::{Deserialize, Serialize};
use std::fmt;

/// How many receive the serve: three by default, four or two when the
/// scout tells so for a rally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
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

/// How many blocked: against our attack, how many of theirs (none too);
/// on our block, how many of ours. As DataVolley tells them, with the hole
/// block: two with a gap between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BlockersEnum {
    NoBlock,
    One,
    Two,
    Three,
    Hole,
}

impl BlockersEnum {
    pub const ALL: [BlockersEnum; 5] = [
        BlockersEnum::NoBlock,
        BlockersEnum::One,
        BlockersEnum::Two,
        BlockersEnum::Three,
        BlockersEnum::Hole,
    ];

    /// The ones the scout can tell on the event: all of them against our
    /// attack, on our block all but none; nothing for the other events.
    pub fn of(event: EventTypeEnum) -> &'static [BlockersEnum] {
        match event {
            EventTypeEnum::A => &Self::ALL,
            EventTypeEnum::B => &Self::ALL[1..],
            _ => &[],
        }
    }

    /// Its name in the labels (see `Labels::blockers`).
    pub fn name(&self, labels: &crate::localization::Labels) -> &'static str {
        let names = labels.blockers;
        match self {
            BlockersEnum::NoBlock => names[0],
            BlockersEnum::One => names[1],
            BlockersEnum::Two => names[2],
            BlockersEnum::Three => names[3],
            BlockersEnum::Hole => names[4],
        }
    }

    /// As DataVolley writes it: 0 to 3 blockers, 4 the hole block.
    pub fn of_datavolley(c: char) -> Option<Self> {
        match c {
            '0' => Some(BlockersEnum::NoBlock),
            '1' => Some(BlockersEnum::One),
            '2' => Some(BlockersEnum::Two),
            '3' => Some(BlockersEnum::Three),
            '4' => Some(BlockersEnum::Hole),
            _ => None,
        }
    }

    /// The next one the scout tells on the event, after `current`: from
    /// the first, then none told again after the last.
    pub fn next(event: EventTypeEnum, current: Option<Self>) -> Option<Self> {
        let all = Self::of(event);
        match current.and_then(|c| all.iter().position(|b| *b == c)) {
            Some(i) => all.get(i + 1).copied(),
            None => all.first().copied(),
        }
    }

    /// Its code in the flags (see `BLOCKERS`). Never to be reordered.
    fn code(&self) -> u16 {
        match self {
            BlockersEnum::NoBlock => 1,
            BlockersEnum::One => 2,
            BlockersEnum::Two => 3,
            BlockersEnum::Three => 4,
            BlockersEnum::Hole => 5,
        }
    }

    fn from_code(code: u16) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.code() == code)
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
const TARGET: u16 = 0b111_1111 << TARGET_SHIFT;
const BLOCKERS_SHIFT: u16 = 10;
const BLOCKERS: u16 = 0b111 << BLOCKERS_SHIFT;
const SERVE_START_SHIFT: u16 = 13;
const SERVE_START: u16 = 0b111 << SERVE_START_SHIFT;

impl EventFlags {
    /// All the bits, as the database keeps them: those this version
    /// doesn't know too.
    pub fn bits(&self) -> u16 {
        self.0
    }

    pub fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

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
    pub fn target(&self) -> Option<CourtTarget> {
        CourtTarget::from_code((self.0 & TARGET) >> TARGET_SHIFT)
    }

    pub fn with_target(self, target: Option<CourtTarget>) -> Self {
        let bits = target.map_or(0, |t| t.code() << TARGET_SHIFT);
        Self(self.0 & !TARGET | bits)
    }

    /// How many blocked, if the scout told it (in a way this version
    /// knows).
    pub fn blockers(&self) -> Option<BlockersEnum> {
        BlockersEnum::from_code((self.0 & BLOCKERS) >> BLOCKERS_SHIFT)
    }

    pub fn with_blockers(self, blockers: Option<BlockersEnum>) -> Self {
        let bits = blockers.map_or(0, |b| b.code() << BLOCKERS_SHIFT);
        Self(self.0 & !BLOCKERS | bits)
    }

    /// Where our serve started from, if the scout told it (in a way this
    /// version knows).
    pub fn serve_start(&self) -> Option<ServeStartEnum> {
        ServeStartEnum::from_code((self.0 & SERVE_START) >> SERVE_START_SHIFT)
    }

    pub fn with_serve_start(self, start: Option<ServeStartEnum>) -> Self {
        let bits = start.map_or(0, |z| z.code() << SERVE_START_SHIFT);
        Self(self.0 & !SERVE_START | bits)
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
