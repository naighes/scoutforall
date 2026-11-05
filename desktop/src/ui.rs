//! The pieces every screen is made of: the bars, the panels, the pill
//! buttons, the rows of a list, the keys shown next to what they do. The
//! amber marks what acts and what's chosen, as a line marks the court.

use {
    crate::theme::{
        self, BORDER, Colors, RADIUS_PANEL, RADIUS_PILL, SIZE_BODY, SIZE_SMALL, SIZE_TITLE,
    },
    iced::{
        Alignment, Background, Border, Color, Element, Length, Padding,
        widget::{button, column, container, responsive, row, space, svg, text, tooltip},
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

/// A panel that waits for a choice: lit in amber, its title too; the
/// others stay quiet.
pub fn lit_panel<'a, M: 'a>(
    title: Option<&str>,
    content: impl Into<Element<'a, M>>,
    lit: bool,
    c: Colors,
) -> Element<'a, M> {
    let body: Element<'a, M> = match title {
        Some(title) => column![
            row![
                mark(c, 18.0),
                text(title.to_string())
                    .size(SIZE_TITLE)
                    .font(theme::fonts().bold)
                    .color(match lit {
                        true => c.accent,
                        false => c.muted,
                    })
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
            border: match lit {
                true => border(c.accent, 2.0, RADIUS_PANEL),
                false => border(c.border, BORDER, RADIUS_PANEL),
            },
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

/// An icon of `assets/icons`, `size` wide, in `color`.
pub fn icon<'a>(bytes: &'static [u8], size: f32, color: Color) -> svg::Svg<'a> {
    svg(svg::Handle::from_memory(bytes))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .style(move |_, _| svg::Style { color: Some(color) })
}

/// What a button without words does, said under the mouse.
pub fn tip<'a, M: 'a>(content: impl Into<Element<'a, M>>, what: &str, c: Colors) -> Element<'a, M> {
    tooltip(
        content,
        container(text(what.to_string()).size(SIZE_SMALL).color(c.text))
            .padding(Padding::from([4, 10]))
            .style(move |_| container::Style {
                background: Some(c.panel.into()),
                border: border(c.border, BORDER, 8.0),
                ..container::Style::default()
            }),
        tooltip::Position::Bottom,
    )
    .gap(6)
    .into()
}

/// A round button on a bar, with an icon: an outline, amber under the
/// mouse; what it does said under the mouse.
pub fn bar_button<'a, M: Clone + 'a>(
    icon_bytes: &'static [u8],
    what: &str,
    message: M,
    c: Colors,
) -> Element<'a, M> {
    let button = button(
        container(icon(icon_bytes, 22.0, c.text))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(Length::Fixed(42.0))
    .height(Length::Fixed(42.0))
    .padding(0)
    .on_press(message)
    .style(move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: match hovered {
                true => Some(Color { a: 0.7, ..c.button }.into()),
                false => None,
            },
            text_color: c.text,
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
    });
    tip(button, what, c)
}

/// The round amber button that adds one more to a list (a player, a
/// match), on the title of its panel.
pub fn add_button<'a, M: Clone + 'a>(what: &str, message: M, c: Colors) -> Element<'a, M> {
    let button = button(
        container(icon(theme::icons::ADD, 22.0, c.on_accent))
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(Length::Fixed(34.0))
    .height(Length::Fixed(34.0))
    .padding(0)
    .on_press(message)
    .style(move |_, status| button::Style {
        background: Some(
            match status {
                button::Status::Hovered | button::Status::Pressed => Color {
                    a: 0.85,
                    ..c.accent
                },
                _ => c.accent,
            }
            .into(),
        ),
        text_color: c.on_accent,
        border: border(c.accent, BORDER, RADIUS_PILL),
        ..button::Style::default()
    });
    tip(button, what, c)
}

/// A panel with its title and, on its right, what it offers to do.
pub fn panel_with<'a, M: 'a>(
    title: &str,
    actions: Vec<Element<'a, M>>,
    content: impl Into<Element<'a, M>>,
    c: Colors,
) -> Element<'a, M> {
    let head = row![
        mark(c, 18.0),
        text(title.to_string())
            .size(SIZE_TITLE)
            .font(theme::fonts().bold)
            .color(c.text)
            .width(Length::Fill),
        row(actions).spacing(8).align_y(Alignment::Center),
    ]
    .spacing(10)
    .height(Length::Fixed(34.0))
    .align_y(Alignment::Center);
    panel(None, column![head, content.into()].spacing(12), c)
}

/// The score of a match, the sets of the team first: green when it won,
/// red when it lost, as on Android; in the color of the text while played.
pub fn score_color(won: u8, lost: u8, finished: bool, c: Colors) -> Color {
    match (finished, won > lost) {
        (false, _) => c.text,
        (true, true) => c.won,
        (true, false) => c.lost,
    }
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

/// The bar on top: back, the home and the logo on the left, the screen in
/// the middle, the buttons on the right; an amber line below.
pub fn top_bar<'a, M: Clone + 'a>(
    back: Option<M>,
    home: Option<M>,
    title: String,
    right: Vec<Element<'a, M>>,
    s: &crate::strings::Strings,
    c: Colors,
) -> Element<'a, M> {
    let back: Element<'a, M> = match back {
        Some(back) => bar_button(theme::icons::BACK, s.back, back, c),
        None => space::horizontal().width(0).into(),
    };
    let home: Element<'a, M> = match home {
        Some(home) => bar_button(theme::icons::HOME, s.home_page, home, c),
        None => space::horizontal().width(0).into(),
    };
    let logo = row![back, home, ball(26.0, c), wordmark(17.0, c)]
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

/// How wide a letter of the body is: the font is monospaced, its letters
/// six tenths of the size.
const BODY_LETTER: f32 = SIZE_BODY * 0.6;

/// A name in the room its row leaves it, on one line: whole if it fits,
/// else abbreviated as on the court ("Chr. Di Bal."), and cut if not even
/// that fits. Never under the numbers next to it, whatever the window.
pub fn name<'a, M: 'a>(name: String) -> Element<'a, M> {
    responsive(move |size| {
        let room = (size.width / BODY_LETTER).floor() as usize;
        container(body(scout4all_core::names::fitted(&name, room)).wrapping(text::Wrapping::None))
            .width(Length::Fill)
            .clip(true)
            .into()
    })
    .width(Length::Fill)
    .height(Length::Shrink)
    .into()
}

/// The word with its first letter in capitals, for a title.
pub fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// A question to confirm, in the middle: Enter does it, Esc doesn't.
pub fn confirm<'a, M: Clone + 'a>(
    question: &str,
    yes: &str,
    on_yes: M,
    no: &str,
    on_no: M,
    c: Colors,
) -> Element<'a, M> {
    let question = panel(
        None,
        column![
            body(question.to_string()),
            row![
                pill(&format!("Esc  {no}"), false, Some(on_no), c),
                pill(&format!("Enter  {yes}"), true, Some(on_yes), c),
            ]
            .spacing(10)
        ]
        .spacing(18)
        .align_x(Alignment::Center),
        c,
    );
    container(container(question).max_width(560))
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

/// The template with `{1}`, `{2}`… replaced by the values, in the order
/// the language puts them.
pub fn fill(template: &str, values: &[String]) -> String {
    values
        .iter()
        .enumerate()
        .fold(template.to_string(), |s, (i, v)| {
            s.replace(&format!("{{{}}}", i + 1), v)
        })
}

/// The option after (or before) `current`, round.
pub fn cycle<T: PartialEq + Clone>(options: &[T], current: &T, forward: bool) -> Option<T> {
    let count = options.len();
    let at = options.iter().position(|o| o == current);
    let next = match (at, forward) {
        (_, _) if count == 0 => return None,
        (Some(i), true) => (i + 1) % count,
        (Some(i), false) => (i + count - 1) % count,
        (None, _) => 0,
    };
    options.get(next).cloned()
}

/// A line of choices: what it is, then its pills; the one the arrows
/// are on marked in amber, as the filters of the statistics.
pub fn choice_line<'a, M: 'a>(
    name: &str,
    on: bool,
    pills: Vec<Element<'a, M>>,
    c: Colors,
) -> Element<'a, M> {
    let label = text(name.to_string())
        .size(SIZE_SMALL)
        .font(theme::fonts().bold)
        .color(match on {
            true => c.accent,
            false => c.muted,
        })
        .width(Length::Fixed(130.0));
    row![
        container(
            text(match on {
                true => "▸",
                false => " ",
            })
            .size(SIZE_SMALL)
            .color(c.accent)
            .width(Length::Fixed(12.0))
        )
        .padding(Padding::from([5, 0])),
        container(label).padding(Padding::from([5, 0])),
        iced::widget::Row::with_children(pills)
            .spacing(4)
            .wrap()
            .vertical_spacing(4)
    ]
    .spacing(6)
    // the name by the first line of pills, when they wrap
    .align_y(Alignment::Start)
    .into()
}
