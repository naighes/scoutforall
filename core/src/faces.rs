//! The face of a person in a photo (TODO/011 §2.1): found by a detector that
//! runs on the device (`rustface`, the model is built in, see
//! `models/README.md`), then cropped as a square with some margin. Only the
//! position is found: nobody is recognized.
//!
//! A photo is accepted with exactly one face: none or several are errors.

use crate::{assets, errors::AppError};
use image::{DynamicImage, GenericImageView, ImageFormat, imageops::FilterType};
use rustface::{Detector, ImageData};
use std::io::Cursor;

static MODEL: &[u8] = include_bytes!("../models/seeta_fd_frontal_v1.0.bin");

/// The detector looks at an image no larger than this (the time grows with
/// the pixels, a face is still large in it).
const DETECT_SIDE: u32 = 640;
/// The smallest face found, in pixels of the image looked at.
const MIN_FACE: u32 = 40;
/// Below this score a window is not a face (the library suggests 2.0).
const MIN_SCORE: f64 = 2.0;
/// A face smaller than this fraction of the largest is background, not a
/// second person.
const BACKGROUND_FRACTION: f64 = 0.4;
/// The crop is this many times the width of the face found (hair, chin).
const MARGIN: f64 = 1.7;

#[derive(Debug)]
pub enum PhotoError {
    /// no face found
    NoFace,
    /// more than one face found
    ManyFaces(usize),
    Image(AppError),
}

impl std::fmt::Display for PhotoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PhotoError::NoFace => write!(f, "no face found"),
            PhotoError::ManyFaces(n) => write!(f, "{n} faces found"),
            PhotoError::Image(e) => write!(f, "{e}"),
        }
    }
}

impl From<AppError> for PhotoError {
    fn from(e: AppError) -> Self {
        PhotoError::Image(e)
    }
}

/// A square in the pixels of the image: left, top, side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Square {
    pub x: u32,
    pub y: u32,
    pub side: u32,
}

/// The boxes (left, top, width, height) of the faces that count: the ones
/// that are not background next to a much larger one.
fn significant(boxes: &[(i32, i32, u32, u32)]) -> Vec<(i32, i32, u32, u32)> {
    let largest = boxes.iter().map(|b| b.2).max().unwrap_or(0);
    boxes
        .iter()
        .copied()
        .filter(|b| f64::from(b.2) >= f64::from(largest) * BACKGROUND_FRACTION)
        .collect()
}

/// The square around a face, with margin, kept inside an image of
/// `width` x `height` (shifted, then shrunk, if it doesn't fit).
pub fn crop_square(face: (i32, i32, u32, u32), width: u32, height: u32) -> Square {
    let (x, y, w, h) = face;
    let side = ((f64::from(w.max(h)) * MARGIN).round() as u32)
        .min(width)
        .min(height)
        .max(1);
    let center = |start: i32, len: u32| i64::from(start) + i64::from(len) / 2;
    let place = |center: i64, limit: u32| {
        (center - i64::from(side) / 2).clamp(0, i64::from(limit - side)) as u32
    };
    Square {
        x: place(center(x, w), width),
        y: place(center(y, h), height),
        side,
    }
}

fn detector() -> Result<Box<dyn Detector>, PhotoError> {
    let model = rustface::read_model(Cursor::new(MODEL))
        .map_err(|e| PhotoError::Image(AppError::IO(crate::errors::IOError::Msg(e.to_string()))))?;
    let mut detector = rustface::create_detector_with_model(model);
    detector.set_min_face_size(MIN_FACE);
    detector.set_score_thresh(MIN_SCORE);
    detector.set_pyramid_scale_factor(0.8);
    detector.set_slide_window_step(4, 4);
    Ok(detector)
}

/// The significant faces of the image, in its own pixels.
fn faces(image: &DynamicImage) -> Result<Vec<(i32, i32, u32, u32)>, PhotoError> {
    let (width, height) = image.dimensions();
    let scale = f64::from(DETECT_SIDE) / f64::from(width.max(height));
    let (looked, scale) = if scale < 1.0 {
        let w = ((f64::from(width) * scale).round() as u32).max(1);
        let h = ((f64::from(height) * scale).round() as u32).max(1);
        (image.resize_exact(w, h, FilterType::Triangle), scale)
    } else {
        (image.clone(), 1.0)
    };
    let gray = looked.to_luma8();
    let (w, h) = gray.dimensions();
    let found = detector()?.detect(&ImageData::new(&gray, w, h));
    let boxes: Vec<_> = found
        .iter()
        .map(|f| {
            let b = f.bbox();
            let back = |v: i32| (f64::from(v) / scale).round() as i32;
            let size = |v: u32| (f64::from(v) / scale).round() as u32;
            (back(b.x()), back(b.y()), size(b.width()), size(b.height()))
        })
        .collect();
    Ok(significant(&boxes))
}

/// The face in the photo, cropped as a square, not yet reduced: PNG bytes,
/// to show as a preview and to hand to `assets::set_person_photo`.
pub fn crop_face(bytes: &[u8]) -> Result<Vec<u8>, PhotoError> {
    let image = assets::decode_image(bytes)?;
    let (width, height) = image.dimensions();
    let found = faces(&image)?;
    let face = match found.as_slice() {
        [] => return Err(PhotoError::NoFace),
        [one] => *one,
        many => return Err(PhotoError::ManyFaces(many.len())),
    };
    let square = crop_square(face, width, height);
    let mut out = Vec::new();
    image
        .crop_imm(square.x, square.y, square.side, square.side)
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|e| PhotoError::Image(AppError::IO(crate::errors::IOError::Msg(e.to_string()))))?;
    Ok(out)
}
