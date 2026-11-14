//! A set scouted on both sides (`scouting = both`): one sequence of
//! touches, each saved once, by the team playing it and from its point of
//! view; the stream of each side, the one `Snapshot` replays, is derived
//! from it (the projection).
//!
//! The stream of a side is its own touches, plus what the other side's
//! touches tell it, never saved:
//! - the freeball the other side sends (`FB`) is the side's `OF`, and the
//!   other way round, at the same place of the rally;
//! - a rally ended by the other side, without a touch of the side ending
//!   it too, is closed at the end of the point: the side's `OE` when it
//!   won it, `OS` when it lost it, or its fault (`F`, the team's) when the
//!   other side told its error without a touch (`OE`).
//!
//! The other side of the same point (an ace, then the reception error)
//! comes right after the touch ending it, with the same winner.
use crate::{
    domain::Side,
    shapes::{
        enums::{EventTypeEnum, LineupRulesEnum, TeamSideEnum},
        flags::EventFlags,
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
    },
};
use chrono::{DateTime, Utc};

/// An event of the stream of a side, with the touch of the sequence it
/// comes from: the touch itself, or the one telling it.
#[derive(Debug, Clone, PartialEq)]
pub struct Projected {
    pub entry: EventEntry,
    pub source: usize,
}

/// The streams of the two sides of a set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Streams {
    pub a: Vec<Projected>,
    pub b: Vec<Projected>,
}

impl Streams {
    pub fn of(&self, side: Side) -> &[Projected] {
        match side {
            Side::A => &self.a,
            Side::B => &self.b,
        }
    }

    fn push(mut self, side: Side, entry: EventEntry, source: usize) -> Self {
        let projected = Projected { entry, source };
        match side {
            Side::A => self.a.push(projected),
            Side::B => self.b.push(projected),
        }
        self
    }
}

/// A point told by one side only so far.
#[derive(Debug, Clone, Copy)]
struct Point {
    winner: Side,
    // the side telling it, and the event it tells it with
    told_by: Side,
    event_type: EventTypeEnum,
    source: usize,
    timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
enum State {
    Playing,
    Told(Point),
}

/// The team winning the rally the touch of the side ends, if it does.
fn winner(side: Side, entry: &EventEntry) -> Option<Side> {
    entry
        .event_type
        .rally_winner(entry.eval)
        .map(|team| match team {
            TeamSideEnum::Us => side,
            TeamSideEnum::Them => side.other(),
        })
}

/// An event telling the side what the other did: a team event, no player,
/// no evaluation, the time of the touch telling it.
fn told(event_type: EventTypeEnum, timestamp: DateTime<Utc>) -> EventEntry {
    EventEntry {
        timestamp,
        event_type,
        player: None,
        eval: None,
        target_player: None,
        technique: None,
        flags: EventFlags::default(),
    }
}

/// The point to the side not telling it yet.
fn close(streams: Streams, point: &Point) -> Streams {
    let side = point.told_by.other();
    let event_type = match (side == point.winner, point.event_type) {
        (true, _) => EventTypeEnum::OE,
        (false, EventTypeEnum::OE) => EventTypeEnum::F,
        (false, _) => EventTypeEnum::OS,
    };
    streams.push(side, told(event_type, point.timestamp), point.source)
}

/// The touch, in a rally being played.
fn play(streams: Streams, source: usize, side: Side, entry: &EventEntry) -> (Streams, State) {
    let streams = streams.push(side, entry.clone(), source);
    match (winner(side, entry), entry.event_type) {
        (Some(winner), event_type) => (
            streams,
            State::Told(Point {
                winner,
                told_by: side,
                event_type,
                source,
                timestamp: entry.timestamp,
            }),
        ),
        (None, EventTypeEnum::FB) => (
            streams.push(
                side.other(),
                told(EventTypeEnum::OF, entry.timestamp),
                source,
            ),
            State::Playing,
        ),
        (None, EventTypeEnum::OF) => (
            streams.push(
                side.other(),
                told(EventTypeEnum::FB, entry.timestamp),
                source,
            ),
            State::Playing,
        ),
        (None, _) => (streams, State::Playing),
    }
}

/// The streams of the two sides of a set, from its touches in order.
pub fn project(touches: &[(Side, EventEntry)]) -> Streams {
    let (streams, state) = touches.iter().enumerate().fold(
        (Streams::default(), State::Playing),
        |(streams, state), (source, (side, entry))| match state {
            // the other side of the same point: never a serve, starting
            // the next rally
            State::Told(point)
                if *side != point.told_by
                    && entry.event_type != EventTypeEnum::S
                    && winner(*side, entry) == Some(point.winner) =>
            {
                (streams.push(*side, entry.clone(), source), State::Playing)
            }
            State::Told(point) => play(close(streams, &point), source, *side, entry),
            State::Playing => play(streams, source, *side, entry),
        },
    );
    match state {
        State::Told(point) => close(streams, &point),
        State::Playing => streams,
    }
}

/// Why a set scouted on both sides doesn't replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoherence {
    /// the two sides don't start with the serve of the same team
    Serving,
    /// the event of the side, told by the touch at `source` (its own or
    /// the other side's), can't follow in the side's stream
    Refused {
        side: Side,
        source: usize,
        reason: String,
    },
    /// the two sides don't give the same rallies to the same team: a
    /// guard, the projection telling each point to both
    Points,
}

impl std::fmt::Display for Incoherence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Incoherence::Serving => write!(f, "the two sides don't start with the same serve"),
            Incoherence::Refused {
                side,
                source,
                reason,
            } => write!(
                f,
                "touch {source} doesn't follow on side {side:?}: {reason}"
            ),
            Incoherence::Points => write!(f, "the two sides don't agree on the points"),
        }
    }
}

/// The snapshot of the side, its stream replayed from its start.
fn replay_side(
    start: &SetEntry,
    side: Side,
    stream: &[Projected],
    max_substitutions: u8,
) -> Result<Snapshot, Incoherence> {
    let refused = |source: usize, reason: String| Incoherence::Refused {
        side,
        source,
        reason,
    };
    let rules = start.lineup_rules.unwrap_or(LineupRulesEnum::CURRENT);
    let snapshot = Snapshot::with_rules(start, max_substitutions, rules)
        .map_err(|e| refused(stream.first().map_or(0, |p| p.source), e.to_string()))?;
    stream.iter().try_fold(snapshot, |mut snapshot, projected| {
        snapshot
            .add_event(&projected.entry)
            .map(|_| snapshot)
            .map_err(|e| refused(projected.source, e.to_string()))
    })
}

/// The snapshots of A and B, each from its point of view: their starts
/// (lineup, serving team as each sees it; their events aside) and the
/// touches of the set in order. The two streams must replay and give each
/// rally to the same team.
pub fn replay(
    start_a: &SetEntry,
    start_b: &SetEntry,
    touches: &[(Side, EventEntry)],
    max_substitutions: u8,
) -> Result<(Snapshot, Snapshot), Incoherence> {
    let streams = project(touches);
    let same_server = start_a.serving_team != start_b.serving_team;
    let a = same_server
        .then_some(())
        .ok_or(Incoherence::Serving)
        .and_then(|_| replay_side(start_a, Side::A, &streams.a, max_substitutions))?;
    let b = replay_side(start_b, Side::B, &streams.b, max_substitutions)?;
    let points_of = |snapshot: &Snapshot, side: Side| -> Vec<Side> {
        snapshot
            .points
            .iter()
            .map(|team| match team {
                TeamSideEnum::Us => side,
                TeamSideEnum::Them => side.other(),
            })
            .collect()
    };
    match points_of(&a, Side::A) == points_of(&b, Side::B) {
        true => Ok((a, b)),
        false => Err(Incoherence::Points),
    }
}

/// The side of the net the touch of the side leaves the ball on, while
/// the rally goes on (see `EventTypeEnum::ball_side_after`).
fn ball_after(side: Side, entry: &EventEntry) -> Option<Side> {
    entry
        .event_type
        .ball_side_after(entry.eval)
        .map(|team| match team {
            TeamSideEnum::Us => side,
            TeamSideEnum::Them => side.other(),
        })
}

/// Whether the event touches the ball: not a substitution, nor a libero
/// or setter change.
fn touches_ball(entry: &EventEntry) -> bool {
    !matches!(
        entry.event_type,
        EventTypeEnum::R
            | EventTypeEnum::CL
            | EventTypeEnum::LR
            | EventTypeEnum::LF
            | EventTypeEnum::CS
    )
}

/// Whether the touch answers the other side's touch, telling the same
/// transition: the reception of a serve, the dig or the block of an
/// attack.
pub(crate) fn answers(touch: EventTypeEnum, answered: EventTypeEnum) -> bool {
    matches!(
        (answered, touch),
        (EventTypeEnum::S, EventTypeEnum::P)
            | (EventTypeEnum::A, EventTypeEnum::D | EventTypeEnum::B)
    )
}

/// Whether the side can add the touch now, after the touches so far,
/// replayed as `a` and `b` (see [`replay`]): the stream of the side must
/// offer it, and
/// - a touch of the ball must find it on the side's half, where the last
///   touch left it as its side sees it, or else answer the other side's
///   last touch (see `answers`), leaving the ball where that touch did
///   (their serve `/`, our reception `/`), or be the other half of the point the other
///   side's last touch ended, with the same winner (their ace, our
///   reception error);
/// - after a serve in play, only the reception of the other side;
/// - a fault, a substitution, a libero or setter change need nothing more;
/// - the other side's point, error or freeball (`OS`, `OE`, `OF`) never:
///   its touches tell them.
pub fn admits(
    touches: &[(Side, EventEntry)],
    a: &Snapshot,
    b: &Snapshot,
    side: Side,
    entry: &EventEntry,
) -> bool {
    use EventTypeEnum::*;
    let own = match side {
        Side::A => a,
        Side::B => b,
    };
    // the last touch of the ball, and where it left it: as its side sees
    // it while the rally goes on, with the serving team before the serve
    let last = touches.iter().rev().find(|(_, e)| touches_ball(e));
    let ball = last
        .and_then(|(s, e)| ball_after(*s, e))
        .unwrap_or(match a.get_ball_side() {
            TeamSideEnum::Us => Side::A,
            TeamSideEnum::Them => Side::B,
        });
    let last = last.filter(|(s, _)| *s != side);
    let after = ball_after(side, entry);
    // a serve still in play: the reception of the other side comes next
    let serve_in_play = matches!(
        touches.iter().rev().find(|(_, e)| touches_ball(e)),
        Some((_, e)) if e.event_type == S && ball_after(side, e).is_some()
    );
    // on the side's half, or answering in play where the last touch left
    // the ball
    let in_play = ball == side || after == Some(ball);
    let touch_admitted = |event_type: EventTypeEnum| match (serve_in_play, last) {
        (true, Some(_)) => event_type == P && in_play,
        (true, None) => false,
        (false, Some((s, e))) if winner(*s, e).is_some() => {
            ball == side || winner(side, entry) == winner(*s, e)
        }
        (false, Some((_, e))) => ball == side || answers(event_type, e.event_type) && in_play,
        (false, None) => ball == side,
    };
    own.get_available_options().contains(&entry.event_type)
        && match entry.event_type {
            OS | OE | OF => false,
            F | R | CL | LR | LF | CS => true,
            event_type => touch_admitted(event_type),
        }
}
