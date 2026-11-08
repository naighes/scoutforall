//! The data in a folder of the file system: a key is a path in it.

use crate::{
    errors::{AppError, IOError},
    storage::{Stamp, Stamps, Store, StoreEntry},
};
use async_trait::async_trait;
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    fs::{
        File, OpenOptions, create_dir_all, metadata, read, read_dir, remove_dir_all, remove_file,
        rename, try_exists,
    },
    io::AsyncWriteExt,
};

#[derive(Debug)]
pub struct FsStore(PathBuf);

/// How long a file must be left alone before its stamp tells it changed:
/// some file systems keep when a file was modified to the second (or two,
/// FAT), and a file written twice within that time, the same size, would
/// look the same. As git does with the files changed right after its index.
const SETTLED: Duration = Duration::from_secs(3);

/// The stamps of each folder right in the folder, the hidden ones left out.
fn folder_stamps_of(folder: &Path) -> Result<Vec<(String, Stamps)>, AppError> {
    let dir = match std::fs::read_dir(folder) {
        Ok(dir) => dir,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(AppError::IO(IOError::from(e))),
    };
    let mut folders = Vec::new();
    for entry in dir {
        let entry = entry.map_err(|e| AppError::IO(IOError::from(e)))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // the links count as what they point to
        if name.starts_with('.') || !entry.path().is_dir() {
            continue;
        }
        folders.push((name, stamps_of(&entry.path())?));
    }
    folders.sort_by(|(a, _), (b, _)| a.cmp(b));
    Ok(folders)
}

/// The files right in the folder, the hidden ones left out (see `list`),
/// with their stamps, `None` for the ones not settled.
fn stamps_of(folder: &Path) -> Result<Stamps, AppError> {
    let dir = match std::fs::read_dir(folder) {
        Ok(dir) => dir,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(AppError::IO(IOError::from(e))),
    };
    let now = SystemTime::now();
    let mut stamps = Vec::new();
    for entry in dir {
        let entry = entry.map_err(|e| AppError::IO(IOError::from(e)))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // the links count as what they point to; the others from the
        // folder read, without looking the path up again
        let meta = match entry.file_type() {
            Ok(t) if t.is_symlink() => std::fs::metadata(entry.path()),
            _ => entry.metadata(),
        };
        let Ok(meta) = meta else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let modified = meta
            .modified()
            .ok()
            // in the future (the clock moved back) isn't settled either
            .filter(|t| now.duration_since(*t).is_ok_and(|age| age >= SETTLED))
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .and_then(|d| u64::try_from(d.as_nanos()).ok());
        stamps.push((
            name,
            modified.map(|modified| Stamp {
                size: meta.len(),
                modified,
            }),
        ));
    }
    stamps.sort_by(|(a, _), (b, _)| a.cmp(b));
    Ok(stamps)
}

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

    /// Beside the folder, hidden: where it goes while it's removed.
    fn removed_path(path: &Path) -> Result<PathBuf, AppError> {
        match (path.parent(), path.file_name()) {
            (Some(parent), Some(name)) => {
                Ok(parent.join(format!(".{}.removed", name.to_string_lossy())))
            }
            _ => Err(AppError::IO(IOError::Msg(format!(
                "no folder name: {:?}",
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

    async fn stamps(&self, folder: &str) -> Result<Vec<(String, Option<Stamp>)>, AppError> {
        // the folder at once: a task for each file costs more than the file
        let path = self.path(folder)?;
        tokio::task::spawn_blocking(move || stamps_of(&path))
            .await
            .map_err(|e| AppError::IO(IOError::Msg(e.to_string())))?
    }

    async fn folder_stamps(&self, folder: &str) -> Result<Vec<(String, Stamps)>, AppError> {
        let path = self.path(folder)?;
        tokio::task::spawn_blocking(move || folder_stamps_of(&path))
            .await
            .map_err(|e| AppError::IO(IOError::Msg(e.to_string())))?
    }

    async fn remove(&self, key: &str) -> Result<(), AppError> {
        let path = self.path(key)?;
        match remove_file(&path).await {
            Ok(()) => Self::sync_folder(&path).await,
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::IO(IOError::from(e))),
        }
    }

    async fn remove_folder(&self, folder: &str) -> Result<(), AppError> {
        let path = match folder {
            "" => Err(AppError::IO(IOError::Msg(
                "the root can't be removed".into(),
            ))),
            _ => self.path(folder),
        }?;
        // hidden first, at once: the readers never see half of it, and what
        // a removal stopped halfway leaves is never data; then away
        let hidden = Self::removed_path(&path)?;
        match remove_dir_all(&hidden).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::IO(IOError::from(e))),
        }?;
        match rename(&path, &hidden).await {
            Ok(()) => {
                Self::sync_folder(&path).await?;
                remove_dir_all(&hidden)
                    .await
                    .map_err(|e| AppError::IO(IOError::from(e)))
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::IO(IOError::from(e))),
        }
    }
}
