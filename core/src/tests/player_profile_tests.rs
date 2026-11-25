#[cfg(test)]
mod tests {
    use crate::{
        names::initials,
        player_profile::{PlayerSeasonStats, age, has_training},
        shapes::{
            enums::{EvalEnum, EventTypeEnum, TeamSideEnum},
            r#match::MatchEntry,
            player::PlayerEntry,
            set::SetEntry,
            snapshot::EventEntry,
            team::TeamEntry,
        },
        training::totals::Totals,
    };
    use chrono::{DateTime, NaiveDate, Utc};
    use uuid::Uuid;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn event(
        event_type: EventTypeEnum,
        player: Uuid,
        eval: Option<EvalEnum>,
        target_player: Option<Uuid>,
    ) -> EventEntry {
        EventEntry {
            timestamp: DateTime::<Utc>::UNIX_EPOCH,
            event_type,
            player: Some(player),
            eval,
            target_player,
            technique: None,
            flags: Default::default(),
        }
    }

    /// Players 1 to 8: 1 to 6 start (1 sets), 7 is the libero, 8 the bench.
    fn a_match(sets: Vec<Vec<EventEntry>>) -> MatchEntry {
        let players: Vec<PlayerEntry> = (1..=8)
            .map(|n| PlayerEntry {
                id: id(n),
                name: format!("Player {n}"),
                number: n as u8,
                ..PlayerEntry::default()
            })
            .collect();
        let sets = sets
            .into_iter()
            .zip(1..)
            .map(|(events, number)| {
                let positions = std::array::from_fn(|i| id(i as u128 + 1));
                let mut set =
                    SetEntry::new(number, TeamSideEnum::Us, positions, id(7), None, id(1)).unwrap();
                set.events = events;
                set
            })
            .collect();
        MatchEntry {
            opponent: "Other Club".into(),
            date: "2026-11-25T18:00:00+01:00".parse().unwrap(),
            id: "match".into(),
            team: TeamEntry {
                id: id(20),
                players,
                ..TeamEntry::default()
            },
            home: true,
            max_substitutions: 6,
            notes: String::new(),
            roster: (1..=8).map(id).collect(),
            sets,
            source: Default::default(),
            video_clock: false,
        }
    }

    fn ace(player: u128) -> EventEntry {
        event(EventTypeEnum::S, id(player), Some(EvalEnum::Perfect), None)
    }

    #[test]
    fn points_are_the_aces_kills_and_blocks_of_the_player() {
        let m = a_match(vec![vec![
            ace(1),
            event(EventTypeEnum::S, id(1), Some(EvalEnum::Error), None),
            event(EventTypeEnum::P, id(2), Some(EvalEnum::Perfect), None),
            event(EventTypeEnum::A, id(4), Some(EvalEnum::Perfect), None),
            ace(2),
            event(EventTypeEnum::S, id(2), Some(EvalEnum::Positive), None),
            event(EventTypeEnum::B, id(4), Some(EvalEnum::Perfect), None),
        ]]);
        let of = |player| {
            let s = PlayerSeasonStats::of(id(player), std::slice::from_ref(&m)).unwrap();
            (s.attack_points, s.block_points, s.serve_points, s.points())
        };
        // the hitter then blocking, and the one serving an ace after them
        assert_eq!([of(4), of(2)], [(1, 1, 0, 2), (0, 0, 1, 1)]);
    }

    #[test]
    fn a_set_is_played_by_who_enters_it() {
        let substitution = event(EventTypeEnum::R, id(2), None, Some(id(8)));
        let m = a_match(vec![vec![ace(1)], vec![substitution]]);
        let of = |player| {
            let season = PlayerSeasonStats::of(id(player), std::slice::from_ref(&m)).unwrap();
            (season.matches_played, season.sets_played)
        };
        // a starter, the one coming in the second set, the libero, a
        // starter then replaced
        assert_eq!(
            [of(1), of(8), of(7), of(2)],
            [(1, 2), (1, 1), (1, 2), (1, 2)]
        );
    }

    #[test]
    fn the_roster_alone_is_not_participation_and_a_match_without_sets_is_not_scouted() {
        let matches = [a_match(vec![vec![ace(1)]]), a_match(vec![])];
        let season = PlayerSeasonStats::of(id(8), &matches).unwrap();
        assert_eq!(
            season,
            PlayerSeasonStats {
                matches_scouted: 1,
                ..PlayerSeasonStats::default()
            }
        );
    }

    #[test]
    fn a_set_that_doesnt_replay_is_an_error() {
        let stranger = event(EventTypeEnum::R, id(99), None, Some(id(8)));
        assert!(PlayerSeasonStats::of(id(1), &[a_match(vec![vec![stranger]])]).is_err());
    }

    #[test]
    fn cancelled_sessions_alone_are_no_training() {
        let of = |planned, completed, cancelled| {
            has_training(&Totals {
                planned,
                completed,
                cancelled,
                ..Totals::default()
            })
        };
        assert_eq!(
            [of(0, 0, 0), of(0, 0, 3), of(1, 0, 0), of(0, 1, 0)],
            [false, false, true, true]
        );
    }

    #[test]
    fn the_age_turns_on_the_birthday() {
        let day = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        let birth = day(2004, 2, 29);
        assert_eq!(
            [day(2026, 2, 28), day(2026, 3, 1), day(2000, 1, 1)].map(|today| age(birth, today)),
            [Some(21), Some(22), None]
        );
    }

    #[test]
    fn initials_are_of_the_first_two_words() {
        assert_eq!(
            ["anna maria rossi", "  Édo ", ""].map(initials),
            ["AM".to_string(), "É".to_string(), String::new()]
        );
    }
}
