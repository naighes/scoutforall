use crate::{
    constants::MATCH_DESCRIPTOR_FILE_NAME,
    errors::{AppError, IOError},
    io::path::get_match_folder_path,
    localization::current_labels,
    screens::{file_system_screen::FileSystemAction, screen::AppAction},
    shapes::{r#match::MatchEntry, set::SetEntry, snapshot::EventEntry, team::TeamEntry},
};
use csv::ReaderBuilder;
use hf::is_hidden;
use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};
use uuid::Uuid;
use zip::ZipArchive;

pub struct ImportMatchAction {
    team: TeamEntry,
}

type SetMap = BTreeMap<
    u8,
    (
        Option<(String, SetEntry)>,
        Option<(String, Vec<EventEntry>)>,
    ),
>;

impl FileSystemAction for ImportMatchAction {
    fn is_selectable(&self, path: &Path) -> bool {
        !is_hidden(path).unwrap_or_default()
            && path
                .extension()
                .and_then(|s| s.to_str())
                .map(|e| e.eq_ignore_ascii_case("zip"))
                .unwrap_or(false)
    }

    fn is_visible(&self, path: &Path) -> bool {
        !is_hidden(path).unwrap_or_default()
            && (path.is_dir()
                || path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|e| e.eq_ignore_ascii_case("zip"))
                    .unwrap_or(false))
    }

    fn on_selected(&mut self, path: &Path) -> Result<AppAction, AppError> {
        path.file_stem()
            .and_then(|s| s.to_str())
            .ok_or(AppError::IO(IOError::Msg(
                current_labels().invalid_file_name.to_string(),
            )))
            .and_then(|folder_name| {
                let dest_folder = get_match_folder_path(&self.team.id, folder_name)?;
                if dest_folder.exists() {
                    Err(AppError::IO(IOError::Msg(
                        current_labels().match_already_exists.to_string(),
                    )))
                } else {
                    Ok(dest_folder)
                }
            })
            .and_then(|dest_folder| {
                fs::File::open(path)
                    .map_err(|e| AppError::IO(IOError::from(e)))
                    .map(|f| (dest_folder, f))
            })
            .and_then(|(dest_folder, mut f)| {
                ImportMatchAction::archive_content(&dest_folder, &mut f)
            })?;
        Ok(AppAction::Back(true, Some(1)))
    }
}

impl ImportMatchAction {
    pub fn new(team: TeamEntry) -> Self {
        Self { team }
    }

    fn parse_set_json(set_number: u8, content: String) -> Option<(String, SetEntry)> {
        serde_json::from_str(&content)
            .ok()
            .map(|mut entry: SetEntry| {
                entry.set_number = set_number;
                (content, entry)
            })
    }

    fn parse_set_csv(content: String) -> Option<(String, Vec<EventEntry>)> {
        let mut rdr = ReaderBuilder::new()
            .has_headers(false)
            .from_reader(content.as_bytes());
        let mut events = Vec::new();
        for result in rdr.deserialize() {
            let event: Result<_, AppError> = result.map_err(|_| {
                AppError::IO(IOError::Msg(
                    current_labels().import_match_error.to_string(),
                ))
            });
            match event {
                Err(_) => return None,
                Ok(ev) => events.push(ev),
            }
        }
        Some((content, events))
    }

    fn read_file_content(
        archive: &mut ZipArchive<&mut File>,
        i: usize,
    ) -> Result<(String, String), AppError> {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        let name = entry.name().to_string();
        let mut content = String::new();
        entry
            .read_to_string(&mut content)
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        Ok((name, content))
    }

    fn write_file_to_tmp_folder(
        tmp_folder: &Path,
        name: &str,
        content: &str,
    ) -> Result<(), AppError> {
        let dest_path = tmp_folder.join(name);
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::IO(IOError::from(e)))?;
        }
        fs::write(&dest_path, content).map_err(|e| AppError::IO(IOError::from(e)))?;
        Ok(())
    }

    fn archive_content(dest_folder: &Path, file: &mut File) -> Result<(), AppError> {
        let mut archive = ZipArchive::new(file).map_err(|e| AppError::IO(IOError::from(e)))?;
        let mut tmp_folder: PathBuf = env::temp_dir();
        tmp_folder.push(format!("tmp-{}", Uuid::new_v4()));
        fs::create_dir_all(&tmp_folder).map_err(|e| AppError::IO(IOError::from(e)))?;
        let mut set_map: SetMap = BTreeMap::new();
        let mut match_entry_content = String::new();
        // extract in memory and write on temp folder
        for i in 0..archive.len() {
            let (name, content) = Self::read_file_content(&mut archive, i)?;
            // write the file to the temp folder
            Self::write_file_to_tmp_folder(&tmp_folder, &name, &content)?;
            if name.eq_ignore_ascii_case(MATCH_DESCRIPTOR_FILE_NAME) {
                match_entry_content = content;
            } else if let Some(caps) = name.strip_prefix("set_") {
                if let Some((num_str, ext)) = caps.split_once('.') {
                    if let Ok(num) = num_str.parse::<u8>() {
                        let entry = set_map.entry(num).or_insert((None, None));
                        match ext {
                            "json" => entry.0 = ImportMatchAction::parse_set_json(num, content),
                            "csv" => entry.1 = ImportMatchAction::parse_set_csv(content),
                            _ => {}
                        }
                    }
                }
            }
        }
        let len = set_map.len();
        // validation
        ImportMatchAction::parse_match_entry(&match_entry_content)?;
        ImportMatchAction::validate_sets(&set_map).inspect_err(|_| {
            let _ = fs::remove_dir_all(&tmp_folder);
        })?;
        // add sets
        let mut sets = Vec::new();
        for (_, (set_opt, evt_opt)) in set_map {
            if let (Some((_, mut entry)), Some((_, events))) = (set_opt, evt_opt) {
                entry.events = events.clone();
                sets.push(entry);
            }
        }
        if sets.len() != len {
            let _ = fs::remove_dir_all(&tmp_folder);
            return Err(AppError::IO(IOError::Msg(
                current_labels().import_match_error.to_string(),
            )));
        }
        // rename
        fs::rename(&tmp_folder, dest_folder).map_err(|e| {
            // cleanup on error
            let _ = fs::remove_dir_all(&tmp_folder);
            AppError::IO(IOError::from(e))
        })?;
        Ok(())
    }

    fn parse_match_entry(content: &str) -> Result<MatchEntry, AppError> {
        serde_json::from_str(content).map_err(|_| {
            AppError::IO(IOError::Msg(
                current_labels().import_match_error.to_string(),
            ))
        })
    }

    fn validate_sets(set_map: &SetMap) -> Result<(), AppError> {
        if set_map.len() < 3 || set_map.len() > 5 {
            return Err(AppError::IO(IOError::Msg(
                current_labels().import_match_error.to_string(),
            )));
        }
        Ok(())
    }
}
