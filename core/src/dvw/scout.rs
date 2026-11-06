//! The lines of `[3SCOUT]`: the code, then the phases, the time, the set,
//! the zones of the setters, the second in the video and the six on court
//! of both teams.

use crate::dvw::{
    meta::{Line, at, fields, non_empty},
    model::{Code, ScoutLine, Side, Skill, SkillCode},
};

pub(crate) fn scout_lines(lines: &[Line]) -> Vec<ScoutLine> {
    lines
        .iter()
        // some sites append their page to the file they serve, e.g. MEVZA:
        // no code starts with `<`
        .take_while(|(_, l)| !l.starts_with('<'))
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(number, l)| {
            let f = fields(l);
            ScoutLine {
                line: *number,
                code: code(at(&f, 0)),
                point_phase: at(&f, 1).chars().next(),
                attack_phase: at(&f, 2).chars().next(),
                time: non_empty(at(&f, 7)),
                video_seconds: at(&f, 12).parse().ok(),
                set_number: at(&f, 8).parse().ok(),
                setter_zones: (at(&f, 9).parse().ok(), at(&f, 10).parse().ok()),
                home_court: court(&f, 14),
                visiting_court: court(&f, 20),
            }
        })
        .collect()
}

fn court(f: &[&str], from: usize) -> Option<[u8; 6]> {
    let numbers: Vec<u8> = (from..from + 6)
        .map_while(|i| at(f, i).parse().ok())
        .collect();
    numbers.try_into().ok()
}

fn code(text: &str) -> Code {
    let mut chars = text.chars();
    let marker = chars.next();
    let rest = chars.as_str();
    match (
        text.strip_prefix("**"),
        marker,
        marker.and_then(Side::from_marker),
    ) {
        (Some(end), _, _) => end_of_set(end).unwrap_or_else(|| Code::Unknown(text.to_string())),
        (None, Some('>'), _) => Code::Note(text.to_string()),
        (None, _, Some(side)) => {
            team_code(side, rest).unwrap_or_else(|| Code::Unknown(text.to_string()))
        }
        _ => Code::Unknown(text.to_string()),
    }
}

// `1set`
fn end_of_set(text: &str) -> Option<Code> {
    let number = text
        .to_ascii_lowercase()
        .strip_suffix("set")?
        .parse()
        .ok()?;
    Some(Code::EndOfSet(number))
}

fn team_code(side: Side, rest: &str) -> Option<Code> {
    // `>LUp` marks the lines of the lineup, at the start of a set
    let (code, lineup) = match rest.split_once('>') {
        Some((code, note)) if note.eq_ignore_ascii_case("LUp") => (code, true),
        _ => (rest, false),
    };
    let mut chars = code.chars();
    match (chars.next()?, chars.as_str()) {
        ('p', score) => pair(score).map(|(home, visiting)| Code::Point(side, home, visiting)),
        ('z', zone) => zone.parse().ok().map(|zone| Code::SetterZone(side, zone)),
        ('P', number) => number.parse().ok().map(|player| Code::Setter {
            side,
            player,
            lineup,
        }),
        // `c03.11` too, a slip of some scouts
        ('c', change) => pair(&change.replace('.', ":")).map(|(out, enters)| Code::Substitution {
            side,
            out,
            enters,
        }),
        ('T', "") => Some(Code::Timeout(side)),
        _ => touch(side, rest),
    }
}

// `01:00`
fn pair(text: &str) -> Option<(u8, u8)> {
    let (first, second) = text.split_once(':')?;
    Some((first.trim().parse().ok()?, second.trim().parse().ok()?))
}

// `06SM#~~~18C`, `$$B=` (the player unknown), `$$&H#` (no touch)
fn touch(side: Side, rest: &str) -> Option<Code> {
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let (player, code) = match (rest.strip_prefix("$$"), digits) {
        (Some(code), _) => (None, code),
        (None, 0) => return None,
        (None, _) => (Some(rest.get(..digits)?.parse().ok()?), rest.get(digits..)?),
    };
    let chars: Vec<char> = code.chars().collect();
    let char_at = |i: usize| chars.get(i).copied().filter(|c| *c != '~');
    let zone = |i: usize| {
        char_at(i)
            .and_then(|c| c.to_digit(10))
            .and_then(|z| u8::try_from(z).ok())
    };
    match (player, char_at(0)?) {
        (None, '&') => char_at(2).map(|evaluation| Code::Green(side, evaluation)),
        (_, skill) => Some(Code::Skill(
            side,
            SkillCode {
                player,
                skill: Skill::from_code(skill)?,
                skill_type: char_at(1)?,
                evaluation: char_at(2)?,
                combination: chars
                    .get(3..5)
                    .map(|c| c.iter().collect::<String>())
                    .filter(|c| !c.contains('~')),
                target: char_at(5),
                start_zone: zone(6),
                end_zone: zone(7),
                end_subzone: char_at(8),
                skill_subtype: char_at(9),
                num_players: char_at(10),
                special: char_at(11),
                custom: chars
                    .get(12..)
                    .map(|c| c.iter().collect())
                    .unwrap_or_default(),
            },
        )),
    }
}
