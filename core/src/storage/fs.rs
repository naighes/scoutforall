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
    fs::{File, OpenOptions, create_dir_all, metadata, read, read_dir, rename, try_exists},
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

    /// Beside the file, hidden: what's written goes there first.
    fn temp_path(path: &Path) -> Result<PathBuf, AppError> {
        match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => {
                Ok(parent.join(format!(".{}.tmp", name.to_string_lossy())))
            }
            _ => Err(AppError::IO(IOError::Msg(format!(
                "no file name: {:?}",
                path
            )))),
        }
    }

    /// On the disk, not only in the cache of the system: the battery may
    /// run out right after.
    async fn sync(file: &File) -> Result<(), AppError> {
        file.sync_all()
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))
    }

    /// The folder knows the file renamed in it, on the disk too; only on
    /// unix a folder can be opened as a file.
    async fn sync_folder(path: &Path) -> Result<(), AppError> {
        match path.parent() {
            Some(parent) if cfg!(unix) => {
                let folder = File::open(parent)
                    .await
                    .map_err(|e| AppError::IO(IOError::from(e)))?;
                Self::sync(&folder).await
            }
            _ => Ok(()),
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
        // a new file beside, then in its place: a write stopped halfway (the
        // app killed, the battery out) leaves the old one, never half of it
        let temp = Self::temp_path(&path)?;
        let mut file = File::create(&temp)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        file.write_all(data)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        Self::sync(&file).await?;
        drop(file);
        rename(&temp, &path)
            .await
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        Self::sync_folder(&path).await
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
            .map_err(|e| AppError::IO(IOError::from(e)))?;
        Self::sync(&file).await
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
            // hidden: a write stopped halfway, or the files of the system
            // (.DS_Store), never data
            if name.starts_with('.') {
                continue;
            }
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
