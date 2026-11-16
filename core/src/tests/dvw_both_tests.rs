//! The touches of a set of DataVolley on both sides, fused from the
//! translations of the two teams (`dvw::both`): each event with its line.

#[cfg(test)]
mod tests {
    use crate::domain::Side;
    use crate::dvw::both::{disagreeing, fused, fused_lined};
    use crate::dvw::translate::Issue;
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum},
        flags::EventFlags,
        snapshot::EventEntry,
    };
    use EvalEnum::*;
    use EventTypeEnum::*;
    use chrono::{DateTime, TimeZone, Utc};

    // the time of the event: its line, in seconds
    fn at(line: usize) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 14, 18, 0, 0)
            .single()
            .unwrap_or_default()
            + chrono::TimeDelta::seconds(i64::try_from(line).unwrap_or_default())
    }

    fn event(event_type: EventTypeEnum, eval: Option<EvalEnum>, line: usize) -> EventEntry {
        EventEntry {
            timestamp: at(line),
            event_type,
            player: None,
            eval,
            target_player: None,
            technique: None,
            flags: EventFlags::default(),
        }
    }

    type Translated = [(EventTypeEnum, Option<EvalEnum>, usize)];

    // the touches fused from the events of A and B with their lines, as
    // (side, event, evaluation)
    fn fuse(a: &Translated, b: &Translated) -> Vec<(Side, EventTypeEnum, Option<EvalEnum>)> {
        let events = |t: &Translated| -> (Vec<EventEntry>, Vec<usize>) {
            t.iter().map(|(e, v, l)| (event(*e, *v, *l), *l)).unzip()
        };
        let (a, lines_a) = events(a);
        let (b, lines_b) = events(b);
        fused(&a, &lines_a, &b, &lines_b)
            .into_iter()
            .map(|(side, e)| (side, e.event_type, e.eval))
            .collect()
    }

    #[test]
    fn what_the_other_side_tells_is_left_out_and_the_touches_go_by_line() {
        assert_eq!(
            fuse(
                &[(S, Some(Positive), 10), (OS, None, 13)],
                &[(P, Some(Negative), 11), (A, Some(Perfect), 12)],
            ),
            vec![
                (Side::A, S, Some(Positive)),
                (Side::B, P, Some(Negative)),
                (Side::B, A, Some(Perfect)),
            ]
        );
    }

    #[test]
    fn at_the_same_line_the_changes_come_first() {
        assert_eq!(
            fuse(
                &[(S, Some(Positive), 10)],
                &[(R, None, 10), (P, Some(Negative), 11)],
            ),
            vec![
                (Side::B, R, None),
                (Side::A, S, Some(Positive)),
                (Side::B, P, Some(Negative)),
            ]
        );
    }

    #[test]
    fn a_freeball_received_alone_is_the_freeball_the_other_side_sends() {
        let a = [
            (S, Some(Positive), 10),
            (OF, None, 10),
            (A, Some(Perfect), 13),
        ];
        let b = [(P, Some(Negative), 11)];
        assert_eq!(
            fuse(&a, &b),
            vec![
                (Side::A, S, Some(Positive)),
                (Side::B, P, Some(Negative)),
                (Side::B, FB, None),
                (Side::A, A, Some(Perfect)),
            ]
        );
    }

    #[test]
    fn a_freeball_received_with_the_freeball_sent_is_left_out() {
        let a = [
            (S, Some(Positive), 10),
            (OF, None, 12),
            (A, Some(Perfect), 13),
        ];
        let b = [(P, Some(Negative), 11), (FB, None, 12)];
        assert_eq!(
            fuse(&a, &b),
            vec![
                (Side::A, S, Some(Positive)),
                (Side::B, P, Some(Negative)),
                (Side::B, FB, None),
                (Side::A, A, Some(Perfect)),
            ]
        );
    }

    #[test]
    fn a_freeball_received_after_a_reception_over_is_left_out() {
        let a = [(S, Some(Over), 10), (OF, None, 10), (A, Some(Over), 12)];
        let b = [(P, Some(Over), 11)];
        assert_eq!(
            fuse(&a, &b),
            vec![
                (Side::A, S, Some(Over)),
                (Side::B, P, Some(Over)),
                (Side::A, A, Some(Over)),
            ]
        );
    }

    // the first answer the touch it answers doesn't admit, as (line,
    // answered, its line, answer)
    fn disagreement(
        a: &Translated,
        b: &Translated,
    ) -> Option<(
        usize,
        (EventTypeEnum, Option<EvalEnum>),
        usize,
        (EventTypeEnum, Option<EvalEnum>),
    )> {
        let events = |t: &Translated| -> (Vec<EventEntry>, Vec<usize>) {
            t.iter().map(|(e, v, l)| (event(*e, *v, *l), *l)).unzip()
        };
        let (a, lines_a) = events(a);
        let (b, lines_b) = events(b);
        match disagreeing(&fused_lined(&a, &lines_a, &b, &lines_b)) {
            Some((
                line,
                Issue::Answer {
                    answered,
                    answered_line,
                    answer,
                },
            )) => Some((line, answered, answered_line, answer)),
            Some((_, issue)) => panic!("{issue:?}"),
            None => None,
        }
    }

    /// Nothing reconciled (TODO/001, §4.2): the evaluations of both teams
    /// are the file's, even where they disagree.
    #[test]
    fn the_evaluations_are_the_file_s() {
        let a = [(S, Some(Over), 10), (A, Some(Perfect), 13)];
        let b = [(P, Some(Negative), 11), (FB, None, 12)];
        assert_eq!(
            fuse(&a, &b),
            vec![
                (Side::A, S, Some(Over)),
                (Side::B, P, Some(Negative)),
                (Side::B, FB, None),
                (Side::A, A, Some(Perfect)),
            ]
        );
        assert_eq!(
            fuse(&[(A, Some(Positive), 10)], &[(B, Some(Perfect), 11)]),
            vec![(Side::A, A, Some(Positive)), (Side::B, B, Some(Perfect))]
        );
    }

    #[test]
    fn an_answer_the_touch_does_not_admit_is_told_with_its_lines() {
        let a = [(S, Some(Over), 10), (A, Some(Perfect), 13)];
        let b = [(P, Some(Negative), 11), (FB, None, 12)];
        assert_eq!(
            disagreement(&a, &b),
            Some((11, (S, Some(Over)), 10, (P, Some(Negative))))
        );
        // a block point after an attack coming back
        assert_eq!(
            disagreement(&[(A, Some(Positive), 20)], &[(B, Some(Perfect), 21)]),
            Some((21, (A, Some(Positive)), 20, (B, Some(Perfect))))
        );
        // an answer to an error
        assert_eq!(
            disagreement(&[(S, Some(Error), 30)], &[(P, Some(Error), 31)]),
            Some((31, (S, Some(Error)), 30, (P, Some(Error))))
        );
    }

    #[test]
    fn answers_the_touch_admits_agree() {
        assert_eq!(
            disagreement(&[(S, Some(Exclamative), 10)], &[(P, Some(Exclamative), 11)]),
            None
        );
        // changes between the touch and its answer don't count
        assert_eq!(
            disagreement(
                &[(S, Some(Positive), 10)],
                &[(R, None, 11), (P, Some(Negative), 11)]
            ),
            None
        );
        // an attack with no answer
        assert_eq!(
            disagreement(&[(A, Some(Perfect), 10), (OS, None, 10)], &[]),
            None
        );
    }

    #[test]
    fn a_dig_at_the_line_of_the_freeball_sent_is_the_freeball_received() {
        let a = [(D, Some(Perfect), 11), (A, Some(Perfect), 12)];
        let b = [(A, Some(Exclamative), 10), (FB, None, 11)];
        assert_eq!(
            fuse(&a, &b),
            vec![
                (Side::B, A, Some(Exclamative)),
                (Side::B, FB, None),
                (Side::A, A, Some(Perfect)),
            ]
        );
    }

    #[test]
    fn a_point_no_touch_ends_is_the_point_of_the_side_without_the_ball() {
        let a = [(S, Some(Negative), 10), (OS, None, 14)];
        let b = [(P, Some(Perfect), 11), (OE, None, 14)];
        assert_eq!(
            fuse(&a, &b),
            vec![
                (Side::A, S, Some(Negative)),
                (Side::B, P, Some(Perfect)),
                (Side::A, OS, None),
            ]
        );
    }

    #[test]
    fn a_point_no_touch_ends_with_the_ball_is_the_error_the_other_side_tells() {
        let a = [(S, Some(Over), 10), (OS, None, 14)];
        let b = [(P, Some(Over), 11), (OE, None, 14)];
        assert_eq!(
            fuse(&a, &b),
            vec![
                (Side::A, S, Some(Over)),
                (Side::B, P, Some(Over)),
                (Side::B, OE, None),
            ]
        );
    }

    #[test]
    fn a_point_a_touch_ends_needs_nothing_told() {
        assert_eq!(
            fuse(
                &[(S, Some(Perfect), 10), (S, Some(Positive), 13)],
                &[(OS, None, 12), (P, Some(Negative), 14)],
            ),
            vec![
                (Side::A, S, Some(Perfect)),
                (Side::A, S, Some(Positive)),
                (Side::B, P, Some(Negative)),
            ]
        );
    }
}
