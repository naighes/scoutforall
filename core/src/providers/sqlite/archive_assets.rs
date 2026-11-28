//! The images in the archives (TODO/011 §4): apart from the JSON, one file
//! each under `assets/` and an index, `assets.json`, that tells whose they
//! are. The rows of `teams` and `persons` never carry the columns that
//! point to them.
//!
//! Importing, the image of a team or a person is taken only when the one
//! here has none: the local data win.

use crate::{
    assets::{self, AssetKind},
    errors::{AppError, IOError},
    localization::Labels,
    providers::sqlite::{
        archive::{Row, invalid},
        sqlite,
    },
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

pub(crate) const INDEX: &str = "assets.json";

/// The tables whose rows point to an image: the column, the kind.
const LINKS: [(&str, &str, AssetKind); 2] = [
    ("teams", "logo_asset_id", AssetKind::TeamLogo),
    ("persons", "photo_asset_id", AssetKind::PersonPhoto),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    id: String,
    /// `team_logo` or `person_photo`
    kind: String,
    /// the id of the team or of the person
    owner: String,
    mime: String,
    file: String,
}

fn extension(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        _ => "jpg",
    }
}

fn link(table: &str) -> Option<(&'static str, AssetKind)> {
    LINKS
        .iter()
        .find(|(t, _, _)| *t == table)
        .map(|(_, column, kind)| (*column, *kind))
}

fn text(row: &Row, column: &str) -> Option<String> {
    row.get(column)
        .and_then(Json::as_str)
        .map(ToString::to_string)
}

/// The row without the columns that point to an image.
pub(crate) fn strip(row: &mut Row) {
    LINKS.iter().for_each(|(_, column, _)| {
        row.remove(*column);
    });
}

/// The files of the images that the rows point to, with their index first
/// (none when there are no images); the rows lose the pointers.
pub(crate) fn gather(
    tx: &Connection,
    tables: &mut [(&str, Vec<Row>)],
) -> Result<Vec<(String, Vec<u8>)>, AppError> {
    let mut entries = Vec::new();
    let mut files = Vec::new();
    for (table, rows) in tables.iter_mut() {
        let link = link(table);
        for row in rows.iter_mut() {
            let found = link.and_then(|(column, kind)| {
                text(row, column).zip(text(row, "id")).map(|f| (f, kind))
            });
            if let Some(((id, owner), kind)) = found
                && let Some(image) = assets::load(tx, &id)?
            {
                let file = format!("assets/{id}.{}", extension(image.mime));
                entries.push(Entry {
                    id,
                    kind: match kind {
                        AssetKind::TeamLogo => "team_logo",
                        AssetKind::PersonPhoto => "person_photo",
                    }
                    .to_string(),
                    owner,
                    mime: image.mime.to_string(),
                    file: file.clone(),
                });
                files.push((file, image.data));
            }
            strip(row);
        }
    }
    if entries.is_empty() {
        return Ok(vec![]);
    }
    let index = serde_json::to_vec_pretty(&entries).map_err(|e| AppError::IO(IOError::from(e)))?;
    files.insert(0, (INDEX.to_string(), index));
    Ok(files)
}

/// The images of the archive in `staging` (of today's schema), each
/// pointed to by its team or person there. Not valid: an index that
/// doesn't read, a missing file, bytes that aren't an image.
pub(crate) fn stage(
    staging: &Connection,
    files: &[(String, Vec<u8>)],
    labels: &Labels,
) -> Result<(), AppError> {
    let Some((_, index)) = files.iter().find(|(name, _)| name == INDEX) else {
        return Ok(());
    };
    let entries: Vec<Entry> = serde_json::from_slice(index).map_err(|e| invalid(labels, e))?;
    entries.iter().try_for_each(|entry| {
        let table = match entry.kind.as_str() {
            "team_logo" => Ok("teams"),
            "person_photo" => Ok("persons"),
            other => Err(invalid(labels, format!("{INDEX}: kind {other:?}"))),
        }?;
        let column = link(table).map_or("", |(column, _)| column);
        let data = files
            .iter()
            .find(|(name, _)| *name == entry.file)
            .map(|(_, data)| data)
            .ok_or_else(|| invalid(labels, &entry.file))?;
        let decoded = assets::decode_image(data).map_err(|e| invalid(labels, e))?;
        let mime = match entry.mime.as_str() {
            "image/png" | "image/jpeg" => Ok(entry.mime.as_str()),
            other => Err(invalid(labels, format!("{INDEX}: mime {other:?}"))),
        }?;
        staging
            .execute(
                "INSERT INTO assets (id, kind, mime, width, height, data)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    entry.id,
                    entry.kind,
                    mime,
                    decoded.width(),
                    decoded.height(),
                    data
                ],
            )
            .map_err(|e| invalid(labels, e))?;
        staging
            .execute(
                &format!("UPDATE {table} SET {column} = ?1 WHERE id = ?2"),
                params![entry.id, entry.owner],
            )
            .map(|_| ())
            .map_err(|e| invalid(labels, e))
    })
}

/// The pointers of the rows read from `staging`, taken off them: (table,
/// owner, image).
pub(crate) fn links(tables: &mut [(&str, Vec<Row>)]) -> Vec<(String, String, String)> {
    tables
        .iter_mut()
        .flat_map(|(table, rows)| {
            let column = link(table).map(|(column, _)| column);
            rows.iter_mut()
                .filter_map(|row| {
                    let found = column
                        .and_then(|c| text(row, c))
                        .zip(text(row, "id"))
                        .map(|(image, owner)| (table.to_string(), owner, image));
                    strip(row);
                    found
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Gives the images of the archive to the teams and persons here that
/// have none.
pub(crate) fn adopt(
    tx: &Connection,
    staging: &Connection,
    links: &[(String, String, String)],
) -> Result<(), AppError> {
    links.iter().try_for_each(|(table, owner, image)| {
        let Some((column, kind)) = link(table) else {
            return Ok(());
        };
        let here: Option<Option<String>> = tx
            .query_row(
                &format!("SELECT {column} FROM {table} WHERE id = ?1"),
                [owner],
                |row| row.get(0),
            )
            .optional()
            .map_err(sqlite)?;
        match (here, assets::load(staging, image)?) {
            (Some(None), Some(found)) => {
                let id = assets::insert(tx, kind, &found)?;
                tx.execute(
                    &format!("UPDATE {table} SET {column} = ?1 WHERE id = ?2"),
                    params![id, owner],
                )
                .map(|_| ())
                .map_err(sqlite)
            }
            _ => Ok(()),
        }
    })
}
