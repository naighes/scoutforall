//! The state of a set as the scouting shows it: the score, the sequence of
//! the points, the phase and rotation, the substitutions and the players on
//! court.

use crate::screens::components::panel;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use scout4all_core::{
    localization::current_labels,
    shapes::{
        enums::{EventTypeEnum, FriendlyName, PhaseEnum, TeamSideEnum},
        r#match::MatchEntry,
        player::PlayerEntry,
        snapshot::Snapshot,
        team::TeamEntry,
    },
};
use uuid::Uuid;

/// The teams with their sets won and the score of the set, the home team on
/// the left.
pub fn render_score_header(
    f: &mut Frame,
    area: Rect,
    current_match: &MatchEntry,
    set_number: u8,
    snapshot: &Snapshot,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!("set {}", set_number))
        .border_style(Style::default().fg(Color::Yellow))
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );
    let inner_area = block.inner(area);
    f.render_widget(block, area);
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(20),
            Constraint::Percentage(40),
        ])
        .split(inner_area);
    let (us_wins, them_wins) = current_match
        .get_status()
        .map(|s| (s.us_wins.to_string(), s.them_wins.to_string()))
        .unwrap_or(("-".into(), "-".into()));
    let (left_name, left_wins, right_name, right_wins, score) = if current_match.home {
        (
            &current_match.team.name,
            &us_wins,
            &current_match.opponent,
            &them_wins,
            format!("{} – {}", snapshot.score_us, snapshot.score_them),
        )
    } else {
        (
            &current_match.opponent,
            &them_wins,
            &current_match.team.name,
            &us_wins,
            format!("{} – {}", snapshot.score_them, snapshot.score_us),
        )
    };
    let left = Paragraph::new(format!("[{left_wins}] {left_name}"))
        .alignment(Alignment::Left)
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(left, chunks[0]);
    let middle = Paragraph::new(score)
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::White));
    f.render_widget(middle, chunks[1]);
    let right = Paragraph::new(format!("{right_name} [{right_wins}]"))
        .alignment(Alignment::Right)
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(right, chunks[2]);
}

/// The widest team name in the point sequence.
const POINT_SEQUENCE_MAX_NAME_WIDTH: usize = 16;
/// The width of a point in the point sequence: two digits and a space.
const POINT_WIDTH: usize = 3;

/// The latest points of the set, one column per point, as many as fit: a
/// row per team, the home team on top, with the score reached by the team
/// scoring the point in its row.
pub fn render_point_sequence(
    f: &mut Frame,
    area: Rect,
    current_match: &MatchEntry,
    snapshot: &Snapshot,
) {
    let block = panel::section(current_labels().point_sequence);
    let inner_area = block.inner(area);
    f.render_widget(block, area);
    let teams = match current_match.home {
        true => [
            (TeamSideEnum::Us, current_match.team.name.as_str()),
            (TeamSideEnum::Them, current_match.opponent.as_str()),
        ],
        false => [
            (TeamSideEnum::Them, current_match.opponent.as_str()),
            (TeamSideEnum::Us, current_match.team.name.as_str()),
        ],
    };
    let name_width = teams
        .iter()
        .map(|(_, name)| name.chars().count())
        .max()
        .unwrap_or(0)
        .min(POINT_SEQUENCE_MAX_NAME_WIDTH);
    let visible = (inner_area.width as usize).saturating_sub(name_width + 2) / POINT_WIDTH;
    let points = snapshot.point_sequence();
    let latest = &points[points.len().saturating_sub(visible)..];
    let lines: Vec<Line> = teams
        .iter()
        .map(|(team, name)| {
            let name: String = name.chars().take(name_width).collect();
            let label = Span::styled(
                format!(" {name:<name_width$} "),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );
            let cells = latest.iter().flat_map(|(side, score)| match side == team {
                true => [
                    Span::styled(format!("{score:>2}"), point_style(*side)),
                    Span::raw(" "),
                ],
                false => [Span::raw("  "), Span::raw(" ")],
            });
            Line::from(std::iter::once(label).chain(cells).collect::<Vec<_>>())
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner_area);
}

/// The color of the points of a team: ours as the team names, theirs in red.
fn point_style(side: TeamSideEnum) -> Style {
    match side {
        TeamSideEnum::Us => Style::default().bg(Color::Yellow).fg(Color::Black),
        TeamSideEnum::Them => Style::default().bg(Color::Red).fg(Color::White),
    }
    .add_modifier(Modifier::BOLD)
}

/// The substitutions made and allowed, as the set status shows them.
fn substitutions_text(snapshot: &Snapshot) -> String {
    format!(
        "{} {}/{}",
        current_labels().substitutions,
        snapshot.current_lineup.get_substitutions().len(),
        snapshot.current_lineup.get_max_substitutions()
    )
}

/// The width the set status takes to show its texts in full, the same
/// all along the set: the substitutions, any phase and any event (the
/// player is shortened when the screen is narrow).
pub fn set_status_width(snapshot: &Snapshot) -> u16 {
    let labels = current_labels();
    let phases = PhaseEnum::ALL.map(|phase| phase.to_string());
    let events = EventTypeEnum::ALL.map(|event| event.friendly_name(labels).to_string());
    std::iter::once(substitutions_text(snapshot))
        .chain(phases)
        .chain(events)
        .map(|text| Span::raw(text).width() as u16)
        .max()
        .unwrap_or(0)
}

/// The phase, the rotation and the substitutions, with the event and the
/// player being scouted, if any.
pub fn render_set_status(
    f: &mut Frame,
    area: Rect,
    snapshot: &Snapshot,
    event: Option<String>,
    player: Option<String>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(0),
        ])
        .split(area);
    let cells: Vec<(Rect, Option<String>, Color, Color)> = vec![
        (
            chunks[0],
            Some(snapshot.current_lineup.get_current_phase().to_string()),
            Color::Yellow,
            Color::Black,
        ),
        (
            chunks[1],
            snapshot
                .current_lineup
                .get_current_rotation()
                .ok()
                .map(|r| format!("{}{}", current_labels().setter_prefix, r + 1)),
            Color::Cyan,
            Color::Black,
        ),
        (chunks[2], event, Color::LightGreen, Color::Black),
        (chunks[3], player, Color::Magenta, Color::Black),
        (
            chunks[4],
            Some(substitutions_text(snapshot)),
            Color::Gray,
            Color::Black,
        ),
    ];
    for cell in cells {
        if let (area, Some(text), bg, fg) = cell {
            let paragraph = Paragraph::new(format!("\n{}\n", text))
                .style(Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center);
            f.render_widget(paragraph, area);
        }
    }
}

/// The players on court in their positions: the front row on top.
pub fn render_court(f: &mut Frame, area: Rect, team: &TeamEntry, snapshot: &Snapshot) {
    let court_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    let position_map: [usize; 6] = [3, 2, 1, 4, 5, 0];
    for (row_index, row_area) in court_rows.iter().enumerate() {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(34),
                Constraint::Percentage(33),
            ])
            .split(*row_area);
        for (column_index, cell_area) in columns.iter().enumerate() {
            let position_index = position_map[row_index * 3 + column_index];
            if let Some(player_id) = snapshot.current_lineup.get(position_index) {
                render_court_cell(f, cell_area, team, snapshot, player_id, position_index);
            }
        }
    }
}

fn format_court_cell(
    player: Option<&PlayerEntry>,
    position_index: usize,
    is_serving: bool,
    is_libero: bool,
    is_setter: bool,
) -> String {
    player
        .map(|player| {
            let arrow = (if is_serving {
                "\n\n\n\n\n\n .\n / \\\n /   \\\n/_   _\\\n | |"
            } else {
                ""
            })
            .to_string();
            if is_setter {
                format!(
                    "{}\n{}\n({}){}",
                    player.number,
                    current_labels().setter_prefix,
                    player.name,
                    arrow
                )
            } else if is_libero {
                format!("{}\n{}\n(L){}", player.number, player.name, arrow)
            } else {
                format!("{}\n{}{}", player.number, player.name, arrow)
            }
        })
        .unwrap_or_else(|| format!("pos {}", position_index + 1))
}

fn render_court_cell(
    f: &mut Frame,
    area: &Rect,
    team: &TeamEntry,
    snapshot: &Snapshot,
    player_id: Uuid,
    position_index: usize,
) {
    let is_setter = snapshot.current_lineup.has_setter_at_pos(position_index);
    let is_libero = snapshot.current_lineup.has_libero_at_pos(position_index);
    let is_serving =
        snapshot.get_available_options().contains(&EventTypeEnum::S) && position_index == 0;
    let player = team.find_player(player_id);
    let content = format_court_cell(player, position_index, is_serving, is_libero, is_setter);
    let cell = Paragraph::new(content)
        .alignment(ratatui::layout::Alignment::Center)
        .block(if is_setter {
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::LightBlue))
        } else if is_libero {
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().add_modifier(Modifier::REVERSED))
        } else {
            Block::default().borders(Borders::ALL)
        });
    f.render_widget(cell, *area);
}
