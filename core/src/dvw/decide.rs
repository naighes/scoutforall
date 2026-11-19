//! The touches of a rally of DataVolley as our rules tell them (TODO/001,
//! §4.2 and §10, phase 2), before either team is translated: the same for
//! both teams, so that their translations agree. DataVolley is copied, the
//! two evaluations of a transition are a pair of the tables
//! (`rally::answer_evals`), and when the two teams of the file tell it
//! differently the answering touch decides:
//! - the reception decides the serve (`S! R+` is `S-`, `R+`);
//! - the dig decides the attack (`A+ D+` is `A-`, `D+`); a dig `!` of the
//!   other team's attack is a dig `+` (it lets them build: `!` is only a
//!   cover);
//! - the block decides the attack, its error always (`A+ B=` is `A#`,
//!   `B=`); after an invasion (`B/`) the attack is left out;
//! - a block touching the other team's kill (`A# B-`) is a block `=`: the kill
//!   stays (DataVolley credits it);
//! - a block in play is decided by who plays next: the attackers, the block
//!   `!` and the attack `!` (the ball back to them); the blockers, their
//!   dig decides it (`+`/`#` the block `+`, `-`/`/` the block `-`), `+`
//!   without one;
//! - a dig of a team right after its own attack is its cover: the attack
//!   `!` (back from the block);
//! - a freeball received (DataVolley writes only that one, p. 56) is the
//!   freeball the other team sends over, untold unless the ball was over
//!   already; right after the other team's attack it's a dig, right after
//!   the team's own block or attack `!` its dig or its cover.
//!
//! Then the ball goes from a team to the other only with a touch sending it
//! over: a team keeping the ball (a reception or a dig in play, a set, a
//! block `+`/`-`, an attack `!`) with the other team touching next, not
//! answering it, sent a freeball, and the other way round a team playing
//! again a ball it sent over (its attack `-`, its serve), but its attack `+`
//! (the roads open: the other team's dig `/` untold), had it back with a
//! freeball. Two scenarios are impossible, errors of the scout that stop the
//! match (the file has to be corrected): a touch of a team after its own
//! reception or dig over (`/`), and a dig or a block of a team without the
//! attack of the other one.
use crate::dvw::{
    model::{ScoutLine, Side, Skill, SkillCode, WinningSymbols},
    translate::{Impossible, Issue, Touch},
};

/// A touch of the rally with the line of the file it's at: a touch told by
/// none (a freeball sent untold) is at the line of the touch after it.
pub(crate) type Lined<'a> = (&'a ScoutLine, Touch);

/// The touches of the rally as our rules tell them, and the decisions taken
/// (by name, counted in the summary of the import).
#[derive(Debug)]
pub(crate) struct Decided<'a> {
    pub touches: Vec<Lined<'a>>,
    pub mapped: Vec<&'static str>,
}

/// Where a touch leaves the ball.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ball {
    /// on the side of the team who played it
    Own,
    /// on the other side
    Other,
    /// nowhere: the rally is over
    Ended,
}

/// Whether the evaluation ends the rally, as the file's symbols tell it.
fn ending(symbols: &WinningSymbols, code: &SkillCode) -> bool {
    symbols.winning(code.skill).contains(&code.evaluation)
        || symbols.losing(code.skill).contains(&code.evaluation)
}

/// Where the touch leaves the ball, in the file.
fn ball(symbols: &WinningSymbols, touch: &Touch) -> Option<Ball> {
    match touch {
        Touch::Green(_, _) => None,
        Touch::Sent(_, _) => Some(Ball::Other),
        Touch::Skill(_, code) => Some(match (ending(symbols, code), code.skill, code.evaluation) {
            (true, _, _) => Ball::Ended,
            (_, Skill::Serve, _) => Ball::Other,
            (_, Skill::Reception | Skill::Dig | Skill::Set | Skill::Freeball, '/') => Ball::Other,
            (_, Skill::Reception | Skill::Dig | Skill::Set | Skill::Freeball, _) => Ball::Own,
            (_, Skill::Attack, '!') => Ball::Own,
            (_, Skill::Attack, _) => Ball::Other,
            (_, Skill::Block, '!') => Ball::Other,
            (_, Skill::Block, _) => Ball::Own,
        }),
    }
}

/// The touch with another skill or evaluation, its side and the rest as
/// written.
fn with(touch: &Touch, skill: Option<Skill>, evaluation: Option<char>) -> Touch {
    let code = |code: &SkillCode| SkillCode {
        skill: skill.unwrap_or(code.skill),
        evaluation: evaluation.unwrap_or(code.evaluation),
        ..code.clone()
    };
    match touch {
        Touch::Skill(side, c) => Touch::Skill(*side, code(c)),
        Touch::Sent(side, c) => Touch::Sent(*side, code(c)),
        Touch::Green(side, e) => Touch::Green(*side, *e),
    }
}

/// The freeball a team sends over untold: no player.
fn sent(side: Side) -> Touch {
    Touch::Sent(
        side,
        SkillCode {
            player: None,
            skill: Skill::Freeball,
            skill_type: 'H',
            evaluation: '+',
            combination: None,
            target: None,
            start_zone: None,
            end_zone: None,
            end_subzone: None,
            skill_subtype: None,
            num_players: None,
            special: None,
            custom: String::new(),
        },
    )
}

/// The touch of a player: its side, its code, whether it's a freeball sent.
fn played(touch: &Touch) -> Option<(Side, &SkillCode, bool)> {
    match touch {
        Touch::Skill(side, code) => Some((*side, code, false)),
        Touch::Sent(side, code) => Some((*side, code, true)),
        Touch::Green(_, _) => None,
    }
}

/// The index of the next touch of a player after `index`.
fn next_played(touches: &[Lined], index: usize) -> Option<usize> {
    touches
        .iter()
        .enumerate()
        .skip(index + 1)
        .find(|(_, (_, t))| played(t).is_some())
        .map(|(i, _)| i)
}

/// The touches of the rally (the freeballs sent as the file tells them
/// already, see `translate::freeballs_sent`) as our rules tell them, or the
/// line of the touch the rally can't go on with and why.
pub(crate) fn decide<'a>(
    touches: &[Lined<'a>],
    symbols: &WinningSymbols,
) -> Result<Decided<'a>, (usize, Issue)> {
    let (touches, mut mapped) = freeballs_received(touches, symbols);
    let (touches, decided) = evaluations(touches, symbols);
    mapped.extend(decided);
    let (touches, untold) = untold(touches, symbols)?;
    mapped.extend(untold);
    Ok(Decided { touches, mapped })
}

/// A freeball received (not sent) as the touch it is: right after the
/// other team's attack a dig; right after the other team's block `!` the
/// cover; right after the team's own block in play or attack `!` its dig
/// or its cover; right after another touch of the team keeping the ball a
/// pass (left out, its freeball when over); otherwise the freeball the team
/// who had the ball sends over, left out when its last touch sent it over
/// already. A reception right after the other team's attack is a dig.
fn freeballs_received<'a>(
    touches: &[Lined<'a>],
    symbols: &WinningSymbols,
) -> (Vec<Lined<'a>>, Vec<&'static str>) {
    touches.iter().fold(
        (vec![], vec![]),
        |(mut out, mut mapped): (Vec<Lined<'a>>, Vec<&'static str>), (line, touch)| {
            let previous = out.iter().rev().find_map(|(_, t)| played(t).map(|_| t));
            match received(touch, previous, symbols) {
                Some((name, told)) => {
                    mapped.push(name);
                    out.extend(told.into_iter().map(|t| (*line, t)));
                }
                None => out.push((*line, touch.clone())),
            }
            (out, mapped)
        },
    )
}

/// What the touch is, a freeball received or a reception, after the touch
/// of a player before it: the decision and the touches it's told as; none
/// when it's as the file writes it.
fn received(
    touch: &Touch,
    previous: Option<&Touch>,
    symbols: &WinningSymbols,
) -> Option<(&'static str, Vec<Touch>)> {
    let (side, code) = match touch {
        Touch::Skill(side, code) => Some((*side, code)),
        _ => None,
    }?;
    let (before, skill) = previous.and_then(played).map(|(s, c, _)| (s, c.skill))?;
    let ball = previous.and_then(|t| ball(symbols, t))?;
    let dig = || vec![with(touch, Some(Skill::Dig), None)];
    // an error receiving it: the error of the team stays, after the
    // freeball sent
    let error = ending(symbols, code);
    let sent_by = |team: Side| match error {
        true => vec![sent(team), touch.clone()],
        false => vec![sent(team)],
    };
    match (code.skill, before == side, skill, ball) {
        (_, _, _, Ball::Ended) => None,
        (Skill::Reception, false, Skill::Attack, _) => {
            Some(("R after the other team's attack: D", dig()))
        }
        (Skill::Freeball, false, Skill::Attack, _) => {
            Some(("F after the other team's attack: D", dig()))
        }
        (Skill::Freeball, false, Skill::Block, Ball::Other) => {
            Some(("F after the other team's block !: the cover D", dig()))
        }
        (Skill::Freeball, false, Skill::Serve, _) => None,
        (Skill::Freeball, false, _, Ball::Other) if error => None,
        (Skill::Freeball, false, _, Ball::Other) => {
            Some(("F after the ball sent over: left out", vec![]))
        }
        (Skill::Freeball, false, _, Ball::Own) => Some((
            "F received: the freeball the other team sends",
            sent_by(before),
        )),
        (Skill::Freeball, true, Skill::Block | Skill::Attack, Ball::Own) => {
            Some(("F after the team's own block or attack !: D", dig()))
        }
        // a pass of the team's own ball (its second touch): over its
        // freeball, an error its fault, otherwise left out as a set
        (Skill::Freeball, true, _, Ball::Own) => match (code.evaluation, error) {
            ('/', _) => Some(("F of the team's own ball over: FB", vec![sent(side)])),
            (_, true) => None,
            (_, false) => Some(("F of the team's own ball: left out", vec![])),
        },
        (Skill::Freeball, true, _, Ball::Other) => Some((
            "F received: the freeball the other team sends",
            sent_by(side.other()),
        )),
        _ => None,
    }
}

/// The serve the reception tells: `#` the reception error, `/` the
/// reception over, `+` the reception `-`, `!` the `!`, `-` the reception
/// `+` or `#`.
fn serve_of(reception: char) -> Option<char> {
    match reception {
        '=' => Some('#'),
        '/' => Some('/'),
        '-' => Some('+'),
        '!' => Some('!'),
        '+' | '#' => Some('-'),
        _ => None,
    }
}

/// The attack the dig of the other team tells (its `!` a `+` already).
fn attack_dug(dig: char) -> Option<char> {
    match dig {
        '=' => Some('#'),
        '#' | '+' => Some('-'),
        '-' | '/' => Some('+'),
        _ => None,
    }
}

/// The attack the block tells, none after an invasion (left out).
fn attack_blocked(block: char) -> Option<Option<char>> {
    match block {
        '#' => Some(Some('/')),
        '=' => Some(Some('#')),
        '+' => Some(Some('-')),
        '-' => Some(Some('+')),
        '!' => Some(Some('!')),
        '/' => Some(None),
        _ => None,
    }
}

/// What a decision does to a touch of the rally.
enum Change {
    Evaluation(char),
    Out,
}

/// The evaluations of the rally as the answering touches decide them: the
/// blocks in play first (by who plays next), then the attacks (by the dig
/// or the block answering them, or the cover after them), the serves (by
/// the reception).
fn evaluations<'a>(
    touches: Vec<Lined<'a>>,
    symbols: &WinningSymbols,
) -> (Vec<Lined<'a>>, Vec<&'static str>) {
    let mut mapped = vec![];
    let mut touches = touches;
    // a dig `!` of the other team's attack, or after the team's own block:
    // a `+` (it lets them build)
    let digs: Vec<usize> = (0..touches.len())
        .filter(|i| {
            let previous = touches
                .get(..*i)
                .unwrap_or_default()
                .iter()
                .rev()
                .find_map(|(_, t)| played(t));
            match (touches.get(*i).and_then(|(_, t)| played(t)), previous) {
                (Some((side, code, false)), Some((before, answered, false))) => {
                    code.skill == Skill::Dig
                        && code.evaluation == '!'
                        && matches!(
                            (before == side, answered.skill),
                            (false, Skill::Attack) | (true, Skill::Block)
                        )
                }
                _ => false,
            }
        })
        .collect();
    for i in digs {
        if let Some((_, t)) = touches.get_mut(i) {
            *t = with(t, None, Some('+'));
            mapped.push("D! of an attack or after a block: D+");
        }
    }
    // a block touching the other team's kill (`A# B-`, the ball down all
    // the same: DataVolley credits the kill) is a block `=`: the kill
    // stays, the dig after it is the blockers' own
    let touching: Vec<usize> = (0..touches.len())
        .filter(|i| {
            let previous = touches
                .get(..*i)
                .unwrap_or_default()
                .iter()
                .rev()
                .find_map(|(_, t)| played(t));
            match (touches.get(*i).and_then(|(_, t)| played(t)), previous) {
                (Some((side, block, false)), Some((before, attack, false))) => {
                    block.skill == Skill::Block
                        && matches!(block.evaluation, '+' | '-')
                        && before != side
                        && attack.skill == Skill::Attack
                        && symbols.winning(Skill::Attack).contains(&attack.evaluation)
                }
                _ => false,
            }
        })
        .collect();
    for i in touching {
        if let Some((_, t)) = touches.get_mut(i) {
            *t = with(t, None, Some('='));
            mapped.push("B touching a kill: B=");
        }
    }
    let blocks = changes(&touches, |touch, next| match (touch, next) {
        (Touch::Skill(side, block), next)
            if block.skill == Skill::Block && !ending(symbols, block) =>
        {
            let evaluation = match next {
                Some(Touch::Skill(s, dig)) if s == side && dig.skill == Skill::Dig => {
                    match dig.evaluation {
                        '#' | '+' => Some('+'),
                        _ => Some('-'),
                    }
                }
                Some(t) => match played(t) {
                    Some((s, _, _)) if s == *side => Some('+'),
                    Some(_) => Some('!'),
                    None => None,
                },
                None => None,
            };
            evaluation
                .filter(|e| *e != block.evaluation)
                .map(|e| ("B in play decided by who plays next", Change::Evaluation(e)))
        }
        _ => None,
    });
    let touches = apply(touches, blocks, &mut mapped);
    // a touch ending the rally is never decided by an answer in play: that
    // comes after the end (their block touching our kill, the ball down all
    // the same)
    let after_the_end = |answered: &SkillCode, answer: &SkillCode| {
        ending(symbols, answered) && !ending(symbols, answer)
    };
    let attacks = changes(&touches, |touch, next| match (touch, next) {
        (Touch::Skill(side, attack), Some(Touch::Skill(s, answer)))
            if attack.skill == Skill::Attack && !(s != side && after_the_end(attack, answer)) =>
        {
            match (s == side, answer.skill) {
                // the attack error before the invasion: the error first
                (false, Skill::Block) if attack.evaluation == '=' && answer.evaluation == '/' => {
                    None
                }
                (false, Skill::Block) => {
                    attack_blocked(answer.evaluation).and_then(|told| match told {
                        Some(e) if e == attack.evaluation => None,
                        Some(e) => Some(("A decided by the block", Change::Evaluation(e))),
                        None => Some(("A before an invasion: left out", Change::Out)),
                    })
                }
                (false, Skill::Dig) => attack_dug(answer.evaluation)
                    .filter(|e| *e != attack.evaluation)
                    .map(|e| ("A decided by the dig", Change::Evaluation(e))),
                (true, Skill::Dig) if !ending(symbols, attack) && attack.evaluation != '!' => {
                    Some(("A covered by the team: A!", Change::Evaluation('!')))
                }
                _ => None,
            }
        }
        (Touch::Skill(side, serve), Some(Touch::Skill(s, reception)))
            if serve.skill == Skill::Serve
                && reception.skill == Skill::Reception
                && s != side
                && !after_the_end(serve, reception) =>
        {
            serve_of(reception.evaluation)
                .filter(|e| *e != serve.evaluation)
                .map(|e| ("S decided by the reception", Change::Evaluation(e)))
        }
        _ => None,
    });
    let touches = apply(touches, attacks, &mut mapped);
    (touches, mapped)
}

/// The changes of the touches of a player, each told by the touch and the
/// next touch of a player.
fn changes(
    touches: &[Lined],
    change: impl Fn(&Touch, Option<&Touch>) -> Option<(&'static str, Change)>,
) -> Vec<(usize, &'static str, Change)> {
    touches
        .iter()
        .enumerate()
        .filter_map(|(i, (_, touch))| {
            let next = next_played(touches, i).and_then(|n| touches.get(n).map(|(_, t)| t));
            change(touch, next).map(|(name, c)| (i, name, c))
        })
        .collect()
}

fn apply<'a>(
    touches: Vec<Lined<'a>>,
    changes: Vec<(usize, &'static str, Change)>,
    mapped: &mut Vec<&'static str>,
) -> Vec<Lined<'a>> {
    mapped.extend(changes.iter().map(|(_, name, _)| *name));
    touches
        .into_iter()
        .enumerate()
        .filter_map(
            |(i, (line, touch))| match changes.iter().find(|(c, _, _)| *c == i) {
                Some((_, _, Change::Out)) => None,
                Some((_, _, Change::Evaluation(e))) => Some((line, with(&touch, None, Some(*e)))),
                None => Some((line, touch)),
            },
        )
        .collect()
}

/// Whether the touch answers the touch before it, of the other team: the
/// reception the serve, the dig or the block the attack.
fn answers(touch: &SkillCode, before: &SkillCode) -> bool {
    matches!(
        (before.skill, touch.skill),
        (Skill::Serve, Skill::Reception) | (Skill::Attack, Skill::Dig | Skill::Block)
    )
}

/// The freeballs sent untold, where the ball goes over without a touch
/// sending it, at the line of the touch after them; or the first touch the
/// rally can't go on with. Up to the end of the rally.
fn untold<'a>(
    touches: Vec<Lined<'a>>,
    symbols: &WinningSymbols,
) -> Result<(Vec<Lined<'a>>, Vec<&'static str>), (usize, Issue)> {
    let mut out: Vec<Lined<'a>> = vec![];
    let mut mapped = vec![];
    for (line, touch) in touches {
        let previous = out
            .iter()
            .rev()
            .find_map(|(l, t)| played(t).map(|p| (l.line, p, ball(symbols, t))));
        // after the end what the file adds is up to the translation
        let over = out
            .iter()
            .any(|(_, t)| ball(symbols, t) == Some(Ball::Ended));
        let now = played(&touch).filter(|_| !over);
        let insert = match (previous, now) {
            (Some((before_line, (before, b, _), Some(where_))), Some((side, code, _))) => {
                let impossible = |what: fn(Touch, usize, Touch) -> Impossible| {
                    Err((
                        line.line,
                        Issue::Impossible(what(
                            Touch::Skill(before, b.clone()),
                            before_line,
                            Touch::Skill(side, code.clone()),
                        )),
                    ))
                };
                // the invasion (`B/`): a fault, answering nothing
                let fault = code.skill == Skill::Block && code.evaluation == '/';
                let dig_or_block = matches!(code.skill, Skill::Dig | Skill::Block);
                let over = matches!(b.skill, Skill::Reception | Skill::Dig) && b.evaluation == '/';
                let open_roads = b.skill == Skill::Attack && b.evaluation == '+';
                let keeps_to_dig = code.skill == Skill::Dig
                    && matches!(
                        (b.skill, b.evaluation),
                        (Skill::Block, _) | (Skill::Attack, '!')
                    );
                match (where_, before == side) {
                    _ if fault => Ok(None),
                    (Ball::Ended, _) => Ok(None),
                    // the team's dig after its own block, its cover after
                    // its attack `!`
                    (Ball::Own, true) if dig_or_block && !keeps_to_dig => {
                        impossible(missing_attack)
                    }
                    (Ball::Own, true) => Ok(None),
                    (Ball::Other, false) => Ok(None),
                    (Ball::Own, false) if answers(code, b) => Ok(None),
                    (Ball::Own, false) if dig_or_block => impossible(missing_attack),
                    (Ball::Own, false) => Ok(Some(before)),
                    (Ball::Other, true) if over => impossible(touch_after_over),
                    (Ball::Other, true) if dig_or_block => impossible(missing_attack),
                    (Ball::Other, true) if open_roads => Ok(None),
                    (Ball::Other, true) => Ok(Some(before.other())),
                }
            }
            _ => Ok(None),
        }?;
        if let Some(side) = insert {
            mapped.push("freeball sent untold: FB");
            out.push((line, sent(side)));
        }
        out.push((line, touch));
    }
    Ok((out, mapped))
}

fn missing_attack(before: Touch, before_line: usize, touch: Touch) -> Impossible {
    Impossible::MissingAttack {
        before: code_of(&before),
        before_line,
        touch: code_of(&touch),
    }
}

fn touch_after_over(over: Touch, over_line: usize, touch: Touch) -> Impossible {
    Impossible::TouchAfterOver {
        over: code_of(&over),
        over_line,
        touch: code_of(&touch),
    }
}

/// The skill and the evaluation of a touch, as the file writes them.
fn code_of(touch: &Touch) -> (Skill, char) {
    match touch {
        Touch::Skill(_, code) | Touch::Sent(_, code) => (code.skill, code.evaluation),
        Touch::Green(_, e) => (Skill::Freeball, *e),
    }
}
