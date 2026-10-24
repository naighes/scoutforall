//! How good a figure is for the level of the team: the band of the
//! middle teams of the level, for the color of the figure (red below it,
//! yellow in it, green above it). No standard tells these bands: they're
//! reference values, to tune as matches come.
//!
//! The efficiencies are the FIPAV ones, as `Metric::Efficiency`. The bands
//! of the high national level are the middle half (25th to 75th
//! percentile) of the teams of the MEVZA League 2021/22, men and women,
//! one team per match, from the DataVolley files of the league
//! (<https://mevza.volleynet.at/Download/59>, `/58` for the women). The
//! other levels move the band of reception, attack and serve as a table
//! of yellow bands by level, put together by the user from studies, moves
//! its own (on a formula giving smaller figures): by the shift of its
//! middle, scaled to the width of the band. Block, side out and break
//! point have no such table: their band is the same at every level.
use crate::shapes::enums::{EventTypeEnum, GenderEnum, TeamClassificationEnum};

/// A figure with a band for the level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Measure {
    /// the efficiencies
    Reception,
    Attack,
    Serve,
    Block,
    /// the share of the rallies won
    SideOut,
    BreakPoint,
}

impl Measure {
    /// The measure of the efficiency of a fundamental, none for the dig
    /// (too few digs are scouted to make a band).
    pub fn of(event_type: EventTypeEnum) -> Option<Self> {
        use EventTypeEnum::*;
        match event_type {
            P => Some(Measure::Reception),
            A => Some(Measure::Attack),
            S => Some(Measure::Serve),
            B => Some(Measure::Block),
            _ => None,
        }
    }
}

/// The figures of the middle teams of a level, in percent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub low: f64,
    pub high: f64,
}

impl Band {
    const fn new(low: i32, high: i32) -> Self {
        Band {
            low: low as f64,
            high: high as f64,
        }
    }

    /// Where the value stands, from 0 (red) to 1 (green): 0.5 (yellow) in
    /// the middle of the band, 0.25 and 0.75 at its edges, 0 and 1 half a
    /// band below and above it.
    pub fn grade(&self, value: f64) -> f64 {
        let width = self.high - self.low;
        match width > 0.0 {
            true => ((value - (self.low - width / 2.0)) / (2.0 * width)).clamp(0.0, 1.0),
            false => 0.5,
        }
    }
}

// the bands by level, from the top: international, high national,
// national mid and low level, regional, local divisions

const MEN_RECEPTION: [Band; 6] = [
    Band::new(39, 56),
    Band::new(30, 47),
    Band::new(22, 39),
    Band::new(13, 30),
    Band::new(5, 22),
    Band::new(-4, 13),
];
const MEN_ATTACK: [Band; 6] = [
    Band::new(29, 45),
    Band::new(21, 37),
    Band::new(13, 29),
    Band::new(5, 21),
    Band::new(-3, 13),
    Band::new(-11, 5),
];
const MEN_SERVE: [Band; 6] = [
    Band::new(10, 28),
    Band::new(5, 23),
    Band::new(1, 19),
    Band::new(-2, 16),
    Band::new(-7, 11),
    Band::new(-13, 5),
];
const WOMEN_RECEPTION: [Band; 6] = [
    Band::new(33, 55),
    Band::new(22, 44),
    Band::new(11, 33),
    Band::new(0, 22),
    Band::new(-11, 11),
    Band::new(-22, 0),
];
const WOMEN_ATTACK: [Band; 6] = [
    Band::new(21, 35),
    Band::new(14, 28),
    Band::new(7, 21),
    Band::new(0, 14),
    Band::new(-7, 7),
    Band::new(-14, 0),
];
const WOMEN_SERVE: [Band; 6] = [
    Band::new(23, 47),
    Band::new(17, 41),
    Band::new(11, 35),
    Band::new(7, 31),
    Band::new(1, 25),
    Band::new(-7, 17),
];

/// The band of the measure for a team of the level and gender.
pub fn band(level: TeamClassificationEnum, gender: GenderEnum, measure: Measure) -> Band {
    use GenderEnum::*;
    use Measure::*;
    use TeamClassificationEnum::*;
    let by_level = |[top, high, mid, low, regional, local]: [Band; 6]| match level {
        TopInternational => top,
        HighNational => high,
        NationalMidLevel => mid,
        NationalLowLevel => low,
        Regional => regional,
        LocalDivision => local,
    };
    match (gender, measure) {
        (Men, Reception) => by_level(MEN_RECEPTION),
        (Men, Attack) => by_level(MEN_ATTACK),
        (Men, Serve) => by_level(MEN_SERVE),
        (Men, Block) => Band::new(-17, 14),
        (Men, SideOut) => Band::new(61, 73),
        (Men, BreakPoint) => Band::new(27, 39),
        (Women, Reception) => by_level(WOMEN_RECEPTION),
        (Women, Attack) => by_level(WOMEN_ATTACK),
        (Women, Serve) => by_level(WOMEN_SERVE),
        (Women, Block) => Band::new(-15, 22),
        (Women, SideOut) => Band::new(53, 63),
        (Women, BreakPoint) => Band::new(37, 47),
    }
}

/// How good the value is for the team (see `Band::grade`), or `None` when
/// the level or the gender of the team isn't known (teams saved before
/// they were asked).
pub fn grade(
    level: Option<TeamClassificationEnum>,
    gender: Option<GenderEnum>,
    measure: Measure,
    value: f64,
) -> Option<f64> {
    level
        .zip(gender)
        .map(|(level, gender)| band(level, gender, measure).grade(value))
}
