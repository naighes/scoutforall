use crate::{
    errors::{AppError, IOError},
    io::path::get_match_folder_path,
    screens::{file_system_screen::FileSystemAction, screen::AppAction},
    shapes::team::TeamEntry,
};
use hf::is_hidden;
use std::{fs, io::Write};
use std::{fs::File, path::Path};
use zip::write::FileOptions;
use zip::ZipWriter;

pub struct ExportMatchAction {
    team: TeamEntry,
    match_id: String,
}

impl ExportMatchAction {
    pub fn new(team: TeamEntry, match_id: String) -> Self {
        Self { team, match_id }
    }
}

impl FileSystemAction for ExportMatchAction {
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
        let match_folder_path = get_match_folder_path(&self.team.id, &self.match_id)?;
        let zip_file_path = path.join(format!("{}.zip", self.match_id));
        let file = File::create(&zip_file_path).map_err(|e| AppError::IO(IOError::from(e)))?;
        let mut zip = ZipWriter::new(file);
        for entry in fs::read_dir(&match_folder_path).map_err(|e| AppError::IO(IOError::from(e)))? {
            let entry = entry.map_err(|e| AppError::IO(IOError::from(e)))?;
            let path = entry.path();
            if path.is_file() {
                let file_name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| AppError::IO(IOError::Msg("invalid file name".into())))?;
                let data = fs::read(&path).map_err(|e| AppError::IO(IOError::from(e)))?;

                zip.start_file::<_, ()>(
                    file_name,
                    FileOptions::default().compression_method(zip::CompressionMethod::Deflated),
                )
                .map_err(|e| AppError::IO(IOError::from(e)))?;
                zip.write_all(&data)
                    .map_err(|e| AppError::IO(IOError::from(e)))?;
            }
        }

        zip.finish().map_err(|e| AppError::IO(IOError::from(e)))?;
        Ok(AppAction::Back(true, Some(1)))
    }
}
