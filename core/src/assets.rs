//! The images of the teams and of the persons (TODO/011): decoded whatever
//! the format the user picked (PNG, JPEG, WebP), made small and stored in
//! the table `assets`, which the team (`logo_asset_id`) or the person
//! (`photo_asset_id`) points to. The same code serves every app.
//!
//! - a logo keeps its proportions and its transparency (PNG);
//! - a photo is a square JPEG (no transparency to keep, much smaller).

use crate::{
    errors::{AppError, IOError},
    providers::sqlite::Database,
};
use image::{
    DynamicImage, ImageEncoder, ImageFormat,
    codecs::{jpeg::JpegEncoder, png::PngEncoder},
    imageops::FilterType,
};
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

/// The longest side of a logo, in pixels.
pub const LOGO_MAX_SIDE: u32 = 256;
/// The side of a photo, in pixels.
pub const PHOTO_SIDE: u32 = 256;
/// The quality of the JPEG of a photo.
pub const PHOTO_QUALITY: u8 = 80;
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

/// The photo: a square of `PHOTO_SIDE` (the image is expected to be already
/// the face, cropped), flattened on white, as JPEG.
pub fn normalize_photo(bytes: &[u8]) -> Result<Image, AppError> {
    let square = decode_image(bytes)?.resize_exact(PHOTO_SIDE, PHOTO_SIDE, FilterType::Lanczos3);
    let rgba = square.to_rgba8();
    let mut flat = image::RgbImage::new(PHOTO_SIDE, PHOTO_SIDE);
    flat.pixels_mut().zip(rgba.pixels()).for_each(|(out, px)| {
        let alpha = u32::from(px[3]);
        let on_white = |c: u8| ((u32::from(c) * alpha + 255 * (255 - alpha)) / 255) as u8;
        *out = image::Rgb([on_white(px[0]), on_white(px[1]), on_white(px[2])]);
    });
    let mut data = Vec::new();
    JpegEncoder::new_with_quality(&mut data, PHOTO_QUALITY)
        .write_image(
            &flat,
            PHOTO_SIDE,
            PHOTO_SIDE,
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| msg(e.to_string()))?;
    Ok(Image {
        mime: "image/jpeg",
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
                mime: if mime == "image/png" {
                    "image/png"
                } else {
                    "image/jpeg"
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

/// The photo of a person, as `set_team_logo`; `bytes` is the face already
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

/// `set_team_logo` in one transaction on the shared database.
pub fn store_team_logo(db: &Database, team_id: &str, bytes: Option<&[u8]>) -> Result<(), AppError> {
    db.write(|tx| set_team_logo(tx, team_id, bytes))
}

/// The photo of the person (the face, as JPEG), if they have one.
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
