//! The face in a photo: found, cropped with margin, refused when there is
//! none or more than one. The fixtures are crops of the Solvay conference
//! photo of 1927 (public domain).

#[cfg(test)]
mod tests {
    use crate::{
        assets::PHOTO_SIDE,
        faces::{PhotoError, Square, crop_face, crop_square, mask_in, mask_of},
    };
    use image::{
        DynamicImage, GrayImage, ImageFormat, Luma, Rgb, RgbImage,
        imageops::{self, FilterType},
    };
    use std::io::Cursor;

    const ONE_FACE: &[u8] = include_bytes!("fixtures/solvay_one_face.jpg");
    const THREE_FACES: &[u8] = include_bytes!("fixtures/solvay_three_faces.jpg");

    #[test]
    fn a_photo_with_one_face_is_cropped_as_a_square_around_it() {
        let png = crop_face(ONE_FACE).unwrap();
        let image = image::load_from_memory_with_format(&png, ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        assert_eq!(image.width(), image.height());
        assert!(image.width() >= 65, "the crop holds the face and a margin");
        assert!(image.width() <= 100, "the crop is inside the photo");
        assert!(image.get_pixel(image.width() / 2, image.height() / 2)[3] > 0);
        assert!(image.pixels().any(|pixel| pixel[3] == 0));
    }

    #[test]
    fn a_photo_with_several_faces_is_refused() {
        assert!(matches!(
            crop_face(THREE_FACES),
            Err(PhotoError::ManyFaces(3))
        ));
    }

    #[test]
    fn a_photo_without_a_face_is_refused() {
        let flat = DynamicImage::ImageRgb8(RgbImage::from_pixel(300, 300, Rgb([90, 120, 60])));
        let mut bytes = Cursor::new(Vec::new());
        flat.write_to(&mut bytes, ImageFormat::Png).unwrap();
        assert!(matches!(
            crop_face(&bytes.into_inner()),
            Err(PhotoError::NoFace)
        ));
    }

    #[test]
    fn something_that_is_not_an_image_is_an_error() {
        assert!(matches!(
            crop_face(b"not an image"),
            Err(PhotoError::Image(_))
        ));
    }

    #[test]
    fn the_crop_has_a_margin_and_stays_inside_the_photo() {
        // in the middle: wider margin, with the face above the middle
        let middle = crop_square((400, 300, 100, 100), 1000, 800);
        assert_eq!(
            middle,
            Square {
                x: 340,
                y: 260,
                side: 220
            }
        );
        // at the corner: shifted, not cut
        let corner = crop_square((0, 0, 100, 100), 1000, 800);
        assert_eq!((corner.x, corner.y, corner.side), (0, 0, 220));
        let far = crop_square((950, 750, 100, 100), 1000, 800);
        assert_eq!((far.x + far.side, far.y + far.side), (1000, 800));
        // a face as large as the photo: the square shrinks to the photo
        let big = crop_square((0, 0, 90, 90), 100, 120);
        assert_eq!(big.side, 100);
    }

    const SIDE: usize = 320;
    const SHAPE: [usize; 4] = [1, 1, SIDE, SIDE];

    /// Values going from 2 to 4 over the whole mask.
    fn values() -> Vec<f32> {
        let last = (SIDE * SIDE - 1) as f32;
        (0..SIDE * SIDE)
            .map(|i| 2.0 + 2.0 * i as f32 / last)
            .collect()
    }

    #[test]
    fn the_mask_takes_the_whole_grey_scale() {
        let mask = mask_of(&SHAPE, &values()).unwrap();
        assert_eq!(mask.get_pixel(0, 0).0, [0]);
        assert_eq!(mask.get_pixel(319, 319).0, [255]);
    }

    #[test]
    fn a_mask_that_cant_be_used_is_an_error() {
        let with = |value| {
            let mut values = values();
            values[7] = value;
            values
        };
        let refused = [
            mask_of(&[1, 1, SIDE, 1], &values()),
            mask_of(&SHAPE, &values()[1..]),
            mask_of(&SHAPE, &vec![0.5; SIDE * SIDE]),
            mask_of(&SHAPE, &with(f32::NAN)),
            mask_of(&SHAPE, &with(f32::INFINITY)),
            mask_of(&SHAPE, &[]),
        ];
        for mask in refused {
            assert!(matches!(mask, Err(PhotoError::Image(_))));
        }
    }

    /// A person in the middle of the mask, with a soft edge.
    fn person_mask() -> GrayImage {
        GrayImage::from_fn(320, 320, |x, y| {
            let distance = ((f64::from(x) - 160.0).powi(2) + (f64::from(y) - 140.0).powi(2)).sqrt();
            Luma([((110.0 - distance) * 20.0).clamp(0.0, 255.0) as u8])
        })
    }

    #[test]
    fn the_mask_under_the_square_is_the_one_of_the_whole_photo() {
        let mask = person_mask();
        for ((width, height), square) in [
            (
                (100, 120),
                Square {
                    x: 0,
                    y: 10,
                    side: 100,
                },
            ),
            (
                (640, 480),
                Square {
                    x: 160,
                    y: 0,
                    side: 480,
                },
            ),
            (
                (3000, 3600),
                Square {
                    x: 900,
                    y: 700,
                    side: 1500,
                },
            ),
            (
                (6000, 7200),
                Square {
                    x: 0,
                    y: 0,
                    side: 300,
                },
            ),
            (
                (6000, 7200),
                Square {
                    x: 2700,
                    y: 2500,
                    side: 300,
                },
            ),
            (
                (6000, 7200),
                Square {
                    x: 5700,
                    y: 6900,
                    side: 300,
                },
            ),
            (
                (7200, 6000),
                Square {
                    x: 1000,
                    y: 0,
                    side: 6000,
                },
            ),
        ] {
            // the mask of the whole photo at the scale of the result
            let side = square.side.min(PHOTO_SIDE);
            let scaled =
                |v: u32| (f64::from(v) * f64::from(side) / f64::from(square.side)).round() as u32;
            let whole =
                imageops::resize(&mask, scaled(width), scaled(height), FilterType::Lanczos3);
            let (left, top) = (scaled(square.x), scaled(square.y));
            let found = mask_in(&mask, (width, height), square);
            assert_eq!(found.dimensions(), (side, side), "{square:?}");
            // a pixel apart at most: each value within those around its
            // place in the whole (the square starts between two pixels of
            // the mask, and its stretch is rounded to whole pixels)
            let (wide, tall) = whole.dimensions();
            let near = |v: u32, limit: u32| v.saturating_sub(1)..=(v + 1).min(limit - 1);
            let around = |x: u32, y: u32| {
                near(left + x, wide).flat_map(move |x| near(top + y, tall).map(move |y| (x, y)))
            };
            let outside = found.enumerate_pixels().find(|(x, y, value)| {
                let values: Vec<u8> = around(*x, *y)
                    .map(|(x, y)| whole.get_pixel(x, y)[0])
                    .collect();
                let (low, high) = (values.iter().min(), values.iter().max());
                low.zip(high).is_none_or(|(low, high)| {
                    value[0] < low.saturating_sub(4) || value[0] > high.saturating_add(4)
                })
            });
            assert_eq!(outside, None, "{square:?}");
        }
    }

    #[test]
    fn a_large_photo_is_cropped_to_the_side_of_the_photo_stored() {
        let large = image::load_from_memory(ONE_FACE).unwrap().resize_exact(
            1500,
            1800,
            FilterType::CatmullRom,
        );
        let mut bytes = Cursor::new(Vec::new());
        large.write_to(&mut bytes, ImageFormat::Jpeg).unwrap();
        let png = crop_face(&bytes.into_inner()).unwrap();
        let image = image::load_from_memory_with_format(&png, ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        assert_eq!(image.dimensions(), (PHOTO_SIDE, PHOTO_SIDE));
        assert!(image.get_pixel(PHOTO_SIDE / 2, PHOTO_SIDE / 2)[3] > 0);
        assert!(image.pixels().any(|pixel| pixel[3] == 0));
    }
}
