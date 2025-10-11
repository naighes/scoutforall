use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::{
    errors::{AppError, IOError},
    localization::Labels,
};

pub trait FriendlyName {
    fn friendly_name(&self, labels: &Labels) -> &'static str;
}

/// Represents the two possible phases of play in volleyball.
///
/// In volleyball, the game alternates between two fundamental phases:
///
/// - **SideOut**:  
///   The team is *receiving* the serve. The main objective is to
///   successfully receive the ball and score a point to regain
///   the right to serve (a "side-out").
///
/// - **Break**:  
///   The team is *serving*. The main objective is to put pressure
///   on the opponent with the serve and subsequent defense in order
///   to win consecutive points ("break points").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhaseEnum {
    Break,
    SideOut,
}

impl PhaseEnum {
    pub const ALL: [PhaseEnum; 2] = [PhaseEnum::Break, PhaseEnum::SideOut];
}

impl fmt::Display for PhaseEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            PhaseEnum::Break => "break",
            PhaseEnum::SideOut => "side-out",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for PhaseEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use PhaseEnum::*;
        match s.to_uppercase().as_str() {
            "break" => Ok(Break),
            "side-out" => Ok(SideOut),
            _ => Err(AppError::IO(IOError::Msg(format!("invalid phase: {}", s)))),
        }
    }
}

impl FriendlyName for PhaseEnum {
    fn friendly_name(&self, labels: &Labels) -> &'static str {
        use PhaseEnum::*;
        match self {
            Break => "break",
            SideOut => labels.sideout,
        }
    }
}

/// Identifies which team an event or action refers to.
///
/// This enum distinguishes between:
///
/// - **Us**:  
///   Our team (the one being scouted).
///
/// - **Them**:  
///   The opposing team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TeamSideEnum {
    Us,
    Them,
}

impl fmt::Display for TeamSideEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use TeamSideEnum::*;
        let label = match self {
            Us => "us",
            Them => "them",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for TeamSideEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use TeamSideEnum::*;
        match s.to_uppercase().as_str() {
            "us" => Ok(Us),
            "them" => Ok(Them),
            _ => Err(AppError::IO(IOError::Msg(format!(
                "invalid team side: {}",
                s
            )))),
        }
    }
}

/// Represents the type of event that can occur during a volleyball rally.
///
/// The variants are stored as short codes (following common scouting
/// notation) and can be converted into human-friendly names using
/// [`friendly_name`](EventTypeEnum::friendly_name).
///
/// The supported event types are:
///
/// - **S**: Serve  
///   A player performs the serve to start the rally.
///
/// - **P**: Pass (Reception)  
///   The action of receiving and controlling the opponent's serve.
///
/// - **A**: Attack  
///   An offensive action aimed at scoring a point by hitting the ball
///   into the opponent's court.
///
/// - **D**: Dig (Defense)  
///   A defensive action to keep the ball in play after an opponent's attack.
///
/// - **B**: Block  
///   A defensive net action to stop or deflect the opponent's attack.
///
/// - **F**: Fault  
///   A team error resulting in a lost point (e.g., net touch, rotation fault).
///
/// - **OS**: Opponent Score  
///   A point scored directly by the opponent (not caused by our own fault).
///
/// - **OE**: Opponent Error  
///   A point gained because of an opponent's mistake.
///
/// - **R**: Substitution  
///   A player change performed by either team.
///
/// - **CL**: Change Libero  
///   A libero change performed by either team.
///
/// - **CS**: Change Setter
///   A setter change performed by either team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum EventTypeEnum {
    S,
    P,
    A,
    D,
    B,
    F,
    OS,
    OE,
    R,
    CL,
    CS,
}

impl fmt::Display for EventTypeEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use EventTypeEnum::*;
        let label = match self {
            S => "S",
            P => "P",
            A => "A",
            D => "D",
            B => "B",
            F => "F",
            OS => "OS",
            OE => "OE",
            R => "R",
            CL => "CL",
            CS => "CS",
        };
        write!(f, "{}", label)
    }
}

impl EventTypeEnum {
    pub fn requires_evaluation(&self) -> bool {
        use EventTypeEnum::*;
        matches!(self, S | P | A | D | B)
    }

    pub fn requires_player(&self) -> bool {
        use EventTypeEnum::*;
        matches!(self, A | B | P | F | D | R | S | CS)
    }

    pub fn available_evals(&self) -> Vec<EvalEnum> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        match self {
            S | A | B => vec![Perfect, Positive, Over, Negative, Error],
            D | P => vec![Perfect, Positive, Exclamative, Over, Negative, Error],
            _ => vec![],
        }
    }

    pub fn provides_direct_points(&self) -> bool {
        use EventTypeEnum::*;
        matches!(self, S | A | B)
    }

    pub fn error_type(&self, eval: Option<EvalEnum>) -> Option<ErrorTypeEnum> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        match (&self, eval) {
            (A, Some(Error)) | (S, Some(Error)) | (B, Some(Over)) | (F, _) => {
                Some(ErrorTypeEnum::Unforced)
            }
            (A, Some(Over)) | (B, Some(Error)) | (P, Some(Error)) | (D, Some(Error)) => {
                Some(ErrorTypeEnum::Forced)
            }
            _ => None,
        }
    }
}

impl FriendlyName for EventTypeEnum {
    fn friendly_name(&self, labels: &Labels) -> &'static str {
        use EventTypeEnum::*;
        match self {
            S => labels.serve,
            P => labels.reception,
            A => labels.attack,
            D => labels.defense,
            B => labels.block,
            F => labels.fault,
            OS => labels.opponent_score,
            OE => labels.opponent_error,
            R => labels.substitution,
            CL => labels.change_libero,
            CS => labels.change_setter,
        }
    }
}

impl FromStr for EventTypeEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use EventTypeEnum::*;
        match s.to_uppercase().as_str() {
            "S" => Ok(S),
            "P" => Ok(P),
            "A" => Ok(A),
            "D" => Ok(D),
            "B" => Ok(B),
            "F" => Ok(F),
            "OS" => Ok(OS),
            "OE" => Ok(OE),
            "R" => Ok(R),
            "CL" => Ok(CL),
            "CS" => Ok(CS),
            _ => Err(AppError::IO(IOError::Msg(format!(
                "invalid event type: {}",
                s
            )))),
        }
    }
}

/// Represents the evaluation of a volleyball action, expressed using
/// standard scouting symbols.
///
/// Each variant corresponds to a one-character code that encodes the
/// quality or outcome of a play. These evaluations are applied across
/// all fundamentals (serve, reception, attack, block, dig/defense).
///
/// The supported evaluations are:
///
/// - **Perfect (`#`)**
///   An optimal action.
///   - Serve: ace (direct point).
///   - Reception: perfect pass, enabling all attack options.
///   - Attack: kill (direct point).
///   - Block: block point.
///   - Defense: controlled dig that keeps all offensive options open.
///
/// - **Positive (`+`)**
///   A good action, effective but not flawless.
///   - Serve: puts the opponent in trouble, limiting their attack.
///   - Reception: accurate pass, but not perfect — still allows
///     first-tempo attacks.  
///   - Attack: does not score, but keeps the rally alive and allows
///     our team to continue the play (a chance for a re-attack).
///   - Block: successful, but does not score - the ball remains
///     playable and allows a re-attack.
///   - Defense: playable ball, but with limited attacking transition.
///
/// - **Exclamative (`!`)**  
///   An action worth highlighting, often unusual or remarkable.
///   - Serve: similar to `/` - the opponent returns the ball,
///     but it comes back at the "third touch", giving us a chance to continue.
///   - Reception: between `+` and `-` - pass is neither perfect nor bad.
///   - Attack: keeps the rally alive thanks to our team coverage,
///     allowing a possible re-attack.
///   - Block: not used.
///   - Defense: similar to reception - playable, but not perfect.
///
/// - **Over (`/`)**  
///   The action gives the opponent a direct advantage or results
///   in an immediate turnover.
///   - Serve: the opponent's reception immediately sends the ball
///     back to our side (ball returns straight away).
///   - Reception: overpass, ball goes directly back to the opponent.
///   - Attack: blocked - the opponent scores directly with a block.
///   - Block: team commits an invasion (net or crossing the plane),
///     giving a point to the opponent (unforced error).
///   - Defense: uncontrolled dig that crosses the net and lands
///     directly in the opponent's court.
///
/// - **Error (`=`)**
///   A direct error that ends the rally in favor of the opponent.
///   - Serve: fault (unforced error).
///   - Reception: a reception error.
///   - Attack: ball out or into the net (unforced error).
///   - Block: block out.
///   - Defense: unhandled ball.
///
/// - **Negative (`-`)**
///   A weak action that puts the team at disadvantage, though play continues.
///   - Serve: too easy, giving full control to the opponent.
///   - Reception: poor pass, only one predictable attack available.
///   - Attack: easily defended.
///   - Defense: keeps the ball in play, but no chance for a strong counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EvalEnum {
    #[serde(rename = "#")]
    Perfect,
    #[serde(rename = "+")]
    Positive,
    #[serde(rename = "!")]
    Exclamative,
    #[serde(rename = "/")]
    Over,
    #[serde(rename = "=")]
    Error,
    #[serde(rename = "-")]
    Negative,
}

impl EvalEnum {
    #[allow(dead_code)]
    pub const ALL: [EvalEnum; 6] = [
        EvalEnum::Perfect,
        EvalEnum::Positive,
        EvalEnum::Exclamative,
        EvalEnum::Over,
        EvalEnum::Error,
        EvalEnum::Negative,
    ];
}

impl fmt::Display for EvalEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use EvalEnum::*;
        let label = match self {
            Perfect => "#",
            Positive => "+",
            Exclamative => "!",
            Over => "/",
            Error => "=",
            Negative => "-",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for EvalEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use EvalEnum::*;
        match s.to_uppercase().as_str() {
            "#" => Ok(Perfect),
            "+" => Ok(Positive),
            "!" => Ok(Exclamative),
            "/" => Ok(Over),
            "=" => Ok(Error),
            "-" => Ok(Negative),
            _ => Err(AppError::IO(IOError::Msg(format!("invalid eval: {}", s)))),
        }
    }
}

impl EvalEnum {
    pub fn friendly_description(&self, event: EventTypeEnum, labels: &Labels) -> Option<String> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        match (self, event) {
            (Perfect, A) => Some(labels.winning_attack.to_string()),
            (Positive, A) => Some(labels.attack_continuation.to_string()),
            (Negative, A) => Some(labels.opponent_counter_attack_opportunity.to_string()),

            (Perfect, B) => Some(labels.winning_block.to_string()),
            (Positive, B) => Some(labels.counter_attack_opportunity.to_string()),
            (Negative, B) => Some(labels.opponent_counter_attack_opportunity.to_string()),

            (Positive, D) => Some(labels.first_tempo_still_available.to_string()),
            (Exclamative, D) => Some(labels.few_attack_options_available.to_string()),
            (Negative, D) => Some(labels.limited_attack_options.to_string()),
            (Over, D) => Some(labels.ball_goes_straight_over_the_net.to_string()),

            (Positive, P) => Some(labels.first_tempo_still_available.to_string()),
            (Exclamative, P) => Some(labels.few_attack_options_available.to_string()),
            (Negative, P) => Some(labels.limited_attack_options.to_string()),
            (Over, P) => Some(labels.ball_goes_straight_over_the_net.to_string()),

            (Perfect, S) => None,
            (Positive, S) => Some(labels.opponent_with_limited_attack_options.to_string()),
            (Negative, S) => Some(labels.opponent_with_full_attack_options.to_string()),
            (Over, S) => Some(labels.ball_goes_straight_back_to_our_court.to_string()),
            _ => None,
        }
    }

    pub fn friendly_name(&self, event: EventTypeEnum, labels: &Labels) -> String {
        use EvalEnum::*;
        use EventTypeEnum::*;
        match (self, event) {
            (Perfect, A) => labels.score.to_string(),
            (Positive, A) => labels.positive.to_string(),
            (Negative, A) => labels.negative.to_string(),
            (Error, A) => labels.error.to_string(),
            (Over, A) => labels.blocked.to_string(),

            (Perfect, B) => labels.winning_block.to_string(),
            (Positive, B) => labels.positive.to_string(),
            (Negative, B) => labels.negative.to_string(),
            (Error, B) => labels.error.to_string(),
            (Over, B) => labels.net_fault.to_string(),

            (Perfect, D) => labels.perfect.to_string(),
            (Positive, D) => labels.positive.to_string(),
            (Exclamative, D) => labels.subpositive.to_string(),
            (Negative, D) => labels.negative.to_string(),
            (Error, D) => labels.error.to_string(),
            (Over, D) => labels.overpass.to_string(),

            (Perfect, P) => labels.perfect.to_string(),
            (Positive, P) => labels.positive.to_string(),
            (Exclamative, P) => labels.subpositive.to_string(),
            (Negative, P) => labels.negative.to_string(),
            (Error, P) => labels.error.to_string(),
            (Over, P) => labels.overpass.to_string(),

            (Perfect, S) => labels.ace.to_string(),
            (Positive, S) => labels.positive.to_string(),
            (Negative, S) => labels.negative.to_string(),
            (Error, S) => labels.error.to_string(),
            (Over, S) => labels.overpass.to_string(),

            _ => event.to_string(),
        }
    }
}

/// Represents the numbered zones of a volleyball court.
///
/// Zones are numbered from 1 to 9, typically used for reception, attack,
/// and defensive positioning. The numbering follows standard scouting notation:
///
/// ```text
///  ------------
/// | 4 | 3 | 2 |   Front row
///  ------------
/// | 7 | 8 | 9 |
/// | 5 | 6 | 1 |
///  ------------
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ZoneEnum {
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
}

impl fmt::Display for ZoneEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ZoneEnum::*;
        let label = match self {
            One => "1",
            Two => "2",
            Three => "3",
            Four => "4",
            Five => "5",
            Six => "6",
            Seven => "7",
            Eight => "8",
            Nine => "9",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for ZoneEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use ZoneEnum::*;
        match s.to_uppercase().as_str() {
            "1" => Ok(One),
            "2" => Ok(Two),
            "3" => Ok(Three),
            "4" => Ok(Four),
            "5" => Ok(Five),
            "6" => Ok(Six),
            "7" => Ok(Seven),
            "8" => Ok(Eight),
            "9" => Ok(Nine),
            _ => Err(AppError::IO(IOError::Msg(format!("invalid zone: {}", s)))),
        }
    }
}

impl TryFrom<u8> for ZoneEnum {
    type Error = String;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use ZoneEnum::*;
        match value {
            1 => Ok(One),
            2 => Ok(Two),
            3 => Ok(Three),
            4 => Ok(Four),
            5 => Ok(Five),
            6 => Ok(Six),
            7 => Ok(Seven),
            8 => Ok(Eight),
            9 => Ok(Nine),
            _ => Err(format!("invalid zone: {}", value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorTypeEnum {
    Forced,
    Unforced,
}

impl fmt::Display for ErrorTypeEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ErrorTypeEnum::*;
        let label = match self {
            Forced => "forced",
            Unforced => "unforced",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for ErrorTypeEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use ErrorTypeEnum::*;
        match s.to_uppercase().as_str() {
            "forced" => Ok(Forced),
            "unforced" => Ok(Unforced),
            _ => Err(AppError::IO(IOError::Msg(format!(
                "invalid error type: {}",
                s
            )))),
        }
    }
}

/// Represents the team rotations in volleyball.
///
/// A rotation defines the position of players on the court, particularly
/// the location of the **setter** in the formation.  
/// Rotations are numbered from 1 to 6, following standard volleyball notation.
///
/// Example layout for **Rotation 1** (setter in position 1):
///
/// ```text
/// -----------------------------------------------------------|
/// |                                                          |
/// |   4 (Opposite)     |     3 (Middle 2)    | 2 (Outside 1) |
/// |                                                          |
/// ---------------------------------------------------------- |
/// |                                                          |
/// |                                                          |
/// |   5 (Outside 2)    | 6 (Middle 1/Libero) | 1 (Setter)    |
/// |                                                          |
/// |                                                          |
/// -----------------------------------------------------------|
/// ```
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RotationEnum {
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
}

impl RotationEnum {
    pub const ALL: [RotationEnum; 6] = [
        RotationEnum::One,
        RotationEnum::Two,
        RotationEnum::Three,
        RotationEnum::Four,
        RotationEnum::Five,
        RotationEnum::Six,
    ];
}

impl fmt::Display for RotationEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use RotationEnum::*;
        let label = match self {
            One => "1",
            Two => "2",
            Three => "3",
            Four => "4",
            Five => "5",
            Six => "6",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for RotationEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use RotationEnum::*;
        match s.to_uppercase().as_str() {
            "1" => Ok(One),
            "2" => Ok(Two),
            "3" => Ok(Three),
            "4" => Ok(Four),
            "5" => Ok(Five),
            "6" => Ok(Six),
            _ => Err(AppError::IO(IOError::Msg(format!(
                "invalid rotation: {}",
                s
            )))),
        }
    }
}

impl TryFrom<u8> for RotationEnum {
    type Error = String;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use RotationEnum::*;
        match value {
            1 => Ok(One),
            2 => Ok(Two),
            3 => Ok(Three),
            4 => Ok(Four),
            5 => Ok(Five),
            6 => Ok(Six),
            _ => Err(format!("invalid rotation: {}", value)),
        }
    }
}

/// Represents the main player roles in volleyball.
///
/// Each role has specific responsibilities during a match:
///
/// - **Libero**: defensive specialist, plays only in the back row; cannot attack or serve.
/// - **OppositeHitter**: attacks from the right side, usually the main scorer; also blocks against opponent's outside hitter.
/// - **Setter**: organizes the offense, sets up attacks for hitters; equivalent to the "playmaker".
/// - **OutsideHitter**: attacks from the left side; often primary attacker and passer.
/// - **MiddleBlocker**: blocks opponent attacks in the center and performs quick middle attacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoleEnum {
    Libero,
    OppositeHitter,
    Setter,
    OutsideHitter,
    MiddleBlocker,
}

impl RoleEnum {
    pub const ALL: [RoleEnum; 5] = [
        RoleEnum::Libero,
        RoleEnum::MiddleBlocker,
        RoleEnum::OppositeHitter,
        RoleEnum::OutsideHitter,
        RoleEnum::Setter,
    ];
}

impl fmt::Display for RoleEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use RoleEnum::*;
        let label = match self {
            Libero => "libero",
            MiddleBlocker => "middle-blocker",
            OppositeHitter => "opposite-hitter",
            OutsideHitter => "outside-hitter",
            Setter => "setter",
        };
        write!(f, "{}", label)
    }
}

impl FriendlyName for RoleEnum {
    fn friendly_name(&self, labels: &Labels) -> &'static str {
        use RoleEnum::*;
        match self {
            Libero => labels.libero,
            MiddleBlocker => labels.middle_blocker,
            OppositeHitter => labels.opposite_hitter,
            OutsideHitter => labels.outside_hitter,
            Setter => labels.setter,
        }
    }
}

impl FromStr for RoleEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use RoleEnum::*;
        match s.to_uppercase().as_str() {
            "libero" => Ok(Libero),
            "middle-blocker" => Ok(MiddleBlocker),
            "opposite-hitter" => Ok(OppositeHitter),
            "outside-hitter" => Ok(OutsideHitter),
            "setter" => Ok(Setter),
            _ => Err(AppError::IO(IOError::Msg(format!("invalid role: {}", s)))),
        }
    }
}

/// Supported languages.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguageEnum {
    En,
    It,
}

impl LanguageEnum {
    pub const ALL: [LanguageEnum; 2] = [LanguageEnum::En, LanguageEnum::It];
}

impl fmt::Display for LanguageEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use LanguageEnum::*;
        let label = match self {
            En => "english",
            It => "italiano",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for LanguageEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use LanguageEnum::*;
        match s.to_uppercase().as_str() {
            "EN" | "ENGLISH" => Ok(En),
            "IT" | "ITALIANO" | "ITALIAN" => Ok(It),
            _ => Err(AppError::IO(IOError::Msg(format!(
                "invalid language: {}",
                s
            )))),
        }
    }
}

/// Global classification system that categorizes teams by their performance model, competitive context and level of professionalism.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum TeamClassificationEnum {
    TopInternational,
    HighNational,
    NationalMidLevel,
    NationalLowLevel,
    Regional,
    LocalDivision,
}

impl fmt::Display for TeamClassificationEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use TeamClassificationEnum::*;
        let label = match self {
            TopInternational => "top-international",
            HighNational => "high-national",
            NationalMidLevel => "national-mid-level",
            NationalLowLevel => "national-low-level",
            Regional => "regional",
            LocalDivision => "local-division",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for TeamClassificationEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use TeamClassificationEnum::*;
        match s.to_uppercase().as_str() {
            "TOP-INTERNATIONAL" => Ok(TopInternational),
            "HIGH-NATIONAL" => Ok(HighNational),
            "NATIONAL-MID-LEVEL" => Ok(NationalMidLevel),
            "NATIONAL-LOW-LEVEL" => Ok(NationalLowLevel),
            "REGIONAL" => Ok(Regional),
            "LOCAL-DIVISION" => Ok(LocalDivision),
            _ => Err(AppError::IO(IOError::Msg(format!(
                "invalid team classification: {}",
                s
            )))),
        }
    }
}

impl TeamClassificationEnum {
    pub fn friendly_description(&self, labels: &Labels) -> &'static str {
        use TeamClassificationEnum::*;
        match self {
            TopInternational => labels.top_international_description,
            HighNational => labels.high_national_description,
            NationalMidLevel => labels.national_mid_level_description,
            NationalLowLevel => labels.national_low_level_description,
            Regional => labels.regional_description,
            LocalDivision => labels.local_division_description,
        }
    }

    pub const ALL: [TeamClassificationEnum; 6] = [
        TeamClassificationEnum::TopInternational,
        TeamClassificationEnum::HighNational,
        TeamClassificationEnum::NationalMidLevel,
        TeamClassificationEnum::NationalLowLevel,
        TeamClassificationEnum::Regional,
        TeamClassificationEnum::LocalDivision,
    ];
}

impl FriendlyName for TeamClassificationEnum {
    fn friendly_name(&self, labels: &Labels) -> &'static str {
        use TeamClassificationEnum::*;
        match self {
            TopInternational => labels.top_international,
            HighNational => labels.high_national,
            NationalMidLevel => labels.national_mid_level,
            NationalLowLevel => labels.national_low_level,
            Regional => labels.regional,
            LocalDivision => labels.local_division,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum GenderEnum {
    Men,
    Women,
}

impl fmt::Display for GenderEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use GenderEnum::*;
        let label = match self {
            Men => "men",
            Women => "women",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for GenderEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use GenderEnum::*;
        match s.to_uppercase().as_str() {
            "MEN" => Ok(Men),
            "WOMEN" => Ok(Women),
            _ => Err(AppError::IO(IOError::Msg(format!("invalid gender: {}", s)))),
        }
    }
}

impl FriendlyName for GenderEnum {
    fn friendly_name(&self, labels: &Labels) -> &'static str {
        use GenderEnum::*;
        match self {
            Men => labels.men,
            Women => labels.women,
        }
    }
}

impl GenderEnum {
    pub const ALL: [GenderEnum; 2] = [GenderEnum::Men, GenderEnum::Women];
}
