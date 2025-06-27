use crate::structs::MenuFlow;
use crate::team::{prompt_team, show_teams};
use crate::util::clear_screen;
use inquire::{InquireError, Select};

pub fn run_menu() {
    clear_screen();
    loop {
        let options = vec!["new team", "teams", "exit"];
        let ans = Select::new("choose an option:", options.clone()).prompt();
        match ans {
            Ok("new team") => {
                clear_screen();
                prompt_team();
            }
            Ok("teams") => {
                clear_screen();
                match show_teams() {
                    Ok(MenuFlow::Continue | MenuFlow::Back) => {}
                    Err(e) => {
                        eprintln!("error: {}", e);
                        break;
                    }
                }
            }
            Ok("exit") | Err(InquireError::OperationCanceled) => {
                println!("👋 bye");
                break;
            }
            Err(e) => {
                eprintln!("something went wrong: {}", e);
                break;
            }
            _ => {}
        }
    }
}
