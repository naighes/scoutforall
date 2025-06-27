use chrono::{DateTime, FixedOffset, NaiveDate, TimeZone};
use crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{Clear, ClearType},
};
use inquire::{InquireError, Select, Text};
use std::{collections::HashMap, io::stdout};
use uuid::Uuid;

use crate::structs::MenuFlow;

pub fn clear_screen() {
    let mut stdout = stdout();
    execute!(stdout, Clear(ClearType::All), MoveTo(0, 0)).unwrap();
}

pub fn sanitize_filename(s: &str) -> String {
    s.to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// prompting

// TODO: remove?
/// Handles user input errors and cancellation for prompt-based interactions.
///
/// If the prompt succeeds, returns `Ok(value)`.
/// If the user cancels the operation, returns `Err(MenuFlow::Back)`.
/// If any other error occurs, logs the error and also returns `Err(MenuFlow::Back)`.
pub fn prompt_or_back<T>(res: Result<T, InquireError>) -> Result<T, MenuFlow> {
    match res {
        Ok(val) => Ok(val),
        Err(InquireError::OperationCanceled) => Err(MenuFlow::Back),
        Err(e) => {
            eprintln!("unexpected error: {}", e);
            Err(MenuFlow::Back)
        }
    }
}

pub fn prompt_date(label: &str) -> Result<DateTime<FixedOffset>, InquireError> {
    loop {
        match Text::new(label).prompt() {
            Ok(input) => match NaiveDate::parse_from_str(&input, "%Y-%m-%d") {
                Ok(date) => {
                    let naive_datetime = match date.and_hms_opt(0, 0, 0) {
                        Some(ndt) => ndt,
                        None => {
                            println!("invalid time when creating datetime, please try again");
                            continue;
                        }
                    };
                    let fixed_offset = match FixedOffset::east_opt(0) {
                        Some(fo) => fo,
                        None => {
                            println!("invalid fixed offset, please try again");
                            continue;
                        }
                    };
                    return Ok(fixed_offset.from_utc_datetime(&naive_datetime));
                }
                Err(_) => {
                    println!("invalid format: use YYYY-MM-DD, please try again");
                    continue;
                }
            },
            Err(InquireError::OperationCanceled) => return Err(InquireError::OperationCanceled),
            Err(e) => {
                eprintln!("unexpected error: {}", e);
                return Err(e);
            }
        }
    }
}

pub fn prompt_player_id_or_back(
    label: &str,
    all_labels: &[String],
    player_map: &HashMap<String, Uuid>,
) -> Result<Uuid, MenuFlow> {
    match prompt_or_back(Select::new(label, all_labels.to_vec()).prompt()) {
        Ok(selection) => match player_map.get(&selection) {
            Some(id) => Ok(*id),
            None => {
                println!("invalid choice: try again.");
                Err(MenuFlow::Back)
            }
        },
        Err(flow) => Err(flow),
    }
}
