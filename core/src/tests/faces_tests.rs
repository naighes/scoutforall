//! The face in a photo: found, cropped with margin, refused when there is
//! none or more than one. The fixtures are crops of the Solvay conference
//! photo of 1927 (public domain).

#[cfg(test)]
mod tests {
    use crate::faces::{PhotoError, Square, crop_face, crop_square};
    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
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
}
