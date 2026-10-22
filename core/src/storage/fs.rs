//! The data in a folder of the file system: a key is a path in it.

use crate::{
    errors::{AppError, IOError},
    storage::{Store, StoreEntry},
};
use async_trait::async_trait;
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};
use tokio::{
    fs::{OpenOptions, create_dir_all, metadata, read, read_dir, try_exists, write},
    io::AsyncWriteExt,
};

#[derive(Debug)]
pub struct FsStore(PathBuf);

impl FsStore {
    pub fn new(root: &Path) -> Self {
        Self(root.to_path_buf())
    }

    /// The path of the key, which can't leave the root folder.
    fn path(&self, key: &str) -> Result<PathBuf, AppError> {
        match key {
            "" => Ok(self.0.clone()),
            _ => key.split('/').try_fold(self.0.clone(), |path, segment| {
                match segment {
                    "" | "." | ".." => None,
                    _ if segment.contains(['\\', ':']) => None,
                    _ => Some(path.join(segment)),
                }
                .ok_or_else(|| AppError::IO(IOError::Msg(format!("invalid key: {:?}", key))))
            }),
        }
    }

    async fn create_parent(path: &Path) -> Result<(), AppError> {
        match path.parent() {
            Some(parent) => create_dir_all(parent)
                .await
                .map_err(|e| AppError::IO(IOError::from(e))),
            None => Ok(()),
        }
    }
}

#[async_trait]
impl Store for FsStore {
    async fn read(&self, key: &str) -> Result<Option<Vec<u8>>, AppError> {
        match read(self.path(key)?).await {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(AppError::IO(IOError::from(e))),
        }
    }

    async fn write(&self, key: &str, data: &[u8]) -> Result<(), AppError> {
        let path = self.path(key)?;
        Self::create_parent(&path).await?;
        // written when awaited: a tokio file isn't, unless flushed
        write(&path, data)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))
    }

    async fn append(&self, key: &str, data: &[u8]) -> Result<(), AppError> {
        let path = self.path(key)?;
        Self::create_parent(&path).await?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        file.write_all(data)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        file.flush()
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))
    }

    async fn exists(&self, key: &str) -> Result<bool, AppError> {
        try_exists(self.path(key)?)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))
    }

    async fn list(&self, folder: &str) -> Result<Vec<StoreEntry>, AppError> {
        let mut dir = match read_dir(self.path(folder)?).await {
            Ok(dir) => dir,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(AppError::IO(IOError::from(e))),
        };
        let mut entries = Vec::new();
        while let Some(entry) = dir
            .next_entry()
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?
        {
            let name = entry.file_name().to_string_lossy().into_owned();
            // the links count as what they point to
            match metadata(entry.path()).await {
                Ok(m) if m.is_dir() => entries.push(StoreEntry::Folder(name)),
                Ok(m) if m.is_file() => entries.push(StoreEntry::Blob(name)),
                _ => {}
            }
        }
        entries.sort();
        Ok(entries)
    }
}
