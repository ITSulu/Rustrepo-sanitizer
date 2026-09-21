//! Storage for uploaded repositories and archives.
//!
//! Uploads are written under one managed root keyed by an opaque id; the id
//! never maps directly to a filesystem path, so a caller cannot address files
//! outside the store. Entries expire.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UploadKind {
    Archive,
    Directory,
}

#[derive(Clone, Debug)]
pub struct UploadEntry {
    pub id: String,
    pub path: PathBuf,
    pub kind: UploadKind,
    pub name: String,
    pub bytes: u64,
    pub created: Instant,
}

pub struct UploadStore {
    root: PathBuf,
    entries: Mutex<HashMap<String, UploadEntry>>,
    max_bytes: u64,
}

impl UploadStore {
    pub fn new(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root).context("creating upload root")?;
        Ok(Self {
            root,
            entries: Mutex::new(HashMap::new()),
            max_bytes: 512 * 1024 * 1024,
        })
    }

    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Streams an upload into the store, enforcing the size bound. The returned
    /// id is opaque.
    pub fn put<R: std::io::Read>(&self, name: &str, mut reader: R) -> Result<UploadEntry> {
        let id = uuid::Uuid::new_v4().to_string();
        let dir = self.root.join(&id);
        std::fs::create_dir_all(&dir).context("creating upload directory")?;
        let safe_name = Path::new(name)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "upload".to_owned());
        let path = dir.join(&safe_name);
        let mut file = std::fs::File::create(&path).context("creating upload file")?;
        let mut limited = std::io::Read::take(reader.by_ref(), self.max_bytes + 1);
        let bytes = std::io::copy(&mut limited, &mut file).context("writing upload")?;
        if bytes > self.max_bytes {
            let _ = std::fs::remove_dir_all(&dir);
            bail!("upload exceeds the size limit");
        }
        file.flush().ok();
        let entry = UploadEntry {
            id: id.clone(),
            path,
            kind: UploadKind::Archive,
            name: safe_name,
            bytes,
            created: Instant::now(),
        };
        self.entries.lock().unwrap().insert(id, entry.clone());
        Ok(entry)
    }

    /// Registers an existing directory (used for local/test inputs).
    pub fn register_directory(&self, path: PathBuf) -> Result<UploadEntry> {
        let canonical = std::fs::canonicalize(&path)?;
        if !canonical.starts_with(&self.root) {
            bail!("registered directory must live inside the upload root");
        }
        let entry = UploadEntry {
            id: uuid::Uuid::new_v4().to_string(),
            name: canonical
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "upload".to_owned()),
            path: canonical,
            kind: UploadKind::Directory,
            bytes: 0,
            created: Instant::now(),
        };
        self.entries
            .lock()
            .unwrap()
            .insert(entry.id.clone(), entry.clone());
        Ok(entry)
    }

    pub fn get(&self, id: &str) -> Option<UploadEntry> {
        self.entries.lock().unwrap().get(id).cloned()
    }

    pub fn remove(&self, id: &str) -> bool {
        let entry = self.entries.lock().unwrap().remove(id);
        match entry {
            Some(entry) => {
                if entry.kind == UploadKind::Archive {
                    if let Some(parent) = entry.path.parent() {
                        let _ = std::fs::remove_dir_all(parent);
                    }
                }
                true
            }
            None => false,
        }
    }

    pub fn cleanup_expired(&self, ttl: Duration) -> usize {
        let now = Instant::now();
        let expired: Vec<String> = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, entry)| now.duration_since(entry.created) > ttl)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &expired {
            self.remove(id);
        }
        expired.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_retrieves_uploads_by_opaque_id() {
        let root = tempfile::tempdir().unwrap();
        let store = UploadStore::new(root.path().to_path_buf()).unwrap();
        let entry = store
            .put("repo.tar.gz", &b"payload"[..])
            .expect("store upload");
        assert!(entry.path.starts_with(root.path()));
        assert_eq!(store.get(&entry.id).unwrap().bytes, 7);
        assert!(store.remove(&entry.id));
        assert!(store.get(&entry.id).is_none());
    }

    #[test]
    fn rejects_oversized_uploads() {
        let root = tempfile::tempdir().unwrap();
        let store = UploadStore::new(root.path().to_path_buf())
            .unwrap()
            .with_max_bytes(4);
        assert!(store.put("big.zip", &b"12345"[..]).is_err());
    }

    #[test]
    fn expires_old_entries() {
        let root = tempfile::tempdir().unwrap();
        let store = UploadStore::new(root.path().to_path_buf()).unwrap();
        store.put("a.zip", &b"x"[..]).unwrap();
        assert_eq!(store.cleanup_expired(Duration::ZERO), 1);
        assert_eq!(store.cleanup_expired(Duration::ZERO), 0);
    }

    #[test]
    fn registers_directories_only_inside_the_root() {
        let root = tempfile::tempdir().unwrap();
        let store = UploadStore::new(root.path().to_path_buf()).unwrap();
        let inside = root.path().join("repo");
        std::fs::create_dir_all(&inside).unwrap();
        assert!(store.register_directory(inside).is_ok());
        let outside = tempfile::tempdir().unwrap();
        assert!(store
            .register_directory(outside.path().to_path_buf())
            .is_err());
    }
}
