//! The images of teams and persons: normalization, storage, replacement
//! and removal of the asset.

#[cfg(test)]
mod tests {
    use crate::{
        assets::{self, LOGO_MAX_SIDE, PHOTO_SIDE},
        db,
    };
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use rusqlite::Connection;
    use std::io::Cursor;

    const TEAM: &str = "11111111-1111-1111-1111-111111111111";
    const PERSON: &str = "22222222-2222-2222-2222-222222222222";

    fn encoded(width: u32, height: u32, pixel: Rgba<u8>, format: ImageFormat) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(width, height, pixel));
        let image = match format {
            ImageFormat::Jpeg => DynamicImage::ImageRgb8(image.to_rgb8()),
            _ => image,
        };
        image.write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    fn database() -> Connection {
        let db = db::open_in_memory().unwrap();
        db.execute(
            "INSERT INTO teams (id, name, year) VALUES (?1, 'Team', 2026)",
            [TEAM],
        )
        .unwrap();
        db.execute(
            "INSERT INTO persons (id, name) VALUES (?1, 'Person')",
            [PERSON],
        )
        .unwrap();
        db
    }

    fn count(db: &Connection) -> i64 {
        db.query_row("SELECT count(*) FROM assets", [], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn a_big_logo_shrinks_keeping_proportions_and_transparency() {
        let bytes = encoded(1024, 512, Rgba([200, 10, 10, 0]), ImageFormat::Png);
        let logo = assets::normalize_logo(&bytes).unwrap();
        assert_eq!(
            (logo.width, logo.height),
            (LOGO_MAX_SIDE, LOGO_MAX_SIDE / 2)
        );
        assert_eq!(logo.mime, "image/png");
        let back = image::load_from_memory(&logo.data).unwrap().to_rgba8();
        assert_eq!(back.get_pixel(0, 0)[3], 0);
    }

    #[test]
    fn a_small_logo_is_not_enlarged() {
        let bytes = encoded(40, 30, Rgba([1, 2, 3, 255]), ImageFormat::Png);
        let logo = assets::normalize_logo(&bytes).unwrap();
        assert_eq!((logo.width, logo.height), (40, 30));
    }

    #[test]
    fn png_jpeg_and_webp_are_taken_in_by_content() {
        [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP]
            .into_iter()
            .for_each(|format| {
                let bytes = encoded(64, 64, Rgba([9, 9, 9, 255]), format);
                assert!(assets::normalize_logo(&bytes).is_ok(), "{format:?}");
            });
    }

    #[test]
    fn what_is_not_a_supported_image_is_refused() {
        assert!(assets::normalize_logo(b"not an image").is_err());
        let gif = encoded(8, 8, Rgba([0, 0, 0, 255]), ImageFormat::Gif);
        assert!(assets::normalize_logo(&gif).is_err());
    }

    #[test]
    fn a_photo_is_a_small_square_png() {
        let bytes = encoded(900, 600, Rgba([120, 80, 60, 255]), ImageFormat::Png);
        let photo = assets::normalize_photo(&bytes).unwrap();
        assert_eq!((photo.width, photo.height), (PHOTO_SIDE, PHOTO_SIDE));
        assert_eq!(photo.mime, "image/png");
        assert_eq!(
            image::load_from_memory(&photo.data).unwrap().color(),
            image::ColorType::Rgba8
        );
    }

    #[test]
    fn a_photo_keeps_its_transparent_background() {
        let mut pixels = RgbaImage::from_pixel(80, 80, Rgba([10, 20, 30, 0]));
        for y in 30..50 {
            for x in 30..50 {
                pixels.put_pixel(x, y, Rgba([200, 100, 50, 255]));
            }
        }
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(pixels)
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        let photo = assets::normalize_photo(&bytes.into_inner()).unwrap();
        let restored = image::load_from_memory(&photo.data).unwrap().to_rgba8();
        assert_eq!(restored.get_pixel(0, 0)[3], 0);
        assert!(restored.get_pixel(PHOTO_SIDE / 2, PHOTO_SIDE / 2)[3] > 240);
    }

    #[test]
    fn existing_jpeg_photos_are_read_without_reencoding() {
        let db = database();
        let bytes = encoded(40, 30, Rgba([10, 20, 30, 255]), ImageFormat::Jpeg);
        db.execute(
            "INSERT INTO assets (id, kind, mime, width, height, data)
             VALUES ('44444444-4444-4444-4444-444444444444', 'person_photo', 'image/jpeg', 40, 30, ?1)",
            [&bytes],
        )
        .unwrap();
        db.execute(
            "UPDATE persons SET photo_asset_id = '44444444-4444-4444-4444-444444444444' WHERE id = ?1",
            [PERSON],
        )
        .unwrap();
        let photo = assets::load(&db, "44444444-4444-4444-4444-444444444444")
            .unwrap()
            .unwrap();
        assert_eq!(photo.mime, "image/jpeg");
        assert_eq!((photo.width, photo.height), (40, 30));
        assert_eq!(photo.data, bytes);
    }

    #[test]
    fn replacing_the_logo_removes_the_previous_asset() {
        let db = database();
        let first = encoded(50, 50, Rgba([1, 1, 1, 255]), ImageFormat::Png);
        let second = encoded(60, 60, Rgba([2, 2, 2, 255]), ImageFormat::Png);
        assets::set_team_logo(&db, TEAM, Some(&first)).unwrap();
        assets::set_team_logo(&db, TEAM, Some(&second)).unwrap();
        assert_eq!(count(&db), 1);
        let id: String = db
            .query_row(
                "SELECT logo_asset_id FROM teams WHERE id = ?1",
                [TEAM],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(assets::load(&db, &id).unwrap().unwrap().width, 60);
    }

    #[test]
    fn removing_the_logo_leaves_no_asset_and_no_reference() {
        let db = database();
        let bytes = encoded(50, 50, Rgba([1, 1, 1, 255]), ImageFormat::Png);
        assets::set_team_logo(&db, TEAM, Some(&bytes)).unwrap();
        assets::set_team_logo(&db, TEAM, None).unwrap();
        assert_eq!(count(&db), 0);
        let id: Option<String> = db
            .query_row(
                "SELECT logo_asset_id FROM teams WHERE id = ?1",
                [TEAM],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(id, None);
    }

    #[test]
    fn an_invalid_image_changes_nothing() {
        let db = database();
        let bytes = encoded(50, 50, Rgba([1, 1, 1, 255]), ImageFormat::Png);
        assets::set_person_photo(&db, PERSON, Some(&bytes)).unwrap();
        assert!(assets::set_person_photo(&db, PERSON, Some(b"junk")).is_err());
        assert_eq!(count(&db), 1);
    }

    /// A logo of `color` on a transparent square, `share` of it painted.
    fn on_transparent(color: [u8; 3], share: f64) -> Vec<u8> {
        let [r, g, b] = color;
        let side = 20u32;
        let painted = (f64::from(side * side) * share).round() as u32;
        let image = RgbaImage::from_fn(side, side, |x, y| match y * side + x < painted {
            true => Rgba([r, g, b, 255]),
            false => Rgba([0, 0, 0, 0]),
        });
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    /// Painted half white and half `other`, on a transparent square.
    fn white_and(other: [u8; 3]) -> Vec<u8> {
        let image = RgbaImage::from_fn(20, 20, |x, _| match x {
            0..5 => Rgba([255, 255, 255, 255]),
            5..10 => Rgba([other[0], other[1], other[2], 255]),
            _ => Rgba([0, 0, 0, 0]),
        });
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(image)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn the_tone_of_a_logo_is_the_one_of_its_visible_part() {
        use assets::{LogoTone, logo_tone};
        // the transparent part counts for nothing, however big
        assert_eq!(
            logo_tone(&on_transparent([255, 255, 255], 0.1)),
            LogoTone::Light
        );
        assert_eq!(
            logo_tone(&on_transparent([230, 230, 230], 0.9)),
            LogoTone::Light
        );
        assert_eq!(logo_tone(&on_transparent([0, 0, 0], 0.3)), LogoTone::Dark);
        assert_eq!(
            logo_tone(&on_transparent([30, 30, 40], 0.5)),
            LogoTone::Dark
        );
        // a color that shows on both backgrounds
        assert_eq!(
            logo_tone(&on_transparent([200, 30, 30], 0.5)),
            LogoTone::Mixed
        );
        assert_eq!(
            logo_tone(&on_transparent([128, 128, 128], 0.5)),
            LogoTone::Mixed
        );
        // half white, half dark: the dark half shows on a light background
        assert_eq!(logo_tone(&white_and([0, 0, 0])), LogoTone::Mixed);
        assert_eq!(logo_tone(&white_and([250, 250, 250])), LogoTone::Light);
    }

    #[test]
    fn a_logo_with_its_own_background_or_unreadable_has_no_tone() {
        use assets::{LogoTone, logo_tone};
        let opaque = encoded(30, 30, Rgba([255, 255, 255, 255]), ImageFormat::Png);
        assert_eq!(logo_tone(&opaque), LogoTone::Mixed);
        let jpeg = encoded(30, 30, Rgba([255, 255, 255, 255]), ImageFormat::Jpeg);
        assert_eq!(logo_tone(&jpeg), LogoTone::Mixed);
        assert_eq!(
            logo_tone(&on_transparent([255, 255, 255], 0.0)),
            LogoTone::Mixed
        );
        assert_eq!(logo_tone(b"junk"), LogoTone::Mixed);
    }

    #[test]
    fn only_a_logo_of_the_tone_of_the_background_gets_a_backdrop() {
        use assets::{LogoBackdrop, LogoTone};
        let cases = [
            (LogoTone::Light, false, LogoBackdrop::Dark),
            (LogoTone::Light, true, LogoBackdrop::None),
            (LogoTone::Dark, true, LogoBackdrop::Light),
            (LogoTone::Dark, false, LogoBackdrop::None),
            (LogoTone::Mixed, false, LogoBackdrop::None),
            (LogoTone::Mixed, true, LogoBackdrop::None),
        ];
        for (tone, dark_theme, backdrop) in cases {
            assert_eq!(tone.backdrop(dark_theme), backdrop, "{tone:?} {dark_theme}");
        }
    }
}
