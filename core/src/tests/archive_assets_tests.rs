//! The images in the archives (`providers::sqlite::archive_assets`,
//! TODO/011 §4): files apart from the JSON, which never points to them;
//! they come back with the team and the person; the local ones win; what
//! isn't an image refuses the archive.

#[cfg(test)]
mod tests {
    use crate::{
        assets,
        legacy::migrate::unzip,
        providers::sqlite::{
            Database,
            archive::{Subject, export, import, zip_files},
        },
        tests::sqlite_archive_tests::tests::{database, en, execute},
    };
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use serde_json::Value as Json;
    use std::io::Cursor;
    use zip::CompressionMethod::Deflated;

    const TEAM: &str = "11111111-1111-1111-1111-111111111111";
    const PERSON: &str = "22222222-2222-2222-2222-222222222222";
    const REMOVED: &str = "33333333-3333-3333-3333-333333333333";

    fn picture(pixel: [u8; 4]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(40, 30, Rgba(pixel)))
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    /// A team with a player and a removed one; the logo and the photos if asked.
    fn with_team(images: bool) -> Database {
        let db = database();
        execute(
            &db,
            &format!(
                "INSERT INTO teams (id, name, year) VALUES ('{TEAM}', 'Team', 2026);
                 INSERT INTO persons (id, name) VALUES ('{PERSON}', 'Player');
                 INSERT INTO persons (id, name) VALUES ('{REMOVED}', 'Removed');
                 INSERT INTO team_members (team_id, person_id, position, number, role, deleted)
                    VALUES ('{TEAM}', '{PERSON}', 0, 7, 'setter', 0);
                 INSERT INTO team_members (team_id, person_id, position, number, role, deleted)
                    VALUES ('{TEAM}', '{REMOVED}', 1, 8, 'setter', 1);"
            ),
        );
        if images {
            assets::store_team_logo(&db, TEAM, Some(&picture([200, 0, 0, 255]))).unwrap();
            assets::store_person_photo(&db, PERSON, Some(&picture([0, 200, 0, 255]))).unwrap();
            assets::store_person_photo(&db, REMOVED, Some(&picture([0, 0, 200, 255]))).unwrap();
        }
        db
    }

    fn uuid(id: &str) -> uuid::Uuid {
        uuid::Uuid::parse_str(id).unwrap()
    }

    #[test]
    fn the_images_go_as_files_and_the_rows_do_not_point_to_them() {
        let archive = export(&with_team(true), Subject::Team(uuid(TEAM))).unwrap();
        let files = unzip(&archive).unwrap();
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        // the logo and the photo of the player; not the one of who was removed
        assert_eq!(names.iter().filter(|n| n.starts_with("assets/")).count(), 2);
        assert!(names.contains(&"assets.json"));
        for table in ["teams.json", "persons.json"] {
            let data = &files.iter().find(|(n, _)| n == table).unwrap().1;
            let text = String::from_utf8(data.clone()).unwrap();
            assert!(!text.contains("asset_id"), "{table}");
        }
        let _: Json =
            serde_json::from_slice(&files.iter().find(|(n, _)| n == "assets.json").unwrap().1)
                .unwrap();
    }

    #[test]
    fn an_archive_without_images_has_none_of_the_files() {
        let archive = export(&with_team(false), Subject::Team(uuid(TEAM))).unwrap();
        assert!(
            unzip(&archive)
                .unwrap()
                .iter()
                .all(|(n, _)| n != "assets.json" && !n.starts_with("assets/"))
        );
    }

    #[test]
    fn the_images_come_back_with_the_team_and_the_person() {
        let archive = export(&with_team(true), Subject::Team(uuid(TEAM))).unwrap();
        let there = database();
        import(&there, &archive, &en()).unwrap();
        let logo = assets::team_logo(&there, TEAM).unwrap().unwrap();
        assert_eq!((logo.width, logo.height, logo.mime), (40, 30, "image/png"));
        let photo = assets::person_photo(&there, PERSON).unwrap().unwrap();
        assert_eq!((photo.width, photo.height), (256, 256));
        assert!(assets::person_photo(&there, REMOVED).unwrap().is_none());
        // again: nothing doubles
        import(&there, &archive, &en()).unwrap();
        let count: i64 = there
            .read(|tx| {
                tx.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))
                    .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn the_local_images_win_and_the_missing_ones_come() {
        let archive = export(&with_team(true), Subject::Team(uuid(TEAM))).unwrap();
        let here = with_team(false);
        let mine = picture([1, 2, 3, 255]);
        assets::store_team_logo(&here, TEAM, Some(&mine)).unwrap();
        import(&here, &archive, &en()).unwrap();
        let logo = assets::team_logo(&here, TEAM).unwrap().unwrap();
        assert_eq!(logo.data, assets::normalize_logo(&mine).unwrap().data);
        assert!(assets::person_photo(&here, PERSON).unwrap().is_some());
    }

    #[test]
    fn what_is_not_an_image_refuses_the_archive() {
        let archive = export(&with_team(true), Subject::Team(uuid(TEAM))).unwrap();
        let files: Vec<(String, Vec<u8>)> = unzip(&archive)
            .unwrap()
            .into_iter()
            .map(|(n, d)| match n.starts_with("assets/") {
                true => (n, b"not an image".to_vec()),
                false => (n, d),
            })
            .collect();
        let bad = zip_files(&files, Deflated).unwrap();
        let there = database();
        assert!(import(&there, &bad, &en()).is_err());
        let teams: i64 = there
            .read(|tx| {
                tx.query_row("SELECT count(*) FROM teams", [], |r| r.get(0))
                    .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        assert_eq!(teams, 0);
    }
}
