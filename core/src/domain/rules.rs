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
    match rules.unwrap_or(ScoutingRulesEnum::V1) {
        ScoutingRulesEnum::V1 => from_v2(&from_v1(touches)),
        ScoutingRulesEnum::V2 => from_v2(touches),
        ScoutingRulesEnum::V3 => touches.to_vec(),
    }
}

/// Whether the event is a change of the lineup, not a touch of the ball.
fn lineup_change(entry: &EventEntry) -> bool {
    use EventTypeEnum::*;
    matches!(entry.event_type, R | CL | LR | LF | CS)
}

/// The next touch of the side after the `i`-th, the changes of the lineup
/// aside.
fn next_of(touches: &[(Side, EventEntry)], i: usize, side: Side) -> Option<&EventEntry> {
    touches
        .iter()
        .skip(i.saturating_add(1))
        .filter(|(s, e)| *s == side && !lineup_change(e))
        .map(|(_, e)| e)
        .next()
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
