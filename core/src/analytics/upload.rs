use crate::{
    analytics::queue_manager::QueueManager,
    errors::{AppError, IOError},
    logging::logger::{log_error, log_info},
    providers::{queue_reader::QueueReader, queue_writer::QueueWriter},
    shapes::{
        country::CountryCode,
        enums::{GenderEnum, TeamClassificationEnum},
        r#match::{MatchEntry, MatchSource},
        snapshot::EventEntry,
    },
    util::config_var,
};
use async_trait::async_trait;
use csv::WriterBuilder;
use futures::TryFutureExt;
#[cfg(target_os = "android")]
use reqwest::Certificate;
use reqwest::{Client, ClientBuilder, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{to_string_pretty, to_vec_pretty};
use std::{
    io::{Cursor, Write},
    time::Duration,
};
use uuid::Uuid;
use zip::{CompressionMethod, ZipWriter, write::FileOptions};

#[derive(Debug, Serialize)]
pub struct UploadRequest {
    pub filename: String,
    /// for the keys of the match, when its team has gender and country
    #[serde(rename = "match", skip_serializing_if = "Option::is_none")]
    pub match_keys: Option<KeysRequest>,
}

/// What the API needs for the keys of a match: the names leave the device
/// for this only, and the API keeps nothing.
#[derive(Debug, Serialize)]
pub struct KeysRequest {
    pub team: KeysTeam,
    pub opponent: KeysOpponent,
    /// the day of the match, `YYYY-MM-DD`
    pub date: String,
    /// the score of each set, ours first
    pub sets: Vec<(u8, u8)>,
}

#[derive(Debug, Serialize)]
pub struct KeysTeam {
    pub name: String,
    pub country: String,
    pub gender: String,
    pub year: u16,
}

#[derive(Debug, Serialize)]
pub struct KeysOpponent {
    pub name: String,
}

/// The keys the API gives back: the same team, and the same match, have
/// the same keys among the uploads of different clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchKeys {
    pub team_key: String,
    pub opponent_key: String,
    pub match_key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MatchMetadata {
    pub date: String,
    pub home: bool,
    pub team_id: Uuid,
    pub gender: Option<GenderEnum>,
    pub year: u16,
    pub classification: Option<TeamClassificationEnum>,
    /// missing for the teams without one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<CountryCode>,
    pub max_substitutions: u8,
    /// since v0.12
    pub source: MatchSource,
    /// since v0.12, when the API gave them
    #[serde(default, flatten, skip_serializing_if = "Option::is_none")]
    pub keys: Option<MatchKeys>,
}

/// The request of the keys of `m`: none when its team lacks gender or
/// country, or a set doesn't replay.
pub(crate) fn keys_request(m: &MatchEntry) -> Option<KeysRequest> {
    let (country, gender) = (m.team.country?, m.team.gender?);
    let sets = m.set_scores().ok().filter(|s| !s.is_empty())?;
    Some(KeysRequest {
        team: KeysTeam {
            name: m.team.name.clone(),
            country: country.to_string(),
            gender: gender.to_string(),
            year: m.team.year,
        },
        opponent: KeysOpponent {
            name: m.opponent.clone(),
        },
        date: m.date.format("%Y-%m-%d").to_string(),
        sets,
    })
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
    tls_roots(Client::builder())
        .user_agent(concat!("scout4all/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::IO(IOError::from(e)))
}

// the verifier of the platform: on Android it needs the context of the app
// (JNI), and without it the first request aborts the process. The roots of
// Mozilla, built in, are enough for the API and the storage
#[cfg(target_os = "android")]
fn tls_roots(builder: ClientBuilder) -> ClientBuilder {
    builder.tls_certs_only(
        webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .filter_map(|der| Certificate::from_der(der).ok()),
    )
}

#[cfg(not(target_os = "android"))]
fn tls_roots(builder: ClientBuilder) -> ClientBuilder {
    builder
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

/// A match to upload, as its source keeps it.
pub struct ToUpload {
    pub entry: MatchEntry,
    /// the id in the name of its file: the same match keeps the same name
    /// (the folder of the match before the database, for the matches moved
    /// from there)
    pub file_id: String,
    /// the lineup and the events of each set, in the format of the files:
    /// `set_<n>_descriptor.json`, `set_<n>_events.csv`
    pub set_files: Vec<(String, Vec<u8>)>,
}

/// Where the matches to upload are read (`SqliteUploads`).
#[async_trait]
pub trait UploadSource: Sync {
    /// The match `match_id` of team `team_id`, as in the queue.
    async fn read(&self, team_id: &Uuid, match_id: &str) -> Result<ToUpload, AppError>;
}

/// The names of the files of a set in the archive.
fn set_file_names(set_number: u8) -> (String, String) {
    (
        format!("set_{set_number}_descriptor.json"),
        format!("set_{set_number}_events.csv"),
    )
}

/// The events in the format of the files of the versions before the
/// database (`set_<n>.csv`), which the API reads.
fn events_csv(events: &[EventEntry]) -> Result<Vec<u8>, AppError> {
    let mut writer = WriterBuilder::new().has_headers(false).from_writer(vec![]);
    events
        .iter()
        .try_for_each(|event| writer.serialize(event))
        .map_err(|e| AppError::IO(IOError::from(e)))?;
    writer
        .into_inner()
        .map_err(|e| AppError::IO(IOError::Msg(e.to_string())))
}

/// The files of the sets of `m` written as the versions before the
/// database saved them: the lineup as `set_<n>.json`, the events as
/// `set_<n>.csv`.
pub(crate) fn written_set_files(m: &MatchEntry) -> Result<Vec<(String, Vec<u8>)>, AppError> {
    m.sets
        .iter()
        .map(|set| {
            let (descriptor, events) = set_file_names(set.set_number);
            let json = to_vec_pretty(set).map_err(|e| AppError::IO(IOError::from(e)))?;
            Ok([(descriptor, json), (events, events_csv(&set.events)?)])
        })
        .collect::<Result<Vec<_>, AppError>>()
        .map(|files| files.into_iter().flatten().collect())
}

/// The archive of an upload: `match.json` (`MatchMetadata`), then the
/// files of the sets.
pub(crate) fn match_archive(
    m: &MatchEntry,
    set_files: Vec<(String, Vec<u8>)>,
    keys: Option<MatchKeys>,
) -> Result<Vec<u8>, AppError> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let metadata = MatchMetadata {
        date: m.date.to_string(),
        home: m.home,
        team_id: m.team.id,
        gender: m.team.gender,
        year: m.team.year,
        classification: m.team.classification,
        country: m.team.country,
        max_substitutions: m.max_substitutions,
        source: m.source,
        keys,
    };
    let metadata_json = to_string_pretty(&metadata).map_err(|e| AppError::IO(IOError::from(e)))?;
    let files =
        std::iter::once(("match.json".to_string(), metadata_json.into_bytes())).chain(set_files);
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

// retrieve signed URL for sending analytics data, and the keys of the match
// when asked
async fn get_signed_url(
    m: &MatchEntry,
    file_id: &str,
    upload_url: &str,
) -> Result<(String, Option<MatchKeys>), AppError> {
    let client = http_client()?;
    let uuid = generate_upload_id(m.team.id, file_id);
    let match_keys = keys_request(m);
    let asked = match_keys.is_some();
    let payload = UploadRequest {
        filename: format!("{}.zip", uuid),
        match_keys,
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
        // keys asked and not given (an API not updated yet): the match
        // goes without, as before
        let keys = match asked {
            true => response.json::<MatchKeys>().await.map_or_else(
                |e| {
                    log_error(&format!("no keys for file '{uuid}.zip': {e}"));
                    None
                },
                |keys| {
                    log_info(&format!(
                        "keys for file '{uuid}.zip': match {}",
                        keys.match_key
                    ));
                    Some(keys)
                },
            ),
            false => None,
        };
        Ok((signed_url, keys))
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
/// `upload_url`. Whoever calls it decides when: the desktop every few
/// seconds, the phone when there's a network.
pub async fn upload_next<QR: QueueReader + Send + Sync, QW: QueueWriter + Send + Sync>(
    source: &(impl UploadSource + ?Sized),
    queue_manager: &QueueManager<QR, QW>,
    upload_url: &str,
) -> Result<UploadStep, AppError> {
    let queue = queue_manager.load().await?;
    let Some(pending) = queue.peek().cloned() else {
        return Ok(UploadStep::Empty);
    };
    match source.read(&pending.team_id, &pending.match_id).await {
        Ok(m) => match process_single_upload(m, upload_url).await {
            Ok(()) => {
                log_info(&format!(
                    "statistics sent: match {} after {} failed attempts",
                    pending.match_id, pending.retry_count
                ));
                queue_manager
                    .dequeue(&pending.team_id, &pending.match_id)
                    .await?;
                Ok(UploadStep::Uploaded)
            }
            Err(e) => {
                log_error(&format!(
                    "statistics not sent: match {}, attempt {}: {e}",
                    pending.match_id,
                    u16::from(pending.retry_count) + 1
                ));
                // kept in the queue indefinitely
                queue_manager
                    .mark_retry(&pending.team_id, &pending.match_id)
                    .await?;
                Ok(UploadStep::Failed)
            }
        },
        Err(e) => {
            log_error(&format!(
                "statistics dropped: match {} can't be read: {e}",
                pending.match_id
            ));
            queue_manager
                .dequeue(&pending.team_id, &pending.match_id)
                .await?;
            Ok(UploadStep::Dropped)
        }
    }
}

// process a single upload
async fn process_single_upload(m: ToUpload, upload_url: &str) -> Result<(), AppError> {
    let (signed_url, keys) = get_signed_url(&m.entry, &m.file_id, upload_url).await?;
    upload_to_signed_url(&signed_url, match_archive(&m.entry, m.set_files, keys)?).await
}
