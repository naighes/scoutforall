use crate::ops::{create_player, create_team, get_teams};
use crate::r#match::{prompt_match, show_matches};
use crate::shapes::enums::RoleEnum;
use crate::shapes::team::TeamEntry;
use crate::structs::MenuFlow;
use crate::util::{clear_screen, prompt_or_back};
use comfy_table::{Cell, ContentArrangement, Row, Table};
use inquire::{CustomType, Text};
use inquire::{InquireError, Select};

/// Prompts the user to enter team information (name, league, year) and creates a new team.
///
/// # Returns
/// - `Ok(MenuFlow::Continue)` if the team is created or the flow should continue.
/// - `Ok(MenuFlow::Back)` if the user cancels input or an error occurs during prompting.
/// - `Err` if an unexpected failure occurs during team creation.
/// prompt_team()?;
/// ```
pub fn prompt_team() -> Result<MenuFlow, Box<dyn std::error::Error>> {
    let name = match prompt_or_back(Text::new("team name:").prompt()) {
        Ok(v) => v,
        Err(flow) => return Ok(flow),
    };
    let league = match prompt_or_back(Text::new("league:").prompt()) {
        Ok(v) => v,
        Err(flow) => return Ok(flow),
    };
    let year = match prompt_or_back(
        CustomType::<u16>::new("year:")
            .with_error_message("please enter a valid year")
            .prompt(),
    ) {
        Ok(v) => v,
        Err(flow) => return Ok(flow),
    };
    match create_team(name, league, year) {
        Ok(team) => {
            println!("team '{}' saved successfully", team.name);
        }
        Err(e) => {
            eprintln!("error saving team: {}", e);
        }
    }

    Ok(MenuFlow::Continue)
}

/// Displays the list of available teams and allows the user to select one.
///
/// If no teams are found, a message is printed and control returns to the previous menu.
///
/// # Returns
/// - `Ok(MenuFlow::Back)` if the user selects "back" or cancels the operation.
/// - `Ok(MenuFlow::Continue)` if the user exits the team menu and chooses to continue.
/// - `Err` if an unexpected error occurs while prompting the selection.
///
/// # Errors
/// Returns an error if the prompt fails unexpectedly (other than user cancellation).
pub fn show_teams() -> Result<MenuFlow, Box<dyn std::error::Error>> {
    let mut teams = get_teams();
    if teams.is_empty() {
        println!("no teams found");
        return Ok(MenuFlow::Back);
    }
    loop {
        let mut team_names: Vec<String> = teams
            .iter()
            .map(|t| format!("{} – {} ({})", t.name, t.league, t.year))
            .collect();
        team_names.push("back".to_string());
        let selection = Select::new("select a team:", team_names.clone())
            .with_page_size(10)
            .prompt();
        match selection {
            Ok(choice) if choice == "back" => {
                clear_screen();
                return Ok(MenuFlow::Back);
            }
            Ok(choice) => {
                let index = team_names.iter().position(|s| s == &choice);
                if let Some(i) = index {
                    if let Some(team) = teams.get_mut(i) {
                        match team_menu(team)? {
                            MenuFlow::Continue | MenuFlow::Back => {
                                clear_screen();
                                continue;
                            }
                        }
                    } else {
                        println!("invalid selection");
                    }
                } else {
                    println!("unexpected selection");
                }
            }
            Err(InquireError::OperationCanceled) => {
                clear_screen();
                return Ok(MenuFlow::Back);
            }
            Err(e) => {
                eprintln!("unexpected error: {}", e);
                return Err(Box::new(e));
            }
        }
    }
}

pub fn print_team_details(team: &TeamEntry) {
    let mut info_table = Table::new();
    info_table.set_content_arrangement(ContentArrangement::Dynamic);
    info_table.set_header(vec![
        Cell::new("name"),
        Cell::new("league"),
        Cell::new("year"),
    ]);
    let mut row: Row = Row::new();
    row.add_cell(Cell::new(&team.name));
    row.add_cell(Cell::new(&team.league));
    row.add_cell(Cell::new(team.year));
    info_table.add_row(row);
    println!("{}", info_table);
    let mut players_table = Table::new();
    players_table.set_content_arrangement(ContentArrangement::Dynamic);
    for player in team.players.iter() {
        let mut row: Row = Row::new();
        let str = format!("{} ({}) - {}", player.name, player.number, player.role);
        row.add_cell(Cell::new(str));
        players_table.add_row(row);
    }
    println!("{}", players_table);
}

fn team_menu(team: &mut TeamEntry) -> Result<MenuFlow, Box<dyn std::error::Error>> {
    clear_screen();
    loop {
        print_team_details(team);
        let options = vec!["new player", "matches", "new match", "back"];
        let choice = match prompt_or_back(
            Select::new("choose an option:", options.clone())
                .with_page_size(10)
                .prompt(),
        ) {
            Ok(v) => v,
            Err(flow) => return Ok(flow),
        };
        let flow = match &*choice {
            "new player" => prompt_new_player(team),
            "matches" => show_matches(team),
            "new match" => prompt_match(team),
            "back" => return Ok(MenuFlow::Back),
            _ => {
                eprintln!("invalid option: {}", choice);
                continue;
            }
        };
        match flow {
            Ok(MenuFlow::Continue) => continue,
            Ok(MenuFlow::Back) => continue,
            Err(e) => {
                eprintln!("error handling '{}': {}", choice, e);
                continue;
            }
        }
    }
}

/// Prompts the user to input player data and adds the player to the given team.
///
/// # Arguments
/// - `team`: A mutable reference to the team where the player should be added.
///
/// # Returns
/// - `Ok(MenuFlow::Continue)` if the player is successfully created.
/// - `Ok(MenuFlow::Back)` if the user cancels input or an error occurs during prompting.
/// - `Err` if player creation fails (e.g. validation, persistence errors).
fn prompt_new_player(team: &mut TeamEntry) -> Result<MenuFlow, Box<dyn std::error::Error>> {
    let name = match prompt_or_back(Text::new("player name:").prompt()) {
        Ok(v) => v,
        Err(flow) => return Ok(flow),
    };
    let role_labels = vec![
        "Libero",
        "Opposite Hitter",
        "Setter",
        "Outside Hitter",
        "Middle Blocker",
    ];
    let roles = [
        RoleEnum::Libero,
        RoleEnum::OppositeHitter,
        RoleEnum::Setter,
        RoleEnum::OutsideHitter,
        RoleEnum::MiddleBlocker,
    ];
    let selected_label = match prompt_or_back(Select::new("role:", role_labels.clone()).prompt()) {
        Ok(v) => v,
        Err(flow) => return Ok(flow),
    };
    let selected_index = role_labels
        .iter()
        .position(|r| r == &selected_label)
        .expect("selected role not found in label list");
    let role = roles[selected_index].clone();
    let number = match prompt_or_back(
        CustomType::<u8>::new("number:")
            .with_error_message("insert a valid number")
            .prompt(),
    ) {
        Ok(v) => v,
        Err(flow) => return Ok(flow),
    };
    create_player(name, role, number, team)?;
    clear_screen();
    Ok(MenuFlow::Continue)
}
