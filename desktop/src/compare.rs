//! The statistics of the two sides of a match scouted on both sides side by
//! side, A | B: the same figures as each side has, no new calculation. In
//! the summary of a match and on the scouting screen (F3).

use {
    crate::{
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL, TEAM_THEM, TEAM_US, grade_color},
        ui,
    },
    iced::{
        Alignment, Element, Length,
        widget::{Column, column, container, row, space, text},
    },
    scout4all_mobile::{LiveStats, PhaseStats},
};

/// How wide the name of a row stands between the sides.
const LABEL_WIDTH: f32 = 96.0;

fn pct(v: Option<f64>) -> String {
    v.map_or_else(|| "–".to_string(), |v| format!("{v:.0}%"))
}

/// A figure, graded when the team has a level.
fn graded<'a, M: 'a>(v: Option<f64>, g: Option<f64>, c: Colors) -> Element<'a, M> {
    text(pct(v))
        .size(SIZE_BODY)
        .font(theme::fonts().bold)
        .color(g.map_or(c.text, |g| grade_color(g, c)))
        .into()
}

fn counts<'a, M: 'a>(label: String, c: Colors) -> Element<'a, M> {
    text(label).size(SIZE_SMALL).color(c.muted).into()
}

/// One row: the figure of A, the name, the figure of B.
fn line<'a, M: 'a>(
    name: String,
    a: Element<'a, M>,
    b: Element<'a, M>,
    c: Colors,
) -> Element<'a, M> {
    row![
        container(a).width(Length::Fill).align_x(Alignment::Center),
        container(ui::body(name).color(c.muted))
            .width(Length::Fixed(LABEL_WIDTH))
            .align_x(Alignment::Center),
        container(b).width(Length::Fill).align_x(Alignment::Center),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

fn phase<'a, M: 'a>(p: &Option<PhaseStats>, c: Colors) -> Element<'a, M> {
    column![
        graded(
            p.as_ref().map(|p| p.percentage),
            p.as_ref().and_then(|p| p.grade),
            c
        ),
        counts(
            p.as_ref()
                .map(|p| format!("{}/{}", p.won, p.rallies))
                .unwrap_or_default(),
            c
        ),
    ]
    .align_x(Alignment::Center)
    .into()
}

/// The fundamentals, the side out and the break point of the two sides,
/// each name over its column.
pub fn comparison<'a, M: 'a>(
    a: &LiveStats,
    b: &LiveStats,
    s: &'static Strings,
    c: Colors,
) -> Element<'a, M> {
    let title = |name: String, color| {
        text(name)
            .size(SIZE_BODY)
            .font(theme::fonts().bold)
            .color(color)
    };
    let head = row![
        container(title(a.team_name.clone(), TEAM_US))
            .width(Length::Fill)
            .align_x(Alignment::Center),
        space::horizontal().width(Length::Fixed(LABEL_WIDTH)),
        container(title(b.team_name.clone(), TEAM_THEM))
            .width(Length::Fill)
            .align_x(Alignment::Center),
    ]
    .spacing(10);
    let fundamental = |f: &scout4all_mobile::FundamentalStats| {
        column![
            graded(f.efficiency, f.grade, c),
            counts(format!("{} · #{} · ={}", f.total, f.perfect, f.errors), c),
        ]
        .align_x(Alignment::Center)
        .into()
    };
    let fundamentals = a
        .fundamentals
        .iter()
        .zip(&b.fundamentals)
        .map(|(fa, fb)| {
            line(
                ui::capitalized(&fa.name),
                fundamental(fa),
                fundamental(fb),
                c,
            )
        });
    let content = column![
        head,
        Column::with_children(fundamentals).spacing(8),
        line(
            s.side_out.to_string(),
            phase(&a.side_out, c),
            phase(&b.side_out, c),
            c
        ),
        line(
            s.break_point.to_string(),
            phase(&a.break_points, c),
            phase(&b.break_points, c),
            c
        ),
    ]
    .spacing(12);
    ui::panel(Some(s.compare), content, c)
}
