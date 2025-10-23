use crate::{
    errors::{AppError, IOError},
    localization::current_labels,
    providers::fs::path::get_match_folder_path,
    screens::{file_system_screen::FileSystemAction, screen::AppAction},
    shapes::team::TeamEntry,
};
use async_trait::async_trait;
use hf::is_hidden;
use std::{fs, io::Write, path::PathBuf};
use std::{fs::File, path::Path};
use zip::write::FileOptions;
use zip::ZipWriter;

pub struct ExportMatchAction {
    team: TeamEntry,
    match_id: String,
    base_path: PathBuf,
    exported_file_path: Option<PathBuf>,
}

impl ExportMatchAction {
    pub fn new(team: TeamEntry, match_id: String, base_path: PathBuf) -> Self {
        Self {
            team,
            match_id,
            base_path,
            exported_file_path: None,
        }
    }
}

#[async_trait]
impl FileSystemAction for ExportMatchAction {
    fn is_selectable(&self, path: &Path) -> bool {
        !is_hidden(path).unwrap_or_default() && path.is_dir()
    }

    fn is_visible(&self, path: &Path) -> bool {
        !is_hidden(path).unwrap_or_default() && path.is_dir()
    }

    fn success_message_suffix(&self) -> Option<String> {
        self.exported_file_path
            .as_ref()
            .map(|p| p.display().to_string())
    }

    async fn on_selected(&mut self, path: &Path) -> Result<AppAction, AppError> {
        let match_folder_path =
            get_match_folder_path(&self.base_path, &self.team.id, &self.match_id)?;
        let zip_file_path = path.join(format!("{}.zip", self.match_id));
        if zip_file_path.exists() {
            return Err(AppError::IO(IOError::Msg(
                current_labels().file_already_exists.to_string(),
            )));
        }
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
        self.exported_file_path = Some(zip_file_path);
        Ok(AppAction::Back(true, Some(1)))
    }
}
