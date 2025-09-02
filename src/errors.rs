use thiserror::Error;

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
}

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("missing initial rotation: {0}")]
    MissingInitialRotation(String),
    #[error("missing initial rotation: {0}")]
    LineupError(String),
}

#[derive(Debug, Error)]
pub enum IOError {
    #[error("IO error: {0}")]
    Error(String),
    #[error("serialization error")]
    SerializationError,
    #[error("encoding error: {0}")]
    EncodingError(String),
}
