//! Where the ball goes on the court of the opponent: one of the nine zones,
//! or the line between two of them (a conflict zone, e.g. 4/3), as the
//! scout tells it. Seen from behind our court, the net at the top:
//!
//! ```text
//! +-----+-----+-----+-----+-----+
//! |  4  | 4/3 |  3  | 3/2 |  2  |
//! +-----+-----+-----+-----+-----+
//! |    4/7    | 3/8 |    2/9    |
//! +-----+-----+-----+-----+-----+
//! |  7  | 7/8 |  8  | 8/9 |  9  |
//! +-----+-----+-----+-----+-----+
//! |    5/7    | 6/8 |    1/9    |
//! +-----+-----+-----+-----+-----+
//! |  5  | 5/6 |  6  | 6/1 |  1  |
//! +-----+-----+-----+-----+-----+
//! ```

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CourtZoneEnum {
    Z1,
    Z2,
    Z3,
    Z4,
    Z5,
    Z6,
    Z7,
    Z8,
    Z9,
    Z43,
    Z32,
    Z78,
    Z89,
    Z56,
    Z61,
    Z47,
    Z38,
    Z29,
    Z57,
    Z68,
    Z19,
}

/// Where the cursor choosing a zone moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

use CourtZoneEnum::*;

/// The rows of the court, from the net: the zones with the conflict zones
/// between them, then the conflict zones between two rows.
const GRID: [&[CourtZoneEnum]; 5] = [
    &[Z4, Z43, Z3, Z32, Z2],
    &[Z47, Z38, Z29],
    &[Z7, Z78, Z8, Z89, Z9],
    &[Z57, Z68, Z19],
    &[Z5, Z56, Z6, Z61, Z1],
];

impl CourtZoneEnum {
    /// In the order of their code in the flags of an event (see
    /// `EventFlags::target`): never to be reordered, only added to.
    pub const ALL: [CourtZoneEnum; 21] = [
        Z1, Z2, Z3, Z4, Z5, Z6, Z7, Z8, Z9, Z43, Z32, Z78, Z89, Z56, Z61, Z47, Z38, Z29, Z57, Z68,
        Z19,
    ];

    /// The rows of the court, from the net (see the module).
    pub fn grid() -> &'static [&'static [CourtZoneEnum]; 5] {
        &GRID
    }

    /// Where the cursor starts: the middle of the court.
    pub const CENTER: CourtZoneEnum = Z8;

    /// Its code in the flags of an event, 1 to 21.
    pub(crate) fn code(&self) -> u16 {
        Self::ALL
            .iter()
            .position(|z| z == self)
            .and_then(|i| u16::try_from(i + 1).ok())
            .unwrap_or(0)
    }

    /// The zone of a code, `None` for 0 or a code this version doesn't
    /// know.
    pub(crate) fn from_code(code: u16) -> Option<Self> {
        usize::from(code)
            .checked_sub(1)
            .and_then(|i| Self::ALL.get(i))
            .copied()
    }

    /// The zone with the number, 1 to 9.
    pub fn numbered(number: u8) -> Option<Self> {
        match number {
            1 => Some(Z1),
            2 => Some(Z2),
            3 => Some(Z3),
            4 => Some(Z4),
            5 => Some(Z5),
            6 => Some(Z6),
            7 => Some(Z7),
            8 => Some(Z8),
            9 => Some(Z9),
            _ => None,
        }
    }

    /// One of the nine zones, not a conflict zone.
    pub fn is_zone(&self) -> bool {
        matches!(self, Z1 | Z2 | Z3 | Z4 | Z5 | Z6 | Z7 | Z8 | Z9)
    }

    /// The row and the column in `grid`.
    fn place(&self) -> (usize, usize) {
        GRID.iter()
            .enumerate()
            .find_map(|(r, row)| row.iter().position(|z| z == self).map(|c| (r, c)))
            .unwrap_or((2, 2))
    }

    /// The zone next to this one, this one at the edge of the court. Up and
    /// down, a row of five meets a row of three: the zones at the sides
    /// meet the conflict zones under them, the middle ones the middle one.
    pub fn moved(&self, direction: Direction) -> Self {
        let (row, column) = self.place();
        let target = match direction {
            Direction::Up => row.checked_sub(1).map(|r| (r, column)),
            Direction::Down => Some(row + 1)
                .filter(|r| *r < GRID.len())
                .map(|r| (r, column)),
            Direction::Left => column.checked_sub(1).map(|c| (row, c)),
            Direction::Right => Some((row, column + 1)),
        };
        target
            .and_then(|(r, c)| {
                let from = GRID.get(row).map_or(0, |cells| cells.len());
                let to = GRID.get(r)?;
                let c = match (direction, from, to.len()) {
                    (Direction::Up | Direction::Down, 5, 3) => [0, 0, 1, 2, 2].get(c).copied()?,
                    (Direction::Up | Direction::Down, 3, 5) => [0, 2, 4].get(c).copied()?,
                    _ => c,
                };
                to.get(c).copied()
            })
            .unwrap_or(*self)
    }
}

/// As the scout reads it: "4", "4/3".
impl fmt::Display for CourtZoneEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Z1 => "1",
            Z2 => "2",
            Z3 => "3",
            Z4 => "4",
            Z5 => "5",
            Z6 => "6",
            Z7 => "7",
            Z8 => "8",
            Z9 => "9",
            Z43 => "4/3",
            Z32 => "3/2",
            Z78 => "7/8",
            Z89 => "8/9",
            Z56 => "5/6",
            Z61 => "6/1",
            Z47 => "4/7",
            Z38 => "3/8",
            Z29 => "2/9",
            Z57 => "5/7",
            Z68 => "6/8",
            Z19 => "1/9",
        };
        write!(f, "{label}")
    }
}
