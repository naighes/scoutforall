//! Why our scouting refuses an event: told as data, the players by their
//! id (or, for whoever shows it, by their number), so that the import of a
//! file can say it in the language of the app.

use crate::shapes::{
    enums::{EvalEnum, EventTypeEnum},
    technique::TechniqueEnum,
};
use std::fmt::{self, Display, Formatter};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal<P = Uuid> {
    /// The set is won already.
    SetOver,
    /// The event can't follow the ones before it.
    NotNow {
        available: Vec<EventTypeEnum>,
    },
    MissingEvaluation,
    EvaluationNotAllowed(EvalEnum),
    UnexpectedEvaluation,
    TechniqueNotAllowed(TechniqueEnum),
    MissingPlayer,
    UnexpectedPlayer,
    /// The player isn't on court (or, for a substitution, in the game).
    NotOnCourt(P),
    /// The libero attacking, setting as the setter, or replacing itself.
    Libero(P),
    /// A player of the back row blocking.
    BackRowBlock(P),
    /// The serve of a player not in position 1.
    NotServing(P),
    MissingReplacement,
    UnexpectedReplacement,
    /// The player can't come in: in the game already, or a libero.
    CantComeIn(P),
    /// The player went out once already.
    AlreadyReplaced(P),
    /// The player came in once already.
    AlreadyReplacement(P),
    /// The libero substituted.
    LiberoReplaced,
    MaxSubstitutions,
    /// The player who came in goes out only for the one they replaced.
    OnlyReplacedBy {
        replaced: P,
        by: P,
    },
}

impl<P> Refusal<P> {
    /// The same refusal, each player told by `f`.
    pub fn map<Q>(self, f: impl Fn(P) -> Q) -> Refusal<Q> {
        use Refusal::*;
        match self {
            SetOver => SetOver,
            NotNow { available } => NotNow { available },
            MissingEvaluation => MissingEvaluation,
            EvaluationNotAllowed(eval) => EvaluationNotAllowed(eval),
            UnexpectedEvaluation => UnexpectedEvaluation,
            TechniqueNotAllowed(technique) => TechniqueNotAllowed(technique),
            MissingPlayer => MissingPlayer,
            UnexpectedPlayer => UnexpectedPlayer,
            NotOnCourt(p) => NotOnCourt(f(p)),
            Libero(p) => Libero(f(p)),
            BackRowBlock(p) => BackRowBlock(f(p)),
            NotServing(p) => NotServing(f(p)),
            MissingReplacement => MissingReplacement,
            UnexpectedReplacement => UnexpectedReplacement,
            CantComeIn(p) => CantComeIn(f(p)),
            AlreadyReplaced(p) => AlreadyReplaced(f(p)),
            AlreadyReplacement(p) => AlreadyReplacement(f(p)),
            LiberoReplaced => LiberoReplaced,
            MaxSubstitutions => MaxSubstitutions,
            OnlyReplacedBy { replaced, by } => OnlyReplacedBy {
                replaced: f(replaced),
                by: f(by),
            },
        }
    }
}

/// The technical detail, in English, as the errors of the snapshot and the
/// lineup have always told it.
impl Display for Refusal<Uuid> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use Refusal::*;
        match self {
            SetOver => write!(f, "the set is over"),
            NotNow { available } => {
                write!(f, "not available, the available events are {available:?}")
            }
            MissingEvaluation => write!(f, "missing evaluation"),
            EvaluationNotAllowed(eval) => write!(f, "evaluation {eval} not allowed"),
            UnexpectedEvaluation => write!(f, "unexpected evaluation"),
            TechniqueNotAllowed(technique) => write!(f, "technique {technique} not allowed"),
            MissingPlayer => write!(f, "missing player"),
            UnexpectedPlayer => write!(f, "unexpected player"),
            NotOnCourt(p) | Libero(p) | BackRowBlock(p) | NotServing(p) => {
                write!(f, "player {p} can't play it")
            }
            MissingReplacement => write!(f, "missing replacement"),
            UnexpectedReplacement => write!(f, "unexpected replacement"),
            CantComeIn(p) => write!(f, "player {p} can't come in"),
            AlreadyReplaced(p) => write!(f, "player {p:?} was already replaced"),
            AlreadyReplacement(p) => write!(f, "player {p:?} was already a replacement"),
            LiberoReplaced => write!(f, "cannot replace the libero player"),
            MaxSubstitutions => write!(f, "max number of substitutions was reached"),
            OnlyReplacedBy { replaced, by } => write!(
                f,
                "player {replaced:?} can be only replaced by player {by:?}"
            ),
        }
    }
}
