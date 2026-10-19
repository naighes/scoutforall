use crate::{
    errors::{AppError, IOError},
    localization::{Labels, current_labels},
    shapes::{
        enums::{EvalEnum, EventTypeEnum, RoleEnum, WithDesc},
        snapshot::EventEntry,
    },
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

/// The set an attacker receives, in the scouting notation:
///
/// - middle blockers: 1, 2, 3, 4, C, B, 7 and the fast (F);
/// - outside hitters in the front row: high (A), 5, super (S), 9, quick (Q);
/// - the opposite in the front row: high (A), 6, super (S), quick (Q);
/// - outside hitters in the back row: high (A), pipe (0);
/// - the opposite in the back row: high (A), 8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SetTypeEnum {
    One,
    Two,
    Three,
    Four,
    C,
    B,
    Seven,
    Fast,
    High,
    Five,
    Six,
    Super,
    Nine,
    Quick,
    Pipe,
    Eight,
}

/// The serve: float (F) or spin (S).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServeTypeEnum {
    Float,
    Spin,
}

/// How an event was played, saved in the last column of the events: the
/// set received by the attacker, or the serve. Missing in the sets scouted
/// up to v0.4.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TechniqueEnum {
    Set(SetTypeEnum),
    Serve(ServeTypeEnum),
}

impl SetTypeEnum {
    pub const ALL: [SetTypeEnum; 16] = [
        SetTypeEnum::One,
        SetTypeEnum::Two,
        SetTypeEnum::Three,
        SetTypeEnum::Four,
        SetTypeEnum::C,
        SetTypeEnum::B,
        SetTypeEnum::Seven,
        SetTypeEnum::Fast,
        SetTypeEnum::High,
        SetTypeEnum::Five,
        SetTypeEnum::Six,
        SetTypeEnum::Super,
        SetTypeEnum::Nine,
        SetTypeEnum::Quick,
        SetTypeEnum::Pipe,
        SetTypeEnum::Eight,
    ];

    /// The sets an attacker can receive, told by their role and row: none
    /// for the setter (who attacks on the second touch only) and the libero.
    pub fn available(role: RoleEnum, back_row: bool) -> Vec<SetTypeEnum> {
        use RoleEnum::*;
        use SetTypeEnum::*;
        match (role, back_row) {
            (MiddleBlocker, _) => vec![One, Two, Three, Four, C, B, Seven, Fast],
            (OutsideHitter, false) => vec![High, Five, Super, Nine, Quick],
            (OppositeHitter, false) => vec![High, Six, Super, Quick],
            (OutsideHitter, true) => vec![High, Pipe],
            (OppositeHitter, true) => vec![High, Eight],
            (Setter | Libero, _) => vec![],
        }
    }

    /// The set chosen before the scout picks one, told by the role and row
    /// of the attacker and by the touch before the attack (event and
    /// evaluation):
    ///
    /// - middle blockers: 1;
    /// - back row: pipe (0) for the outside hitters, 8 for the opposite;
    /// - front row, after a reception or a dig `#`: super; `+`: 5 for the
    ///   outside hitters, 6 for the opposite; `-` or `!`: high;
    /// - front row, after their reception coming straight back (serve `/`),
    ///   an attack of ours `+` or `!` or a block `+`: super;
    /// - front row, after anything else: high.
    pub fn preselected(
        role: RoleEnum,
        back_row: bool,
        previous: Option<(EventTypeEnum, EvalEnum)>,
    ) -> Option<SetTypeEnum> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        use RoleEnum::*;
        match (role, back_row, previous) {
            (MiddleBlocker, _, _) => Some(SetTypeEnum::One),
            (OutsideHitter, true, _) => Some(SetTypeEnum::Pipe),
            (OppositeHitter, true, _) => Some(SetTypeEnum::Eight),
            (OutsideHitter, false, Some((P | D, Positive))) => Some(SetTypeEnum::Five),
            (OppositeHitter, false, Some((P | D, Positive))) => Some(SetTypeEnum::Six),
            (
                OutsideHitter | OppositeHitter,
                false,
                Some((P | D, Perfect) | (S, Over) | (A, Positive | Exclamative) | (B, Positive)),
            ) => Some(SetTypeEnum::Super),
            (OutsideHitter | OppositeHitter, false, _) => Some(SetTypeEnum::High),
            (Setter | Libero, _, _) => None,
        }
    }

    /// The name of the sets called by a word, e.g. "high" for A.
    pub fn friendly_name(&self, labels: &Labels) -> Option<&'static str> {
        use SetTypeEnum::*;
        match self {
            Fast => Some(labels.fast_set),
            High => Some(labels.high_set),
            Super => Some(labels.super_set),
            Quick => Some(labels.quick_set),
            Pipe => Some(labels.pipe_set),
            One | Two | Three | Four | C | B | Seven | Five | Six | Nine | Eight => None,
        }
    }
}

impl fmt::Display for SetTypeEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use SetTypeEnum::*;
        let label = match self {
            One => "1",
            Two => "2",
            Three => "3",
            Four => "4",
            C => "C",
            B => "B",
            Seven => "7",
            Fast => "F",
            High => "A",
            Five => "5",
            Six => "6",
            Super => "S",
            Nine => "9",
            Quick => "Q",
            Pipe => "0",
            Eight => "8",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for SetTypeEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        SetTypeEnum::ALL
            .into_iter()
            .find(|set| set.to_string() == s.to_uppercase())
            .ok_or_else(|| AppError::IO(IOError::Msg(format!("invalid set type: {}", s))))
    }
}

impl WithDesc<SetTypeEnum> for SetTypeEnum {
    /// the set with its name, if it has one, e.g. "A (high)" or "5"
    fn with_desc(self) -> (SetTypeEnum, String) {
        let desc = match self.friendly_name(current_labels()) {
            Some(name) => format!("{} ({})", self, name),
            None => self.to_string(),
        };
        (self, desc)
    }
}

impl ServeTypeEnum {
    pub const ALL: [ServeTypeEnum; 2] = [ServeTypeEnum::Float, ServeTypeEnum::Spin];

    /// The serve chosen before the scout picks one: the one of the last
    /// serve of the player among the events, float if they didn't serve yet.
    pub fn preselected<'a>(
        server: &Uuid,
        events: impl DoubleEndedIterator<Item = &'a EventEntry>,
    ) -> ServeTypeEnum {
        events
            .rev()
            .find_map(|e| match (e.event_type, e.player, e.technique) {
                (EventTypeEnum::S, Some(player), Some(TechniqueEnum::Serve(serve)))
                    if player == *server =>
                {
                    Some(serve)
                }
                _ => None,
            })
            .unwrap_or(ServeTypeEnum::Float)
    }

    pub fn friendly_name(&self, labels: &Labels) -> &'static str {
        match self {
            ServeTypeEnum::Float => labels.float_serve,
            ServeTypeEnum::Spin => labels.spin_serve,
        }
    }
}

impl fmt::Display for ServeTypeEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            ServeTypeEnum::Float => "F",
            ServeTypeEnum::Spin => "S",
        };
        write!(f, "{}", label)
    }
}

impl FromStr for ServeTypeEnum {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ServeTypeEnum::ALL
            .into_iter()
            .find(|serve| serve.to_string() == s.to_uppercase())
            .ok_or_else(|| AppError::IO(IOError::Msg(format!("invalid serve type: {}", s))))
    }
}

impl WithDesc<ServeTypeEnum> for ServeTypeEnum {
    /// the serve with its name, e.g. "F (float)"
    fn with_desc(self) -> (ServeTypeEnum, String) {
        (
            self,
            format!("{} ({})", self, self.friendly_name(current_labels())),
        )
    }
}

impl TechniqueEnum {
    /// The technique saved with the event: its code means a set for an
    /// attack and a serve for a serve (e.g. "F" is the fast or the float).
    pub fn parse(event_type: EventTypeEnum, code: &str) -> Result<TechniqueEnum, AppError> {
        match event_type {
            EventTypeEnum::A => SetTypeEnum::from_str(code).map(TechniqueEnum::Set),
            EventTypeEnum::S => ServeTypeEnum::from_str(code).map(TechniqueEnum::Serve),
            _ => Err(AppError::IO(IOError::Msg(format!(
                "unexpected technique {} for event {}",
                code, event_type
            )))),
        }
    }

    /// Whether the event can be played with this technique: sets go with
    /// attacks, serves with serves.
    pub fn fits(&self, event_type: EventTypeEnum) -> bool {
        matches!(
            (self, event_type),
            (TechniqueEnum::Set(_), EventTypeEnum::A) | (TechniqueEnum::Serve(_), EventTypeEnum::S)
        )
    }

    /// e.g. "A (high)" or "F (float)"
    pub fn description(&self) -> String {
        match self {
            TechniqueEnum::Set(set) => set.with_desc().1,
            TechniqueEnum::Serve(serve) => serve.with_desc().1,
        }
    }
}

impl fmt::Display for TechniqueEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TechniqueEnum::Set(set) => write!(f, "{}", set),
            TechniqueEnum::Serve(serve) => write!(f, "{}", serve),
        }
    }
}
