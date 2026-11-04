//! Where the ball goes: on the court of the opponent (our serve, our
//! attack) or on ours (their serve, where we receive it; their attack,
//! where we dig it).
//!
//! The scout tells a quarter of a zone (a cell: the subzones of DataVolley,
//! `1A`), or two cells side by side (`1B-6A`), or, from a file without the
//! subzone, the whole zone (`1`). The statistics put it in the nine zones:
//! two cells of different zones half in each. Two cells side by side across
//! the line between two zones of a row (`1D-6A`, `1C-6B`) are on a conflict
//! zone (`6/1`), where two players could take the ball: only the lines
//! across the court, from the net to the end line; two cells one behind the
//! other (`1C-9D`) aren't.
//!
//! The court of the opponent as our server sees it, their end line at the
//! top; each zone in four cells as DataVolley tells them (A behind on the
//! right of who plays there, then B, C, D anticlockwise: seen from our
//! side, A at the top on the left):
//!
//! ```text
//! +----+----+----+----+----+----+
//! | 1A | 1D | 6A | 6D | 5A | 5D |
//! | 1B | 1C | 6B | 6C | 5B | 5C |
//! +----+----+----+----+----+----+
//! | 9A | 9D | 8A | 8D | 7A | 7D |
//! | 9B | 9C | 8B | 8C | 7B | 7C |
//! +----+----+----+----+----+----+
//! | 2A | 2D | 3A | 3D | 4A | 4D |
//! | 2B | 2C | 3B | 3C | 4B | 4C |
//! +----+----+----+----+----+----+
//!                 net
//! ```
//!
//! Our court, seen from the bench with the net at the top, is the same
//! court turned round (`CourtSide::Ours`): 4C 4B on the left at the net,
//! 1D 1A on the right at our end line. The cells, their codes and the pairs
//! are the same on both: only how they're shown changes.

use crate::shapes::enums::EventTypeEnum;
use std::fmt;

/// Which court the ball went to, as it's shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CourtSide {
    /// The opponent's, as our server sees it: their end line at the top.
    Theirs,
    /// Ours, as seen from the bench: the net at the top.
    Ours,
}

impl CourtSide {
    /// The court where the ball of the event goes: our serve and our
    /// attack to theirs, our reception and our dig on ours; none for the
    /// other events.
    pub fn of(event: EventTypeEnum) -> Option<Self> {
        match event {
            EventTypeEnum::S | EventTypeEnum::A => Some(CourtSide::Theirs),
            EventTypeEnum::P | EventTypeEnum::D => Some(CourtSide::Ours),
            _ => None,
        }
    }

    /// The rows as shown on this side, from the top: ours is theirs turned
    /// round.
    fn shown<T: Copy>(&self, rows: &[&[T]]) -> Vec<Vec<T>> {
        match self {
            CourtSide::Theirs => rows.iter().map(|row| row.to_vec()).collect(),
            CourtSide::Ours => rows
                .iter()
                .rev()
                .map(|row| row.iter().rev().copied().collect())
                .collect(),
        }
    }

    /// The direction on the court as our server sees it of a move on the
    /// court shown.
    fn turned(&self, direction: Direction) -> Direction {
        match (self, direction) {
            (CourtSide::Theirs, d) => d,
            (CourtSide::Ours, Direction::Up) => Direction::Down,
            (CourtSide::Ours, Direction::Down) => Direction::Up,
            (CourtSide::Ours, Direction::Left) => Direction::Right,
            (CourtSide::Ours, Direction::Right) => Direction::Left,
        }
    }
}

/// One of the nine zones, or a conflict zone: the line between two zones
/// of a row, from the net to the end line.
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
}

/// Where the cursor choosing a cell moves, on the court as it's shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

use CourtZoneEnum::*;

/// The rows of the court as our server sees it, from their end line: the
/// zones with the conflict zones between them.
const GRID: [&[CourtZoneEnum]; 3] = [
    &[Z1, Z61, Z6, Z56, Z5],
    &[Z9, Z89, Z8, Z78, Z7],
    &[Z2, Z32, Z3, Z43, Z4],
];

/// The zones as our server sees them, from their end line.
const ZONES: [[u8; 3]; 3] = [[1, 6, 5], [9, 8, 7], [2, 3, 4]];

impl CourtZoneEnum {
    /// The nine zones, then the conflict zones.
    pub const ALL: [CourtZoneEnum; 15] = [
        Z1, Z2, Z3, Z4, Z5, Z6, Z7, Z8, Z9, Z43, Z32, Z78, Z89, Z56, Z61,
    ];

    /// The nine zones.
    pub const ZONES: [CourtZoneEnum; 9] = [Z1, Z2, Z3, Z4, Z5, Z6, Z7, Z8, Z9];

    /// The conflict zones.
    pub const CONFLICTS: [CourtZoneEnum; 6] = [Z43, Z32, Z78, Z89, Z56, Z61];

    /// The rows of the court as our server sees it (see `GRID`).
    pub fn grid() -> &'static [&'static [CourtZoneEnum]; 3] {
        &GRID
    }

    /// The rows of the court as shown on `side`, from the top, with the
    /// conflict zones between the zones.
    pub fn shown(side: CourtSide) -> Vec<Vec<CourtZoneEnum>> {
        side.shown(&GRID)
    }

    /// The rows of the nine zones as shown on `side`, from the top.
    pub fn zones_shown(side: CourtSide) -> Vec<Vec<CourtZoneEnum>> {
        Self::shown(side)
            .into_iter()
            .map(|row| row.into_iter().filter(Self::is_zone).collect())
            .collect()
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

    /// The number of one of the nine zones, none for a conflict zone.
    pub fn number(&self) -> Option<u8> {
        (1..=9).find(|n| Self::numbered(*n) == Some(*self))
    }

    /// The conflict zone between two zones side by side in a row, none for
    /// two zones one behind the other or apart.
    pub fn between(a: u8, b: u8) -> Option<Self> {
        match (a.min(b), a.max(b)) {
            (3, 4) => Some(Z43),
            (2, 3) => Some(Z32),
            (7, 8) => Some(Z78),
            (8, 9) => Some(Z89),
            (5, 6) => Some(Z56),
            (1, 6) => Some(Z61),
            _ => None,
        }
    }

    /// One of the nine zones, not a conflict zone.
    pub fn is_zone(&self) -> bool {
        self.number().is_some()
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
        };
        write!(f, "{label}")
    }
}

/// A quarter of a zone, as DataVolley names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SubzoneEnum {
    A,
    B,
    C,
    D,
}

impl SubzoneEnum {
    pub const ALL: [SubzoneEnum; 4] = [
        SubzoneEnum::A,
        SubzoneEnum::B,
        SubzoneEnum::C,
        SubzoneEnum::D,
    ];

    /// The subzone of a letter of DataVolley.
    pub fn of(letter: char) -> Option<Self> {
        match letter.to_ascii_uppercase() {
            'A' => Some(SubzoneEnum::A),
            'B' => Some(SubzoneEnum::B),
            'C' => Some(SubzoneEnum::C),
            'D' => Some(SubzoneEnum::D),
            _ => None,
        }
    }

    fn index(&self) -> usize {
        match self {
            SubzoneEnum::A => 0,
            SubzoneEnum::B => 1,
            SubzoneEnum::C => 2,
            SubzoneEnum::D => 3,
        }
    }

    /// Where it is in its zone as our server sees it: the row (0 toward
    /// their end line) and the column (0 on the left).
    fn place(&self) -> (usize, usize) {
        match self {
            SubzoneEnum::A => (0, 0),
            SubzoneEnum::D => (0, 1),
            SubzoneEnum::B => (1, 0),
            SubzoneEnum::C => (1, 1),
        }
    }

    fn at(row: usize, column: usize) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.place() == (row, column))
    }
}

impl fmt::Display for SubzoneEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let letter = match self {
            SubzoneEnum::A => "A",
            SubzoneEnum::B => "B",
            SubzoneEnum::C => "C",
            SubzoneEnum::D => "D",
        };
        write!(f, "{letter}")
    }
}

/// A quarter of a zone: a cell of the court.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CourtCell {
    zone: u8,
    subzone: SubzoneEnum,
}

/// The cells across and down the court.
pub const CELLS: usize = 6;

impl CourtCell {
    /// The cell of a zone, 1 to 9.
    pub fn new(zone: u8, subzone: SubzoneEnum) -> Option<Self> {
        (1..=9)
            .contains(&zone)
            .then_some(CourtCell { zone, subzone })
    }

    /// Where the cursor starts: in the middle of the court.
    pub const CENTER: CourtCell = CourtCell {
        zone: 8,
        subzone: SubzoneEnum::A,
    };

    pub fn zone_number(&self) -> u8 {
        self.zone
    }

    pub fn zone(&self) -> CourtZoneEnum {
        CourtZoneEnum::numbered(self.zone).unwrap_or(Z8)
    }

    pub fn subzone(&self) -> SubzoneEnum {
        self.subzone
    }

    /// The cell at the row (0 at their end line) and the column (0 on the
    /// left) of the court as our server sees it.
    pub fn at(row: usize, column: usize) -> Option<Self> {
        let zone = ZONES.get(row / 2)?.get(column / 2).copied()?;
        let subzone = SubzoneEnum::at(row % 2, column % 2)?;
        Self::new(zone, subzone)
    }

    /// Its row and column (see `at`).
    pub fn place(&self) -> (usize, usize) {
        let (zone_row, zone_column) = ZONES
            .iter()
            .enumerate()
            .find_map(|(r, row)| row.iter().position(|z| *z == self.zone).map(|c| (r, c)))
            .unwrap_or((1, 1));
        let (row, column) = self.subzone.place();
        (zone_row * 2 + row, zone_column * 2 + column)
    }

    /// The rows of the court as our server sees it, from their end line.
    pub fn grid() -> Vec<Vec<CourtCell>> {
        (0..CELLS)
            .map(|r| (0..CELLS).filter_map(|c| Self::at(r, c)).collect())
            .collect()
    }

    /// The rows of the court as shown on `side`, from the top.
    pub fn shown(side: CourtSide) -> Vec<Vec<CourtCell>> {
        let grid = Self::grid();
        let rows: Vec<&[CourtCell]> = grid.iter().map(Vec::as_slice).collect();
        side.shown(&rows)
    }

    /// The cell next to this one on the court as shown on `side`.
    pub fn moved_on(&self, side: CourtSide, direction: Direction) -> Self {
        self.moved(side.turned(direction))
    }

    /// The cell next to this one, this one at the edge of the court.
    pub fn moved(&self, direction: Direction) -> Self {
        let (row, column) = self.place();
        let to = match direction {
            Direction::Up => row.checked_sub(1).map(|r| (r, column)),
            Direction::Down => Some((row + 1, column)),
            Direction::Left => column.checked_sub(1).map(|c| (row, c)),
            Direction::Right => Some((row, column + 1)),
        };
        to.and_then(|(r, c)| Self::at(r, c)).unwrap_or(*self)
    }

    /// Side by side, across or down: not on a corner.
    pub fn next_to(&self, other: &CourtCell) -> bool {
        let ((r1, c1), (r2, c2)) = (self.place(), other.place());
        r1.abs_diff(r2) + c1.abs_diff(c2) == 1
    }

    fn index(&self) -> u16 {
        u16::from(self.zone - 1) * 4 + u16::try_from(self.subzone.index()).unwrap_or(0)
    }

    fn of_index(index: u16) -> Option<Self> {
        let zone = u8::try_from(index / 4).ok()? + 1;
        let subzone = SubzoneEnum::ALL.get(usize::from(index % 4)).copied()?;
        Self::new(zone, subzone)
    }
}

/// As the scout reads it: "1A".
impl fmt::Display for CourtCell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.zone, self.subzone)
    }
}

/// Where the scout tells the ball went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CourtTarget {
    /// The whole zone, 1 to 9: a file that doesn't tell the subzone.
    Zone(u8),
    Cell(CourtCell),
    /// Two cells side by side, the one higher or more to the left first
    /// (as our server sees the court).
    Pair(CourtCell, CourtCell),
}

/// The codes in the flags of an event: 0 not told, the zones, the cells
/// (by zone, then subzone), then the pairs (side by side across, row by
/// row, then down, row by row). Never to be reordered.
const FIRST_CELL: u16 = 10;
const FIRST_PAIR: u16 = FIRST_CELL + 36;
const ACROSS: u16 = (CELLS * (CELLS - 1)) as u16;
/// The highest code: 7 bits.
pub(crate) const LAST_CODE: u16 = FIRST_PAIR + 2 * ACROSS - 1;

impl CourtTarget {
    /// The two cells side by side, in order; none if they aren't.
    pub fn pair(a: CourtCell, b: CourtCell) -> Option<Self> {
        a.next_to(&b).then(|| match a.place() < b.place() {
            true => CourtTarget::Pair(a, b),
            false => CourtTarget::Pair(b, a),
        })
    }

    /// The zone of a number, with its subzone if told.
    pub fn of_zone(zone: u8, subzone: Option<SubzoneEnum>) -> Option<Self> {
        match subzone {
            Some(s) => CourtCell::new(zone, s).map(CourtTarget::Cell),
            None => (1..=9).contains(&zone).then_some(CourtTarget::Zone(zone)),
        }
    }

    /// The zones the statistics put it in, each with its weight: its
    /// zone, or half in each zone of the two cells.
    pub fn zones(&self) -> Vec<(CourtZoneEnum, f64)> {
        match self {
            CourtTarget::Zone(z) => CourtZoneEnum::numbered(*z)
                .map(|zone| (zone, 1.0))
                .into_iter()
                .collect(),
            CourtTarget::Cell(c) => vec![(c.zone(), 1.0)],
            CourtTarget::Pair(a, b) if a.zone == b.zone => vec![(a.zone(), 1.0)],
            CourtTarget::Pair(a, b) => vec![(a.zone(), 0.5), (b.zone(), 0.5)],
        }
    }

    /// The conflict zone it's on: two cells side by side across the line
    /// between two zones of a row (see `CourtZoneEnum::between`).
    pub fn conflict(&self) -> Option<CourtZoneEnum> {
        match self {
            CourtTarget::Pair(a, b) if a.place().0 == b.place().0 => {
                CourtZoneEnum::between(a.zone, b.zone)
            }
            _ => None,
        }
    }

    /// Whether the cell is one told.
    pub fn has(&self, cell: &CourtCell) -> bool {
        match self {
            CourtTarget::Zone(_) => false,
            CourtTarget::Cell(c) => c == cell,
            CourtTarget::Pair(a, b) => a == cell || b == cell,
        }
    }

    /// The target after the scout touches the cell: a cell alone first,
    /// then one side by side along with it; touched again, a cell is taken
    /// back; a cell elsewhere starts over from it.
    pub fn touched(current: Option<CourtTarget>, cell: CourtCell) -> Option<CourtTarget> {
        match current {
            Some(CourtTarget::Cell(c)) if c == cell => None,
            Some(CourtTarget::Cell(c)) => {
                Some(Self::pair(c, cell).unwrap_or(CourtTarget::Cell(cell)))
            }
            Some(CourtTarget::Pair(a, b)) if a == cell => Some(CourtTarget::Cell(b)),
            Some(CourtTarget::Pair(a, b)) if b == cell => Some(CourtTarget::Cell(a)),
            Some(CourtTarget::Pair(_, _) | CourtTarget::Zone(_)) | None => {
                Some(CourtTarget::Cell(cell))
            }
        }
    }

    /// Its code in the flags of an event (see `FIRST_CELL`).
    pub(crate) fn code(&self) -> u16 {
        match self {
            CourtTarget::Zone(z) => u16::from(*z),
            CourtTarget::Cell(c) => FIRST_CELL + c.index(),
            CourtTarget::Pair(a, b) => {
                let ((r, c), (r2, _)) = (a.place(), b.place());
                let (r, c) = (r as u16, c as u16);
                let cells = CELLS as u16;
                match r == r2 as u16 {
                    true => FIRST_PAIR + r * (cells - 1) + c,
                    false => FIRST_PAIR + ACROSS + r * cells + c,
                }
            }
        }
    }

    /// The target of a code, none for 0 or a code this version doesn't
    /// know.
    pub(crate) fn from_code(code: u16) -> Option<Self> {
        let cells = CELLS as u16;
        let at = |r: u16, c: u16| CourtCell::at(usize::from(r), usize::from(c));
        match code {
            1..=9 => u8::try_from(code).ok().map(CourtTarget::Zone),
            FIRST_CELL..FIRST_PAIR => CourtCell::of_index(code - FIRST_CELL).map(CourtTarget::Cell),
            FIRST_PAIR..=LAST_CODE => {
                let index = code - FIRST_PAIR;
                let (a, b) = match index < ACROSS {
                    true => {
                        let (r, c) = (index / (cells - 1), index % (cells - 1));
                        (at(r, c)?, at(r, c + 1)?)
                    }
                    false => {
                        let index = index - ACROSS;
                        let (r, c) = (index / cells, index % cells);
                        (at(r, c)?, at(r + 1, c)?)
                    }
                };
                Self::pair(a, b)
            }
            _ => None,
        }
    }

    /// As the scout reads it, with the conflict zone two cells are on:
    /// "1D-6A (6/1)".
    pub fn described(&self) -> String {
        match self.conflict() {
            Some(zone) => format!("{self} ({zone})"),
            None => self.to_string(),
        }
    }

    /// Every target the scout can tell, by code.
    pub fn all() -> Vec<CourtTarget> {
        (1..=LAST_CODE).filter_map(Self::from_code).collect()
    }
}

/// As the scout reads it: "1", "1A", "1B-6A".
impl fmt::Display for CourtTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CourtTarget::Zone(z) => write!(f, "{z}"),
            CourtTarget::Cell(c) => write!(f, "{c}"),
            CourtTarget::Pair(a, b) => write!(f, "{a}-{b}"),
        }
    }
}
