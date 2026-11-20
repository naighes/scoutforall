//! A set of DataVolley as one of ours, for one of the two teams: rally by
//! rally, our touches become our events and the touches of the opponent
//! only what ends a rally (`OS`, `OE`), all validated by our `Snapshot` as
//! the scouting would. Before that the rally is told as our rules tell it,
//! the same for both teams (`decide`): who decides an evaluation, the
//! touches the file leaves untold, the scenarios refused. Nothing is
//! guessed: a rally that doesn't translate exactly, a lineup different from
//! the six on court of the file, a score different from the file's stop the
//! set; an error of the scout (`Impossible`) stops it with the line and the
//! touches, to correct the file.

use crate::{
    dvw::decide::decide,
    dvw::model::{
        AttackCombination, Code, DvwFile, ScoutLine, Side, Skill, SkillCode, WinningSymbols,
    },
    rally::Incoherence,
    shapes::{
        court_zone::{CourtTarget, ServeStartEnum, SubzoneEnum, TargetKind},
        enums::{EvalEnum, EventTypeEnum, PhaseEnum, RoleEnum, TeamSideEnum},
        flags::{BlockersEnum, EventFlags},
        lineup::Lineup,
        refusal::Refusal,
        set::SetEntry,
        snapshot::{EventEntry, Snapshot},
        technique::{ServeTypeEnum, SetTypeEnum, TechniqueEnum},
    },
};
use chrono::{DateTime, FixedOffset, NaiveTime, TimeDelta, Utc};
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
    /// The touches of the two teams don't replay on both sides.
    Sides(Incoherence),
    /// An answer (a reception, a dig, a block) the touch it answers, at
    /// `answered_line`, doesn't admit (`rally::answer_evals`): the two
    /// teams tell the same transition differently.
    Answer {
        answered: (EventTypeEnum, Option<EvalEnum>),
        answered_line: usize,
        answer: (EventTypeEnum, Option<EvalEnum>),
    },
    /// An error of the scout (`dvw::decide`): the match doesn't import
    /// until the file is corrected.
    Impossible(Impossible),
}

/// A rally that can't have been played as the file tells it: the touches
/// by skill and evaluation, as the file writes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Impossible {
    /// A touch of the team after its own reception or dig over (`/`), at
    /// `over_line`: the ball was on the other side.
    TouchAfterOver {
        over: (Skill, char),
        over_line: usize,
        touch: (Skill, char),
    },
    /// A dig or a block without the attack of the other team, right after
    /// `before`, at `before_line`.
    MissingAttack {
        before: (Skill, char),
        before_line: usize,
        touch: (Skill, char),
    },
}

/// Where an event comes from in the file: the line, and the touch of the
/// rally (as `dvw::decide` tells it, the same for both teams) it comes
/// from, 0 before the touches, `usize::MAX` the point.
pub(crate) type At = (usize, usize);

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
    date: DateTime<FixedOffset>,
    max_substitutions: u8,
) -> Result<(SetEntry, Mappings), TranslateError> {
    translate_lined(file, us, set_number, date, max_substitutions)
        .map(|(set, mappings, _)| (set, mappings))
}

/// [`translate_set`], with the line of the file each event comes from: the
/// touch it translates, the first touch of the rally for what comes before
/// the touches (a libero change), the point for what ends the rally on the
/// other side.
pub(crate) fn translate_lined(
    file: &DvwFile,
    us: &Us,
    set_number: u8,
    date: DateTime<FixedOffset>,
    max_substitutions: u8,
) -> Result<(SetEntry, Mappings, Vec<At>), TranslateError> {
    let fail = |line: Option<usize>, issue: Issue| TranslateError {
        set_number,
        line,
        issue,
    };
    let lines = set_lines(&file.scout, set_number);
    let start = set_start(lines, us, set_number).map_err(|(line, issue)| fail(line, issue))?;
    let snapshot = Snapshot::new(&start, max_substitutions)
        .map_err(|e| fail(None, Issue::Event(e.to_string())))?;
    let video_clock = has_video_clock(file);
    let mut set = SetTranslator {
        us,
        symbols: file.winning_symbols.clone().unwrap_or_else(default_symbols),
        combinations: &file.attack_combinations,
        snapshot,
        events: vec![],
        lines: vec![],
        step: 0,
        rally_start: 0,
        mappings: Mappings::new(),
        clock: midnight(date, video_clock),
        date,
        video_clock,
        rally_number: 0,
        chosen: HashMap::new(),
        earlier: None,
        at: None,
        inverted_p1: false,
        block_left_out: false,
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
            set.lines,
        )),
        None => Err(fail(None, Issue::Unfinished)),
    }
}

/// Where the times of the events start from: midnight of the day of the
/// match in UTC on the video clock (`video::video_midnight`), otherwise in
/// the time zone of the match, as its clock.
fn midnight(date: DateTime<FixedOffset>, video_clock: bool) -> DateTime<Utc> {
    match video_clock {
        true => date.date_naive().and_time(NaiveTime::MIN).and_utc(),
        false => wall_clock(date, NaiveTime::MIN),
    }
}

/// The time of the clock of the file on the day of the match: a local time,
/// in the time zone of the match (as the start of a video from its name).
fn wall_clock(date: DateTime<FixedOffset>, time: NaiveTime) -> DateTime<Utc> {
    let local = date.date_naive().and_time(time);
    // out of the range of the dates only at their ends
    local
        .checked_sub_offset(*date.offset())
        .unwrap_or(local)
        .and_utc()
}

/// Whether the scout is linked to the video of the match: the times of the
/// events are then its seconds in the video, from midnight of `date` (the
/// clock of the file can be anything, e.g. a PC with the wrong time).
pub fn has_video_clock(file: &DvwFile) -> bool {
    file.scout
        .iter()
        .any(|l| l.video_seconds.is_some_and(|s| s > 0))
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
pub(crate) fn default_symbols() -> WinningSymbols {
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

/// A touch of the rally: of a player, the freeball a player sends over,
/// or a point without one (`$$&`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Touch {
    Skill(Side, SkillCode),
    Sent(Side, SkillCode),
    Green(Side, char),
}

impl Touch {
    // a player's touch: its side and its code
    pub(crate) fn played(&self) -> Option<(Side, &SkillCode)> {
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
/// scouts write it. After a touch of the same side sending the ball over
/// (`/`) it's received all the same, sent back by the other side
/// (`&MIL-27.dvw` `a11SQ/ *18RQ/ *20FH=`).
fn freeballs_sent<'a>(touches: &[(&'a ScoutLine, Touch)]) -> Vec<(&'a ScoutLine, Touch)> {
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
                (Touch::Skill(side, skill), Some((before_side, before)), Some((after_side, _)))
                    if skill.skill == Skill::Freeball
                        && before_side == *side
                        && before.evaluation != '/'
                        && after_side != *side =>
                {
                    (*line, Touch::Sent(*side, skill.clone()))
                }
                (Touch::Skill(side, skill), Some((before_side, before)), None)
                    if skill.skill == Skill::Freeball
                        && before_side == *side
                        && before.evaluation != '/'
                        && !matches!(before.skill, Skill::Attack | Skill::Block) =>
                {
                    (*line, Touch::Sent(*side, skill.clone()))
                }
                _ => (*line, touch.clone()),
            }
        })
        .collect()
}

struct SetTranslator<'a> {
    us: &'a Us<'a>,
    symbols: WinningSymbols,
    // the attack combinations of the file, for the set of each attack
    combinations: &'a [AttackCombination],
    snapshot: Snapshot,
    events: Vec<EventEntry>,
    // where each event comes from in the file
    lines: Vec<At>,
    // the touch of the rally being translated (see `At`)
    step: usize,
    // where the rally being translated starts in `events`
    rally_start: usize,
    mappings: Mappings,
    clock: DateTime<Utc>,
    // the match: its day, and the time zone of the clock of the file
    date: DateTime<FixedOffset>,
    // the times from the seconds in the video, not from the clock
    video_clock: bool,
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
    // the P1 inverted, as the scouting keeps it: told by where our opposite
    // or outside hitter attacked from in P1 in side out (see
    // `inverted_p1_of`), the last one told going on; canonical until then
    inverted_p1: bool,
    // our block of no attack keeping the ball left out (see `apply_flagged`):
    // our dig after it too
    block_left_out: bool,
}

/// Where a rally starts (the dead ball before it): to translate again from
/// there, choosing who the libero is for.
struct Checkpoint {
    line: usize,
    snapshot: Snapshot,
    events: usize,
    mappings: Mappings,
    clock: DateTime<Utc>,
    inverted_p1: bool,
}

impl SetTranslator<'_> {
    fn translate(&mut self, lines: &[ScoutLine]) -> Result<(), Failure> {
        let mut rally: Vec<(&ScoutLine, Touch)> = vec![];
        let mut checkpoints = vec![self.checkpoint(0)];
        let mut index = 0;
        while let Some(line) = lines.get(index) {
            index += 1;
            let at = Some(line.line);
            // a substitution, a setter change: at their line
            self.at = at;
            self.step = 0;
            self.tick(line);
            match &line.code {
                // the six on court of the file left aside: DataVolley may
                // leave them behind, ours follow the rules
                Code::Skill(side, skill) => {
                    rally.push((line, Touch::Skill(*side, skill.clone())));
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
                    if let Err(issue) = self.rally(&rally, winner, line.line) {
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
            inverted_p1: self.inverted_p1,
        }
    }

    fn restore(&mut self, checkpoint: &Checkpoint, rally_number: usize) {
        self.snapshot = checkpoint.snapshot.clone();
        self.events.truncate(checkpoint.events);
        self.lines.truncate(checkpoint.events);
        self.mappings = checkpoint.mappings.clone();
        self.clock = checkpoint.clock;
        self.inverted_p1 = checkpoint.inverted_p1;
        self.rally_number = rally_number;
        self.block_left_out = false;
    }

    // the time of the line (in the video, when linked to it), or a second
    // after the previous event
    fn tick(&mut self, line: &ScoutLine) {
        let midnight = midnight(self.date, self.video_clock);
        let time = match self.video_clock {
            true => line
                .video_seconds
                .map(|s| midnight + TimeDelta::seconds(i64::from(s))),
            false => line
                .time
                .as_deref()
                .and_then(|t| NaiveTime::parse_from_str(&t.replace('.', ":"), "%H:%M:%S").ok())
                .map(|t| wall_clock(self.date, t)),
        };
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
        use EvalEnum::*;
        use EventTypeEnum::*;
        // the block answers an attack (scouting rules v4): our block of a
        // ball coming over without one (their freeball, their reception
        // back after our serve `/`) told as the translation of v3 tells it
        // (`domain::rules`): `#` an attack point, `=` an attack error, `!`
        // our freeball, `+` and `-` (the ball kept) left out with our dig
        // after them
        let no_attack = matches!(
            self.snapshot
                .last_event
                .as_ref()
                .map(|e| (e.event_type, e.eval)),
            Some((OF, _) | (S, Some(Over)))
        );
        let told = match (event_type, eval, no_attack, self.block_left_out) {
            (D, _, _, true) => None,
            (B, Some(Perfect), true, _) => Some((A, Some(Perfect), None)),
            (B, Some(Error), true, _) => Some((A, Some(Error), None)),
            (B, Some(Exclamative), true, _) => Some((FB, None, None)),
            (B, Some(Positive | Negative), true, _) => None,
            _ => Some((event_type, eval, technique)),
        };
        // a change of the lineup between the block and the dig keeps it
        self.block_left_out = match event_type {
            R | CL | LR | LF | CS => self.block_left_out,
            _ => matches!(
                (event_type, eval, no_attack),
                (B, Some(Positive | Negative), true)
            ),
        };
        let Some((told_type, eval, technique)) = told else {
            self.mapped("B of no attack kept, or the dig after it: left out");
            return Ok(());
        };
        if told_type != event_type {
            self.mapped("B of no attack as A or FB");
        }
        let event_type = told_type;
        let event = EventEntry {
            timestamp: self.clock,
            event_type,
            player,
            eval,
            target_player,
            technique,
            flags: flags.with_inverted_p1(self.inverted_p1),
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
        self.lines.push((self.at.unwrap_or_default(), self.step));
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
        point: usize,
    ) -> Result<(), Issue> {
        self.rally_start = self.events.len();
        self.inverted_p1 = self.inverted_p1_of(touches).unwrap_or(self.inverted_p1);
        self.at = touches.first().map(|(line, _)| line.line);
        self.step = 0;
        let decided =
            decide(&freeballs_sent(touches), &self.symbols).map_err(|(line, issue)| {
                self.at = Some(line);
                issue
            })?;
        decided.mapped.iter().for_each(|name| self.mapped(name));
        let touches = &decided.touches;
        self.change_libero(touches)?;
        self.libero_in_or_out(touches)?;
        let mut last_cause: Option<(Side, Ending)> = None;
        // a point without a touch: the cause when no touch is, otherwise
        // the other side of the same point (`a07AH#`, then `*$$&H=`)
        let mut green: Option<(Side, Ending)> = None;
        for (index, (line, touch)) in touches.iter().enumerate() {
            self.at = Some(line.line);
            self.step = index + 1;
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
                    let sent = matches!(touch, Touch::Sent(_, _));
                    self.our_touch(skill, sent, next)?;
                    last_cause = None;
                }
                (false, Touch::Skill(side, skill) | Touch::Sent(side, skill)) => {
                    let sent = matches!(touch, Touch::Sent(_, _));
                    last_cause = self.their_touch(*side, skill, sent)?.or(last_cause);
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
        self.at = Some(point);
        self.step = usize::MAX;
        // the winner a point without a touch tells
        let green = green.map(|(side, ending)| match (side == self.us.side, ending) {
            (true, Ending::Won) | (false, Ending::Lost) => TeamSideEnum::Us,
            _ => TeamSideEnum::Them,
        });
        match (self.rally_over(), winner, last_cause, green) {
            (true, _, _, _) => Ok(()),
            // their point: their ace, kill or block point; the ball ours,
            // our fault, though the file tells their point
            (false, TeamSideEnum::Them, Some((side, Ending::Won)), _)
                if side != self.us.side
                    && !self
                        .snapshot
                        .get_available_options()
                        .contains(&EventTypeEnum::OS) =>
            {
                self.mapped("their point with the ball ours: F");
                self.apply(EventTypeEnum::F, None, None, None, None)
            }
            (false, TeamSideEnum::Them, Some((side, Ending::Won)), _) if side != self.us.side => {
                self.apply(EventTypeEnum::OS, None, None, None, None)
            }
            // ours: their error
            (false, TeamSideEnum::Us, Some((side, Ending::Lost)), _) if side != self.us.side => {
                self.apply(EventTypeEnum::OE, None, None, None, None)
            }
            // a point without a touch (`$$&`, e.g. a rotation fault, a net
            // fault): the point without a cause, ours or theirs
            (false, winner, None, Some(green)) if green == winner => {
                self.mapped("point without a touch: PW or PL");
                let event_type = match winner {
                    TeamSideEnum::Us => EventTypeEnum::PW,
                    TeamSideEnum::Them => EventTypeEnum::PL,
                };
                self.apply(event_type, None, None, None, None)
            }
            (false, _, _, _) => Err(Issue::Untranslatable(Untranslatable::PointWithoutCause)),
        }
    }

    /// Whether the P1 is inverted in the rally, when it tells: in P1 in
    /// side out, our opposite in the front row attacking from 2 (inverted)
    /// or 4 (canonical), or our outside hitter from 4 (inverted) or 2
    /// (canonical), the first of them attacking with its start zone told.
    /// None elsewhere, or with no such attack (an ace, an attack of the
    /// middle blocker, a pipe).
    fn inverted_p1_of(&self, touches: &[(&ScoutLine, Touch)]) -> Option<bool> {
        let lineup = &self.snapshot.current_lineup;
        let p1_side_out = lineup.get_current_phase() == PhaseEnum::SideOut
            && lineup.get_current_rotation().is_ok_and(|r| r == 0);
        let told = |skill: &SkillCode| -> Option<bool> {
            let id = skill.player.and_then(|n| self.us.players.get(&n))?;
            let front = !lineup.is_back_row_player(id);
            match (lineup.get_role(id).ok()?, front, skill.start_zone?) {
                (RoleEnum::OppositeHitter, true, 2) | (RoleEnum::OutsideHitter, true, 4) => {
                    Some(true)
                }
                (RoleEnum::OppositeHitter, true, 4) | (RoleEnum::OutsideHitter, true, 2) => {
                    Some(false)
                }
                _ => None,
            }
        };
        match p1_side_out {
            true => touches.iter().find_map(|(_, t)| match t.played() {
                Some((side, skill)) if side == self.us.side && skill.skill == Skill::Attack => {
                    told(skill)
                }
                _ => None,
            }),
            false => None,
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

    /// Whether they attack after a touch of ours, before our next touch.
    fn they_attack(&self, next: &[(&ScoutLine, Touch)]) -> bool {
        next.iter()
            .filter_map(|(_, t)| t.played())
            .take_while(|(side, _)| *side != self.us.side)
            .any(|(_, skill)| skill.skill == Skill::Attack)
    }

    fn our_touch(
        &mut self,
        skill: &SkillCode,
        sent: bool,
        next: &[(&ScoutLine, Touch)],
    ) -> Result<(), Issue> {
        use EvalEnum::*;
        use EventTypeEnum::*;
        // the touch as `dvw::decide` tells it: the evaluations agree with
        // the other team's, the freeballs sent told
        let kind = skill.skill;
        // a freeball sent untold (`dvw::decide`) has no player
        let player = match (skill.player, sent) {
            (Some(number), _) => Some(player(self.us, number)?),
            (None, true) => None,
            (None, false) => {
                return Err(Issue::Untranslatable(Untranslatable::OurTouchWithoutPlayer));
            }
        };
        // our set `=` attacked by us next: the rally went on, the `=` is
        // how bad the set was, not a fault
        let attacked_next = next
            .iter()
            .find_map(|(_, t)| t.played())
            .is_some_and(|(side, s)| side == self.us.side && s.skill == Skill::Attack);
        let ending = match (
            kind,
            self.ending(skill.skill, skill.evaluation),
            attacked_next,
        ) {
            (Skill::Set, Ending::Lost, true) => {
                self.mapped("set error attacked: no fault");
                Ending::Goes
            }
            (_, ending, _) => ending,
        };
        let last = self.rally_events().last().map(|e| (e.event_type, e.eval));
        // the touch after the one before: our cover after our attack `!`
        // (their block `!` untold), our dig after our block, the first
        // touch of their freeball (in it already)
        let after_last = match last {
            Some((A, Some(Exclamative))) => Some(Kept::Cover),
            Some((B, Some(Positive | Negative))) => Some(Kept::Block),
            Some((OF, None) | (S, Some(Over))) => Some(Kept::Freeball),
            _ => None,
        };
        let they_attack = self.they_attack(next);
        let event = match (kind, ending, skill.evaluation, after_last, they_attack) {
            // the ending touches: as the file says, `/` blocked or invasion
            (Skill::Serve, Ending::Won, _, _, _) => Some((S, Some(Perfect))),
            (Skill::Serve, Ending::Lost, _, _, _) => Some((S, Some(Error))),
            (Skill::Attack, Ending::Won, _, _, _) => Some((A, Some(Perfect))),
            (Skill::Attack, Ending::Lost, '/', _, _) => Some((A, Some(Over))),
            (Skill::Attack, Ending::Lost, _, _, _) => Some((A, Some(Error))),
            (Skill::Block, Ending::Won, _, _, _) => Some((B, Some(Perfect))),
            // the invasion, a fault: with the ball ours (no block now)
            // the player's fault
            (Skill::Block, Ending::Lost, '/', _, _)
                if !self.snapshot.get_available_options().contains(&B) =>
            {
                self.mapped("invasion with the ball ours: F");
                Some((F, None))
            }
            (Skill::Block, Ending::Lost, '/', _, _) => Some((B, Some(Over))),
            (Skill::Block, Ending::Lost, _, _, _) => Some((B, Some(Error))),
            (Skill::Reception, Ending::Lost, _, _, _) => Some((P, Some(Error))),
            // the error of our cover, of our dig after our block or of
            // their freeball: our fault
            (Skill::Dig, Ending::Lost, _, Some(_), _) => {
                self.mapped("cover, dig after the block or of a freeball, error: F");
                Some((F, None))
            }
            (Skill::Dig, Ending::Lost, _, None, _) => Some((D, Some(Error))),
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
            // the serve without their reception, attacked: not over
            (Skill::Serve, Ending::Goes, '/', _, true) => {
                self.mapped("S/ attacked as S+");
                Some((S, Some(Positive)))
            }
            (
                Skill::Serve | Skill::Reception | Skill::Attack | Skill::Block,
                Ending::Goes,
                e,
                _,
                _,
            ) => Some((event_of(kind), Some(eval(e)?))),
            // our cover: it rebuilds (`!`) or struggles (`-`), over the net
            // our freeball
            (Skill::Dig, Ending::Goes, '/', Some(Kept::Cover | Kept::Freeball), _) => {
                self.mapped("cover or dig of a freeball over the net: FB");
                Some((FB, None))
            }
            (Skill::Dig, Ending::Goes, '#' | '+' | '!', Some(Kept::Cover), _) => {
                Some((D, Some(Exclamative)))
            }
            (Skill::Dig, Ending::Goes, _, Some(Kept::Cover), _) => Some((D, Some(Negative))),
            // our dig of their freeball: in the freeball
            (Skill::Dig, Ending::Goes, _, Some(Kept::Freeball), _) => {
                self.mapped("dig of a freeball: in the freeball");
                None
            }
            // `!` lets us build: our dig `!` is only a cover
            (Skill::Dig, Ending::Goes, '!', _, _) => {
                self.mapped("D! as D+");
                Some((D, Some(Positive)))
            }
            (Skill::Dig, Ending::Goes, e, _, _) => Some((D, Some(eval(e)?))),
            // our freeball sent over
            (Skill::Freeball, Ending::Goes, _, _, _) if sent => Some((FB, None)),
            // their freeball, received by us: told by their freeball sent
            // (`dvw::decide`), or after our serve `/` by the serve
            (Skill::Freeball, Ending::Goes, _, _, _) => {
                self.mapped("F received: in the freeball");
                None
            }
            // the set: not scouted, unless it goes over (`/`), our
            // freeball
            (Skill::Set, Ending::Goes, '/', _, _) => {
                self.mapped("set over: FB");
                Some((FB, None))
            }
            (Skill::Set, Ending::Goes, _, _, _) => None,
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
                    A => set_type(skill, self.combinations).map(TechniqueEnum::Set),
                    _ => None,
                };
                // where our serve or attack went, or where we received or
                // dug theirs: the zone, with its subzone if told (a cell
                // of ours; DataVolley tells no two cells); where we blocked
                // it, only the zone at the net
                let target = match (event_type, skill.end_zone) {
                    (S | P | A | D, Some(zone)) => {
                        CourtTarget::of_zone(zone, skill.end_subzone.and_then(SubzoneEnum::of))
                    }
                    (B, Some(zone)) => {
                        CourtTarget::of_zone(zone, None).filter(|t| TargetKind::Net.fits(t))
                    }
                    _ => None,
                };
                // how many of theirs blocked our attack, or of ours blocked
                // theirs
                let blockers = skill
                    .num_players
                    .and_then(BlockersEnum::of_datavolley)
                    .filter(|b| BlockersEnum::of(event_type).contains(b));
                // where our serve started from, behind our end line
                let serve_start = match event_type {
                    S => skill.start_zone.and_then(ServeStartEnum::numbered),
                    _ => None,
                };
                self.apply_flagged(
                    event_type,
                    player,
                    eval,
                    None,
                    technique,
                    EventFlags::default()
                        .with_target(target)
                        .with_blockers(blockers)
                        .with_serve_start(serve_start),
                )
            }
            None => Ok(()),
        }
    }

    /// Their touch: what it says about the end of the rally, for the
    /// closing event (their attack blocked, our block untold, is their
    /// error: the block is optional); their freeball received is our
    /// freeball (`FB`), a team event, unless our freeball sent tells it
    /// already; their freeball sent is theirs (`OF`), unless our serve `/`
    /// tells it.
    fn their_touch(
        &mut self,
        side: Side,
        skill: &SkillCode,
        sent: bool,
    ) -> Result<Option<(Side, Ending)>, Issue> {
        let ending = self.ending(skill.skill, skill.evaluation);
        let last = self.rally_events().last().map(|e| (e.event_type, e.eval));
        match (skill.skill, ending, skill.evaluation, sent, last) {
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
            // their freeball received: the freeball sent, ours, is told
            // already (`dvw::decide`)
            (Skill::Freeball, Ending::Goes, _, false, _) => Ok(None),
            // their set over the net: their freeball
            (Skill::Set, Ending::Goes, '/', _, _) => {
                self.mapped("their set over: OF");
                self.apply(EventTypeEnum::OF, None, None, None, None)
                    .map(|_| None)
            }
            (_, Ending::Goes, _, _, _) => Ok(None),
            (_, ending, _, _, _) => Ok(Some((side, ending))),
        }
    }
}

/// What the ball was left by the touch before our dig: our attack `!`
/// (our dig a cover), our block keeping it, their freeball or their
/// reception back over after our serve `/` (no attack: our first touch
/// isn't scouted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kept {
    Cover,
    Block,
    Freeball,
}

/// Our event of a touch of ours with an evaluation.
fn event_of(skill: Skill) -> EventTypeEnum {
    match skill {
        Skill::Serve => EventTypeEnum::S,
        Skill::Reception => EventTypeEnum::P,
        Skill::Block => EventTypeEnum::B,
        Skill::Dig => EventTypeEnum::D,
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

/// The set of an attack, told by its combination (`X5`): its tempo and the
/// zone it starts from, as `[3ATTACKCOMBINATION]` defines it, or those of
/// the attack when the file doesn't define it. Every quick is in zone 3 or
/// 4: the quick of the middle blockers is told by its code, the one
/// DataVolley comes with. None when the attack doesn't tell it (the setter
/// tipping, `O`; a quick of another code).
pub(crate) fn set_type(
    attack: &SkillCode,
    combinations: &[AttackCombination],
) -> Option<SetTypeEnum> {
    use SetTypeEnum::*;
    let code = attack.combination.as_deref();
    let defined = code.and_then(|code| combinations.iter().find(|c| c.code == code));
    let tempo = defined.and_then(|c| c.tempo).unwrap_or(attack.skill_type);
    let zone = defined.and_then(|c| c.start_zone).or(attack.start_zone);
    match (attack.skill, tempo, zone, code) {
        (Skill::Attack, 'Q', _, Some("X1")) => Some(One),
        (Skill::Attack, 'Q', _, Some("X2")) => Some(Two),
        (Skill::Attack, 'Q', _, Some("X7")) => Some(Seven),
        (Skill::Attack, 'Q', _, Some("XC")) => Some(C),
        (Skill::Attack, 'Q', _, Some("XD")) => Some(DoubleC),
        (Skill::Attack, 'Q', _, Some("XF" | "CB" | "CD" | "CF")) | (Skill::Attack, 'N', _, _) => {
            Some(Fast)
        }
        (Skill::Attack, 'H', _, _) => Some(High),
        (Skill::Attack, 'U', _, _) => Some(Super),
        (Skill::Attack, 'T' | 'M', Some(8), _) => Some(Pipe),
        (Skill::Attack, 'T', Some(4), _) => Some(Five),
        (Skill::Attack, 'T', Some(2), _) => Some(Six),
        (Skill::Attack, 'T', Some(9), _) => Some(Eight),
        (Skill::Attack, 'M', _, _) => Some(Quick),
        _ => None,
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
