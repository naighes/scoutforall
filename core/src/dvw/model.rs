//! A DataVolley file as it's written, before any translation into our
//! matches: the parser only reads, the meaning of the codes is up to the
//! import.

use chrono::{NaiveDate, NaiveTime};

/// The two teams of a file: `*` the home team, `a` the visiting one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Home,
    Visiting,
}

impl Side {
    pub(crate) fn from_marker(marker: char) -> Option<Side> {
        match marker {
            '*' => Some(Side::Home),
            'a' => Some(Side::Visiting),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DvwFile {
    /// The program that wrote the file, e.g. "Data Volley Release 3.7.3".
    pub generator: Option<String>,
    pub last_change: Option<String>,
    pub match_info: MatchInfo,
    pub home: Team,
    pub visiting: Team,
    pub sets: Vec<SetResult>,
    pub home_players: Vec<Player>,
    pub visiting_players: Vec<Player>,
    pub attack_combinations: Vec<AttackCombination>,
    /// Missing in the files of Click&Scout: the defaults of DataVolley.
    pub winning_symbols: Option<WinningSymbols>,
    pub scout: Vec<ScoutLine>,
}

impl DvwFile {
    pub fn team(&self, side: Side) -> &Team {
        match side {
            Side::Home => &self.home,
            Side::Visiting => &self.visiting,
        }
    }

    pub fn players(&self, side: Side) -> &[Player] {
        match side {
            Side::Home => &self.home_players,
            Side::Visiting => &self.visiting_players,
        }
    }
}

/// The day of the match: the files write it in the format of the computer
/// of the scout, so `08/11/2020` may be either day first or month first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchDate {
    Known(NaiveDate),
    /// Day first and month first, both valid: someone has to choose.
    Ambiguous(NaiveDate, NaiveDate),
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchInfo {
    pub date: MatchDate,
    pub time: Option<NaiveTime>,
    /// As written, e.g. `2014/2015`; some files have something else there.
    pub season: Option<String>,
    pub league: Option<String>,
    pub phase: Option<String>,
    /// The code page declared by the file, e.g. `1250`, or `UTF-8`.
    pub text_encoding: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Team {
    pub code: String,
    pub name: String,
    pub sets_won: Option<u8>,
    pub coach: Option<String>,
    pub assistant: Option<String>,
}

/// A set played: the others are left out.
#[derive(Debug, Clone, PartialEq)]
pub struct SetResult {
    pub set_number: u8,
    /// The scores at 8, 16 and 21 points (home, visiting).
    pub partials: Vec<(u8, u8)>,
    pub final_score: (u8, u8),
    /// Minutes.
    pub duration: Option<u16>,
}

/// Where a player starts a set: in a zone of the starting six, or on the
/// bench and entering later (`*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetStart {
    Zone(u8),
    Substitute,
    NotPlayed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DvwRole {
    Libero,
    OutsideHitter,
    Opposite,
    MiddleBlocker,
    Setter,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    pub number: u8,
    /// One per set, up to five.
    pub starts: Vec<SetStart>,
    /// The id given by the scout or the league, e.g. `BR1`.
    pub id: String,
    pub surname: String,
    pub name: String,
    pub role: Option<DvwRole>,
    /// The special role column: `L` for a libero, `C` for the captain.
    pub libero: bool,
    pub captain: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttackCombination {
    pub code: String,
    pub start_zone: Option<u8>,
    /// `L`, `R` or `C`: where the attacker is from the setter.
    pub side: Option<char>,
    /// `Q` quick, `T` tense, `H` high...
    pub tempo: Option<char>,
    pub description: String,
    /// `F` front, `B` back, `C` center, `P` pipe, `S` setter.
    pub target: Option<char>,
}

/// The skills of DataVolley, in the order of `[3WINNINGSYMBOLS]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Skill {
    Serve,
    Reception,
    Attack,
    Block,
    Dig,
    Set,
    Freeball,
}

impl Skill {
    pub const ALL: [Skill; 7] = [
        Skill::Serve,
        Skill::Reception,
        Skill::Attack,
        Skill::Block,
        Skill::Dig,
        Skill::Set,
        Skill::Freeball,
    ];

    /// The letter of the skill in the codes, e.g. `A` for the attack.
    pub fn code(self) -> char {
        match self {
            Skill::Serve => 'S',
            Skill::Reception => 'R',
            Skill::Attack => 'A',
            Skill::Block => 'B',
            Skill::Dig => 'D',
            Skill::Set => 'E',
            Skill::Freeball => 'F',
        }
    }

    pub(crate) fn from_code(code: char) -> Option<Skill> {
        match code {
            'S' => Some(Skill::Serve),
            'R' => Some(Skill::Reception),
            'A' => Some(Skill::Attack),
            'B' => Some(Skill::Block),
            'D' => Some(Skill::Dig),
            'E' => Some(Skill::Set),
            'F' => Some(Skill::Freeball),
            _ => None,
        }
    }
}

/// The evaluations that end the rally, per skill: the point is lost by the
/// team of the player, or won.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinningSymbols {
    /// One per skill, in the order of `Skill::ALL`.
    pub skills: Vec<(Skill, Vec<char>, Vec<char>)>,
}

impl WinningSymbols {
    pub fn losing(&self, skill: Skill) -> &[char] {
        self.skills
            .iter()
            .find(|(s, _, _)| *s == skill)
            .map(|(_, losing, _)| losing.as_slice())
            .unwrap_or_default()
    }

    pub fn winning(&self, skill: Skill) -> &[char] {
        self.skills
            .iter()
            .find(|(s, _, _)| *s == skill)
            .map(|(_, _, winning)| winning.as_slice())
            .unwrap_or_default()
    }
}

/// A touch of the ball, e.g. `*06SM#~~~18C`: the fields after the
/// evaluation are optional, `~` when empty.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillCode {
    /// None when the scout didn't know who played it (`$$`).
    pub player: Option<u8>,
    pub skill: Skill,
    /// `H` high, `M` medium, `Q` quick, `T` tense, `U` super, `N` fast, `O`
    /// other: for a serve `H` float, `M` jump float, `Q` jump spin.
    pub skill_type: char,
    pub evaluation: char,
    /// The attack combination (`X5`) or, for a set, the setter call (`K1`).
    pub combination: Option<String>,
    /// The target of a set: `F`, `B`, `C`, `P`, `S`.
    pub target: Option<char>,
    pub start_zone: Option<u8>,
    pub end_zone: Option<u8>,
    pub end_subzone: Option<char>,
    pub skill_subtype: Option<char>,
    pub num_players: Option<char>,
    pub special: Option<char>,
    pub custom: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Code {
    Skill(Side, SkillCode),
    /// `$$&`: a point won or lost without a touch, e.g. a net fault; the
    /// side is the team who played, the evaluation `#` or `=`.
    Green(Side, char),
    /// `*p01:00`: the point and the score after it (home, visiting).
    Point(Side, u8, u8),
    /// `*z3`: the zone of the setter.
    SetterZone(Side, u8),
    /// `*P04`: the setter on court; `>LUp` at the start of a set.
    Setter {
        side: Side,
        player: u8,
        lineup: bool,
    },
    /// `*c05:12`: the first leaves, the second enters.
    Substitution {
        side: Side,
        out: u8,
        enters: u8,
    },
    Timeout(Side),
    /// `**1set`.
    EndOfSet(u8),
    /// The notes of the scout, e.g. `>ROT` for a rotation fault or a card.
    Note(String),
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoutLine {
    /// In the file, from 1: for the messages to the user.
    pub line: usize,
    pub code: Code,
    /// `p` breakpoint, `s` sideout: the phase of a winning attack.
    pub point_phase: Option<char>,
    /// `r` reception, `s`/`p` transition in sideout/breakpoint.
    pub attack_phase: Option<char>,
    pub time: Option<String>,
    pub set_number: Option<u8>,
    /// The zones of the setters, home and visiting.
    pub setter_zones: (Option<u8>, Option<u8>),
    /// The numbers on court in zones 1–6, home and visiting: missing in
    /// truncated lines.
    pub home_court: Option<[u8; 6]>,
    pub visiting_court: Option<[u8; 6]>,
}
