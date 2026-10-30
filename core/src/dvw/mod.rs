//! The files of DataVolley (`.dvw`), also written by Click&Scout, ovscout2
//! and the export of VolleyStation: a text in sections, `[3MATCH]`,
//! `[3TEAMS]`... and `[3SCOUT]`, a line per touch of both teams. There's no
//! public specification: the reference is the parser of openvolley
//! (`datavolley`, R).

pub mod model;

pub(crate) mod meta;
pub(crate) mod scout;
pub(crate) mod text;

use crate::dvw::{
    meta::{Line, attack_combinations, match_info, players, program, sets, teams, winning_symbols},
    model::DvwFile,
    scout::scout_lines,
};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DvwError {
    #[error("not a DataVolley file")]
    NotDvw,
    #[error("missing section {0}")]
    MissingSection(&'static str),
    #[error("line {line}: {reason}")]
    Invalid { line: usize, reason: String },
}

/// Reads a file, as it's written: the meaning of the codes is up to the
/// import.
pub fn parse(bytes: &[u8]) -> Result<DvwFile, DvwError> {
    let content = text::decode(bytes);
    let sections = sections(&content);
    let section = |name: &'static str| {
        sections
            .get(name)
            .map(Vec::as_slice)
            .ok_or(DvwError::MissingSection(name))
    };
    let header = section("[3DATAVOLLEYSCOUT]").map_err(|_| DvwError::NotDvw)?;
    let (home, visiting) = teams(section("[3TEAMS]")?)?;
    Ok(DvwFile {
        generator: program(header, "GENERATOR"),
        last_change: program(header, "LASTCHANGE"),
        match_info: match_info(section("[3MATCH]")?)?,
        home,
        visiting,
        sets: sets(section("[3SET]")?)?,
        home_players: players(section("[3PLAYERS-H]")?)?,
        visiting_players: players(section("[3PLAYERS-V]")?)?,
        attack_combinations: attack_combinations(
            section("[3ATTACKCOMBINATION]").unwrap_or_default(),
        ),
        winning_symbols: winning_symbols(section("[3WINNINGSYMBOLS]").unwrap_or_default())?,
        scout: scout_lines(section("[3SCOUT]")?),
    })
}

// the lines of each section, by its name; numbered from 1 as in an editor
fn sections(content: &str) -> HashMap<&str, Vec<Line<'_>>> {
    let mut sections: HashMap<&str, Vec<Line>> = HashMap::new();
    let mut current: Option<&str> = None;
    for (number, line) in (1..).zip(content.lines()) {
        let line = line.trim_end_matches('\r');
        match (
            line.starts_with("[3") && line.trim_end().ends_with(']'),
            current,
        ) {
            (true, _) => {
                let name = line.trim_end();
                sections.entry(name).or_default();
                current = Some(name);
            }
            (false, Some(name)) => sections.entry(name).or_default().push((number, line)),
            (false, None) => {}
        }
    }
    sections
}
