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
    const SESSION: &str = "55555555-5555-5555-5555-555555555555";

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

    /// The archive with its JSON files changed.
    fn rewritten(archive: &[u8], change: impl Fn(&str, &mut Json)) -> Vec<u8> {
        let files: Vec<(String, Vec<u8>)> = unzip(archive)
            .unwrap()
            .into_iter()
            .map(|(name, data)| match name.ends_with(".json") {
                true => {
                    let mut json: Json = serde_json::from_slice(&data).unwrap();
                    change(&name, &mut json);
                    (name, serde_json::to_vec(&json).unwrap())
                }
                false => (name, data),
            })
            .collect();
        zip_files(&files, Deflated).unwrap()
    }

    fn each_row(json: &mut Json, change: impl Fn(&mut serde_json::Map<String, Json>)) {
        json.as_array_mut()
            .into_iter()
            .flatten()
            .filter_map(Json::as_object_mut)
            .for_each(change);
    }

    /// As v1.3.0 wrote a team: schema 4 whatever the images (rows without
    /// the pointers), without the later columns.
    #[test]
    fn a_team_of_v1_3_0_with_images_is_imported() {
        let archive = export(&with_team(true), Subject::Team(uuid(TEAM))).unwrap();
        let old = rewritten(&archive, |name, json| match name {
            "manifest.json" => json["schema"] = Json::from(4),
            "persons.json" => each_row(json, |row| {
                ["nationality", "dominant_hand", "federation_id"]
                    .iter()
                    .for_each(|c| {
                        row.remove(*c);
                    })
            }),
            // no table of the gyms in v4
            "venues.json" => assert_eq!(json, &Json::Array(vec![])),
            _ => {}
        });
        let there = database();
        import(&there, &old, &en()).unwrap();
        assert!(assets::team_logo(&there, TEAM).unwrap().is_some());
        assert!(assets::person_photo(&there, PERSON).unwrap().is_some());
    }

    /// A team with images and a session the player attended.
    fn trained() -> Database {
        let db = with_team(true);
        execute(
            &db,
            &format!(
                "INSERT INTO training_sessions (id, team_id, start_local, duration_min, status)
                     VALUES ('{SESSION}', '{TEAM}', '2026-11-10T18:00', 90, 'completed');
                 INSERT INTO training_attendance (session_id, person_id, status)
                     VALUES ('{SESSION}', '{PERSON}', 'present');"
            ),
        );
        db
    }

    #[test]
    fn the_training_rows_do_not_point_to_the_images() {
        let archive = export(&trained(), Subject::Training(uuid(TEAM))).unwrap();
        unzip(&archive)
            .unwrap()
            .into_iter()
            .for_each(|(name, data)| {
                let text = String::from_utf8(data).unwrap();
                assert!(!text.contains("asset_id"), "{name}");
            });
        import(&with_team(false), &archive, &en()).unwrap();
    }

    /// v1.3.0 left the pointers in the rows of the training, without the
    /// images: the archive is imported all the same.
    #[test]
    fn a_training_of_v1_3_0_with_pointers_is_imported() {
        let archive = export(&trained(), Subject::Training(uuid(TEAM))).unwrap();
        let pointer = Json::from("44444444-4444-4444-4444-444444444444");
        let old = rewritten(&archive, |name, json| match name {
            "manifest.json" => json["schema"] = Json::from(7),
            "teams.json" => each_row(json, |row| {
                row.insert("logo_asset_id".into(), pointer.clone());
            }),
            "persons.json" => each_row(json, |row| {
                row.insert("photo_asset_id".into(), pointer.clone());
                ["nationality", "dominant_hand", "federation_id"]
                    .iter()
                    .for_each(|c| {
                        row.remove(*c);
                    })
            }),
            _ => {}
        });
        let here = with_team(false);
        let imported = import(&here, &old, &en()).unwrap();
        assert_eq!(imported.training.sessions.added, 1);
        assert!(assets::team_logo(&here, TEAM).unwrap().is_none());
    }

    #[tokio::test]
    async fn the_logo_goes_with_the_removed_team_and_the_photos_stay() {
        use crate::providers::{sqlite::teams::SqliteTeamWriter, team_writer::TeamWriter};
        let db = with_team(true);
        SqliteTeamWriter::new(db.clone())
            .remove(&uuid(TEAM))
            .await
            .unwrap();
        assert!(assets::team_logo(&db, TEAM).unwrap().is_none());
        assert!(assets::person_photo(&db, PERSON).unwrap().is_some());
        let rows: i64 = db
            .read(|tx| {
                tx.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))
                    .map_err(crate::providers::sqlite::sqlite)
            })
            .unwrap();
        assert_eq!(rows, 2);
    }
}
