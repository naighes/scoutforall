use crate::{
    constants::MATCH_DESCRIPTOR_FILE_NAME,
    errors::{AppError, IOError},
    localization::current_labels,
    providers::{match_reader::MatchReader, match_writer::MatchWriter, set_writer::SetWriter},
    screens::{file_system_screen::FileSystemAction, screen::AppAction},
    shapes::{r#match::MatchEntry, set::SetEntry, snapshot::EventEntry, team::TeamEntry},
};
use async_trait::async_trait;
use csv::ReaderBuilder;
use hf::is_hidden;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::Path,
    sync::Arc,
};
use zip::ZipArchive;

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

type SetMap = BTreeMap<
    u8,
    (
        Option<(String, SetEntry)>,
        Option<(String, Vec<EventEntry>)>,
    ),
>;

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
        // selected file name
        let match_id = path.file_stem().and_then(|s| s.to_str()).ok_or_else(|| {
            AppError::IO(IOError::Msg(current_labels().invalid_file_name.to_string()))
        })?;
        // check if the match already exists
        let exists = self.match_reader.exists(&self.team, match_id).await?;
        if exists {
            return Err(AppError::IO(IOError::Msg(
                current_labels().match_already_exists.to_string(),
            )));
        }
        let match_entry = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or(AppError::IO(IOError::Msg(
                current_labels().invalid_file_name.to_string(),
            )))
            .and_then(|match_id| {
                fs::File::open(path)
                    .map_err(|e| AppError::IO(IOError::from(e)))
                    .map(|f| (match_id, f))
            })
            .and_then(|(match_id, mut f)| Self::archive_content(&mut f, &self.team, match_id))?;
        // save match entry
        let m = match_entry.clone();
        self.match_writer
            .create(
                &match_entry.team,
                match_entry.opponent,
                match_entry.date,
                match_entry.home,
            )
            .await?;
        for set in match_entry.sets {
            self.set_writer
                .create(
                    &m,
                    set.set_number,
                    set.serving_team,
                    set.initial_positions,
                    set.libero,
                    set.fallback_libero,
                    set.setter,
                    set.events,
                )
                .await?;
        }
        Ok(AppAction::Back(true, Some(1)))
    }
}

impl<
        MR: MatchReader + Send + Sync,
        MW: MatchWriter + Send + Sync,
        SSW: SetWriter + Send + Sync,
    > ImportMatchAction<MR, MW, SSW>
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

    fn archive_content(
        file: &mut File,
        team: &TeamEntry,
        match_id: &str,
    ) -> Result<MatchEntry, AppError> {
        let mut archive = ZipArchive::new(file).map_err(|e| AppError::IO(IOError::from(e)))?;
        let mut set_map: SetMap = BTreeMap::new();
        let mut match_entry_content = String::new();
        // extract match entry and sets
        for i in 0..archive.len() {
            let (name, content) = Self::read_file_content(&mut archive, i)?;
            if name.eq_ignore_ascii_case(MATCH_DESCRIPTOR_FILE_NAME) {
                match_entry_content = content;
            } else if let Some(caps) = name.strip_prefix("set_") {
                if let Some((num_str, ext)) = caps.split_once('.') {
                    if let Ok(num) = num_str.parse::<u8>() {
                        let entry = set_map.entry(num).or_insert((None, None));
                        match ext {
                            "json" => entry.0 = Self::parse_set_json(num, content),
                            "csv" => entry.1 = Self::parse_set_csv(content),
                            _ => {}
                        }
                    }
                }
            }
        }
        let len = set_map.len();
        // validation
        let mut match_entry = Self::parse_match_entry(&match_entry_content, team, match_id)?;
        // add sets to match entry
        for (_, (set_opt, evt_opt)) in set_map {
            if let (Some((_, mut entry)), Some((_, events))) = (set_opt, evt_opt) {
                entry.events = events.clone();
                match_entry.sets.push(entry);
            }
        }
        if match_entry.sets.len() != len {
            return Err(AppError::IO(IOError::Msg(
                current_labels().import_match_error.to_string(),
            )));
        }
        Ok(match_entry)
    }

    fn parse_match_entry(
        content: &str,
        team: &TeamEntry,
        match_id: &str,
    ) -> Result<MatchEntry, AppError> {
        let mut entry: MatchEntry = serde_json::from_str(content).map_err(|_| {
            AppError::IO(IOError::Msg(
                current_labels().import_match_error.to_string(),
            ))
        })?;
        entry.id = match_id.to_string();
        entry.team = team.clone();
        Ok(entry)
    }
}
