//! The sections before `[3SCOUT]`: the match, the teams, the sets, the
//! players and the tables of the codes.

use crate::dvw::{
    DvwError, Field,
    model::{
        AttackCombination, DvwRole, MatchDate, MatchInfo, Player, SetResult, SetStart, Skill, Team,
        WinningSymbols,
    },
    text::prefer_utf8,
};
use chrono::{Days, NaiveDate, NaiveTime};

/// A line of a section, with its number in the file (from 1).
pub(crate) type Line<'a> = (usize, &'a str);

pub(crate) fn fields(line: &str) -> Vec<&str> {
    line.split(';').collect()
}

pub(crate) fn at<'a>(fields: &[&'a str], index: usize) -> &'a str {
    fields.get(index).map(|f| f.trim()).unwrap_or_default()
}

pub(crate) fn non_empty(text: &str) -> Option<String> {
    Some(text.trim())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

fn invalid(line: usize, field: Field, text: &str) -> DvwError {
    DvwError::Invalid {
        line,
        field,
        text: text.to_string(),
    }
}

/// `GENERATOR-PRG: Data Volley` and `GENERATOR-REL: Release 3.7.3` as
/// "Data Volley Release 3.7.3".
pub(crate) fn program(header: &[Line], prefix: &str) -> Option<String> {
    let value = |key: &str| {
        header
            .iter()
            .find_map(|(_, l)| l.strip_prefix(&format!("{prefix}-{key}:")))
            .and_then(non_empty)
    };
    let parts: Vec<String> = [value("PRG"), value("REL")].into_iter().flatten().collect();
    Some(parts.join(" ")).filter(|p| !p.is_empty())
}

pub(crate) fn match_info(lines: &[Line]) -> Result<MatchInfo, DvwError> {
    let (number, first) = lines
        .first()
        .copied()
        .ok_or(DvwError::MissingSection("[3MATCH]"))?;
    let first = fields(first);
    // the second line keeps the day as a number, as in a spreadsheet: it
    // tells day first from month first, when it's not a placeholder
    let serial = lines
        .get(1)
        .and_then(|(_, l)| at(&fields(l), 2).parse::<u64>().ok())
        .and_then(|days| NaiveDate::from_ymd_opt(1899, 12, 30)?.checked_add_days(Days::new(days)));
    let time = at(&first, 1).replace('.', ":");
    Ok(MatchInfo {
        date: match_date(at(&first, 0), serial)
            .ok_or_else(|| invalid(number, Field::Date, at(&first, 0)))?,
        time: NaiveTime::parse_from_str(&time, "%H:%M:%S")
            .or_else(|_| NaiveTime::parse_from_str(&time, "%H:%M"))
            .ok(),
        season: non_empty(at(&first, 2)),
        league: non_empty(at(&first, 3)),
        phase: non_empty(at(&first, 4)),
        text_encoding: non_empty(at(&first, 8)),
    })
}

// `25/01/2015`, `01/25/2015`, `2015/01/25`, also with `-` or `.`; None
// when it's not a date at all
pub(crate) fn match_date(text: &str, serial: Option<NaiveDate>) -> Option<MatchDate> {
    let parts: Vec<&str> = text.split(['/', '-', '.']).map(str::trim).collect();
    let numbers: Vec<u32> = parts.iter().filter_map(|p| p.parse().ok()).collect();
    let ymd = |y: u32, m: u32, d: u32| NaiveDate::from_ymd_opt(i32::try_from(y).ok()?, m, d);
    match (text.trim().is_empty(), parts.as_slice(), numbers.as_slice()) {
        (true, _, _) => Some(MatchDate::Missing),
        (false, [year, _, _], [y, m, d]) if year.len() == 4 => {
            ymd(*y, *m, *d).map(MatchDate::Known)
        }
        (false, [_, _, _], [a, b, y]) => {
            let candidates = (ymd(*y, *b, *a), ymd(*y, *a, *b));
            match (candidates, serial) {
                ((Some(day_first), Some(month_first)), _) if day_first == month_first => {
                    Some(MatchDate::Known(day_first))
                }
                ((Some(day_first), Some(month_first)), Some(s))
                    if s == day_first || s == month_first =>
                {
                    Some(MatchDate::Known(s))
                }
                ((Some(day_first), Some(month_first)), _) => {
                    Some(MatchDate::Ambiguous(day_first, month_first))
                }
                ((Some(date), None) | (None, Some(date)), _) => Some(MatchDate::Known(date)),
                ((None, None), _) => None,
            }
        }
        _ => None,
    }
}

/// The two lines of `[3TEAMS]`: home first.
pub(crate) fn teams(lines: &[Line]) -> Result<(Team, Team), DvwError> {
    let team = |index: usize| {
        lines
            .get(index)
            .map(|(_, l)| team(&fields(l)))
            .ok_or(DvwError::MissingSection("[3TEAMS]"))
    };
    Ok((team(0)?, team(1)?))
}

fn team(f: &[&str]) -> Team {
    Team {
        code: at(f, 0).to_string(),
        name: prefer_utf8(at(f, 1), f.get(6).copied()),
        sets_won: at(f, 2).parse().ok(),
        coach: non_empty(&prefer_utf8(at(f, 3), f.get(7).copied())),
        assistant: non_empty(&prefer_utf8(at(f, 4), f.get(8).copied())),
    }
}

/// `True; 4- 8;12-16;16-21;21-25;22;`: the scores at 8, 16 and 21, the
/// final one, the minutes.
pub(crate) fn sets(lines: &[Line]) -> Result<Vec<SetResult>, DvwError> {
    lines
        .iter()
        .filter(|(_, l)| !l.trim().is_empty())
        .zip(1u8..)
        .filter_map(|((number, l), set_number)| {
            let f = fields(l);
            match at(&f, 4) {
                "" => None,
                last => Some(
                    score(last)
                        .ok_or_else(|| invalid(*number, Field::Score, last))
                        .map(|final_score| SetResult {
                            set_number,
                            partials: (1..=3).filter_map(|i| score(at(&f, i))).collect(),
                            final_score,
                            duration: at(&f, 5).parse().ok(),
                        }),
                ),
            }
        })
        .collect()
}

fn score(text: &str) -> Option<(u8, u8)> {
    let (home, visiting) = text.split_once('-')?;
    Some((home.trim().parse().ok()?, visiting.trim().parse().ok()?))
}

pub(crate) fn players(lines: &[Line]) -> Result<Vec<Player>, DvwError> {
    lines
        .iter()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(number, l)| {
            let f = fields(l);
            let special = at(&f, 12);
            at(&f, 1)
                .parse()
                .map_err(|_| invalid(*number, Field::PlayerNumber, at(&f, 1)))
                .map(|player_number| Player {
                    number: player_number,
                    starts: (3..=7).map(|i| set_start(at(&f, i))).collect(),
                    id: at(&f, 8).to_string(),
                    surname: prefer_utf8(at(&f, 9), f.get(17).copied()),
                    name: prefer_utf8(at(&f, 10), f.get(18).copied()),
                    role: role(at(&f, 13)),
                    libero: special.contains('L'),
                    captain: special.contains('C'),
                })
        })
        .collect()
}

fn set_start(text: &str) -> SetStart {
    match (text, text.parse::<u8>()) {
        ("*", _) => SetStart::Substitute,
        (_, Ok(zone)) => SetStart::Zone(zone),
        _ => SetStart::NotPlayed,
    }
}

fn role(text: &str) -> Option<DvwRole> {
    match text {
        "1" => Some(DvwRole::Libero),
        "2" => Some(DvwRole::OutsideHitter),
        "3" => Some(DvwRole::Opposite),
        "4" => Some(DvwRole::MiddleBlocker),
        "5" => Some(DvwRole::Setter),
        _ => None,
    }
}

pub(crate) fn attack_combinations(lines: &[Line]) -> Vec<AttackCombination> {
    lines
        .iter()
        .map(|(_, l)| fields(l))
        .filter(|f| !at(f, 0).is_empty())
        .map(|f| AttackCombination {
            code: at(&f, 0).to_string(),
            start_zone: at(&f, 1).parse().ok(),
            side: at(&f, 2).chars().next(),
            tempo: at(&f, 3).chars().next(),
            description: at(&f, 4).to_string(),
            target: at(&f, 8).chars().next(),
        })
        .collect()
}

/// 7 skills × 8 characters: 4 evaluations that lose the point, 4 that win
/// it, `~` when unused. An empty section: None.
pub(crate) fn winning_symbols(lines: &[Line]) -> Result<Option<WinningSymbols>, DvwError> {
    match lines.iter().find(|(_, l)| !l.trim().is_empty()) {
        None => Ok(None),
        Some((number, l)) => {
            let chars: Vec<char> = l.trim().chars().collect();
            let symbols = |part: &[char]| part.iter().copied().filter(|c| *c != '~').collect();
            match chars.len() {
                56 => Ok(Some(WinningSymbols {
                    skills: Skill::ALL
                        .iter()
                        .zip(chars.chunks(8))
                        .map(|(skill, part)| {
                            let (losing, winning) = part.split_at(4);
                            (*skill, symbols(losing), symbols(winning))
                        })
                        .collect(),
                })),
                _ => Err(invalid(*number, Field::WinningSymbols, "")),
            }
        }
    }
}
