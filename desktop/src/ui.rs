//! The pieces every screen is made of: the bars, the panels, the pill
//! buttons, the rows of a list, the keys shown next to what they do. The
//! amber marks what acts and what's chosen, as a line marks the court.

use {
    crate::theme::{
        self, BORDER, Colors, RADIUS_PANEL, RADIUS_PILL, SIZE_BODY, SIZE_SMALL, SIZE_TITLE,
    },
    iced::{
        Alignment, Background, Border, Color, Element, Length, Padding,
        widget::{button, column, container, row, space, svg, text},
    },
};

fn border(color: Color, width: f32, radius: f32) -> Border {
    Border {
        color,
        width,
        radius: radius.into(),
    }
}

/// The amber mark before a title, a line of the court.
fn mark<'a, M: 'a>(c: Colors, height: f32) -> Element<'a, M> {
    container(space::horizontal().width(0))
        .width(Length::Fixed(4.0))
        .height(Length::Fixed(height))
        .style(move |_| container::Style {
            background: Some(c.accent.into()),
            border: border(Color::TRANSPARENT, 0.0, 2.0),
            ..container::Style::default()
        })
        .into()
}

/// A panel with its title on top, after the amber mark.
pub fn panel<'a, M: 'a>(
    title: Option<&str>,
    content: impl Into<Element<'a, M>>,
    c: Colors,
) -> Element<'a, M> {
    let body: Element<'a, M> = match title {
        Some(title) => column![
            row![
                mark(c, 18.0),
                text(title.to_string())
                    .size(SIZE_TITLE)
                    .font(theme::fonts().bold)
                    .color(c.text)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            content.into()
        ]
        .spacing(12)
        .into(),
        None => content.into(),
    };
    container(body)
        .padding(16)
        .style(move |_| container::Style {
            background: Some(c.panel.into()),
            border: border(c.border, BORDER, RADIUS_PANEL),
            ..container::Style::default()
        })
        .into()
}

/// A small title inside a panel, for a group of rows.
pub fn section<'a, M: 'a>(title: &str, c: Colors) -> Element<'a, M> {
    text(title.to_string())
        .size(SIZE_SMALL)
        .font(theme::fonts().bold)
        .color(c.muted)
        .into()
}

/// A pill button: chosen, it's filled with amber, as the site marks what's
/// selected.
pub fn pill<'a, M: Clone + 'a>(
    label: &str,
    chosen: bool,
    message: Option<M>,
    c: Colors,
) -> Element<'a, M> {
    button(
        text(label.to_string())
            .size(SIZE_SMALL)
            .align_x(Alignment::Center),
    )
    .padding(Padding::from([4, 14]))
    .on_press_maybe(message)
    .style(move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: match chosen {
                true => Some(Background::Color(c.accent)),
                false => None,
            },
            text_color: match chosen {
                true => c.on_accent,
                false => c.text,
            },
            border: border(
                match (chosen, hovered) {
                    (true, _) => c.accent,
                    (false, true) => Color { a: 0.7, ..c.accent },
                    (false, false) => c.border,
                },
                BORDER,
                RADIUS_PILL,
            ),
            ..button::Style::default()
        }
    })
    .into()
}

/// The key that does something, as a keycap.
pub fn keycap<'a, M: 'a>(key: &str, c: Colors) -> Element<'a, M> {
    container(
        text(key.to_string())
            .size(SIZE_SMALL)
            .font(theme::fonts().bold)
            .color(c.accent),
    )
    .padding(Padding::from([1, 7]))
    .style(move |_| container::Style {
        border: border(Color { a: 0.6, ..c.accent }, 1.0, 6.0),
        ..container::Style::default()
    })
    .into()
}

/// A row of a list: the key that chooses it on the left, if any; chosen,
/// it's lit.
pub fn choice<'a, M: Clone + 'a>(
    key: Option<String>,
    label: impl Into<Element<'a, M>>,
    chosen: bool,
    message: Option<M>,
    c: Colors,
) -> Element<'a, M> {
    let key: Element<'a, M> = match key {
        Some(k) => keycap(&k, c),
        None => space::horizontal().width(0).into(),
    };
    let body = row![
        container(key)
            .width(Length::Fixed(64.0))
            .align_x(Alignment::Start),
        label.into()
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    list_row(body, chosen, message, c)
}

/// A row of a list, the mouse lighting it.
pub fn list_row<'a, M: Clone + 'a>(
    content: impl Into<Element<'a, M>>,
    chosen: bool,
    message: Option<M>,
    c: Colors,
) -> Element<'a, M> {
    button(content)
        .width(Length::Fill)
        .padding(Padding::from([5, 10]))
        .on_press_maybe(message)
        .style(move |_, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            button::Style {
                background: match (chosen, hovered) {
                    (true, _) => Some(
                        Color {
                            a: 0.14,
                            ..c.accent
                        }
                        .into(),
                    ),
                    (false, true) => Some(Color { a: 0.7, ..c.button }.into()),
                    (false, false) => None,
                },
                text_color: c.text,
                border: border(
                    match chosen {
                        true => c.accent,
                        false => Color::TRANSPARENT,
                    },
                    1.0,
                    8.0,
                ),
                ..button::Style::default()
            }
        })
        .into()
}

/// A round button on a bar: an outline, amber under the mouse.
pub fn bar_button<'a, M: Clone + 'a>(label: &str, message: M, c: Colors) -> Element<'a, M> {
    button(
        text(label.to_string())
            .size(SIZE_TITLE)
            .align_x(Alignment::Center),
    )
    .padding(Padding::from([3, 14]))
    .on_press(message)
    .style(move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: None,
            text_color: match hovered {
                true => c.accent,
                false => c.text,
            },
            border: border(
                match hovered {
                    true => c.accent,
                    false => c.border,
                },
                BORDER,
                RADIUS_PILL,
            ),
            ..button::Style::default()
        }
    })
    .into()
}

/// The ball of the logo, in the color of the text.
pub fn ball<'a>(size: f32, c: Colors) -> svg::Svg<'a> {
    svg(svg::Handle::from_memory(theme::BALL))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style {
            color: Some(c.text),
        })
}

/// The name of the app as the site writes it, `height` tall.
pub fn wordmark<'a>(height: f32, c: Colors) -> svg::Svg<'a> {
    svg(svg::Handle::from_memory(theme::WORDMARK))
        .width(Length::Fixed(height * 9112.0 / 1512.0))
        .height(Length::Fixed(height))
        .style(move |_, _| svg::Style {
            color: Some(c.text),
        })
}

/// A line across, in amber or in the color of the borders.
fn line<'a, M: 'a>(color: Color, height: f32) -> Element<'a, M> {
    container(space::horizontal().width(0))
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .style(move |_| container::Style {
            background: Some(color.into()),
            ..container::Style::default()
        })
        .into()
}

fn bar<'a, M: 'a>(content: impl Into<Element<'a, M>>, height: f32, c: Colors) -> Element<'a, M> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .padding(Padding::from([0, 18]))
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(c.bar.into()),
            ..container::Style::default()
        })
        .into()
}

/// The bar on top: back and the logo on the left, the screen in the
/// middle, the buttons on the right; an amber line below.
pub fn top_bar<'a, M: Clone + 'a>(
    back: Option<M>,
    title: String,
    right: Vec<Element<'a, M>>,
    c: Colors,
) -> Element<'a, M> {
    let back: Element<'a, M> = match back {
        Some(back) => bar_button("←", back, c),
        None => space::horizontal().width(0).into(),
    };
    let logo = row![back, ball(26.0, c), wordmark(17.0, c)]
        .spacing(12)
        .align_y(Alignment::Center);
    let title = text(title)
        .size(SIZE_BODY)
        .font(theme::fonts().bold)
        .color(c.muted);
    column![
        bar(
            row![
                container(logo).width(Length::Fill),
                title,
                container(row(right).spacing(10))
                    .width(Length::Fill)
                    .align_x(Alignment::End),
            ]
            .align_y(Alignment::Center),
            58.0,
            c,
        ),
        line(c.accent, 2.0),
    ]
    .into()
}

/// The bar at the bottom: the version on the left, the keys of the screen
/// on the right.
pub fn bottom_bar<'a, M: 'a>(hints: Vec<(String, String)>, c: Colors) -> Element<'a, M> {
    let hints = row(hints.into_iter().map(|(key, what)| {
        row![keycap(&key, c), text(what).size(SIZE_SMALL).color(c.muted)]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
    }))
    .spacing(18);
    column![
        line(c.border, 1.0),
        bar(
            row![
                text(format!("v{}", scout4all_mobile::version()))
                    .size(SIZE_SMALL)
                    .color(c.muted),
                space::horizontal(),
                hints,
            ]
            .align_y(Alignment::Center),
            36.0,
            c,
        ),
    ]
    .into()
}

/// Plain text of the body.
pub fn body<'a>(s: impl ToString) -> iced::widget::Text<'a> {
    text(s.to_string()).size(SIZE_BODY)
}
