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
    fn a_photo_is_a_small_square_jpeg() {
        let bytes = encoded(900, 600, Rgba([120, 80, 60, 255]), ImageFormat::Png);
        let photo = assets::normalize_photo(&bytes).unwrap();
        assert_eq!((photo.width, photo.height), (PHOTO_SIDE, PHOTO_SIDE));
        assert_eq!(photo.mime, "image/jpeg");
        assert!(photo.data.len() < bytes.len());
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
}
