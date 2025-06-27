use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::errors::{AppError, IOError};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoleEnum {
    Libero,
    OppositeHitter,
    Setter,
    OutsideHitter,
    MiddleBlocker,
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
