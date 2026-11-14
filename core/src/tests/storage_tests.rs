//! Where the blobs live (the settings, the clips, the data folder of the
//! versions before the database): every store keeps them the same way.

#[cfg(test)]
mod tests {
    use crate::storage::{Store, StoreEntry, fs::FsStore, layout, memory::MemoryStore};
    use tempfile::TempDir;

    async fn blobs_and_folders(store: &dyn Store) {
        assert_eq!(store.read("a/b.json").await.unwrap(), None);
        assert_eq!(store.list("a").await.unwrap(), vec![]);
        assert!(!store.exists("a").await.unwrap());
        store.write("a/b.json", b"{}").await.unwrap();
        store.write("a/b.json", b"[]").await.unwrap();
        assert_eq!(store.read("a/b.json").await.unwrap(), Some(b"[]".to_vec()));
        store.write("a/c/d.csv", b"1\n").await.unwrap();
        assert_eq!(
            store.read("a/c/d.csv").await.unwrap(),
            Some(b"1\n".to_vec())
        );
        store.write("top.json", b"").await.unwrap();
        assert!(store.exists("a").await.unwrap());
        assert!(store.exists("a/c").await.unwrap());
        assert!(store.exists("a/b.json").await.unwrap());
        assert!(!store.exists("a/e").await.unwrap());
        assert_eq!(
            store.list("a").await.unwrap(),
            vec![
                StoreEntry::Blob("b.json".into()),
                StoreEntry::Folder("c".into())
            ]
        );
        assert_eq!(
            store.list(layout::ROOT).await.unwrap(),
            vec![
                StoreEntry::Blob("top.json".into()),
                StoreEntry::Folder("a".into())
            ]
        );
    }

    /// A blob removed: gone, its folder listing without it; a blob that
    /// isn't there removed without an error.
    async fn removing(store: &dyn Store) {
        store.write("a/b.json", b"{}").await.unwrap();
        store.write("a/c.csv", b"1").await.unwrap();
        store.remove("a/b.json").await.unwrap();
        assert_eq!(store.read("a/b.json").await.unwrap(), None);
        assert!(!store.exists("a/b.json").await.unwrap());
        assert_eq!(
            store.list("a").await.unwrap(),
            vec![StoreEntry::Blob("c.csv".into())]
        );
        store.remove("a/b.json").await.unwrap();
        store.remove("nowhere/x.json").await.unwrap();
    }

    /// A folder removed: gone with all it holds, the folders beside it
    /// kept; one that isn't there removed without an error, the root never.
    async fn removing_folders(store: &dyn Store) {
        store.write("a/b/c.json", b"{}").await.unwrap();
        store.write("a/b/d/e.csv", b"1").await.unwrap();
        store.write("a/f.json", b"{}").await.unwrap();
        store.write("ab/g.json", b"{}").await.unwrap();
        store.remove_folder("a/b").await.unwrap();
        assert!(!store.exists("a/b").await.unwrap());
        assert_eq!(store.read("a/b/c.json").await.unwrap(), None);
        assert_eq!(store.read("a/b/d/e.csv").await.unwrap(), None);
        assert_eq!(
            store.list("a").await.unwrap(),
            vec![StoreEntry::Blob("f.json".into())]
        );
        store.remove_folder("a").await.unwrap();
        assert_eq!(
            store.list(layout::ROOT).await.unwrap(),
            vec![StoreEntry::Folder("ab".into())]
        );
        store.remove_folder("a").await.unwrap();
        store.remove_folder("nowhere/x").await.unwrap();
        assert!(store.remove_folder(layout::ROOT).await.is_err());
        assert!(store.exists("ab/g.json").await.unwrap());
    }

    #[tokio::test]
    async fn the_stores_remove_folders_alike() {
        let dir = TempDir::new().unwrap();
        removing_folders(&FsStore::new(dir.path())).await;
        removing_folders(&MemoryStore::new()).await;
    }

    /// What a removal stopped halfway leaves, hidden, is never data, and
    /// goes with the next removal of the folder.
    #[tokio::test]
    async fn a_folder_removal_stopped_halfway_leaves_no_data() {
        let dir = TempDir::new().unwrap();
        let store = FsStore::new(dir.path());
        store.write("a/b.json", b"{}").await.unwrap();
        std::fs::create_dir_all(dir.path().join(".a.removed/c")).unwrap();
        std::fs::write(dir.path().join(".a.removed/c/d.json"), b"{}").unwrap();
        assert_eq!(
            store.list(layout::ROOT).await.unwrap(),
            vec![StoreEntry::Folder("a".into())]
        );
        store.remove_folder("a").await.unwrap();
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn the_stores_remove_blobs_alike() {
        let dir = TempDir::new().unwrap();
        removing(&FsStore::new(dir.path())).await;
        removing(&MemoryStore::new()).await;
    }

    #[tokio::test]
    async fn the_stores_keep_blobs_and_folders_alike() {
        let dir = TempDir::new().unwrap();
        blobs_and_folders(&FsStore::new(dir.path())).await;
        blobs_and_folders(&MemoryStore::new()).await;
    }

    /// A write goes to a hidden file first, then in place of the old one:
    /// stopped halfway, the old one is still there, whole; what's left of
    /// it is never data.
    #[tokio::test]
    async fn a_write_stopped_halfway_leaves_the_old_file() {
        let dir = TempDir::new().unwrap();
        let store = FsStore::new(dir.path());
        store.write("a/b.json", b"old").await.unwrap();
        assert!(!dir.path().join("a/.b.json.tmp").exists());
        // a write stopped halfway: the new file never took the place of
        // the old one
        std::fs::write(dir.path().join("a/.b.json.tmp"), b"ne").unwrap();
        std::fs::write(dir.path().join("a/.DS_Store"), b"").unwrap();
        assert_eq!(store.read("a/b.json").await.unwrap(), Some(b"old".to_vec()));
        assert_eq!(
            store.list("a").await.unwrap(),
            vec![StoreEntry::Blob("b.json".into())]
        );
        // the next write goes over it
        store.write("a/b.json", b"new").await.unwrap();
        assert_eq!(store.read("a/b.json").await.unwrap(), Some(b"new".to_vec()));
        assert!(!dir.path().join("a/.b.json.tmp").exists());
        // the new file can't be written: the old one stays
        std::fs::create_dir(dir.path().join("a/.b.json.tmp")).unwrap();
        assert!(store.write("a/b.json", b"newer").await.is_err());
        assert_eq!(store.read("a/b.json").await.unwrap(), Some(b"new".to_vec()));
    }

    /// A key can't reach out of the folder of the data.
    #[tokio::test]
    async fn the_file_system_store_stays_in_its_folder() {
        let dir = TempDir::new().unwrap();
        let store = FsStore::new(&dir.path().join("data"));
        for key in [
            "../out.json",
            "a/../../out.json",
            "a//b",
            "/abs",
            "a\\b",
            "c:x",
        ] {
            assert!(store.write(key, b"x").await.is_err(), "{key}");
            assert!(store.read(key).await.is_err(), "{key}");
        }
        assert!(!dir.path().join("out.json").exists());
    }
}
