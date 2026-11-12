use thiserror::Error;
use zip::result::ZipError;

use crate::localization::Labels;

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

impl AppError {
    /// The error for the user, in the language of `labels`: the messages
    /// of the core as they are, the files and the network by their kind.
    pub fn message(&self, labels: &Labels) -> String {
        match self {
            AppError::IO(e) => e.message(labels),
            other => other.to_string(),
        }
    }
}

impl IOError {
    /// See `AppError::message`.
    pub fn message(&self, labels: &Labels) -> String {
        match self {
            IOError::Msg(message) => message.clone(),
            IOError::Io(e) => io_message(e, labels),
            IOError::SerdeJson(e) => format!("{}: {e}", labels.io_invalid_file),
            IOError::Csv(e) => format!("{}: {e}", labels.io_invalid_file),
            IOError::Zip(e) => format!("{}: {e}", labels.io_invalid_file),
            IOError::Reqwest(e) => format!("{}: {e}", labels.io_network),
            IOError::Std(e) => format!("{}: {e}", labels.io_failed),
        }
    }
}

/// An error of the files in the language of `labels`: the kinds the user
/// can act on in words, the others with the system's text.
pub fn io_message(e: &std::io::Error, labels: &Labels) -> String {
    use std::io::ErrorKind::*;
    match e.kind() {
        NotFound => labels.io_not_found.to_string(),
        PermissionDenied => labels.io_permission_denied.to_string(),
        StorageFull | QuotaExceeded => labels.io_storage_full.to_string(),
        ReadOnlyFilesystem => labels.io_read_only.to_string(),
        _ => format!("{}: {e}", labels.io_failed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{localization::labels, shapes::enums::LanguageEnum};

    #[test]
    fn the_errors_of_the_files_in_the_language() {
        let it = labels(LanguageEnum::It);
        let full = AppError::IO(IOError::Io(std::io::Error::from(
            std::io::ErrorKind::StorageFull,
        )));
        assert_eq!(full.message(it), "il disco è pieno");
        let missing = IOError::Io(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert_eq!(missing.message(it), "file o cartella non trovati");
        let other = IOError::Io(std::io::Error::other("boom"));
        assert_eq!(
            other.message(it),
            "lettura o scrittura del file non riuscita: boom"
        );
        let said = AppError::IO(IOError::Msg("partita già esistente".into()));
        assert_eq!(said.message(it), "partita già esistente");
        let json = IOError::SerdeJson(serde_json::from_str::<u8>("x").unwrap_err());
        assert!(json.message(it).starts_with("il file non è valido: "));
    }
}
