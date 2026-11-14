//! A set of DataVolley on both sides (`scouting = both`): each team
//! translated as ours (`translate`), then the real touches of the two, in
//! the order of the lines of the file, as one sequence of touches, the one
//! saved; what a side's translation adds for the other team (`OS`, `OE`,
//! `OF`) is left out, the projection telling it again (`rally`). The
//! sequence must replay on both sides.
use crate::{
    domain::Side,
    dvw::{
        model::DvwFile,
        translate::{Issue, Mappings, TranslateError, Us, translate_lined},
    },
    rally::{answers, replay},
    shapes::{
        enums::{EvalEnum, EventTypeEnum, TeamSideEnum},
        set::SetEntry,
        snapshot::EventEntry,
    },
};
use chrono::{DateTime, FixedOffset};

/// A set on both sides: the set of each team as its own translation has
/// it (its lineup, the serving team as it sees it, its stream), and the
/// touches of the two in order.
#[derive(Debug, Clone, PartialEq)]
pub struct BothSet {
    pub a: SetEntry,
    pub b: SetEntry,
    pub touches: Vec<(Side, EventEntry)>,
}

/// Whether the event is told by the other side's touches: never saved.
fn told(entry: &EventEntry) -> bool {
    matches!(
        entry.event_type,
        EventTypeEnum::OS | EventTypeEnum::OE | EventTypeEnum::OF
    )
}

/// Whether the event touches the ball: a substitution, a libero or setter
/// change comes before the touch at the same line.
fn off_the_ball(entry: &EventEntry) -> bool {
    matches!(
        entry.event_type,
        EventTypeEnum::R
            | EventTypeEnum::CL
            | EventTypeEnum::LR
            | EventTypeEnum::LF
            | EventTypeEnum::CS
    )
}

/// A touch of the ball by the side, not what it's told.
fn played_by(touch: &(Side, EventEntry), side: Side) -> bool {
    touch.0 == side && !off_the_ball(&touch.1) && !told(&touch.1)
}

/// The team as the side sees it, as a side.
fn absolute(side: Side, team: TeamSideEnum) -> Side {
    match team {
        TeamSideEnum::Us => side,
        TeamSideEnum::Them => side.other(),
    }
}

/// The side of the net the touch of the side leaves the ball on, while the
/// rally goes on.
fn ball_after(side: Side, entry: &EventEntry) -> Option<Side> {
    entry
        .event_type
        .ball_side_after(entry.eval)
        .map(|t| absolute(side, t))
}

/// The events of the two sides, each with the line it comes from, in the
/// order of the file: at the same line the changes first, then A's, each
/// side's in its order.
fn sorted(
    a: &[EventEntry],
    lines_a: &[usize],
    b: &[EventEntry],
    lines_b: &[usize],
) -> Vec<(Side, EventEntry)> {
    let side = |side: Side, events: &[EventEntry], lines: &[usize]| {
        events
            .iter()
            .zip(lines)
            .enumerate()
            .map(|(index, (entry, line))| {
                (
                    (*line, !off_the_ball(entry), side == Side::B, index),
                    (side, entry.clone()),
                )
            })
            .collect::<Vec<_>>()
    };
    let mut events = side(Side::A, a, lines_a);
    events.extend(side(Side::B, b, lines_b));
    events.sort_by_key(|(key, _)| *key);
    // a freeball received after an attack, one side's dig, the other's
    // freeball sent at the same line: the sender's
    let sent: Vec<(usize, Side)> = events
        .iter()
        .filter(|(_, (_, e))| e.event_type == EventTypeEnum::FB)
        .map(|((line, ..), (side, _))| (*line, *side))
        .collect();
    events
        .into_iter()
        .filter(|((line, ..), (side, e))| {
            e.event_type != EventTypeEnum::D || !sent.contains(&(*line, side.other()))
        })
        .map(|(_, event)| event)
        .collect()
}

/// The freeballs as the sender's: a side's freeball received (`OF`) is
/// left out where the other side's last touch before the side's next one
/// sends the ball over (its freeball, `FB`, or a reception over);
/// otherwise it's the other side's freeball, sent before that touch.
fn freeballs(events: Vec<(Side, EventEntry)>) -> Vec<(Side, EventEntry)> {
    let received = events
        .iter()
        .position(|(_, e)| e.event_type == EventTypeEnum::OF);
    match received {
        None => events,
        Some(index) => {
            let side = events.get(index).map_or(Side::A, |(s, _)| *s);
            let next_touch = events
                .iter()
                .enumerate()
                .skip(index + 1)
                .find(|(_, t)| played_by(t, side))
                .map_or(events.len(), |(i, _)| i);
            // the other side's last touch before the side's next one
            // sending the ball over already: its freeball, or a reception
            // over
            let over = events
                .get(..next_touch)
                .unwrap_or_default()
                .iter()
                .rev()
                .take_while(|t| !played_by(t, side))
                .find(|t| played_by(t, side.other()))
                .is_some_and(|(s, e)| ball_after(*s, e) == Some(side));
            let freeball = events.get(index).map(|(_, e)| {
                (
                    side.other(),
                    EventEntry {
                        event_type: EventTypeEnum::FB,
                        ..e.clone()
                    },
                )
            });
            let mut rest: Vec<(Side, EventEntry)> = events
                .into_iter()
                .enumerate()
                .filter(|(i, _)| *i != index)
                .map(|(_, t)| t)
                .collect();
            if let (false, Some(freeball)) = (over, freeball) {
                rest.insert(next_touch.saturating_sub(1).min(rest.len()), freeball);
            }
            freeballs(rest)
        }
    }
}

/// The evaluations to try for a touch answered by the other side's, most
/// alike first.
fn preferred(touch: EventTypeEnum, answer: &EventEntry) -> [EvalEnum; 6] {
    use EvalEnum::*;
    use EventTypeEnum::*;
    match (touch, answer.event_type, answer.eval) {
        (S, _, Some(Exclamative)) => [Exclamative, Positive, Negative, Perfect, Over, Error],
        (S, _, Some(Positive | Perfect)) => [Negative, Exclamative, Positive, Perfect, Over, Error],
        (S, _, _) => [Positive, Exclamative, Negative, Perfect, Over, Error],
        (A, B, _) => [Over, Exclamative, Negative, Positive, Perfect, Error],
        _ => [Negative, Positive, Exclamative, Perfect, Over, Error],
    }
}

/// Whether the touch of `side` tells what its answer by the other side
/// does: the same winner, or the ball left on the same side.
fn agrees(side: Side, touch: &EventEntry, answer: &EventEntry) -> bool {
    let other = side.other();
    let winner =
        |s: Side, e: &EventEntry| e.event_type.rally_winner(e.eval).map(|t| absolute(s, t));
    winner(side, touch) == winner(other, answer)
        && ball_after(side, touch) == ball_after(other, answer)
}

/// The evaluation of a touch answered by the other side (a serve by the
/// reception, an attack by the dig or the block) as the answer tells it:
/// its own when it agrees, otherwise the most alike that does.
fn answered(events: Vec<(Side, EventEntry)>) -> Vec<(Side, EventEntry)> {
    let next_touch = |from: usize| {
        events
            .iter()
            .skip(from + 1)
            .find(|(_, e)| !off_the_ball(e) && !told(e))
    };
    events
        .iter()
        .enumerate()
        .map(|(index, (side, entry))| match next_touch(index) {
            Some((other, answer))
                if *other != *side
                    && answers(answer.event_type, entry.event_type)
                    && !agrees(*side, entry, answer) =>
            {
                let eval = preferred(entry.event_type, answer)
                    .into_iter()
                    .map(|eval| EventEntry {
                        eval: Some(eval),
                        ..entry.clone()
                    })
                    .find(|e| agrees(*side, e, answer));
                (*side, eval.unwrap_or_else(|| entry.clone()))
            }
            _ => (*side, entry.clone()),
        })
        .collect()
}

/// The touches of the two sides in the order of the file, as both sides
/// tell them: the freeballs as the sender's, the touches answered as their
/// answers tell them, what's told by the other side left out.
pub(crate) fn fused(
    a: &[EventEntry],
    lines_a: &[usize],
    b: &[EventEntry],
    lines_b: &[usize],
) -> Vec<(Side, EventEntry)> {
    points(answered(freeballs(sorted(a, lines_a, b, lines_b))))
}

/// What's told by the other side left out, but the point of a rally no
/// touch ends (a fault of the team, written by the file without a touch):
/// the projection tells it to the other side. The other side's point
/// (`OS`) when the side telling it hasn't the ball, otherwise the error
/// the other side tells (`OE`, the side's fault).
fn points(events: Vec<(Side, EventEntry)>) -> Vec<(Side, EventEntry)> {
    #[derive(Default)]
    struct Rally {
        kept: Vec<(Side, EventEntry)>,
        // no touch has ended the rally yet
        open: bool,
        // where the last touch left the ball
        ball: Option<Side>,
        // what each side tells of the end, at the end of the rally
        told: Vec<(Side, EventEntry)>,
    }
    impl Rally {
        fn flush(mut self) -> Self {
            let ball = self.ball;
            let point = self
                .told
                .iter()
                .find(|(s, e)| e.event_type == EventTypeEnum::OS && ball != Some(*s))
                .or_else(|| {
                    self.told
                        .iter()
                        .find(|(_, e)| e.event_type == EventTypeEnum::OE)
                })
                .cloned();
            if let (true, Some(point)) = (self.open, point) {
                self.kept.push(point);
                self.open = false;
            }
            self.told.clear();
            self
        }
    }
    let rally = events
        .into_iter()
        .fold(Rally::default(), |mut rally, (side, entry)| {
            match (entry.event_type, told(&entry)) {
                (EventTypeEnum::OS | EventTypeEnum::OE, true) => {
                    rally.told.push((side, entry));
                    rally
                }
                (_, true) => rally,
                (event_type, false) => {
                    let mut rally = rally.flush();
                    let ends = event_type.rally_winner(entry.eval).is_some();
                    rally.open = match (event_type, ends) {
                        (_, true) => false,
                        (EventTypeEnum::S, false) => true,
                        _ => rally.open,
                    };
                    rally.ball = match off_the_ball(&entry) {
                        true => rally.ball,
                        false => ball_after(side, &entry),
                    };
                    rally.kept.push((side, entry));
                    rally
                }
            }
        })
        .flush();
    rally.kept
}

/// The set on both sides, A the team of `a`, B the other one of the file;
/// the mappings of both translations. A side that doesn't translate, or a
/// sequence that doesn't replay on both, stops the set.
pub fn translate_both(
    file: &DvwFile,
    a: &Us,
    b: &Us,
    set_number: u8,
    date: DateTime<FixedOffset>,
    max_substitutions: u8,
) -> Result<(BothSet, Mappings), TranslateError> {
    let (set_a, mut mappings, lines_a) =
        translate_lined(file, a, set_number, date, max_substitutions)?;
    let (set_b, used_b, lines_b) = translate_lined(file, b, set_number, date, max_substitutions)?;
    let touches = fused(&set_a.events, &lines_a, &set_b.events, &lines_b);
    replay(&set_a, &set_b, &touches, max_substitutions).map_err(|e| TranslateError {
        set_number,
        line: None,
        issue: Issue::Sides(e),
    })?;
    for (name, count) in used_b {
        *mappings.entry(name).or_default() += count;
    }
    Ok((
        BothSet {
            a: set_a,
            b: set_b,
            touches,
        },
        mappings,
    ))
}
