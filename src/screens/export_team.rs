use crate::{
    constants::TEAM_DESCRIPTOR_FILE_NAME,
    errors::{AppError, IOError},
    io::path::get_team_folder_path,
    screens::{file_system_screen::FileSystemAction, screen::AppAction},
};
use hf::is_hidden;
use std::io::Write;
use std::{fs::File, io::Read, path::Path};
use uuid::Uuid;
use zip::write::FileOptions;
use zip::CompressionMethod::*;
use zip::ZipWriter;

pub struct ExportTeamAction {
    team_id: Uuid,
}

impl ExportTeamAction {
    pub fn new(team_id: Uuid) -> Self {
        Self { team_id }
    }
}

impl FileSystemAction for ExportTeamAction {
    fn is_selectable(&self, path: &Path) -> bool {
        !is_hidden(path).unwrap_or_default() && path.is_dir()
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
        let team_descriptor_file_path =
            get_team_folder_path(&self.team_id)?.join(TEAM_DESCRIPTOR_FILE_NAME);
        let file = File::create(path.join(format!("{}.zip", &self.team_id)))
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        let mut zip = ZipWriter::new(file);
        let mut team_file =
            File::open(&team_descriptor_file_path).map_err(|e| AppError::IO(IOError::from(e)))?;
        let mut team_json = String::new();
        team_file
            .read_to_string(&mut team_json)
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        zip.start_file::<_, ()>(
            TEAM_DESCRIPTOR_FILE_NAME,
            FileOptions::default().compression_method(Stored),
        )
        .map_err(|e| AppError::IO(IOError::from(e)))?;
        zip.write_all(team_json.as_bytes())
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        zip.finish().map_err(|e| AppError::IO(IOError::from(e)))?;
        Ok(AppAction::Back(true, Some(1)))
    }
}
