//! The choice of a gym in a form: the pinned first, then the ones the team
//! used (TODO/021 §13).

use {
    crate::form::Value,
    scout4all_mobile::Scout4All,
};

const EMPTY: &str = "–";

/// The gyms a form of the team offers, and the one `current` even when it
/// was removed: id and name, in the order of the choice (pinned first, then
/// the ones the team used).
pub fn venue_options(
    core: &Scout4All,
    team_id: &str,
    current: Option<&str>,
) -> Vec<(String, String)> {
    let mut options: Vec<(String, String)> = core
        .venue_choices(team_id.to_string(), String::new())
        .unwrap_or_default()
        .into_iter()
        .map(|v| (v.id, v.name))
        .collect();
    if let Some(kept) = current
        .filter(|id| !options.iter().any(|(o, _)| o == id))
        .and_then(|id| core.venue(id.to_string()).ok())
    {
        options.push((kept.id, kept.name));
    }
    options
}

/// The choice of a gym: the empty one, then `options`; `current` is the
/// one chosen.
pub fn venue_value(options: &[(String, String)], current: Option<&str>) -> Value {
    Value::Choice {
        options: std::iter::once(EMPTY.to_string())
            .chain(options.iter().map(|(_, name)| name.clone()))
            .collect(),
        index: current
            .and_then(|id| options.iter().position(|(o, _)| o == id))
            .map_or(0, |at| at + 1),
    }
}

/// The id of the gym at the position `index` of a choice made by `venue_value`.
pub fn venue_at(options: &[(String, String)], index: usize) -> Option<String> {
    index
        .checked_sub(1)
        .and_then(|at| options.get(at))
        .map(|(id, _)| id.clone())
}
