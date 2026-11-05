//! The statistics over several matches (see `analysis`).
#[cfg(test)]
mod tests {
    use crate::analysis::{Analysis, MOVING_WINDOW, Period};
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, LineupRulesEnum, PhaseEnum, TeamSideEnum},
        r#match::MatchEntry,
        set::SetEntry,
        snapshot::EventEntry,
        stats::StatsFilter,
        team::TeamEntry,
    };
    use crate::trend::Series;
    use EvalEnum::*;
    use EventTypeEnum::*;
    use chrono::{NaiveDate, TimeZone, Utc};
    use uuid::Uuid;

    struct Team {
        positions: [Uuid; 6],
        libero: Uuid,
    }

    impl Team {
        fn new() -> Self {
            Team {
                positions: std::array::from_fn(|_| Uuid::new_v4()),
                libero: Uuid::new_v4(),
            }
        }

        /// A set receiving first, with these receptions of the libero, each
        /// followed by a kill of the opposite, our serve and their point.
        fn set(&self, receptions: &[EvalEnum]) -> SetEntry {
            let [setter, _, _, opposite, ..] = self.positions;
            let touch = |event_type, player, eval| EventEntry {
                timestamp: Utc::now(),
                event_type,
                player: Some(player),
                eval: Some(eval),
                target_player: None,
                technique: None,
                flags: Default::default(),
            };
            let theirs = EventEntry {
                timestamp: Utc::now(),
                event_type: OS,
                player: None,
                eval: None,
                target_player: None,
                technique: None,
                flags: Default::default(),
            };
            let mut events = vec![];
            for (i, eval) in receptions.iter().enumerate() {
                // the server: the player in position 1 after rotating
                let server = self.positions[(i + 1) % 6];
                events.push(touch(P, self.libero, *eval));
                events.push(touch(A, opposite, Perfect));
                events.push(touch(S, server, Positive));
                events.push(theirs.clone());
            }
            SetEntry {
                set_number: 1,
                serving_team: TeamSideEnum::Them,
                initial_positions: self.positions,
                libero: self.libero,
                fallback_libero: None,
                setter,
                lineup_rules: Some(LineupRulesEnum::CURRENT),
                events,
            }
        }

        fn played(&self, day: (i32, u32, u32), receptions: &[EvalEnum]) -> MatchEntry {
            MatchEntry {
                sets: vec![self.set(receptions)],
                ..unplayed(day)
            }
        }
    }

    fn unplayed(day: (i32, u32, u32)) -> MatchEntry {
        MatchEntry {
            source: Default::default(),
            opponent: format!("{day:?}"),
            date: Utc
                .with_ymd_and_hms(day.0, day.1, day.2, 18, 0, 0)
                .single()
                .unwrap_or_default()
                .fixed_offset(),
            id: format!("{day:?}"),
            team: TeamEntry::default(),
            home: true,
            roster: vec![],
            max_substitutions: DEFAULT_MAX_SUBSTITUTIONS,
            sets: vec![],
            notes: String::new(),
        }
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap_or_default()
    }

    fn ids(matches: &[&MatchEntry]) -> Vec<String> {
        matches.iter().map(|m| m.id.clone()).collect()
    }

    #[test]
    fn the_last_month_from_the_same_day_today_included() {
        let t = Team::new();
        let matches = vec![
            t.played((2026, 10, 4), &[Perfect]),
            t.played((2026, 11, 5), &[Perfect]),
            t.played((2026, 10, 5), &[Perfect]),
            // not played yet
            unplayed((2026, 11, 1)),
            // to come
            t.played((2026, 11, 6), &[Perfect]),
        ];
        let today = date(2026, 11, 5);
        let chosen = Period::LastMonth.select(&matches, today);
        assert_eq!(ids(&chosen), ["(2026, 10, 5)", "(2026, 11, 5)"]);
        assert_eq!(
            Period::LastMonth.days(&chosen, today),
            Some((date(2026, 10, 5), today))
        );
        // a shorter month before: its last day
        assert_eq!(
            Period::LastMonth.days(&[], date(2026, 3, 31)),
            Some((date(2026, 2, 28), date(2026, 3, 31)))
        );
    }

    #[test]
    fn the_last_matches_played() {
        let t = Team::new();
        let matches = vec![
            t.played((2026, 9, 1), &[Perfect]),
            t.played((2026, 10, 1), &[Perfect]),
            unplayed((2026, 10, 15)),
            t.played((2026, 8, 1), &[Perfect]),
            t.played((2026, 11, 1), &[Perfect]),
        ];
        let today = date(2026, 11, 5);
        let chosen = Period::LastMatches(2).select(&matches, today);
        assert_eq!(ids(&chosen), ["(2026, 10, 1)", "(2026, 11, 1)"]);
        assert_eq!(
            Period::LastMatches(2).days(&chosen, today),
            Some((date(2026, 10, 1), date(2026, 11, 1)))
        );
        assert_eq!(Period::LastMatches(10).select(&matches, today).len(), 4);
        assert_eq!(Period::LastMatches(2).days(&[], today), None);
    }

    #[test]
    fn between_two_days_both_included() {
        let t = Team::new();
        let matches = vec![
            t.played((2026, 9, 30), &[Perfect]),
            t.played((2026, 10, 1), &[Perfect]),
            t.played((2026, 10, 31), &[Perfect]),
            t.played((2026, 11, 1), &[Perfect]),
        ];
        let period = Period::Between {
            from: date(2026, 10, 1),
            to: date(2026, 10, 31),
        };
        let chosen = period.select(&matches, date(2026, 11, 5));
        assert_eq!(ids(&chosen), ["(2026, 10, 1)", "(2026, 10, 31)"]);
    }

    /// The receptions match by match (100, 0, 100, 0), and together with
    /// the two matches before counting every touch: 100, 50 (2 of 4), 60
    /// (3 of 5), 25 (1 of 4, the first match out).
    #[test]
    fn match_by_match_and_the_moving_average() {
        let t = Team::new();
        let matches = vec![
            t.played((2026, 10, 1), &[Perfect, Positive]),
            t.played((2026, 10, 8), &[Negative, Exclamative]),
            t.played((2026, 10, 15), &[Perfect]),
            t.played((2026, 10, 22), &[Exclamative]),
        ];
        let chosen = Period::LastMatches(4).select(&matches, date(2026, 11, 5));
        let analysis = Analysis::of(&chosen);
        let all = StatsFilter::default();
        assert_eq!(
            analysis.series(Series::Reception, &all),
            [Some(100.0), Some(0.0), Some(100.0), Some(0.0)]
        );
        assert_eq!(analysis.touches(Series::Reception, &all), [2, 2, 1, 1]);
        assert_eq!(
            analysis.moving(Series::Reception, &all, MOVING_WINDOW),
            [Some(100.0), Some(50.0), Some(60.0), Some(25.0)]
        );
        assert_eq!(
            Series::Reception.value_in(&analysis.total(), &all),
            Some(50.0)
        );
        // every rally in side out won, every one in break lost
        assert_eq!(analysis.series(Series::SideOut, &all), [Some(100.0); 4]);
        assert_eq!(analysis.series(Series::BreakPoint, &all), [Some(0.0); 4]);
        // side out asked in break: nothing
        let break_only = StatsFilter::new(Some(PhaseEnum::Break), None, None);
        assert_eq!(analysis.series(Series::SideOut, &break_only), [None; 4]);
    }

    /// A match that doesn't replay is left out, not the analysis.
    #[test]
    fn a_match_not_replaying_is_left_out() {
        let t = Team::new();
        let mut broken = t.played((2026, 10, 8), &[Perfect]);
        // a reception while serving: not a valid set
        if let Some(set) = broken.sets.first_mut() {
            set.serving_team = TeamSideEnum::Us;
        }
        let matches = vec![t.played((2026, 10, 1), &[Perfect]), broken];
        let chosen = Period::LastMatches(5).select(&matches, date(2026, 11, 5));
        let analysis = Analysis::of(&chosen);
        assert_eq!(analysis.matches.len(), 1);
        assert_eq!(ids(&analysis.skipped), ["(2026, 10, 8)"]);
    }
}
