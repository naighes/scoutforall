//! The match report: what it shows (`build_report`) and that the template
//! lays it out (`render_report_pdf`), whatever the names and the length of
//! the match.
#[cfg(test)]
mod tests {
    use crate::constants::DEFAULT_MAX_SUBSTITUTIONS;
    use crate::localization::labels;
    use crate::reporting::model::{Cell, Point, Report, build_report};
    use crate::reporting::pdf::{render_match_pdf, render_report_pdf};
    use crate::shapes::{
        enums::{EvalEnum, EventTypeEnum, LanguageEnum, LineupRulesEnum, PhaseEnum, TeamSideEnum},
        r#match::MatchEntry,
        player::PlayerEntry,
        set::SetEntry,
        snapshot::EventEntry,
        stats::{AttackSummary, Stats, StatsFilter},
        team::TeamEntry,
    };
    use crate::tests::EN;
    use EvalEnum::*;
    use EventTypeEnum::*;
    use chrono::Utc;
    use uuid::Uuid;

    struct Team {
        setter: Uuid,
        oh1: Uuid,
        mb2: Uuid,
        opposite: Uuid,
        oh2: Uuid,
        mb1: Uuid,
        libero: Uuid,
        bench: Uuid,
    }

    impl Team {
        fn new() -> Self {
            Team {
                setter: Uuid::new_v4(),
                oh1: Uuid::new_v4(),
                mb2: Uuid::new_v4(),
                opposite: Uuid::new_v4(),
                oh2: Uuid::new_v4(),
                mb1: Uuid::new_v4(),
                libero: Uuid::new_v4(),
                bench: Uuid::new_v4(),
            }
        }

        /// Numbered 1 to 8 in this order.
        fn players(&self) -> Vec<PlayerEntry> {
            [
                self.setter,
                self.oh1,
                self.mb2,
                self.opposite,
                self.oh2,
                self.mb1,
                self.libero,
                self.bench,
            ]
            .iter()
            .enumerate()
            .map(|(i, id)| PlayerEntry {
                id: *id,
                name: format!("player {}", i + 1),
                number: i as u8 + 1,
                ..PlayerEntry::default()
            })
            .collect()
        }

        /// Rotation 0 (setter in position 1): outside hitter 1 in 2, middle
        /// blocker 2 in 3, opposite in 4, outside hitter 2 in 5, the libero
        /// in 6 for middle blocker 1.
        fn set(
            &self,
            set_number: u8,
            serving_team: TeamSideEnum,
            events: Vec<EventEntry>,
        ) -> SetEntry {
            SetEntry {
                set_number,
                serving_team,
                initial_positions: [
                    self.setter,
                    self.oh1,
                    self.mb2,
                    self.opposite,
                    self.oh2,
                    self.mb1,
                ],
                libero: self.libero,
                fallback_libero: None,
                setter: self.setter,
                lineup_rules: Some(LineupRulesEnum::CURRENT),
                events,
            }
        }

        fn team(&self, name: &str) -> TeamEntry {
            TeamEntry {
                name: name.to_string(),
                players: self.players(),
                ..TeamEntry::default()
            }
        }

        /// Them serving: our kill on the reception, the opposite replaced
        /// by the bench player, our serve error, our attack blocked on the
        /// reception, their point, their error. 2-3 for them.
        fn rallies(&self) -> SetEntry {
            self.set(
                1,
                TeamSideEnum::Them,
                vec![
                    act(P, self.libero, Perfect),
                    act(A, self.oh1, Perfect),
                    substitution(self.opposite, self.bench),
                    act(S, self.oh1, Error),
                    act(P, self.libero, Positive),
                    act(A, self.bench, Over),
                    opp(OS),
                    opp(OE),
                ],
            )
        }
    }

    fn ev(event_type: EventTypeEnum, player: Option<Uuid>, eval: Option<EvalEnum>) -> EventEntry {
        EventEntry {
            timestamp: Utc::now(),
            event_type,
            player,
            eval,
            target_player: None,
            technique: None,
        }
    }

    /// An action of ours, with its evaluation.
    fn act(event_type: EventTypeEnum, player: Uuid, eval: EvalEnum) -> EventEntry {
        ev(event_type, Some(player), Some(eval))
    }

    /// An event of theirs: their point or error.
    fn opp(event_type: EventTypeEnum) -> EventEntry {
        ev(event_type, None, None)
    }

    fn substitution(replaced: Uuid, replacement: Uuid) -> EventEntry {
        EventEntry {
            target_player: Some(replacement),
            ..ev(R, Some(replaced), None)
        }
    }

    /// A set won by `winners` in order, with no actions but our faults:
    /// their error when we score, our fault (with us serving) or their
    /// point (with them serving) when they do.
    fn rallies_won_by(
        t: &Team,
        serving_team: TeamSideEnum,
        winners: &[TeamSideEnum],
    ) -> Vec<EventEntry> {
        winners
            .iter()
            .scan(serving_team, |serving, winner| {
                let event = match (*serving, winner) {
                    (_, TeamSideEnum::Us) => opp(OE),
                    (TeamSideEnum::Us, TeamSideEnum::Them) => ev(F, Some(t.setter), None),
                    (TeamSideEnum::Them, TeamSideEnum::Them) => opp(OS),
                };
                *serving = *winner;
                Some(event)
            })
            .collect()
    }

    /// The points of a set ending `us`-`them`: one each in turn, then the
    /// rest of the winner's.
    fn points(us: u8, them: u8) -> Vec<TeamSideEnum> {
        let alternating = us.min(them);
        (0..alternating)
            .flat_map(|_| [TeamSideEnum::Us, TeamSideEnum::Them])
            .chain((alternating..us).map(|_| TeamSideEnum::Us))
            .chain((alternating..them).map(|_| TeamSideEnum::Them))
            .collect()
    }

    fn match_entry(team: TeamEntry, opponent: &str, home: bool, sets: Vec<SetEntry>) -> MatchEntry {
        MatchEntry {
            roster: vec![],
            opponent: opponent.to_string(),
            date: Utc::now().fixed_offset(),
            id: "match".to_string(),
            team,
            home,
            max_substitutions: DEFAULT_MAX_SUBSTITUTIONS,
            sets,
            notes: String::new(),
        }
    }

    /// The set of `Team::rallies`, playing away: they come first.
    fn away_report() -> Report {
        let t = Team::new();
        let m = match_entry(t.team("us"), "them", false, vec![t.rallies()]);
        build_report(&m, EN).expect("expected the report")
    }

    fn row_of<'a>(rows: &'a [Vec<String>], label: &str) -> &'a [String] {
        rows.iter()
            .find(|r| r[0] == label)
            .unwrap_or_else(|| panic!("expected the row {label}"))
    }

    #[test]
    fn the_home_team_comes_first() {
        let report = away_report();
        let [first, second] = &report.header.teams;
        assert_eq!((first.name.as_str(), first.us), ("them", false));
        assert_eq!((second.name.as_str(), second.us), ("us", true));
        assert_eq!(report.point_sequence.teams, ["them", "us"]);
        // the score, the longest runs and the largest leads: theirs first
        assert_eq!(
            report.sets.rows,
            [["1", "3-2", "", "4<8", "3 · 1", "2 · 1"]]
        );
    }

    #[test]
    fn the_point_sequence_in_the_row_of_the_team_scoring() {
        let report = away_report();
        let point = |team, us, score| Point { team, us, score };
        assert_eq!(
            report.point_sequence.sets[0].points,
            [
                point(1, true, 1),
                point(0, false, 1),
                point(0, false, 2),
                point(0, false, 3),
                point(1, true, 2),
            ]
        );
    }

    /// Side out: two won (our kill, their error) out of four; break: our
    /// serve error, none won.
    #[test]
    fn the_key_figures() {
        let labels = EN;
        let report = away_report();
        let kpi = |label: &str| {
            report
                .kpis
                .iter()
                .find(|k| k.label == label)
                .map(|k| (k.value.as_str(), k.detail.as_str()))
                .unwrap_or_else(|| panic!("expected the figure {label}"))
        };
        assert_eq!(kpi(labels.sideout), ("50%", "2/4"));
        assert_eq!(kpi("break"), ("0%", "0/1"));
        // a kill and an attack blocked out of two attacks
        assert_eq!(kpi(labels.attack).0, "50%");
        assert_eq!(kpi(labels.serve).0, "0 / 1");
        assert_eq!(kpi(labels.errors).0, "2");
    }

    /// The points won add up to our score, the ones lost to theirs.
    #[test]
    fn where_the_points_come_from() {
        let labels = EN;
        let report = away_report();
        let won = &report.points_won.rows;
        assert_eq!(row_of(won, labels.attack)[1..], ["1", "50"]);
        assert_eq!(row_of(won, labels.opponent_errors)[1], "1");
        assert_eq!(row_of(won, labels.ace)[1..], ["-", "-"]);
        assert_eq!(row_of(won, labels.total)[1..], ["2", "100"]);
        let lost = &report.points_lost.rows;
        assert_eq!(row_of(lost, labels.opponent_score)[1], "1");
        assert_eq!(row_of(lost, labels.serve)[1], "1");
        assert_eq!(row_of(lost, labels.attack_blocked)[1], "1");
        assert_eq!(row_of(lost, labels.total)[1], "3");
        assert!(report.points_lost.total);
    }

    #[test]
    fn the_attacks_on_the_reception_and_in_counter_attack() {
        let labels = EN;
        let report = away_report();
        let rows = &report.attack_by_phase.rows;
        assert_eq!(
            row_of(rows, labels.after_reception)[1..],
            ["2", "-", "1", "1", "50", "0"]
        );
        assert_eq!(row_of(rows, labels.counter_attack)[1..], ["-"; 6]);
    }

    /// The setter, the starters, the player from the bench, the libero
    /// (never from the bench), by number, then the total of the team.
    #[test]
    fn the_players_with_their_lineup_in_each_set() {
        let labels = EN;
        let report = away_report();
        let rows = &report.players.rows;
        let cells = |number: &str| {
            rows.iter()
                .find(|r| {
                    r.lead[0]
                        == Cell::Text {
                            text: number.to_string(),
                        }
                })
                .map(|r| &r.lead[2])
                .unwrap_or_else(|| panic!("expected the player {number}"))
        };
        assert_eq!(
            *cells("1"),
            Cell::Start {
                position: 1,
                setter: true
            }
        );
        assert_eq!(
            *cells("4"),
            Cell::Start {
                position: 4,
                setter: false
            }
        );
        assert_eq!(*cells("7"), Cell::Empty);
        assert_eq!(*cells("8"), Cell::Bench);
        let numbers: Vec<&Cell> = rows.iter().map(|r| &r.lead[0]).collect();
        assert!(numbers.is_sorted_by_key(|c| match c {
            Cell::Text { text } => text.parse::<u8>().ok(),
            _ => Some(u8::MAX),
        }));
        let total = rows.last().expect("expected the total");
        assert!(total.total);
        assert_eq!(
            total.lead[1],
            Cell::Text {
                text: labels.total.to_string()
            }
        );
        assert!(rows[..rows.len() - 1].iter().all(|r| !r.total));
        // the opponent errors: none for a player, one for the team
        assert_eq!(total.values.last().map(String::as_str), Some("1"));
        assert!(
            rows[..rows.len() - 1]
                .iter()
                .all(|r| r.values.last().map(String::as_str) == Some("-"))
        );
    }

    /// Every table of statistics fills the columns of its groups.
    #[test]
    fn the_statistics_tables_fill_their_columns() {
        let report = away_report();
        let stats_columns: usize = report.stats_groups.iter().map(|g| g.columns.len()).sum();
        for table in [&report.players, &report.rotations, &report.set_stats] {
            let lead_columns: usize = table.lead.iter().map(|g| g.columns.len()).sum();
            assert!(
                table
                    .rows
                    .iter()
                    .all(|r| { r.lead.len() == lead_columns && r.values.len() == stats_columns })
            );
        }
        // 6 rotations, then the total
        assert_eq!(report.rotations.rows.len(), 7);
        assert_eq!(report.set_stats.rows.len(), 2);
    }

    /// The rallies won by phase, in the whole match and by rotation.
    #[test]
    fn the_rallies_won_by_phase() {
        let t = Team::new();
        let snapshot = t
            .rallies()
            .compute_snapshot(DEFAULT_MAX_SUBSTITUTIONS)
            .expect("expected the set to replay");
        let stats = &snapshot.stats;
        let phase = |phase| StatsFilter::new(Some(phase), None, None);
        assert_eq!(
            stats.rallies_won(&phase(PhaseEnum::SideOut)),
            Some((50.0, 2, 4))
        );
        assert_eq!(
            stats.rallies_won(&phase(PhaseEnum::Break)),
            Some((0.0, 0, 1))
        );
        assert_eq!(Stats::new().rallies_won(&StatsFilter::default()), None);
        assert_eq!(
            stats.attack_summary(false, &StatsFilter::default()),
            Some(AttackSummary {
                total: 2,
                errors: 0,
                blocked: 1,
                kills: 1,
            })
        );
        assert_eq!(stats.attack_summary(true, &StatsFilter::default()), None);
    }

    /// The names are data, never typst code: quotes, backslashes and markup
    /// don't break the report.
    #[test]
    fn any_name_renders() {
        let t = Team::new();
        let names = [
            r#"Volley "A""#,
            r"back\slash",
            "#set text(red) *bold* $x$ ] [ @ref <label>",
        ];
        for name in names {
            let mut team = t.team(name);
            team.players[0].name = name.to_string();
            let m = match_entry(team, name, true, vec![t.rallies()]);
            let report = build_report(&m, EN).expect("expected the report");
            assert_eq!(report.header.teams[0].name, name);
            render_report_pdf(&report)
                .unwrap_or_else(|e| panic!("expected the report of {name}: {e}"));
        }
    }

    /// A match of five sets, with sets longer than a row of the point
    /// sequence.
    #[test]
    fn a_long_match_renders() {
        let t = Team::new();
        let sets = [(26, 24), (20, 25), (25, 23), (23, 25), (16, 14)]
            .iter()
            .enumerate()
            .map(|(i, (us, them))| {
                let events = rallies_won_by(&t, TeamSideEnum::Us, &points(*us, *them));
                t.set(i as u8 + 1, TeamSideEnum::Us, events)
            })
            .collect();
        let m = match_entry(t.team("us"), "them", true, sets);
        let report = build_report(&m, EN).expect("expected the report");
        assert_eq!(
            [report.header.teams[0].sets, report.header.teams[1].sets],
            [3, 2]
        );
        assert_eq!(report.sets.rows.len(), 5);
        assert_eq!(report.sets.rows[4][1], "16-14");
        assert_eq!(report.point_sequence.sets[0].points.len(), 50);
        render_report_pdf(&report).expect("expected the report");
    }

    /// A match not started yet: no sets, no statistics.
    #[test]
    fn an_empty_match_renders() {
        let t = Team::new();
        let m = match_entry(t.team("us"), "them", true, vec![]);
        let report = build_report(&m, EN).expect("expected the report");
        assert!(report.sets.rows.is_empty());
        // only the total of the team
        assert_eq!(report.players.rows.len(), 1);
        render_match_pdf(&m, EN).expect("expected the report");
    }

    /// The report in every language, with the names in the same one.
    #[test]
    fn every_language_renders() {
        let t = Team::new();
        for language in LanguageEnum::ALL {
            let m = match_entry(t.team("東京"), "Łódź São", true, vec![t.rallies()]);
            render_match_pdf(&m, labels(language))
                .unwrap_or_else(|e| panic!("expected the report in {language}: {e}"));
        }
    }
}
