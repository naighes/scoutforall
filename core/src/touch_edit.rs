//! Correcting a touch of a set scouted on both sides: its type, evaluation
//! and set or serve, as `event_edit` does for a set of A alone. The touch
//! answered by another one (a serve and its reception, an attack and its
//! dig or block) tells the same transition: the other touch follows the
//! correction, with the evaluation the pair admits (`rally::answer_evals`),
//! and both are saved together (`SetWriter::update_sides`). The touches
//! after them must still fit, on both sides, and the set must end as it
//! did.
use crate::domain::Side;
use crate::event_edit::{Correcting, Draft, EventEditError};
use crate::rally;
use crate::shapes::enums::{EvalEnum, EventTypeEnum, TeamSideEnum};
use crate::shapes::set::SetEntry;
use crate::shapes::snapshot::{EventEntry, Snapshot};

type Touch = (Side, EventEntry);

/// A touch of a set scouted on both sides being corrected.
#[derive(Debug, Clone)]
pub struct TouchEdit {
    a: SetEntry,
    b: SetEntry,
    touches: Vec<Touch>,
    index: usize,
    max_substitutions: u8,
    winner: Option<TeamSideEnum>,
    side: Side,
    draft: Draft,
}

/// The sides after the first `n` touches.
fn sides_at(
    a: &SetEntry,
    b: &SetEntry,
    touches: &[Touch],
    n: usize,
    max_substitutions: u8,
) -> Result<(Snapshot, Snapshot), EventEditError> {
    rally::replay(a, b, &touches[..n.min(touches.len())], max_substitutions)
        .map_err(|_| EventEditError::Unreadable)
}

impl TouchEdit {
    /// Starts correcting the touch at `index`: `a` and `b` the starts of
    /// the sides, `touches` the touches in order.
    pub fn new(
        a: &SetEntry,
        b: &SetEntry,
        touches: &[Touch],
        index: usize,
        max_substitutions: u8,
    ) -> Result<Self, EventEditError> {
        let (side, event) = match touches.get(index) {
            None => Err(EventEditError::Unreadable),
            Some((_, event)) if event.event_type.changes_lineup() => {
                Err(EventEditError::NotEditable)
            }
            Some(touch) => Ok(touch),
        }?;
        let (before_a, before_b) = sides_at(a, b, touches, index, max_substitutions)?;
        let (all_a, _) = sides_at(a, b, touches, touches.len(), max_substitutions)?;
        Ok(TouchEdit {
            a: a.clone(),
            b: b.clone(),
            touches: touches.to_vec(),
            index,
            max_substitutions,
            winner: all_a.get_set_winner(a.set_number),
            side: *side,
            draft: Draft {
                before: match side {
                    Side::A => before_a,
                    Side::B => before_b,
                },
                event: event.clone(),
            },
        })
    }

    /// The side playing the touch.
    pub fn side(&self) -> Side {
        self.side
    }

    /// The touches with `event` in place of the one corrected, and the
    /// touch it is paired with adjusted: its evaluation kept when the pair
    /// admits it, else the first the pair admits; `None` when the pair
    /// admits none.
    fn resolved(&self, event: &EventEntry) -> Option<Vec<Touch>> {
        let i = self.index;
        // the touch answers the one before it
        let answered = i
            .checked_sub(1)
            .and_then(|p| Some((p, self.touches.get(p)?)))
            .filter(|(_, (side, e))| {
                *side != self.side && rally::answers(event.event_type, e.event_type)
            })
            .map(|(p, (_, answered))| answered_eval(answered, event).map(|eval| (p, eval)));
        // and the one after it answers the touch
        let answer = self
            .touches
            .get(i + 1)
            .filter(|(side, e)| {
                *side != self.side && rally::answers(e.event_type, event.event_type)
            })
            .map(|(_, answer)| answer_eval(event, answer).map(|eval| (i + 1, Some(eval))));
        let paired: Option<Vec<(usize, Option<EvalEnum>)>> =
            [answered, answer].into_iter().flatten().collect();
        let mut touches = self.touches.clone();
        *touches.get_mut(i)? = (self.side, event.clone());
        paired?
            .into_iter()
            .try_fold(touches, |mut touches, (k, eval)| {
                touches.get_mut(k)?.1.eval = eval;
                Some(touches)
            })
    }

    /// Whether the touches differing from the saved ones are admitted where
    /// they stand, the sides replay and the set ends as it did.
    fn fits(&self, touches: &[Touch]) -> bool {
        let Ok((a, _)) = sides_at(
            &self.a,
            &self.b,
            touches,
            touches.len(),
            self.max_substitutions,
        ) else {
            return false;
        };
        a.get_set_winner(self.a.set_number) == self.winner
            && touches
                .iter()
                .zip(&self.touches)
                .enumerate()
                .filter(|(_, (new, old))| new != old)
                .all(|(k, ((side, entry), _))| self.admitted(touches, k, *side, entry))
    }

    /// Whether the touch `entry` of `side` is admitted after the first `k`
    /// touches of `touches`.
    fn admitted(&self, touches: &[Touch], k: usize, side: Side, entry: &EventEntry) -> bool {
        sides_at(&self.a, &self.b, touches, k, self.max_substitutions).is_ok_and(|(a, b)| {
            rally::admits(touches.get(..k).unwrap_or(touches), &a, &b, side, entry)
        })
    }

    fn valid(&self, event: &EventEntry) -> bool {
        self.resolved(event).is_some_and(|t| self.fits(&t))
    }

    /// The touches with the correction, to save: the touch and the one
    /// paired with it changed, every touch after them still admitted on its
    /// side (but the ones that weren't), and the set ending as it did.
    pub fn apply(&self) -> Result<Vec<Touch>, EventEditError> {
        let touches = self
            .resolved(&self.draft.event)
            .ok_or(EventEditError::DoesNotFit)?;
        let (a, _) = sides_at(
            &self.a,
            &self.b,
            &touches,
            touches.len(),
            self.max_substitutions,
        )
        .map_err(|_| EventEditError::DoesNotFit)?;
        match a.get_set_winner(self.a.set_number) == self.winner {
            false => Err(EventEditError::ChangesTheResult),
            true if self.breaks(&touches) => Err(EventEditError::DoesNotFit),
            true => Ok(touches),
        }
    }

    /// Whether a touch from the first one changed on is no longer admitted
    /// in `touches`, although it was in the saved ones.
    fn breaks(&self, touches: &[Touch]) -> bool {
        let first = touches
            .iter()
            .zip(&self.touches)
            .position(|(new, old)| new != old)
            .unwrap_or(0);
        touches
            .iter()
            .zip(&self.touches)
            .enumerate()
            .skip(first)
            .any(|(k, ((side, entry), (old_side, old)))| {
                !self.admitted(touches, k, *side, entry)
                    && self.admitted(&self.touches, k, *old_side, old)
            })
    }
}

impl Correcting for TouchEdit {
    fn draft(&self) -> &Draft {
        &self.draft
    }

    fn draft_mut(&mut self) -> &mut Draft {
        &mut self.draft
    }

    fn index(&self) -> usize {
        self.index
    }

    /// The types it can become: the ones the side could play at that point,
    /// by the same player, with an evaluation that keeps the touches fit.
    fn event_types(&self) -> Vec<EventTypeEnum> {
        self.draft
            .event_types()
            .filter(|t| !self.evals_of(*t).is_empty())
            .collect()
    }

    /// The evaluations of `event_type` the side offers at that point that
    /// keep the touches fit: only none for a type having none, if it fits.
    fn evals_of(&self, event_type: EventTypeEnum) -> Vec<Option<EvalEnum>> {
        self.draft
            .evals_of(event_type)
            .into_iter()
            .filter(|eval| self.valid(&self.draft.retyped(event_type, *eval)))
            .collect()
    }
}

/// The evaluation of the touch `answered` that `event` answers: its own
/// when the pair admits it, else the first the pair admits; `None` when
/// the pair admits none.
fn answered_eval(answered: &EventEntry, event: &EventEntry) -> Option<Option<EvalEnum>> {
    let fits = |e: Option<EvalEnum>| {
        event.eval.is_some_and(|eval| {
            rally::answer_evals(answered.event_type, e, event.event_type).contains(&eval)
        })
    };
    std::iter::once(answered.eval)
        .chain(EvalEnum::ALL.into_iter().map(Some))
        .find(|e| fits(*e))
}

/// The evaluation of the touch `answer` answering `event`: its own when
/// the pair admits it, else the first the pair admits; `None` when the
/// pair admits none.
fn answer_eval(event: &EventEntry, answer: &EventEntry) -> Option<EvalEnum> {
    let allowed = rally::answer_evals(event.event_type, event.eval, answer.event_type);
    answer
        .eval
        .filter(|e| allowed.contains(e))
        .or_else(|| allowed.first().copied())
}
