use crate::menu::run_menu;
use crate::ops::{
    append_event, compute_event, compute_snapshot, create_match, create_set, get_matches, get_mb1,
    get_mb2, get_oh1, get_oh2, get_opposite, get_serving_player, get_set_snapshot, get_set_winner,
    get_sets, get_setter, is_back_row_player, EvalEnum, EventTypeEnum, MatchEntry, PhaseEnum,
    PlayerEntry, SetEntry, TeamSideEnum,
};
use crate::ops::{EventEntry, Snapshot, TeamEntry};
use crate::util::clear_screen;
use chrono::{NaiveDate, ParseResult, Utc};
use comfy_table::{Cell, ContentArrangement, Row, Table};
use crossterm::event::{read, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use inquire::Select;
use inquire::Text;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::io;
use std::io::{stdout, Write};
use std::str::FromStr;
use uuid::Uuid;

fn prompt_date(label: &str) -> Result<NaiveDate, Box<dyn std::error::Error>> {
    let date_str = Text::new(label).prompt()?;
    let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?;
    Ok(date)
}

fn prompt_str(label: &str, fallback: &str) -> String {
    Text::new(label)
        .prompt()
        .unwrap_or_else(|_| fallback.into())
}

fn prompt_select(label: &str, options: Vec<&str>, fallback: &str) -> String {
    Select::new(label, options)
        .prompt()
        .unwrap_or(fallback)
        .into()
}

pub fn prompt_match(team: &TeamEntry) {
    // ask for match date
    let date = match prompt_date("match date (YYYY-MM-DD):") {
        Ok(d) => d,
        Err(_) => {
            eprintln!("invalid date format");
            return;
        }
    };
    // ask for opponent
    let opponent = prompt_str("opponent name:", "unknown");
    // ask for home/away
    let location = prompt_select("where is the match played?", vec!["home", "away"], "home");
    let home = match location {
        s if s == "home".to_string() => true,
        s if s == "away".to_string() => false,
        _ => true,
    };
    // create match and set
    let m = match create_match(team, opponent, date, home) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("could not create match: {}", e);
            return;
        }
    };
    // continue match
    let mut s = match continue_match(m.clone()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("could not continue match: {}", e);
            return;
        }
    };
    // start events loop
    if let Err(e) = events_loop(team, m.id, &mut s) {
        eprintln!("error during events loop: {}", e);
    }
}

pub fn prompt_set_lineup(
    m: &MatchEntry,
) -> Result<([Uuid; 6], Uuid, Uuid), Box<dyn std::error::Error>> {
    let mut player_map: HashMap<String, Uuid> = HashMap::new();
    let mut all_labels = Vec::new();
    for player in &m.team.players {
        let label = format!("{} (#{})", player.name, player.number);
        player_map.insert(label.clone(), player.id);
        all_labels.push(label);
    }
    // lineup selection
    let mut selected_positions: Vec<Uuid> = Vec::new();
    let mut selected_labels: Vec<String> = Vec::new();
    let mut i = 0;
    while i < 6 {
        let mut available_labels: Vec<String> = all_labels
            .iter()
            .filter(|label| !selected_labels.contains(label))
            .cloned()
            .collect();
        if i > 0 {
            available_labels.insert(0, "UNDO (go back to previous position)".to_string());
        }
        let choice = Select::new(
            &format!("select player for position {}:", i + 1),
            available_labels,
        )
        .prompt()?;
        if choice == "UNDO (go back to previous position)" {
            i -= 1;
            selected_positions.pop();
            selected_labels.pop();
            continue;
        }
        let id = *player_map.get(&choice).ok_or("invalid player choice")?;
        selected_positions.push(id);
        selected_labels.push(choice);
        i += 1;
    }
    let len = selected_positions.len();
    let positions: [Uuid; 6] = selected_positions
        .try_into()
        .map_err(|_| format!("expected exactly 6 players for lineup, but got {}", len))?;
    // libero selection
    let libero_id = loop {
        let mut libero_options = all_labels.clone();
        libero_options.insert(0, "UNDO (restart lineup selection)".to_string());
        let choice = Select::new("select libero:", libero_options).prompt()?;
        if choice == "UNDO (restart lineup selection)" {
            return prompt_set_lineup(m);
        }
        if let Some(id) = player_map.get(&choice) {
            break *id;
        } else {
            println!("invalid libero choice");
        }
    };
    // setter selection
    let setter_id = loop {
        let mut setter_options = selected_labels.clone();
        setter_options.insert(0, "UNDO (restart libero selection)".to_string());
        let choice = Select::new("select setter:", setter_options).prompt()?;
        if choice == "UNDO (restart libero selection)" {
            return prompt_set_lineup(m);
        }
        if let Some(id) = player_map.get(&choice) {
            break *id;
        } else {
            println!("invalid setter choice");
        }
    };
    Ok((positions, libero_id, setter_id))
}

fn prompt_set_details(
    m: &MatchEntry,
    set_number: u8,
    sets: &[SetEntry],
) -> Result<(TeamSideEnum, Vec<Uuid>, Uuid, Uuid), Box<dyn std::error::Error>> {
    let serving_team = if set_number == 1 || set_number == 5 {
        let choice = Select::new("who serves first?", vec!["us", "opponent"])
            .prompt()
            .unwrap_or("us");
        match choice {
            "us" => TeamSideEnum::Us,
            "opponent" => TeamSideEnum::Them,
            _ => return Err("invalid serving team choice".into()),
        }
    } else {
        let previous_set_number = set_number - 1;
        let prev_team = sets
            .iter()
            .find(|s| s.set_number == previous_set_number)
            .map(|s| s.serving_team);
        match prev_team {
            Some(TeamSideEnum::Us) => TeamSideEnum::Them,
            Some(TeamSideEnum::Them) => TeamSideEnum::Us,
            None => {
                return Err(format!(
                    "could not determine serving team for set {} (previous set missing or invalid)",
                    previous_set_number
                )
                .into());
            }
        }
    };
    let (positions_array, libero_id, setter_id) = prompt_set_lineup(m)?;
    let positions = positions_array.to_vec();
    Ok((serving_team, positions, libero_id, setter_id))
}

pub fn continue_match(m: MatchEntry) -> Result<SetEntry, Box<dyn std::error::Error>> {
    let sets: Vec<SetEntry> = get_sets(&m);
    // helper set creation
    let create_set_interactively = |set_number: u8| {
        let (serving_team, positions, libero, setter) = prompt_set_details(&m, set_number, &sets)?;
        let len = positions.len();
        let positions: [Uuid; 6] = match positions.try_into() {
            Ok(array) => array,
            Err(_) => {
                eprintln!("error: expected exactly 6 positions, got {}", len);
                return Err("invalid number of positions".into());
            }
        };
        create_set(&m, set_number, serving_team, positions, libero, setter)
    };
    for set_number in 1..=5 {
        if let Some(set_entry) = sets.iter().find(|s| s.set_number == set_number) {
            if let Some((snapshot, _)) = get_set_snapshot(&m, set_number) {
                if get_set_winner(&snapshot, set_number).is_none() {
                    return Ok(set_entry.clone()); // set is existing, but not finished yet
                } else {
                    continue; // set is over: jump to the next one
                }
            } else {
                // snapshot is missing: it's treated as a missing set
                return create_set_interactively(set_number);
            }
        } else {
            // set does not exist: create
            return create_set_interactively(set_number);
        }
    }

    Err("all 5 sets are completed: cannot continue".into())
}

pub fn show_matches(team: &TeamEntry) {
    let entries: Vec<MatchEntry> = get_matches(&team);
    if entries.is_empty() {
        println!("no matches found");
        return;
    }

    println!("matches:");
    for MatchEntry {
        date,
        opponent,
        team,
        home,
        ..
    } in entries
    {
        let formatted_date = date.format("%d %B %Y");
        if home {
            println!("{} vs {} ({})", team.name, opponent, formatted_date);
        } else {
            println!("{} vs {} ({})", opponent, team.name, formatted_date);
        }
    }
}

fn describe_event_type(e: &EventTypeEnum) -> &'static str {
    match e {
        EventTypeEnum::S => "serve",
        EventTypeEnum::P => "pass",
        EventTypeEnum::A => "attack",
        EventTypeEnum::D => "dig",
        EventTypeEnum::B => "block",
        EventTypeEnum::F => "fault",
        EventTypeEnum::OS => "opponent score",
        EventTypeEnum::OE => "opponent error",
    }
}

fn prompt_event_type(
    available_options: &[EventTypeEnum],
) -> Result<EventTypeEnum, Box<dyn std::error::Error>> {
    println!("\navailable event types:");
    for opt in available_options {
        println!("  {} → {}", opt.to_string(), describe_event_type(opt));
    }
    print!("\npress the key(s) for the event code: ");
    enable_raw_mode()?;
    stdout().flush()?;
    let valid_codes: HashSet<String> = available_options
        .iter()
        .map(|e| e.to_string().to_uppercase())
        .collect();
    let mut buffer = String::new();
    let result = loop {
        if let Event::Key(event) = read()? {
            match event.code {
                KeyCode::Char(c) => {
                    let uc = c.to_ascii_uppercase();
                    buffer.push(uc);
                    print!("{}", uc);
                    stdout().flush()?;
                    if valid_codes.contains(&buffer) {
                        println!();
                        let parsed = EventTypeEnum::from_str(&buffer)?;
                        break Ok(parsed);
                    }
                    if buffer.len() > 2 {
                        println!("\ninvalid code: {}", buffer);
                        buffer.clear();
                        print!("try again: ");
                        stdout().flush()?;
                    }
                }
                KeyCode::Backspace => {
                    if !buffer.is_empty() {
                        buffer.pop();
                        print!(
                            "\r{}{}",
                            " ".repeat(50),
                            "\rpress the key(s) for the event code: "
                        );
                        print!("{}", buffer);
                        stdout().flush()?;
                    }
                }
                KeyCode::Esc => {
                    break Err("cancelled input".into());
                }
                _ => {}
            }
        }
    };

    disable_raw_mode()?;
    result
}

fn prompt_player(set: &SetEntry, rotation: u8, event_type: EventTypeEnum) -> Option<Uuid> {
    let mut options = vec![
        (1, "setter", get_setter(set)),
        (2, "outside hitter 1", get_oh1(set)),
        (3, "middle blocker 2", get_mb2(set)),
        (4, "opposite", get_opposite(set)),
        (5, "outside hitter 2", get_oh2(set)),
        (6, "middle blocker 1", get_mb1(set)),
        (7, "libero", set.libero),
    ];
    if matches!(event_type, EventTypeEnum::A | EventTypeEnum::B) {
        options.retain(|(_, _, id)| *id != set.libero);
    }
    if event_type == EventTypeEnum::B {
        options.retain(|(_, _, id)| !is_back_row_player(set, rotation, *id));
    }
    println!("available players:");
    for (num, role, _) in &options {
        println!("  {} → {}", num, role);
    }
    print!("select a player by pressing a number key: ");
    if enable_raw_mode().is_err() {
        eprintln!("could not activate raw mode");
        return None;
    }
    io::stdout().flush().unwrap();

    let result = loop {
        if let Ok(Event::Key(event)) = read() {
            match event.code {
                KeyCode::Char(c) => {
                    if let Some(digit) = c.to_digit(10) {
                        let digit = digit as u8;
                        if let Some((_, _, id)) = options.iter().find(|(code, _, _)| *code == digit)
                        {
                            println!();
                            break Some(*id);
                        }
                    }

                    println!("\ninvalid selection: '{}'", c);
                    print!("try again: ");
                    io::stdout().flush().unwrap();
                }
                KeyCode::Esc => {
                    println!("\ncancelled");
                    break None;
                }
                _ => {}
            }
        }
    };

    if disable_raw_mode().is_err() {
        eprintln!("could not deactivate raw mode");
    }

    result
}

fn prompt_eval() -> Result<EvalEnum, Box<dyn std::error::Error>> {
    println!("\navailable evaluations:");
    for e in &[
        EvalEnum::Perfect,
        EvalEnum::Positive,
        EvalEnum::Exclamative,
        EvalEnum::Over,
        EvalEnum::Error,
        EvalEnum::Negative,
    ] {
        println!("  {} → {}", get_eval_symbol(e), describe_eval(e));
    }
    print!("\npress the evaluation symbol: ");
    enable_raw_mode()?;
    stdout().flush()?;
    let result = loop {
        if let Event::Key(event) = read()? {
            match event.code {
                KeyCode::Char(c) => {
                    let eval = match c {
                        '#' => EvalEnum::Perfect,
                        '+' => EvalEnum::Positive,
                        '!' => EvalEnum::Exclamative,
                        '/' => EvalEnum::Over,
                        '=' => EvalEnum::Error,
                        '-' => EvalEnum::Negative,
                        _ => {
                            println!("\ninvalid symbol: '{}'", c);
                            print!("try again: ");
                            stdout().flush()?;
                            continue;
                        }
                    };
                    println!();
                    break Ok(eval);
                }
                KeyCode::Esc => {
                    println!("\ncancelled.");
                    break Err("input cancelled".into());
                }
                _ => {}
            }
        }
    };
    disable_raw_mode()?;
    result
}

fn input_event(
    set: &SetEntry,
    snapshot: &mut Snapshot,
    available_options: Vec<EventTypeEnum>,
) -> Result<EventEntry, Box<dyn Error>> {
    let rotation = snapshot.rotation;

    // on serve, event type is "S"
    let event_type = if snapshot.new_serve {
        EventTypeEnum::S
    } else {
        prompt_event_type(&available_options)?
    };

    // determine the player
    let player_id: Option<Uuid> = if snapshot.new_serve {
        Some(get_serving_player(set, rotation))
    } else if matches!(event_type, EventTypeEnum::OE | EventTypeEnum::OS) {
        None
    } else {
        prompt_player(set, rotation, event_type.clone())
    };

    // evaluation prompt (not available for F, OR, OS or P)
    let eval = if event_type == EventTypeEnum::F
        || event_type == EventTypeEnum::OE
        || event_type == EventTypeEnum::OS
        || event_type == EventTypeEnum::F
    {
        None
    } else {
        Some(prompt_eval()?)
    };

    Ok(EventEntry {
        timestamp: Utc::now(),
        rotation,
        event_type,
        player: player_id,
        eval,
    })
}

fn describe_eval(e: &EvalEnum) -> &'static str {
    match e {
        EvalEnum::Perfect => "perfect (#)",
        EvalEnum::Positive => "positive (+)",
        EvalEnum::Exclamative => "exclamative (!)",
        EvalEnum::Over => "over (/)",
        EvalEnum::Error => "error (=)",
        EvalEnum::Negative => "negative (-)",
    }
}

fn get_eval_symbol(e: &EvalEnum) -> &'static str {
    match e {
        EvalEnum::Perfect => "#",
        EvalEnum::Positive => "+",
        EvalEnum::Exclamative => "!",
        EvalEnum::Over => "/",
        EvalEnum::Error => "=",
        EvalEnum::Negative => "-",
    }
}

pub fn print_court(set: &SetEntry, rotation: u8, players: &[PlayerEntry]) {
    let player_map: HashMap<Uuid, &PlayerEntry> = players.iter().map(|p| (p.id, p)).collect();
    let rotated_positions: Vec<_> = (0..6)
        .map(|i| {
            let rotated_index = (i + 6 - (rotation + 1) as usize) % 6;
            set.positions[rotated_index]
        })
        .collect();
    let get_name = |uuid: Uuid| {
        let base = player_map
            .get(&uuid)
            .map(|p| format!("{} {}", p.number, p.name))
            .unwrap_or_else(|| "?".to_string());

        if uuid == set.setter {
            format!("{} (S)", base)
        } else {
            base
        }
    };
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.load_preset(comfy_table::presets::UTF8_FULL);
    let top_row = Row::from(vec![
        Cell::new(get_name(rotated_positions[3])),
        Cell::new(get_name(rotated_positions[2])),
        Cell::new(get_name(rotated_positions[1])),
    ]);
    let bottom_row = Row::from(vec![
        Cell::new(get_name(rotated_positions[4])),
        Cell::new(get_name(rotated_positions[5])),
        Cell::new(get_name(rotated_positions[0])),
    ]);
    table.add_row(top_row);
    table.add_row(bottom_row);
    println!("\ncurrent court rotation (our side):");
    println!("{}", table);
}

fn print_last_events(team: &TeamEntry, recent_events: Vec<&EventEntry>) {
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![
        Cell::new("rotation"),
        Cell::new("event type"),
        Cell::new("player"),
        Cell::new("evaluation"),
    ]);
    for e in recent_events.iter().rev() {
        let player = match e.player {
            Some(pl) => team.players.iter().find(|p| p.id == pl),
            None => None,
        };
        let player_str = player.map_or("".to_string(), |u| u.name.clone());
        let eval_str = match e.eval.clone() {
            Some(ev) => match ev {
                EvalEnum::Error => "error",
                EvalEnum::Exclamative => "exclamative",
                EvalEnum::Negative => "negative",
                EvalEnum::Over => {
                    if e.event_type == EventTypeEnum::A {
                        "blocked (opponent scored)"
                    } else if e.event_type == EventTypeEnum::B {
                        "fault"
                    } else {
                        "slash"
                    }
                }
                EvalEnum::Perfect => {
                    if e.event_type == EventTypeEnum::S {
                        "ace"
                    } else if e.event_type == EventTypeEnum::A {
                        "score"
                    } else if e.event_type == EventTypeEnum::B {
                        "score"
                    } else {
                        "perfect"
                    }
                }
                EvalEnum::Positive => "positive",
            },
            None => "",
        };
        let event_type_str = match e.event_type.clone() {
            EventTypeEnum::A => "attack",
            EventTypeEnum::B => "block",
            EventTypeEnum::D => "dig",
            EventTypeEnum::F => "fault",
            EventTypeEnum::OE => "opponent error",
            EventTypeEnum::OS => "opponent scored",
            EventTypeEnum::S => "serve",
            EventTypeEnum::P => "pass",
        };
        let mut row: Row = Row::new();
        row.add_cell(Cell::new(format!("S{}", e.rotation + 1)));
        row.add_cell(Cell::new(event_type_str));
        row.add_cell(Cell::new(player_str));
        row.add_cell(Cell::new(eval_str));
        table.add_row(row);
    }
    println!("{}", table);
}

fn print_set_status(snapshot: &Snapshot) {
    let mut status_table = Table::new();
    status_table.set_content_arrangement(ContentArrangement::Dynamic);
    status_table.set_header(vec![
        Cell::new("score us"),
        Cell::new("score them"),
        Cell::new("current phase"),
        Cell::new("current rotation"),
    ]);
    let phase_str = if snapshot.phase == PhaseEnum::Break {
        "break"
    } else {
        "side-out"
    };
    let mut row: Row = Row::new();
    row.add_cell(Cell::new(snapshot.score_us));
    row.add_cell(Cell::new(snapshot.score_them));
    row.add_cell(Cell::new(phase_str));
    row.add_cell(Cell::new(format!("S{}", snapshot.rotation + 1)));
    status_table.add_row(row);
    println!("{}", status_table);
}

fn events_loop(team: &TeamEntry, match_id: Uuid, set: &mut SetEntry) -> Result<(), Box<dyn Error>> {
    let (mut snapshot, mut available_options) = compute_snapshot(set);

    loop {
        clear_screen();
        let recent_events: Vec<_> = set.events.iter().rev().take(5).collect();
        print_last_events(team, recent_events);
        print_set_status(&snapshot);
        print_court(&set, snapshot.rotation, &team.players);
        let event = input_event(set, &mut snapshot, available_options.clone())?;
        append_event(team, match_id, set.set_number, &event)?;
        set.events.push(event.clone());
        available_options = compute_event(&mut snapshot, &event);

        if let Some(winner) = get_set_winner(&snapshot, set.set_number) {
            println!("\nset finished");
            println!(
                "team {} won the set with score {}–{}",
                match winner {
                    TeamSideEnum::Us => &team.name,
                    TeamSideEnum::Them => "opponent",
                    _ => "unknown",
                },
                snapshot.score_us,
                snapshot.score_them
            );
            println!("\npress enter to continue...");
            let _ = std::io::stdin().read_line(&mut String::new());
            run_menu();
            break;
        }
    }

    Ok(())
}
