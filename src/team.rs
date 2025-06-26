use crate::ops::{create_player, create_team, get_teams, RoleEnum, TeamEntry};
use crate::r#match::{prompt_match, show_matches};
use crate::util::clear_screen;
use inquire::Select;
use inquire::{CustomType, Text};

pub fn input_team() {
    let name = Text::new("team name:")
        .prompt()
        .expect("failed to read team name");
    let league = Text::new("league:")
        .prompt()
        .expect("failed to read league");
    let year: u16 = CustomType::new("year:")
        .with_error_message("please enter a valid year")
        .prompt()
        .expect("failed to read year");
    match create_team(name, league, year) {
        Ok(team) => {
            println!("team '{}' saved successfully", team.name);
        }
        Err(e) => {
            eprintln!("error saving team: {}", e);
        }
    }
}

pub fn show_teams() {
    let mut teams = get_teams();
    if teams.is_empty() {
        println!("no teams found");
        return;
    }
    let team_names: Vec<String> = teams
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{}. {} – {} ({})", i + 1, t.name, t.league, t.year))
        .collect();
    let team_selection = Select::new("select a team:", team_names.clone())
        .with_page_size(10)
        .prompt();
    match team_selection {
        Ok(selected) => {
            let index = team_names.iter().position(|s| s == &selected).unwrap();
            if let Some(team) = teams.get_mut(index) {
                team_menu(team);
            } else {
                println!("invalid selection");
            }
        }
        Err(_) => println!("no team selected"),
    }
}

fn show_players(team: &TeamEntry) {
    if team.players.is_empty() {
        println!("no players in this team");
    } else {
        println!("players:");
        for (_, p) in team.players.iter().enumerate() {
            println!("{} (#{}, {})", p.name, p.number, p.role);
        }
    }
}

fn team_menu(team: &mut TeamEntry) {
    loop {
        println!("\nteam: {}\n", team.name);
        let options = vec![
            "show players",
            "add player",
            "show matches",
            "new match",
            "back",
        ];

        let action = Select::new("choose an option:", options.clone())
            .with_page_size(10)
            .prompt();

        match action.as_deref() {
            Ok("show players") => {
                clear_screen();
                show_players(team)
            }
            Ok("add player") => {
                clear_screen();
                if let Err(e) = input_player(team) {
                    eprintln!("failed to add player: {}", e);
                }
            }
            Ok("show matches") => {
                clear_screen();
                show_matches(team)
            }
            Ok("new match") => {
                clear_screen();
                prompt_match(team)
            }
            Ok("back") | Err(_) => break,
            _ => println!("invalid option"),
        }
    }
}

fn input_player(team: &mut TeamEntry) -> Result<(), Box<dyn std::error::Error>> {
    let name = Text::new("player name:")
        .prompt()
        .unwrap_or_else(|_| String::from("unnamed"));

    let role_labels = vec![
        "Libero",
        "Opposite Hitter",
        "Setter",
        "Outside Hitter",
        "Middle Blocker",
    ];
    let roles = vec![
        RoleEnum::Libero,
        RoleEnum::OppositeHitter,
        RoleEnum::Setter,
        RoleEnum::OutsideHitter,
        RoleEnum::MiddleBlocker,
    ];

    let selected_index = Select::new("role:", role_labels.clone())
        .prompt()
        .map(|s| role_labels.iter().position(|r| r == &s).unwrap_or(0))
        .unwrap_or(0);

    let role = roles[selected_index].clone();

    let number = CustomType::new("number:")
        .with_error_message("insert a valid number")
        .prompt()
        .unwrap_or(0);

    let player = create_player(name, role, number, team)?;
    println!("player '{}' saved successfully", player.name);

    Ok(())
}
