//! What the app tells the user, one look everywhere: a note under the
//! field to correct, a dialog for what went wrong (or well) doing
//! something, a banner that doesn't stop the scouting. Each has a level,
//! its color and its icon: the same as on Android (`Feedback.kt`).

use crate::{
    strings::Strings,
    theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
    ui,
};
use iced::{
    Alignment, Color, Element, Length, Padding,
    widget::{column, container, row, stack, text},
};
use scout4all_keys::{BACK_KEY, KeyCombination, key};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// it didn't go: to correct, or to try again
    Error,
    /// to know before going on
    Warning,
    /// only told: what went well
    Info,
}

impl Level {
    pub fn color(self, c: Colors) -> Color {
        match self {
            Level::Error => c.danger,
            Level::Warning => c.warning,
            Level::Info => c.info,
        }
    }

    pub fn icon(self) -> &'static [u8] {
        match self {
            Level::Error => theme::icons::ERROR_OUTLINE,
            Level::Warning => theme::icons::WARNING_OUTLINE,
            Level::Info => theme::icons::INFO,
        }
    }

    pub fn title(self, s: &Strings) -> &'static str {
        match self {
            Level::Error => s.notice_error,
            Level::Warning => s.notice_warning,
            Level::Info => s.notice_info,
        }
    }
}

/// Something to tell in a dialog: its level and the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub level: Level,
    pub text: String,
}

impl Notice {
    pub fn error(text: impl Into<String>) -> Self {
        Notice {
            level: Level::Error,
            text: text.into(),
        }
    }

    pub fn warning(text: impl Into<String>) -> Self {
        Notice {
            level: Level::Warning,
            text: text.into(),
        }
    }
}

/// The color of the level, light, behind its text: readable in both
/// themes, the text staying the theme's.
fn tint(level: Level, c: Colors) -> Color {
    Color {
        a: match c.dark {
            true => 0.16,
            false => 0.10,
        },
        ..level.color(c)
    }
}

/// The text with the icon of its level, on the level's tint.
fn callout<'a, M: 'a>(
    level: Level,
    what: &str,
    size: f32,
    c: Colors,
) -> container::Container<'a, M> {
    let color = level.color(c);
    container(
        row![
            ui::icon(level.icon(), size + 2.0, color),
            text(ui::capitalized(what))
                .size(size)
                .color(c.text)
                .shaping(text::Shaping::Advanced),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .style(move |_| container::Style {
        background: Some(tint(level, c).into()),
        border: iced::Border {
            color: Color { a: 0.45, ..color },
            width: 1.0,
            radius: 8.0.into(),
        },
        ..container::Style::default()
    })
}

/// Under a field: what to correct in it, small, as wide as it is.
pub fn field_note<'a, M: 'a>(what: &str, c: Colors) -> Element<'a, M> {
    callout(Level::Error, what, SIZE_SMALL, c)
        .padding(Padding::from([4, 10]))
        .into()
}

/// Over the top of a screen that must not stop (the scouting): what
/// happened, across it, until the next action.
pub fn banner<'a, M: 'a>(level: Level, what: &str, c: Colors) -> Element<'a, M> {
    callout(level, what, SIZE_BODY, c)
        .padding(Padding::from([8, 14]))
        .width(Length::Fill)
        .into()
}

/// The notice in the middle, over `under` dimmed: the icon of its level in
/// a disc of its tint, the title, the text, OK. Enter and Esc close it too
/// (the screen's keys).
pub fn dialog<'a, M: Clone + 'a>(
    under: Element<'a, M>,
    notice: &Notice,
    s: &Strings,
    on_close: M,
    c: Colors,
) -> Element<'a, M> {
    let level = notice.level;
    let badge = container(ui::icon(level.icon(), 26.0, level.color(c)))
        .width(44)
        .height(44)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_| container::Style {
            background: Some(tint(level, c).into()),
            border: iced::Border {
                radius: 22.0.into(),
                ..iced::Border::default()
            },
            ..container::Style::default()
        });
    let card = ui::panel(
        None,
        column![
            row![
                badge,
                text(level.title(s).to_string())
                    .size(20)
                    .font(theme::fonts().bold)
                    .color(c.text),
            ]
            .spacing(14)
            .align_y(Alignment::Center),
            ui::body(ui::capitalized(&notice.text)).shaping(text::Shaping::Advanced),
            ui::answers(None, theme::icons::CHECK, s.ok, Some(on_close), c),
        ]
        .spacing(18),
        c,
    );
    let scrim = container(iced::widget::space::vertical())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(
                Color {
                    a: match c.dark {
                        true => 0.6,
                        false => 0.35,
                    },
                    ..Color::BLACK
                }
                .into(),
            ),
            ..container::Style::default()
        });
    // the scrim takes the clicks: the screen under it doesn't move
    let scrim = iced::widget::mouse_area(scrim).interaction(iced::mouse::Interaction::Idle);
    stack![
        under,
        iced::widget::opaque(scrim),
        container(container(card).max_width(480))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(24)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
    ]
    .into()
}

/// The keys of a dialog open: Enter and Esc close it, the others do
/// nothing (the screen under it waits).
pub fn closes(ck: KeyCombination) -> bool {
    ck == key!(enter) || ck == BACK_KEY
}

/// The keys of a dialog open, for the bar under the page.
pub fn hints(s: &Strings) -> Vec<(String, String)> {
    vec![("Enter Esc".to_string(), s.ok.to_string())]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_level_has_its_color_in_both_themes() {
        for c in [theme::colors(true), theme::colors(false)] {
            let colors = [Level::Error, Level::Warning, Level::Info].map(|l| l.color(c));
            assert_ne!(colors[0], colors[1]);
            assert_ne!(colors[1], colors[2]);
            assert_ne!(colors[0], colors[2]);
            // none of them the accent: the actions are amber
            assert!(colors.iter().all(|color| *color != c.accent));
        }
    }

    #[test]
    fn a_dialog_closes_with_enter_or_esc_only() {
        assert!(closes(key!(enter)));
        assert!(closes(key!(esc)));
        assert!(!closes(key!(a)));
        assert!(!closes(key!(backspace)));
    }
}
