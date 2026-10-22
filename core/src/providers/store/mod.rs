//! The providers over a `Store`, wherever it keeps the data: they read and
//! write its format, JSON and CSV, as the older versions expect it.

pub mod match_reader;
pub mod match_writer;
pub mod queue_reader;
pub mod queue_writer;
pub mod set_reader;
pub mod set_writer;
pub mod team_reader;
pub mod team_writer;

use crate::{
    errors::{AppError, IOError},
    storage::Store,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{from_slice, to_vec_pretty};

/// The blob under the key, which must be there.
pub(crate) async fn read_blob(store: &dyn Store, key: &str) -> Result<Vec<u8>, AppError> {
    store
        .read(key)
        .await?
        .ok_or_else(|| AppError::IO(IOError::Msg(format!("not found: {}", key))))
}

pub(crate) async fn read_json<T: DeserializeOwned>(
    store: &dyn Store,
    key: &str,
) -> Result<T, AppError> {
    from_slice(&read_blob(store, key).await?).map_err(|e| AppError::IO(IOError::from(e)))
}

pub(crate) async fn write_json<T: Serialize>(
    store: &dyn Store,
    key: &str,
    value: &T,
) -> Result<(), AppError> {
    let json = to_vec_pretty(value).map_err(|e| AppError::IO(IOError::from(e)))?;
    store.write(key, &json).await
}
