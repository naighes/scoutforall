use crate::{
    analytics::queue_manager::QueueManager,
    errors::{AppError, IOError},
    logging::logger::{log_error, log_info},
    providers::{
        fs::path::{get_set_descriptor_file_path, get_set_events_file_path},
        match_reader::MatchReader,
        queue_reader::QueueReader,
        queue_writer::QueueWriter,
        team_reader::TeamReader,
    },
    shapes::{
        enums::{GenderEnum, TeamClassificationEnum},
        r#match::MatchEntry,
    },
};
use futures::TryFutureExt;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;
use std::{
    env::{temp_dir, var},
    fs::remove_file,
    io::Write,
    path::Path,
    time::Duration,
};
use std::{path::PathBuf, sync::Arc};
use tokio::time::interval;
use tokio::{fs::File, io::AsyncReadExt};
use tokio::{spawn, task::JoinHandle};
use uuid::Uuid;
use zip::{write::FileOptions, CompressionMethod, ZipWriter};

#[derive(Debug, Serialize)]
pub struct UploadRequest {
    pub filename: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MatchMetadata {
    pub date: String,
    pub home: bool,
    pub team_id: Uuid,
    pub gender: Option<GenderEnum>,
    pub year: u16,
    pub classification: Option<TeamClassificationEnum>,
}

// generate a deterministic UUID v5 from team_id and match_id
fn generate_upload_id(team_id: Uuid, match_id: &str) -> Uuid {
    // custom namespace for our application
    const NAMESPACE: Uuid = Uuid::from_bytes([
        0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30,
        0xc8,
    ]);
    let name = format!("{}:{}", team_id, match_id);
    Uuid::new_v5(&NAMESPACE, name.as_bytes())
}

// upload file to signed URL using PUT
async fn upload_file_to_signed_url(signed_url: &str, file_path: &Path) -> Result<(), AppError> {
    let mut file = File::open(file_path)
        .await
        .map_err(|e| AppError::IO(IOError::from(e)))?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)
        .await
        .map_err(|e| AppError::IO(IOError::from(e)))?;
    let client = Client::new();
    let response = client
        .put(signed_url)
        .header("Content-Type", "application/zip")
        .body(contents)
        .send()
        .await
        .map_err(|e| AppError::IO(IOError::from(e)))?;
    if response.status().is_success() {
        log_info(format!("uploaded file to signed URL '{}'", signed_url).as_str());
        Ok(())
    } else {
        let message = format!(
            "could not upload file: server returned unexpected status code ({})",
            response.status()
        );
        log_error(&message);
        Err(AppError::IO(IOError::Msg(message)))
    }
}

// create a ZIP archive with match data and set files
async fn create_match_archive(
    m: &MatchEntry,
    base_path: &Path,
    zip_path: &Path,
) -> Result<(), AppError> {
    let file = std::fs::File::create(zip_path).map_err(|e| AppError::IO(IOError::from(e)))?;
    let mut zip = ZipWriter::new(file);
    let metadata = MatchMetadata {
        date: m.date.to_string(),
        home: m.home,
        team_id: m.team.id,
        gender: m.team.gender,
        year: m.team.year,
        classification: m.team.classification,
    };
    let metadata_json = to_string_pretty(&metadata).map_err(|e| AppError::IO(IOError::from(e)))?;
    zip.start_file::<_, ()>(
        "match.json",
        FileOptions::default().compression_method(CompressionMethod::Deflated),
    )
    .map_err(|e| AppError::IO(IOError::from(e)))?;
    zip.write_all(metadata_json.as_bytes())
        .map_err(|e| AppError::IO(IOError::from(e)))?;
    // add set files
    for set in &m.sets {
        let set_descriptor_path =
            get_set_descriptor_file_path(base_path, &m.team.id, &m.id, set.set_number)?;
        let set_events_path =
            get_set_events_file_path(base_path, &m.team.id, &m.id, set.set_number)?;
        // descriptor
        if set_descriptor_path.exists() {
            let mut file = File::open(&set_descriptor_path)
                .await
                .map_err(|e| AppError::IO(IOError::from(e)))?;
            let mut contents = Vec::new();
            file.read_to_end(&mut contents)
                .await
                .map_err(|e| AppError::IO(IOError::from(e)))?;
            let filename = format!("set_{}_descriptor.json", set.set_number);
            zip.start_file::<_, ()>(
                &filename,
                FileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .map_err(|e| AppError::IO(IOError::from(e)))?;
            zip.write_all(&contents)
                .map_err(|e| AppError::IO(IOError::from(e)))?;
        }
        // events
        if set_events_path.exists() {
            let mut file = File::open(&set_events_path)
                .await
                .map_err(|e| AppError::IO(IOError::from(e)))?;
            let mut contents = Vec::new();
            file.read_to_end(&mut contents)
                .await
                .map_err(|e| AppError::IO(IOError::from(e)))?;
            let filename = format!("set_{}_events.csv", set.set_number);
            zip.start_file::<_, ()>(
                &filename,
                FileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .map_err(|e| AppError::IO(IOError::from(e)))?;
            zip.write_all(&contents)
                .map_err(|e| AppError::IO(IOError::from(e)))?;
        }
    }
    zip.finish().map_err(|e| AppError::IO(IOError::from(e)))?;
    Ok(())
}

// retrieve signed URL for sending analytics data
async fn get_signed_url(m: &MatchEntry) -> Result<(String, Uuid), AppError> {
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::IO(IOError::from(e)))?;
    let uuid = generate_upload_id(m.team.id, m.id.as_str());
    let payload = UploadRequest {
        filename: format!("{}.zip", uuid),
    };
    let analytics_upload_url = var("ANALYTICS_UPLOAD_URL")
        .map_err(|_| AppError::IO(IOError::Msg("could not get analytics upload url".into())))?;
    let response = client
        .post(analytics_upload_url)
        .json(&payload)
        .send()
        .map_err(|e| AppError::IO(IOError::from(e)))
        .await?;
    if response.status() != StatusCode::CREATED {
        let message = format!(
            "could not retrieve upload signed URL: unexpected status code ({})",
            response.status()
        );
        log_error(&message);
        Err(AppError::IO(IOError::Msg(message)))
    } else {
        let signed_url = response
            .headers()
            .get("Location")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::IO(IOError::Msg("missing Location header".into())))?
            .to_string();
        log_info(
            format!(
                "generated signed URL '{}' for file '{}.zip'",
                signed_url, uuid
            )
            .as_str(),
        );
        Ok((signed_url, uuid))
    }
}

pub struct AnalyticsUploadWorker<
    TR: TeamReader + Send + Sync + 'static,
    MR: MatchReader + Send + Sync + 'static,
    QR: QueueReader + Send + Sync + 'static,
    QW: QueueWriter + Send + Sync + 'static,
> {
    base_path: PathBuf,
    team_reader: Arc<TR>,
    match_reader: Arc<MR>,
    queue_manager: Arc<QueueManager<QR, QW>>,
    polling_interval: Duration,
}

impl<
        TR: TeamReader + Send + Sync + 'static,
        MR: MatchReader + Send + Sync + 'static,
        QR: QueueReader + Send + Sync + 'static,
        QW: QueueWriter + Send + Sync + 'static,
    > AnalyticsUploadWorker<TR, MR, QR, QW>
{
    pub fn new(
        base_path: PathBuf,
        team_reader: Arc<TR>,
        match_reader: Arc<MR>,
        queue_reader: Arc<QR>,
        queue_writer: Arc<QW>,
        polling_interval: Duration,
    ) -> Self {
        let queue_manager = Arc::new(QueueManager::new(queue_reader, queue_writer));
        Self {
            base_path,
            team_reader,
            match_reader,
            queue_manager,
            polling_interval,
        }
    }

    // get a reference to the queue manager (for enqueuing from other parts of the app)
    pub fn queue_manager(&self) -> Arc<QueueManager<QR, QW>> {
        Arc::clone(&self.queue_manager)
    }

    // start background worker that processes upload queue
    pub fn start(&self) -> JoinHandle<()> {
        let base_path = self.base_path.clone();
        let polling_interval = self.polling_interval;
        let team_reader = self.team_reader.clone();
        let match_reader = self.match_reader.clone();
        let queue_manager = self.queue_manager.clone();

        spawn(async move {
            let mut ticker = interval(polling_interval);
            loop {
                ticker.tick().await;
                let _ = async {
                    let queue = queue_manager.load().await.ok()?;
                    if queue.is_empty() {
                        return Some(());
                    }
                    // process first item
                    let pending = queue.peek().cloned()?;
                    match load_match_from_disk(
                        Arc::clone(&team_reader),
                        Arc::clone(&match_reader),
                        &pending.team_id,
                        &pending.match_id,
                    )
                    .await
                    {
                        Ok(match_entry) => {
                            // upload attempt
                            if process_single_upload(&match_entry, &base_path)
                                .await
                                .is_ok()
                            {
                                // success
                                let _ = queue_manager.dequeue(&pending.match_id).await;
                            } else {
                                // failure: just mark retry, keep in queue indefinitely
                                let _ = queue_manager.mark_retry(&pending.match_id).await;
                            }
                        }
                        Err(_) => {
                            // match not found
                            let _ = queue_manager.dequeue(&pending.match_id).await;
                        }
                    }
                    Some(())
                }
                .await;
            }
        })
    }
}

// process a single upload
async fn process_single_upload(m: &MatchEntry, base_path: &Path) -> Result<(), AppError> {
    let (signed_url, uuid) = get_signed_url(m).await?;
    let temp_dir = temp_dir();
    let zip_path = temp_dir.join(format!("{}.zip", uuid));
    create_match_archive(m, base_path, &zip_path).await?;
    let result = upload_file_to_signed_url(&signed_url, &zip_path).await;
    let _ = remove_file(&zip_path);
    result
}

// helper to load match from disk
async fn load_match_from_disk<
    TR: TeamReader + Send + Sync + 'static,
    MR: MatchReader + Send + Sync + 'static,
>(
    team_reader: Arc<TR>,
    match_reader: Arc<MR>,
    team_id: &Uuid,
    match_id: &str,
) -> Result<MatchEntry, AppError> {
    let team = team_reader.read_single(team_id).await?;
    match_reader.read_single(&team, match_id).await
}
