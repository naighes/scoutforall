use crate::shapes::snapshot::Snapshot;
use crate::shapes::team::TeamEntry;
use crate::structs::MenuFlow;
use crossterm::event::{read, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{self, Write};
use uuid::Uuid;

fn prompt_numeric_choice<T>(
    prompt: &str,
    items: &[(u8, T)],
    render: impl Fn(&T) -> String,
) -> Result<u8, MenuFlow> {
    println!("\n{}", prompt);
    for (num, item) in items {
        println!("  {}. {}", num, render(item));
    }
    print!("press number key (Esc to cancel): ");
    io::stdout().flush().ok();
    enable_raw_mode().ok();
    let result = loop {
        if let Ok(Event::Key(event)) = read() {
            match event.code {
                KeyCode::Char(c) if c.is_ascii_digit() => {
                    if let Some(d) = c.to_digit(10).map(|n| n as u8) {
                        if items.iter().any(|(code, _)| *code == d) {
                            println!();
                            break Ok(d);
                        }
                    }
                    println!("\ninvalid selection '{}': try again:", c);
                    io::stdout().flush().ok();
                }
                KeyCode::Esc => {
                    println!("\ncanceled");
                    break Err(MenuFlow::Back);
                }
                _ => {}
            }
        }
    };
    disable_raw_mode().ok();
    result
}

pub fn prompt_substitution(
    snapshot: &Snapshot,
    team: &TeamEntry,
) -> Result<(Uuid, Uuid), MenuFlow> {
    let valid_options = snapshot.current_lineup.get_repleceable_lineup();
    if valid_options.is_empty() {
        println!("no valid players to pull out: press any key to return...");
        let _ = read();
        return Err(MenuFlow::Back);
    }
    // choosing the replaced
    let choice_out = prompt_numeric_choice(
        "select the player to pull out:",
        &valid_options,
        |(role, id)| {
            format!(
                "{:<20} ({})",
                role,
                team.players
                    .iter()
                    .find(|p| p.id == *id)
                    .map_or("unknown", |p| p.name.as_str())
            )
        },
    )?;
    let replaced_id = valid_options
        .iter()
        .find(|(num, _)| *num == choice_out)
        .map(|(_, (_, id))| *id)
        .unwrap();
    // choosing the replacement
    let available_subs = snapshot
        .current_lineup
        .get_available_replacements(team, replaced_id);
    if available_subs.is_empty() {
        println!("no available replacements: press any key to return...");
        let _ = read();
        return Err(MenuFlow::Back);
    }
    let choice_in = prompt_numeric_choice("select the replacement:", &available_subs, |p| {
        format!("{:<20} ({:?})", p.name, p.role)
    })?;
    let replacement_id = available_subs
        .iter()
        .find(|(num, _)| *num == choice_in)
        .map(|(_, p)| p.id)
        .unwrap();
    Ok((replaced_id, replacement_id))
}
