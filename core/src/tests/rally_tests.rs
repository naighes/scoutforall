//! The projection of a set scouted on both sides: the stream of each side
//! from the one sequence of touches.
#[cfg(test)]
mod tests {
    use crate::domain::Side;
    use crate::rally::{Streams, project};
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum},
        flags::EventFlags,
        snapshot::EventEntry,
    };
    use EvalEnum::*;
    use EventTypeEnum::*;
    use chrono::{DateTime, TimeZone, Utc};
    use uuid::Uuid;

    // the n-th second of the set: the told events keep the time of the
    // touch telling them, exactly
    fn at(n: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 14, 18, 0, n)
            .single()
            .unwrap_or_default()
    }

    fn touch(
        side: Side,
        event_type: EventTypeEnum,
        eval: Option<EvalEnum>,
        n: u32,
    ) -> (Side, EventEntry) {
        let player = event_type.asks_player().then(Uuid::new_v4);
        (
            side,
            EventEntry {
                timestamp: at(n),
                event_type,
                player: player.filter(|_| event_type != F),
                eval,
                target_player: None,
                technique: None,
                flags: EventFlags::default(),
            },
        )
    }

    // the stream of the side as (event, evaluation, touch it comes from)
    fn stream(streams: &Streams, side: Side) -> Vec<(EventTypeEnum, Option<EvalEnum>, usize)> {
        streams
            .of(side)
            .iter()
            .map(|p| (p.entry.event_type, p.entry.eval, p.source))
            .collect()
    }

    #[test]
    fn each_side_keeps_its_touches_and_learns_the_point_it_lost() {
        let streams = project(&[
            touch(Side::A, S, Some(Positive), 0),
            touch(Side::B, P, Some(Negative), 1),
            touch(Side::B, A, Some(Perfect), 2),
        ]);
        assert_eq!(
            stream(&streams, Side::A),
            vec![(S, Some(Positive), 0), (OS, None, 2)]
        );
        assert_eq!(
            stream(&streams, Side::B),
            vec![(P, Some(Negative), 1), (A, Some(Perfect), 2)]
        );
    }

    #[test]
    fn the_other_side_of_the_same_point_needs_nothing_told() {
        // the ace, then the reception error
        let streams = project(&[
            touch(Side::A, S, Some(Perfect), 0),
            touch(Side::B, P, Some(Error), 1),
        ]);
        assert_eq!(stream(&streams, Side::A), vec![(S, Some(Perfect), 0)]);
        assert_eq!(stream(&streams, Side::B), vec![(P, Some(Error), 1)]);
    }

    #[test]
    fn an_ace_without_the_reception_is_their_point() {
        let streams = project(&[touch(Side::A, S, Some(Perfect), 0)]);
        assert_eq!(stream(&streams, Side::B), vec![(OS, None, 0)]);
    }

    #[test]
    fn an_error_is_the_other_side_s_opponent_error() {
        let streams = project(&[touch(Side::A, S, Some(Error), 0)]);
        assert_eq!(stream(&streams, Side::B), vec![(OE, None, 0)]);
    }

    #[test]
    fn a_fault_is_the_other_side_s_opponent_error() {
        let streams = project(&[touch(Side::B, F, None, 0)]);
        assert_eq!(stream(&streams, Side::A), vec![(OE, None, 0)]);
    }

    #[test]
    fn an_opponent_error_without_a_touch_is_the_other_side_s_fault() {
        let streams = project(&[touch(Side::A, OE, None, 0)]);
        assert_eq!(stream(&streams, Side::B), vec![(F, None, 0)]);
        assert_eq!(streams.b.first().and_then(|p| p.entry.player), None);
    }

    #[test]
    fn an_opponent_point_without_a_touch_is_the_other_side_s_opponent_error() {
        let streams = project(&[touch(Side::A, OS, None, 0)]);
        assert_eq!(stream(&streams, Side::B), vec![(OE, None, 0)]);
    }

    #[test]
    fn a_freeball_sent_is_the_freeball_the_other_side_receives() {
        let streams = project(&[
            touch(Side::A, S, Some(Positive), 0),
            touch(Side::B, P, Some(Negative), 1),
            touch(Side::B, FB, None, 2),
            touch(Side::A, A, Some(Perfect), 3),
        ]);
        assert_eq!(
            stream(&streams, Side::A),
            vec![(S, Some(Positive), 0), (OF, None, 2), (A, Some(Perfect), 3)]
        );
        assert_eq!(
            stream(&streams, Side::B),
            vec![(P, Some(Negative), 1), (FB, None, 2), (OS, None, 3)]
        );
        assert_eq!(streams.a.get(1).map(|p| p.entry.timestamp), Some(at(2)));
    }

    #[test]
    fn a_freeball_told_without_a_touch_is_the_other_side_s_freeball() {
        let streams = project(&[touch(Side::A, OF, None, 0)]);
        assert_eq!(stream(&streams, Side::B), vec![(FB, None, 0)]);
    }

    #[test]
    fn the_point_is_told_before_what_comes_after_it() {
        let streams = project(&[
            touch(Side::A, S, Some(Perfect), 0),
            touch(Side::B, R, None, 1),
            touch(Side::A, S, Some(Error), 2),
        ]);
        assert_eq!(
            stream(&streams, Side::B),
            vec![(OS, None, 0), (R, None, 1), (OE, None, 2)]
        );
        assert_eq!(streams.b.first().map(|p| p.entry.timestamp), Some(at(0)));
    }

    #[test]
    fn a_touch_with_another_winner_starts_the_next_rally() {
        // the ace told, then a point of theirs: the next rally
        let streams = project(&[
            touch(Side::A, S, Some(Perfect), 0),
            touch(Side::B, OE, None, 1),
        ]);
        assert_eq!(
            stream(&streams, Side::A),
            vec![(S, Some(Perfect), 0), (F, None, 1)]
        );
        assert_eq!(
            stream(&streams, Side::B),
            vec![(OS, None, 0), (OE, None, 1)]
        );
    }

    #[test]
    fn a_rally_being_played_tells_nothing_yet() {
        let streams = project(&[
            touch(Side::A, S, Some(Positive), 0),
            touch(Side::B, P, Some(Positive), 1),
        ]);
        assert_eq!(stream(&streams, Side::A), vec![(S, Some(Positive), 0)]);
        assert_eq!(stream(&streams, Side::B), vec![(P, Some(Positive), 1)]);
    }
}
