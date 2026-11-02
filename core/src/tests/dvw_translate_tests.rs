//! A set of DataVolley translated for one team: files written here rally
//! by rally, the six on court of both teams kept as DataVolley writes them
//! (the libero aside, the player it replaces in its place).

#[cfg(test)]
mod tests {
    use crate::dvw::{
        model::Side,
        parse,
        translate::{Issue, Us, translate_set},
    };
    use crate::shapes::enums::{EvalEnum, EventTypeEnum};
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
            while self.score.0 < 25 {
                match self.home_serves {
                    true => self.ace(),
                    false => self.their_serve_error(),
                }
            }
            self.lines.push("**1set".to_string());
            self
        }

        fn text(&self) -> String {
            let player = |team: u8, n: u8, role: u8| {
                let libero = if role == 1 { "L" } else { "" };
                format!("{team};{n};{n};*;;;;;P{n};Surname{n};Name{n};;{libero};{role};False;;;")
            };
            let roles = [5, 2, 4, 3, 2, 4, 1];
            let players = |team: u8, first: u8| {
                (0..7u8)
                    .map(|i| player(team, first + i, roles[usize::from(i)]))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            [
                "[3DATAVOLLEYSCOUT]\nFILEFORMAT: 2.0\nGENERATOR-PRG: Data Volley".to_string(),
                "[3MATCH]\n01/25/2015;;2014/2015;Test;;;;;1;1;Z;0;".to_string(),
                "[3TEAMS]\nH;Home;1;;;;\nV;Visiting;0;;;;".to_string(),
                "[3SET]\nTrue;;;;25-0;;\nTrue;;;;;;\nTrue;;;;;;\nTrue;;;;;;\nTrue;;;;;;"
                    .to_string(),
                format!("[3PLAYERS-H]\n{}", players(0, 1)),
                format!("[3PLAYERS-V]\n{}", players(1, 11)),
                format!("[3SCOUT]\n{}", self.lines.join("\n")),
            ]
            .join("\n")
        }
    }

    /// The set translated for the home team: the events, by type and
    /// player number.
    fn translated(file: &File) -> Result<Vec<(EventTypeEnum, Option<u8>)>, Issue> {
        let dvw = parse(file.text().as_bytes()).expect("expected a DataVolley file");
        let players: HashMap<u8, Uuid> = (1..=7).map(|n| (n, Uuid::new_v4())).collect();
        let numbers: HashMap<Uuid, u8> = players.iter().map(|(n, id)| (*id, *n)).collect();
        let us = Us {
            side: Side::Home,
            players: &players,
            liberos: &[HOME_LIBERO],
        };
        let date = NaiveDate::from_ymd_opt(2015, 1, 25).expect("a date");
        translate_set(&dvw, &us, 1, date, 6)
            .map(|(set, _)| {
                set.events
                    .iter()
                    .map(|e| {
                        (
                            e.event_type,
                            e.player.and_then(|p| numbers.get(&p).copied()),
                        )
                    })
                    .collect()
            })
            .map_err(|e| e.issue)
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
            Err(Issue::Untranslatable(
                "the libero for more than one player: who is out can't be told"
            ))
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
            Err(Issue::Untranslatable(
                "the libero can't change player in time"
            ))
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
            Err(Issue::Untranslatable(
                "the libero can't change player in time"
            ))
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
            Err(Issue::Untranslatable("the libero has nobody to replace"))
        );
    }

    /// The server of the file not in zone 1 by the rules: refused.
    #[test]
    fn a_server_not_in_zone_1_is_refused() {
        let mut file = File::new();
        file.their_serve_error();
        file.rally(&["*03SM#"], true);
        match translated(&file.finished()) {
            Err(Issue::Event(text)) => assert!(text.contains("can't play it"), "{text}"),
            other => panic!("expected the serve refused, got {other:?}"),
        }
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
        let players: HashMap<u8, Uuid> = (1..=7).map(|n| (n, Uuid::new_v4())).collect();
        let us = Us {
            side: Side::Home,
            players: &players,
            liberos: &[HOME_LIBERO],
        };
        let date = NaiveDate::from_ymd_opt(2015, 1, 25).expect("a date");
        let (set, _) = translate_set(&dvw, &us, 1, date, 6).expect("the set");
        assert_eq!(set.events[0].eval, Some(EvalEnum::Positive));
    }
}
