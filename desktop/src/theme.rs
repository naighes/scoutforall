//! The colors of the Android app (its `Theme.kt`), dark as there, and a
//! light version of them for the desktop.

use iced::{Color, Theme, theme::Palette};

const fn rgb(hex: u32) -> Color {
    Color::from_rgb8(
        ((hex >> 16) & 0xFF) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}

/// The actions and what's chosen.
pub const AMBER: Color = rgb(0xFCD34D);
pub const WON: Color = rgb(0x4ADE80);
pub const LOST: Color = rgb(0xF87171);
pub const FAIR: Color = rgb(0xFACC15);

/// A color for each set in the trend: none of them amber (the actions),
/// red or green (the grades), as in the app.
const SET_COLORS: [Color; 5] = [
    rgb(0x60A5FA),
    rgb(0xF472B6),
    rgb(0xA78BFA),
    rgb(0x2DD4BF),
    rgb(0xFB923C),
];

pub fn set_color(set_number: u8) -> Color {
    let i = usize::from(set_number.saturating_sub(1)).min(SET_COLORS.len().saturating_sub(1));
    SET_COLORS.get(i).copied().unwrap_or(AMBER)
}

/// The colors the screens draw with, besides the palette of iced.
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub background: Color,
    pub surface: Color,
    pub raised: Color,
    pub text: Color,
    pub muted: Color,
    /// the actions; darker on light, to read on white
    pub accent: Color,
    pub court: Color,
    pub court_lines: Color,
}

pub const DARK: Colors = Colors {
    background: rgb(0x121212),
    surface: rgb(0x18181B),
    raised: rgb(0x2F2F31),
    text: rgb(0xEEEEEE),
    muted: rgb(0xA8A8A8),
    accent: AMBER,
    court: rgb(0x1F2A24),
    court_lines: rgb(0xD4D4D8),
};

pub const LIGHT: Colors = Colors {
    background: rgb(0xFAFAFA),
    surface: rgb(0xFFFFFF),
    raised: rgb(0xE7E7EA),
    text: rgb(0x18181B),
    muted: rgb(0x5B5B60),
    accent: rgb(0xB45309),
    court: rgb(0xE8F0EA),
    court_lines: rgb(0x3F3F46),
};

pub fn colors(dark: bool) -> Colors {
    match dark {
        true => DARK,
        false => LIGHT,
    }
}

pub fn theme(dark: bool) -> Theme {
    let c = colors(dark);
    Theme::custom(
        match dark {
            true => "scout4all dark",
            false => "scout4all light",
        },
        Palette {
            background: c.background,
            text: c.text,
            primary: c.accent,
            success: WON,
            warning: FAIR,
            danger: LOST,
        },
    )
}

fn lerp(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgb(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}

/// How good a figure is for the level of the team, from 0 (red) through
/// yellow to 1 (green), as in the app.
pub fn grade_color(grade: f64) -> Color {
    let t = grade.clamp(0.0, 1.0) as f32;
    match t < 0.5 {
        true => lerp(LOST, FAIR, t * 2.0),
        false => lerp(FAIR, WON, (t - 0.5) * 2.0),
    }
}
