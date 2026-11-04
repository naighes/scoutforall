//! The home: the matches in progress, the recent ones and the teams side by
//! side. The arrows move among the matches in progress, Enter resumes one.

use {
    crate::{strings::Strings, theme::Colors},
    iced::{
        Alignment, Element, Length, Padding,
        widget::{Column, button, column, container, row, scrollable, space, text},
    },
    scout4all_mobile::{Home, HomeMatch, NextSetKind, TeamListing},
};

#[derive(Debug, Clone)]
pub struct Open {
    pub team_id: String,
    pub match_id: String,
}

/// The match can be scouted here: a set in progress. Starting a set is not
/// in the prototype.
pub fn resumable(m: &HomeMatch) -> Option<Open> {
    match m.next {
        NextSetKind::Resume { .. } => Some(Open {
            team_id: m.team_id.clone(),
            match_id: m.id.clone(),
        }),
        NextSetKind::Start { .. } | NextSetKind::Over => None,
    }
}

fn date(m: &HomeMatch) -> String {
    m.date.get(..10).unwrap_or(&m.date).to_string()
}

fn column_of<'a, M: 'a>(
    heading: &str,
    items: Vec<Element<'a, M>>,
    empty: &str,
    c: Colors,
) -> Element<'a, M> {
    let body: Element<'a, M> = match items.is_empty() {
        true => text(empty.to_string()).color(c.muted).into(),
        false => Column::with_children(items).spacing(4).into(),
    };
    container(
        column![
            text(heading.to_uppercase()).size(13).color(c.muted),
            scrollable(body).height(Length::Fill)
        ]
        .spacing(10),
    )
    .padding(14)
    .width(Length::FillPortion(1))
    .height(Length::Fill)
    .style(move |_| container::Style {
        background: Some(c.surface.into()),
        border: iced::Border {
            radius: 8.0.into(),
            width: 1.0,
            color: c.raised,
        },
        ..container::Style::default()
    })
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
        NextSetKind::Start { set_number } => {
            format!("{} {set_number}: {}", s.set_n, s.start_elsewhere)
        }
        NextSetKind::Over => s.match_over.to_string(),
    };
    let body = row![
        column![
            text(format!("{} – {}", m.team_name, m.opponent)).size(15),
            text(format!("{} · {next}", date(m)))
                .size(12)
                .color(c.muted),
        ]
        .width(Length::Fill),
        text(format!("{}–{}", m.won, m.lost)).size(20),
    ]
    .align_y(Alignment::Center)
    .spacing(8);
    button(body)
        .width(Length::Fill)
        .padding(Padding::from([6, 10]))
        .on_press_maybe(resumable(m).map(on_open))
        .style(move |_, status| button::Style {
            background: match (selected, status) {
                (true, _) => Some(
                    iced::Color {
                        a: 0.25,
                        ..c.accent
                    }
                    .into(),
                ),
                (false, button::Status::Hovered) => Some(c.raised.into()),
                _ => None,
            },
            text_color: c.text,
            border: iced::Border {
                radius: 6.0.into(),
                ..iced::Border::default()
            },
            ..button::Style::default()
        })
        .into()
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
            } => row![
                text(match pinned {
                    true => "★",
                    false => " ",
                })
                .color(c.accent)
                .width(Length::Fixed(18.0)),
                text(name.clone()).size(15).width(Length::Fill),
                text(format!("{year} · {players} {}", s.players))
                    .size(12)
                    .color(c.muted),
            ]
            .align_y(Alignment::Center)
            .padding(Padding::from([6, 4]))
            .into(),
            TeamListing::Unreadable { folder } => text(folder.clone()).color(c.muted).into(),
        })
        .collect();
    let header = row![
        text("scout4all").size(24).color(c.accent),
        space::horizontal(),
        text("↑ ↓  Enter").size(12).color(c.muted),
    ]
    .align_y(Alignment::Center);
    column![
        header,
        row![
            column_of(s.in_progress, in_progress, s.no_matches, c),
            column_of(s.recent, recent, s.no_matches, c),
            column_of(s.teams, teams, "", c),
        ]
        .spacing(12)
        .height(Length::Fill)
    ]
    .spacing(12)
    .padding(16)
    .into()
}
