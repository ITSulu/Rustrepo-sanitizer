//! In-memory job registry and sanitization runner.
//!
//! v0.6.0 is stateless apart from these bounded, expiring jobs and their temp
//! workspaces. Each job owns its workspace for its lifetime; completion,
//! cancellation, TTL expiry, and download all release it deterministically.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::sanitizer::{self, ProgressEvent};
use anyhow::{bail, Context, Result};
use serde::Serialize;

use crate::web::acquire::{AcquireError, Acquirer};
use crate::web::dto::{InputMode, InputSpec, OptionsDto};
use crate::web::reports::extract_reports;
use crate::web::security::safe_output_name;
use crate::web::state::AppState;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running {
        phase: String,
        examined: usize,
        included: usize,
    },
    Completed {
        included: usize,
        excluded: usize,
        redactions: usize,
        dry_run: bool,
        archive: Option<String>,
        reports: Vec<String>,
    },
    Failed {
        message: String,
    },
    Cancelled,
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobStatus::Completed { .. } | JobStatus::Failed { .. } | JobStatus::Cancelled
        )
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct JobView {
    pub id: String,
    pub input_mode: InputMode,
    pub status: JobStatus,
    pub age_secs: u64,
}

struct JobInner {
    id: String,
    input_mode: InputMode,
    status: JobStatus,
    created: Instant,
    workspace: Option<tempfile::TempDir>,
    archive: Option<PathBuf>,
    reports: Vec<PathBuf>,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct Jobs {
    entries: Mutex<HashMap<String, Arc<Mutex<JobInner>>>>,
}

impl Jobs {
    pub fn new() -> Self {
        Self::default()
    }

    fn insert(&self, inner: JobInner) -> Arc<Mutex<JobInner>> {
        let handle = Arc::new(Mutex::new(inner));
        self.entries
            .lock()
            .unwrap()
            .insert(handle.lock().unwrap().id.clone(), handle.clone());
        handle
    }

    pub fn get(&self, id: &str) -> Option<JobView> {
        let handle = self.entries.lock().unwrap().get(id).cloned()?;
        let inner = handle.lock().unwrap();
        Some(JobView {
            id: inner.id.clone(),
            input_mode: inner.input_mode.clone(),
            status: inner.status.clone(),
            age_secs: inner.created.elapsed().as_secs(),
        })
    }

    pub fn cancel(&self, id: &str) -> bool {
        if let Some(handle) = self.entries.lock().unwrap().get(id).cloned() {
            handle.lock().unwrap().cancel.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Removes jobs older than `ttl`, dropping their workspaces.
    pub fn cleanup_expired(&self, ttl: Duration) -> usize {
        let now = Instant::now();
        let mut entries = self.entries.lock().unwrap();
        let stale: Vec<String> = entries
            .iter()
            .filter(|(_, handle)| {
                let inner = handle.lock().unwrap();
                now.duration_since(inner.created) > ttl && inner.status.is_terminal()
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &stale {
            entries.remove(id);
        }
        stale.len()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Resolves a downloadable artifact: the sanitized archive or a named report.
    pub fn artifact(&self, id: &str, name: Option<&str>) -> Option<(PathBuf, String)> {
        let handle = self.entries.lock().unwrap().get(id).cloned()?;
        let inner = handle.lock().unwrap();
        match name {
            None => inner
                .archive
                .as_ref()
                .map(|path| (path.clone(), download_name(path))),
            Some(requested) => inner
                .reports
                .iter()
                .find(|path| {
                    path.file_name()
                        .map(|n| n.to_string_lossy() == requested)
                        .unwrap_or(false)
                })
                .map(|path| (path.clone(), requested.to_owned())),
        }
    }

    /// Lists the report names available for a completed job.
    pub fn reports(&self, id: &str) -> Vec<String> {
        self.entries
            .lock()
            .unwrap()
            .get(id)
            .map(|handle| {
                handle
                    .lock()
                    .unwrap()
                    .reports
                    .iter()
                    .filter_map(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn download_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sanitized-output".to_owned())
}

/// Creates and starts a sanitization job, returning its id immediately.
pub async fn start_job(
    state: Arc<AppState>,
    spec: InputSpec,
    options: OptionsDto,
) -> Result<String> {
    // Reject unknown upload ids before allocating a workspace.
    if let InputSpec::Upload { upload_id } = &spec {
        if state.uploads.get(upload_id).is_none() {
            bail!("uploaded repository was not found or has expired");
        }
    }
    // Bound the registry so a flood cannot pin unbounded workspaces in memory.
    if state.jobs.len() >= state.limits.max_jobs {
        bail!("the server is at its job limit; try again later");
    }
    let mode = spec.mode();
    let label = format!("{mode:?}").to_ascii_lowercase();
    let workspace = state
        .workspaces
        .create(&label)
        .context("allocating workspace")?;
    let id = uuid::Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    let handle = state.jobs.insert(JobInner {
        id: id.clone(),
        input_mode: mode,
        status: JobStatus::Queued,
        created: Instant::now(),
        workspace: Some(workspace),
        archive: None,
        reports: Vec::new(),
        cancel,
    });

    tokio::spawn(async move {
        if let Err(err) = run_job(state.clone(), handle.clone(), spec, options).await {
            let mut inner = handle.lock().unwrap();
            if !inner.status.is_terminal() {
                inner.status = JobStatus::Failed {
                    message: format!("{err:#}"),
                };
            }
        }
    });
    Ok(id)
}

async fn run_job(
    state: Arc<AppState>,
    handle: Arc<Mutex<JobInner>>,
    spec: InputSpec,
    options: OptionsDto,
) -> Result<()> {
    let _slot = state
        .workspaces
        .acquire_slot()
        .await
        .context("acquiring slot")?;
    let workspace = handle
        .lock()
        .unwrap()
        .workspace
        .as_ref()
        .map(|dir| dir.path().to_path_buf())
        .context("workspace missing")?;
    let repo_dest = workspace.join("repo");
    let output_dir = workspace.join("output");
    std::fs::create_dir_all(&output_dir)?;

    let acquirer = Acquirer {
        runner: state.runner.clone(),
        resolver: state.resolver.clone(),
        uploads: state.uploads.clone(),
        integrations: state.integrations.clone(),
        allowed_local_roots: state.allowed_local_roots.clone(),
        max_repo_bytes: state.limits.max_repo_bytes,
        extraction_budget: crate::web::security::ExtractionBudget::default(),
    };

    set_status(
        &handle,
        JobStatus::Running {
            phase: "acquiring".into(),
            examined: 0,
            included: 0,
        },
    );
    let repo = acquirer.acquire(&spec, &repo_dest).await.map_err(|err| {
        match err.downcast::<AcquireError>() {
            Ok(err) => anyhow::Error::msg(err.to_string()),
            Err(err) => err,
        }
    })?;

    let file_name = match options.output_name.clone() {
        Some(name) => safe_output_name(&name).map_err(|err| anyhow::anyhow!(err.to_string()))?,
        None => sanitizer::default_output_path(
            &repo,
            options.format,
            options.compression,
            options.timestamp_name,
        )?
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sanitized-output".to_owned()),
    };
    let output = output_dir.join(&file_name);
    let config = options
        .to_config(repo, output.clone())
        .map_err(anyhow::Error::msg)?;

    let progress_handle = handle.clone();
    let cancel_flag = handle.lock().unwrap().cancel.clone();
    let cancel_for_check = cancel_flag.clone();
    let summary = tokio::task::spawn_blocking(move || {
        sanitizer::run_with_progress(
            config,
            move |event| {
                let status = match event {
                    ProgressEvent::Scanning { examined, .. } => JobStatus::Running {
                        phase: "scanning".into(),
                        examined,
                        included: 0,
                    },
                    ProgressEvent::Writing { included } => JobStatus::Running {
                        phase: "writing".into(),
                        examined: 0,
                        included,
                    },
                    ProgressEvent::Finished => JobStatus::Running {
                        phase: "finishing".into(),
                        examined: 0,
                        included: 0,
                    },
                };
                set_status(&progress_handle, status);
            },
            || cancel_flag.load(Ordering::Relaxed),
        )
    })
    .await
    .context("sanitization task panicked")??;

    if cancel_for_check.load(Ordering::Relaxed) {
        set_status(&handle, JobStatus::Cancelled);
        return Ok(());
    }

    let reports_dir = workspace.join("reports");
    let report_paths = extract_reports(&output, options.format, options.compression, &reports_dir)
        .unwrap_or_default();
    let report_names: Vec<String> = report_paths
        .iter()
        .filter_map(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();

    {
        let mut inner = handle.lock().unwrap();
        // A dry run writes no archive, so nothing is downloadable.
        inner.archive = if summary.dry_run {
            None
        } else {
            Some(output.clone())
        };
        inner.reports = report_paths;
        inner.status = JobStatus::Completed {
            included: summary.included,
            excluded: summary.excluded,
            redactions: summary.redactions,
            dry_run: summary.dry_run,
            archive: if summary.dry_run {
                None
            } else {
                Some(file_name)
            },
            reports: report_names,
        };
    }
    Ok(())
}

fn set_status(handle: &Arc<Mutex<JobInner>>, status: JobStatus) {
    let mut inner = handle.lock().unwrap();
    if !inner.cancel.load(Ordering::Relaxed) {
        inner.status = status;
    }
}
