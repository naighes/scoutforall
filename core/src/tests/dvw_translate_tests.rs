//! A set of DataVolley translated for one team: files written here rally
//! by rally, the six on court of both teams kept as DataVolley writes them
//! (the libero aside, the player it replaces in its place).

#[cfg(test)]
mod tests {
    use crate::dvw::{
        model::Side,
        parse,
        translate::{Issue, Untranslatable, Us, translate_set},
    };
    use crate::shapes::court_zone::CourtZoneEnum;
    use crate::shapes::enums::{EvalEnum, EventTypeEnum};
    use crate::shapes::refusal::Refusal;
    use crate::shapes::snapshot::EventEntry;
    use EventTypeEnum::*;
    use chrono::NaiveDate;
    use std::collections::HashMap;
    use uuid::Uuid;

    /// The home team is ours: the setter 1, outside hitters 2 and 5,
    /// middle blockers 3 and 6, the opposite 4, the libero 7; the setter in
    /// zone 1, middle blocker 6 in zone 6. The visiting team, 11 to 17.
    const HOME_LIBERO: u8 = 7;

    /// The file being written: the six on court of both teams, P1 first,
    /// the score, who serves.
    struct File {
        lines: Vec<String>,
        home: [u8; 6],
        visiting: [u8; 6],
        score: (u8, u8),
        home_serves: bool,
        /// The final scores of the sets ended (home, visiting).
        sets: Vec<(u8, u8)>,
    }

    impl File {
        /// The visiting team serves first.
        fn new() -> Self {
            let mut file = File {
                lines: vec![],
                home: [1, 2, 3, 4, 5, 6],
                visiting: [11, 12, 13, 14, 15, 16],
                score: (0, 0),
                home_serves: false,
                sets: vec![],
            };
            file.line("*P01>LUp");
            file.line("aP11>LUp");
            file
        }

        fn line(&mut self, code: &str) {
            let six = |s: [u8; 6]| s.map(|n| n.to_string()).join(";");
            self.lines.push(format!(
                "{code};;;;;;;;1;1;1;;;;{};{};",
                six(self.home),
                six(self.visiting)
            ));
        }

        /// A rally: its touches, then the point.
        fn rally(&mut self, touches: &[&str], home_wins: bool) {
            touches.iter().for_each(|t| self.line(t));
            match (home_wins, self.home_serves) {
                (true, false) => self.home.rotate_left(1),
                (false, true) => self.visiting.rotate_left(1),
                _ => {}
            }
            self.home_serves = home_wins;
            self.score = match home_wins {
                true => (self.score.0 + 1, self.score.1),
                false => (self.score.0, self.score.1 + 1),
            };
            let (marker, (h, v)) = (if home_wins { '*' } else { 'a' }, self.score);
            self.line(&format!("{marker}p{h:02}:{v:02}"));
        }

        /// The server of the team serving, in zone 1.
        fn server(&self) -> String {
            match self.home_serves {
                true => format!("*{:02}", self.home[0]),
                false => format!("a{:02}", self.visiting[0]),
            }
        }

        /// The point to the team serving: an ace.
        fn ace(&mut self) {
            let serve = format!("{}SM#", self.server());
            self.rally(&[&serve], self.home_serves);
        }

        /// Their serve received by `receiver`, and our kill of the outside
        /// hitter in the front row.
        fn received_by(&mut self, receiver: u8) {
            let serve = format!("{}SM+", self.server());
            let reception = format!("*{receiver:02}RM+");
            let hitter = self
                .home
                .iter()
                .enumerate()
                .find(|(zone, n)| matches!(zone, 1..=3) && matches!(n, 2 | 5))
                .map(|(_, n)| *n)
                .unwrap_or_default();
            let attack = format!("*{hitter:02}AH#");
            self.rally(&[&serve, &reception, &attack], true);
        }

        /// Their serve error.
        fn their_serve_error(&mut self) {
            let serve = format!("{}SM=", self.server());
            self.rally(&[&serve], true);
        }

        /// Aces of ours to the end of the set.
        fn finished(mut self) -> Self {
            while self.score.0 < self.target() {
                match self.home_serves {
                    true => self.ace(),
                    false => self.their_serve_error(),
                }
            }
            self.end_set()
        }

        /// The points to win the set: 15 in the fifth.
        fn target(&self) -> u8 {
            match self.sets.len() {
                4 => 15,
                _ => 25,
            }
        }

        /// The end of the set at its score.
        fn end_set(mut self) -> Self {
            self.sets.push(self.score);
            self.lines.push(format!("**{}set", self.sets.len()));
            self
        }

        /// The next set: the six of the first one, the visiting team
        /// serving.
        fn next_set(mut self) -> Self {
            self.home = [1, 2, 3, 4, 5, 6];
            self.visiting = [11, 12, 13, 14, 15, 16];
            self.score = (0, 0);
            self.home_serves = false;
            self.line("*P01>LUp");
            self.line("aP11>LUp");
            self
        }

        /// Their aces to the end of the set.
        fn lost(mut self) -> Self {
            while self.score.1 < self.target() {
                match self.home_serves {
                    true => {
                        let serve = format!("{}SM=", self.server());
                        self.rally(&[&serve], false);
                    }
                    false => self.ace(),
                }
            }
            self.end_set()
        }

        fn text(&self) -> String {
            let player = |team: u8, n: u8, role: u8| {
                let libero = if role == 1 { "L" } else { "" };
                format!("{team};{n};{n};*;;;;;P{n};Surname{n};Name{n};;{libero};{role};False;;;")
            };
            let roles = [5, 2, 4, 3, 2, 4, 1, 2];
            let players = |team: u8, first: u8| {
                (0..8u8)
                    .map(|i| player(team, first + i, roles[usize::from(i)]))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            [
                "[3DATAVOLLEYSCOUT]\nFILEFORMAT: 2.0\nGENERATOR-PRG: Data Volley".to_string(),
                "[3MATCH]\n01/25/2015;;2014/2015;Test;;;;;1;1;Z;0;".to_string(),
                "[3TEAMS]\nH;Home;1;;;;\nV;Visiting;0;;;;".to_string(),
                format!(
                    "[3SET]\n{}",
                    (0..5)
                        .map(|i| match self.sets.get(i) {
                            Some((h, v)) => format!("True;;;;{h}-{v};;"),
                            None => "True;;;;;;".to_string(),
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                format!("[3PLAYERS-H]\n{}", players(0, 1)),
                format!("[3PLAYERS-V]\n{}", players(1, 11)),
                format!("[3SCOUT]\n{}", self.lines.join("\n")),
            ]
            .join("\n")
        }
    }

    /// The set translated for the home team: the events, and the number of
    /// each player.
    fn translated_events(file: &File) -> Result<(Vec<EventEntry>, HashMap<Uuid, u8>), Issue> {
        let dvw = parse(file.text().as_bytes()).expect("expected a DataVolley file");
        let players: HashMap<u8, Uuid> = (1..=8).map(|n| (n, Uuid::new_v4())).collect();
        let numbers: HashMap<Uuid, u8> = players.iter().map(|(n, id)| (*id, *n)).collect();
        let us = Us {
            side: Side::Home,
            players: &players,
            liberos: &[HOME_LIBERO],
        };
        let date = NaiveDate::from_ymd_opt(2015, 1, 25).expect("a date");
        translate_set(&dvw, &us, 1, date, 6)
            .map(|(set, _)| (set.events, numbers))
            .map_err(|e| e.issue)
    }

    /// The set translated for the home team: the events, by type and
    /// player number.
    fn translated(file: &File) -> Result<Vec<(EventTypeEnum, Option<u8>)>, Issue> {
        translated_events(file).map(|(events, numbers)| {
            events
                .iter()
                .map(|e| {
                    (
                        e.event_type,
                        e.player.and_then(|p| numbers.get(&p).copied()),
                    )
                })
                .collect()
        })
    }

    /// The set translated for the home team: the events, by type and
    /// evaluation.
    fn evaluated(file: &File) -> Result<Vec<(EventTypeEnum, Option<EvalEnum>)>, Issue> {
        translated_events(file)
            .map(|(events, _)| events.iter().map(|e| (e.event_type, e.eval)).collect())
    }

    /// The events of the set up to the first `count`.
    fn first(
        events: &[(EventTypeEnum, Option<u8>)],
        count: usize,
    ) -> Vec<(EventTypeEnum, Option<u8>)> {
        events.iter().take(count).copied().collect()
    }

    /// The libero touching as it plays: nothing to change.
    #[test]
    fn the_libero_plays_as_ever() {
        let mut file = File::new();
        file.received_by(HOME_LIBERO);
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(first(&events, 2), vec![(P, Some(7)), (A, Some(2))]);
        assert!(!events.iter().any(|(t, _)| *t == LR), "{events:?}");
    }

    /// The middle blocker the libero replaces receives: the libero out
    /// before the rally; in the next rally (our ace) nobody tells it, back
    /// in.
    #[test]
    fn the_middle_blocker_touching_takes_the_libero_out() {
        let mut file = File::new();
        file.received_by(6);
        file.ace();
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(
            first(&events, 5),
            vec![
                (LR, None),
                (P, Some(6)),
                (A, Some(2)),
                (LR, None),
                (S, Some(2))
            ]
        );
        assert_eq!(events.iter().filter(|(t, _)| *t == LR).count(), 2);
    }

    /// Out for the rallies the middle blocker touches only: in between,
    /// nobody telling it, the libero plays, as it does most of the time.
    #[test]
    fn the_libero_out_for_the_rallies_of_the_middle_blocker_only() {
        let mut file = File::new();
        file.received_by(6);
        // our ace, then their point on our serve error: they serve again,
        // middle blocker 6 still in the back row (zone 5)
        file.ace();
        file.rally(&["*02SM="], false);
        file.received_by(6);
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(
            first(&events, 10),
            vec![
                (LR, None),
                (P, Some(6)),
                (A, Some(2)),
                (LR, None),
                (S, Some(2)),
                (S, Some(2)),
                (LR, None),
                (P, Some(6)),
                (A, Some(5)),
                (LR, None),
            ]
        );
        assert_eq!(events.iter().filter(|(t, _)| *t == LR).count(), 4);
    }

    /// The libero and the middle blocker it replaces in the same rally,
    /// receiving: the libero for another player of the back row, but two
    /// don't touch (the setter, the outside hitter 5): who is out can't be
    /// told, the set isn't translated.
    #[test]
    fn the_libero_for_one_of_two_players_is_refused() {
        let mut file = File::new();
        file.rally(&["a11SM+", "*07RM+", "*06AH#"], true);
        assert_eq!(
            translated(&file.finished()),
            Err(Issue::Untranslatable(Untranslatable::LiberoForWhom(vec![
                1, 5
            ])))
        );
    }

    /// The libero and the middle blocker it replaces touching, the only
    /// other player who can be out the outside hitter 2: the libero for
    /// them, chosen a rally earlier (the libero out for that rally, a
    /// rally between two libero replacements).
    #[test]
    fn the_libero_for_the_only_player_out_a_rally_earlier() {
        let mut file = File::new();
        // their serve error: we serve, outside hitter 2 in position 1
        file.their_serve_error();
        // our serve error: they serve, 2 in position 1 receiving
        file.rally(&["*02SM="], false);
        // the libero receives, the setter (6) and the middle blocker (5)
        // touch: 2 is out
        file.rally(&["a12SM+", "*07RM+", "*01EM+", "*06AH#"], true);
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(
            first(&events, 7),
            vec![
                (OE, None),
                (LF, Some(2)),
                (S, Some(2)),
                (P, Some(7)),
                (A, Some(6)),
                (S, Some(3)),
                (S, Some(3)),
            ]
        );
    }

    /// The same in the first rally: no rally before to choose the player
    /// in, the set isn't translated.
    #[test]
    fn the_libero_for_another_player_in_the_first_rally_is_refused() {
        let mut file = File::new();
        // receiving: the setter, the middle blocker and the libero touch,
        // the outside hitter 5 the only one out
        file.rally(&["a11SM+", "*07RM+", "*01EM+", "*06AH#"], true);
        assert_eq!(
            translated(&file.finished()),
            Err(Issue::Untranslatable(Untranslatable::LiberoTooLate))
        );
    }

    /// The libero touching in the rally before too: it can't go out to
    /// come back for the other player, the set isn't translated.
    #[test]
    fn the_libero_touching_the_rally_before_is_refused() {
        let mut file = File::new();
        file.their_serve_error();
        // our serve, their attack dug by the libero, our attack out: they
        // serve, the libero on court in the rally just before
        file.rally(&["*02SM+", "a12AM+", "*07DM+", "*03AH="], false);
        file.rally(&["a12SM+", "*07RM+", "*01EM+", "*06AH#"], true);
        assert_eq!(
            translated(&file.finished()),
            Err(Issue::Untranslatable(Untranslatable::LiberoTooLate))
        );
    }

    /// The libero touching with the three players of the back row: it
    /// can't be on court, the set isn't translated.
    #[test]
    fn the_libero_with_the_whole_back_row_is_refused() {
        let mut file = File::new();
        file.rally(
            &[
                "a11SM+", "*07RM+", "*01EM+", "*05AH+", "a12DM+", "a13AM+", "*06DM+", "*02AH#",
            ],
            true,
        );
        assert_eq!(
            translated(&file.finished()),
            Err(Issue::Untranslatable(Untranslatable::LiberoForNobody))
        );
    }

    /// A player substituted, then touching the ball (as in a file of
    /// Cortona vs Pol Remo, the substitute's touch given to the player out):
    /// the touch refused, at its own line, the player by number.
    #[test]
    fn a_touch_of_a_player_substituted_is_refused_at_its_line() {
        let mut file = File::new();
        file.their_serve_error();
        file.line("*c04:08");
        let serve = format!("{}SM+", file.server());
        file.rally(&[&serve, "a12RM+", "a13AH+", "*04DH+", "*02AH#"], true);
        let dig = file
            .lines
            .iter()
            .position(|l| l.starts_with("*04DH+"))
            .expect("the dig");
        let text = file.finished().text();
        // the scout's lines after its header, numbered from 1
        let scout = text
            .lines()
            .position(|l| l == "[3SCOUT]")
            .expect("the scout");
        let dvw = parse(text.as_bytes()).expect("expected a DataVolley file");
        let players: HashMap<u8, Uuid> = (1..=8).map(|n| (n, Uuid::new_v4())).collect();
        let us = Us {
            side: Side::Home,
            players: &players,
            liberos: &[HOME_LIBERO],
        };
        let date = NaiveDate::from_ymd_opt(2015, 1, 25).expect("a date");
        let error = translate_set(&dvw, &us, 1, date, 6).expect_err("the dig refused");
        assert_eq!(
            error.issue,
            Issue::Refused {
                event: D,
                refusal: Refusal::NotOnCourt(4),
            }
        );
        assert_eq!(error.line, Some(scout + 2 + dig));
    }

    /// The server of the file not in zone 1 by the rules: refused.
    #[test]
    fn a_server_not_in_zone_1_is_refused() {
        let mut file = File::new();
        file.their_serve_error();
        file.rally(&["*03SM#"], true);
        match translated(&file.finished()) {
            Err(Issue::Refused {
                event: S,
                refusal: Refusal::NotServing(3),
            }) => {}
            other => panic!("expected the serve refused, got {other:?}"),
        }
    }

    /// Their attack blocked (`/`) with our point by nobody (`$$&`), as
    /// files scouted without blocks write it: our block wants the player,
    /// the set is refused.
    #[test]
    fn their_attack_blocked_without_our_block_is_refused() {
        let mut file = File::new();
        file.rally(&["a11SM+", "*07RM+", "*02AH+", "a14AH/", "*$$&H#"], true);
        assert_eq!(
            translated(&file.finished()),
            Err(Issue::Untranslatable(
                Untranslatable::TheirAttackBlockedWithoutOurBlock
            ))
        );
    }

    /// Their attack, then their freeball sent: the ball came back to them,
    /// our touch not scouted (a dig over, a freeball): our freeball
    /// (`FB`), then theirs (`OF`).
    #[test]
    fn their_freeball_right_after_their_attack_follows_ours() {
        let mut file = File::new();
        file.rally(
            &[
                "a11SM+", "*07RM+", "*02AH+", "a14AH+", "a13FH+", "*01EH+", "*04AH#",
            ],
            true,
        );
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(
            first(&events, 6),
            vec![
                (P, Some(7)),
                (A, Some(2)),
                (FB, None),
                (OF, None),
                (A, Some(4)),
                (S, Some(2))
            ]
        );
    }

    /// Our serve `-`, their reception, their set and their freeball sent:
    /// the serve keeps its evaluation, their freeball (`OF`) follows.
    #[test]
    fn their_freeball_after_their_reception_of_our_serve() {
        let mut file = File::new();
        file.their_serve_error();
        let serve = format!("{}SM-", file.server());
        file.rally(
            &[&serve, "a11RM#", "a13EH+", "a14FH+", "*07FH+", "*04AH#"],
            true,
        );
        let file = file.finished();
        let events = translated(&file).expect("expected the set");
        assert_eq!(
            first(&events, 4),
            vec![(OE, None), (S, Some(2)), (OF, None), (A, Some(4))]
        );
        let evals = evaluated(&file).expect("expected the set");
        assert_eq!(evals.get(1), Some(&(S, Some(EvalEnum::Negative))));
    }

    /// Our serves with the zone where they went: the zone, its quadrant
    /// (the subzone) left out; none when the file doesn't tell it, nor for
    /// the other touches.
    #[test]
    fn the_zone_of_our_serves() {
        let mut file = File::new();
        file.their_serve_error();
        let to = |zone: &str| format!("{}SM#~~~1{zone}", file.server());
        // the same server all along: we keep serving
        let (to_4, to_6) = (to("4C"), to("6"));
        file.rally(&[&to_4], true);
        file.rally(&[&to_6], true);
        let plain = format!("{}SM#", file.server());
        file.rally(&[&plain], true);
        let file = file.finished();
        let (events, _) = translated_events(&file).expect("expected the set");
        let targets: Vec<_> = events
            .iter()
            .take(4)
            .map(|e| (e.event_type, e.flags.target()))
            .collect();
        assert_eq!(
            targets,
            vec![
                (OE, None),
                (S, Some(CourtZoneEnum::Z4)),
                (S, Some(CourtZoneEnum::Z6)),
                (S, None),
            ]
        );
    }

    /// Our serve `+`, their reception `-` and our attack, their touches
    /// after the reception not scouted: the ball came back without their
    /// attack, a freeball (`OF`), the serve keeping its evaluation.
    #[test]
    fn their_freeball_not_scouted_after_their_reception() {
        let mut file = File::new();
        file.their_serve_error();
        let serve = format!("{}SM+", file.server());
        file.rally(&[&serve, "a11RM-", "*04AH#"], true);
        let file = file.finished();
        let events = translated(&file).expect("expected the set");
        assert_eq!(
            first(&events, 4),
            vec![(OE, None), (S, Some(2)), (OF, None), (A, Some(4))]
        );
        let evals = evaluated(&file).expect("expected the set");
        assert_eq!(evals.get(1), Some(&(S, Some(EvalEnum::Positive))));
    }

    /// Their reception `/`, straight over: our serve `/`, no freeball.
    #[test]
    fn their_reception_over_is_our_serve_over() {
        let mut file = File::new();
        file.their_serve_error();
        let serve = format!("{}SM+", file.server());
        file.rally(&[&serve, "a11RM/", "*04AH#"], true);
        let file = file.finished();
        let events = translated(&file).expect("expected the set");
        assert_eq!(
            first(&events, 3),
            vec![(OE, None), (S, Some(2)), (A, Some(4))]
        );
        let evals = evaluated(&file).expect("expected the set");
        assert_eq!(evals.get(1), Some(&(S, Some(EvalEnum::Over))));
    }

    /// Our attack `-`, their dig and their freeball sent: our attack keeps
    /// `-`, their freeball (`OF`) follows; scouted or not.
    #[test]
    fn their_freeball_after_digging_our_attack() {
        for touches in [
            vec!["a11SM+", "*07RM+", "*02AH-", "a12DH+", "a13FH+", "*04AH#"],
            vec!["a11SM+", "*07RM+", "*02AH-", "a12DH+", "*04AH#"],
        ] {
            let mut file = File::new();
            file.rally(&touches, true);
            let file = file.finished();
            let events = translated(&file).expect("expected the set");
            assert_eq!(
                first(&events, 4),
                vec![(P, Some(7)), (A, Some(2)), (OF, None), (A, Some(4))],
                "{touches:?}"
            );
            let evals = evaluated(&file).expect("expected the set");
            assert_eq!(
                evals.get(1),
                Some(&(A, Some(EvalEnum::Negative))),
                "{touches:?}"
            );
        }
    }

    /// Our serve, their reception and set, our block: they attacked, not
    /// in the file; our serve keeps its evaluation.
    #[test]
    fn our_block_after_their_reception_tells_their_attack() {
        let mut file = File::new();
        file.their_serve_error();
        let serve = format!("{}SM-", file.server());
        file.rally(&[&serve, "a11RM+", "a13EH+", "*03BH="], false);
        let file = file.finished();
        let events = evaluated(&file).expect("expected the set");
        assert_eq!(
            events.iter().skip(1).take(2).copied().collect::<Vec<_>>(),
            vec![(S, Some(EvalEnum::Negative)), (B, Some(EvalEnum::Error))]
        );
    }

    /// Our attack `-` back from their block, our dig: no freeball of
    /// theirs, the ball back to us without their attack (our `+`).
    #[test]
    fn our_attack_back_from_their_block_is_no_freeball() {
        let mut file = File::new();
        file.rally(
            &["a11SM+", "*07RM+", "*02AH-", "a13BH+", "*07DH+", "*04AH#"],
            true,
        );
        let file = file.finished();
        let events = evaluated(&file).expect("expected the set");
        assert_eq!(
            events.iter().take(3).copied().collect::<Vec<_>>(),
            vec![
                (P, Some(EvalEnum::Positive)),
                (A, Some(EvalEnum::Positive)),
                (A, Some(EvalEnum::Perfect))
            ]
        );
    }

    /// Their freeball received by us and sent straight back (they play it
    /// next): their freeball, then ours; told by their set going over too.
    #[test]
    fn their_freeball_received_back_over_is_ours() {
        for touches in [
            vec![
                "a11SM+", "*07RM+", "*02AH+", "a12DH+", "a13FH+", "*07FH-", "a14AH#",
            ],
            vec![
                "a11SM+", "*07RM+", "*02AH+", "a12DH+", "a13EH-", "*07FH/", "a14AH#",
            ],
        ] {
            let mut file = File::new();
            file.rally(&touches, false);
            let events = translated(&file.finished()).expect("expected the set");
            assert_eq!(
                first(&events, 5),
                vec![
                    (P, Some(7)),
                    (A, Some(2)),
                    (OF, None),
                    (FB, None),
                    (OS, None)
                ],
                "{touches:?}"
            );
        }
    }

    /// Our cover after their block going over, their attack next: our
    /// freeball.
    #[test]
    fn our_cover_played_by_them_is_our_freeball() {
        let mut file = File::new();
        file.rally(
            &["a11SM+", "*07RM+", "*02AH-", "a13BH+", "*07DH+", "a14AH#"],
            false,
        );
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(
            first(&events, 4),
            vec![(P, Some(7)), (A, Some(2)), (FB, None), (OS, None)]
        );
    }

    /// Our error and their point, both without a player, the ball ours:
    /// our fault.
    #[test]
    fn their_point_with_the_ball_ours_is_our_fault() {
        let mut file = File::new();
        file.rally(
            &[
                "a11SM+", "*07RM+", "*02AH-", "a13BH+", "*07DH-", "*$$&H=", "a$$&H#",
            ],
            false,
        );
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(
            first(&events, 3),
            vec![(P, Some(7)), (A, Some(2)), (F, None)]
        );
    }

    /// Our block on their reception straight over and on their freeball:
    /// a block, as the file tells, not an attack.
    #[test]
    fn our_block_on_a_ball_not_attacked() {
        let mut file = File::new();
        file.their_serve_error();
        let serve = format!("{}SM+", file.server());
        file.rally(&[&serve, "a11RM/", "*03BH#"], true);
        let serve = format!("{}SM-", file.server());
        file.rally(&[&serve, "a11RM+", "a13EH+", "a14FH+", "*03BH="], false);
        let file = file.finished();
        let events = evaluated(&file).expect("expected the set");
        assert_eq!(
            events.iter().take(6).copied().collect::<Vec<_>>(),
            vec![
                (OE, None),
                (S, Some(EvalEnum::Over)),
                (B, Some(EvalEnum::Perfect)),
                (S, Some(EvalEnum::Negative)),
                (OF, None),
                (B, Some(EvalEnum::Error))
            ]
        );
    }

    /// Their winning attack off our hands ("mani fuori", our block `=`),
    /// then our dig `=` trying to reach it: the point is the block's, the
    /// dig dropped.
    #[test]
    fn our_dig_after_our_block_out_is_dropped() {
        let mut file = File::new();
        file.rally(
            &[
                "a11SM+", "*07RM+", "*02AH-", "a13DH+", "a14AH#", "*03BH=", "*04DH=",
            ],
            false,
        );
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(
            first(&events, 3),
            vec![(P, Some(7)), (A, Some(2)), (B, Some(3))]
        );
        assert!(!events.iter().any(|(e, _)| *e == D), "{events:?}");
    }

    /// Our attack blocked (`/`, their block `#`), then our dig `=` trying
    /// to cover it: the point is the block's, the dig dropped.
    #[test]
    fn our_dig_after_our_attack_blocked_is_dropped() {
        let mut file = File::new();
        file.rally(&["a11SM+", "*07RM+", "*02AH/", "a13BH#", "*04DH="], false);
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(first(&events, 2), vec![(P, Some(7)), (A, Some(2))]);
        assert!(!events.iter().any(|(e, _)| *e == D), "{events:?}");
    }

    /// Our kill touched by their block (`a13BH-`, then their dig `=`, as
    /// in a file of Cortona vs Cascina): the point is ours, their block
    /// dropped.
    #[test]
    fn their_block_on_our_kill_is_dropped() {
        for rally in [
            vec!["a11SM+", "*07RM+", "*02AH#", "a13BH-"],
            vec!["a11SM+", "*07RM+", "*02AH#", "a13BH-", "a15DH="],
            vec!["a11SM+", "*07RM+", "*02AH#", "a13BH!"],
        ] {
            let mut file = File::new();
            file.rally(&rally, true);
            let events = translated(&file.finished()).expect("expected the set");
            assert_eq!(
                first(&events, 2),
                vec![(P, Some(7)), (A, Some(2))],
                "{rally:?}"
            );
            assert!(!events.iter().any(|(e, _)| *e == B), "{events:?}");
        }
    }

    /// Their touch after the end that keeps the ball is still refused when
    /// it isn't their block on our kill: a dig after our kill, a block
    /// after our ace.
    #[test]
    fn their_touch_after_the_end_otherwise_is_refused() {
        let mut ace = File::new();
        ace.their_serve_error();
        let serve = format!("{}SM#", ace.server());
        ace.rally(&[&serve, "a13BH-"], true);
        let mut files = vec![ace];
        for rally in [
            vec!["a11SM+", "*07RM+", "*02AH#", "a15DH+"],
            vec!["a11SM+", "*07RM+", "*02AH#", "a15DH+", "a13BH-"],
        ] {
            let mut file = File::new();
            file.rally(&rally, true);
            files.push(file);
        }
        for file in files {
            let rally = file.lines.clone();
            assert_eq!(
                translated(&file.finished()),
                Err(Issue::Untranslatable(Untranslatable::TheirTouchAfterTheEnd)),
                "{rally:?}"
            );
        }
    }

    /// After our block `=` or our attack `/` only our dig `=` is dropped:
    /// a dig keeping the ball, or a dig `=` after another end, is still our
    /// touch after the end.
    #[test]
    fn our_touch_after_the_end_otherwise_is_refused() {
        for rally in [
            vec![
                "a11SM+", "*07RM+", "*02AH-", "a13DH+", "a14AH#", "*03BH=", "*04DH+",
            ],
            vec!["a11SM+", "*07RM+", "*02AH=", "*04DH="],
            vec![
                "a11SM+", "*07RM+", "*02AH-", "a13DH+", "a14AH#", "*04DH=", "*03BH=",
            ],
            vec![
                "a11SM+", "*07RM+", "*02AH-", "a13DH+", "a14AH#", "*03BH=", "*04AH=",
            ],
            vec!["a11SM+", "*07RM+", "*02AH/", "a13BH#", "*04DH+"],
            vec!["a11SM+", "*07RM+", "*02AH/", "a13BH#", "*04AH="],
        ] {
            let mut file = File::new();
            file.rally(&rally, false);
            assert_eq!(
                translated(&file.finished()),
                Err(Issue::Untranslatable(Untranslatable::OurTouchAfterTheEnd)),
                "{rally:?}"
            );
        }
    }

    /// Our set `=` then our attack: the rally went on, the set is no fault
    /// and the attack ends it as the file says.
    #[test]
    fn our_set_error_attacked_is_no_fault() {
        let mut file = File::new();
        file.rally(&["a11SM!", "*07RM+", "*01EM=", "*02AM="], false);
        let events = evaluated(&file.finished()).expect("expected the set");
        assert_eq!(
            events.iter().take(2).copied().collect::<Vec<_>>(),
            vec![(P, Some(EvalEnum::Positive)), (A, Some(EvalEnum::Error))]
        );
        assert!(!events.iter().any(|(e, _)| *e == F), "{events:?}");
    }

    /// Our set `=` with nothing after it: our fault, as before.
    #[test]
    fn our_set_error_alone_is_a_fault() {
        let mut file = File::new();
        file.rally(&["a11SM!", "*07RM+", "*01EM="], false);
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(first(&events, 2), vec![(P, Some(7)), (F, Some(1))]);
    }

    /// Our attack `=` then their block `=`, our point: the ball off their
    /// hands ("mani fuori"), our winning attack.
    #[test]
    fn our_attack_error_off_their_block_out_is_our_winning_attack() {
        let mut file = File::new();
        file.rally(&["a11SM+", "*07RM+", "*02AM=", "a13BN="], true);
        let events = evaluated(&file.finished()).expect("expected the set");
        assert_eq!(
            events.iter().take(2).copied().collect::<Vec<_>>(),
            vec![(P, Some(EvalEnum::Positive)), (A, Some(EvalEnum::Perfect))]
        );
    }

    /// Our attack `=` with their block `/` (invasion) or nothing after it:
    /// still our error, the file's point theirs.
    #[test]
    fn our_attack_error_otherwise_stays_an_error() {
        for touches in [
            vec!["a11SM+", "*07RM+", "*02AM="],
            vec!["a11SM+", "*07RM+", "*02AM=", "a13BN/"],
        ] {
            let mut file = File::new();
            file.rally(&touches, false);
            let events = evaluated(&file.finished()).expect("expected the set");
            assert_eq!(
                events.iter().take(2).copied().collect::<Vec<_>>(),
                vec![(P, Some(EvalEnum::Positive)), (A, Some(EvalEnum::Error))],
                "{touches:?}"
            );
        }
    }

    /// Their attack `!`, back from our block, then their freeball: our
    /// block wants the player, the set is refused.
    #[test]
    fn their_freeball_after_their_attack_back_from_our_block_is_refused() {
        let mut file = File::new();
        file.rally(
            &["a11SM+", "*07RM+", "*02AH+", "a14AH!", "a13FH+", "*04AH#"],
            true,
        );
        assert_eq!(
            translated(&file.finished()),
            Err(Issue::Untranslatable(
                Untranslatable::TheirAttackBackWithoutOurBlock
            ))
        );
    }

    /// Our `F` after our reception, theirs touching next: our freeball
    /// sent over (`FB`), the reception keeping its evaluation; their `F`
    /// receiving it tells nothing more.
    #[test]
    fn our_freeball_sent_is_ours() {
        let mut file = File::new();
        file.rally(
            &["a11SM+", "*07RM+", "*01FH+", "a12FH+", "a13EH+", "a14AH#"],
            false,
        );
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(
            first(&events, 3),
            vec![(P, Some(7)), (FB, None), (OS, None)]
        );
    }

    /// Their `F` after their dig, ours touching next: their freeball sent
    /// over (`OF`), after our attack `+`; our `F` receiving it tells
    /// nothing more.
    #[test]
    fn their_freeball_sent_is_theirs() {
        let mut file = File::new();
        file.rally(
            &[
                "a11SM+", "*07RM+", "*02AH+", "a12DH+", "a13FH+", "*07FH+", "*01EH+", "*04AH#",
            ],
            true,
        );
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(
            first(&events, 4),
            vec![(P, Some(7)), (A, Some(2)), (OF, None), (A, Some(4))]
        );
    }

    /// Reading the evaluations: a reception `+` kept is ours `+`.
    #[test]
    fn the_reception_keeps_its_evaluation() {
        let mut file = File::new();
        file.received_by(HOME_LIBERO);
        let dvw = parse(file.finished().text().as_bytes()).expect("a file");
        let players: HashMap<u8, Uuid> = (1..=8).map(|n| (n, Uuid::new_v4())).collect();
        let us = Us {
            side: Side::Home,
            players: &players,
            liberos: &[HOME_LIBERO],
        };
        let date = NaiveDate::from_ymd_opt(2015, 1, 25).expect("a date");
        let (set, _) = translate_set(&dvw, &us, 1, date, 6).expect("the set");
        assert_eq!(set.events[0].eval, Some(EvalEnum::Positive));
    }

    /// The match of a side, as the import plans it: a new team, the day of
    /// the file at midnight, six substitutions.
    fn planned(
        text: &str,
        side: Side,
    ) -> Result<crate::dvw::import::MatchPlan, crate::dvw::import::MatchIssue> {
        planned_for(
            text,
            side,
            crate::dvw::import::TeamChoice::New {
                classification: Some(crate::shapes::enums::TeamClassificationEnum::Regional),
                gender: Some(crate::shapes::enums::GenderEnum::Women),
                country: crate::shapes::country::CountryCode::parse("AT"),
            },
        )
        .map(|(_, plan)| plan)
    }

    /// The team and the match of a side, the team as chosen.
    fn planned_for(
        text: &str,
        side: Side,
        choice: crate::dvw::import::TeamChoice,
    ) -> Result<
        (crate::dvw::import::TeamPlan, crate::dvw::import::MatchPlan),
        crate::dvw::import::MatchIssue,
    > {
        use crate::dvw::import::{match_date, match_plan, team_plan};
        let dvw = parse(text.as_bytes()).expect("expected a DataVolley file");
        let team = team_plan(&dvw, side, choice).expect("a team");
        let day = NaiveDate::from_ymd_opt(2015, 1, 25).expect("a date");
        let offset = chrono::FixedOffset::east_opt(3600).expect("an offset");
        match_plan(&dvw, side, &team, match_date(&dvw, day, offset), 6).map(|m| (team, m))
    }

    fn three_sets_won() -> File {
        let mut file = File::new();
        file.received_by(5);
        file.finished().next_set().finished().next_set().finished()
    }

    /// Three sets won: the match against the other team, at home, every
    /// set with its events, the roster of the file, finished.
    #[test]
    fn the_match_of_the_home_team() {
        let plan = planned(&three_sets_won().text(), Side::Home).expect("a match");
        let m = &plan.entry;
        assert_eq!(m.opponent, "Visiting");
        assert!(m.home);
        assert_eq!(m.max_substitutions, 6);
        assert_eq!(m.date.to_rfc3339(), "2015-01-25T00:00:00+01:00");
        assert_eq!(
            m.id,
            crate::shapes::r#match::MatchEntry::id_of("Visiting", &m.date)
        );
        assert_eq!(m.sets.len(), 3);
        assert!(m.sets.iter().all(|s| !s.events.is_empty()));
        assert_eq!(m.roster.len(), 8);
        assert_eq!(m.team.players.len(), 8);
        let status = m.get_status().expect("a status");
        assert!(status.match_finished);
        assert_eq!((status.us_wins, status.them_wins), (3, 0));
    }

    /// The same file for the visiting team: away, against the home team,
    /// three sets lost.
    #[test]
    fn the_match_of_the_visiting_team() {
        let mut file = File::new();
        file.their_serve_error();
        let file = file.finished().next_set().finished().next_set().finished();
        let plan = planned(&file.text(), Side::Visiting).expect("a match");
        assert_eq!(plan.entry.opponent, "Home");
        assert!(!plan.entry.home);
        let status = plan.entry.get_status().expect("a status");
        assert_eq!((status.us_wins, status.them_wins), (0, 3));
    }

    /// Two sets: the match isn't finished, nothing to import.
    #[test]
    fn an_unfinished_match_is_refused() {
        let file = File::new().finished().next_set().lost();
        assert_eq!(
            planned(&file.text(), Side::Home).map(|p| p.entry.sets.len()),
            Err(crate::dvw::import::MatchIssue::Unfinished)
        );
    }

    /// A set that doesn't translate: the match isn't imported, the set and
    /// the issue said.
    #[test]
    fn a_set_not_translated_refuses_the_match() {
        let mut file = File::new().finished().next_set();
        file.rally(&["a11SM+", "*07RM+", "*02AH=", "*04DH="], false);
        let file = file.finished().next_set().finished();
        match planned(&file.text(), Side::Home) {
            Err(crate::dvw::import::MatchIssue::Set(e)) => {
                assert_eq!(e.set_number, 2);
                assert_eq!(
                    e.issue,
                    Issue::Untranslatable(Untranslatable::OurTouchAfterTheEnd)
                );
            }
            other => panic!("expected the set refused: {other:?}"),
        }
    }

    /// The final score of `[3SET]` not the one of the rallies: refused.
    #[test]
    fn a_final_score_not_the_rallies_is_refused() {
        let text = three_sets_won()
            .text()
            .replacen("True;;;;25-0;;", "True;;;;25-2;;", 1);
        assert_eq!(
            planned(&text, Side::Home).map(|p| p.entry.sets.len()),
            Err(crate::dvw::import::MatchIssue::SetScore {
                set_number: 1,
                file: (25, 2),
                ours: (25, 0),
            })
        );
    }

    /// The time of the file, when it has one.
    #[test]
    fn the_time_of_the_file() {
        let text = three_sets_won().text().replacen(
            "01/25/2015;;2014/2015",
            "01/25/2015;20.30.00;2014/2015",
            1,
        );
        let plan = planned(&text, Side::Home).expect("a match");
        assert_eq!(plan.entry.date.to_rfc3339(), "2015-01-25T20:30:00+01:00");
    }

    /// Writes the match of three sets won to `DVW_OUT`, the fixture of the
    /// import tests of the TUI and the app.
    #[test]
    #[ignore]
    fn write_three_sets_won() {
        let path = std::env::var("DVW_OUT").expect("DVW_OUT");
        std::fs::write(path, three_sets_won().text()).expect("written");
    }

    /// Saving an import into a store in memory: the matches queued for
    /// the statistics, as the finished ones.
    mod saving {
        use super::*;
        use crate::{
            analytics::finished::FinishedMatches,
            dvw::import::{MatchPlan, TeamChoice, TeamPlan, match_exists, save_import},
            errors::{AppError, MatchError},
            localization::Language,
            providers::{
                match_reader::MatchReader,
                store::{
                    match_reader::StoreMatchReader, match_writer::StoreMatchWriter,
                    set_reader::StoreSetReader, set_writer::StoreSetWriter,
                    team_reader::StoreTeamReader, team_writer::StoreTeamWriter,
                },
                team_reader::TeamReader,
            },
            shapes::r#match::MatchSource,
            storage::{Store, layout, memory::MemoryStore},
        };
        use std::sync::{Arc, Mutex};

        #[derive(Default)]
        struct Queued(Mutex<Vec<(Uuid, String)>>);

        #[async_trait::async_trait]
        impl FinishedMatches for Queued {
            async fn finished(&self, team_id: Uuid, match_id: String) {
                if let Ok(mut queued) = self.0.lock() {
                    queued.push((team_id, match_id));
                }
            }
        }

        struct Providers {
            store: Arc<dyn Store>,
            team_reader: StoreTeamReader,
            team_writer: StoreTeamWriter,
            match_reader: StoreMatchReader,
            match_writer: StoreMatchWriter,
            set_writer: StoreSetWriter,
            queued: Queued,
        }

        impl Providers {
            fn new() -> Self {
                let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
                Self {
                    team_reader: StoreTeamReader::new(Arc::clone(&store), Language::default()),
                    team_writer: StoreTeamWriter::new(Arc::clone(&store)),
                    match_reader: StoreMatchReader::new(
                        Arc::clone(&store),
                        Arc::new(StoreSetReader::new(Arc::clone(&store), Language::default())),
                        Language::default(),
                    ),
                    match_writer: StoreMatchWriter::new(Arc::clone(&store)),
                    set_writer: StoreSetWriter::new(Arc::clone(&store), Language::default()),
                    queued: Queued::default(),
                    store,
                }
            }

            async fn save(
                &self,
                team: &TeamPlan,
                plan: &MatchPlan,
                overwrite: bool,
            ) -> Result<crate::shapes::r#match::MatchEntry, AppError> {
                save_import(
                    team,
                    plan,
                    overwrite,
                    &self.team_writer,
                    &self.match_reader,
                    &self.match_writer,
                    &self.set_writer,
                    &self.queued,
                )
                .await
            }

            fn queued(&self) -> Vec<(Uuid, String)> {
                self.queued.0.lock().map(|q| q.clone()).unwrap_or_default()
            }
        }

        fn new_team() -> TeamChoice<'static> {
            TeamChoice::New {
                classification: Some(crate::shapes::enums::TeamClassificationEnum::Regional),
                gender: Some(crate::shapes::enums::GenderEnum::Women),
                country: crate::shapes::country::CountryCode::parse("AT"),
            }
        }

        /// Five sets: won, lost, won, lost, won.
        fn five_sets() -> File {
            File::new()
                .finished()
                .next_set()
                .lost()
                .next_set()
                .finished()
                .next_set()
                .lost()
                .next_set()
                .finished()
        }

        /// A new team: saved with the players of the file, the match with
        /// its sets as translated, queued once.
        #[tokio::test]
        async fn a_new_team_and_its_match() {
            let p = Providers::new();
            let (team, plan) =
                planned_for(&three_sets_won().text(), Side::Home, new_team()).expect("a match");
            let saved = p.save(&team, &plan, false).await.expect("saved");
            let read_team = p
                .team_reader
                .read_single(&team.team.id)
                .await
                .expect("the team");
            assert_eq!(read_team.players.len(), 8);
            let read = p
                .match_reader
                .read_single(&read_team, &saved.id)
                .await
                .expect("the match");
            assert_eq!(read.opponent, "Visiting");
            assert_eq!(read.roster, team.roster);
            assert_eq!(read.sets.len(), 3);
            for (read, planned) in read.sets.iter().zip(&plan.entry.sets) {
                assert_eq!(read.events.len(), planned.events.len());
            }
            assert!(read.get_status().expect("a status").match_finished);
            assert_eq!(read.source, MatchSource::Dvw);
            assert_eq!(p.queued(), vec![(team.team.id, saved.id)]);
        }

        /// The match exported and imported on another device: still
        /// imported from DataVolley.
        #[tokio::test]
        async fn an_imported_match_stays_one_through_the_archives() {
            let p = Providers::new();
            let (team, plan) =
                planned_for(&three_sets_won().text(), Side::Home, new_team()).expect("a match");
            let saved = p.save(&team, &plan, false).await.expect("saved");
            let team_zip = crate::archive::export_team(&*p.store, &team.team.id)
                .await
                .expect("the team archive");
            let match_zip = crate::archive::export_match(&*p.store, &team.team.id, &saved.id)
                .await
                .expect("the match archive");
            let other = Providers::new();
            let imported_team = crate::archive::import_team(
                &team_zip,
                Some(team.team.id),
                &other.team_reader,
                &other.team_writer,
                crate::tests::EN,
            )
            .await
            .expect("the team imported");
            let imported = crate::archive::import_match(
                &match_zip,
                &imported_team,
                &other.match_reader,
                &other.match_writer,
                &other.set_writer,
                crate::tests::EN,
            )
            .await
            .expect("the match imported");
            let read = other
                .match_reader
                .read_single(&imported_team, &imported.id)
                .await
                .expect("the match");
            assert_eq!(read.source, MatchSource::Dvw);
        }

        /// The match saved already and the user not confirming: nothing
        /// saved, the team neither, nothing queued.
        #[tokio::test]
        async fn a_match_saved_already_is_kept_unless_confirmed() {
            let p = Providers::new();
            let text = three_sets_won().text();
            let (team, plan) = planned_for(&text, Side::Home, new_team()).expect("a match");
            p.save(&team, &plan, false).await.expect("saved");
            let saved_team = p
                .team_reader
                .read_single(&team.team.id)
                .await
                .expect("the team");
            assert!(
                match_exists(&plan, &p.match_reader)
                    .await
                    .expect("an answer")
            );
            let dvw = parse(text.as_bytes()).expect("a file");
            let again =
                crate::dvw::import::team_plan(&dvw, Side::Home, TeamChoice::Existing(&saved_team))
                    .expect("a team");
            let mut more = again.clone();
            more.team.players.push(crate::shapes::player::PlayerEntry {
                name: "Anna Huber".to_string(),
                number: 30,
                ..Default::default()
            });
            assert!(matches!(
                p.save(&more, &plan, false).await,
                Err(AppError::Match(MatchError::MatchAlreadyExists(_)))
            ));
            let kept = p
                .team_reader
                .read_single(&team.team.id)
                .await
                .expect("the team");
            assert_eq!(kept.players.len(), 8);
            assert_eq!(p.queued().len(), 1);
        }

        /// Confirmed: the match in place of the saved one, its sets beyond
        /// the imported ones removed.
        #[tokio::test]
        async fn a_match_overwritten_when_confirmed() {
            let p = Providers::new();
            let (team, five) =
                planned_for(&five_sets().text(), Side::Home, new_team()).expect("a match");
            let saved = p.save(&team, &five, false).await.expect("saved");
            let saved_team = p
                .team_reader
                .read_single(&team.team.id)
                .await
                .expect("the team");
            let (again, three) = planned_for(
                &three_sets_won().text(),
                Side::Home,
                TeamChoice::Existing(&saved_team),
            )
            .expect("a match");
            assert_eq!(three.entry.id, saved.id);
            p.save(&again, &three, true).await.expect("overwritten");
            let read = p
                .match_reader
                .read_single(&saved_team, &saved.id)
                .await
                .expect("the match");
            assert_eq!(read.sets.len(), 3);
            let status = read.get_status().expect("a status");
            assert_eq!((status.us_wins, status.them_wins), (3, 0));
            assert_eq!(read.source, MatchSource::Dvw);
            for set_number in [4, 5] {
                for key in [
                    layout::set_descriptor(&saved_team.id, &saved.id, set_number),
                    layout::set_events(&saved_team.id, &saved.id, set_number),
                ] {
                    assert!(!p.store.exists(&key).await.expect("an answer"), "{key}");
                }
            }
            assert_eq!(p.queued().len(), 2);
        }
    }
}
