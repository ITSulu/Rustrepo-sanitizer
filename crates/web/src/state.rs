//! Shared application state, assembled once at startup.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;

use crate::acquire::{CloneRunner, HostResolver, SystemCloneRunner, SystemResolver};
use crate::integrations::{Integrations, IntegrationsConfig};
use crate::jobs::Jobs;
use crate::uploads::UploadStore;
use crate::workspace::{Limits, WorkspaceManager};

pub struct AppState {
    pub workspaces: WorkspaceManager,
    pub uploads: Arc<UploadStore>,
    pub integrations: Arc<Integrations>,
    pub jobs: Jobs,
    pub token: Option<String>,
    pub allowed_local_roots: Vec<PathBuf>,
    pub limits: Limits,
    pub runner: Arc<dyn CloneRunner>,
    pub resolver: Arc<dyn HostResolver>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        workspaces: WorkspaceManager,
        uploads: Arc<UploadStore>,
        integrations: Arc<Integrations>,
        token: Option<String>,
        allowed_local_roots: Vec<PathBuf>,
        limits: Limits,
        runner: Arc<dyn CloneRunner>,
        resolver: Arc<dyn HostResolver>,
    ) -> Self {
        Self {
            workspaces,
            uploads,
            integrations,
            jobs: Jobs::new(),
            token,
            allowed_local_roots,
            limits,
            runner,
            resolver,
        }
    }

    /// Builds state from the process environment. Used by the binary.
    pub fn from_env(root: PathBuf) -> Result<Self> {
        let limits = Limits::default();
        let workspaces = WorkspaceManager::new(root.join("workspaces"), limits.clone())?;
        let uploads = Arc::new(
            UploadStore::new(root.join("uploads"))?.with_max_bytes(limits.max_upload_bytes),
        );
        let integrations = Arc::new(Integrations::new(IntegrationsConfig::from_env()));
        let token = std::env::var("RUSTREPO_WEB_TOKEN")
            .ok()
            .filter(|t| !t.is_empty());
        let allowed_local_roots = std::env::var("RUSTREPO_WEB_LOCAL_ROOTS")
            .unwrap_or_default()
            .split(':')
            .filter(|entry| !entry.trim().is_empty())
            .map(PathBuf::from)
            .collect();
        Ok(Self::new(
            workspaces,
            uploads,
            integrations,
            token,
            allowed_local_roots,
            limits,
            Arc::new(SystemCloneRunner {
                timeout: std::time::Duration::from_secs(300),
            }),
            Arc::new(SystemResolver),
        ))
    }
}

/// Default bind address: loopback only, so the trust boundary is explicit.
pub fn default_bind() -> SocketAddr {
    "127.0.0.1:8787".parse().expect("valid default bind")
}

pub fn workspace_root() -> PathBuf {
    std::env::var("RUSTREPO_WEB_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("rustrepo-sanitizer-web"))
}
