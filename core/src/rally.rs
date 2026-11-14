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
        enums::{EventTypeEnum, TeamSideEnum},
        flags::EventFlags,
        snapshot::EventEntry,
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
            // the other side of the same point
            State::Told(point)
                if *side != point.told_by && winner(*side, entry) == Some(point.winner) =>
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
