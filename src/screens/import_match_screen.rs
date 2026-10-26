use crate::localization::current_labels;
use crate::screens::{file_system_screen::FileSystemAction, screen::AppAction};
use async_trait::async_trait;
use hf::is_hidden;
use scout4all_core::{
    archive::import_match,
    errors::{AppError, IOError},
    providers::{match_reader::MatchReader, match_writer::MatchWriter, set_writer::SetWriter},
    shapes::team::TeamEntry,
};
use std::{fs, path::Path, sync::Arc};

pub struct ImportMatchAction<
    MR: MatchReader + Send + Sync,
    MW: MatchWriter + Send + Sync,
    SSW: SetWriter + Send + Sync,
> {
    team: TeamEntry,
    match_reader: Arc<MR>,
    match_writer: Arc<MW>,
    set_writer: Arc<SSW>,
}

#[async_trait]
impl<
    MR: MatchReader + Send + Sync + 'static,
    MW: MatchWriter + Send + Sync + 'static,
    SSW: SetWriter + Send + Sync + 'static,
> FileSystemAction for ImportMatchAction<MR, MW, SSW>
{
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

    async fn on_selected(&mut self, path: &Path) -> Result<AppAction, AppError> {
        let archive = fs::read(path).map_err(|e| AppError::IO(IOError::from(e)))?;
        import_match(
            &archive,
            &self.team,
            &*self.match_reader,
            &*self.match_writer,
            &*self.set_writer,
            current_labels().domain,
        )
        .await?;
        Ok(AppAction::Back(true, Some(1)))
    }
}

impl<MR: MatchReader + Send + Sync, MW: MatchWriter + Send + Sync, SSW: SetWriter + Send + Sync>
    ImportMatchAction<MR, MW, SSW>
{
    pub fn new(
        team: TeamEntry,
        match_reader: Arc<MR>,
        match_writer: Arc<MW>,
        set_writer: Arc<SSW>,
    ) -> Self {
        Self {
            team,
            match_reader,
            match_writer,
            set_writer,
        }
    }
}
