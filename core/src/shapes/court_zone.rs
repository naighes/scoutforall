//! Where the ball goes: on the court of the opponent (our serve, our
//! attack) or on ours (their serve, where we receive it; their attack,
//! where we dig it, or block it at the net).
//!
//! The block tells only the zone at the net, 4, 3 or 2 (`TargetKind::Net`),
//! as DataVolley: in front of their attack.
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
    /// attack to theirs, our reception, our dig and our block on ours; none
    /// for the other events.
    pub fn of(event: EventTypeEnum) -> Option<Self> {
        match event {
            EventTypeEnum::S | EventTypeEnum::A => Some(CourtSide::Theirs),
            EventTypeEnum::P | EventTypeEnum::D | EventTypeEnum::B => Some(CourtSide::Ours),
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

/// How the scout tells where the ball went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TargetKind {
    /// A cell or two side by side anywhere on the court.
    Cells,
    /// One of the zones at the net, 4, 3 or 2: where we blocked, in front
    /// of their attack.
    Net,
}

impl TargetKind {
    /// How the scout tells where the ball of the event went: the block at
    /// the net, the others on the cells; none where it isn't told.
    pub fn of(event: EventTypeEnum) -> Option<Self> {
        match (event, CourtSide::of(event)) {
            (EventTypeEnum::B, _) => Some(TargetKind::Net),
            (_, Some(_)) => Some(TargetKind::Cells),
            (_, None) => None,
        }
    }

    /// Whether the target can be told this way: at the net only a whole
    /// zone there.
    pub fn fits(&self, target: &CourtTarget) -> bool {
        match (self, target) {
            (TargetKind::Cells, _) => true,
            (TargetKind::Net, CourtTarget::Zone(z)) => {
                CourtZoneEnum::numbered(*z).is_some_and(|zone| CourtZoneEnum::NET.contains(&zone))
            }
            (TargetKind::Net, CourtTarget::Cell(_) | CourtTarget::Pair(_, _)) => false,
        }
    }

    /// The zones the statistics show: the nine, or the ones at the net.
    pub fn zones(&self) -> &'static [CourtZoneEnum] {
        match self {
            TargetKind::Cells => &CourtZoneEnum::ZONES,
            TargetKind::Net => &CourtZoneEnum::NET,
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

    /// The zones at the net, where we block.
    pub const NET: [CourtZoneEnum; 3] = [Z2, Z3, Z4];

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

    /// The zones at the net as shown on `side`, from the left: 4, 3, 2 on
    /// ours.
    pub fn net_shown(side: CourtSide) -> Vec<CourtZoneEnum> {
        Self::zones_shown(side)
            .into_iter()
            .flatten()
            .filter(|zone| Self::NET.contains(zone))
            .collect()
    }

    /// The zone at the net next to this one on `side`, this one at the
    /// end; the first one shown from elsewhere.
    pub fn moved_at_net(&self, side: CourtSide, direction: Direction) -> Self {
        let net = Self::net_shown(side);
        let at = net.iter().position(|z| z == self);
        let to = match (at, direction) {
            (None, _) => Some(0),
            (Some(i), Direction::Left) => i.checked_sub(1),
            (Some(i), Direction::Right) => Some(i + 1),
            (Some(i), Direction::Up | Direction::Down) => Some(i),
        };
        to.and_then(|i| net.get(i))
            .or_else(|| at.and_then(|i| net.get(i)))
            .copied()
            .unwrap_or(*self)
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

    /// The target after the scout touches a whole zone: told, or taken
    /// back when it was.
    pub fn touched_zone(current: Option<CourtTarget>, zone: CourtZoneEnum) -> Option<CourtTarget> {
        let target = zone.number().map(CourtTarget::Zone);
        match current == target {
            true => None,
            false => target,
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

/// Where our server stands behind our end line, as DataVolley tells it: the
/// zone of the back row the serve starts from, 1 (on the right of our
/// server, facing the net), 9, 6, 7 or 5 (on the left).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ServeStartEnum {
    Z1,
    Z9,
    Z6,
    Z7,
    Z5,
}

impl ServeStartEnum {
    /// From the right of our server to the left: as DataVolley numbers
    /// them.
    pub const ALL: [ServeStartEnum; 5] = [
        ServeStartEnum::Z1,
        ServeStartEnum::Z9,
        ServeStartEnum::Z6,
        ServeStartEnum::Z7,
        ServeStartEnum::Z5,
    ];

    /// As shown below their court (`CourtSide::Theirs`), from the left:
    /// as our server sees our end line, facing the net.
    pub const SHOWN: [ServeStartEnum; 5] = [
        ServeStartEnum::Z5,
        ServeStartEnum::Z7,
        ServeStartEnum::Z6,
        ServeStartEnum::Z9,
        ServeStartEnum::Z1,
    ];

    /// The number of the zone: 1, 9, 6, 7 or 5.
    pub fn number(&self) -> u8 {
        match self {
            ServeStartEnum::Z1 => 1,
            ServeStartEnum::Z9 => 9,
            ServeStartEnum::Z6 => 6,
            ServeStartEnum::Z7 => 7,
            ServeStartEnum::Z5 => 5,
        }
    }

    /// The zone of a number, none for a zone the serve doesn't start from.
    pub fn numbered(number: u8) -> Option<Self> {
        Self::ALL.into_iter().find(|z| z.number() == number)
    }

    /// The one next to it as shown (see `SHOWN`), this one at the end.
    pub fn moved(&self, direction: Direction) -> Self {
        let at = Self::SHOWN.iter().position(|z| z == self);
        let to = match (at, direction) {
            (Some(i), Direction::Left) => i.checked_sub(1),
            (Some(i), Direction::Right) => Some(i + 1),
            (Some(i), Direction::Up | Direction::Down) => Some(i),
            (None, _) => None,
        };
        to.and_then(|i| Self::SHOWN.get(i))
            .copied()
            .unwrap_or(*self)
    }

    /// The one below a cell of their court as shown, the column of the
    /// cell (0 to 5) across the five: the middle one under the two middle
    /// columns.
    pub fn below(column: usize) -> Self {
        let at = match column {
            0 => 0,
            1 | 2 => column,
            3 => 2,
            _ => column.saturating_sub(1),
        };
        Self::SHOWN.get(at).copied().unwrap_or(ServeStartEnum::Z6)
    }

    /// The column of their court as shown (0 to 5) above it: the way back
    /// of `below`.
    pub fn column(&self) -> usize {
        match self {
            ServeStartEnum::Z5 => 0,
            ServeStartEnum::Z7 => 1,
            ServeStartEnum::Z6 => 2,
            ServeStartEnum::Z9 => 4,
            ServeStartEnum::Z1 => 5,
        }
    }

    /// Its code in the flags of an event: 0 is not told. Never to be
    /// reordered.
    pub(crate) fn code(&self) -> u16 {
        match self {
            ServeStartEnum::Z1 => 1,
            ServeStartEnum::Z9 => 2,
            ServeStartEnum::Z6 => 3,
            ServeStartEnum::Z7 => 4,
            ServeStartEnum::Z5 => 5,
        }
    }

    pub(crate) fn from_code(code: u16) -> Option<Self> {
        Self::ALL.into_iter().find(|z| z.code() == code)
    }

    /// The serve to start from where the server served last among the
    /// events, if they told it: none if they didn't serve yet.
    pub fn preselected<'a>(
        server: &uuid::Uuid,
        events: impl DoubleEndedIterator<Item = &'a crate::shapes::snapshot::EventEntry>,
    ) -> Option<Self> {
        events.rev().find_map(|e| match (e.event_type, e.player) {
            (EventTypeEnum::S, Some(player)) if player == *server => e.flags.serve_start(),
            _ => None,
        })
    }
}

/// As the scout reads it: "1".
impl fmt::Display for ServeStartEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.number())
    }
}
