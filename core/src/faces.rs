//! The person in a photo (TODO/011 §2.1): a detector finds the face, then a
//! local segmentation model removes the background before the square crop.
//! Both models run on the device (see `models/README.md`); nobody is
//! recognized.
//!
//! A photo is accepted with exactly one face: none or several are errors.

use crate::{assets, errors::AppError};
use image::{
    DynamicImage, GenericImageView, GrayImage, ImageFormat, RgbaImage, imageops::FilterType,
};
use rustface::{Detector, ImageData};
use std::{io::Cursor, sync::OnceLock};
use tract_onnx::prelude::{
    Framework, InferenceModelExt, IntoRunnable, Tensor, TypedRunnableModel, tvec,
};

static MODEL: &[u8] = include_bytes!("../models/seeta_fd_frontal_v1.0.bin");
static SEGMENTATION_MODEL: &[u8] = include_bytes!("../models/u2netp.onnx");

const SEGMENTATION_SIDE: u32 = 320;
static SEGMENTER: OnceLock<Result<std::sync::Arc<TypedRunnableModel>, String>> = OnceLock::new();

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
/// The crop is this many times the width of the face found (head and
/// shoulders where the source photo contains them).
const MARGIN: f64 = 2.2;

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
    // Put the face a little above the middle, leaving more of the torso in
    // the square while keeping the same framing for old circular thumbnails.
    let center_x = i64::from(x) + i64::from(w) / 2;
    let center_y = i64::from(y) + i64::from(h) / 2 + i64::from(h) / 5;
    let place = |center: i64, limit: u32| {
        (center - i64::from(side) / 2).clamp(0, i64::from(limit.saturating_sub(side))) as u32
    };
    Square {
        x: place(center_x, width),
        y: place(center_y, height),
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

/// A failure of the segmentation, told as an image error.
fn failed(message: impl std::fmt::Display) -> PhotoError {
    PhotoError::Image(AppError::IO(crate::errors::IOError::Msg(
        message.to_string(),
    )))
}

fn segmenter() -> Result<&'static std::sync::Arc<TypedRunnableModel>, PhotoError> {
    SEGMENTER
        .get_or_init(|| {
            tract_onnx::onnx()
                .model_for_read(&mut Cursor::new(SEGMENTATION_MODEL))
                .and_then(|model| model.into_optimized())
                .and_then(IntoRunnable::into_runnable)
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(failed)
}

/// The output of the model as a grey mask, its range stretched to the whole
/// scale: an unexpected size, values not finite or all alike are errors.
pub(crate) fn mask_of(shape: &[usize], values: &[f32]) -> Result<GrayImage, PhotoError> {
    let side = SEGMENTATION_SIDE as usize;
    let (minimum, maximum) = values
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), value| {
            (min.min(*value), max.max(*value))
        });
    let usable = values.iter().all(|v| v.is_finite()) && maximum > minimum;
    match (shape == [1, 1, side, side], usable) {
        (false, _) => Err(failed("unexpected segmentation mask size")),
        (_, false) => Err(failed("invalid segmentation mask")),
        (true, true) => {
            let pixels = values
                .iter()
                .map(|v| (((v - minimum) / (maximum - minimum)).clamp(0.0, 1.0) * 255.0) as u8)
                .collect();
            GrayImage::from_raw(SEGMENTATION_SIDE, SEGMENTATION_SIDE, pixels)
                .ok_or_else(|| failed("invalid segmentation mask size"))
        }
    }
}

/// Runs the local U2NetP model once per uploaded photo and applies its mask
/// to the original pixels. The loaded model is reused for later uploads.
fn remove_background(image: &DynamicImage) -> Result<RgbaImage, PhotoError> {
    let resized = image.resize_exact(SEGMENTATION_SIDE, SEGMENTATION_SIDE, FilterType::CatmullRom);
    let rgb = resized.to_rgb8();
    let mean = [0.485_f32, 0.456, 0.406];
    let deviation = [0.229_f32, 0.224, 0.225];
    let input_values: Vec<f32> = (0..3)
        .flat_map(|channel| {
            rgb.pixels().map(move |pixel| {
                (f32::from(pixel[channel]) / 255.0 - mean[channel]) / deviation[channel]
            })
        })
        .collect();
    let input = tract_onnx::prelude::tract_ndarray::Array4::from_shape_vec(
        (1, 3, SEGMENTATION_SIDE as usize, SEGMENTATION_SIDE as usize),
        input_values,
    )
    .map_err(failed)?;
    let output = segmenter()?
        .run(tvec!(Tensor::from(input).into()))
        .map_err(failed)?;
    let output = output
        .first()
        .ok_or_else(|| failed("segmentation produced no mask"))?;
    let output = output.to_plain_array_view::<f32>().map_err(failed)?;
    let values: Vec<f32> = output.iter().copied().collect();
    let mask = mask_of(output.shape(), &values)?;
    let mask = image::imageops::resize(&mask, image.width(), image.height(), FilterType::Lanczos3);
    let mut rgba = image.to_rgba8();
    rgba.pixels_mut()
        .zip(mask.pixels())
        .for_each(|(pixel, alpha)| {
            pixel[3] = alpha[0];
        });
    Ok(rgba)
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
    let foreground = remove_background(&image)?;
    let crop =
        DynamicImage::ImageRgba8(foreground).crop_imm(square.x, square.y, square.side, square.side);
    let mut out = Vec::new();
    crop.write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .map_err(|e| PhotoError::Image(AppError::IO(crate::errors::IOError::Msg(e.to_string()))))?;
    Ok(out)
}
