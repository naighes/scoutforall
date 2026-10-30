//! A set of DataVolley as one of ours, for one of the two teams: rally by
//! rally, our touches become our events and the touches of the opponent
//! only what ends a rally (`OS`, `OE`), all validated by our `Snapshot` as
//! the scouting would. Nothing is guessed: a rally that doesn't translate
//! exactly, a lineup different from the six on court of the file, a score
//! different from the file's stop the set.

use crate::{
    dvw::model::{Code, DvwFile, ScoutLine, Side, Skill, SkillCode, WinningSymbols},
    shapes::{
        enums::{EvalEnum, EventTypeEnum, TeamSideEnum},
        flags::EventFlags,
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        technique::{ServeTypeEnum, TechniqueEnum},
    },
};
use chrono::{DateTime, NaiveDate, NaiveTime, TimeDelta, Utc};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

/// Why a set doesn't translate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issue {
    /// The set isn't in the file.
    NoSet,
    NoLineup,
    NoServe,
    NoSetter,
    NoLibero,
    /// A number on court or touching the ball not among the players.
    UnknownPlayer(u8),
    UnknownCode(String),
    /// The six on court of the file (numbers, zones 1–6) and ours.
    LineupMismatch {
        file: [u8; 6],
        ours: [u8; 6],
    },
    /// The file ends the rally with an evaluation that doesn't end it for
    /// us, or the other way round: a table of winning symbols not ours.
    Ending {
        skill: Skill,
        evaluation: char,
    },
    /// A rally the file tells in a way we can't: what's missing.
    Untranslatable(&'static str),
    /// Our snapshot refuses the event.
    Event(String),
    /// The score after a rally (us, them): the file's and ours.
    Score {
        file: (u8, u8),
        ours: (u8, u8),
    },
    /// The file ends the set before one of the teams wins it.
    Unfinished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslateError {
    pub set_number: u8,
    /// The line of the file, when there's one to point at.
    pub line: Option<usize>,
    pub issue: Issue,
}

/// The evaluations of the file mapped to ours by what follows them, which
/// our scale tells apart differently: counted, for the summary.
pub type Mappings = BTreeMap<&'static str, usize>;

/// Our team in the file: its side and its players by number.
pub struct Us<'a> {
    pub side: Side,
    pub players: &'a HashMap<u8, Uuid>,
    /// The numbers of the liberos.
    pub liberos: &'a [u8],
}

/// The set, with its events, and the mappings used.
pub fn translate_set(
    file: &DvwFile,
    us: &Us,
    set_number: u8,
    date: NaiveDate,
    max_substitutions: u8,
) -> Result<(SetEntry, Mappings), TranslateError> {
    let fail = |line: Option<usize>, issue: Issue| TranslateError {
        set_number,
        line,
        issue,
    };
    let lines = set_lines(&file.scout, set_number);
    let start = set_start(lines, us, set_number).map_err(|(line, issue)| fail(line, issue))?;
    let snapshot = Snapshot::new(&start, max_substitutions)
        .map_err(|e| fail(None, Issue::Event(e.to_string())))?;
    let mut set = SetTranslator {
        us,
        symbols: file.winning_symbols.clone().unwrap_or_else(default_symbols),
        snapshot,
        events: vec![],
        rally_start: 0,
        mappings: Mappings::new(),
        clock: date.and_time(NaiveTime::MIN).and_utc(),
        date,
    };
    set.translate(lines)
        .map_err(|(line, issue)| fail(line, issue))?;
    match set.snapshot.get_set_winner(set_number) {
        Some(_) => Ok((
            SetEntry {
                events: set.events,
                ..start
            },
            set.mappings,
        )),
        None => Err(fail(None, Issue::Unfinished)),
    }
}

// the lines of a set: between the ends of set (`**1set`)
fn set_lines(scout: &[ScoutLine], set_number: u8) -> &[ScoutLine] {
    let ends: Vec<usize> = scout
        .iter()
        .enumerate()
        .filter_map(|(i, l)| matches!(l.code, Code::EndOfSet(_)).then_some(i))
        .collect();
    let index = usize::from(set_number.saturating_sub(1));
    let from = match index {
        0 => Some(0),
        _ => ends.get(index - 1).map(|i| i + 1),
    };
    let to = ends.get(index).copied().unwrap_or(scout.len());
    from.and_then(|from| scout.get(from..to))
        .unwrap_or_default()
}

type Failure = (Option<usize>, Issue);

// the six on court at the first serve, the setter of the lineup, the team
// serving first and the liberos: the first one touching the ball starts
fn set_start(lines: &[ScoutLine], us: &Us, set_number: u8) -> Result<SetEntry, Failure> {
    let first_serve = lines
        .iter()
        .find_map(|l| match &l.code {
            Code::Skill(side, skill) if skill.skill == Skill::Serve => Some((l, *side)),
            _ => None,
        })
        .ok_or((None, Issue::NoServe))?;
    let (serve_line, serving) = first_serve;
    let court = court(serve_line, us.side).ok_or((Some(serve_line.line), Issue::NoLineup))?;
    let positions = court
        .iter()
        .map(|n| player(us, *n).map_err(|issue| (Some(serve_line.line), issue)))
        .collect::<Result<Vec<Uuid>, Failure>>()?;
    let setter = lines
        .iter()
        .find_map(|l| match l.code {
            Code::Setter { side, player, .. } if side == us.side => Some(player),
            _ => None,
        })
        .ok_or((None, Issue::NoSetter))
        .and_then(|n| player(us, n).map_err(|issue| (None, issue)))?;
    let touching: Vec<u8> = lines
        .iter()
        .filter_map(|l| match &l.code {
            Code::Skill(side, skill) if *side == us.side => skill.player,
            _ => None,
        })
        .filter(|n| us.liberos.contains(n))
        .collect();
    let mut liberos: Vec<u8> =
        touching
            .iter()
            .chain(us.liberos)
            .copied()
            .fold(vec![], |mut all, n| {
                if !all.contains(&n) {
                    all.push(n);
                }
                all
            });
    liberos.truncate(2);
    let libero = liberos
        .first()
        .ok_or((None, Issue::NoLibero))
        .and_then(|n| player(us, *n).map_err(|issue| (None, issue)))?;
    let fallback = liberos
        .get(1)
        .map(|n| player(us, *n).map_err(|issue| (None, issue)))
        .transpose()?;
    let positions: [Uuid; 6] = positions
        .try_into()
        .map_err(|_| (Some(serve_line.line), Issue::NoLineup))?;
    let serving_team = match serving == us.side {
        true => TeamSideEnum::Us,
        false => TeamSideEnum::Them,
    };
    SetEntry::new(
        set_number,
        serving_team,
        positions,
        libero,
        fallback,
        setter,
    )
    .map_err(|e| (Some(serve_line.line), Issue::Event(e.to_string())))
}

fn court(line: &ScoutLine, side: Side) -> Option<[u8; 6]> {
    match side {
        Side::Home => line.home_court,
        Side::Visiting => line.visiting_court,
    }
}

fn player(us: &Us, number: u8) -> Result<Uuid, Issue> {
    us.players
        .get(&number)
        .copied()
        .ok_or(Issue::UnknownPlayer(number))
}

/// The symbols of DataVolley when the file has none (Click&Scout).
fn default_symbols() -> WinningSymbols {
    WinningSymbols {
        skills: vec![
            (Skill::Serve, vec!['='], vec!['#']),
            (Skill::Reception, vec!['='], vec![]),
            (Skill::Attack, vec!['=', '/'], vec!['#']),
            (Skill::Block, vec!['=', '/'], vec!['#']),
            (Skill::Dig, vec!['='], vec![]),
            (Skill::Set, vec!['='], vec![]),
            (Skill::Freeball, vec!['='], vec![]),
        ],
    }
}

/// How a touch ends the rally for the team who played it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ending {
    Won,
    Lost,
    Goes,
}

/// Who plays the ball after a touch of ours, up to our next touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum After {
    /// we do, at once
    Kept,
    /// we do, sending a freeball: their freeball received (`F`)
    OurFreeball,
    /// they do, attacking
    TheyAttack,
    /// they do, and send it back without attacking: we receive their
    /// freeball (`F`)
    TheirFreeball,
    /// they do, and send it back without attacking (e.g. their dig over)
    BackWithoutAttack,
    /// they do, and the rally ends on their side without their attack
    /// (e.g. their error)
    Ended,
}

/// A touch of the rally: of a player, or a point without one (`$$&`).
#[derive(Debug, Clone, Copy)]
enum Touch<'a> {
    Skill(Side, &'a SkillCode),
    Green(Side, char),
}

struct SetTranslator<'a> {
    us: &'a Us<'a>,
    symbols: WinningSymbols,
    snapshot: Snapshot,
    events: Vec<EventEntry>,
    // where the rally being translated starts in `events`
    rally_start: usize,
    mappings: Mappings,
    clock: DateTime<Utc>,
    date: NaiveDate,
}

impl SetTranslator<'_> {
    fn translate(&mut self, lines: &[ScoutLine]) -> Result<(), Failure> {
        let mut rally: Vec<(&ScoutLine, Touch)> = vec![];
        for line in lines {
            let at = Some(line.line);
            self.tick(line);
            match &line.code {
                Code::Skill(side, skill) => {
                    if rally.is_empty() {
                        self.check_lineup(line)?;
                    }
                    rally.push((line, Touch::Skill(*side, skill)));
                }
                Code::Green(side, evaluation) => {
                    rally.push((line, Touch::Green(*side, *evaluation)))
                }
                Code::Point(side, home, visiting) => {
                    let file_score = match self.us.side {
                        Side::Home => (*home, *visiting),
                        Side::Visiting => (*visiting, *home),
                    };
                    let winner = match *side == self.us.side {
                        true => TeamSideEnum::Us,
                        false => TeamSideEnum::Them,
                    };
                    self.rally(&rally, winner).map_err(|issue| (at, issue))?;
                    let ours = (self.snapshot.score_us, self.snapshot.score_them);
                    match ours == file_score {
                        true => rally.clear(),
                        false => {
                            return Err((
                                at,
                                Issue::Score {
                                    file: file_score,
                                    ours,
                                },
                            ));
                        }
                    }
                }
                Code::Substitution { side, out, enters } if *side == self.us.side => {
                    match rally.is_empty() {
                        true => {
                            let (out, enters) = (self.player(*out, at)?, self.player(*enters, at)?);
                            self.apply(EventTypeEnum::R, Some(out), None, Some(enters), None)
                                .map_err(|issue| (at, issue))?;
                        }
                        false => {
                            return Err((at, Issue::Untranslatable("substitution in a rally")));
                        }
                    }
                }
                Code::Setter {
                    side,
                    player,
                    lineup: false,
                } if *side == self.us.side => {
                    let setter = self.player(*player, at)?;
                    let current = self.snapshot.current_lineup.get_setter();
                    match (current == Some(setter), rally.is_empty()) {
                        (true, _) => {}
                        (false, true) => self
                            .apply(EventTypeEnum::CS, Some(setter), None, None, None)
                            .map_err(|issue| (at, issue))?,
                        (false, false) => {
                            return Err((at, Issue::Untranslatable("setter change in a rally")));
                        }
                    }
                }
                Code::Unknown(text) => return Err((at, Issue::UnknownCode(text.clone()))),
                // the other team's changes, the zones of the setters (the
                // six on court tell them), timeouts, notes (a rotation
                // fault or a card comes with its `$$&`)
                _ => {}
            }
        }
        Ok(())
    }

    // the time of the line, or a second after the previous event
    fn tick(&mut self, line: &ScoutLine) {
        let time = line
            .time
            .as_deref()
            .and_then(|t| NaiveTime::parse_from_str(&t.replace('.', ":"), "%H:%M:%S").ok())
            .map(|t| self.date.and_time(t).and_utc());
        self.clock = match time {
            Some(t) if t > self.clock => t,
            _ => self.clock + TimeDelta::seconds(1),
        };
    }

    fn player(&self, number: u8, at: Option<usize>) -> Result<Uuid, Failure> {
        player(self.us, number).map_err(|issue| (at, issue))
    }

    // at the serve: the six on court of the file are ours, the libero
    // aside (the file writes the player it replaces)
    fn check_lineup(&self, line: &ScoutLine) -> Result<(), Failure> {
        let at = Some(line.line);
        let file = court(line, self.us.side).ok_or((at, Issue::NoLineup))?;
        let numbers: HashMap<Uuid, u8> = self.us.players.iter().map(|(n, id)| (*id, *n)).collect();
        let ours = self
            .snapshot
            .current_lineup
            .get_positions_without_libero()
            .map(|id| numbers.get(&id).copied().unwrap_or_default());
        match file == ours {
            true => Ok(()),
            false => Err((at, Issue::LineupMismatch { file, ours })),
        }
    }

    fn apply(
        &mut self,
        event_type: EventTypeEnum,
        player: Option<Uuid>,
        eval: Option<EvalEnum>,
        target_player: Option<Uuid>,
        technique: Option<TechniqueEnum>,
    ) -> Result<(), Issue> {
        let event = EventEntry {
            timestamp: self.clock,
            event_type,
            player,
            eval,
            target_player,
            technique,
            flags: EventFlags::default(),
        };
        self.snapshot
            .add_event(&event)
            .map_err(|e| Issue::Event(e.to_string()))?;
        self.events.push(event);
        Ok(())
    }

    fn mapped(&mut self, name: &'static str) {
        *self.mappings.entry(name).or_default() += 1;
    }

    fn ending(&self, skill: Skill, evaluation: char) -> Ending {
        match (
            self.symbols.winning(skill).contains(&evaluation),
            self.symbols.losing(skill).contains(&evaluation),
        ) {
            (true, _) => Ending::Won,
            (_, true) => Ending::Lost,
            _ => Ending::Goes,
        }
    }

    // the events of the rally being translated
    fn rally_events(&self) -> &[EventEntry] {
        self.events.get(self.rally_start..).unwrap_or_default()
    }

    fn rally_over(&self) -> bool {
        self.rally_events()
            .iter()
            .any(|e| e.event_type.rally_winner(e.eval).is_some())
    }

    /// The rally from the serve to the point: our touches as our events,
    /// then, if it's not over, what ended it on their side.
    fn rally(
        &mut self,
        touches: &[(&ScoutLine, Touch)],
        winner: TeamSideEnum,
    ) -> Result<(), Issue> {
        self.rally_start = self.events.len();
        self.change_libero(touches)?;
        self.libero_in_or_out(touches)?;
        let mut last_cause: Option<(Side, Ending)> = None;
        // a point without a touch: the cause when no touch is, otherwise
        // the other side of the same point (`a07AH#`, then `*$$&H=`)
        let mut green: Option<(Side, Ending)> = None;
        for (index, (_, touch)) in touches.iter().enumerate() {
            let next = touches.get(index + 1..).unwrap_or_default();
            match (self.rally_over(), touch) {
                // what the file adds after the end: the other side of the
                // same point, e.g. their reception error after our ace
                (true, Touch::Skill(side, skill)) if *side != self.us.side => {
                    match self.ending(skill.skill, skill.evaluation) {
                        Ending::Goes => {
                            return Err(Issue::Untranslatable("their touch after the end"));
                        }
                        Ending::Won | Ending::Lost => {}
                    }
                }
                (true, Touch::Green(_, _)) => {}
                (true, _) => return Err(Issue::Untranslatable("our touch after the end")),
                (false, Touch::Skill(side, skill)) if *side == self.us.side => {
                    let before = touches.get(..index).unwrap_or_default();
                    self.our_touch(skill, before, next)?;
                    last_cause = None;
                }
                (false, Touch::Skill(side, skill)) => {
                    last_cause = self.their_touch(*side, skill, next)?.or(last_cause);
                }
                (false, Touch::Green(side, evaluation)) => {
                    let ending = match evaluation {
                        '#' => Ending::Won,
                        _ => Ending::Lost,
                    };
                    green = Some((*side, ending));
                }
            }
        }
        match (self.rally_over(), winner, last_cause.or(green)) {
            (true, _, _) => Ok(()),
            // their point: their ace, kill or block point, or their point
            // without a touch
            (false, TeamSideEnum::Them, Some((side, Ending::Won))) if side != self.us.side => {
                self.apply(EventTypeEnum::OS, None, None, None, None)
            }
            // ours: their error, or our point without a touch (their fault)
            (false, TeamSideEnum::Us, Some((side, Ending::Lost))) if side != self.us.side => {
                self.apply(EventTypeEnum::OE, None, None, None, None)
            }
            (false, TeamSideEnum::Us, Some((side, Ending::Won))) if side == self.us.side => {
                self.mapped("our point without a touch: OE");
                self.apply(EventTypeEnum::OE, None, None, None, None)
            }
            // our fault without a player: the team's (e.g. a rotation fault)
            (false, TeamSideEnum::Them, Some((side, Ending::Lost))) if side == self.us.side => {
                self.apply(EventTypeEnum::F, None, None, None, None)
            }
            (false, _, _) => Err(Issue::Untranslatable("a point without its cause")),
        }
    }

    // the libero not on court touching the ball: changed before the rally
    fn change_libero(&mut self, touches: &[(&ScoutLine, Touch)]) -> Result<(), Issue> {
        let fallback = self.snapshot.current_lineup.get_fallback_libero();
        let touches_fallback = touches.iter().any(|(_, t)| match t {
            Touch::Skill(side, skill) if *side == self.us.side => skill
                .player
                .and_then(|n| self.us.players.get(&n))
                .is_some_and(|id| Some(*id) == fallback),
            _ => false,
        });
        match touches_fallback {
            true => self.apply(EventTypeEnum::CL, None, None, None, None),
            false => Ok(()),
        }
    }

    // the libero out when the player it replaces touches the ball (the file
    // writes the six without it, so only the touches tell), back in
    // otherwise, as it plays most of the time; both touching, it can't be
    fn libero_in_or_out(&mut self, touches: &[(&ScoutLine, Touch)]) -> Result<(), Issue> {
        let lineup = &self.snapshot.current_lineup;
        let out = lineup.is_libero_out();
        // the lineup with the libero in, to tell who it replaces
        let mut with_libero = lineup.clone();
        if out {
            with_libero
                .toggle_libero()
                .map_err(|e| Issue::Event(e.to_string()))?;
        }
        let libero = with_libero.get_current_libero();
        let ours: Vec<Uuid> = touches
            .iter()
            .filter_map(|(_, t)| match t {
                Touch::Skill(side, skill) if *side == self.us.side => skill.player,
                _ => None,
            })
            .filter_map(|n| self.us.players.get(&n).copied())
            .collect();
        let libero_touches = ours.contains(&libero);
        let replaced_touches = ours.iter().any(|id| with_libero.is_out_for_the_libero(id));
        match (libero_touches, replaced_touches, out) {
            (true, true, _) => Err(Issue::Untranslatable(
                "the libero and the player it replaces both touch",
            )),
            (false, true, false) | (_, false, true) => {
                self.apply(EventTypeEnum::LR, None, None, None, None)
            }
            _ => Ok(()),
        }
    }

    /// Who plays the ball after a touch of ours, up to our next touch: our
    /// evaluations tell it, the file's tell how good the touch was.
    fn after(&self, next: &[(&ScoutLine, Touch)]) -> After {
        let skills: Vec<(Side, Skill)> = next
            .iter()
            .filter_map(|(_, t)| match t {
                Touch::Skill(side, skill) => Some((*side, skill.skill)),
                Touch::Green(_, _) => None,
            })
            .collect();
        let ours = |side: &Side| *side == self.us.side;
        let theirs: Vec<Skill> = skills
            .iter()
            .take_while(|(side, _)| !ours(side))
            .map(|(_, skill)| *skill)
            .collect();
        let our_next = skills
            .iter()
            .find(|(side, _)| ours(side))
            .map(|(_, skill)| *skill);
        match (theirs.first(), theirs.contains(&Skill::Attack), our_next) {
            // their freeball received: we sent it
            (Some(Skill::Freeball), _, _) => After::OurFreeball,
            (_, true, _) => After::TheyAttack,
            (None, _, Some(Skill::Freeball)) | (Some(_), _, Some(Skill::Freeball)) => {
                After::TheirFreeball
            }
            // our block next: they attacked, not in the file
            (None, _, Some(Skill::Block)) => After::TheyAttack,
            (None, _, Some(_)) => After::Kept,
            (Some(_), _, Some(_)) => After::BackWithoutAttack,
            (_, _, None) => After::Ended,
        }
    }

    fn our_touch(
        &mut self,
        skill: &SkillCode,
        before: &[(&ScoutLine, Touch)],
        next: &[(&ScoutLine, Touch)],
    ) -> Result<(), Issue> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        // the freeball received after their attack: an easy attack, our dig
        let they_attacked = before
            .iter()
            .rev()
            .filter_map(|(_, t)| match t {
                Touch::Skill(side, s) => Some((*side, s.skill)),
                Touch::Green(_, _) => None,
            })
            .take_while(|(side, _)| *side != self.us.side)
            .any(|(_, skill)| skill == Skill::Attack);
        let kind = match (skill.skill, they_attacked) {
            (Skill::Freeball, true) => {
                self.mapped("F after their attack as D");
                Skill::Dig
            }
            (kind, _) => kind,
        };
        let number = skill
            .player
            .ok_or(Issue::Untranslatable("our touch without a player"))?;
        let player = player(self.us, number)?;
        let ending = self.ending(skill.skill, skill.evaluation);
        let touched = self
            .rally_events()
            .iter()
            .any(|e| e.event_type.ball_side_after(e.eval).is_some());
        let ball_ours =
            self.snapshot.get_ball_side() == TeamSideEnum::Us && touched && !self.rally_over();
        let after = self.after(next);
        let last = self.rally_events().last().map(|e| (e.event_type, e.eval));
        let event = match (kind, ending, skill.evaluation, ball_ours, after) {
            // the ending touches: as the file says, `/` blocked or invasion
            (Skill::Serve, Ending::Won, _, _, _) => Some((S, Some(Perfect))),
            (Skill::Serve, Ending::Lost, _, _, _) => Some((S, Some(Error))),
            (Skill::Attack, Ending::Won, _, _, _) => Some((A, Some(Perfect))),
            (Skill::Attack, Ending::Lost, '/', _, _) => Some((A, Some(Over))),
            (Skill::Attack, Ending::Lost, _, _, _) => Some((A, Some(Error))),
            (Skill::Block, Ending::Won, _, _, _) => Some((B, Some(Perfect))),
            (Skill::Block, Ending::Lost, '/', _, _) => Some((B, Some(Over))),
            (Skill::Block, Ending::Lost, _, _, _) => Some((B, Some(Error))),
            (Skill::Reception, Ending::Lost, _, _, _) => Some((P, Some(Error))),
            (Skill::Dig, Ending::Lost, _, false, _) => Some((D, Some(Error))),
            // a dig with the ball on our side covers our attack: an error
            // is our fault, a ball sent over our freeball
            (Skill::Dig, Ending::Lost, _, true, _) => {
                self.mapped("cover error: F");
                Some((F, None))
            }
            (Skill::Set | Skill::Freeball, Ending::Lost, _, _, _) => {
                self.mapped("set or freeball error: F");
                Some((F, None))
            }
            (_, Ending::Won, _, _, _) => {
                return Err(Issue::Ending {
                    skill: skill.skill,
                    evaluation: skill.evaluation,
                });
            }
            // the serve: they attack (`/` is then our `+`), or the ball
            // comes back without their attack, their freeball too: our `/`
            (Skill::Serve, Ending::Goes, '/', _, After::TheyAttack) => {
                self.mapped("S/ attacked as S+");
                Some((S, Some(Positive)))
            }
            (Skill::Serve, Ending::Goes, e, _, After::TheyAttack | After::Ended) => {
                Some((S, Some(eval(e)?)))
            }
            // back to us, sent back as our freeball (their `F`) too
            (Skill::Serve, Ending::Goes, e, _, _) => {
                if e != '/' {
                    self.mapped("S not attacked as S/");
                }
                Some((S, Some(Over)))
            }
            // reception and dig: kept, `/` our `-`; played by them, our `/`
            (Skill::Reception, Ending::Goes, '/', _, After::Kept | After::OurFreeball) => {
                self.mapped("R/ kept as P-");
                Some((P, Some(Negative)))
            }
            (Skill::Reception, Ending::Goes, e, _, After::Kept | After::OurFreeball) => {
                Some((P, Some(eval(e)?)))
            }
            (Skill::Reception, Ending::Goes, e, _, _) => {
                if e != '/' {
                    self.mapped("R over as P/");
                }
                Some((P, Some(Over)))
            }
            // attack and block: they attack, our `-`; the ball back without
            // their attack, our `+` (`!` blocked and covered), their
            // freeball after it
            (Skill::Attack, Ending::Goes, '!', _, After::Kept | After::OurFreeball) => {
                Some((A, Some(Exclamative)))
            }
            (
                Skill::Attack | Skill::Block,
                Ending::Goes,
                e,
                _,
                After::Kept | After::OurFreeball | After::BackWithoutAttack | After::TheirFreeball,
            ) => {
                if e != '+' {
                    self.mapped("A or B back without their attack as +");
                }
                Some((attack_or_block(skill.skill), Some(Positive)))
            }
            (Skill::Attack | Skill::Block, Ending::Goes, e, _, _) => {
                if e != '-' {
                    self.mapped("A or B played by them as -");
                }
                Some((attack_or_block(skill.skill), Some(Negative)))
            }
            // a cover of our attack: not scouted, unless it sends the ball
            // over
            (Skill::Dig, Ending::Goes, '/', true, _) => {
                self.mapped("cover over the net: FB");
                Some((FB, None))
            }
            (Skill::Dig, Ending::Goes, _, true, _) => {
                self.mapped("cover skipped");
                None
            }
            (Skill::Dig, Ending::Goes, '/', false, After::Kept | After::OurFreeball) => {
                self.mapped("D/ kept as D-");
                Some((D, Some(Negative)))
            }
            (Skill::Dig, Ending::Goes, e, false, After::Kept | After::OurFreeball) => {
                Some((D, Some(eval(e)?)))
            }
            (Skill::Dig, Ending::Goes, e, false, _) => {
                if e != '/' {
                    self.mapped("D over as D/");
                }
                Some((D, Some(Over)))
            }
            // their freeball, received by us: told by our serve `/` already
            (Skill::Freeball, Ending::Goes, _, _, _) => match last {
                Some((S, Some(Over))) => {
                    self.mapped("their freeball after S/: in the serve");
                    None
                }
                _ => Some((OF, None)),
            },
            // the set: not scouted, unless it goes over without an attack,
            // our freeball (theirs received tells it already)
            (Skill::Set, Ending::Goes, _, _, After::Kept | After::OurFreeball | After::Ended) => {
                None
            }
            (Skill::Set, Ending::Goes, _, _, _) => {
                self.mapped("set over: FB");
                Some((FB, None))
            }
        };
        // an evaluation ending the rally for us and not in the file
        let ends = event.is_some_and(|(t, e)| t.rally_winner(e).is_some());
        if ends && ending == Ending::Goes {
            return Err(Issue::Ending {
                skill: skill.skill,
                evaluation: skill.evaluation,
            });
        }
        match event {
            Some((event_type @ (OF | FB), _)) => self.apply(event_type, None, None, None, None),
            Some((event_type, eval)) => {
                let technique = match event_type {
                    S => serve_type(skill.skill_type).map(TechniqueEnum::Serve),
                    _ => None,
                };
                self.apply(event_type, Some(player), eval, None, technique)
            }
            None => Ok(()),
        }
    }

    /// Their touch: what it says about the end of the rally, for the
    /// closing event; their freeball received is our freeball (`FB`), a
    /// team event.
    fn their_touch(
        &mut self,
        side: Side,
        skill: &SkillCode,
        next: &[(&ScoutLine, Touch)],
    ) -> Result<Option<(Side, Ending)>, Issue> {
        let ending = self.ending(skill.skill, skill.evaluation);
        let block_point_next = next
            .iter()
            .find_map(|(_, t)| match t {
                Touch::Skill(side, skill) => Some((*side, *skill)),
                Touch::Green(_, _) => None,
            })
            .filter(|(side, _)| *side == self.us.side)
            .is_some_and(|(_, s)| {
                s.skill == Skill::Block && self.ending(s.skill, s.evaluation) == Ending::Won
            });
        match (skill.skill, ending, skill.evaluation) {
            // blocked by us: our block point must follow
            (Skill::Attack, Ending::Lost, '/') if !block_point_next => Err(Issue::Untranslatable(
                "their attack blocked without our block",
            )),
            (Skill::Freeball, Ending::Goes, _) => self
                .apply(EventTypeEnum::FB, None, None, None, None)
                .map(|_| None),
            (_, Ending::Goes, _) => Ok(None),
            (_, ending, _) => Ok(Some((side, ending))),
        }
    }
}

fn attack_or_block(skill: Skill) -> EventTypeEnum {
    match skill {
        Skill::Block => EventTypeEnum::B,
        _ => EventTypeEnum::A,
    }
}

fn eval(code: char) -> Result<EvalEnum, Issue> {
    match code {
        '#' => Ok(EvalEnum::Perfect),
        '+' => Ok(EvalEnum::Positive),
        '!' => Ok(EvalEnum::Exclamative),
        '-' => Ok(EvalEnum::Negative),
        '/' => Ok(EvalEnum::Over),
        '=' => Ok(EvalEnum::Error),
        _ => Err(Issue::Untranslatable("unknown evaluation")),
    }
}

// `H` float, `M` jump float, `Q` jump spin, `T` topspin
fn serve_type(code: char) -> Option<ServeTypeEnum> {
    match code {
        'H' | 'M' => Some(ServeTypeEnum::Float),
        'Q' | 'T' => Some(ServeTypeEnum::Spin),
        _ => None,
    }
}
