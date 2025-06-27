use crate::shapes::enums::TeamSideEnum;
use crate::shapes::r#match::MatchEntry;
use crate::shapes::set::SetEntry;
use crate::structs::MenuFlow;
use crate::util::prompt_player_id_or_back;
use inquire::{InquireError, Select};
use std::collections::HashMap;
use uuid::Uuid;

/// get available players, excluding those who've been already selected
fn available_labels(all: &[String], selected: &[String]) -> Vec<String> {
    all.iter()
        .filter(|l| !selected.contains(l))
        .cloned()
        .collect()
}

/// select players for the six positions
fn prompt_positions_selection(
    all_labels: &[String],
    player_map: &HashMap<String, Uuid>,
) -> Result<[Uuid; 6], Box<dyn std::error::Error>> {
    let mut selected_labels = Vec::new();
    let mut selected_positions = Vec::new();
    for i in 0..6 {
        let labels = available_labels(all_labels, &selected_labels);
        let choice =
            Select::new(&format!("select player for position {}:", i + 1), labels).prompt()?;
        let id = *player_map.get(&choice).ok_or("invalid player choice")?;
        selected_positions.push(id);
        selected_labels.push(choice);
    }
    let len = selected_positions.len();
    let positions: [Uuid; 6] = selected_positions
        .try_into()
        .map_err(|_| format!("expected exactly 6 players, got {}", len))?;
    Ok(positions)
}

/// select libero prompt
fn prompt_libero_selection(
    all_labels: &[String],
    selected_labels: &[String],
    player_map: &HashMap<String, Uuid>,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let available = available_labels(all_labels, selected_labels);
    prompt_player_id_or_back("select libero:", &available, player_map)
        .map_err(|_| "libero selection failed".into())
}

/// select setter prompt
fn prompt_setter_selection(
    selected_labels: &[String],
    player_map: &HashMap<String, Uuid>,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let selected_map: HashMap<String, Uuid> = selected_labels
        .iter()
        .map(|label| (label.clone(), *player_map.get(label).unwrap()))
        .collect();
    prompt_player_id_or_back("select setter:", selected_labels, &selected_map)
        .map_err(|_| "setter selection failed".into())
}

/// set lineup prompt
fn prompt_set_lineup(
    m: &MatchEntry,
) -> Result<([Uuid; 6], Uuid, Uuid), Box<dyn std::error::Error>> {
    // players map
    let player_map: HashMap<String, Uuid> = m
        .team
        .players
        .iter()
        .map(|p| (format!("{} (#{})", p.name, p.number), p.id))
        .collect();
    let all_labels: Vec<String> = player_map.keys().cloned().collect();
    // six positions
    let positions = prompt_positions_selection(&all_labels, &player_map)?;
    // libero
    let selected_labels: Vec<String> = positions
        .iter()
        .map(|id| {
            player_map
                .iter()
                .find(|(_, &v)| v == *id)
                .map(|(k, _)| k.clone())
                .unwrap()
        })
        .collect();
    let libero_id = prompt_libero_selection(&all_labels, &selected_labels, &player_map)?;
    // setter
    let setter_id = prompt_setter_selection(&selected_labels, &player_map)?;
    Ok((positions, libero_id, setter_id))
}

pub fn prompt_set_details(
    m: &MatchEntry,
    set_number: u8,
    sets: &[SetEntry],
) -> Result<(TeamSideEnum, Vec<Uuid>, Uuid, Uuid), MenuFlow> {
    let serving_team = if set_number == 1 || set_number == 5 {
        // set 1 or 5: decide who serve
        match Select::new("who serves first?", vec!["us", "opponent"]).prompt() {
            Ok(choice) => match choice.as_ref() {
                "us" => TeamSideEnum::Us,
                "opponent" => TeamSideEnum::Them,
                // not recognized: back
                _ => return Err(MenuFlow::Back),
            },
            // cancel: back
            Err(InquireError::OperationCanceled) => return Err(MenuFlow::Back),
            // generic error: back
            Err(e) => {
                eprintln!("unexpected error: {}", e);
                return Err(MenuFlow::Back);
            }
        }
    } else {
        // set 2-4: invert server of previous set
        let previous_set_number = set_number - 1;
        let prev_team = sets
            .iter()
            .find(|s| s.set_number == previous_set_number)
            .map(|s| s.serving_team);
        match prev_team {
            Some(TeamSideEnum::Us) => TeamSideEnum::Them,
            Some(TeamSideEnum::Them) => TeamSideEnum::Us,
            None => return Err(MenuFlow::Back),
        }
    };
    // ask for set lineup
    let (positions_array, libero_id, setter_id) =
        prompt_set_lineup(m).map_err(|_| MenuFlow::Back)?;
    let positions = positions_array.to_vec();
    Ok((serving_team, positions, libero_id, setter_id))
}
