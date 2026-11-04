//! The home: the matches in progress, the recent ones and the teams side by
//! side. The arrows move among the matches in progress, Enter resumes one.

use {
    crate::{
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, column, row, scrollable, text},
    },
    scout4all_mobile::{Home, HomeMatch, NextSetKind, TeamListing},
};

#[derive(Debug, Clone)]
pub struct Open {
    pub team_id: String,
    pub match_id: String,
    /// a set to start, not one in progress
    pub start: bool,
}

/// What the match goes on with: a set to start or to resume; nothing once
/// it's over.
pub fn openable(m: &HomeMatch) -> Option<Open> {
    let open = |start| {
        Some(Open {
            team_id: m.team_id.clone(),
            match_id: m.id.clone(),
            start,
        })
    };
    match m.next {
        NextSetKind::Resume { .. } => open(false),
        NextSetKind::Start { .. } => open(true),
        NextSetKind::Over => None,
    }
}

fn date(m: &HomeMatch) -> String {
    m.date.get(..10).unwrap_or(&m.date).to_string()
}

pub fn hints(s: &Strings) -> Vec<(String, String)> {
    vec![
        ("↑ ↓".to_string(), String::new()),
        ("Enter".to_string(), format!("{} / {}", s.resume, s.start)),
        ("F2".to_string(), s.settings.to_string()),
    ]
}

fn list<'a, M: 'a>(
    title: &str,
    items: Vec<Element<'a, M>>,
    empty: &str,
    c: Colors,
) -> Element<'a, M> {
    let body: Element<'a, M> = match items.is_empty() {
        true => text(empty.to_string())
            .size(SIZE_BODY)
            .color(c.muted)
            .into(),
        false => Column::with_children(items).spacing(4).into(),
    };
    iced::widget::container(ui::panel(
        Some(title),
        scrollable(body).height(Length::Fill),
        c,
    ))
    .width(Length::FillPortion(1))
    .height(Length::Fill)
    .into()
}

fn match_row<'a, M: Clone + 'a>(
    m: &HomeMatch,
    s: &'static Strings,
    selected: bool,
    on_open: impl Fn(Open) -> M,
    c: Colors,
) -> Element<'a, M> {
    let next = match m.next {
        NextSetKind::Resume { set_number } => format!("{} · {} {set_number}", s.resume, s.set_n),
        NextSetKind::Start { set_number } => format!("{} · {} {set_number}", s.start, s.set_n),
        NextSetKind::Over => s.match_over.to_string(),
    };
    let bold = theme::fonts().bold;
    let body = row![
        column![
            row![
                text(m.team_name.clone()).size(SIZE_BODY).font(bold),
                text(format!("– {}", m.opponent))
                    .size(SIZE_BODY)
                    .color(c.muted),
            ]
            .spacing(6)
            .wrap(),
            text(format!("{} · {next}", date(m)))
                .size(SIZE_SMALL)
                .color(c.muted),
        ]
        .spacing(2)
        .width(Length::Fill),
        text(format!("{}–{}", m.won, m.lost))
            .size(22)
            .font(bold)
            .color(c.accent),
    ]
    .align_y(Alignment::Center)
    .spacing(10);
    ui::list_row(body, selected, openable(m).map(on_open), c)
}

pub fn view<'a, M: Clone + 'a>(
    home: &Home,
    selected: usize,
    s: &'static Strings,
    on_open: impl Fn(Open) -> M + Copy,
    c: Colors,
) -> Element<'a, M> {
    let in_progress = home
        .in_progress
        .iter()
        .enumerate()
        .map(|(i, m)| match_row(m, s, i == selected, on_open, c))
        .collect();
    let recent = home
        .recent
        .iter()
        .map(|m| match_row(m, s, false, on_open, c))
        .collect();
    let teams = home
        .teams
        .iter()
        .map(|t| match t {
            TeamListing::Team {
                name,
                year,
                players,
                pinned,
                ..
            } => ui::list_row(
                row![
                    text(match pinned {
                        true => "★",
                        false => "☆",
                    })
                    .size(SIZE_BODY)
                    .color(match pinned {
                        true => c.accent,
                        false => c.muted,
                    })
                    .width(Length::Fixed(22.0)),
                    text(name.clone())
                        .size(SIZE_BODY)
                        .font(theme::fonts().bold)
                        .width(Length::Fill),
                    text(format!("{year} · {players} {}", s.players))
                        .size(SIZE_SMALL)
                        .color(c.muted),
                ]
                .align_y(Alignment::Center),
                false,
                None,
                c,
            ),
            TeamListing::Unreadable { folder } => {
                text(folder.clone()).size(SIZE_SMALL).color(c.muted).into()
            }
        })
        .collect();
    row![
        list(s.in_progress, in_progress, s.no_matches, c),
        list(s.recent, recent, s.no_matches, c),
        list(s.teams, teams, "", c),
    ]
    .spacing(14)
    .height(Length::Fill)
    .into()
}
