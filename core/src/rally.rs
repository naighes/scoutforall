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
//!   other side told its error without a touch (`OE`); a point without a
//!   cause (`PW`, `PL`) is the other one's, the other way round.
//!
//! The other side of the same point (an ace, then the reception error)
//! comes right after the touch ending it, with the same winner.
use crate::{
    domain::Side,
    shapes::{
        enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, TeamSideEnum},
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
        (_, EventTypeEnum::PW) => EventTypeEnum::PL,
        (_, EventTypeEnum::PL) => EventTypeEnum::PW,
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
/// attack, the cover after a block.
pub(crate) fn answers(touch: EventTypeEnum, answered: EventTypeEnum) -> bool {
    matches!(
        (answered, touch),
        (EventTypeEnum::S, EventTypeEnum::P)
            | (EventTypeEnum::A, EventTypeEnum::D | EventTypeEnum::B)
            | (EventTypeEnum::B, EventTypeEnum::D)
    )
}

/// The evaluations of the answer to the other side's touch (see
/// `answers`): the two touches tell the same transition, so the answered
/// one decides the answer (TODO/001, §4.2). One value is no choice for the
/// scout; none, no answer at all (a serve or an attack error).
///
/// | serve | reception |
/// |---|---|
/// | `#` | `=` |
/// | `/` | `/` |
/// | `+` | `-` |
/// | `!` | `!` |
/// | `-` | `#` or `+` |
///
/// | attack | dig | block |
/// |---|---|---|
/// | `#` | `=` | `=` (never `/`) |
/// | `/` | | `#` |
/// | `-` | `# +` | `+` |
/// | `+` | `- /` | `-` |
/// | `!` | | `!` |
///
/// The block `/` (an invasion) answers nothing: a fault.
///
/// | block | cover of the attackers |
/// |---|---|
/// | `!` | `! -` |
/// | `+`, `-` | (the ball stays with the blockers) |
pub fn answer_evals(
    answered: EventTypeEnum,
    eval: Option<EvalEnum>,
    answer: EventTypeEnum,
) -> &'static [EvalEnum] {
    use EvalEnum::*;
    use EventTypeEnum::*;
    match (answered, eval, answer) {
        (S, Some(Perfect), P) | (A, Some(Perfect), D | B) => &[Error],
        (S, Some(Over), P) => &[Over],
        (S, Some(Positive), P) => &[Negative],
        (S, Some(Exclamative), P) => &[Exclamative],
        (S, Some(Negative), P) => &[Perfect, Positive],
        (A, Some(Over), B) => &[Perfect],
        (A, Some(Negative), D) => &[Perfect, Positive],
        (A, Some(Negative), B) => &[Positive],
        (A, Some(Positive), D) => &[Negative, Over],
        (A, Some(Positive), B) => &[Negative],
        (A, Some(Exclamative), B) => &[Exclamative],
        (B, Some(Exclamative), D) => &[Exclamative, Negative],
        _ => &[],
    }
}

/// Whether the side can add the touch now, after the touches so far,
/// replayed as `a` and `b` (see [`replay`]): the stream of the side must
/// offer it, with its evaluation, and
/// - a touch of the ball must find it on the side's half, where the last
///   touch left it as its side sees it, or else answer the other side's
///   last touch (see `answers`) with an evaluation `answer_evals` admits:
///   in play, leaving the ball where that touch did (their serve `/`, our
///   reception `/`), or the other half of the point that touch ended
///   (their ace, our reception error), which the stream of the side has
///   closed already;
/// - after its own attack `+` (the roads open) the side plays again, the
///   other side's dig `/` untold: an attack or a freeball, never a dig, a
///   block or a reception without the other side's attack;
/// - after a serve in play, only the reception of the other side;
/// - a fault, a point without a cause, a substitution, a libero or setter
///   change need nothing more;
///   the block `/` (an invasion) is a fault, but never right after the
///   other side's attack: that attack isn't saved;
/// - any other block answers the other side's attack, never a ball coming
///   over without one;
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
    // the side's own attack `+`, the roads open: back to it
    let open_roads = touches
        .iter()
        .rev()
        .find(|(_, e)| touches_ball(e))
        .is_some_and(|(s, e)| {
            *s == side && e.event_type == A && e.eval == Some(EvalEnum::Positive)
        });
    let touch_admitted = |event_type: EventTypeEnum| match (serve_in_play, last) {
        (true, Some(_)) => event_type == P && in_play,
        (true, None) => false,
        (false, Some((s, e))) if winner(*s, e).is_some() => ball == side,
        (false, Some((_, e))) => ball == side || answers(event_type, e.event_type) && in_play,
        (false, None) => ball == side || open_roads && matches!(event_type, A | FB),
    };
    let offered = own.get_available_options().contains(&entry.event_type)
        && entry
            .eval
            .is_none_or(|eval| own.available_evals(entry.event_type).contains(&eval));
    let answered = last.filter(|(_, e)| answers(entry.event_type, e.event_type));
    let fits = |answered: &EventEntry| {
        entry.eval.is_some_and(|eval| {
            answer_evals(answered.event_type, answered.eval, entry.event_type).contains(&eval)
        })
    };
    match (entry.event_type, answered) {
        (OS | OE | OF, _) => false,
        (B, _) if entry.eval == Some(EvalEnum::Over) => {
            offered && !last.is_some_and(|(_, e)| e.event_type == A)
        }
        // a block answers the other side's attack, nothing else
        (B, None) => false,
        (F | PW | PL | R | CL | LR | LF | CS, _) => offered,
        // the other half of the point: closed in the stream of the side
        (_, Some((s, e))) if winner(*s, e).is_some() => fits(e),
        (event_type, Some((_, e))) => offered && fits(e) && touch_admitted(event_type),
        (event_type, None) => offered && touch_admitted(event_type),
    }
}
