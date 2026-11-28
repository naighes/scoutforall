//! A set of DataVolley translated for one team: files written here rally
//! by rally, the six on court of both teams kept as DataVolley writes them
//! (the libero aside, the player it replaces in its place).

#[cfg(test)]
mod tests {
    use crate::dvw::{
        model::{Side, Skill},
        parse,
        translate::{Impossible, Issue, Untranslatable, Us, translate_set},
    };
    use crate::shapes::court_zone::{CourtTarget, SubzoneEnum};
    use crate::shapes::enums::{EvalEnum, EventTypeEnum};
    use crate::shapes::refusal::Refusal;
    use crate::shapes::snapshot::EventEntry;
    use crate::shapes::technique::{SetTypeEnum, TechniqueEnum};
    use EventTypeEnum::*;
    use chrono::{DateTime, FixedOffset, NaiveDate, NaiveTime, Utc};
    use std::collections::HashMap;
    use uuid::Uuid;

    /// The home team is ours: the setter 1, outside hitters 2 and 5,
    /// middle blockers 3 and 6, the opposite 4, the libero 7; the setter in
    /// zone 1, middle blocker 6 in zone 6. The visiting team, 11 to 17.
    const HOME_LIBERO: u8 = 7;

    /// The file being written: the six on court of both teams, P1 first,
    /// the score, who serves.
    #[derive(Clone)]
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
            // a good reception of a weak serve: the two agree (TODO/001,
            // §4.2)
            let serve = format!("{}SM-", self.server());
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
                COMBINATIONS.to_string(),
                format!("[3SCOUT]\n{}", self.lines.join("\n")),
            ]
            .join("\n")
        }
    }

    /// The attack combinations of the files, as DataVolley comes with them
    /// (code, zone, side, tempo, description).
    const COMBINATIONS: &str = "[3ATTACKCOMBINATION]
X1;3;R;Q;Veloce davanti;;;;;
X2;2;L;Q;Veloce dietro;;;;;
X7;4;R;Q;Sette davanti;;;;;
XC;3;R;Q;Veloce spostata;;;;;
XD;3;R;Q;Doppia C;;;;;
XM;3;C;Q;Veloce in punto 3;;;;;
CF;2;L;Q;Fast lontano dal palleggiatore;;;;;
X5;4;R;T;Spinta in posto 4;;;;;
X6;2;L;T;Spinta in posto 2;;;;;
X8;9;C;T;Spinta in posto 1;;;;;
X0;7;C;T;Spinta in posto 5;;;;;
XP;8;C;M;Pipe;;;;;
XT;3;R;M;Mezza da posto 4;;;;;
C5;4;R;U;Super in posto 4;;;;;
V5;4;R;H;Alta in posto 4;;;;;
VP;8;C;H;Pipe alta;;;;;
PP;3;L;O;Pallonetto alzatore;;;;;";

    /// The line in the file (from 1) of the line `index` of the scout.
    fn line_of(file: &File, index: usize) -> usize {
        let text = file.text();
        let scout = text
            .lines()
            .position(|l| l == "[3SCOUT]")
            .unwrap_or_default();
        scout + 2 + index
    }

    /// The set translated for the home team: the events, and the number of
    /// each player.
    fn translated_events(file: &File) -> Result<(Vec<EventEntry>, HashMap<Uuid, u8>), Issue> {
        translated_in(file, 0)
    }

    /// The set translated for the home team, played on 2015-01-25 in the
    /// time zone `east_s` seconds east of UTC.
    fn translated_in(
        file: &File,
        east_s: i32,
    ) -> Result<(Vec<EventEntry>, HashMap<Uuid, u8>), Issue> {
        let dvw = parse(file.text().as_bytes()).expect("expected a DataVolley file");
        let players: HashMap<u8, Uuid> = (1..=8).map(|n| (n, Uuid::new_v4())).collect();
        let numbers: HashMap<Uuid, u8> = players.iter().map(|(n, id)| (*id, *n)).collect();
        let us = Us {
            side: Side::Home,
            players: &players,
            liberos: &[HOME_LIBERO],
        };
        let date = NaiveDate::from_ymd_opt(2015, 1, 25)
            .expect("a date")
            .and_time(NaiveTime::MIN)
            .and_local_timezone(FixedOffset::east_opt(east_s).expect("an offset"))
            .single()
            .expect("a time");
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

    /// The lines with the clock `time` and, when given, `video` seconds
    /// from 100, 10 a line.
    fn timed(mut file: File, time: &str, video: bool) -> File {
        file.lines = file
            .lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let mut fields: Vec<String> = line.split(';').map(str::to_string).collect();
                // the ends of set have no fields
                if let Some(f) = fields.get_mut(7) {
                    *f = time.to_string();
                }
                if let (true, Some(f)) = (video, fields.get_mut(12)) {
                    *f = (100 + 10 * i).to_string();
                }
                fields.join(";")
            })
            .collect();
        file
    }

    /// Linked to the video, the times of the events are its seconds from
    /// midnight, whatever the clock of the file says.
    #[test]
    fn the_events_on_the_video_clock() {
        let mut file = File::new();
        file.received_by(2);
        let file = timed(file.finished(), "23.59.59", true);
        let lines = file.lines.len();
        let (events, _) = translated_events(&file).expect("expected the set");
        let midnight = NaiveDate::from_ymd_opt(2015, 1, 25)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .expect("a time")
            .and_utc();
        let seconds: Vec<i64> = events
            .iter()
            .map(|e| (e.timestamp - midnight).num_seconds())
            .collect();
        assert!(
            seconds
                .iter()
                .all(|s| (100..100 + 10 * lines as i64).contains(s) && s % 10 == 0),
            "{seconds:?}"
        );
        assert!(seconds.windows(2).all(|w| w[0] <= w[1]), "{seconds:?}");
    }

    /// Without the video, the clock of the file, as ever.
    #[test]
    fn the_events_on_the_clock_of_the_file() {
        let mut file = File::new();
        file.received_by(2);
        let file = timed(file.finished(), "18.30.00", false);
        let (events, _) = translated_events(&file).expect("expected the set");
        let start = NaiveDate::from_ymd_opt(2015, 1, 25)
            .and_then(|d| d.and_hms_opt(18, 30, 0))
            .expect("a time")
            .and_utc();
        // the same time on every line: a second after the previous one
        let first = events.first().map(|e| (e.timestamp - start).num_seconds());
        assert!(first.is_some_and(|s| (0..60).contains(&s)), "{first:?}");
    }

    /// The clock of the file is the local time where the match is played
    /// (as the names of the videos): 18:30 in Rome is 17:30 UTC; 08:00 in
    /// Tokyo is the day before in UTC, not after midnight UTC.
    #[test]
    fn the_clock_of_the_file_in_the_time_zone_of_the_match() {
        let mut file = File::new();
        file.received_by(2);
        let at = |time: &str, east_s: i32| {
            let file = timed(file.clone().finished(), time, false);
            let (events, _) = translated_in(&file, east_s).expect("expected the set");
            events.first().map(|e| e.timestamp)
        };
        let utc = |d: u32, h: u32, m: u32| {
            NaiveDate::from_ymd_opt(2015, 1, d)
                .and_then(|d| d.and_hms_opt(h, m, 0))
                .expect("a time")
                .and_utc()
        };
        let near = |t: Option<DateTime<Utc>>, expected: DateTime<Utc>| {
            t.is_some_and(|t| (0..60).contains(&(t - expected).num_seconds()))
        };
        assert!(near(at("18.30.00", 3600), utc(25, 17, 30)));
        assert!(near(at("08.00.00", 9 * 3600), utc(24, 23, 0)));
        assert!(near(at("18.30.00", -5 * 3600), utc(25, 23, 30)));
    }

    /// On the video clock the time zone doesn't count: the seconds from
    /// midnight UTC, as the video reads them (`video::video_midnight`).
    #[test]
    fn the_video_clock_whatever_the_time_zone() {
        let mut file = File::new();
        file.received_by(2);
        let file = timed(file.finished(), "18.30.00", true);
        let times = |east_s| {
            translated_in(&file, east_s)
                .expect("expected the set")
                .0
                .iter()
                .map(|e| e.timestamp)
                .collect::<Vec<_>>()
        };
        assert_eq!(times(3600), times(0));
    }

    /// Both teams: their touches fused into one sequence, each side
    /// replaying it from its point of view with the score of the file.
    #[test]
    fn both_teams_are_one_sequence_of_touches() {
        use crate::domain::Side as Team;
        use crate::dvw::both::translate_both;
        use crate::rally::replay;
        let mut file = File::new();
        file.received_by(2);
        file.ace();
        let dvw = parse(file.finished().text().as_bytes()).expect("expected a DataVolley file");
        let home: HashMap<u8, Uuid> = (1..=8).map(|n| (n, Uuid::new_v4())).collect();
        let visiting: HashMap<u8, Uuid> = (11..=18).map(|n| (n, Uuid::new_v4())).collect();
        let a = Us {
            side: Side::Home,
            players: &home,
            liberos: &[HOME_LIBERO],
        };
        let b = Us {
            side: Side::Visiting,
            players: &visiting,
            liberos: &[17],
        };
        let date = NaiveDate::from_ymd_opt(2015, 1, 25)
            .expect("a date")
            .and_time(NaiveTime::MIN)
            .and_utc()
            .fixed_offset();
        let (both, _) = translate_both(&dvw, &a, &b, 1, date, 6).expect("expected the set");
        let first: Vec<(Team, EventTypeEnum)> = both
            .touches
            .iter()
            .take(4)
            .map(|(side, e)| (*side, e.event_type))
            .collect();
        assert_eq!(
            first,
            vec![(Team::B, S), (Team::A, P), (Team::A, A), (Team::A, S)]
        );
        let (ours, theirs) = replay(&both.a, &both.b, &both.touches, 6).expect("both sides");
        assert_eq!((ours.score_us, ours.score_them), (25, 0));
        assert_eq!((theirs.score_us, theirs.score_them), (0, 25));
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
        let date = NaiveDate::from_ymd_opt(2015, 1, 25)
            .expect("a date")
            .and_time(NaiveTime::MIN)
            .and_utc()
            .fixed_offset();
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
    /// files scouted without blocks write it: the block is optional
    /// (TODO/001, §4), their attack blocked is their error (`OE`), as the
    /// projection tells it to the side not scouting the block.
    #[test]
    fn their_attack_blocked_without_our_block_is_their_error() {
        let mut file = File::new();
        file.rally(&["a11SM+", "*07RM+", "*02AH+", "a14AH/", "*$$&H#"], true);
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(
            first(&events, 4),
            vec![(P, Some(7)), (A, Some(2)), (OE, None), (S, Some(2))]
        );
    }

    /// Their attack `-`, then their freeball sent: the ball came back to
    /// them, our touch not scouted (a dig, then a freeball): our freeball
    /// (`FB`), sent untold, then theirs (`OF`).
    #[test]
    fn their_freeball_right_after_their_attack_follows_ours() {
        let mut file = File::new();
        file.rally(
            &[
                "a11SM+", "*07RM+", "*02AH+", "a14AH-", "a13FH+", "*01EH+", "*04AH#",
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

    /// Our serves with where they went: the cell of the zone and its
    /// subzone, the whole zone without the subzone; none when the file
    /// doesn't tell it, nor for the other touches.
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
                (S, CourtTarget::of_zone(4, Some(SubzoneEnum::C))),
                (S, Some(CourtTarget::Zone(6))),
                (S, None),
            ]
        );
    }

    /// Where our serves started from: the first zone of the serve, behind
    /// our end line (1, 9, 6, 7, 5); none when the file doesn't tell it or
    /// tells a zone the serve can't start from.
    #[test]
    fn where_our_serves_started_from() {
        use crate::shapes::court_zone::ServeStartEnum::*;
        let mut file = File::new();
        file.their_serve_error();
        let from = |start: &str| format!("{}SM#~~~{start}", file.server());
        let serves = [from("74C"), from("5"), from("9"), from("8"), from("~6")];
        for serve in &serves {
            file.rally(&[serve], true);
        }
        let file = file.finished();
        let (events, _) = translated_events(&file).expect("expected the set");
        let starts: Vec<_> = events
            .iter()
            .take(6)
            .map(|e| (e.event_type, e.flags.serve_start()))
            .collect();
        assert_eq!(
            starts,
            vec![
                (OE, None),
                (S, Some(Z7)),
                (S, Some(Z5)),
                (S, Some(Z9)),
                (S, None),
                (S, None),
            ]
        );
        // where it went, as before
        assert_eq!(
            events.get(1).and_then(|e| e.flags.target()),
            CourtTarget::of_zone(4, Some(SubzoneEnum::C))
        );
    }

    /// Our receptions with where we received: the cell of the zone and its
    /// subzone (from 5 to 1D), the whole zone without the subzone; none
    /// when the file doesn't tell it.
    #[test]
    fn the_zone_of_our_receptions() {
        let rallies = [
            ["a11SM+~~~51D", "*07RM+~~~51D", "*02AH#"],
            ["a11SM+~~~56", "*07RM+~~~56", "*02AH#"],
            ["a11SM+", "*07RM+", "*02AH#"],
        ];
        let targets: Vec<_> = rallies
            .iter()
            .map(|touches| {
                let mut file = File::new();
                file.rally(touches, true);
                let (events, _) = translated_events(&file.finished()).expect("expected the set");
                events
                    .iter()
                    .find(|e| e.event_type == P)
                    .and_then(|e| e.flags.target())
            })
            .collect();
        assert_eq!(
            targets,
            vec![
                CourtTarget::of_zone(1, Some(SubzoneEnum::D)),
                Some(CourtTarget::Zone(6)),
                None,
            ]
        );
    }

    /// Our attacks with where they went: the cell of the zone and its
    /// subzone (from 4 to 5B), the whole zone without the subzone; none
    /// when the file doesn't tell it.
    #[test]
    fn the_zone_of_our_attacks() {
        let rallies = [
            ["a11SM+", "*07RM+", "*02AH#~~~45B"],
            ["a11SM+", "*07RM+", "*02AH#~~~46"],
            ["a11SM+", "*07RM+", "*02AH#"],
        ];
        let targets: Vec<_> = rallies
            .iter()
            .map(|touches| {
                let mut file = File::new();
                file.rally(touches, true);
                let (events, _) = translated_events(&file.finished()).expect("expected the set");
                events
                    .iter()
                    .find(|e| e.event_type == A)
                    .and_then(|e| e.flags.target())
            })
            .collect();
        assert_eq!(
            targets,
            vec![
                CourtTarget::of_zone(5, Some(SubzoneEnum::B)),
                Some(CourtTarget::Zone(6)),
                None,
            ]
        );
    }

    /// The set of each attack of ours, from its combination as the file
    /// defines it (the quick of the middle blockers by its code), or from
    /// the tempo and zone of the attack when the file doesn't define it;
    /// none for the setter tipping, a quick of another code, an attack
    /// telling nothing.
    #[test]
    fn the_set_of_our_attacks() {
        use SetTypeEnum::*;
        let attacks = [
            ("*03AQ#X1~35", Some(One)),
            ("*03AQ#X2~25", Some(Two)),
            ("*03AQ#X7~45", Some(Seven)),
            ("*03AQ#XC~35", Some(C)),
            ("*03AQ#XD~35", Some(DoubleC)),
            ("*03AQ#XM~35", None),
            ("*03AQ#CF~25", Some(Fast)),
            ("*02AT#X5~45", Some(Five)),
            ("*04AT#X6~25", Some(Six)),
            ("*04AT#X8~95", Some(Eight)),
            ("*02AT#X0~75", None),
            ("*02AM#XP~85", Some(Pipe)),
            ("*02AM#XT~35", Some(Quick)),
            ("*02AU#C5~45", Some(Super)),
            ("*02AH#V5~45", Some(High)),
            ("*02AH#VP~85", Some(High)),
            ("*01AO#PP~35", None),
            // not defined: the tempo and zone of the attack
            ("*02AT#ZZ~45", Some(Five)),
            ("*02AH#~~~45", Some(High)),
            ("*02AT#", None),
        ];
        for (attack, expected) in attacks {
            let mut file = File::new();
            file.rally(&["a11SM+", "*07RM+", attack], true);
            let (events, _) = translated_events(&file.finished()).expect("expected the set");
            let technique = events
                .iter()
                .find(|e| e.event_type == A)
                .and_then(|e| e.technique);
            assert_eq!(technique, expected.map(TechniqueEnum::Set), "{attack}");
        }
    }

    /// Our digs with where we dug their attack: the cell (from 4 to 1C),
    /// copied from their attack; none when the file doesn't tell it.
    #[test]
    fn the_zone_of_our_digs() {
        let rallies = [
            ["a07RM+", "a02AH+~~~41C", "*05DH+~~~41C", "*02AH#"],
            ["a07RM+", "a02AH+", "*05DH+", "*02AH#"],
        ];
        let targets: Vec<_> = rallies
            .iter()
            .map(|touches| {
                let mut file = File::new();
                file.their_serve_error();
                let serve = format!("{}SM+", file.server());
                let touches: Vec<&str> = std::iter::once(serve.as_str())
                    .chain(touches.iter().copied())
                    .collect();
                file.rally(&touches, true);
                let (events, _) = translated_events(&file.finished()).expect("expected the set");
                events
                    .iter()
                    .find(|e| e.event_type == D)
                    .and_then(|e| e.flags.target())
            })
            .collect();
        assert_eq!(
            targets,
            vec![CourtTarget::of_zone(1, Some(SubzoneEnum::C)), None]
        );
    }

    /// How many blocked: on our block, how many of ours; on our attack,
    /// how many of theirs (none too, the hole block); not told, none.
    #[test]
    fn how_many_blocked() {
        use crate::shapes::flags::BlockersEnum;
        let rallies = [
            [
                "a07RM+",
                "a02AH+~~~47~H2",
                "*05BH+~~~~2~~2",
                "*02AH#~~~45BH0",
            ],
            [
                "a07RM+",
                "a02AH+~~~47~H2",
                "*05BH+~~~~2~~4",
                "*02AH#~~~45BH3",
            ],
            ["a07RM+", "a02AH+", "*05BH+", "*02AH#"],
        ];
        let told: Vec<_> = rallies
            .iter()
            .map(|touches| {
                let mut file = File::new();
                file.their_serve_error();
                let serve = format!("{}SM+", file.server());
                let touches: Vec<&str> = std::iter::once(serve.as_str())
                    .chain(touches.iter().copied())
                    .collect();
                file.rally(&touches, true);
                let (events, _) = translated_events(&file.finished()).expect("expected the set");
                let of = |event_type| {
                    events
                        .iter()
                        .find(|e| e.event_type == event_type)
                        .and_then(|e| e.flags.blockers())
                };
                (of(B), of(A))
            })
            .collect();
        assert_eq!(
            told,
            vec![
                (Some(BlockersEnum::Two), Some(BlockersEnum::NoBlock)),
                (Some(BlockersEnum::Hole), Some(BlockersEnum::Three)),
                (None, None),
            ]
        );
    }

    /// Our blocks with where we blocked their attack: the zone at the net,
    /// in front of it (their attack from 4, our block in 2); none for
    /// another zone or when the file doesn't tell it.
    #[test]
    fn the_zone_of_our_blocks() {
        let rallies = [
            ["a07RM+", "a02AH+~~~47", "*05BH+~~~~2", "*02AH#"],
            ["a07RM+", "a02AH+~~~47", "*05BH+~~~~8", "*02AH#"],
            ["a07RM+", "a02AH+", "*05BH+", "*02AH#"],
        ];
        let targets: Vec<_> = rallies
            .iter()
            .map(|touches| {
                let mut file = File::new();
                file.their_serve_error();
                let serve = format!("{}SM+", file.server());
                let touches: Vec<&str> = std::iter::once(serve.as_str())
                    .chain(touches.iter().copied())
                    .collect();
                file.rally(&touches, true);
                let (events, _) = translated_events(&file.finished()).expect("expected the set");
                events
                    .iter()
                    .find(|e| e.event_type == B)
                    .map(|e| e.flags.target())
            })
            .collect();
        assert_eq!(
            targets,
            vec![Some(Some(CourtTarget::Zone(2))), Some(None), Some(None)]
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

    /// Our serve, their reception and set, our block: their attack is
    /// missing, an error of the scout (TODO/001, §10: the match doesn't
    /// import), told at the line of the block with the set before it.
    #[test]
    fn our_block_after_their_reception_without_their_attack_is_refused() {
        let mut file = File::new();
        file.their_serve_error();
        let serve = format!("{}SM-", file.server());
        file.rally(&[&serve, "a11RM+", "a13EH+", "*03BH="], false);
        // the set, before the block and the point
        let set = file.lines.len() - 3;
        let file = file.finished();
        assert_eq!(
            evaluated(&file),
            Err(Issue::Impossible(Impossible::MissingAttack {
                before: (Skill::Set, '+'),
                before_line: line_of(&file, set),
                touch: (Skill::Block, '='),
            }))
        );
    }

    /// Our attack `-` back from their block `+`, our dig: we play next, so
    /// their block is `!` and our attack `!` (blocked and covered), no
    /// freeball of theirs; our dig is our cover, `+` rebuilding (`!`).
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
            events.iter().take(4).copied().collect::<Vec<_>>(),
            vec![
                (P, Some(EvalEnum::Positive)),
                (A, Some(EvalEnum::Exclamative)),
                (D, Some(EvalEnum::Exclamative)),
                (A, Some(EvalEnum::Perfect))
            ]
        );
    }

    /// Our dig `!` of their attack: the dig `!` is only a cover, DataVolley's
    /// `!` lets us build: our `+`.
    #[test]
    fn our_dig_exclamative_of_their_attack_is_positive() {
        let mut file = File::new();
        file.rally(
            &[
                "a11SM+", "*07RM+", "*02AH-", "a12DH+", "a13AH+", "*07DH!", "*04AH#",
            ],
            true,
        );
        let events = evaluated(&file.finished()).expect("expected the set");
        assert_eq!(
            events.iter().take(4).copied().collect::<Vec<_>>(),
            vec![
                (P, Some(EvalEnum::Positive)),
                (A, Some(EvalEnum::Negative)),
                (D, Some(EvalEnum::Positive)),
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

    /// Our cover after their block, their attack next: the cover (`!`),
    /// then our freeball sent untold.
    #[test]
    fn our_cover_played_by_them_is_our_freeball() {
        let mut file = File::new();
        file.rally(
            &["a11SM+", "*07RM+", "*02AH-", "a13BH+", "*07DH+", "a14AH#"],
            false,
        );
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(
            first(&events, 5),
            vec![
                (P, Some(7)),
                (A, Some(2)),
                (D, Some(7)),
                (FB, None),
                (OS, None)
            ]
        );
    }

    /// Our error and their point, both without a player (`$$&`), the ball
    /// ours after our cover: their point without a cause (`PL`).
    #[test]
    fn their_point_without_a_touch_with_the_ball_ours() {
        let mut file = File::new();
        file.rally(
            &[
                "a11SM+", "*07RM+", "*02AH-", "a13BH+", "*07DH-", "*$$&H=", "a$$&H#",
            ],
            false,
        );
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(
            first(&events, 4),
            vec![(P, Some(7)), (A, Some(2)), (D, Some(7)), (PL, None)]
        );
    }

    /// Our block on their reception straight over and on their freeball:
    /// the block answers an attack (scouting rules v4), so the point is an
    /// attack point, the error an attack error.
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
                (A, Some(EvalEnum::Perfect)),
                (S, Some(EvalEnum::Negative)),
                (OF, None),
                (A, Some(EvalEnum::Error))
            ]
        );
    }

    /// Our block `+` of their freeball and our dig after it: the ball
    /// kept, like a freeball received, which isn't told (scouting rules
    /// v4): both left out, our attack after their freeball.
    #[test]
    fn our_block_kept_on_their_freeball_is_left_out_with_our_dig() {
        let mut file = File::new();
        file.their_serve_error();
        let serve = format!("{}SM-", file.server());
        file.rally(
            &[
                &serve, "a11RM+", "a13EH+", "a14FH+", "*03BH+", "*05DH+", "*04AH#",
            ],
            true,
        );
        let events = evaluated(&file.finished()).expect("expected the set");
        assert_eq!(
            events.iter().take(4).copied().collect::<Vec<_>>(),
            vec![
                (OE, None),
                (S, Some(EvalEnum::Negative)),
                (OF, None),
                (A, Some(EvalEnum::Perfect)),
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
    /// block is optional (TODO/001, §4), the ball theirs after their attack
    /// `!`: their freeball (`OF`), nothing of ours.
    #[test]
    fn their_freeball_after_their_attack_back_from_our_block_untold() {
        let mut file = File::new();
        file.rally(
            &["a11SM+", "*07RM+", "*02AH+", "a14AH!", "a13FH+", "*04AH#"],
            true,
        );
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(
            first(&events, 4),
            vec![(P, Some(7)), (A, Some(2)), (OF, None), (A, Some(4))]
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
        let date = NaiveDate::from_ymd_opt(2015, 1, 25)
            .expect("a date")
            .and_time(NaiveTime::MIN)
            .and_utc()
            .fixed_offset();
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
        use crate::dvw::import::{
            OpponentChoice, match_date, match_plan, opponent_plan, team_plan,
        };
        let dvw = parse(text.as_bytes()).expect("expected a DataVolley file");
        let team = team_plan(&dvw, side, choice).expect("a team");
        let opponent = opponent_plan(&dvw, side, &team.team, OpponentChoice::Name).expect("team B");
        let day = NaiveDate::from_ymd_opt(2015, 1, 25).expect("a date");
        let offset = chrono::FixedOffset::east_opt(3600).expect("an offset");
        let labels = crate::localization::Language::default().labels();
        match_plan(
            &dvw,
            side,
            &team,
            &opponent,
            match_date(&dvw, day, offset),
            6,
            labels,
        )
        .map(|m| (team, m))
    }

    /// The teams and the match of a side, team B as chosen; the file of
    /// the league "Serie C Maschile", first leg, day 1.
    fn planned_against(
        text: &str,
        side: Side,
        choice: crate::dvw::import::TeamChoice,
        opponent: crate::dvw::import::OpponentChoice,
    ) -> (
        crate::dvw::import::TeamPlan,
        crate::dvw::import::OpponentPlan,
        crate::dvw::import::MatchPlan,
    ) {
        use crate::dvw::import::{match_date, match_plan, opponent_plan, team_plan};
        let text = text.replace(
            "01/25/2015;;2014/2015;Test;;;;",
            "01/25/2015;;2014/2015;Serie C Maschile;Andata;;1;",
        );
        let dvw = parse(text.as_bytes()).expect("expected a DataVolley file");
        let team = team_plan(&dvw, side, choice).expect("a team");
        let b = opponent_plan(&dvw, side, &team.team, opponent).expect("team B");
        let day = NaiveDate::from_ymd_opt(2015, 1, 25).expect("a date");
        let offset = chrono::FixedOffset::east_opt(3600).expect("an offset");
        let labels = crate::localization::labels(crate::shapes::enums::LanguageEnum::It);
        let plan = match_plan(
            &dvw,
            side,
            &team,
            &b,
            match_date(&dvw, day, offset),
            6,
            labels,
        )
        .expect("a match");
        (team, b, plan)
    }

    // the rules of the import (TODO/001, §10, phase 2), on both sides:
    // the touches saved replay on both, so the two translations agree

    /// The players of the visiting team, 11 to 18, the libero 17.
    const VISITING_LIBERO: u8 = 17;

    /// A touch saved on both sides: the team (A home, B visiting), the
    /// event, the evaluation.
    type Saved = (Team, EventTypeEnum, Option<EvalEnum>);

    /// The first set on both sides, A the home team: the touches saved,
    /// without the changes of the lineup, or why not.
    fn both(file: &File) -> Result<Vec<Saved>, Issue> {
        use crate::dvw::both::translate_both;
        let dvw = parse(file.text().as_bytes()).expect("expected a DataVolley file");
        let home: HashMap<u8, Uuid> = (1..=8).map(|n| (n, Uuid::new_v4())).collect();
        let visiting: HashMap<u8, Uuid> = (11..=18).map(|n| (n, Uuid::new_v4())).collect();
        let a = Us {
            side: Side::Home,
            players: &home,
            liberos: &[HOME_LIBERO],
        };
        let b = Us {
            side: Side::Visiting,
            players: &visiting,
            liberos: &[VISITING_LIBERO],
        };
        let date = NaiveDate::from_ymd_opt(2015, 1, 25)
            .expect("a date")
            .and_time(NaiveTime::MIN)
            .and_utc()
            .fixed_offset();
        translate_both(&dvw, &a, &b, 1, date, 6)
            .map(|(set, _)| {
                set.touches
                    .iter()
                    .filter(|(_, e)| !matches!(e.event_type, R | CL | LR | LF | CS))
                    .map(|(side, e)| (*side, e.event_type, e.eval))
                    .collect()
            })
            .map_err(|e| e.issue)
    }

    /// The first set translated for the visiting team alone: the events by
    /// type and evaluation, without the changes of the lineup.
    fn evaluated_for_visiting(
        file: &File,
    ) -> Result<Vec<(EventTypeEnum, Option<EvalEnum>)>, Issue> {
        let dvw = parse(file.text().as_bytes()).expect("expected a DataVolley file");
        let players: HashMap<u8, Uuid> = (11..=18).map(|n| (n, Uuid::new_v4())).collect();
        let us = Us {
            side: Side::Visiting,
            players: &players,
            liberos: &[VISITING_LIBERO],
        };
        let date = NaiveDate::from_ymd_opt(2015, 1, 25)
            .expect("a date")
            .and_time(NaiveTime::MIN)
            .and_utc()
            .fixed_offset();
        translate_set(&dvw, &us, 1, date, 6)
            .map(|(set, _)| {
                set.events
                    .iter()
                    .filter(|e| !matches!(e.event_type, R | CL | LR | LF | CS))
                    .map(|e| (e.event_type, e.eval))
                    .collect()
            })
            .map_err(|e| e.issue)
    }

    /// The touches of the first rally of a file written with `rally`, on
    /// both sides, up to `count`.
    fn first_rally(touches: &[&str], home_wins: bool, count: usize) -> Result<Vec<Saved>, Issue> {
        let mut file = File::new();
        file.rally(touches, home_wins);
        both(&file.finished()).map(|t| t.into_iter().take(count).collect())
    }

    /// The events of the first rally of a file written with `rally`, for
    /// one team alone (the home team, then the visiting one), up to
    /// `count` each.
    fn first_rally_alone(
        touches: &[&str],
        home_wins: bool,
        count: usize,
    ) -> Result<
        (
            Vec<(EventTypeEnum, Option<EvalEnum>)>,
            Vec<(EventTypeEnum, Option<EvalEnum>)>,
        ),
        Issue,
    > {
        let mut file = File::new();
        file.rally(touches, home_wins);
        let file = file.finished();
        let take = |events: Vec<(EventTypeEnum, Option<EvalEnum>)>| -> Vec<_> {
            events
                .into_iter()
                .filter(|(e, _)| !matches!(e, R | CL | LR | LF | CS))
                .take(count)
                .collect()
        };
        Ok((
            take(evaluated(&file)?),
            take(evaluated_for_visiting(&file)?),
        ))
    }

    use crate::domain::Side as Team;
    use EvalEnum::{
        Error as E, Exclamative as X, Negative as M, Over as O, Perfect as H, Positive as G,
    };

    /// The reception decides the serve: `S! R+` is `S-` (the visitors'),
    /// `R+` (ours), on both sides and for each team alone.
    #[test]
    fn the_reception_decides_the_serve_on_both_sides() {
        let rally = ["a11SM!", "*07RM+", "*02AH#"];
        assert_eq!(
            first_rally(&rally, true, 3),
            Ok(vec![
                (Team::B, S, Some(M)),
                (Team::A, P, Some(G)),
                (Team::A, A, Some(H))
            ])
        );
        assert_eq!(
            first_rally_alone(&rally, true, 3),
            Ok((
                vec![(P, Some(G)), (A, Some(H)), (S, Some(H))],
                // their serve `-`, our kill, our ace
                vec![(S, Some(M)), (OS, None), (OS, None)]
            ))
        );
    }

    /// The dig decides the attack: `A+ D+` is `A-`; a dig `!` of the
    /// attack is `+`.
    #[test]
    fn the_dig_decides_the_attack_on_both_sides() {
        for dig in ["a13DH+", "a13DH!"] {
            assert_eq!(
                first_rally(&["a11SM-", "*07RM+", "*02AH+", dig, "a14AH#"], false, 5),
                Ok(vec![
                    (Team::B, S, Some(M)),
                    (Team::A, P, Some(G)),
                    (Team::A, A, Some(M)),
                    (Team::B, D, Some(G)),
                    (Team::B, A, Some(H))
                ]),
                "{dig}"
            );
        }
    }

    /// The block decides the attack: its error is the attack point ("mani
    /// fuori"), told by both teams.
    #[test]
    fn the_block_error_is_the_attack_point_on_both_sides() {
        assert_eq!(
            first_rally(&["a11SM-", "*07RM+", "*02AH+", "a13BH="], true, 4),
            Ok(vec![
                (Team::B, S, Some(M)),
                (Team::A, P, Some(G)),
                (Team::A, A, Some(H)),
                (Team::B, B, Some(E))
            ])
        );
    }

    /// After the invasion the attack is left out (163 times in the files,
    /// `A# B/`, `A+ B/`): only the invasion, a fault of B.
    #[test]
    fn the_invasion_leaves_out_the_attack_on_both_sides() {
        for attack in ["*02AH#", "*02AH+"] {
            assert_eq!(
                first_rally(&["a11SM-", "*07RM+", attack, "a13BH/"], true, 3),
                Ok(vec![
                    (Team::B, S, Some(M)),
                    (Team::A, P, Some(G)),
                    (Team::B, B, Some(O))
                ]),
                "{attack}"
            );
        }
    }

    /// Who plays after the block decides it: we, the attackers, play next,
    /// their block `!`, our attack `!` and our cover; they play next with
    /// their dig, which decides it (`-`: their block `-`, our attack `+`).
    #[test]
    fn who_plays_after_the_block_decides_it_on_both_sides() {
        assert_eq!(
            first_rally(
                &["a11SM-", "*07RM+", "*02AH-", "a13BH+", "*07DH+", "*04AH#"],
                true,
                6
            ),
            Ok(vec![
                (Team::B, S, Some(M)),
                (Team::A, P, Some(G)),
                (Team::A, A, Some(X)),
                (Team::B, B, Some(X)),
                (Team::A, D, Some(X)),
                (Team::A, A, Some(H))
            ])
        );
        assert_eq!(
            first_rally(
                &["a11SM-", "*07RM+", "*02AH-", "a13BH+", "a15DH-", "a14AH#"],
                false,
                6
            ),
            Ok(vec![
                (Team::B, S, Some(M)),
                (Team::A, P, Some(G)),
                (Team::A, A, Some(G)),
                (Team::B, B, Some(M)),
                (Team::B, D, Some(M)),
                (Team::B, A, Some(H))
            ])
        );
    }

    /// A freeball received is the freeball the other team sends
    /// (`&WIL-23.dvw` `*07AH- a07BH- *12FH+`): our attack `-`, their block
    /// `+` (they played next), their freeball; ours received is told by
    /// it, never saved.
    #[test]
    fn the_freeball_received_is_the_freeball_sent_on_both_sides() {
        assert_eq!(
            first_rally(
                &["a11SM-", "*07RM+", "*02AH-", "a13BH-", "*05FH+", "*04AH#"],
                true,
                6
            ),
            Ok(vec![
                (Team::B, S, Some(M)),
                (Team::A, P, Some(G)),
                (Team::A, A, Some(M)),
                (Team::B, B, Some(G)),
                (Team::B, FB, None),
                (Team::A, A, Some(H))
            ])
        );
    }

    /// The reception keeping the ball, then the team who served plays
    /// (`&MIL-11.dvw` `*08SM+ a16RM- *06FH#`, `&MIL-23.dvw` `*09SQ+ a02RQ-
    /// *13EH+`): the freeball the receivers sent, untold; the reception
    /// keeps its evaluation (not "over" any more).
    #[test]
    fn the_freeball_after_the_reception_on_both_sides() {
        for then in ["a12FH#", "a13EH+"] {
            assert_eq!(
                first_rally(&["a11SM+", "*07RM-", then, "a14AH#"], false, 4),
                Ok(vec![
                    (Team::B, S, Some(G)),
                    (Team::A, P, Some(M)),
                    (Team::A, FB, None),
                    (Team::B, A, Some(H))
                ]),
                "{then}"
            );
            assert_eq!(
                first_rally_alone(&["a11SM+", "*07RM-", then, "a14AH#"], false, 3),
                Ok((
                    vec![(P, Some(M)), (FB, None), (OS, None)],
                    vec![(S, Some(G)), (OF, None), (A, Some(H))]
                )),
                "{then}"
            );
        }
    }

    /// The freeball B sends untold is at the line of our touch after it,
    /// before it on both sides: our serve, their reception keeping the
    /// ball, our kill.
    #[test]
    fn the_freeball_untold_comes_before_the_touch_at_its_line() {
        let mut file = File::new();
        file.their_serve_error();
        let serve = format!("{}SM+", file.server());
        file.rally(&[&serve, "a12RM-", "*04AH#"], true);
        let touches = both(&file.finished()).expect("the set on both sides");
        assert_eq!(
            touches.get(1..5),
            Some(
                &[
                    (Team::A, S, Some(G)),
                    (Team::B, P, Some(M)),
                    (Team::B, FB, None),
                    (Team::A, A, Some(H))
                ][..]
            ),
            "{touches:?}"
        );
    }

    /// Their freeball received with an error after their own attack `+`
    /// (`&WIL-16.dvw` `a07AH+ a14FH= *$$&H#`): our freeball sent untold,
    /// their error; the point without a touch after it tells nothing more,
    /// and our freeball isn't told twice.
    #[test]
    fn a_freeball_received_with_an_error_on_both_sides() {
        assert_eq!(
            first_rally(
                &["a11SM-", "*07RM+", "*02AH+", "*05FH=", "a$$&H#"],
                false,
                5
            ),
            Ok(vec![
                (Team::B, S, Some(M)),
                (Team::A, P, Some(G)),
                (Team::A, A, Some(G)),
                (Team::B, FB, None),
                (Team::A, F, None)
            ])
        );
    }

    /// Our freeball sent untold, then our invasion (`&WIL-15.dvw`): their
    /// freeball received is told by ours, never once more after the end.
    #[test]
    fn the_freeball_untold_then_an_invasion_on_both_sides() {
        assert_eq!(
            first_rally(&["a11SM-", "*07RM#", "a13EH+", "*03BH/"], false, 5),
            Ok(vec![
                (Team::B, S, Some(M)),
                (Team::A, P, Some(H)),
                (Team::A, FB, None),
                (Team::A, B, Some(O)),
                // their next serve, an error of `finished`
                (Team::B, S, Some(E))
            ])
        );
    }

    /// Our reception over, then our freeball with an error at the end
    /// (`&MIL-27.dvw` `a11SQ/ *18RQ/ *20FH= a$$&H#`): they sent it back,
    /// untold, and we missed it: no touch after our reception over.
    #[test]
    fn our_freeball_error_after_our_reception_over_on_both_sides() {
        assert_eq!(
            first_rally(&["a11SM/", "*07RM/", "*05FH=", "a$$&H#"], false, 4),
            Ok(vec![
                (Team::B, S, Some(O)),
                (Team::A, P, Some(O)),
                (Team::B, FB, None),
                (Team::A, F, None)
            ])
        );
    }

    /// Our set over the net (`/`, `&MIL-30.dvw` `a17EH/ *09FH+`): our
    /// freeball; their freeball received tells nothing more.
    #[test]
    fn our_set_over_is_our_freeball() {
        assert_eq!(
            first_rally_alone(
                &["a11SM-", "*07RM+", "*01EH/", "a12FH+", "a14AH#"],
                false,
                3
            ),
            Ok((
                vec![(P, Some(G)), (FB, None), (OS, None)],
                vec![(S, Some(M)), (OF, None), (A, Some(H))]
            ))
        );
    }

    /// Our invasion with the ball ours (`&MIL-29.dvw` `a05AT+ *01DT-
    /// *06BH/`): no block now, our fault.
    #[test]
    fn our_invasion_with_the_ball_ours_is_our_fault() {
        let mut file = File::new();
        file.rally(
            &[
                "a11SM-", "*07RM+", "*02AH-", "a13DH+", "a14AH+", "*05DH-", "*04BH/",
            ],
            false,
        );
        let events = translated(&file.finished()).expect("expected the set");
        assert_eq!(
            first(&events, 4),
            vec![(P, Some(7)), (A, Some(2)), (D, Some(5)), (F, Some(4))]
        );
    }

    /// Our dig of their reception back over after our serve `/`
    /// (`&MIL-33.dvw` `*02SQ/ a05RQ/ *11DT+ *06AQ#`): no attack, our first
    /// touch isn't scouted: in the serve.
    #[test]
    fn our_dig_after_our_serve_over_is_in_it() {
        let mut file = File::new();
        file.their_serve_error();
        let serve = format!("{}SM/", file.server());
        file.rally(&[&serve, "a12RM/", "*01DH+", "*04AH#"], true);
        let events = evaluated(&file.finished()).expect("expected the set");
        assert_eq!(events.get(1..3), Some(&[(S, Some(O)), (A, Some(H))][..]));
    }

    /// The serve `/`, the reception `/` and the freeball the servers
    /// receive (`&MIL-10.dvw` `*11SQ/ a04RQ/ *04FH+`): `S/`, `P/`, no
    /// freeball of its own.
    #[test]
    fn the_reception_over_and_the_freeball_back_on_both_sides() {
        assert_eq!(
            first_rally(&["a11SM/", "*07RM/", "a12FH+", "a14AH#"], false, 3),
            Ok(vec![
                (Team::B, S, Some(O)),
                (Team::A, P, Some(O)),
                (Team::B, A, Some(H))
            ])
        );
    }

    /// A point without a touch (`$$&`) is the point without a cause, one
    /// touch saved (the other team's told by the projection): `PL` ours
    /// after our error, `PW` after theirs; for each team alone its own.
    #[test]
    fn a_point_without_a_touch_is_one_point_without_a_cause() {
        assert_eq!(
            first_rally(&["a11SM-", "*07RM+", "*$$&H="], false, 3),
            Ok(vec![
                (Team::B, S, Some(M)),
                (Team::A, P, Some(G)),
                (Team::A, PL, None)
            ])
        );
        let after_their_error =
            first_rally(&["a11SM-", "*07RM+", "a$$&H="], true, 4).expect("the set on both sides");
        assert_eq!(
            after_their_error.get(..3),
            Some(
                &[
                    (Team::B, S, Some(M)),
                    (Team::A, P, Some(G)),
                    (Team::A, PW, None)
                ][..]
            )
        );
        assert!(
            !after_their_error.contains(&(Team::B, PL, None)),
            "{after_their_error:?}"
        );
        assert_eq!(
            first_rally_alone(&["a11SM-", "*07RM+", "*$$&H="], false, 2),
            Ok((
                vec![(P, Some(G)), (PL, None)],
                vec![(S, Some(M)), (PW, None)]
            ))
        );
    }

    /// The errors of the scout stop the set on both sides and for each team
    /// alone, at the line of the touch: a touch after the team's own
    /// reception over, a block without an attack after the reception.
    #[test]
    fn the_errors_of_the_scout_stop_the_set_for_both_teams() {
        for rally in [
            vec!["a11SM/", "*07RM/", "*02AH#"],
            vec!["a11SM+", "*07RM!", "a14BH="],
        ] {
            let mut file = File::new();
            file.rally(&rally, true);
            let file = file.finished();
            let issue = both(&file).expect_err("refused on both sides");
            assert!(
                matches!(issue, Issue::Impossible(_)),
                "{rally:?}: {issue:?}"
            );
            assert_eq!(evaluated(&file).err(), Some(issue.clone()), "{rally:?}");
            assert_eq!(
                evaluated_for_visiting(&file).err(),
                Some(issue),
                "{rally:?}"
            );
        }
    }

    /// An error of the scout keeps the whole match out, not even one team's
    /// side imported, with the line, the codes of the file and the advice
    /// to correct it, in every language.
    #[test]
    fn an_error_of_the_scout_keeps_the_match_out() {
        use crate::dvw::import::MatchIssue;
        use crate::shapes::enums::LanguageEnum;
        let mut file = File::new();
        file.rally(&["a11SM/", "*07RM/", "*02AH#"], true);
        // the reception, the attack after it
        let reception = line_of(&file, 3);
        let attack = line_of(&file, 4);
        let text = file
            .finished()
            .next_set()
            .finished()
            .next_set()
            .finished()
            .text();
        for side in [Side::Home, Side::Visiting] {
            let issue = match planned(&text, side) {
                Err(MatchIssue::Set(e)) => e,
                other => panic!("{side:?}: {:?}", other.map(|p| p.sides)),
            };
            assert_eq!(
                (issue.set_number, issue.line, issue.issue.clone()),
                (
                    1,
                    Some(attack),
                    Issue::Impossible(Impossible::TouchAfterOver {
                        over: (Skill::Reception, '/'),
                        over_line: reception,
                        touch: (Skill::Attack, '#'),
                    })
                )
            );
            for language in [
                LanguageEnum::En,
                LanguageEnum::It,
                LanguageEnum::Pt,
                LanguageEnum::Pl,
                LanguageEnum::Ja,
            ] {
                let message =
                    MatchIssue::Set(issue.clone()).describe(crate::localization::labels(language));
                for part in ["R/", "A#", &reception.to_string(), &attack.to_string()] {
                    assert!(message.contains(part), "{language:?}: {message}");
                }
                assert!(!message.contains('{'), "{language:?}: {message}");
            }
        }
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

    /// The P1 inverted, told by where our opposite (4) or outside hitter
    /// (2) attacks from in P1 in side out: the opposite from 2 or the
    /// outside hitter from 4 inverted, from 4 or 2 canonical; with no such
    /// attack (the middle blocker's), canonical. Every event of the set
    /// after it keeps it, as the scouting does.
    #[test]
    fn the_inverted_p1_told_by_the_attacks() {
        let inverted = |attack: &str| -> Vec<bool> {
            let mut file = File::new();
            let serve = format!("{}SM+", file.server());
            file.rally(&[&serve, "*07RM+", attack], true);
            let (events, _) = translated_events(&file.finished()).expect("the set");
            events.iter().map(|e| e.flags.inverted_p1()).collect()
        };
        // the opposite from 2: the reception and the attack of the rally,
        // and all after
        let from_2 = inverted("*04AH#~~~2");
        assert!(from_2.len() > 3);
        assert!(from_2.iter().all(|i| *i), "{from_2:?}");
        // the outside hitter from 4
        assert!(inverted("*02AH#~~~4").iter().all(|i| *i));
        // canonical: the opposite from 4, the outside hitter from 2
        assert!(inverted("*04AH#~~~4").iter().all(|i| !*i));
        assert!(inverted("*02AH#~~~2").iter().all(|i| !*i));
        // the middle blocker, or no start zone: nothing told
        assert!(inverted("*03AH#~~~3").iter().all(|i| !*i));
        assert!(inverted("*04AH#").iter().all(|i| !*i));
    }

    /// The zone of the attack follows the P1 told: the opposite from 2.
    #[test]
    fn the_attack_zone_with_the_inverted_p1() {
        use crate::shapes::{enums::ZoneEnum, set::SetEntry};
        let mut file = File::new();
        let serve = format!("{}SM+", file.server());
        file.rally(&[&serve, "*07RM+", "*04AH#~~~2"], true);
        let dvw = parse(file.finished().text().as_bytes()).expect("a DataVolley file");
        let players: HashMap<u8, Uuid> = (1..=8).map(|n| (n, Uuid::new_v4())).collect();
        let us = Us {
            side: Side::Home,
            players: &players,
            liberos: &[HOME_LIBERO],
        };
        let date = NaiveDate::from_ymd_opt(2015, 1, 25)
            .expect("a date")
            .and_time(NaiveTime::MIN)
            .and_utc()
            .fixed_offset();
        let (set, _): (SetEntry, _) = translate_set(&dvw, &us, 1, date, 6).expect("the set");
        let keys = set.event_keys(6).expect("replayed");
        let zone = keys
            .iter()
            .flatten()
            .find(|k| k.event_type == A)
            .and_then(|k| k.zone);
        assert_eq!(zone, Some(ZoneEnum::Two));
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
    /// import tests of the desktop app, of `mobile` and of the Android app
    /// (`fixtures/three_sets.dvw`).
    #[test]
    #[ignore]
    fn write_three_sets_won() {
        let path = std::env::var("DVW_OUT").expect("DVW_OUT");
        std::fs::write(path, three_sets_won().text()).expect("written");
    }

    /// Saving an import into the database (TODO/001 step 17): the ids of
    /// the matches are UUIDs, the same match is told by where the files
    /// would keep it (opponent and day) or kept it (`legacy_id`).
    mod saving_in_the_database {
        use super::*;
        use crate::{
            analytics::finished::FinishedMatches,
            dvw::import::{
                MatchPlan, OpponentPlan, TeamChoice, TeamPlan, imported_already, save_import,
            },
            errors::{AppError, MatchError},
            localization::Language,
            providers::{
                match_reader::MatchReader,
                sqlite::{
                    Database,
                    matches::{SqliteMatchReader, SqliteMatchWriter},
                    sets::SqliteSetWriter,
                    teams::{SqliteTeamReader, SqliteTeamWriter},
                },
                team_reader::TeamReader,
            },
            shapes::r#match::{MatchEntry, MatchSource},
        };
        use std::sync::Mutex;

        #[derive(Default)]
        struct Queued(Mutex<Vec<(Uuid, String)>>);

        #[async_trait::async_trait]
        impl FinishedMatches for Queued {
            async fn finished(
                &self,
                team_id: Uuid,
                match_id: String,
                _at: chrono::DateTime<chrono::Utc>,
            ) {
                if let Ok(mut queued) = self.0.lock() {
                    queued.push((team_id, match_id));
                }
            }
        }

        struct Providers {
            db: Database,
            team_reader: SqliteTeamReader,
            team_writer: SqliteTeamWriter,
            match_reader: SqliteMatchReader,
            match_writer: SqliteMatchWriter,
            set_writer: SqliteSetWriter,
            queued: Queued,
        }

        impl Providers {
            fn new() -> Self {
                let db = Database::new(crate::db::open_in_memory().expect("a database"));
                Self {
                    team_reader: SqliteTeamReader::new(db.clone()),
                    team_writer: SqliteTeamWriter::new(db.clone()),
                    match_reader: SqliteMatchReader::new(db.clone(), Language::default()),
                    match_writer: SqliteMatchWriter::new(db.clone()),
                    set_writer: SqliteSetWriter::new(db.clone(), Language::default()),
                    queued: Queued::default(),
                    db,
                }
            }

            async fn save(
                &self,
                team: &TeamPlan,
                plan: &MatchPlan,
            ) -> Result<MatchEntry, AppError> {
                let opponent = match plan.input.opponent {
                    crate::match_form::Opponent::Name(ref name) => OpponentPlan::Name(name.clone()),
                    _ => panic!("team B a name"),
                };
                save_import(
                    team,
                    &opponent,
                    plan,
                    &self.team_writer,
                    &self.match_reader,
                    &self.match_writer,
                    &self.set_writer,
                    &self.queued,
                    crate::tests::clock::now(),
                )
                .await
            }

            async fn save_with(
                &self,
                team: &TeamPlan,
                opponent: &OpponentPlan,
                plan: &MatchPlan,
            ) -> Result<MatchEntry, AppError> {
                save_import(
                    team,
                    opponent,
                    plan,
                    &self.team_writer,
                    &self.match_reader,
                    &self.match_writer,
                    &self.set_writer,
                    &self.queued,
                    crate::tests::clock::now(),
                )
                .await
            }

            /// The id of the match the import tells again, if any.
            async fn imported_already(
                &self,
                opponent: &OpponentPlan,
                plan: &MatchPlan,
            ) -> Option<String> {
                imported_already(plan, opponent, &self.match_reader)
                    .await
                    .expect("an answer")
                    .map(|m| m.id)
            }

            fn text(&self, sql: &str) -> Option<String> {
                self.db
                    .read(|tx| Ok(tx.query_row(sql, [], |r| r.get(0)).expect("a row")))
                    .expect("read")
            }

            fn queued(&self) -> Vec<(Uuid, String)> {
                self.queued.0.lock().map(|q| q.clone()).unwrap_or_default()
            }

            fn matches(&self) -> i64 {
                self.sql("SELECT count(*) FROM matches WHERE deleted = 0")
            }

            fn sql(&self, sql: &str) -> i64 {
                self.db
                    .read(|tx| Ok(tx.query_row(sql, [], |r| r.get(0)).expect("a row")))
                    .expect("read")
            }

            fn execute(&self, sql: &str) {
                self.db
                    .write(|tx| Ok(tx.execute(sql, []).expect("written")))
                    .expect("written");
            }

            async fn team(&self, id: &Uuid) -> crate::shapes::team::TeamEntry {
                self.team_reader.read_single(id).await.expect("the team")
            }
        }

        fn new_team() -> TeamChoice<'static> {
            TeamChoice::New {
                classification: Some(crate::shapes::enums::TeamClassificationEnum::Regional),
                gender: Some(crate::shapes::enums::GenderEnum::Women),
                country: crate::shapes::country::CountryCode::parse("AT"),
            }
        }

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
            let saved = p.save(&team, &plan).await.expect("saved");
            let read_team = p.team(&team.team.id).await;
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
            let saved = p.save(&team, &plan).await.expect("saved");
            let id = Uuid::parse_str(&saved.id).expect("a UUID");
            let archive = crate::providers::sqlite::archive::export(
                &p.db,
                crate::providers::sqlite::archive::Subject::Match(id),
                crate::tests::clock::now(),
            )
            .expect("the archive");
            let other = Providers::new();
            crate::providers::sqlite::archive::import(&other.db, &archive, &Language::default())
                .expect("imported");
            let read = other
                .match_reader
                .read_single(&other.team(&team.team.id).await, &saved.id)
                .await
                .expect("the match");
            assert_eq!(read.source, MatchSource::Dvw);
        }

        /// The match imported already: refused, the team isn't saved
        /// either.
        #[tokio::test]
        async fn a_match_imported_already_keeps_its_team() {
            let p = Providers::new();
            let text = three_sets_won().text();
            let (team, plan) = planned_for(&text, Side::Home, new_team()).expect("a match");
            p.save(&team, &plan).await.expect("saved");
            let saved_team = p.team(&team.team.id).await;
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
                p.save(&more, &plan).await,
                Err(AppError::Match(MatchError::MatchAlreadyExists(_)))
            ));
            assert_eq!(p.team(&team.team.id).await.players.len(), 8);
            assert_eq!(p.queued().len(), 1);
        }

        /// The same file again: told as imported, refused, nothing saved
        /// twice; deleted, it's imported again.
        #[tokio::test]
        async fn the_same_file_again_is_imported_already() {
            let p = Providers::new();
            let text = three_sets_won().text();
            let name = OpponentPlan::Name("Visiting".to_string());
            let (team, plan) = planned_for(&text, Side::Home, new_team()).expect("a match");
            assert_eq!(p.imported_already(&name, &plan).await, None);
            let saved = p.save(&team, &plan).await.expect("saved");
            assert!(Uuid::parse_str(&saved.id).is_ok(), "{}", saved.id);
            assert_ne!(saved.id, plan.entry.id);
            let saved_team = p.team(&team.team.id).await;
            let (again, plan) =
                planned_for(&text, Side::Home, TeamChoice::Existing(&saved_team)).expect("a match");
            assert_eq!(
                p.imported_already(&name, &plan).await,
                Some(saved.id.clone())
            );
            assert!(matches!(
                p.save(&again, &plan).await,
                Err(AppError::Match(MatchError::MatchAlreadyExists(_)))
            ));
            assert_eq!(p.matches(), 1);
            assert_eq!(p.queued(), vec![(team.team.id, saved.id.clone())]);
            p.execute(&format!(
                "UPDATE matches SET deleted = 1 WHERE id = '{}'",
                saved.id
            ));
            assert_eq!(p.imported_already(&name, &plan).await, None);
            p.save(&again, &plan).await.expect("saved again");
        }

        /// Another match of the same day (other scores): not the same, a
        /// second match; nor the same match on another day.
        #[tokio::test]
        async fn other_scores_or_another_day_another_match() {
            let p = Providers::new();
            let name = OpponentPlan::Name("Visiting".to_string());
            let (team, five) =
                planned_for(&five_sets().text(), Side::Home, new_team()).expect("a match");
            p.save(&team, &five).await.expect("saved");
            let saved_team = p.team(&team.team.id).await;
            let (again, three) = planned_for(
                &three_sets_won().text(),
                Side::Home,
                TeamChoice::Existing(&saved_team),
            )
            .expect("a match");
            assert_eq!(
                five.entry.date.date_naive(),
                three.entry.date.date_naive(),
                "the same day"
            );
            assert_eq!(p.imported_already(&name, &three).await, None);
            p.save(&again, &three).await.expect("saved");
            assert_eq!(p.matches(), 2);
            let later = MatchPlan {
                entry: MatchEntry {
                    date: three.entry.date + chrono::Duration::days(1),
                    ..three.entry.clone()
                },
                ..three.clone()
            };
            assert_eq!(p.imported_already(&name, &later).await, None);
        }

        /// Team B a new team (D3): saved with the players of the other team
        /// of the file, the season, level, gender and country of A; its
        /// roster of the match with the numbers of the file; the
        /// competition, the stage and the round of `[3MATCH]` (D12); the
        /// match among those of B too.
        #[tokio::test]
        async fn a_new_team_b_with_its_players_and_the_competition() {
            use crate::dvw::import::OpponentChoice;
            let p = Providers::new();
            let (team, b, plan) = planned_against(
                &three_sets_won().text(),
                Side::Home,
                new_team(),
                OpponentChoice::New,
            );
            let saved = p.save_with(&team, &b, &plan).await.expect("saved");
            let b_id = b.team().expect("a team").team.id;
            let read_b = p.team(&b_id).await;
            assert_eq!((read_b.name.as_str(), read_b.year), ("Visiting", 2014));
            assert_eq!(read_b.players.len(), 8);
            let read_a = p.team(&team.team.id).await;
            assert_eq!(
                (read_b.classification, read_b.gender, read_b.country),
                (read_a.classification, read_a.gender, read_a.country)
            );
            let m = format!("'{}'", saved.id);
            assert_eq!(
                p.text(&format!("SELECT team_b FROM matches WHERE id = {m}")),
                Some(b_id.to_string())
            );
            assert_eq!(
                p.text(&format!("SELECT team_b_name FROM matches WHERE id = {m}")),
                None
            );
            assert_eq!(
                p.sql(&format!(
                    "SELECT count(*) FROM match_roster WHERE match_id = {m} AND team_id = '{b_id}'"
                )),
                8
            );
            assert_eq!(
                p.text(&format!(
                    "SELECT group_concat(number, ',') FROM (SELECT number FROM match_roster
                     WHERE match_id = {m} AND team_id = '{b_id}' ORDER BY number)"
                )),
                Some("11,12,13,14,15,16,17,18".to_string())
            );
            assert_eq!(
                p.text(&format!(
                    "SELECT c.name || '|' || c.format || '|' || c.season FROM matches m
                     JOIN competitions c ON c.id = m.competition_id WHERE m.id = {m}"
                )),
                Some("Serie C Maschile|league|2014/2015".to_string())
            );
            assert_eq!(
                p.text(&format!(
                    "SELECT stage || '|' || round FROM matches WHERE id = {m}"
                )),
                Some("regular|Andata – giornata 1".to_string())
            );
            assert!(
                p.match_reader
                    .played_as_b(&read_b, &saved.id)
                    .expect("an answer")
                    .is_some()
            );
            // the match of A reads as before
            let read = p
                .match_reader
                .read_single(&read_a, &saved.id)
                .await
                .expect("the match");
            assert_eq!(read.opponent, "Visiting");
            assert_eq!(read.roster, team.roster);
        }

        /// The other side of the same file, into team B of the first and
        /// against team A: the same match, refused; against another team B
        /// too (a team doesn't play the same match twice in a day); a new
        /// team against a name, nothing in common: imported.
        #[tokio::test]
        async fn the_other_side_is_the_same_match() {
            use crate::dvw::import::OpponentChoice;
            let p = Providers::new();
            let text = three_sets_won().text();
            let (home, b, plan) =
                planned_against(&text, Side::Home, new_team(), OpponentChoice::New);
            let saved = p.save_with(&home, &b, &plan).await.expect("saved");
            let visiting = p.team(&b.team().expect("a team").team.id).await;
            let first = p.team(&home.team.id).await;
            let (team, b, plan) = planned_against(
                &text,
                Side::Visiting,
                TeamChoice::Existing(&visiting),
                OpponentChoice::Existing(&first),
            );
            assert_eq!(p.imported_already(&b, &plan).await, Some(saved.id.clone()));
            assert!(matches!(
                p.save_with(&team, &b, &plan).await,
                Err(AppError::Match(MatchError::MatchAlreadyExists(_)))
            ));
            // B a new team: team A of the side, B of the first, plays it
            let (_, other, plan) = planned_against(
                &text,
                Side::Visiting,
                TeamChoice::Existing(&visiting),
                OpponentChoice::New,
            );
            assert_eq!(
                p.imported_already(&other, &plan).await,
                Some(saved.id.clone())
            );
            // the home side into team A, against another team B
            let (_, other, plan) = planned_against(
                &text,
                Side::Home,
                TeamChoice::Existing(&first),
                OpponentChoice::New,
            );
            assert_eq!(p.imported_already(&other, &plan).await, Some(saved.id));
            let (team, name, plan) =
                planned_against(&text, Side::Visiting, new_team(), OpponentChoice::Name);
            assert_eq!(p.imported_already(&name, &plan).await, None);
            p.save_with(&team, &name, &plan).await.expect("saved");
            assert_eq!(p.matches(), 2);
        }

        /// A player of A with another number in the file: the number of the
        /// file in the roster of the match (D9), the team keeps its own.
        #[tokio::test]
        async fn the_number_of_the_file_on_the_score_sheet() {
            use crate::dvw::import::OpponentChoice;
            let p = Providers::new();
            let mut ours = crate::shapes::team::TeamEntry {
                name: "Home".to_string(),
                year: 2014,
                id: Uuid::new_v4(),
                ..Default::default()
            };
            let three = crate::shapes::player::PlayerEntry {
                name: "Name3 Surname3".to_string(),
                number: 30,
                ..Default::default()
            };
            ours.players.push(three.clone());
            let (team, b, plan) = planned_against(
                &three_sets_won().text(),
                Side::Home,
                TeamChoice::Existing(&ours),
                OpponentChoice::Name,
            );
            let saved = p.save_with(&team, &b, &plan).await.expect("saved");
            let read_team = p.team(&ours.id).await;
            assert_eq!(read_team.find_player(three.id).map(|p| p.number), Some(30));
            assert_eq!(
                p.sql(&format!(
                    "SELECT number FROM match_roster WHERE match_id = '{}' AND person_id = '{}'",
                    saved.id, three.id
                )),
                3
            );
            let read = p
                .match_reader
                .read_single(&read_team, &saved.id)
                .await
                .expect("the match");
            assert_eq!(read.team.find_player(three.id).map(|p| p.number), Some(3));
            // only a name: no roster of B
            assert_eq!(p.sql("SELECT count(DISTINCT team_id) FROM match_roster"), 1);
        }

        /// Team B changed by the form to a name, the match scouted only on
        /// A's side: its roster of the match goes with it, the one of A
        /// stays.
        #[tokio::test]
        async fn the_roster_of_b_goes_with_it() {
            use crate::{
                domain::ScoutingMode, dvw::import::OpponentChoice,
                providers::match_writer::MatchWriter,
            };
            let p = Providers::new();
            let (team, b, plan) = planned_against(
                &three_sets_won().text(),
                Side::Home,
                new_team(),
                OpponentChoice::New,
            );
            let saved = p.save_with(&team, &b, &plan).await.expect("saved");
            assert_eq!(p.sql("SELECT count(DISTINCT team_id) FROM match_roster"), 2);
            p.match_writer
                .save_scouting(&saved, ScoutingMode::A)
                .await
                .expect("only A");
            p.match_writer
                .update(
                    &saved,
                    crate::match_form::MatchInput {
                        opponent: crate::match_form::Opponent::Name("Visiting".into()),
                        ..plan.input.clone()
                    },
                )
                .await
                .expect("updated");
            assert_eq!(
                p.text("SELECT DISTINCT team_id FROM match_roster"),
                Some(team.team.id.to_string())
            );
            assert_eq!(p.sql("SELECT count(*) FROM match_roster"), 8);
        }

        /// Team B a team and every set translating on both sides: one
        /// match on both sides (TODO/001, phase 2), B's lineups and touches
        /// saved, A read as the plan has it, each side with the score of
        /// the file.
        #[tokio::test]
        async fn both_teams_in_one_match() {
            use crate::{
                dvw::import::{ImportedSides, OpponentChoice},
                providers::sqlite::sets::SqliteSetReader,
            };
            let p = Providers::new();
            let (team, b, plan) = planned_against(
                &three_sets_won().text(),
                Side::Home,
                new_team(),
                OpponentChoice::New,
            );
            assert!(matches!(&plan.sides, ImportedSides::Both(sets) if sets.len() == 3));
            let saved = p.save_with(&team, &b, &plan).await.expect("saved");
            let m = format!("'{}'", saved.id);
            assert_eq!(
                p.text(&format!("SELECT scouting FROM matches WHERE id = {m}")),
                Some("both".to_string())
            );
            assert_eq!(
                p.sql(&format!(
                    "SELECT count(*) FROM set_lineups WHERE match_id = {m} AND side = 'b'"
                )),
                3
            );
            assert!(
                p.sql(&format!(
                    "SELECT count(*) FROM events WHERE match_id = {m} AND side = 'b'"
                )) > 0
            );
            let read_team = p.team(&team.team.id).await;
            let read = p
                .match_reader
                .read_single(&read_team, &saved.id)
                .await
                .expect("the match");
            assert_eq!(read.sets, plan.entry.sets);
            let sides = SqliteSetReader::new(p.db.clone(), Language::default())
                .read_sides(&read)
                .expect("both sides");
            let scores: Vec<_> = sides
                .iter()
                .map(|s| {
                    (
                        (s.a.score_us, s.a.score_them),
                        s.b.as_ref().map(|b| (b.score_them, b.score_us)),
                    )
                })
                .collect();
            assert_eq!(scores, vec![((25, 0), Some((25, 0))); 3]);
            // B's side: B's team against A, losing every set, B's
            // statistics the ones its side replays to
            let of_b = p
                .match_reader
                .read_of_b(&read_team, &saved.id)
                .expect("readable")
                .expect("B's side");
            assert_eq!(
                (Some(of_b.team.id.to_string()), of_b.opponent.as_str()),
                (
                    p.text(&format!("SELECT team_b FROM matches WHERE id = {m}")),
                    read_team.name.as_str()
                )
            );
            let snapshots: Vec<_> = of_b
                .sets
                .iter()
                .map(|s| s.compute_snapshot(of_b.max_substitutions).expect("replays"))
                .collect();
            let set_scores: Vec<_> = snapshots
                .iter()
                .map(|s| (s.score_us, s.score_them))
                .collect();
            assert_eq!(set_scores, vec![(0, 25); 3]);
            let mut of_sides = crate::shapes::stats::Stats::new();
            sides
                .iter()
                .filter_map(|s| s.b.as_ref())
                .for_each(|b| of_sides.merge(&b.stats));
            assert_ne!(of_sides, crate::shapes::stats::Stats::new());
            assert_eq!(
                crate::analysis::match_stats(&of_b).expect("stats"),
                of_sides
            );
        }

        /// Team B only a name: only A's side, nothing to tell why; no
        /// lineup nor touch of B.
        #[tokio::test]
        async fn against_a_name_only_a_side() {
            use crate::dvw::import::{ImportedSides, OpponentChoice};
            let p = Providers::new();
            let text = three_sets_won().text();
            let (team, name, plan) =
                planned_against(&text, Side::Home, new_team(), OpponentChoice::Name);
            assert!(matches!(plan.sides, ImportedSides::A(None)));
            let saved = p.save_with(&team, &name, &plan).await.expect("saved");
            let m = format!("'{}'", saved.id);
            assert_eq!(
                p.text(&format!("SELECT scouting FROM matches WHERE id = {m}")),
                Some("a".to_string())
            );
            assert_eq!(
                p.sql(&format!(
                    "SELECT count(*) FROM set_lineups WHERE match_id = {m} AND side = 'b'"
                )),
                0
            );
            assert_eq!(
                p.sql(&format!(
                    "SELECT count(*) FROM events
                     WHERE match_id = {m} AND side = 'b' AND deleted = 0"
                )),
                0
            );
            let read = p
                .match_reader
                .read_single(&p.team(&team.team.id).await, &saved.id)
                .await
                .expect("the match");
            assert_eq!(read.sets, plan.entry.sets);
        }
    }
}
