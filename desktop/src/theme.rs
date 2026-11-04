//! The look of the desktop app: the amber of the Android app on a deep
//! blue, bars of amber at the top and at the bottom, round panels with
//! light borders, pill buttons, one monospaced font. Dark, and a light
//! version of it.

use iced::{
    Color, Font, Theme,
    font::{Family, Weight},
    theme::Palette,
};

const fn rgb(hex: u32) -> Color {
    Color::from_rgb8(
        ((hex >> 16) & 0xFF) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}

/// The actions and what's chosen, as in the app.
pub const AMBER: Color = rgb(0xFCD34D);
pub const WON: Color = rgb(0x4ADE80);
pub const LOST: Color = rgb(0xF87171);
pub const FAIR: Color = rgb(0xFACC15);

// --- fonts ----------------------------------------------------------------

/// The font of the app, embedded (SIL Open Font License, see
/// `assets/fonts`): the same on every system. Japanese falls back on the
/// fonts of the system.
pub struct Fonts {
    pub regular: Font,
    pub bold: Font,
    pub bytes: [&'static [u8]; 2],
}

const PLEX: Fonts = Fonts {
    regular: Font::with_name("IBM Plex Mono"),
    bold: Font {
        family: Family::Name("IBM Plex Mono"),
        weight: Weight::Semibold,
        ..Font::DEFAULT
    },
    bytes: [
        include_bytes!("../assets/fonts/IBMPlexMono-Regular.ttf"),
        include_bytes!("../assets/fonts/IBMPlexMono-SemiBold.ttf"),
    ],
};

/// The font of the app: IBM Plex Mono.
pub fn fonts() -> &'static Fonts {
    &PLEX
}

// font sizes
pub const SIZE_SMALL: f32 = 13.0;
pub const SIZE_BODY: f32 = 15.0;
pub const SIZE_TITLE: f32 = 18.0;
pub const SIZE_SCORE: f32 = 34.0;

// borders
pub const RADIUS_PANEL: f32 = 15.0;
pub const RADIUS_PILL: f32 = 180.0;
pub const BORDER: f32 = 2.0;

// --- colors ---------------------------------------------------------------

/// The colors the screens draw with.
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub dark: bool,
    /// behind everything
    pub background: Color,
    /// the panels, a veil on the background
    pub panel: Color,
    /// the borders of the panels and of the buttons
    pub border: Color,
    /// the buttons at rest, and the row under the mouse
    pub button: Color,
    pub text: Color,
    pub muted: Color,
    /// the actions, the titles, the bars
    pub accent: Color,
    /// the text on the bars
    pub on_accent: Color,
    pub court: Color,
    pub court_lines: Color,
    pub danger: Color,
}

pub const DARK: Colors = Colors {
    dark: true,
    background: rgb(0x0F172A),
    panel: Color {
        a: 0.55,
        ..rgb(0x1E293B)
    },
    border: Color {
        a: 0.55,
        ..rgb(0x475569)
    },
    button: rgb(0x334155),
    text: rgb(0xEEF2F7),
    muted: rgb(0x94A3B8),
    accent: AMBER,
    on_accent: rgb(0x0F172A),
    court: rgb(0x1B2A44),
    court_lines: rgb(0xCBD5E1),
    danger: LOST,
};

pub const LIGHT: Colors = Colors {
    dark: false,
    background: rgb(0xEEF2F7),
    panel: Color {
        a: 0.85,
        ..rgb(0xFFFFFF)
    },
    border: Color {
        a: 0.8,
        ..rgb(0xCBD5E1)
    },
    button: rgb(0xE2E8F0),
    text: rgb(0x0F172A),
    muted: rgb(0x475569),
    // darker, to read on white; the bars keep the amber of the app
    accent: rgb(0xB45309),
    on_accent: rgb(0x0F172A),
    court: rgb(0xE6EEF8),
    court_lines: rgb(0x334155),
    danger: rgb(0xB91C1C),
};

pub fn colors(dark: bool) -> Colors {
    match dark {
        true => DARK,
        false => LIGHT,
    }
}

/// The amber of the bars, the same on both themes.
pub const BAR: Color = AMBER;

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
            danger: c.danger,
        },
    )
}

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

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgb(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}

/// How good a figure is for the level of the team, from 0 (red) through
/// yellow to 1 (green), as in the app; darker on the light theme, to read
/// on white.
pub fn grade_color(grade: f64, c: Colors) -> Color {
    let t = grade.clamp(0.0, 1.0) as f32;
    let color = match t < 0.5 {
        true => mix(LOST, FAIR, t * 2.0),
        false => mix(FAIR, WON, (t - 0.5) * 2.0),
    };
    match c.dark {
        true => color,
        false => mix(color, Color::BLACK, 0.35),
    }
}
