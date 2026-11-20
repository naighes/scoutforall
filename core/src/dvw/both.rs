//! A set of DataVolley on both sides (`scouting = both`): each team
//! translated as ours (`translate`), then the real touches of the two, in
//! the order of the lines of the file, as one sequence of touches, the one
//! saved; what a side's translation adds for the other team (`OS`, `OE`,
//! `OF`) is left out, the projection telling it again (`rally`). The
//! touches are ordered by line and, within a line, by the touch of the
//! rally decided (`decide`), so a freeball one side sends untold comes
//! before the touch at its line; a freeball or a point without a touch told
//! by both sides is kept once. The sequence must replay on both sides.
use crate::{
    domain::Side,
    dvw::{
        model::DvwFile,
        translate::{At, Issue, Mappings, TranslateError, Us, translate_lined},
    },
    rally::{Incoherence, answer_evals, answers, replay},
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

/// An event of a side with where it comes from in the file.
pub(crate) type Lined = (Side, EventEntry, At);

/// A touch of the ball by the side, not what it's told.
fn played_by(touch: &Lined, side: Side) -> bool {
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

/// The events of the two sides, each with where it comes from, in the
/// order of the file: by line and touch of the rally (a freeball sent
/// untold is at the line of the touch after it, before it), at the same
/// touch the changes first, then A's, each side's in its order.
fn sorted(a: &[EventEntry], lines_a: &[At], b: &[EventEntry], lines_b: &[At]) -> Vec<Lined> {
    let side = |side: Side, events: &[EventEntry], lines: &[At]| {
        events
            .iter()
            .zip(lines)
            .enumerate()
            .map(|(index, (entry, line))| {
                (
                    (line.0, line.1, !off_the_ball(entry), side == Side::B, index),
                    (side, entry.clone(), *line),
                )
            })
            .collect::<Vec<_>>()
    };
    let mut events = side(Side::A, a, lines_a);
    events.extend(side(Side::B, b, lines_b));
    events.sort_by_key(|(key, _)| *key);
    // a freeball received after an attack, one side's dig, the other's
    // freeball sent at the same line: the sender's
    let sent: Vec<(At, Side)> = events
        .iter()
        .filter(|(_, (_, e, _))| e.event_type == EventTypeEnum::FB)
        .map(|(_, (side, _, line))| (*line, *side))
        .collect();
    events
        .into_iter()
        .filter(|(_, (side, e, line))| {
            e.event_type != EventTypeEnum::D || !sent.contains(&(*line, side.other()))
        })
        .map(|(_, event)| event)
        .collect()
}

/// The freeballs as the sender's: a side's freeball received (`OF`) is
/// left out where the other side's last touch before the side's next one
/// sends the ball over (its freeball, `FB`, or a reception over);
/// otherwise it's the other side's freeball, sent before that touch.
fn freeballs(events: Vec<Lined>) -> Vec<Lined> {
    let received = events
        .iter()
        .position(|(_, e, _)| e.event_type == EventTypeEnum::OF);
    match received {
        None => events,
        Some(index) => {
            let side = events.get(index).map_or(Side::A, |(s, ..)| *s);
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
                .is_some_and(|(s, e, _)| ball_after(*s, e) == Some(side));
            let freeball = events.get(index).map(|(_, e, line)| {
                (
                    side.other(),
                    EventEntry {
                        event_type: EventTypeEnum::FB,
                        ..e.clone()
                    },
                    *line,
                )
            });
            let mut rest: Vec<Lined> = events
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

/// The touches of the two sides in the order of the file, each with its
/// line: the freeballs as the sender's, what's told by the other side left
/// out. The evaluations are the file's.
pub(crate) fn fused_lined(
    a: &[EventEntry],
    lines_a: &[At],
    b: &[EventEntry],
    lines_b: &[At],
) -> Vec<Lined> {
    points(freeballs(sent_told(sorted(a, lines_a, b, lines_b))))
}

/// The freeballs received (`OF`) the other side's freeball sent (`FB`) at
/// the same touch tells (the freeballs `dvw::decide` tells, sent untold
/// too) left out.
fn sent_told(events: Vec<Lined>) -> Vec<Lined> {
    let sent: Vec<(At, Side)> = events
        .iter()
        .filter(|(_, e, _)| e.event_type == EventTypeEnum::FB)
        .map(|(side, _, at)| (*at, *side))
        .collect();
    events
        .into_iter()
        .filter(|(side, e, at)| {
            e.event_type != EventTypeEnum::OF || !sent.contains(&(*at, side.other()))
        })
        .collect()
}

/// `fused_lined` without the lines (the tests).
#[cfg(test)]
pub(crate) fn fused(
    a: &[EventEntry],
    lines_a: &[At],
    b: &[EventEntry],
    lines_b: &[At],
) -> Vec<(Side, EventEntry)> {
    fused_lined(a, lines_a, b, lines_b)
        .into_iter()
        .map(|(side, entry, _)| (side, entry))
        .collect()
}

/// The first answer of a side (a reception, a dig, a block) whose
/// evaluation the touch it answers doesn't admit (`rally::answer_evals`):
/// the two teams of the file telling the same transition differently. Its
/// line, with the issue. Nothing is reconciled: the evaluations saved are
/// always the file's, so such a pair stops the set on both sides.
pub(crate) fn disagreeing(touches: &[Lined]) -> Option<(usize, Issue)> {
    let played: Vec<&Lined> = touches
        .iter()
        .filter(|(_, e, _)| !off_the_ball(e) && !told(e))
        .collect();
    played.windows(2).find_map(|pair| match pair {
        [
            (touched, answered, (answered_line, _)),
            (side, answer, (line, _)),
        ] if touched != side
            && answers(answer.event_type, answered.event_type)
            // the invasion answers nothing: a fault
            && !(answer.event_type == EventTypeEnum::B && answer.eval == Some(EvalEnum::Over))
            && !answer.eval.is_some_and(|eval| {
                answer_evals(answered.event_type, answered.eval, answer.event_type).contains(&eval)
            }) =>
        {
            Some((
                *line,
                Issue::Answer {
                    answered: (answered.event_type, answered.eval),
                    answered_line: *answered_line,
                    answer: (answer.event_type, answer.eval),
                },
            ))
        }
        _ => None,
    })
}

/// What's told by the other side left out, but the point of a rally no
/// touch ends (a fault of the team, written by the file without a touch):
/// the projection tells it to the other side. The other side's point
/// (`OS`) when the side telling it hasn't the ball, otherwise the error
/// the other side tells (`OE`, the side's fault).
fn points(events: Vec<Lined>) -> Vec<Lined> {
    #[derive(Default)]
    struct Rally {
        kept: Vec<Lined>,
        // no touch has ended the rally yet
        open: bool,
        // where the last touch left the ball
        ball: Option<Side>,
        // what each side tells of the end, at the end of the rally
        told: Vec<Lined>,
    }
    impl Rally {
        fn flush(mut self) -> Self {
            let ball = self.ball;
            let point = self
                .told
                .iter()
                .find(|(s, e, _)| e.event_type == EventTypeEnum::OS && ball != Some(*s))
                .or_else(|| {
                    self.told
                        .iter()
                        .find(|(_, e, _)| e.event_type == EventTypeEnum::OE)
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
        .fold(Rally::default(), |mut rally, (side, entry, line)| {
            match (entry.event_type, told(&entry)) {
                (EventTypeEnum::OS | EventTypeEnum::OE, true) => {
                    rally.told.push((side, entry, line));
                    rally
                }
                (_, true) => rally,
                // the other half of a point without a cause the other side
                // tells already: the projection tells it
                (EventTypeEnum::PW | EventTypeEnum::PL, false)
                    if rally.kept.last().is_some_and(|(s, e, at)| {
                        *s == side.other()
                            && at.0 == line.0
                            && e.event_type == opposite(entry.event_type)
                    }) =>
                {
                    rally
                }
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
                    rally.kept.push((side, entry, line));
                    rally
                }
            }
        })
        .flush();
    rally.kept
}

/// The point without a cause the other side tells.
fn opposite(event_type: EventTypeEnum) -> EventTypeEnum {
    match event_type {
        EventTypeEnum::PW => EventTypeEnum::PL,
        EventTypeEnum::PL => EventTypeEnum::PW,
        other => other,
    }
}

/// The set on both sides, A the team of `a`, B the other one of the file;
/// the mappings of both translations. A side that doesn't translate, an
/// answer its touch doesn't admit (`disagreeing`), or a sequence that
/// doesn't replay on both, stops the set.
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
    let lined = fused_lined(&set_a.events, &lines_a, &set_b.events, &lines_b);
    match disagreeing(&lined) {
        Some((line, issue)) => Err(TranslateError {
            set_number,
            line: Some(line),
            issue,
        }),
        None => Ok(()),
    }?;
    let lines: Vec<usize> = lined.iter().map(|(_, _, (line, _))| *line).collect();
    let touches: Vec<(Side, EventEntry)> = lined
        .into_iter()
        .map(|(side, entry, _)| (side, entry))
        .collect();
    replay(&set_a, &set_b, &touches, max_substitutions).map_err(|e| TranslateError {
        set_number,
        // the line of the touch the side refuses, when there's one
        line: match &e {
            Incoherence::Refused { source, .. } => lines.get(*source).copied(),
            Incoherence::Serving | Incoherence::Points => None,
        },
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
