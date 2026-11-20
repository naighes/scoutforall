//! The events saved with older scouting rules, translated into the current
//! ones (`ScoutingRulesEnum`) when a set is read: the rows stay as they
//! were until the set is written again, with the current rules.
//!
//! From v1 to v2 (TODO/001, §4.2), the block:
//! - `-` let the ball go back to the team attacking, who attacked again:
//!   it's `!` now, `-` keeping the ball with the blockers;
//! - `+` followed by their freeball (`OF`) let the ball go back to them
//!   too: it's `!`, `+` keeping the ball with the blockers.
//!
//! From v2 to v3, the dig `!` was a dig of their attack with few options
//! left: it's `+` now, `!` being only a cover (after the own attack `!`).
//! The attack `+` (we played the ball again) stays: one of the roads it
//! opens now.
//!
//! From v3 to v4, the block only of an attack, as in DataVolley: the
//! block of a ball coming over without one (after their freeball, after
//! our serve `/`; on both sides after the other side's freeball, or its
//! reception or dig straight over) is told as what it was: `#` an attack
//! point, `=` an attack error, `!` (the ball back to them) our freeball;
//! `+` and `-` kept the ball, like a freeball received, which isn't told:
//! left out, with the dig of the blockers after them. The invasion stays.
//!
//! A set the translation can't make follow the current rules doesn't
//! replay, with the error of the replay: never a guess.
use crate::{
    domain::Side,
    shapes::{
        enums::{EvalEnum, EventTypeEnum, ScoutingRulesEnum},
        snapshot::EventEntry,
    },
};

/// The touches of a set (one side's events, or both sides' in their
/// order) saved with `rules` (none: v1), as the current rules tell them.
pub fn translate(
    rules: Option<ScoutingRulesEnum>,
    touches: &[(Side, EventEntry)],
) -> Vec<(Side, EventEntry)> {
    translate_rows(rules, touches)
        .into_iter()
        .flatten()
        .collect()
}

/// The touches as [`translate`] tells them, each in its place: `None` for
/// the ones the current rules leave out, so that the rows of a set keep
/// their events.
pub fn translate_rows(
    rules: Option<ScoutingRulesEnum>,
    touches: &[(Side, EventEntry)],
) -> Vec<Option<(Side, EventEntry)>> {
    match rules.unwrap_or(ScoutingRulesEnum::V1) {
        ScoutingRulesEnum::V1 => from_v3(&from_v2(&from_v1(touches))),
        ScoutingRulesEnum::V2 => from_v3(&from_v2(touches)),
        ScoutingRulesEnum::V3 => from_v3(touches),
        ScoutingRulesEnum::V4 => touches.iter().cloned().map(Some).collect(),
    }
}

/// The next touch of the side after the `i`-th, the changes of the lineup
/// aside.
fn next_of(touches: &[(Side, EventEntry)], i: usize, side: Side) -> Option<&EventEntry> {
    touches
        .iter()
        .skip(i.saturating_add(1))
        .filter(|(s, e)| *s == side && !e.event_type.changes_lineup())
        .map(|(_, e)| e)
        .next()
}

/// The touch of the ball before the `i`-th, of either side, the changes
/// of the lineup aside, with its place.
fn previous(touches: &[(Side, EventEntry)], i: usize) -> Option<(usize, &(Side, EventEntry))> {
    touches
        .iter()
        .enumerate()
        .take(i)
        .rev()
        .find(|(_, (_, e))| !e.event_type.changes_lineup())
}

/// Whether the `i`-th touch is a block (not an invasion) of a ball coming
/// over without an attack: after their freeball or our serve `/` in the
/// stream of its side; after the other side's freeball, or its reception
/// or dig straight over.
fn block_of_no_attack(touches: &[(Side, EventEntry)], i: usize) -> bool {
    use EvalEnum::*;
    use EventTypeEnum::*;
    let Some((side, entry)) = touches.get(i) else {
        return false;
    };
    let before = previous(touches, i).map(|(_, (s, e))| (s == side, e.event_type, e.eval));
    entry.event_type == B
        && entry.eval != Some(Over)
        && matches!(
            before,
            Some(
                (true, OF, _) | (true, S, Some(Over)) | (false, FB, _) | (false, P | D, Some(Over))
            )
        )
}

fn from_v3(touches: &[(Side, EventEntry)]) -> Vec<Option<(Side, EventEntry)>> {
    use EvalEnum::*;
    use EventTypeEnum::*;
    touches
        .iter()
        .enumerate()
        .map(|(i, (side, entry))| {
            // the dig of the blockers after a block of no attack keeping
            // the ball: left out with it
            let after_kept = entry.event_type == D
                && previous(touches, i).is_some_and(|(j, (s, block))| {
                    s == side
                        && matches!(block.eval, Some(Positive | Negative))
                        && block_of_no_attack(touches, j)
                });
            let told = |event_type: EventTypeEnum, eval: Option<EvalEnum>| {
                Some((
                    *side,
                    EventEntry {
                        event_type,
                        eval,
                        technique: None,
                        ..entry.clone()
                    },
                ))
            };
            match (block_of_no_attack(touches, i), entry.eval, after_kept) {
                (_, _, true) => None,
                (true, Some(Perfect), _) => told(A, Some(Perfect)),
                (true, Some(Error), _) => told(A, Some(Error)),
                (true, Some(Exclamative), _) => told(FB, None),
                (true, _, _) => None,
                (false, _, false) => Some((*side, entry.clone())),
            }
        })
        .collect()
}

fn from_v1(touches: &[(Side, EventEntry)]) -> Vec<(Side, EventEntry)> {
    use EvalEnum::*;
    use EventTypeEnum::*;
    touches
        .iter()
        .enumerate()
        .map(|(i, (side, entry))| {
            let next = next_of(touches, i, *side).map(|e| e.event_type);
            let eval = match (entry.event_type, entry.eval, next) {
                (B, Some(Negative), _) | (B, Some(Positive), Some(OF)) => Some(Exclamative),
                (_, eval, _) => eval,
            };
            (
                *side,
                EventEntry {
                    eval,
                    ..entry.clone()
                },
            )
        })
        .collect()
}

fn from_v2(touches: &[(Side, EventEntry)]) -> Vec<(Side, EventEntry)> {
    touches
        .iter()
        .map(|(side, entry)| {
            let eval = match (entry.event_type, entry.eval) {
                (EventTypeEnum::D, Some(EvalEnum::Exclamative)) => Some(EvalEnum::Positive),
                (_, eval) => eval,
            };
            (
                *side,
                EventEntry {
                    eval,
                    ..entry.clone()
                },
            )
        })
        .collect()
}
