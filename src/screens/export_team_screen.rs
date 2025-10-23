use crate::{
    constants::TEAM_DESCRIPTOR_FILE_NAME,
    errors::{AppError, IOError},
    localization::current_labels,
    providers::fs::path::get_team_folder_path,
    screens::{file_system_screen::FileSystemAction, screen::AppAction},
};
use async_trait::async_trait;
use hf::is_hidden;
use std::{fs::File, io::Read, path::Path};
use std::{io::Write, path::PathBuf};
use uuid::Uuid;
use zip::write::FileOptions;
use zip::CompressionMethod::*;
use zip::ZipWriter;

pub struct ExportTeamAction {
    team_id: Uuid,
    base_path: PathBuf,
    exported_file_path: Option<PathBuf>,
}

impl ExportTeamAction {
    pub fn new(team_id: Uuid, base_path: PathBuf) -> Self {
        Self {
            team_id,
            base_path,
            exported_file_path: None,
        }
    }
}

#[async_trait]
impl FileSystemAction for ExportTeamAction {
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
        let team_descriptor_file_path =
            get_team_folder_path(&self.base_path, &self.team_id)?.join(TEAM_DESCRIPTOR_FILE_NAME);
        let zip_file_path = path.join(format!("{}.zip", &self.team_id));
        if zip_file_path.exists() {
            return Err(AppError::IO(IOError::Msg(
                current_labels().file_already_exists.to_string(),
            )));
        }
        let file = File::create(&zip_file_path).map_err(|e| AppError::IO(IOError::from(e)))?;
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
        self.exported_file_path = Some(zip_file_path);
        Ok(AppAction::Back(true, Some(1)))
    }
}
