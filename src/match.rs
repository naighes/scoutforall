use crate::errors::{AppError, IOError};
use crate::menu::run_menu;
use crate::ops::{
    append_event, compute_snapshot, create_match, create_set, get_match, get_match_status,
    get_matches, get_sets, CreateMatchError,
};
use crate::pdf::open_match_pdf;
use crate::set::prompt_set_details;
use crate::shapes::enums::{EvalEnum, EventTypeEnum, PhaseEnum, TeamSideEnum};
use crate::shapes::player::PlayerEntry;
use crate::shapes::r#match::MatchEntry;
use crate::shapes::set::SetEntry;
use crate::shapes::snapshot::{EventEntry, Snapshot};
use crate::shapes::team::TeamEntry;
use crate::structs::{ContinueMatchResult, MenuFlow};
use crate::substitution::prompt_substitution;
use crate::util::{clear_screen, prompt_date};
use chrono::Utc;
use comfy_table::{Cell, ContentArrangement, Row, Table};
use crossterm::event::{read, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use inquire::Text;
use inquire::{InquireError, Select};
use std::collections::{HashMap, HashSet};
use std::io;
use std::io::{stdout, Write};
use std::str::FromStr;
use uuid::Uuid;

pub fn prompt_match(team: &TeamEntry) -> Result<MenuFlow, Box<dyn std::error::Error>> {
    clear_screen();
    let date = match prompt_date("match date (YYYY-MM-DD):") {
        Ok(d) => d,
        Err(InquireError::OperationCanceled) => return Ok(MenuFlow::Back),
        Err(e) => return Err(Box::new(e)),
    };
    let opponent = loop {
        match Text::new("opponent name:").prompt() {
            Ok(s) => break s,
            Err(InquireError::OperationCanceled) => return Ok(MenuFlow::Back),
            Err(e) => {
                eprintln!("unexpected error: {}, please try again", e);
                continue;
            }
        }
    };
    let home = loop {
        match Select::new("where is the match played?", vec!["home", "away"]).prompt() {
            Ok(selection) => match selection.as_ref() {
                "home" => break true,
                "away" => break false,
                other => {
                    eprintln!(
                        "unexpected selection: {}, please type 'home' or 'away'",
                        other
                    );
                    continue;
                }
            },
            Err(InquireError::OperationCanceled) => return Ok(MenuFlow::Back),
            Err(e) => {
                eprintln!("unexpected error: {}, please try again", e);
                continue;
            }
        }
    };
    // try to create the match
    let m = match create_match(team, opponent.clone(), date, home) {
        Ok(m) => m,
        Err(CreateMatchError::MatchAlreadyExists(id)) => {
            // TODO: may be I need to check if the match is finished before asking for continuing
            // match already exists
            let resume = loop {
                match Text::new("match already exists: continue? (y/n)").prompt() {
                    Ok(input) => {
                        let input = input.trim().to_lowercase();
                        if input == "y" {
                            break true;
                        } else {
                            break false;
                        }
                    }
                    Err(InquireError::OperationCanceled) => break false,
                    Err(e) => {
                        eprintln!("unexpected error: {}, please try again", e);
                        continue;
                    }
                }
            };
            if resume {
                match get_match(team, &id)? {
                    Some(existing) => existing,
                    None => {
                        return Err(format!("match '{}' should exist but wasn't found", id).into())
                    }
                }
            } else {
                return Ok(MenuFlow::Back);
            }
        }
        Err(e) => return Err(Box::new(e)),
    };
    match continue_match(&m) {
        Ok(ContinueMatchResult::SetToPlay(mut set)) => {
            if let Err(e) = events_loop(team, &m.id, &mut set) {
                eprintln!("event loop error: {}", e);
                return Ok(MenuFlow::Back);
            }
            Ok(MenuFlow::Continue)
        }
        Ok(ContinueMatchResult::MatchFinished) => Ok(MenuFlow::Back),
        Err(_) => {
            open_match_pdf(&m);
            Ok(MenuFlow::Back)
        }
    }
}

pub fn continue_match(m: &MatchEntry) -> Result<ContinueMatchResult, MenuFlow> {
    let status = get_match_status(m).map_err(|_| MenuFlow::Back)?;
    // match finished
    if status.match_finished {
        return Ok(ContinueMatchResult::MatchFinished);
    }
    // set is ongoing
    if let Some(set_entry) = status.last_incomplete_set {
        return Ok(ContinueMatchResult::SetToPlay(set_entry));
    }
    // new set
    if let Some(next_set_number) = status.next_set_number {
        let sets = get_sets(m).map_err(|_| MenuFlow::Back)?;
        let (serving_team, positions_vec, libero, setter) =
            prompt_set_details(m, next_set_number, &sets)?;
        let positions: [Uuid; 6] = positions_vec.try_into().map_err(|_| MenuFlow::Back)?;
        return create_set(m, next_set_number, serving_team, positions, libero, setter)
            .map(ContinueMatchResult::SetToPlay)
            .map_err(|_| MenuFlow::Back);
    }
    // match finished
    Ok(ContinueMatchResult::MatchFinished)
}

pub fn show_matches(team: &TeamEntry) -> Result<MenuFlow, Box<dyn std::error::Error>> {
    clear_screen();
    loop {
        let entries = get_matches(team)
            .map_err(|e| format!("could not retrieve the list of matches: {}", e))?;
        if entries.is_empty() {
            println!("no matches available");
            return Ok(MenuFlow::Back);
        }
        let mut options = Vec::new();
        for entry in &entries {
            let status = get_match_status(entry)?;
            let match_in_progress = !status.match_finished && status.last_incomplete_set.is_some();
            let status_label = if status.match_finished {
                if status.us_wins == 3 {
                    "won"
                } else {
                    "lost"
                }
            } else if match_in_progress {
                "in progress"
            } else {
                "not started"
            };
            let score = if entry.home {
                format!("{}–{}", status.us_wins, status.them_wins)
            } else {
                format!("{}–{}", status.them_wins, status.us_wins)
            };
            let formatted_date = entry.date.format("%d %B %Y").to_string();
            let title = if entry.home {
                format!("{} vs {}", entry.team.name, entry.opponent)
            } else {
                format!("{} vs {}", entry.opponent, entry.team.name)
            };
            let label = format!(
                "{:<35} {:<18} {:<12} {}",
                title, score, status_label, formatted_date
            );
            options.push((label, Some(entry)));
        }
        options.push(("back".to_string(), None));
        let labels: Vec<String> = options.iter().map(|(label, _)| label.clone()).collect();
        let selected = Select::new("select a match:", labels).prompt();
        match selected {
            Ok(label) => match options.iter().find(|(l, _)| *l == label) {
                Some((_, Some(entry))) => {
                    clear_screen();
                    match continue_match(entry) {
                        Ok(ContinueMatchResult::SetToPlay(mut set)) => {
                            if let Err(e) = events_loop(team, &entry.id, &mut set) {
                                eprintln!("event loop error: {}", e);
                            }
                        }
                        Ok(ContinueMatchResult::MatchFinished) => {
                            open_match_pdf(&entry);
                            let _ = read();
                        }
                        Err(MenuFlow::Back) => {
                            clear_screen();
                            continue;
                        }
                        Err(MenuFlow::Continue) => {
                            continue;
                        }
                    }
                    clear_screen();
                }
                Some((_, None)) => {
                    clear_screen();
                    return Ok(MenuFlow::Back);
                }
                None => {
                    eprintln!("unexpected selection");
                    clear_screen();
                    return Ok(MenuFlow::Back);
                }
            },
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

fn prompt_event_type(available_options: &[EventTypeEnum]) -> Result<EventTypeEnum, AppError> {
    println!("\navailable event types:");
    for opt in available_options {
        println!("  {} → {}", opt, opt.friendly_name());
    }
    print!("\npress the key(s) for the event code: ");
    enable_raw_mode().map_err(|e| IOError::Error(e))?;
    stdout().flush().map_err(|e| IOError::Error(e))?;
    let valid_codes: HashSet<String> = available_options
        .iter()
        .map(|e| e.to_string().to_uppercase())
        .collect();
    let mut buffer = String::new();
    let result = loop {
        if let Event::Key(event) = read().map_err(|e| IOError::Error(e))? {
            match event.code {
                KeyCode::Char(c) => {
                    let uc = c.to_ascii_uppercase();
                    buffer.push(uc);
                    print!("{}", uc);
                    stdout().flush().map_err(|e| IOError::Error(e))?;
                    if valid_codes.contains(&buffer) {
                        println!();
                        let parsed = EventTypeEnum::from_str(&buffer)?;
                        break Ok(parsed);
                    }
                    if buffer.len() > 2 {
                        println!("\ninvalid code: {}", buffer);
                        buffer.clear();
                        print!("try again: ");
                        stdout().flush().map_err(|e| IOError::Error(e))?;
                    }
                }
                KeyCode::Backspace => {
                    if !buffer.is_empty() {
                        buffer.pop();
                        print!(
                            "\r{}\rpress the key(s) for the event code: ",
                            " ".repeat(50)
                        );
                        print!("{}", buffer);
                        stdout().flush().map_err(|e| IOError::Error(e))?;
                    }
                }
                KeyCode::Esc => {
                    // TODO
                    // break Err("canceled input".into());
                }
                _ => {}
            }
        }
    };
    disable_raw_mode().map_err(|e| IOError::Error(e))?;
    result
}

fn prompt_player(
    team: &TeamEntry,
    snapshot: &Snapshot,
    rotation: u8,
    event_type: EventTypeEnum,
    phase: PhaseEnum,
) -> Option<Uuid> {
    let mut options = vec![
        (1, "setter", snapshot.current_lineup.get_setter()),
        (2, "outside hitter 1", snapshot.current_lineup.get_oh1()),
        (3, "middle blocker 2", snapshot.current_lineup.get_mb2()),
        (4, "opposite", snapshot.current_lineup.get_opposite()),
        (5, "outside hitter 2", snapshot.current_lineup.get_oh2()),
        (6, "middle blocker 1", snapshot.current_lineup.get_mb1()),
    ];
    if event_type == EventTypeEnum::B {
        options.retain(|(_, _, id)| !snapshot.current_lineup.is_back_row_player(id));
    }
    let mb1 = snapshot.current_lineup.get_mb1();
    let mb2 = snapshot.current_lineup.get_mb2();
    let show_mb1 = matches!(rotation, 0 | 1 | 5);
    let show_mb2 = matches!(rotation, 2..=4);

    if phase == PhaseEnum::Break && matches!(rotation, 1 | 4) {
        // break phase with middle blocker serving
        // do nothing and leave the middle blocker in back-row
    } else {
        // replace middle blocker on back row by the libero
        options = options
            .into_iter()
            .map(|(num, role, id)| {
                if (id == mb1 && show_mb1) || (id == mb2 && show_mb2) {
                    (num, "libero", snapshot.current_lineup.get_current_libero())
                } else {
                    (num, role, id)
                }
            })
            .collect();
    }
    for (num, role, id) in &options {
        match team.players.iter().find(|x| x.id == *id) {
            Some(x) => {
                println!("  {}. {} ({}))", num, role, x.name);
            }
            None => {
                println!("  {}. unknown ({}))", num, role);
            }
        }
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
                    println!("\ncanceled");
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

fn prompt_eval() -> Result<EvalEnum, AppError> {
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
    enable_raw_mode().map_err(|e| AppError::IO(IOError::Error(e)))?;
    stdout()
        .flush()
        .map_err(|e| AppError::IO(IOError::Error(e)))?;
    let result = loop {
        if let Event::Key(event) = read().map_err(|e| AppError::IO(IOError::Error(e)))? {
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
                            stdout()
                                .flush()
                                .map_err(|e| AppError::IO(IOError::Error(e)))?;
                            continue;
                        }
                    };
                    println!();
                    break Ok(eval);
                }
                KeyCode::Esc => {
                    // TODO
                    // println!("\ncanceled.");
                    // break Err("input canceled".into());
                }
                _ => {}
            }
        }
    };
    disable_raw_mode().map_err(|e| AppError::IO(IOError::Error(e)))?;
    result
}

fn print_ongoing_action(event_type: EventTypeEnum, player_name: Option<&str>) {
    let mut status_table = Table::new();
    status_table.set_content_arrangement(ContentArrangement::Dynamic);
    let player_text = if event_type == EventTypeEnum::R {
        "replaced player"
    } else {
        "player"
    };
    status_table.set_header(vec![Cell::new("event type"), Cell::new(player_text)]);
    let mut row: Row = Row::new();
    row.add_cell(Cell::new(event_type.friendly_name()));
    row.add_cell(Cell::new(player_name.unwrap_or_default()));
    status_table.add_row(row);
    println!("{}", status_table);
}

fn print_prompt_event_header(
    set: &SetEntry,
    team: &TeamEntry,
    snapshot: &mut Snapshot,
    event_type: Option<EventTypeEnum>,
    player_name: Option<&str>,
) {
    clear_screen();
    // TODO: add const for number of recent events to show
    let recent_events: Vec<_> = set.events.iter().rev().take(5).collect();
    print_last_events(team, recent_events);
    print_set_status(set.set_number, snapshot);
    print_court(&snapshot, &team.players);
    if let Some(et) = event_type {
        print_ongoing_action(et, player_name);
    }
}

fn prompt_event(
    set: &SetEntry,
    team: &TeamEntry,
    snapshot: &mut Snapshot,
    available_options: Vec<EventTypeEnum>,
) -> Result<EventEntry, AppError> {
    print_prompt_event_header(set, team, snapshot, None, None);
    // on serve, event type is "S", "F", "OE" or "R"
    let event_type = if snapshot.get_serving_team() == Some(TeamSideEnum::Us) {
        prompt_event_type(&[
            EventTypeEnum::F,
            EventTypeEnum::S,
            EventTypeEnum::OE,
            EventTypeEnum::R,
        ])?
    } else {
        prompt_event_type(&available_options)?
    };
    print_prompt_event_header(set, team, snapshot, Some(event_type.clone()), None);
    if event_type == EventTypeEnum::R {
        let (replaced, replacement) =
            prompt_substitution(&snapshot, team).expect("TODO: do not panic here!");
        Ok(EventEntry {
            timestamp: Utc::now(),
            event_type,
            eval: None,
            player: Some(replaced),
            target_player: Some(replacement),
        })
    } else {
        // determine the player
        let player_id: Option<Uuid> = if snapshot.get_serving_team() == Some(TeamSideEnum::Us) {
            // on serving
            if event_type == EventTypeEnum::F || event_type == EventTypeEnum::OE {
                // on serve, it could happen a rotation fault (us or them)
                None
            } else {
                // otherwise, just assign the serving player
                Some(snapshot.current_lineup.get_serving_player())
            }
        } else if matches!(event_type, EventTypeEnum::OE | EventTypeEnum::OS) {
            None
        } else {
            prompt_player(
                team,
                &snapshot,
                snapshot.current_lineup.get_current_rotation(),
                event_type.clone(),
                snapshot.current_lineup.get_current_phase(),
            )
        };
        let player_name = match player_id {
            Some(pid) => {
                let p = team.players.iter().find(|p| p.id == pid);
                p.map(|p| p.name.as_str())
            }
            None => None,
        };
        print_prompt_event_header(set, team, snapshot, Some(event_type.clone()), player_name);
        // evaluation prompt (not available for F, OR, OS or P)
        let eval = if event_type == EventTypeEnum::F
            || event_type == EventTypeEnum::OE
            || event_type == EventTypeEnum::OS
        {
            None
        } else {
            Some(prompt_eval()?)
        };
        print_prompt_event_header(set, team, snapshot, Some(event_type.clone()), player_name);
        Ok(EventEntry {
            timestamp: Utc::now(),
            event_type,
            player: player_id,
            eval,
            target_player: None,
        })
    }
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

pub fn print_court(snapshot: &Snapshot, players: &[PlayerEntry]) {
    let player_map: HashMap<Uuid, &PlayerEntry> = players.iter().map(|p| (p.id, p)).collect();
    let get_name = |uuid: Uuid| {
        let base = player_map
            .get(&uuid)
            .map(|p| format!("{} {}", p.number, p.name))
            .unwrap_or_else(|| "?".to_string());
        if uuid == snapshot.current_lineup.get_setter() {
            format!("{} (S)", base)
        } else {
            base
        }
    };
    let mut table = Table::new();
    table.set_content_arrangement(ContentArrangement::Dynamic);
    table.load_preset(comfy_table::presets::UTF8_FULL);
    let top_row = Row::from(vec![
        Cell::new(get_name(snapshot.current_lineup.get(3))),
        Cell::new(get_name(snapshot.current_lineup.get(2))),
        Cell::new(get_name(snapshot.current_lineup.get(1))),
    ]);
    // TODO: centralize libero logic within snapshot computation
    let get_player_for_position_on_pos_0 = |id: Uuid| {
        if snapshot.current_lineup.get_current_phase() != PhaseEnum::Break {
            if snapshot.current_lineup.get_current_rotation() == 1
                || snapshot.current_lineup.get_current_rotation() == 4
            {
                // not serving: replace middle-blocker by libero
                snapshot.current_lineup.get_current_libero()
            } else {
                id
            }
        } else {
            // if middle-blocker is serving, then return the id of the middle-blocker
            id
        }
    };
    let get_player_for_position_on_pos_5 = |id: Uuid| {
        if snapshot.current_lineup.get_current_rotation() == 0
            || snapshot.current_lineup.get_current_rotation() == 3
        {
            // enforce libero here
            snapshot.current_lineup.get_current_libero()
        } else {
            id
        }
    };
    let get_player_for_position_on_pos_4 = |id: Uuid| {
        if snapshot.current_lineup.get_current_rotation() == 2
            || snapshot.current_lineup.get_current_rotation() == 5
        {
            // enforce libero here
            snapshot.current_lineup.get_current_libero()
        } else {
            id
        }
    };
    let bottom_row = Row::from(vec![
        Cell::new(get_name(get_player_for_position_on_pos_4(
            snapshot.current_lineup.get(4),
        ))),
        Cell::new(get_name(get_player_for_position_on_pos_5(
            snapshot.current_lineup.get(5),
        ))),
        Cell::new(get_name(get_player_for_position_on_pos_0(
            snapshot.current_lineup.get(0),
        ))),
    ]);
    table.add_row(top_row);
    table.add_row(bottom_row);
    println!("{}", table);
}

fn print_last_events(team: &TeamEntry, recent_events: Vec<&EventEntry>) {
    if recent_events.is_empty() {
        return;
    }
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
        let replacement = match e.target_player {
            Some(pl) => team.players.iter().find(|p| p.id == pl),
            None => None,
        };
        let player_str = match (player, replacement) {
            (Some(p), Some(r)) => format!("{}/{}", p.name, r.name),
            (Some(p), None) => p.name.clone(),
            _ => "-".to_string(),
        };
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
                    } else if e.event_type == EventTypeEnum::A || e.event_type == EventTypeEnum::B {
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
            EventTypeEnum::R => "substitution",
        };
        let mut row: Row = Row::new();
        row.add_cell(Cell::new(event_type_str));
        row.add_cell(Cell::new(player_str));
        row.add_cell(Cell::new(eval_str));
        table.add_row(row);
    }
    println!("{}", table);
}

fn print_set_status(set_number: u8, snapshot: &Snapshot) {
    let mut status_table = Table::new();
    status_table.set_content_arrangement(ContentArrangement::Dynamic);
    status_table.set_header(vec![
        Cell::new("set"),
        Cell::new("score us"),
        Cell::new("score them"),
        Cell::new("current phase"),
        Cell::new("current rotation"),
    ]);
    let phase_str = if snapshot.current_lineup.get_current_phase() == PhaseEnum::Break {
        "break"
    } else {
        "side-out"
    };
    let mut row: Row = Row::new();
    row.add_cell(Cell::new(set_number));
    row.add_cell(Cell::new(snapshot.score_us));
    row.add_cell(Cell::new(snapshot.score_them));
    row.add_cell(Cell::new(phase_str));
    row.add_cell(Cell::new(format!(
        "S{}",
        snapshot.current_lineup.get_current_rotation() + 1
    )));
    status_table.add_row(row);
    println!("{}", status_table);
}

pub fn events_loop(team: &TeamEntry, match_id: &str, set: &mut SetEntry) -> Result<(), AppError> {
    let (mut snapshot, mut available_options) = compute_snapshot(set)?;
    loop {
        clear_screen();
        let event = prompt_event(set, team, &mut snapshot, available_options.clone())?;
        append_event(team, match_id, set.set_number, &event)?;
        set.events.push(event.clone());
        available_options = snapshot.compute_event(&event, available_options.clone())?;
        if let Some(winner) = snapshot.get_set_winner(set.set_number) {
            println!("\nset finished");
            println!(
                "team {} won the set with score {}–{}",
                match winner {
                    TeamSideEnum::Us => &team.name,
                    TeamSideEnum::Them => "opponent",
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
