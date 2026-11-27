//! The images of the teams and of the persons (TODO/011): decoded whatever
//! the format the user picked (PNG, JPEG, WebP), made small and stored in
//! the table `assets`, which the team (`logo_asset_id`) or the person
//! (`photo_asset_id`) points to. The same code serves every app.
//!
//! - a logo keeps its proportions and its transparency (PNG);
//! - a photo is a square PNG, so the transparent background is kept.

use crate::{
    errors::{AppError, IOError},
    providers::sqlite::Database,
};
use image::{
    DynamicImage, ImageEncoder, ImageFormat, codecs::png::PngEncoder, imageops::FilterType,
};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

/// The longest side of a logo, in pixels.
pub const LOGO_MAX_SIDE: u32 = 256;
/// The side of a photo, in pixels.
pub const PHOTO_SIDE: u32 = 512;
/// The largest file taken in, before decoding: a bigger one is refused
/// instead of filling the memory.
pub const MAX_INPUT_BYTES: usize = 25 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    TeamLogo,
    PersonPhoto,
}

impl AssetKind {
    fn tag(self) -> &'static str {
        match self {
            AssetKind::TeamLogo => "team_logo",
            AssetKind::PersonPhoto => "person_photo",
        }
    }
}

/// An image ready to be shown or stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Image {
    /// The pixels as RGBA, to show them without a decoder at hand.
    pub fn rgba(&self) -> Result<(u32, u32, Vec<u8>), AppError> {
        let image = decode_image(&self.data)?.to_rgba8();
        let (width, height) = image.dimensions();
        Ok((width, height, image.into_raw()))
    }
}

fn msg(text: impl Into<String>) -> AppError {
    AppError::IO(IOError::Msg(text.into()))
}

/// PNG, JPEG or WebP: what the bytes are, not what the name says.
pub(crate) fn decode_image(bytes: &[u8]) -> Result<DynamicImage, AppError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(msg("image too large"));
    }
    let format = image::guess_format(bytes).map_err(|e| msg(e.to_string()))?;
    match format {
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP => {
            image::load_from_memory_with_format(bytes, format).map_err(|e| msg(e.to_string()))
        }
        other => Err(msg(format!("unsupported image format {other:?}"))),
    }
}

/// The logo: scaled down to fit `LOGO_MAX_SIDE` (never up), proportions and
/// transparency kept, as PNG.
pub fn normalize_logo(bytes: &[u8]) -> Result<Image, AppError> {
    let source = decode_image(bytes)?;
    let fitted = match source.width().max(source.height()) > LOGO_MAX_SIDE {
        true => source.resize(LOGO_MAX_SIDE, LOGO_MAX_SIDE, FilterType::Lanczos3),
        false => source,
    }
    .to_rgba8();
    let (width, height) = fitted.dimensions();
    let mut data = Vec::new();
    PngEncoder::new(&mut data)
        .write_image(&fitted, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| msg(e.to_string()))?;
    Ok(Image {
        mime: "image/png",
        width,
        height,
        data,
    })
}

/// How bright the visible part of a logo is: a light one may vanish on a
/// light background, a dark one on a dark background (TODO/011 R8.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogoTone {
    Light,
    Dark,
    /// colors that show on any background, a logo with a background of
    /// its own (no transparency) or one that can't be read
    Mixed,
}

/// What goes behind a logo so it doesn't vanish on the background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogoBackdrop {
    None,
    Dark,
    Light,
}

/// The share of the visible part (weighted by opacity) that must be light,
/// or dark, for the whole logo to be.
const TONE_SHARE: f64 = 0.8;
/// A pixel this bright or more has less than 2:1 of contrast with white.
const LIGHT_LUMINANCE: f64 = 0.45;
/// A pixel this bright or less has less than 2:1 of contrast with the
/// near-black backgrounds of the apps.
const DARK_LUMINANCE: f64 = 0.07;

/// The relative luminance of an sRGB pixel (WCAG).
fn luminance(r: u8, g: u8, b: u8) -> f64 {
    let linear = |c: u8| match f64::from(c) / 255.0 {
        v if v <= 0.04045 => v / 12.92,
        v => ((v + 0.055) / 1.055).powf(2.4),
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/// The tone of the logo the bytes hold (any format taken as a logo).
pub fn logo_tone(bytes: &[u8]) -> LogoTone {
    decode_image(bytes).map_or(LogoTone::Mixed, |image| tone_of(&image.to_rgba8()))
}

fn tone_of(image: &image::RgbaImage) -> LogoTone {
    let (visible, light, dark, transparent) = image.pixels().fold(
        (0.0, 0.0, 0.0, false),
        |(visible, light, dark, transparent), p| {
            let [r, g, b, a] = p.0;
            let weight = f64::from(a) / 255.0;
            let l = luminance(r, g, b);
            (
                visible + weight,
                light + weight * f64::from(u8::from(l >= LIGHT_LUMINANCE)),
                dark + weight * f64::from(u8::from(l <= DARK_LUMINANCE)),
                transparent || a < u8::MAX,
            )
        },
    );
    match (transparent, visible > 0.0) {
        (true, true) if light >= TONE_SHARE * visible => LogoTone::Light,
        (true, true) if dark >= TONE_SHARE * visible => LogoTone::Dark,
        _ => LogoTone::Mixed,
    }
}

impl LogoTone {
    /// The backdrop on the theme: only a logo of the same tone as the
    /// background gets one, of the other tone.
    pub fn backdrop(self, dark_theme: bool) -> LogoBackdrop {
        match (self, dark_theme) {
            (LogoTone::Light, false) => LogoBackdrop::Dark,
            (LogoTone::Dark, true) => LogoBackdrop::Light,
            _ => LogoBackdrop::None,
        }
    }
}

/// The photo: a square of `PHOTO_SIDE` (the image is expected to be already
/// the person, cropped), with transparency kept, as PNG.
pub fn normalize_photo(bytes: &[u8]) -> Result<Image, AppError> {
    let square = decode_image(bytes)?.resize_exact(PHOTO_SIDE, PHOTO_SIDE, FilterType::Lanczos3);
    let rgba = square.to_rgba8();
    let mut data = Vec::new();
    PngEncoder::new(&mut data)
        .write_image(
            &rgba,
            PHOTO_SIDE,
            PHOTO_SIDE,
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| msg(e.to_string()))?;
    Ok(Image {
        mime: "image/png",
        width: PHOTO_SIDE,
        height: PHOTO_SIDE,
        data,
    })
}

/// Stores the image and returns its id.
pub fn insert(db: &Connection, kind: AssetKind, image: &Image) -> Result<String, AppError> {
    let id = Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO assets (id, kind, mime, width, height, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            kind.tag(),
            image.mime,
            image.width,
            image.height,
            image.data
        ],
    )
    .map_err(|e| AppError::IO(e.into()))?;
    Ok(id)
}

pub fn load(db: &Connection, id: &str) -> Result<Option<Image>, AppError> {
    db.query_row(
        "SELECT mime, width, height, data FROM assets WHERE id = ?1",
        [id],
        |row| {
            let mime: String = row.get(0)?;
            Ok(Image {
                mime: match mime.as_str() {
                    "image/png" => "image/png",
                    _ => "image/jpeg",
                },
                width: row.get(1)?,
                height: row.get(2)?,
                data: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|e| AppError::IO(e.into()))
}

/// Removes the image; whoever pointed to it must have been cleared first.
pub fn delete(db: &Connection, id: &str) -> Result<(), AppError> {
    db.execute("DELETE FROM assets WHERE id = ?1", [id])
        .map(|_| ())
        .map_err(|e| AppError::IO(e.into()))
}

/// The logo of a team: the new one replaces the previous (removed), `None`
/// removes it. Nothing changes if the image isn't valid.
pub fn set_team_logo(db: &Connection, team_id: &str, bytes: Option<&[u8]>) -> Result<(), AppError> {
    replace(
        db,
        "teams",
        "logo_asset_id",
        AssetKind::TeamLogo,
        team_id,
        bytes.map(normalize_logo),
    )
}

/// The photo of a person, as `set_team_logo`; `bytes` is the person already
/// cropped.
pub fn set_person_photo(
    db: &Connection,
    person_id: &str,
    bytes: Option<&[u8]>,
) -> Result<(), AppError> {
    replace(
        db,
        "persons",
        "photo_asset_id",
        AssetKind::PersonPhoto,
        person_id,
        bytes.map(normalize_photo),
    )
}

fn replace(
    db: &Connection,
    table: &str,
    column: &str,
    kind: AssetKind,
    owner: &str,
    image: Option<Result<Image, AppError>>,
) -> Result<(), AppError> {
    let new = image.transpose()?;
    let previous: Option<String> = db
        .query_row(
            &format!("SELECT {column} FROM {table} WHERE id = ?1"),
            [owner],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| AppError::IO(e.into()))?
        .flatten();
    let new_id = new.map(|image| insert(db, kind, &image)).transpose()?;
    db.execute(
        &format!("UPDATE {table} SET {column} = ?1 WHERE id = ?2"),
        params![new_id, owner],
    )
    .map_err(|e| AppError::IO(e.into()))?;
    previous.map(|id| delete(db, &id)).transpose().map(|_| ())
}

/// The logo of the team, if it has one.
pub fn team_logo(db: &Database, team_id: &str) -> Result<Option<Image>, AppError> {
    db.read(|tx| {
        let id: Option<String> = tx
            .query_row(
                "SELECT logo_asset_id FROM teams WHERE id = ?1",
                [team_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| AppError::IO(e.into()))?
            .flatten();
        id.map(|id| load(tx, &id)).transpose().map(Option::flatten)
    })
}

/// The logo of the team `team_id` plays against in the match, if the
/// opponent is a team with one.
pub fn opponent_logo(
    db: &Database,
    match_id: &str,
    team_id: &str,
) -> Result<Option<Image>, AppError> {
    db.read(|tx| {
        let id: Option<String> = tx
            .query_row(
                "SELECT t.logo_asset_id FROM matches m
                 JOIN teams t
                   ON t.id = CASE m.team_a WHEN ?2 THEN m.team_b ELSE m.team_a END
                 WHERE m.id = ?1",
                [match_id, team_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| AppError::IO(e.into()))?
            .flatten();
        id.map(|id| load(tx, &id)).transpose().map(Option::flatten)
    })
}

/// `set_team_logo` in one transaction on the shared database.
pub fn store_team_logo(db: &Database, team_id: &str, bytes: Option<&[u8]>) -> Result<(), AppError> {
    db.write(|tx| set_team_logo(tx, team_id, bytes))
}

/// The photo of the person (as PNG for new photos; old JPEGs remain readable), if they have one.
pub fn person_photo(db: &Database, person_id: &str) -> Result<Option<Image>, AppError> {
    db.read(|tx| {
        let id: Option<String> = tx
            .query_row(
                "SELECT photo_asset_id FROM persons WHERE id = ?1",
                [person_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| AppError::IO(e.into()))?
            .flatten();
        id.map(|id| load(tx, &id)).transpose().map(Option::flatten)
    })
}

/// `set_person_photo` in one transaction on the shared database.
pub fn store_person_photo(
    db: &Database,
    person_id: &str,
    bytes: Option<&[u8]>,
) -> Result<(), AppError> {
    db.write(|tx| set_person_photo(tx, person_id, bytes))
}
