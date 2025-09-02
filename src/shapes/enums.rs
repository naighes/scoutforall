use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::errors::{AppError, IOError};

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
        match s.to_uppercase().as_str() {
            "break" => Ok(PhaseEnum::Break),
            "side-out" => Ok(PhaseEnum::SideOut),
            _ => Err(AppError::IO(IOError::EncodingError(format!(
                "invalid phase: {}",
                s
            )))),
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
        let label = match self {
            TeamSideEnum::Us => "us",
            TeamSideEnum::Them => "them",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for TeamSideEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "us" => Ok(TeamSideEnum::Us),
            "them" => Ok(TeamSideEnum::Them),
            _ => Err(AppError::IO(IOError::EncodingError(format!(
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
}

impl fmt::Display for EventTypeEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            EventTypeEnum::S => "S",
            EventTypeEnum::P => "P",
            EventTypeEnum::A => "A",
            EventTypeEnum::D => "D",
            EventTypeEnum::B => "B",
            EventTypeEnum::F => "F",
            EventTypeEnum::OS => "OS",
            EventTypeEnum::OE => "OE",
            EventTypeEnum::R => "R",
        };
        write!(f, "{}", label)
    }
}

impl EventTypeEnum {
    pub fn friendly_name(&self) -> &'static str {
        match self {
            EventTypeEnum::S => "serve",
            EventTypeEnum::P => "pass",
            EventTypeEnum::A => "attack",
            EventTypeEnum::D => "dig",
            EventTypeEnum::B => "block",
            EventTypeEnum::F => "fault",
            EventTypeEnum::OS => "opponent score",
            EventTypeEnum::OE => "opponent error",
            EventTypeEnum::R => "substitution",
        }
    }
}

impl FromStr for EventTypeEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "S" => Ok(EventTypeEnum::S),
            "P" => Ok(EventTypeEnum::P),
            "A" => Ok(EventTypeEnum::A),
            "D" => Ok(EventTypeEnum::D),
            "B" => Ok(EventTypeEnum::B),
            "F" => Ok(EventTypeEnum::F),
            "OS" => Ok(EventTypeEnum::OS),
            "OE" => Ok(EventTypeEnum::OE),
            "R" => Ok(EventTypeEnum::R),
            _ => Err(AppError::IO(IOError::EncodingError(format!(
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

impl fmt::Display for EvalEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            EvalEnum::Perfect => "#",
            EvalEnum::Positive => "+",
            EvalEnum::Exclamative => "!",
            EvalEnum::Over => "/",
            EvalEnum::Error => "=",
            EvalEnum::Negative => "-",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for EvalEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "#" => Ok(EvalEnum::Perfect),
            "+" => Ok(EvalEnum::Positive),
            "!" => Ok(EvalEnum::Exclamative),
            "/" => Ok(EvalEnum::Over),
            "=" => Ok(EvalEnum::Error),
            "-" => Ok(EvalEnum::Negative),
            _ => Err(AppError::IO(IOError::EncodingError(format!(
                "invalid eval: {}",
                s
            )))),
        }
    }
}

impl EvalEnum {
    pub fn friendly_description(&self, event: EventTypeEnum) -> Option<String> {
        match (self, event) {
            (EvalEnum::Perfect, EventTypeEnum::A) => Some("winning attack".to_string()),
            (EvalEnum::Positive, EventTypeEnum::A) => Some("attack continuation".to_string()),
            (EvalEnum::Negative, EventTypeEnum::A) => {
                Some("opponent counter-attack opportunity".to_string())
            }

            (EvalEnum::Perfect, EventTypeEnum::B) => Some("winning block".to_string()),
            (EvalEnum::Positive, EventTypeEnum::B) => {
                Some("counter-attack opportunity".to_string())
            }
            (EvalEnum::Negative, EventTypeEnum::B) => {
                Some("opponent counter-attack opportunity".to_string())
            }

            (EvalEnum::Positive, EventTypeEnum::D) => {
                Some("first-tempo still available".to_string())
            }
            (EvalEnum::Exclamative, EventTypeEnum::D) => {
                Some("few attack options available".to_string())
            }
            (EvalEnum::Negative, EventTypeEnum::D) => Some("limited attack options".to_string()),
            (EvalEnum::Over, EventTypeEnum::D) => {
                Some("ball goes straight over the net".to_string())
            }

            (EvalEnum::Positive, EventTypeEnum::P) => {
                Some("first-tempo still available".to_string())
            }
            (EvalEnum::Exclamative, EventTypeEnum::P) => {
                Some("few attack options available".to_string())
            }
            (EvalEnum::Negative, EventTypeEnum::P) => Some("limited attack options".to_string()),
            (EvalEnum::Over, EventTypeEnum::P) => {
                Some("ball goes straight over the net".to_string())
            }

            (EvalEnum::Perfect, EventTypeEnum::S) => None,
            (EvalEnum::Positive, EventTypeEnum::S) => {
                Some("opponent with limited attack options".to_string())
            }
            (EvalEnum::Negative, EventTypeEnum::S) => {
                Some("opponent with full attack options".to_string())
            }
            (EvalEnum::Over, EventTypeEnum::S) => {
                Some("ball goes straight back to our court".to_string())
            }
            _ => None,
        }
    }

    pub fn friendly_name(&self, event: EventTypeEnum) -> String {
        match (self, event) {
            (EvalEnum::Perfect, EventTypeEnum::A) => "score".to_string(),
            (EvalEnum::Positive, EventTypeEnum::A) => "positive".to_string(),
            (EvalEnum::Negative, EventTypeEnum::A) => "negative".to_string(),
            (EvalEnum::Error, EventTypeEnum::A) => "error".to_string(),
            (EvalEnum::Over, EventTypeEnum::A) => "blocked".to_string(),

            (EvalEnum::Perfect, EventTypeEnum::B) => "winning block".to_string(),
            (EvalEnum::Positive, EventTypeEnum::B) => "positive".to_string(),
            (EvalEnum::Negative, EventTypeEnum::B) => "negative".to_string(),
            (EvalEnum::Error, EventTypeEnum::B) => "error".to_string(),
            (EvalEnum::Over, EventTypeEnum::B) => "net fault".to_string(),

            (EvalEnum::Perfect, EventTypeEnum::D) => "perfect".to_string(),
            (EvalEnum::Positive, EventTypeEnum::D) => "positive".to_string(),
            (EvalEnum::Exclamative, EventTypeEnum::D) => "subpositive".to_string(),
            (EvalEnum::Negative, EventTypeEnum::D) => "negative".to_string(),
            (EvalEnum::Error, EventTypeEnum::D) => "error".to_string(),
            (EvalEnum::Over, EventTypeEnum::D) => "overpass".to_string(),

            (EvalEnum::Perfect, EventTypeEnum::P) => "perfect".to_string(),
            (EvalEnum::Positive, EventTypeEnum::P) => "positive".to_string(),
            (EvalEnum::Exclamative, EventTypeEnum::P) => "subpositive".to_string(),
            (EvalEnum::Negative, EventTypeEnum::P) => "negative".to_string(),
            (EvalEnum::Error, EventTypeEnum::P) => "error".to_string(),
            (EvalEnum::Over, EventTypeEnum::P) => "overpass".to_string(),

            (EvalEnum::Perfect, EventTypeEnum::S) => "ace".to_string(),
            (EvalEnum::Positive, EventTypeEnum::S) => "positive".to_string(),
            (EvalEnum::Negative, EventTypeEnum::S) => "negative".to_string(),
            (EvalEnum::Error, EventTypeEnum::S) => "error".to_string(),
            (EvalEnum::Over, EventTypeEnum::S) => "overpass".to_string(),

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
        let label = match self {
            ZoneEnum::One => "1",
            ZoneEnum::Two => "2",
            ZoneEnum::Three => "3",
            ZoneEnum::Four => "4",
            ZoneEnum::Five => "5",
            ZoneEnum::Six => "6",
            ZoneEnum::Seven => "7",
            ZoneEnum::Eight => "8",
            ZoneEnum::Nine => "9",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for ZoneEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "1" => Ok(ZoneEnum::One),
            "2" => Ok(ZoneEnum::Two),
            "3" => Ok(ZoneEnum::Three),
            "4" => Ok(ZoneEnum::Four),
            "5" => Ok(ZoneEnum::Five),
            "6" => Ok(ZoneEnum::Six),
            "7" => Ok(ZoneEnum::Seven),
            "8" => Ok(ZoneEnum::Eight),
            "9" => Ok(ZoneEnum::Nine),
            _ => Err(AppError::IO(IOError::EncodingError(format!(
                "invalid zone: {}",
                s
            )))),
        }
    }
}

impl TryFrom<u8> for ZoneEnum {
    type Error = String;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(ZoneEnum::One),
            2 => Ok(ZoneEnum::Two),
            3 => Ok(ZoneEnum::Three),
            4 => Ok(ZoneEnum::Four),
            5 => Ok(ZoneEnum::Five),
            6 => Ok(ZoneEnum::Six),
            7 => Ok(ZoneEnum::Seven),
            8 => Ok(ZoneEnum::Eight),
            9 => Ok(ZoneEnum::Nine),
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
        let label = match self {
            ErrorTypeEnum::Forced => "forced",
            ErrorTypeEnum::Unforced => "unforced",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for ErrorTypeEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "forced" => Ok(ErrorTypeEnum::Forced),
            "unforced" => Ok(ErrorTypeEnum::Unforced),
            _ => Err(AppError::IO(IOError::EncodingError(format!(
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RotationEnum {
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
}

impl fmt::Display for RotationEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            RotationEnum::One => "1",
            RotationEnum::Two => "2",
            RotationEnum::Three => "3",
            RotationEnum::Four => "4",
            RotationEnum::Five => "5",
            RotationEnum::Six => "6",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for RotationEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "1" => Ok(RotationEnum::One),
            "2" => Ok(RotationEnum::Two),
            "3" => Ok(RotationEnum::Three),
            "4" => Ok(RotationEnum::Four),
            "5" => Ok(RotationEnum::Five),
            "6" => Ok(RotationEnum::Six),
            _ => Err(AppError::IO(IOError::EncodingError(format!(
                "invalid rotation: {}",
                s
            )))),
        }
    }
}

impl TryFrom<u8> for RotationEnum {
    type Error = String;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(RotationEnum::One),
            2 => Ok(RotationEnum::Two),
            3 => Ok(RotationEnum::Three),
            4 => Ok(RotationEnum::Four),
            5 => Ok(RotationEnum::Five),
            6 => Ok(RotationEnum::Six),
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
        let label = match self {
            RoleEnum::Libero => "libero",
            RoleEnum::MiddleBlocker => "middle-blocker",
            RoleEnum::OppositeHitter => "opposite-hitter",
            RoleEnum::OutsideHitter => "outside-hitter",
            RoleEnum::Setter => "setter",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for RoleEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "libero" => Ok(RoleEnum::Libero),
            "middle-blocker" => Ok(RoleEnum::MiddleBlocker),
            "opposite-hitter" => Ok(RoleEnum::OppositeHitter),
            "outside-hitter" => Ok(RoleEnum::OutsideHitter),
            "setter" => Ok(RoleEnum::Setter),
            _ => Err(AppError::IO(IOError::EncodingError(format!(
                "invalid role: {}",
                s
            )))),
        }
    }
}
