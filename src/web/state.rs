//! Shared application state, assembled once at startup.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use url::Url;

use crate::web::acquire::{CloneRunner, HostResolver, SystemCloneRunner, SystemResolver};
use crate::web::integrations::{Integrations, IntegrationsConfig};
use crate::web::jobs::Jobs;
use crate::web::uploads::UploadStore;
use crate::web::workspace::{Limits, WorkspaceManager};

/// Web runtime settings, from the environment with optional CLI overrides.
#[derive(Clone, Debug)]
pub struct WebSettings {
    pub bind: SocketAddr,
    pub token: Option<String>,
    pub root: PathBuf,
    pub local_roots: Vec<PathBuf>,
    pub forgejo_base: Option<Url>,
    pub forgejo_token: Option<String>,
    pub github_api: Option<Url>,
    pub github_token: Option<String>,
}

impl Default for WebSettings {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            token: None,
            root: workspace_root(),
            local_roots: Vec::new(),
            forgejo_base: None,
            forgejo_token: None,
            github_api: None,
            github_token: None,
        }
    }
}

impl WebSettings {
    /// Reads settings from the process environment.
    pub fn from_env() -> Self {
        let nonempty = |value: Option<String>| value.filter(|v| !v.is_empty());
        let parse_url = |value: Option<String>| value.and_then(|v| Url::parse(&v).ok());
        Self {
            bind: std::env::var("RUSTREPO_WEB_BIND")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or_else(default_bind),
            token: nonempty(std::env::var("RUSTREPO_WEB_TOKEN").ok()),
            root: workspace_root(),
            local_roots: std::env::var("RUSTREPO_WEB_LOCAL_ROOTS")
                .unwrap_or_default()
                .split(':')
                .filter(|entry| !entry.trim().is_empty())
                .map(PathBuf::from)
                .collect(),
            forgejo_base: parse_url(std::env::var("RUSTREPO_WEB_FORGEJO_BASE").ok()),
            forgejo_token: nonempty(std::env::var("RUSTREPO_WEB_FORGEJO_TOKEN").ok()),
            github_api: parse_url(std::env::var("RUSTREPO_WEB_GITHUB_API").ok()),
            github_token: nonempty(std::env::var("RUSTREPO_WEB_GITHUB_TOKEN").ok()),
        }
    }
}

pub struct AppState {
    pub workspaces: WorkspaceManager,
    pub uploads: Arc<UploadStore>,
    pub integrations: Arc<Integrations>,
    pub jobs: Jobs,
    pub token: Option<String>,
    pub allowed_local_roots: Vec<PathBuf>,
    pub limits: Limits,
    /// Mark the UI session cookie `Secure` when the server is reachable beyond
    /// loopback (i.e. likely over TLS via a reverse proxy).
    pub secure_cookies: bool,
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
            secure_cookies: false,
            runner,
            resolver,
        }
    }

    /// Builds state from resolved web settings.
    pub fn from_settings(settings: &WebSettings) -> Result<Self> {
        let limits = Limits::default();
        let workspaces = WorkspaceManager::new(settings.root.join("workspaces"), limits.clone())?;
        let uploads = Arc::new(
            UploadStore::new(settings.root.join("uploads"))?
                .with_max_bytes(limits.max_upload_bytes),
        );
        let integrations = Arc::new(Integrations::new(IntegrationsConfig {
            forgejo_base: settings.forgejo_base.clone(),
            forgejo_token: settings.forgejo_token.clone(),
            github_api: settings
                .github_api
                .clone()
                .unwrap_or_else(|| Url::parse("https://api.github.com").expect("valid default")),
            github_token: settings.github_token.clone(),
        }));
        let mut state = Self::new(
            workspaces,
            uploads,
            integrations,
            settings.token.clone(),
            settings.local_roots.clone(),
            limits,
            Arc::new(SystemCloneRunner {
                timeout: std::time::Duration::from_secs(300),
            }),
            Arc::new(SystemResolver),
        );
        state.secure_cookies = !settings.bind.ip().is_loopback();
        Ok(state)
    }
}

/// Default bind address: loopback only, so the trust boundary is explicit.
pub fn default_bind() -> SocketAddr {
    "127.0.0.1:8787".parse().expect("valid default bind")
}

pub fn workspace_root() -> PathBuf {
    std::env::var("RUSTREPO_WEB_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("rustrepo-sanitizer"))
}
