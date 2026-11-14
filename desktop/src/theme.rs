//! The look of the desktop app, the one of the site and of the Android
//! app: zinc surfaces, the amber of the actions, the white ball of the logo.
//! Dark, and a light version of it.

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

/// The two teams wherever they're told apart (the points in a row, the
/// events of the video): ours light blue, theirs pink.
pub const TEAM_US: Color = rgb(0x38BDF8);
pub const TEAM_THEM: Color = rgb(0xF472B6);

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

/// The font of the app: IBM Plex Mono; the name in the bar is the one of
/// the site, drawn (`assets/wordmark.svg`).
pub fn fonts() -> &'static Fonts {
    &PLEX
}

// font sizes
pub const SIZE_SMALL: f32 = 13.0;
pub const SIZE_BODY: f32 = 15.0;
pub const SIZE_TITLE: f32 = 18.0;
pub const SIZE_SCORE: f32 = 34.0;

// borders
pub const RADIUS_PANEL: f32 = 12.0;
pub const RADIUS_PILL: f32 = 180.0;
pub const BORDER: f32 = 1.0;

// --- colors ---------------------------------------------------------------

/// The colors the screens draw with.
#[derive(Debug, Clone, Copy)]
pub struct Colors {
    pub dark: bool,
    /// behind everything
    pub background: Color,
    /// the bars at the top and at the bottom
    pub bar: Color,
    pub panel: Color,
    /// the borders of the panels and of the buttons
    pub border: Color,
    /// the buttons at rest, and the row under the mouse
    pub button: Color,
    pub text: Color,
    pub muted: Color,
    /// the actions and what's chosen
    pub accent: Color,
    /// the text on the accent
    pub on_accent: Color,
    pub court: Color,
    pub court_lines: Color,
    pub danger: Color,
    /// the score of a match won, of one lost
    pub won: Color,
    pub lost: Color,
    /// the phase of the rally: receiving (side out), serving (break point)
    pub side_out: Color,
    pub break_point: Color,
}

/// The surfaces of the site (`web/src/assets/css/global.css`).
pub const DARK: Colors = Colors {
    dark: true,
    background: rgb(0x121212),
    bar: rgb(0x18181B),
    panel: rgb(0x18181B),
    border: rgb(0x2F2F31),
    button: rgb(0x2F2F31),
    text: rgb(0xEEEEEE),
    muted: rgb(0xA8A8A8),
    accent: AMBER,
    on_accent: rgb(0x121212),
    court: rgb(0x1B2A44),
    court_lines: rgb(0xCBD5E1),
    danger: LOST,
    won: WON,
    lost: LOST,
    side_out: rgb(0x38BDF8),
    break_point: rgb(0xFB923C),
};

pub const LIGHT: Colors = Colors {
    dark: false,
    background: rgb(0xF4F4F5),
    bar: rgb(0xFFFFFF),
    panel: rgb(0xFFFFFF),
    border: rgb(0xE4E4E7),
    button: rgb(0xE4E4E7),
    text: rgb(0x18181B),
    muted: rgb(0x52525B),
    // darker, to read on white
    accent: rgb(0xB45309),
    on_accent: rgb(0xFFFFFF),
    court: rgb(0xE6EEF8),
    court_lines: rgb(0x334155),
    danger: rgb(0xB91C1C),
    // darker, to read on white
    won: rgb(0x15803D),
    lost: rgb(0xB91C1C),
    side_out: rgb(0x0369A1),
    break_point: rgb(0xC2410C),
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

/// The ball of the logo, white, and the name of the app, as on the site.
pub const BALL: &[u8] = include_bytes!("../assets/ball.svg");
pub const WORDMARK: &[u8] = include_bytes!("../assets/wordmark.svg");

/// The icons, the ones of the Android app (`assets/icons`).
pub mod icons {
    pub const HOME: &[u8] = include_bytes!("../assets/icons/home.svg");
    pub const AWAY: &[u8] = include_bytes!("../assets/icons/away.svg");
    pub const BACK: &[u8] = include_bytes!("../assets/icons/back.svg");
    pub const ADD: &[u8] = include_bytes!("../assets/icons/add.svg");
    pub const SETTINGS: &[u8] = include_bytes!("../assets/icons/settings.svg");
    pub const THEME: &[u8] = include_bytes!("../assets/icons/theme.svg");
    pub const SIDE_OUT: &[u8] = include_bytes!("../assets/icons/side_out.svg");
    pub const BREAK: &[u8] = include_bytes!("../assets/icons/break.svg");
    pub const QUIT: &[u8] = include_bytes!("../assets/icons/quit.svg");
    pub const PLAY: &[u8] = include_bytes!("../assets/icons/play.svg");
    pub const PAUSE: &[u8] = include_bytes!("../assets/icons/pause.svg");
    pub const REWIND: &[u8] = include_bytes!("../assets/icons/rewind.svg");
    pub const FORWARD: &[u8] = include_bytes!("../assets/icons/forward.svg");
    pub const ANALYSIS: &[u8] = include_bytes!("../assets/icons/analytics.svg");
    pub const EDIT: &[u8] = include_bytes!("../assets/icons/edit.svg");
    pub const IMPORT: &[u8] = include_bytes!("../assets/icons/file_download.svg");
    pub const STATS: &[u8] = include_bytes!("../assets/icons/insert_chart.svg");
    pub const PDF: &[u8] = include_bytes!("../assets/icons/picture_as_pdf.svg");
    pub const VIDEO: &[u8] = include_bytes!("../assets/icons/videocam.svg");
    pub const RECORD: &[u8] = include_bytes!("../assets/icons/radio_button_checked.svg");
    pub const WON: &[u8] = include_bytes!("../assets/icons/emoji_events.svg");
    pub const LOST: &[u8] = include_bytes!("../assets/icons/cancel.svg");
    pub const COIN: &[u8] = include_bytes!("../assets/icons/toll.svg");
    pub const CALENDAR: &[u8] = include_bytes!("../assets/icons/event.svg");
    pub const VOLLEYBALL: &[u8] = include_bytes!("../assets/icons/sports_volleyball.svg");
    pub const CLOSE: &[u8] = include_bytes!("../assets/icons/close.svg");
    pub const CHECK: &[u8] = include_bytes!("../assets/icons/check.svg");
    pub const FORWARD_ARROW: &[u8] = include_bytes!("../assets/icons/arrow_forward.svg");
    pub const SHARE: &[u8] = include_bytes!("../assets/icons/share.svg");
    pub const SAVE: &[u8] = include_bytes!("../assets/icons/save.svg");
    pub const DELETE: &[u8] = include_bytes!("../assets/icons/delete.svg");
}
