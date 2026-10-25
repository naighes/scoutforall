use crate::{
    analytics::{global::uploads_enabled, queue_manager::QueueManager},
    errors::{AppError, IOError},
    logging::logger::{log_error, log_info},
    providers::{
        match_reader::MatchReader, queue_reader::QueueReader, queue_writer::QueueWriter,
        team_reader::TeamReader,
    },
    shapes::{
        enums::{GenderEnum, TeamClassificationEnum},
        r#match::MatchEntry,
    },
    storage::{Store, layout},
    util::config_var,
};
use futures::TryFutureExt;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;
use std::{
    io::{Cursor, Write},
    sync::Arc,
    time::Duration,
};
use tokio::time::interval;
use tokio::{spawn, task::JoinHandle};
use uuid::Uuid;
use zip::{CompressionMethod, ZipWriter, write::FileOptions};

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
    pub max_substitutions: u8,
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

// the render free plan stops the API when idle: the first request can wait
// for it to start again. The user agent tells the app version in the API log
fn http_client() -> Result<Client, AppError> {
    Client::builder()
        .user_agent(concat!("scout4all/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::IO(IOError::from(e)))
}

// the query string of a signed URL holds its token: keep it out of the log
pub(crate) fn without_token(signed_url: &str) -> &str {
    signed_url.split('?').next().unwrap_or(signed_url)
}

// upload the archive to the signed URL using PUT
async fn upload_to_signed_url(signed_url: &str, archive: Vec<u8>) -> Result<(), AppError> {
    let response = http_client()?
        .put(signed_url)
        .header("Content-Type", "application/zip")
        .body(archive)
        .send()
        .await
        .map_err(|e| AppError::IO(IOError::from(e)))?;
    if response.status().is_success() {
        log_info(
            format!(
                "uploaded file to signed URL '{}'",
                without_token(signed_url)
            )
            .as_str(),
        );
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
pub(crate) async fn create_match_archive(
    m: &MatchEntry,
    store: &dyn Store,
) -> Result<Vec<u8>, AppError> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let metadata = MatchMetadata {
        date: m.date.to_string(),
        home: m.home,
        team_id: m.team.id,
        gender: m.team.gender,
        year: m.team.year,
        classification: m.team.classification,
        max_substitutions: m.max_substitutions,
    };
    let metadata_json = to_string_pretty(&metadata).map_err(|e| AppError::IO(IOError::from(e)))?;
    let set_files = m.sets.iter().flat_map(|set| {
        [
            (
                layout::set_descriptor(&m.team.id, &m.id, set.set_number),
                format!("set_{}_descriptor.json", set.set_number),
            ),
            (
                layout::set_events(&m.team.id, &m.id, set.set_number),
                format!("set_{}_events.csv", set.set_number),
            ),
        ]
    });
    let mut files = vec![("match.json".to_string(), metadata_json.into_bytes())];
    for (key, name) in set_files {
        if let Some(contents) = store.read(&key).await? {
            files.push((name, contents));
        }
    }
    for (name, contents) in files {
        zip.start_file::<_, ()>(
            name,
            FileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .map_err(|e| AppError::IO(IOError::from(e)))?;
        zip.write_all(&contents)
            .map_err(|e| AppError::IO(IOError::from(e)))?;
    }
    zip.finish()
        .map(Cursor::into_inner)
        .map_err(|e| AppError::IO(IOError::from(e)))
}

/// The URL of the API that signs the uploads: from the environment, or the
/// one given when the program was built.
pub fn upload_url() -> Option<String> {
    config_var("ANALYTICS_UPLOAD_URL", option_env!("ANALYTICS_UPLOAD_URL"))
}

// retrieve signed URL for sending analytics data
async fn get_signed_url(m: &MatchEntry, upload_url: &str) -> Result<(String, Uuid), AppError> {
    let client = http_client()?;
    let uuid = generate_upload_id(m.team.id, m.id.as_str());
    let payload = UploadRequest {
        filename: format!("{}.zip", uuid),
    };
    let response = client
        .post(upload_url)
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
                without_token(&signed_url),
                uuid
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
    store: Arc<dyn Store>,
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
        store: Arc<dyn Store>,
        team_reader: Arc<TR>,
        match_reader: Arc<MR>,
        queue_reader: Arc<QR>,
        queue_writer: Arc<QW>,
        polling_interval: Duration,
    ) -> Self {
        let queue_manager = Arc::new(QueueManager::new(queue_reader, queue_writer));
        Self {
            store,
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
        let store = Arc::clone(&self.store);
        let polling_interval = self.polling_interval;
        let team_reader = self.team_reader.clone();
        let match_reader = self.match_reader.clone();
        let queue_manager = self.queue_manager.clone();

        spawn(async move {
            let mut ticker = interval(polling_interval);
            loop {
                ticker.tick().await;
                if let (true, Some(url)) = (uploads_enabled(), upload_url()) {
                    let _ =
                        upload_next(&*store, &*team_reader, &*match_reader, &queue_manager, &url)
                            .await;
                }
            }
        })
    }
}

/// What an attempt at the first upload of the queue did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadStep {
    /// nothing to upload
    Empty,
    /// sent, and out of the queue
    Uploaded,
    /// not sent: to the back of the queue, to try again later
    Failed,
    /// the match can't be read any more (e.g. removed): out of the queue
    Dropped,
}

/// Tries once the first upload of the queue, through the API at
/// `upload_url`. Whoever calls it decides when: the TUI every few seconds,
/// the phone when there's a network.
pub async fn upload_next<QR: QueueReader + Send + Sync, QW: QueueWriter + Send + Sync>(
    store: &dyn Store,
    team_reader: &(impl TeamReader + Sync + ?Sized),
    match_reader: &(impl MatchReader + Sync + ?Sized),
    queue_manager: &QueueManager<QR, QW>,
    upload_url: &str,
) -> Result<UploadStep, AppError> {
    let queue = queue_manager.load().await?;
    let Some(pending) = queue.peek().cloned() else {
        return Ok(UploadStep::Empty);
    };
    let read = async {
        let team = team_reader.read_single(&pending.team_id).await?;
        match_reader.read_single(&team, &pending.match_id).await
    };
    match read.await {
        Ok(m) => match process_single_upload(&m, store, upload_url).await {
            Ok(()) => {
                queue_manager
                    .dequeue(&pending.team_id, &pending.match_id)
                    .await?;
                Ok(UploadStep::Uploaded)
            }
            Err(_) => {
                // kept in the queue indefinitely
                queue_manager
                    .mark_retry(&pending.team_id, &pending.match_id)
                    .await?;
                Ok(UploadStep::Failed)
            }
        },
        Err(_) => {
            queue_manager
                .dequeue(&pending.team_id, &pending.match_id)
                .await?;
            Ok(UploadStep::Dropped)
        }
    }
}

// process a single upload
async fn process_single_upload(
    m: &MatchEntry,
    store: &dyn Store,
    upload_url: &str,
) -> Result<(), AppError> {
    let (signed_url, _) = get_signed_url(m, upload_url).await?;
    upload_to_signed_url(&signed_url, create_match_archive(m, store).await?).await
}
