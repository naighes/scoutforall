//! A rally of DataVolley as our rules tell it, before either team is
//! translated (`dvw::decide`, TODO/001 §4.2 and §10, phase 2): who
//! decides, the touches told by none, the scenarios refused. The rallies are
//! written as the files write them, many from the real files the decisions
//! were taken on (MEVZA); the evaluations decided are checked against the
//! tables of the engine (`rally::answer_evals`), not against a copy of
//! them.

#[cfg(test)]
mod tests {
    use crate::dvw::{
        decide::decide,
        model::{Code, ScoutLine, Side, Skill},
        scout::scout_lines,
        translate::{Impossible, Issue, Touch, default_symbols},
    };
    use crate::rally::answer_evals;
    use crate::shapes::enums::{EvalEnum, EventTypeEnum};

    /// The first line of a rally written by `lines`.
    const FIRST: usize = 10;

    /// The lines of the codes, one each from line `FIRST`.
    fn lines(codes: &[&str]) -> Vec<ScoutLine> {
        let numbered: Vec<(usize, &str)> = codes
            .iter()
            .enumerate()
            .map(|(i, code)| (FIRST + i, *code))
            .collect();
        scout_lines(&numbered)
    }

    /// The touches of the lines, as the translation gives them to `decide`.
    fn touches(lines: &[ScoutLine]) -> Vec<(&ScoutLine, Touch)> {
        lines
            .iter()
            .filter_map(|line| match &line.code {
                Code::Skill(side, skill) => Some((line, Touch::Skill(*side, skill.clone()))),
                Code::Green(side, evaluation) => Some((line, Touch::Green(*side, *evaluation))),
                _ => None,
            })
            .collect()
    }

    fn marker(side: Side) -> char {
        match side {
            Side::Home => '*',
            Side::Visiting => 'a',
        }
    }

    /// A touch as the tests write it: `*S-`, `a>` a freeball sent untold or
    /// as the file tells it, `*$#` a point without a touch.
    fn short(touch: &Touch) -> String {
        match touch {
            Touch::Skill(side, code) => {
                format!("{}{}{}", marker(*side), code.skill.code(), code.evaluation)
            }
            Touch::Sent(side, _) => format!("{}>", marker(*side)),
            Touch::Green(side, evaluation) => format!("{}${evaluation}", marker(*side)),
        }
    }

    /// The rally as `decide` tells it, touch by touch, or why not.
    fn decided(codes: &[&str]) -> Result<Vec<String>, Issue> {
        let lines = lines(codes);
        decide(&touches(&lines), &default_symbols())
            .map(|d| d.touches.iter().map(|(_, t)| short(t)).collect())
            .map_err(|(_, issue)| issue)
    }

    /// The decisions taken on the rally, by name.
    fn mapped(codes: &[&str]) -> Vec<&'static str> {
        let lines = lines(codes);
        decide(&touches(&lines), &default_symbols())
            .map(|d| d.mapped)
            .unwrap_or_default()
    }

    /// The line of the failing touch, with the issue.
    fn refused(codes: &[&str]) -> Option<(usize, Issue)> {
        let lines = lines(codes);
        decide(&touches(&lines), &default_symbols()).err()
    }

    fn eval(c: char) -> Option<EvalEnum> {
        match c {
            '#' => Some(EvalEnum::Perfect),
            '+' => Some(EvalEnum::Positive),
            '!' => Some(EvalEnum::Exclamative),
            '-' => Some(EvalEnum::Negative),
            '/' => Some(EvalEnum::Over),
            '=' => Some(EvalEnum::Error),
            _ => None,
        }
    }

    const EVALUATIONS: [char; 6] = ['#', '+', '!', '-', '/', '='];

    /// The evaluation of the touch written `code` (`*S-`), its last char.
    fn evaluation_of(code: &str) -> char {
        code.chars().last().unwrap_or_default()
    }

    /// Whether the engine admits the answer `answer` to `answered`, both as
    /// the file writes them (`rally::answer_evals`).
    fn admitted(answered: (EventTypeEnum, char), answer: (EventTypeEnum, char)) -> bool {
        eval(answer.1)
            .is_some_and(|e| answer_evals(answered.0, eval(answered.1), answer.0).contains(&e))
    }

    // who decides: the reception, the serve

    /// Every serve with every reception: the reception decides the serve,
    /// and the pair is one the engine admits; a serve ending the rally (an
    /// ace, an error) with a reception in play after it is left as it is,
    /// the reception after the end.
    #[test]
    fn the_reception_decides_the_serve() {
        for serve in EVALUATIONS {
            for reception in EVALUATIONS {
                let codes = [format!("*06SM{serve}"), format!("a07RM{reception}")];
                let codes: Vec<&str> = codes.iter().map(String::as_str).collect();
                let rally = decided(&codes).expect("a serve and its reception");
                let after_the_end = matches!(serve, '#' | '=') && reception != '=';
                let decided_serve = rally.first().map_or('?', |s| evaluation_of(s));
                assert_eq!(
                    rally.get(1).map(String::as_str),
                    Some(format!("aR{reception}").as_str()),
                    "S{serve} R{reception}: the reception as the file writes it"
                );
                match after_the_end {
                    true => assert_eq!(decided_serve, serve, "S{serve} R{reception}"),
                    false => assert!(
                        admitted(
                            (EventTypeEnum::S, decided_serve),
                            (EventTypeEnum::P, reception)
                        ),
                        "S{serve} R{reception} decided as S{decided_serve}"
                    ),
                }
            }
        }
    }

    /// The real files: `S! R+` is `S-`, `S+ R!` is `S!` (the convention of
    /// 30 files), `S+ R/` is `S/`, `S+ R=` is the ace.
    #[test]
    fn the_reception_decides_the_serve_in_the_real_files() {
        for (codes, serve) in [
            (["*14SM!", "a20RM+"], "*S-"),
            (["*08SM+", "a16RM!"], "*S!"),
            (["*11SQ+", "a11RQ/"], "*S/"),
            (["a17SH+", "*20RH="], "aS#"),
            (["*09SH-", "a07RH#"], "*S-"),
        ] {
            let rally = decided(&codes).expect("a serve and its reception");
            assert_eq!(rally.first().map(String::as_str), Some(serve), "{codes:?}");
        }
        assert_eq!(
            mapped(&["*14SM!", "a20RM+"]),
            vec!["S decided by the reception"]
        );
        assert!(mapped(&["*09SH-", "a07RH#"]).is_empty());
    }

    // who decides: the dig, the attack

    /// Every attack with every dig of the other team: the dig decides the
    /// attack (its `!` a `+`, only a cover is `!`), the pair one the engine
    /// admits; an attack ending the rally with a dig in play after it is
    /// left as it is.
    #[test]
    fn the_dig_decides_the_attack() {
        for attack in EVALUATIONS {
            for dig in EVALUATIONS {
                let codes = [format!("*02AH{attack}"), format!("a13DH{dig}")];
                let codes: Vec<&str> = codes.iter().map(String::as_str).collect();
                let rally = decided(&codes).expect("an attack and its dig");
                let told_dig = match dig {
                    '!' => '+',
                    other => other,
                };
                assert_eq!(
                    rally.get(1).map(String::as_str),
                    Some(format!("aD{told_dig}").as_str()),
                    "A{attack} D{dig}"
                );
                let after_the_end = matches!(attack, '#' | '=' | '/') && dig != '=';
                let decided_attack = rally.first().map_or('?', |s| evaluation_of(s));
                match after_the_end {
                    true => assert_eq!(decided_attack, attack, "A{attack} D{dig}"),
                    false => assert!(
                        admitted(
                            (EventTypeEnum::A, decided_attack),
                            (EventTypeEnum::D, told_dig)
                        ),
                        "A{attack} D{dig} decided as A{decided_attack}"
                    ),
                }
            }
        }
    }

    /// The real files: `A+ D+` (686 times) is `A-`, `A+ D/` stays, a dig
    /// `!` of the attack (148 times) is `+`, `A- D/` is `A+`.
    #[test]
    fn the_dig_decides_the_attack_in_the_real_files() {
        for (codes, expected) in [
            (["*07AT+", "a16DT+"], ["*A-", "aD+"]),
            (["a16AT+", "*11DT/"], ["aA+", "*D/"]),
            (["a14AH+", "*11DH!"], ["aA-", "*D+"]),
            (["a09AT-", "*03DT/"], ["aA+", "*D/"]),
            (["a16AT#", "*11DT="], ["aA#", "*D="]),
        ] {
            assert_eq!(
                decided(&codes),
                Ok(expected.iter().map(|s| s.to_string()).collect()),
                "{codes:?}"
            );
        }
    }

    // who decides: the block, the attack

    /// Every attack with every block: the block decides the attack, its
    /// error always (`A+ B=` the attack point), the pair one the engine
    /// admits; after an invasion (`B/`) the attack is left out, but an
    /// attack error (the error first); an attack ending the rally with a
    /// block in play is left as it is (the ball down all the same).
    #[test]
    fn the_block_decides_the_attack() {
        for attack in EVALUATIONS {
            for block in EVALUATIONS {
                let codes = [format!("*02AH{attack}"), format!("a13BH{block}")];
                let codes: Vec<&str> = codes.iter().map(String::as_str).collect();
                let rally = decided(&codes).expect("an attack and its block");
                let after_the_end =
                    matches!(attack, '#' | '=' | '/') && matches!(block, '+' | '-' | '!');
                match (block, attack) {
                    ('/', '=') => assert_eq!(rally, vec!["*A=", "aB/"]),
                    ('/', _) => assert_eq!(rally, vec!["aB/"], "A{attack} B/"),
                    _ => {
                        assert_eq!(
                            rally.get(1).map(String::as_str),
                            Some(format!("aB{block}").as_str())
                        );
                        let decided_attack = rally.first().map_or('?', |s| evaluation_of(s));
                        match after_the_end {
                            true => assert_eq!(decided_attack, attack, "A{attack} B{block}"),
                            false => assert!(
                                admitted(
                                    (EventTypeEnum::A, decided_attack),
                                    (EventTypeEnum::B, block)
                                ),
                                "A{attack} B{block} decided as A{decided_attack}"
                            ),
                        }
                    }
                }
            }
        }
    }

    /// The real files: `A# B/`, `A+ B/` (163 times) keep only the invasion,
    /// `A+ B=` (39 times) and `A- B=` are the attack point ("mani fuori"),
    /// `A! B-` is `A+`.
    #[test]
    fn the_block_decides_the_attack_in_the_real_files() {
        for (codes, expected) in [
            (vec!["a14AT#", "*06BT/"], vec!["*B/"]),
            (vec!["a13AT+", "*08BT/"], vec!["*B/"]),
            (vec!["*11AH+", "a01BN="], vec!["*A#", "aB="]),
            (vec!["*18AQ-", "a17BQ="], vec!["*A#", "aB="]),
            (vec!["a17AH/", "*12BH#"], vec!["aA/", "*B#"]),
            (vec!["a11AH#", "*09BH="], vec!["aA#", "*B="]),
        ] {
            assert_eq!(
                decided(&codes),
                Ok(expected.iter().map(|s| s.to_string()).collect()),
                "{codes:?}"
            );
        }
        assert_eq!(
            mapped(&["a14AT#", "*06BT/"]),
            vec!["A before an invasion: left out"]
        );
    }

    /// The invasion leaves out only the attack right before it: an attack
    /// of the same team before that one, the roads open, stays.
    #[test]
    fn the_invasion_leaves_out_only_the_attack_it_follows() {
        assert_eq!(
            decided(&["a11AT+", "a17EQ+", "a01AQ+", "*04BQ/"]),
            Ok(vec!["aA+".into(), "aE+".into(), "*B/".into()])
        );
    }

    // who decides: who plays after the block

    /// A block in play: the attackers playing next make it `!` and their
    /// attack `!` (the ball back to them); the blockers' dig decides it
    /// (`#`, `+`, `!` the block `+`, the others `-`) and so the attack; the
    /// blockers playing next without a dig make it `+`; nothing after it
    /// leaves it as it is.
    #[test]
    fn who_plays_after_the_block_decides_it() {
        for block in ['+', '-', '!'] {
            let after = |next: &str| {
                let codes = [
                    "*02AH-".to_string(),
                    format!("a13BH{block}"),
                    next.to_string(),
                ];
                let codes: Vec<&str> = codes.iter().map(String::as_str).collect();
                decided(&codes)
            };
            for dig in EVALUATIONS {
                let told = match dig {
                    '#' | '+' | '!' => '+',
                    _ => '-',
                };
                let dig_told = match dig {
                    '!' => '+',
                    other => other,
                };
                let attack = match told {
                    '+' => '-',
                    _ => '+',
                };
                assert_eq!(
                    after(&format!("a15DH{dig}")),
                    Ok(vec![
                        format!("*A{attack}"),
                        format!("aB{told}"),
                        format!("aD{dig_told}")
                    ]),
                    "B{block} D{dig} of the blockers"
                );
            }
            for next in ["a03EH+", "a14AH+"] {
                assert_eq!(
                    after(next),
                    Ok(vec![
                        "*A-".to_string(),
                        "aB+".to_string(),
                        format!("a{}", &next[3..4]) + &next[5..6]
                    ]),
                    "B{block} then {next}"
                );
            }
            for next in ["*07DH-", "*01EH+", "*04AH#"] {
                let rally = after(next).expect("the attackers again");
                assert_eq!(
                    rally.get(..2),
                    Some(&["*A!".to_string(), "aB!".to_string()][..]),
                    "B{block} then {next}"
                );
            }
            let codes = ["*02AH-".to_string(), format!("a13BH{block}")];
            let codes: Vec<&str> = codes.iter().map(String::as_str).collect();
            let alone = decided(&codes).expect("a block alone");
            assert_eq!(alone.get(1), Some(&format!("aB{block}")));
        }
    }

    /// The real files: `*16AQ- a07BQ+ *06EH+ *07AH-` (593 blocks `+` the
    /// attackers play after) is `A!`, `B!`; `*07AT- a05BT+ a16DT+` keeps
    /// `A-`, `B+`; `a05AT- *88BT+ *24DH-` is `A+`, `B-`; `a05AM+ *07BM-
    /// *88DM/` keeps its `-`; a dig `=` of the blockers makes the block `-`.
    #[test]
    fn who_plays_after_the_block_decides_it_in_the_real_files() {
        for (codes, expected) in [
            (
                vec!["*16AQ-", "a07BQ+", "*06EH+", "*07AH-"],
                vec!["*A!", "aB!", "*E+", "*A-"],
            ),
            (
                vec!["*07AT-", "a05BT+", "a16DT+", "a14AT#"],
                vec!["*A-", "aB+", "aD+", "aA#"],
            ),
            (
                vec!["a05AT-", "*88BT+", "*24DH-"],
                vec!["aA+", "*B-", "*D-"],
            ),
            (
                vec!["a05AM+", "*07BM-", "*88DM/"],
                vec!["aA+", "*B-", "*D/"],
            ),
            (
                vec!["*09AH-", "a24BH+", "a06DH="],
                vec!["*A+", "aB-", "aD="],
            ),
            (
                vec!["a05AT-", "*17BT+", "*10DH!"],
                vec!["aA-", "*B+", "*D+"],
            ),
        ] {
            assert_eq!(
                decided(&codes),
                Ok(expected.iter().map(|s| s.to_string()).collect()),
                "{codes:?}"
            );
        }
    }

    // the cover

    /// A dig of the team right after its own attack in play is its cover:
    /// the attack `!` (back from the block, untold); after an attack ending
    /// the rally it's after the end, left as it is.
    #[test]
    fn a_dig_after_the_own_attack_is_its_cover() {
        for attack in EVALUATIONS {
            let codes = [format!("*08AH{attack}"), "*10DH+".to_string()];
            let codes: Vec<&str> = codes.iter().map(String::as_str).collect();
            let rally = decided(&codes).expect("an attack and a dig");
            let expected = match attack {
                '#' | '=' | '/' => attack,
                _ => '!',
            };
            assert_eq!(
                rally,
                vec![format!("*A{expected}"), "*D+".to_string()],
                "A{attack} then our dig"
            );
        }
        // `&MIL-25.dvw`
        assert_eq!(
            mapped(&["*08AH+", "*10DH+"]),
            vec!["A covered by the team: A!"]
        );
    }

    // the freeballs received

    /// A freeball received right after the other team's attack is a dig
    /// (an easy attack), decided as one; a reception there too.
    #[test]
    fn a_freeball_or_a_reception_after_the_attack_is_a_dig() {
        assert_eq!(
            decided(&["*07AH+", "a08FH#"]),
            Ok(vec!["*A-".into(), "aD#".into()])
        );
        // `&MIL-75.dvw`: `a24AQ- *04RQ+`
        assert_eq!(
            decided(&["a24AQ-", "*04RQ+"]),
            Ok(vec!["aA-".into(), "*D+".into()])
        );
        assert_eq!(
            mapped(&["a24AQ-", "*04RQ+"]),
            vec!["R after the other team's attack: D"]
        );
    }

    /// A freeball received is the freeball the other team sends, the
    /// touch of that team (DataVolley writes only the one received, p. 56):
    /// after a block it tells who played after the block. `&WIL-23.dvw`
    /// `*07AH- a07BH- *12FH+` is `A-`, `B+`, the visitors' freeball.
    #[test]
    fn a_freeball_received_is_the_freeball_sent() {
        assert_eq!(
            decided(&["*07AH-", "a07BH-", "*12FH+"]),
            Ok(vec!["*A-".into(), "aB+".into(), "a>".into()])
        );
        // after the other team's reception keeping the ball (`&MIL-11.dvw`)
        assert_eq!(
            decided(&["*08SM+", "a16RM-", "*06FH#"]),
            Ok(vec!["*S+".into(), "aR-".into(), "a>".into()])
        );
        // after the team's own attack (the other team's dig untold): the
        // other team sent it back
        assert_eq!(
            decided(&["a17AT+", "a17FH+"]),
            Ok(vec!["aA+".into(), "*>".into()])
        );
        // the other team's set going over (`&MIL-13.dvw`)
        assert_eq!(
            decided(&["*17RM-", "*06EH+", "a18FH#"]),
            Ok(vec!["*R-".into(), "*E+".into(), "*>".into()])
        );
    }

    /// A freeball received after the ball came over with a touch sending
    /// it (a reception or dig `/`, a set `/`): nothing more to tell, left
    /// out. `&MIL-10.dvw` `*11SQ/ a04RQ/ *04FH+` is the serve `/` and the
    /// reception `/`, no freeball of its own.
    #[test]
    fn a_freeball_received_after_the_ball_over_is_left_out() {
        assert_eq!(
            decided(&["*11SQ/", "a04RQ/", "*04FH+"]),
            Ok(vec!["*S/".into(), "aR/".into()])
        );
        assert_eq!(decided(&["*13EH/", "a04FH+"]), Ok(vec!["*E/".into()]));
    }

    /// A freeball received with an error keeps the error of the team
    /// receiving it, after the freeball sent: the point is its fault.
    #[test]
    fn a_freeball_received_with_an_error_keeps_it() {
        assert_eq!(
            decided(&["a17AT+", "a14FH="]),
            Ok(vec!["aA+".into(), "*>".into(), "aF=".into()])
        );
        assert_eq!(
            decided(&["*11SQ/", "a04RQ/", "*04FH="]),
            Ok(vec!["*S/".into(), "aR/".into(), "*F=".into()])
        );
    }

    /// A freeball received right after the team's own block in play or
    /// attack `!` is its dig after the block, or its cover; right after
    /// the other team's block `!` the cover of the attackers.
    #[test]
    fn a_freeball_after_a_block_is_a_dig_or_a_cover() {
        // `&MIL-11.dvw`: `a08AQ- *06BQ+ *02FH#`
        assert_eq!(
            decided(&["a08AQ-", "*06BQ+", "*02FH#"]),
            Ok(vec!["aA-".into(), "*B+".into(), "*D#".into()])
        );
        assert_eq!(
            decided(&["*18AH!", "*04FH-"]),
            Ok(vec!["*A!".into(), "*D-".into()])
        );
        assert_eq!(
            decided(&["*02AH-", "a13BH!", "*07FH+"]),
            Ok(vec!["*A!".into(), "aB!".into(), "*D+".into()])
        );
    }

    /// A freeball of a team right after its own touch keeping the ball (a
    /// dig, a reception, a set): a pass, its second touch, left out as a
    /// set; over the net it's the team's freeball, an error its fault.
    /// `&MIL-11.dvw`: `a13DT+ a08FH# a08AQ-`.
    #[test]
    fn a_freeball_after_the_own_ball_is_a_pass() {
        assert_eq!(
            decided(&["*11AT+", "a13DT+", "a08FH#", "a08AQ-"]),
            Ok(vec!["*A-".into(), "aD+".into(), "aA-".into()])
        );
        assert_eq!(
            decided(&["*11AT+", "a13DT+", "a08FH/"]),
            Ok(vec!["*A-".into(), "aD+".into(), "a>".into()])
        );
        assert_eq!(
            decided(&["*11AT+", "a13DT+", "a08FH="]),
            Ok(vec!["*A-".into(), "aD+".into(), "aF=".into()])
        );
    }

    /// A freeball received after the serve, the first touch of the rally or
    /// a touch ending it is left as the file writes it, for the translation.
    #[test]
    fn a_freeball_elsewhere_is_as_written() {
        for codes in [
            vec!["*06SM+", "a07FM+"],
            vec!["a07FM+"],
            vec!["*06SM#", "a07FM="],
        ] {
            let expected: Vec<String> = codes
                .iter()
                .map(|c| format!("{}{}{}", &c[..1], &c[3..4], &c[5..6]))
                .collect();
            assert_eq!(decided(&codes), Ok(expected), "{codes:?}");
        }
    }

    // the freeballs sent untold

    /// A team keeping the ball with the other one touching next, not
    /// answering: its freeball sent untold, at the line of that touch.
    /// `&MIL-23.dvw` `*09SQ+ a02RQ- *13EH+`: the visitors' freeball before
    /// the set; after a cover (`&MIL-14.dvw` `*14AT! a19ET+`) the home
    /// team's; after a set (`&MIL-16.dvw` `*13EH/ a04AO#` is over already).
    #[test]
    fn the_freeball_a_team_keeping_the_ball_sends_untold() {
        assert_eq!(
            decided(&["*09SQ+", "a02RQ-", "*13EH+"]),
            Ok(vec!["*S+".into(), "aR-".into(), "a>".into(), "*E+".into()])
        );
        assert_eq!(
            decided(&["*14AT!", "a19ET+", "a18AT#"]),
            Ok(vec!["*A!".into(), "*>".into(), "aE+".into(), "aA#".into()])
        );
        assert_eq!(
            decided(&["*06EH+", "a18AH+"]),
            Ok(vec!["*E+".into(), "*>".into(), "aA+".into()])
        );
        assert_eq!(
            decided(&["*13EH/", "a04AO#"]),
            Ok(vec!["*E/".into(), "aA#".into()])
        );
        assert_eq!(
            mapped(&["*09SQ+", "a02RQ-", "*13EH+"]),
            vec!["freeball sent untold: FB"]
        );
        // at the line of the touch after it
        let lines = lines(&["*09SQ+", "a02RQ-", "*13EH+"]);
        let rally = decide(&touches(&lines), &default_symbols()).expect("the rally");
        let at: Vec<usize> = rally.touches.iter().map(|(l, _)| l.line).collect();
        assert_eq!(at, vec![FIRST, FIRST + 1, FIRST + 2, FIRST + 2]);
    }

    /// A team playing again the ball it sent over (its attack `-`, its
    /// serve, its freeball) had it back with a freeball of the other team,
    /// untold; its attack `+` opens the roads (the other team's dig `/`
    /// untold): nothing more (`&MIL-1.dvw` `a16AH+ a07AH#`).
    #[test]
    fn the_freeball_back_untold_but_after_the_attack_plus() {
        assert_eq!(
            decided(&["*10AH-", "*06ET+"]),
            Ok(vec!["*A-".into(), "a>".into(), "*E+".into()])
        );
        assert_eq!(
            decided(&["a16AH+", "a07AH#"]),
            Ok(vec!["aA+".into(), "aA#".into()])
        );
        assert_eq!(
            decided(&["*06SM+", "*02EH+"]),
            Ok(vec!["*S+".into(), "a>".into(), "*E+".into()])
        );
    }

    /// An answer needs no freeball: the reception of the serve, the dig or
    /// the block of the attack; a fault (the invasion) needs none either.
    #[test]
    fn answers_and_faults_need_no_freeball() {
        for codes in [
            vec!["*06SM-", "a07RM+"],
            vec!["*02AH!", "a13BH!"],
            vec!["*02AH!", "a13DH+"],
            vec!["*06EH+", "a13BH/"],
            vec!["*06RH+", "a13BH/"],
        ] {
            let rally = decided(&codes).expect("the rally");
            assert!(
                !rally.iter().any(|t| t.ends_with('>')),
                "{codes:?}: {rally:?}"
            );
        }
    }

    /// After the end of the rally what the file adds is up to the
    /// translation: no freeball, no refusal, no decision.
    #[test]
    fn nothing_is_told_after_the_end() {
        assert_eq!(
            decided(&["*02AH#", "a15DH+", "a13BH-"]),
            Ok(vec!["*A#".into(), "aD+".into(), "aB-".into()])
        );
        assert_eq!(
            decided(&["*06SM#", "a13BH-"]),
            Ok(vec!["*S#".into(), "aB-".into()])
        );
    }

    // the scenarios refused

    /// A touch of the team after its own reception over (`/`), 82 rallies
    /// in 43 files: refused at the line of the touch, with the reception
    /// and its line (`&MIL-10.dvw` `*19SQ/ a09RQ/ a10AH#`). A dig over the
    /// same.
    #[test]
    fn a_touch_after_the_own_reception_or_dig_over_is_refused() {
        assert_eq!(
            refused(&["*19SQ/", "a09RQ/", "a10AH#"]),
            Some((
                FIRST + 2,
                Issue::Impossible(Impossible::TouchAfterOver {
                    over: (Skill::Reception, '/'),
                    over_line: FIRST + 1,
                    touch: (Skill::Attack, '#'),
                })
            ))
        );
        for touch in ["a17EQ+", "a17EH=", "a19BH="] {
            let refusal = refused(&["*11SQ/", "a17RQ/", touch]);
            assert!(
                matches!(
                    refusal,
                    Some((_, Issue::Impossible(Impossible::TouchAfterOver { .. })))
                ),
                "{touch}: {refusal:?}"
            );
        }
        // `&MIL-73.dvw` `a15AH+ *14DH/ *09EH=`
        assert_eq!(
            refused(&["a15AH+", "*14DH/", "*09EH="]),
            Some((
                FIRST + 2,
                Issue::Impossible(Impossible::TouchAfterOver {
                    over: (Skill::Dig, '/'),
                    over_line: FIRST + 1,
                    touch: (Skill::Set, '='),
                })
            ))
        );
    }

    /// The reception over, then a freeball the team receives: the other
    /// team sent it back, not a touch of the same team.
    #[test]
    fn a_freeball_back_after_the_reception_over_is_no_refusal() {
        assert_eq!(
            decided(&["a11SQ/", "*18RQ/", "*20FH+"]),
            Ok(vec!["aS/".into(), "*R/".into(), "a>".into()])
        );
        // `&MIL-27.dvw`: received with an error
        assert_eq!(
            decided(&["a11SQ/", "*18RQ/", "*20FH="]),
            Ok(vec!["aS/".into(), "*R/".into(), "a>".into(), "*F=".into()])
        );
    }

    /// A dig or a block without the attack of the other team: the reception
    /// followed by the block or the dig of the team who served (4 rallies,
    /// `&WIL-22.dvw` `a07SM+ *03RM! a15BH=`), a dig, a set, a block keeping
    /// the ball followed by the other team's dig or block, a block or a dig
    /// of a team after its own touch keeping the ball (its dig after its
    /// block and its cover aside): refused.
    #[test]
    fn a_dig_or_a_block_without_an_attack_is_refused() {
        assert_eq!(
            refused(&["a07SM+", "*03RM!", "a15BH="]),
            Some((
                FIRST + 2,
                Issue::Impossible(Impossible::MissingAttack {
                    before: (Skill::Reception, '!'),
                    before_line: FIRST + 1,
                    touch: (Skill::Block, '='),
                })
            ))
        );
        for codes in [
            // `&MIL-50.dvw`
            vec!["a07SH+", "*20RH-", "a07DH#"],
            // `&MIL-25.dvw`
            vec!["a16AQ+", "*08DQ+", "a05DH+"],
            // `&MIL-19.dvw`
            vec!["a05RQ+", "a01EH+", "*01BT="],
            // `&MIL-16.dvw`: two blocks
            vec!["a11AT-", "*11BT+", "*03BH+"],
            // `&MIL-16.dvw`: a dig after the team's own set
            vec!["a04SM+", "*11RM-", "*13EH+", "*09DH+"],
            // a block after the team's own attack `+`
            vec!["*02AH+", "*09BH="],
        ] {
            let refusal = refused(&codes);
            assert!(
                matches!(
                    refusal,
                    Some((_, Issue::Impossible(Impossible::MissingAttack { .. })))
                ),
                "{codes:?}: {refusal:?}"
            );
        }
    }

    /// The dig after the team's own block, its cover after its attack `!`
    /// or the other team's block `!`: no attack missing.
    #[test]
    fn the_dig_after_the_block_and_the_cover_need_no_attack() {
        for codes in [
            vec!["a05AT-", "*07BT+", "*10DH#"],
            vec!["*18AH!", "*10DH-"],
            vec!["*02AH-", "a13BH!", "*07DH+"],
        ] {
            assert!(decided(&codes).is_ok(), "{codes:?}");
        }
    }

    // a fuzz of `decide`

    /// xorshift64*: the same rallies on every run.
    struct Rng(u64);

    impl Rng {
        fn new(seed: u64) -> Self {
            Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
        }

        fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) % (n.max(1) as u64)) as usize
        }
    }

    /// A rally of random codes: a serve, then touches of either team, now
    /// and then a point without a touch.
    fn random_rally(rng: &mut Rng) -> Vec<String> {
        let skills = ['R', 'A', 'B', 'D', 'E', 'F'];
        let mut codes = vec![format!(
            "{}06SM{}",
            ['*', 'a'][rng.below(2)],
            EVALUATIONS[rng.below(6)]
        )];
        for _ in 0..rng.below(9) {
            let side = ['*', 'a'][rng.below(2)];
            match rng.below(20) {
                0 => codes.push(format!("{side}$$&H{}", ['#', '='][rng.below(2)])),
                _ => codes.push(format!(
                    "{side}0{}{}H{}",
                    1 + rng.below(6),
                    skills[rng.below(skills.len())],
                    EVALUATIONS[rng.below(6)]
                )),
            }
        }
        codes
    }

    /// Where the touch leaves the ball, as the translation reads it: on the
    /// side of the team who played it (`Some(true)`), on the other
    /// (`Some(false)`), nowhere (`None`, the end of the rally).
    fn keeps(touch: &Touch) -> Option<bool> {
        let symbols = default_symbols();
        match touch {
            Touch::Sent(_, _) => Some(false),
            Touch::Green(_, _) => None,
            Touch::Skill(_, code) => match (
                symbols.winning(code.skill).contains(&code.evaluation)
                    || symbols.losing(code.skill).contains(&code.evaluation),
                code.skill,
                code.evaluation,
            ) {
                (true, _, _) => None,
                (_, Skill::Serve, _) => Some(false),
                (_, Skill::Attack, '!') => Some(true),
                (_, Skill::Attack, _) => Some(false),
                (_, Skill::Block, '!') => Some(false),
                (_, Skill::Block, _) => Some(true),
                (_, _, '/') => Some(false),
                _ => Some(true),
            },
        }
    }

    fn event_type(skill: Skill) -> Option<EventTypeEnum> {
        match skill {
            Skill::Serve => Some(EventTypeEnum::S),
            Skill::Reception => Some(EventTypeEnum::P),
            Skill::Attack => Some(EventTypeEnum::A),
            Skill::Block => Some(EventTypeEnum::B),
            Skill::Dig => Some(EventTypeEnum::D),
            _ => None,
        }
    }

    /// 20000 random rallies: `decide` never fails but for an error of the
    /// scout at a line of the rally; and up to the end of the rally,
    /// - every answer of the other team (the reception of the serve, the
    ///   dig or the block, not the invasion, of the attack) is a pair the
    ///   engine admits;
    /// - the ball goes from a team to the other only with a touch sending
    ///   it, an answer, a fault, or the attack `+` opening the roads;
    /// - no dig `!` answers an attack, no freeball received follows the
    ///   other team's attack.
    #[test]
    fn decide_fuzz() {
        let mut rng = Rng::new(7);
        let (mut decided_rallies, mut refusals) = (0, 0);
        for n in 0..20_000 {
            let codes = random_rally(&mut rng);
            let codes: Vec<&str> = codes.iter().map(String::as_str).collect();
            let lines = lines(&codes);
            let rally = match decide(&touches(&lines), &default_symbols()) {
                Ok(rally) => rally,
                Err((line, Issue::Impossible(_))) => {
                    assert!(
                        (FIRST..FIRST + codes.len()).contains(&line),
                        "{n} {codes:?}: line {line}"
                    );
                    refusals += 1;
                    continue;
                }
                Err(other) => panic!("{n} {codes:?}: {other:?}"),
            };
            decided_rallies += 1;
            let played: Vec<(Side, &Touch)> = rally
                .touches
                .iter()
                .filter_map(|(_, t)| match t {
                    Touch::Skill(side, _) | Touch::Sent(side, _) => Some((*side, t)),
                    Touch::Green(_, _) => None,
                })
                .collect();
            let end = played
                .iter()
                .position(|(_, t)| keeps(t).is_none())
                .map_or(played.len(), |i| i + 1);
            let shown: Vec<String> = rally.touches.iter().map(|(_, t)| short(t)).collect();
            for pair in played.get(..end).unwrap_or_default().windows(2) {
                let [(x, t), (z, u)] = pair else { continue };
                let (
                    Touch::Skill(_, a) | Touch::Sent(_, a),
                    Touch::Skill(_, b) | Touch::Sent(_, b),
                ) = (t, u)
                else {
                    continue;
                };
                let sent = matches!(u, Touch::Sent(_, _));
                let answer = x != z
                    && !sent
                    && matches!(
                        (a.skill, b.skill),
                        (Skill::Serve, Skill::Reception)
                            | (Skill::Attack, Skill::Dig | Skill::Block)
                    );
                let invasion = b.skill == Skill::Block && b.evaluation == '/';
                if let (true, false, Some(answered), Some(answering)) =
                    (answer, invasion, event_type(a.skill), event_type(b.skill))
                {
                    assert!(
                        admitted((answered, a.evaluation), (answering, b.evaluation)),
                        "{n} {codes:?} as {shown:?}: {}{} {}{}",
                        a.skill.code(),
                        a.evaluation,
                        b.skill.code(),
                        b.evaluation
                    );
                }
                assert!(
                    !(answer && b.skill == Skill::Dig && b.evaluation == '!'),
                    "{n} {codes:?} as {shown:?}: a dig ! of the attack"
                );
                assert!(
                    !(x != z && a.skill == Skill::Attack && !sent && b.skill == Skill::Freeball),
                    "{n} {codes:?} as {shown:?}: a freeball received after the attack"
                );
                let ball_on = match keeps(t) {
                    Some(true) => Some(*x),
                    Some(false) => Some(x.other()),
                    None => None,
                };
                let open_roads = a.skill == Skill::Attack && a.evaluation == '+' && x == z;
                assert!(
                    ball_on == Some(*z) || answer || invasion || open_roads,
                    "{n} {codes:?} as {shown:?}: the ball jumps to {z:?} after {}{}",
                    a.skill.code(),
                    a.evaluation
                );
            }
        }
        // both kinds of rallies, plenty of each
        assert!(decided_rallies > 2_000, "{decided_rallies} decided");
        assert!(refusals > 2_000, "{refusals} refused");
    }
}
