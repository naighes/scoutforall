//! A set of DataVolley as one of ours, for one of the two teams: rally by
//! rally, our touches become our events and the touches of the opponent
//! only what ends a rally (`OS`, `OE`), all validated by our `Snapshot` as
//! the scouting would. Nothing is guessed: a rally that doesn't translate
//! exactly, a lineup different from the six on court of the file, a score
//! different from the file's stop the set.

use crate::{
    dvw::model::{Code, DvwFile, ScoutLine, Side, Skill, SkillCode, WinningSymbols},
    shapes::{
        court_zone::{CourtTarget, SubzoneEnum},
        enums::{EvalEnum, EventTypeEnum, PhaseEnum, TeamSideEnum},
        flags::EventFlags,
        lineup::Lineup,
        refusal::Refusal,
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
    /// The file ends the rally with an evaluation that doesn't end it for
    /// us, or the other way round: a table of winning symbols not ours.
    Ending {
        skill: Skill,
        evaluation: char,
    },
    /// A rally the file tells in a way we can't: what's missing.
    Untranslatable(Untranslatable),
    /// Our scouting refuses the event, the players by number.
    Refused {
        event: EventTypeEnum,
        refusal: Refusal<u8>,
    },
    /// Our snapshot refuses the event for another reason: the technical
    /// detail.
    Event(String),
    /// The score after a rally (us, them): the file's and ours.
    Score {
        file: (u8, u8),
        ours: (u8, u8),
    },
    /// The file ends the set before one of the teams wins it.
    Unfinished,
}

/// What the file tells in a way we can't translate exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Untranslatable {
    SubstitutionInRally,
    SetterChangeInRally,
    /// A touch of theirs keeping the ball after the point.
    TheirTouchAfterTheEnd,
    /// A touch of ours after the point.
    OurTouchAfterTheEnd,
    PointWithoutCause,
    /// The libero should change the player it replaces within a rally.
    LiberoTooLate,
    /// Every player of the back row touches the ball with the libero.
    LiberoForNobody,
    /// The libero may replace each of these players: the file doesn't tell
    /// which.
    LiberoForWhom(Vec<u8>),
    OurTouchWithoutPlayer,
    /// Their attack back from our block (`!`) without our block.
    TheirAttackBackWithoutOurBlock,
    /// Their attack blocked (`/`) without our block point.
    TheirAttackBlockedWithoutOurBlock,
    UnknownEvaluation(char),
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
        rally_number: 0,
        chosen: HashMap::new(),
        earlier: None,
        at: None,
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

// the player out for the libero, if on court
fn covered(lineup: &Lineup) -> Option<Uuid> {
    let libero = lineup.get_current_libero();
    lineup
        .get_positions()
        .iter()
        .position(|id| *id == libero)
        .and_then(|position| lineup.get_positions_without_libero().get(position).copied())
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

// the number of one of our players, as the file writes it (0 for an id
// not of the file, which our events never have)
fn number(us: &Us, id: Uuid) -> u8 {
    us.players
        .iter()
        .find(|(_, player)| **player == id)
        .map(|(number, _)| *number)
        .unwrap_or_default()
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
    /// freeball (`F`), or they send it (`F` theirs)
    TheirFreeball,
    /// they do, and send it back without attacking (e.g. their dig over)
    BackWithoutAttack,
    /// they do, and the rally ends on their side without their attack
    /// (e.g. their error)
    Ended,
}

/// A touch of the rally: of a player, the freeball a player sends over,
/// or a point without one (`$$&`).
#[derive(Debug, Clone, Copy)]
enum Touch<'a> {
    Skill(Side, &'a SkillCode),
    Sent(Side, &'a SkillCode),
    Green(Side, char),
}

impl<'a> Touch<'a> {
    // a player's touch: its side and its code
    fn played(&self) -> Option<(Side, &'a SkillCode)> {
        match self {
            Touch::Skill(side, skill) | Touch::Sent(side, skill) => Some((*side, skill)),
            Touch::Green(_, _) => None,
        }
    }
}

// the last touch of ours played before is our block `=` (the ball off our
// hands) or our attack `/` (blocked: their block comes after it)
fn after_ball_off_a_block(before: &[(&ScoutLine, Touch)], us: Side) -> bool {
    matches!(
        before
            .iter()
            .rev()
            .filter_map(|(_, t)| t.played())
            .find(|(side, _)| *side == us),
        Some((_, skill)) if matches!((skill.skill, skill.evaluation), (Skill::Block, '=') | (Skill::Attack, '/'))
    )
}

/// The file's `F` is the freeball received, except right after a touch of
/// the same side with the other side touching next (or nobody, after a
/// touch keeping the ball): then it's the freeball sent over, as some
/// scouts write it.
fn freeballs_sent<'a>(touches: &[(&'a ScoutLine, Touch<'a>)]) -> Vec<(&'a ScoutLine, Touch<'a>)> {
    touches
        .iter()
        .enumerate()
        .map(|(index, (line, touch))| {
            let before = touches
                .get(..index)
                .unwrap_or_default()
                .iter()
                .rev()
                .find_map(|(_, t)| t.played());
            let after = touches
                .get(index + 1..)
                .unwrap_or_default()
                .iter()
                .find_map(|(_, t)| t.played());
            match (touch, before, after) {
                (Touch::Skill(side, skill), Some((before_side, _)), Some((after_side, _)))
                    if skill.skill == Skill::Freeball
                        && before_side == *side
                        && after_side != *side =>
                {
                    (*line, Touch::Sent(*side, skill))
                }
                (Touch::Skill(side, skill), Some((before_side, before)), None)
                    if skill.skill == Skill::Freeball
                        && before_side == *side
                        && !matches!(before.skill, Skill::Attack | Skill::Block) =>
                {
                    (*line, Touch::Sent(*side, skill))
                }
                _ => (*line, *touch),
            }
        })
        .collect()
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
    // the rallies translated so far
    rally_number: usize,
    // the player the libero is for, chosen before the rally (by number):
    // the file telling it a rally later
    chosen: HashMap<usize, Uuid>,
    // the player to choose a rally earlier, the rally failing without
    earlier: Option<Uuid>,
    // the line the rally being translated is at: its first touch for what
    // comes before the touches, then each touch; none for the point
    at: Option<usize>,
}

/// Where a rally starts (the dead ball before it): to translate again from
/// there, choosing who the libero is for.
struct Checkpoint {
    line: usize,
    snapshot: Snapshot,
    events: usize,
    mappings: Mappings,
    clock: DateTime<Utc>,
}

impl SetTranslator<'_> {
    fn translate(&mut self, lines: &[ScoutLine]) -> Result<(), Failure> {
        let mut rally: Vec<(&ScoutLine, Touch)> = vec![];
        let mut checkpoints = vec![self.checkpoint(0)];
        let mut index = 0;
        while let Some(line) = lines.get(index) {
            index += 1;
            let at = Some(line.line);
            self.tick(line);
            match &line.code {
                // the six on court of the file left aside: DataVolley may
                // leave them behind, ours follow the rules
                Code::Skill(side, skill) => {
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
                    if let Err(issue) = self.rally(&rally, winner) {
                        // the libero for another player: chosen a rally
                        // earlier, translated again from there
                        let earlier = self.earlier.take();
                        let previous = self.rally_number.checked_sub(1);
                        match (earlier, previous.and_then(|n| checkpoints.get(n))) {
                            (Some(player), Some(checkpoint))
                                if previous.is_some_and(|n| !self.chosen.contains_key(&n)) =>
                            {
                                let number = previous.unwrap_or_default();
                                self.chosen.insert(number, player);
                                index = checkpoint.line;
                                self.restore(checkpoint, number);
                                checkpoints.truncate(number + 1);
                                rally.clear();
                                continue;
                            }
                            _ => return Err((self.at.take().or(at), issue)),
                        }
                    }
                    self.rally_number += 1;
                    checkpoints.push(self.checkpoint(index));
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
                            return Err((
                                at,
                                Issue::Untranslatable(Untranslatable::SubstitutionInRally),
                            ));
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
                            return Err((
                                at,
                                Issue::Untranslatable(Untranslatable::SetterChangeInRally),
                            ));
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

    fn checkpoint(&self, line: usize) -> Checkpoint {
        Checkpoint {
            line,
            snapshot: self.snapshot.clone(),
            events: self.events.len(),
            mappings: self.mappings.clone(),
            clock: self.clock,
        }
    }

    fn restore(&mut self, checkpoint: &Checkpoint, rally_number: usize) {
        self.snapshot = checkpoint.snapshot.clone();
        self.events.truncate(checkpoint.events);
        self.mappings = checkpoint.mappings.clone();
        self.clock = checkpoint.clock;
        self.rally_number = rally_number;
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

    fn apply(
        &mut self,
        event_type: EventTypeEnum,
        player: Option<Uuid>,
        eval: Option<EvalEnum>,
        target_player: Option<Uuid>,
        technique: Option<TechniqueEnum>,
    ) -> Result<(), Issue> {
        self.apply_flagged(
            event_type,
            player,
            eval,
            target_player,
            technique,
            EventFlags::default(),
        )
    }

    fn apply_flagged(
        &mut self,
        event_type: EventTypeEnum,
        player: Option<Uuid>,
        eval: Option<EvalEnum>,
        target_player: Option<Uuid>,
        technique: Option<TechniqueEnum>,
        flags: EventFlags,
    ) -> Result<(), Issue> {
        let event = EventEntry {
            timestamp: self.clock,
            event_type,
            player,
            eval,
            target_player,
            technique,
            flags,
        };
        if let Some(refusal) = self.snapshot.refusal(&event) {
            return Err(Issue::Refused {
                event: event_type,
                refusal: refusal.map(|id| number(self.us, id)),
            });
        }
        if let Err(e) = self.snapshot.add_event(&event) {
            return Err(Issue::Event(e.to_string()));
        }
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

    // the last touch played before is our attack winning the point
    fn after_our_kill(&self, before: &[(&ScoutLine, Touch)]) -> bool {
        matches!(
            before.iter().rev().find_map(|(_, t)| t.played()),
            Some((side, skill))
                if side == self.us.side
                    && skill.skill == Skill::Attack
                    && self.ending(skill.skill, skill.evaluation) == Ending::Won
        )
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
        let touches = &freeballs_sent(touches);
        self.at = touches.first().map(|(line, _)| line.line);
        self.change_libero(touches)?;
        self.libero_in_or_out(touches)?;
        let mut last_cause: Option<(Side, Ending)> = None;
        // a point without a touch: the cause when no touch is, otherwise
        // the other side of the same point (`a07AH#`, then `*$$&H=`)
        let mut green: Option<(Side, Ending)> = None;
        for (index, (line, touch)) in touches.iter().enumerate() {
            self.at = Some(line.line);
            let next = touches.get(index + 1..).unwrap_or_default();
            match (self.rally_over(), touch) {
                // what the file adds after the end: the other side of the
                // same point, e.g. their reception error after our ace
                (true, Touch::Skill(side, skill) | Touch::Sent(side, skill))
                    if *side != self.us.side =>
                {
                    let block_on_our_kill = skill.skill == Skill::Block
                        && self.after_our_kill(touches.get(..index).unwrap_or_default());
                    match (
                        self.ending(skill.skill, skill.evaluation),
                        block_on_our_kill,
                    ) {
                        // their block touching our kill (`*02AH#`, then
                        // `a13BH-`): the ball went down all the same
                        (Ending::Goes, true) => self.mapped("their block on our kill: dropped"),
                        (Ending::Goes, false) => {
                            return Err(Issue::Untranslatable(
                                Untranslatable::TheirTouchAfterTheEnd,
                            ));
                        }
                        (Ending::Won | Ending::Lost, _) => {}
                    }
                }
                (true, Touch::Green(_, _)) => {}
                // our block `=` (the ball off our hands, "mani fuori") or our
                // attack `/` (blocked): the point is already told, our dig
                // `=` trying to reach the ball after it says nothing more
                (true, Touch::Skill(side, skill))
                    if *side == self.us.side
                        && skill.skill == Skill::Dig
                        && skill.evaluation == '='
                        && after_ball_off_a_block(
                            touches.get(..index).unwrap_or_default(),
                            self.us.side,
                        ) =>
                {
                    self.mapped("dig after a ball off a block: dropped");
                }
                (true, _) => {
                    return Err(Issue::Untranslatable(Untranslatable::OurTouchAfterTheEnd));
                }
                (false, Touch::Skill(side, skill) | Touch::Sent(side, skill))
                    if *side == self.us.side =>
                {
                    let before = touches.get(..index).unwrap_or_default();
                    let sent = matches!(touch, Touch::Sent(_, _));
                    self.our_touch(skill, sent, before, next)?;
                    last_cause = None;
                }
                (false, Touch::Skill(side, skill) | Touch::Sent(side, skill)) => {
                    let before = touches.get(..index).unwrap_or_default();
                    let sent = matches!(touch, Touch::Sent(_, _));
                    last_cause = self
                        .their_touch(*side, skill, sent, before, next)?
                        .or(last_cause);
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
        self.at = None;
        match (self.rally_over(), winner, last_cause.or(green)) {
            (true, _, _) => Ok(()),
            // their point: their ace, kill or block point, or their point
            // without a touch
            // the ball ours: our fault, though the file tells their point
            (false, TeamSideEnum::Them, Some((side, Ending::Won)))
                if side != self.us.side
                    && !self
                        .snapshot
                        .get_available_options()
                        .contains(&EventTypeEnum::OS) =>
            {
                self.mapped("their point with the ball ours: F");
                self.apply(EventTypeEnum::F, None, None, None, None)
            }
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
            (false, _, _) => Err(Issue::Untranslatable(Untranslatable::PointWithoutCause)),
        }
    }

    // the libero not on court touching the ball: changed before the rally
    fn change_libero(&mut self, touches: &[(&ScoutLine, Touch)]) -> Result<(), Issue> {
        let fallback = self.snapshot.current_lineup.get_fallback_libero();
        let touches_fallback = touches.iter().any(|(_, t)| match t.played() {
            Some((side, skill)) if side == self.us.side => skill
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

    // who the libero plays for (the file writes the six without it, so only
    // the touches tell): as our lineup wants it while nothing tells
    // otherwise; out when the player it replaces touches the ball, back in
    // when not; touching with them, for the only other player of the back
    // row who can be out, chosen a rally earlier (a rally between two libero
    // replacements); nobody else, or more than one, it can't be told
    fn libero_in_or_out(&mut self, touches: &[(&ScoutLine, Touch)]) -> Result<(), Issue> {
        let ours: Vec<Uuid> = touches
            .iter()
            .filter_map(|(_, t)| match t.played() {
                Some((side, skill)) if side == self.us.side => skill.player,
                _ => None,
            })
            .filter_map(|n| self.us.players.get(&n).copied())
            .collect();
        let lineup = &self.snapshot.current_lineup;
        let libero_touches = ours.contains(&lineup.get_current_libero());
        let out = lineup.is_libero_out();
        if let Some(player) = self.chosen.get(&self.rally_number).copied() {
            self.mapped("libero for a player: LF");
            self.apply(EventTypeEnum::LF, Some(player), None, None, None)?;
            return match libero_touches {
                true => Err(Issue::Untranslatable(Untranslatable::LiberoTooLate)),
                false => Ok(()),
            };
        }
        let free = |covered: Option<Uuid>| covered.is_none_or(|c| !ours.contains(&c));
        match (covered(lineup), libero_touches, out) {
            (Some(c), _, _) if !ours.contains(&c) => Ok(()),
            (Some(_), false, _) => self.apply(EventTypeEnum::LR, None, None, None, None),
            (Some(_), true, _) => {
                self.earlier = Some(self.only_one_out(&ours)?);
                Err(Issue::Untranslatable(Untranslatable::LiberoTooLate))
            }
            (None, false, true) => {
                let mut back_in = lineup.clone();
                back_in
                    .toggle_libero()
                    .map_err(|e| Issue::Event(e.to_string()))?;
                match free(covered(&back_in)) {
                    true => self.apply(EventTypeEnum::LR, None, None, None, None),
                    false => Ok(()),
                }
            }
            (None, false, false) => Ok(()),
            (None, true, _) => {
                if out {
                    self.apply(EventTypeEnum::LR, None, None, None, None)?;
                }
                match covered(&self.snapshot.current_lineup) {
                    Some(c) if !ours.contains(&c) => Ok(()),
                    _ => {
                        let player = self.only_one_out(&ours)?;
                        self.mapped("libero for a player: LF");
                        self.apply(EventTypeEnum::LF, Some(player), None, None, None)?;
                        match covered(&self.snapshot.current_lineup) == Some(player) {
                            true => Ok(()),
                            false => {
                                self.earlier = Some(player);
                                Err(Issue::Untranslatable(Untranslatable::LiberoTooLate))
                            }
                        }
                    }
                }
            }
        }
    }

    // the only player the libero can be for in the rally: in the back row,
    // not serving, not touching the ball
    fn only_one_out(&self, ours: &[Uuid]) -> Result<Uuid, Issue> {
        let lineup = &self.snapshot.current_lineup;
        let serving = lineup.get_current_phase() == PhaseEnum::Break;
        let positions = lineup.get_positions_without_libero();
        let out: Vec<Uuid> = positions
            .iter()
            .enumerate()
            .filter(|(index, _)| matches!((index, serving), (4 | 5, _) | (0, false)))
            .map(|(_, id)| *id)
            .filter(|id| !ours.contains(id))
            .collect();
        match out.as_slice() {
            [player] => Ok(*player),
            [] => Err(Issue::Untranslatable(Untranslatable::LiberoForNobody)),
            candidates => Err(Issue::Untranslatable(Untranslatable::LiberoForWhom(
                candidates.iter().map(|id| number(self.us, *id)).collect(),
            ))),
        }
    }

    /// Who plays the ball after a touch of ours, up to our next touch: our
    /// evaluations tell it, the file's tell how good the touch was.
    fn after(&self, next: &[(&ScoutLine, Touch)]) -> After {
        // a freeball sent over: no skill, neither received nor an attack
        let skills: Vec<(Side, Option<Skill>)> = next
            .iter()
            .filter_map(|(_, t)| match t {
                Touch::Skill(side, skill) => Some((*side, Some(skill.skill))),
                Touch::Sent(side, _) => Some((*side, None)),
                Touch::Green(_, _) => None,
            })
            .collect();
        let ours = |side: &Side| *side == self.us.side;
        let theirs: Vec<Option<Skill>> = skills
            .iter()
            .take_while(|(side, _)| !ours(side))
            .map(|(_, skill)| *skill)
            .collect();
        let our_next = skills
            .iter()
            .find(|(side, _)| ours(side))
            .map(|(_, skill)| *skill);
        // their reception or dig straight over
        let their_first_over = next
            .iter()
            .find_map(|(_, t)| t.played())
            .filter(|(side, _)| !ours(side))
            .is_some_and(|(_, s)| {
                matches!(s.skill, Skill::Reception | Skill::Dig) && s.evaluation == '/'
            });
        match (
            theirs.first(),
            theirs.contains(&Some(Skill::Attack)),
            our_next,
        ) {
            // their freeball received: we sent it
            (Some(Some(Skill::Freeball)), _, _) => After::OurFreeball,
            (_, true, _) => After::TheyAttack,
            (_, _, Some(Some(Skill::Freeball))) => After::TheirFreeball,
            // their freeball sent over
            _ if theirs.contains(&None) => After::TheirFreeball,
            // our block next: they attacked, not in the file (their
            // reception or dig over isn't an attack)
            (_, _, Some(Some(Skill::Block))) if !their_first_over => After::TheyAttack,
            (None, _, Some(_)) => After::Kept,
            (Some(_), _, Some(_)) => After::BackWithoutAttack,
            (_, _, None) => After::Ended,
        }
    }

    fn our_touch(
        &mut self,
        skill: &SkillCode,
        sent: bool,
        before: &[(&ScoutLine, Touch)],
        next: &[(&ScoutLine, Touch)],
    ) -> Result<(), Issue> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        // the freeball received after their attack: an easy attack, our dig
        let they_attacked = before
            .iter()
            .rev()
            .filter_map(|(_, t)| t.played().map(|(side, s)| (side, s.skill)))
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
            .ok_or(Issue::Untranslatable(Untranslatable::OurTouchWithoutPlayer))?;
        let player = player(self.us, number)?;
        // our set `=` attacked by us next: the rally went on, the `=` is
        // how bad the set was, not a fault
        let attacked_next = next
            .iter()
            .find_map(|(_, t)| t.played())
            .is_some_and(|(side, s)| side == self.us.side && s.skill == Skill::Attack);
        // our attack `=` with their block `=` next: the ball off their
        // hands ("mani fuori"), our winning attack
        let their_block_out_next =
            next.iter()
                .find_map(|(_, t)| t.played())
                .is_some_and(|(side, s)| {
                    side != self.us.side && s.skill == Skill::Block && s.evaluation == '='
                });
        let ending = match (
            kind,
            self.ending(skill.skill, skill.evaluation),
            skill.evaluation,
            attacked_next,
            their_block_out_next,
        ) {
            (Skill::Set, Ending::Lost, _, true, _) => {
                self.mapped("set error attacked: no fault");
                Ending::Goes
            }
            (Skill::Attack, Ending::Lost, '=', _, true) => {
                self.mapped("attack error off their block out: A#");
                Ending::Won
            }
            (_, ending, _, _, _) => ending,
        };
        let touched = self
            .rally_events()
            .iter()
            .any(|e| e.event_type.ball_side_after(e.eval).is_some());
        let ball_ours =
            self.snapshot.get_ball_side() == TeamSideEnum::Us && touched && !self.rally_over();
        let after = self.after(next);
        let last = self.rally_events().last().map(|e| (e.event_type, e.eval));
        // their reception or dig straight over (`/`): back without their
        // freeball
        let their_first_over = next
            .iter()
            .find_map(|(_, t)| t.played())
            .is_some_and(|(_, s)| {
                matches!(s.skill, Skill::Reception | Skill::Dig) && s.evaluation == '/'
            });
        // their block sent it back: no freeball of theirs
        let their_block = next
            .iter()
            .filter_map(|(_, t)| t.played())
            .take_while(|(side, _)| *side != self.us.side)
            .any(|(_, s)| s.skill == Skill::Block);
        // our freeball received going back over
        let over = kind == Skill::Freeball
            && (skill.evaluation == '/'
                || matches!(
                    after,
                    After::TheyAttack | After::TheirFreeball | After::BackWithoutAttack
                ));
        // a team event following the one of the touch
        let mut then: Option<EventTypeEnum> = None;
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
            // the serve: they attack (`/` is then our `+`), or send a
            // freeball (our freeball received tells it, `OF`), the serve
            // telling their reception; their reception back: our `/`
            (Skill::Serve, Ending::Goes, e, _, After::TheirFreeball) if e != '/' => {
                Some((S, Some(eval(e)?)))
            }
            (Skill::Serve, Ending::Goes, '/', _, After::TheyAttack) => {
                self.mapped("S/ attacked as S+");
                Some((S, Some(Positive)))
            }
            (Skill::Serve, Ending::Goes, e, _, After::TheyAttack | After::Ended) => {
                Some((S, Some(eval(e)?)))
            }
            // back to us without their attack: their reception not over,
            // their freeball not scouted
            (Skill::Serve, Ending::Goes, e, _, After::BackWithoutAttack)
                if e != '/' && !their_first_over && !their_block =>
            {
                self.mapped("S back without attack: their freeball OF");
                then = Some(OF);
                Some((S, Some(eval(e)?)))
            }
            // back to us with their reception over
            (Skill::Serve, Ending::Goes, e, _, _) => {
                if e != '/' {
                    self.mapped("S back without attack as S/");
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
            // freeball after it, unless they dug it (`-`)
            (Skill::Attack, Ending::Goes, '!', _, After::Kept | After::OurFreeball) => {
                Some((A, Some(Exclamative)))
            }
            // dug by them (`-`), then their freeball: the evaluation tells
            // their dig, the freeball (`OF`) what they made of it
            (Skill::Attack | Skill::Block, Ending::Goes, '-', _, After::TheirFreeball) => {
                Some((attack_or_block(skill.skill), Some(Negative)))
            }
            (Skill::Attack | Skill::Block, Ending::Goes, '-', _, After::BackWithoutAttack)
                if !their_first_over && !their_block =>
            {
                self.mapped("A or B dug, back without attack: their freeball OF");
                then = Some(OF);
                Some((attack_or_block(skill.skill), Some(Negative)))
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
            (
                Skill::Dig,
                Ending::Goes,
                _,
                true,
                After::TheyAttack | After::TheirFreeball | After::BackWithoutAttack,
            ) => {
                self.mapped("cover played by them: FB");
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
            // our freeball sent over, as some scouts write it
            (Skill::Freeball, Ending::Goes, _, _, _) if sent => {
                self.mapped("freeball sent: FB");
                Some((FB, None))
            }
            // their freeball, received by us: told by our serve `/` already,
            // or by their freeball sent
            (Skill::Freeball, Ending::Goes, e, _, _) => match (last, e) {
                (Some((S, Some(Over))), _) => {
                    self.mapped("their freeball after S/: in the serve");
                    None
                }
                // received straight over (or they play it next): our freeball
                (Some((OF, None)), _) if over => {
                    self.mapped("their freeball received over: FB");
                    Some((FB, None))
                }
                (Some((OF, None)), _) => None,
                _ if over => {
                    self.mapped("their freeball received over: OF, FB");
                    then = Some(FB);
                    Some((OF, None))
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
                // where our serve went, or where we received theirs: the
                // zone, with its subzone if told (a cell of ours;
                // DataVolley tells no two cells)
                let target = match (event_type, skill.end_zone) {
                    (S | P, Some(zone)) => {
                        CourtTarget::of_zone(zone, skill.end_subzone.and_then(SubzoneEnum::of))
                    }
                    _ => None,
                };
                self.apply_flagged(
                    event_type,
                    Some(player),
                    eval,
                    None,
                    technique,
                    EventFlags::default().with_target(target),
                )
            }
            None => Ok(()),
        }?;
        match then {
            Some(event_type) => self.apply(event_type, None, None, None, None),
            None => Ok(()),
        }
    }

    /// Their touch: what it says about the end of the rally, for the
    /// closing event; their freeball received is our freeball (`FB`), a
    /// team event, unless our freeball sent tells it already; their
    /// freeball sent is theirs (`OF`), unless our serve `/` tells it;
    /// right after their attack, the ball came back to them without ours
    /// told: our freeball (`FB`) first, unless our block sent it back
    /// (their attack `!`), whose player we can't tell.
    fn their_touch(
        &mut self,
        side: Side,
        skill: &SkillCode,
        sent: bool,
        before: &[(&ScoutLine, Touch)],
        next: &[(&ScoutLine, Touch)],
    ) -> Result<Option<(Side, Ending)>, Issue> {
        let ending = self.ending(skill.skill, skill.evaluation);
        let block_point_next = next
            .iter()
            .find_map(|(_, t)| t.played())
            .filter(|(side, _)| *side == self.us.side)
            .is_some_and(|(_, s)| {
                s.skill == Skill::Block && self.ending(s.skill, s.evaluation) == Ending::Won
            });
        let after_their_attack = before
            .iter()
            .rev()
            .find_map(|(_, t)| t.played())
            .filter(|(s, attack)| *s == side && attack.skill == Skill::Attack)
            .map(|(_, attack)| attack.evaluation);
        match (skill.skill, ending, after_their_attack, sent) {
            (Skill::Freeball, Ending::Goes, Some('!'), true) => {
                return Err(Issue::Untranslatable(
                    Untranslatable::TheirAttackBackWithoutOurBlock,
                ));
            }
            (Skill::Freeball, Ending::Goes, Some(_), true) => {
                self.mapped("back to them after their attack: FB");
                self.apply(EventTypeEnum::FB, None, None, None, None)?;
            }
            _ => {}
        }
        let last = self.rally_events().last().map(|e| (e.event_type, e.eval));
        match (skill.skill, ending, skill.evaluation, sent, last) {
            // blocked by us: our block point must follow
            (Skill::Attack, Ending::Lost, '/', _, _) if !block_point_next => Err(
                Issue::Untranslatable(Untranslatable::TheirAttackBlockedWithoutOurBlock),
            ),
            (
                Skill::Freeball,
                Ending::Goes,
                _,
                true,
                Some((EventTypeEnum::S, Some(EvalEnum::Over))),
            )
            | (Skill::Freeball, Ending::Goes, _, false, Some((EventTypeEnum::FB, None))) => {
                Ok(None)
            }
            (Skill::Freeball, Ending::Goes, _, true, _) => {
                self.mapped("their freeball sent: OF");
                self.apply(EventTypeEnum::OF, None, None, None, None)
                    .map(|_| None)
            }
            (Skill::Freeball, Ending::Goes, _, false, _) => self
                .apply(EventTypeEnum::FB, None, None, None, None)
                .map(|_| None),
            (_, Ending::Goes, _, _, _) => Ok(None),
            (_, ending, _, _, _) => Ok(Some((side, ending))),
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
        other => Err(Issue::Untranslatable(Untranslatable::UnknownEvaluation(
            other,
        ))),
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
