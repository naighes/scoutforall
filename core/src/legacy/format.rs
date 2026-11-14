//! The files of a team and of a match as the versions before the database
//! saved them, in a folder or in an archive: `team.json`, `match.json`,
//! `set_<n>.json` (the lineup), `set_<n>.csv` (the events). With the rules
//! for what the older versions left out.

use crate::{
    errors::{AppError, IOError},
    logging::logger::log_error,
    shapes::{enums::LineupRulesEnum, r#match::MatchEntry, set::SetEntry, snapshot::EventEntry},
};
use csv::ReaderBuilder;
use serde_json::{Value, from_slice, from_value};

/// The match of a descriptor, as saved in its folder or in an archive,
/// without its sets; and the lineup rules of its sets saved without them:
/// only matches created since v0.3.0 save the substitutions, and v0.3.0
/// didn't save the rules of its sets, but scouted them with V2.
pub(crate) fn read_descriptor(
    content: &[u8],
) -> Result<(MatchEntry, Option<LineupRulesEnum>), AppError> {
    let descriptor: Value = from_slice(content).map_err(|e| AppError::IO(IOError::from(e)))?;
    let rules = descriptor
        .get("max_substitutions")
        .map(|_| LineupRulesEnum::V2);
    let entry: MatchEntry = from_value(descriptor).map_err(|e| AppError::IO(IOError::from(e)))?;
    Ok((entry, rules))
}

/// The sets saved without their rules get `rules`, from `read_descriptor`.
pub(crate) fn fill_lineup_rules(sets: &mut [SetEntry], rules: Option<LineupRulesEnum>) {
    for set in sets.iter_mut() {
        set.lineup_rules = set.lineup_rules.or(rules);
    }
}

/// The lineup of set `set_number`, without its events.
pub(crate) fn read_lineup(set_number: u8, content: &[u8]) -> Option<SetEntry> {
    from_slice(content).ok().map(|entry: SetEntry| SetEntry {
        set_number,
        ..entry
    })
}

/// The number of the set whose lineup (`json`) or events (`csv`) are in
/// the file `name`, and the extension: e.g. `Some(2)` and `csv` for
/// `set_2.csv`, `None` and `json` for `set_300.json` (no set has it).
pub(crate) fn set_file(name: &str) -> Option<(Option<u8>, &str)> {
    let (number, extension) = name.strip_prefix("set_")?.split_once('.')?;
    match !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) {
        true => Some((number.parse::<u8>().ok(), extension)),
        false => None,
    }
}

/// The events saved in the blob. Rows with and without the technique may be
/// mixed, e.g. when v0.4.3 or older adds events to a set scouted by a later
/// version. A last row without its line end was cut while appended (the
/// battery out): it's left out, the set stays readable.
pub(crate) fn read_events(csv: &[u8]) -> Result<Vec<EventEntry>, AppError> {
    let whole = whole_rows(csv);
    if whole.len() < csv.len() {
        log_error(&format!(
            "the last event of a set was cut, left out: {:?}",
            String::from_utf8_lossy(csv.get(whole.len()..).unwrap_or_default())
        ));
    }
    let mut reader = ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(whole);
    // a row that can't be read makes the set unreadable: without it the set
    // would replay to something else
    reader
        .deserialize()
        .collect::<Result<Vec<EventEntry>, csv::Error>>()
        .map_err(|e| AppError::IO(IOError::from(e)))
}

/// The rows that end with a line end: every event is saved with one.
pub(crate) fn whole_rows(csv: &[u8]) -> &[u8] {
    csv.iter()
        .rposition(|b| *b == b'\n')
        .and_then(|end| csv.get(..=end))
        .unwrap_or_default()
}
