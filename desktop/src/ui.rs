//! The pieces every screen is made of: the bars, the panels, the pill
//! buttons, the rows of a list, the keys shown next to what they do.

use {
    crate::theme::{
        self, BAR, BORDER, Colors, RADIUS_PANEL, RADIUS_PILL, SIZE_BODY, SIZE_SMALL, SIZE_TITLE,
    },
    iced::{
        Alignment, Background, Border, Color, Element, Length, Padding, Shadow, Vector,
        widget::{button, column, container, row, space, text},
    },
};

fn border(color: Color, width: f32, radius: f32) -> Border {
    Border {
        color,
        width,
        radius: radius.into(),
    }
}

/// A round panel with its title centered on top, in the color of the
/// actions.
pub fn panel<'a, M: 'a>(
    title: Option<&str>,
    content: impl Into<Element<'a, M>>,
    c: Colors,
) -> Element<'a, M> {
    let body: Element<'a, M> = match title {
        Some(title) => column![
            container(
                text(title.to_string())
                    .size(SIZE_TITLE)
                    .font(theme::fonts().bold)
                    .color(c.accent)
            )
            .width(Length::Fill)
            .align_x(Alignment::Center),
            content.into()
        ]
        .spacing(10)
        .into(),
        None => content.into(),
    };
    container(body)
        .padding(14)
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

/// A pill button: chosen, it has the border of the actions.
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
            background: Some(Background::Color(match (chosen, hovered) {
                (true, _) => theme::mix(c.button, c.accent, 0.25),
                (false, true) => theme::mix(c.button, c.accent, 0.12),
                (false, false) => c.button,
            })),
            text_color: c.text,
            border: border(
                match chosen {
                    true => c.accent,
                    false => Color::TRANSPARENT,
                },
                BORDER,
                RADIUS_PILL,
            ),
            shadow: match hovered {
                true => Shadow {
                    color: Color {
                        a: 0.35,
                        ..Color::BLACK
                    },
                    offset: Vector::new(0.0, 2.0),
                    blur_radius: 2.0,
                },
                false => Shadow::default(),
            },
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
                            a: 0.22,
                            ..c.accent
                        }
                        .into(),
                    ),
                    (false, true) => Some(Color { a: 0.6, ..c.button }.into()),
                    (false, false) => None,
                },
                text_color: c.text,
                border: border(
                    match chosen {
                        true => Color { a: 0.7, ..c.accent },
                        false => Color::TRANSPARENT,
                    },
                    1.0,
                    10.0,
                ),
                ..button::Style::default()
            }
        })
        .into()
}

/// A round button on a bar, dark on the amber.
pub fn bar_button<'a, M: Clone + 'a>(label: &str, message: M, c: Colors) -> Element<'a, M> {
    button(
        text(label.to_string())
            .size(SIZE_TITLE)
            .color(theme::DARK.text)
            .align_x(Alignment::Center),
    )
    .padding(Padding::from([4, 18]))
    .on_press(message)
    .style(move |_, status| button::Style {
        background: Some(
            match status {
                button::Status::Hovered | button::Status::Pressed => {
                    theme::mix(c.on_accent, theme::DARK.button, 0.5)
                }
                _ => c.on_accent,
            }
            .into(),
        ),
        text_color: theme::DARK.text,
        border: border(Color::TRANSPARENT, 0.0, RADIUS_PILL),
        ..button::Style::default()
    })
    .into()
}

fn bar<'a, M: 'a>(content: impl Into<Element<'a, M>>, height: f32) -> Element<'a, M> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .padding(Padding::from([0, 18]))
        .align_y(Alignment::Center)
        .style(|_| container::Style {
            background: Some(BAR.into()),
            ..container::Style::default()
        })
        .into()
}

/// The bar on top: back on the left, the name in the middle, the buttons
/// on the right.
pub fn top_bar<'a, M: Clone + 'a>(
    back: Option<M>,
    right: Vec<Element<'a, M>>,
    c: Colors,
) -> Element<'a, M> {
    let left: Element<'a, M> = match back {
        Some(back) => bar_button("←", back, c),
        None => space::horizontal().width(0).into(),
    };
    let name = text("scout4all")
        .size(26)
        .font(theme::fonts().bold)
        .color(c.on_accent);
    bar(
        row![
            container(left).width(Length::Fill),
            name,
            container(row(right).spacing(10))
                .width(Length::Fill)
                .align_x(Alignment::End),
        ]
        .align_y(Alignment::Center),
        60.0,
    )
}

/// The bar at the bottom: the version on the left, the keys of the screen
/// on the right.
pub fn bottom_bar<'a, M: 'a>(hints: Vec<(String, String)>, c: Colors) -> Element<'a, M> {
    let hints = row(hints.into_iter().map(|(key, what)| {
        row![
            text(key)
                .size(SIZE_SMALL)
                .font(theme::fonts().bold)
                .color(c.on_accent),
            text(what).size(SIZE_SMALL).color(c.on_accent),
        ]
        .spacing(6)
        .into()
    }))
    .spacing(20);
    bar(
        row![
            text(format!("scout4all {}", scout4all_mobile::version()))
                .size(SIZE_SMALL)
                .color(c.on_accent),
            space::horizontal(),
            hints,
        ]
        .align_y(Alignment::Center),
        34.0,
    )
}

/// Plain text of the body.
pub fn body<'a>(s: impl ToString) -> iced::widget::Text<'a> {
    text(s.to_string()).size(SIZE_BODY)
}
