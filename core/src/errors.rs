use thiserror::Error;
use zip::result::ZipError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("snapshot error: {0}")]
    Snapshot(#[from] SnapshotError),

    #[error("match error: {0}")]
    Match(#[from] MatchError),

    #[error("IO error: {0}")]
    IO(#[from] IOError),
}

#[derive(Debug, Error)]
pub enum MatchError {
    #[error("load set error: {0}")]
    LoadSetError(String),
    #[error("set entry error: {0}")]
    SetEntryError(String),
    #[error("match already exists: {0}")]
    MatchAlreadyExists(String),
    #[error("set outcome changed: {0}")]
    SetOutcomeChanged(String),
    /// a set this version doesn't replay, why in the language of the app
    #[error("set not valid: {0}")]
    SetNotValid(String),
}

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("missing initial rotation: {0}")]
    LineupError(String),
    #[error("invalid event: {0}")]
    InvalidEvent(String),
}

#[derive(Debug, Error)]
pub enum IOError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde JSON error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("{0}")]
    Msg(String),
    #[error("CSV error: {0}")]
    Csv(#[from] csv::Error),
    #[error("std error: {0}")]
    Std(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error("zip error: {0}")]
    Zip(#[from] ZipError),
}
