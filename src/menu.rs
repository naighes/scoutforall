use crate::team;
use crate::util::clear_screen;
use inquire::Select;

pub fn run_menu() {
    clear_screen();
    loop {
        let options = vec!["create new team", "show teams", "exit"];
        let ans = Select::new("choose an option:", options.clone()).prompt();
        match ans {
            Ok(choice) => match choice {
                "create new team" => {
                    clear_screen();
                    team::input_team()
                }
                "show teams" => {
                    clear_screen();
                    team::show_teams()
                }
                "exit" => {
                    println!("👋 bye");
                    break;
                }
                _ => unreachable!(),
            },
            Err(_) => {
                println!("something went wrong");
                break;
            }
        }
    }
}
