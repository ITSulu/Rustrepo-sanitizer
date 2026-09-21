//! Isolated, bounded, deterministically cleaned temporary workspaces.
//!
//! Every job gets its own directory under a single managed root. Directories
//! are removed when the job finishes, after download, or when they outlive the
//! configured TTL (swept on demand). Concurrency is bounded by a semaphore so a
//! burst of requests cannot exhaust the host.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Clone, Debug)]
pub struct Limits {
    pub max_concurrent_jobs: usize,
    /// Upper bound on live job entries (queued + running + awaiting download).
    pub max_jobs: usize,
    pub max_upload_bytes: u64,
    pub max_repo_bytes: u64,
    pub job_ttl: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_concurrent_jobs: 4,
            max_jobs: 64,
            max_upload_bytes: 512 * 1024 * 1024,
            max_repo_bytes: 2 * 1024 * 1024 * 1024,
            job_ttl: Duration::from_secs(30 * 60),
        }
    }
}

pub struct WorkspaceManager {
    root: PathBuf,
    limits: Limits,
    permits: Arc<Semaphore>,
}

impl WorkspaceManager {
    pub fn new(root: PathBuf, limits: Limits) -> Result<Self> {
        std::fs::create_dir_all(&root)
            .with_context(|| format!("creating workspace root {}", root.display()))?;
        let permits = Arc::new(Semaphore::new(limits.max_concurrent_jobs.max(1)));
        Ok(Self {
            root,
            limits,
            permits,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    /// Block until a concurrency slot is free.
    pub async fn acquire_slot(&self) -> Result<OwnedSemaphorePermit> {
        self.permits
            .clone()
            .acquire_owned()
            .await
            .context("workspace semaphore closed")
    }

    /// Creates a unique, isolated directory for one job.
    pub fn create(&self, label: &str) -> Result<tempfile::TempDir> {
        let label: String = label
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .take(32)
            .collect();
        tempfile::Builder::new()
            .prefix(&format!("job-{label}-"))
            .tempdir_in(&self.root)
            .context("creating job workspace")
    }

    /// Removes workspaces older than the configured TTL. Returns the count
    /// removed. Used on startup and after each job for deterministic cleanup.
    pub fn cleanup_stale(&self) -> Result<usize> {
        self.cleanup_older_than(self.limits.job_ttl)
    }

    pub fn cleanup_older_than(&self, max_age: Duration) -> Result<usize> {
        let mut removed = 0;
        let now = SystemTime::now();
        for entry in std::fs::read_dir(&self.root).context("reading workspace root")? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let modified = metadata.modified().unwrap_or(now);
            let age = now.duration_since(modified).unwrap_or_default();
            if age > max_age {
                if metadata.is_dir() {
                    std::fs::remove_dir_all(entry.path()).ok();
                } else {
                    std::fs::remove_file(entry.path()).ok();
                }
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Total bytes currently held under the workspace root (best effort).
    pub fn current_bytes(&self) -> u64 {
        fn walk(path: &Path, total: &mut u64) {
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    if let Ok(meta) = std::fs::symlink_metadata(entry.path()) {
                        if meta.file_type().is_symlink() {
                            continue;
                        }
                        if meta.is_dir() {
                            walk(&entry.path(), total);
                        } else if meta.is_file() {
                            *total = total.saturating_add(meta.len());
                        }
                    }
                }
            }
        }
        let mut total = 0;
        walk(&self.root, &mut total);
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_isolated_unique_directories() {
        let root = tempfile::tempdir().unwrap();
        let manager = WorkspaceManager::new(root.path().to_path_buf(), Limits::default()).unwrap();
        let a = manager.create("local").unwrap();
        let b = manager.create("local").unwrap();
        assert_ne!(a.path(), b.path());
        assert!(a.path().starts_with(root.path()));
        assert!(b.path().is_dir());
    }

    #[tokio::test]
    async fn bounds_concurrency() {
        let root = tempfile::tempdir().unwrap();
        let manager = WorkspaceManager::new(
            root.path().to_path_buf(),
            Limits {
                max_concurrent_jobs: 2,
                ..Limits::default()
            },
        )
        .unwrap();
        let first = manager.acquire_slot().await.unwrap();
        let second = manager.acquire_slot().await.unwrap();
        assert!(manager.permits.clone().try_acquire_owned().is_err());
        drop(first);
        assert!(manager.permits.clone().try_acquire_owned().is_ok());
        drop(second);
    }

    #[test]
    fn sweeps_stale_workspaces() {
        let root = tempfile::tempdir().unwrap();
        let manager = WorkspaceManager::new(root.path().to_path_buf(), Limits::default()).unwrap();
        let dir = manager.create("old").unwrap();
        let path = dir.path().to_path_buf();
        // Keep the directory on disk but simulate age.
        let _ = dir.keep();
        assert!(path.exists());
        let removed = manager.cleanup_older_than(Duration::ZERO).unwrap();
        assert_eq!(removed, 1);
        assert!(!path.exists());
    }

    #[test]
    fn reports_current_bytes() {
        let root = tempfile::tempdir().unwrap();
        let manager = WorkspaceManager::new(root.path().to_path_buf(), Limits::default()).unwrap();
        let dir = manager.create("bytes").unwrap();
        std::fs::write(dir.path().join("f"), b"12345").unwrap();
        assert!(manager.current_bytes() >= 5);
    }
}
