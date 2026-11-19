//! The statistics of the two sides of a match scouted on both sides side by
//! side, A | B: the same figures as each side has, no new calculation. In
//! the summary of a match and on the scouting screen (F3): fundamentals,
//! phases, errors, distribution and rotations.

use {
    crate::{
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL, TEAM_THEM, TEAM_US, grade_color},
        ui,
    },
    iced::{
        Alignment, Color, Element, Length,
        widget::{Column, column, container, row, space, text},
    },
    scout4all_mobile::{LiveStats, PhaseStats},
};

/// How wide the name of a row stands between the sides.
const LABEL_WIDTH: f32 = 96.0;

fn pct(v: Option<f64>) -> String {
    v.map_or_else(|| "–".to_string(), |v| format!("{v:.0}%"))
}

/// How wide the figure stands beside its bar.
const FIGURE_WIDTH: f32 = 40.0;

/// How thick a bar is.
const BAR_HEIGHT: f32 = 8.0;

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

/// A bar of `share` parts of `of`, growing from the name (`inward`) toward
/// the figure: the two sides read as one back-to-back chart.
fn bar<'a, M: 'a>(share: f64, of: f64, color: Color, leftward: bool, c: Colors) -> Element<'a, M> {
    let parts = |v: f64| (v.clamp(0.0, of) / of * 1000.0).round() as u16;
    let full = parts(share);
    let filled = container(space::horizontal())
        .width(Length::FillPortion(full.max(1)))
        .height(Length::Fixed(BAR_HEIGHT))
        .style(move |_| container::Style {
            background: Some(color.into()),
            border: iced::Border {
                radius: 2.0.into(),
                ..iced::Border::default()
            },
            ..container::Style::default()
        });
    let rest = space::horizontal().width(Length::FillPortion(1000 - full.min(999)));
    let track = container(match (full, leftward) {
        (0, _) => row![space::horizontal().width(Length::Fill)],
        (_, true) => row![rest, filled],
        (_, false) => row![filled, rest],
    })
    .width(Length::Fill)
    .height(Length::Fixed(BAR_HEIGHT))
    .style(move |_| container::Style {
        background: Some(c.border.into()),
        ..container::Style::default()
    });
    track.into()
}

/// One side of a bar row: the figure outside, the bar toward the name, the
/// counts under both.
fn cell<'a, M: 'a>(
    figure: Element<'a, M>,
    bar: Element<'a, M>,
    small: String,
    leftward: bool,
    c: Colors,
) -> Element<'a, M> {
    let fixed = |e| container(e).width(Length::Fixed(FIGURE_WIDTH));
    let figure = fixed(figure).align_x(if leftward {
        Alignment::Start
    } else {
        Alignment::End
    });
    let top = match leftward {
        true => row![figure, bar],
        false => row![bar, figure],
    }
    .spacing(6)
    .align_y(Alignment::Center);
    column![top, counts(small, c)]
        .align_x(if leftward {
            Alignment::Start
        } else {
            Alignment::End
        })
        .into()
}

/// One row: the side A (leftward), the name, the side B.
fn line<'a, M: 'a>(
    name: String,
    a: Element<'a, M>,
    b: Element<'a, M>,
    c: Colors,
) -> Element<'a, M> {
    row![
        container(a).width(Length::Fill),
        container(ui::body(name).color(c.muted))
            .width(Length::Fixed(LABEL_WIDTH))
            .align_x(Alignment::Center),
        container(b).width(Length::Fill),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

/// A row of two percentages, 0 to 100, as bars.
#[allow(clippy::too_many_arguments)]
fn percent_line<'a, M: 'a>(
    name: String,
    a: (Option<f64>, Option<f64>, String),
    b: (Option<f64>, Option<f64>, String),
    c: Colors,
) -> Element<'a, M> {
    let side = |(v, g, small): (Option<f64>, Option<f64>, String), color, leftward| {
        cell(
            graded(v, g, c),
            bar(v.unwrap_or(0.0), 100.0, color, leftward, c),
            small,
            leftward,
            c,
        )
    };
    line(name, side(a, TEAM_US, true), side(b, TEAM_THEM, false), c)
}

fn phase_line<'a, M: 'a>(
    name: String,
    a: &Option<PhaseStats>,
    b: &Option<PhaseStats>,
    c: Colors,
) -> Element<'a, M> {
    let of = |p: &Option<PhaseStats>| {
        (
            p.as_ref().map(|p| p.percentage),
            p.as_ref().and_then(|p| p.grade),
            p.as_ref()
                .map(|p| format!("{}/{}", p.won, p.rallies))
                .unwrap_or_default(),
        )
    };
    percent_line(name, of(a), of(b), c)
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
    let of = |f: &scout4all_mobile::FundamentalStats| {
        (
            f.efficiency,
            f.grade,
            format!("{}·#{}·={}", f.total, f.perfect, f.errors),
        )
    };
    let fundamentals = a
        .fundamentals
        .iter()
        .zip(&b.fundamentals)
        .map(|(fa, fb)| percent_line(ui::capitalized(&fa.name), of(fa), of(fb), c));
    let rows = [
        (s.unforced_errors, a.unforced_errors, b.unforced_errors),
        (s.forced_errors, a.forced_errors, b.forced_errors),
        (s.aces, a.aces, b.aces),
        (s.opponent_errors, a.opponent_errors, b.opponent_errors),
    ];
    // each row has its own scale: the longer side fills the bar
    let errors = rows.into_iter().map(|(name, x, y)| {
        let most = f64::from(x.max(y).max(1));
        let side = |n: u32, color, leftward| {
            cell(
                text(n.to_string())
                    .size(SIZE_BODY)
                    .font(theme::fonts().bold)
                    .into(),
                bar(f64::from(n), most, color, leftward, c),
                String::new(),
                leftward,
                c,
            )
        };
        line(
            name.to_string(),
            side(x, TEAM_US, true),
            side(y, TEAM_THEM, false),
            c,
        )
    });
    // the zones of the setter on the court, as the two sides have them
    let grid = |stats: &LiveStats| -> Element<'a, M> {
        let zone = |zone: u8| -> Element<'a, M> {
            let z = stats.distribution.iter().find(|z| z.zone == zone);
            column![
                counts(zone.to_string(), c),
                text(z.map_or_else(|| "–".to_string(), |z| format!("{:.0}%", z.share)))
                    .size(SIZE_BODY)
                    .font(theme::fonts().bold),
            ]
            .align_x(Alignment::Center)
            .width(Length::Fill)
            .into()
        };
        column![
            row![zone(4), zone(3), zone(2)],
            row![space::horizontal(), zone(8), zone(9)],
        ]
        .spacing(8)
        .into()
    };
    let zones = row![
        container(grid(a)).width(Length::Fill),
        space::horizontal().width(Length::Fixed(LABEL_WIDTH)),
        container(grid(b)).width(Length::Fill),
    ]
    .spacing(10);
    let rotations = a.rotations.iter().map(|ra| {
        phase_line(
            ra.name.clone(),
            &ra.total,
            &b.rotations
                .iter()
                .find(|rb| rb.rotation == ra.rotation)
                .and_then(|rb| rb.total.clone()),
            c,
        )
    });
    let content = column![
        head,
        Column::with_children(fundamentals).spacing(8),
        phase_line(s.side_out.to_string(), &a.side_out, &b.side_out, c),
        phase_line(
            s.break_point.to_string(),
            &a.break_points,
            &b.break_points,
            c
        ),
        ui::section(s.errors, c),
        Column::with_children(errors).spacing(8),
        ui::section(s.distribution, c),
        zones,
        ui::section(s.rotations, c),
        Column::with_children(rotations).spacing(8),
    ]
    .spacing(12);
    ui::panel(Some(s.compare), content, c)
}
