//! Performance measurements for the web server (idle RSS, request latency,
//! browser payload size, acquisition/sanitize time, concurrent jobs, cleanup).
//!
//! These print metrics on every run and assert loose ceilings so a regression
//! that makes the server pathological fails CI.

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::Request;
use futures_util::future::BoxFuture;
use http_body_util::BodyExt;
use itsulu_repo_sanitizer_web::acquire::{CloneRunner, HostResolver};
use itsulu_repo_sanitizer_web::integrations::{Integrations, IntegrationsConfig};
use itsulu_repo_sanitizer_web::routes::build_router;
use itsulu_repo_sanitizer_web::state::AppState;
use itsulu_repo_sanitizer_web::uploads::UploadStore;
use itsulu_repo_sanitizer_web::workspace::{Limits, WorkspaceManager};
use tower::ServiceExt;

struct FakeRunner;
impl CloneRunner for FakeRunner {
    fn clone(&self, _argv: Vec<String>, _dest: PathBuf) -> BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

struct PublicResolver;
impl HostResolver for PublicResolver {
    fn resolve(&self, _host: String) -> BoxFuture<'static, Result<Vec<IpAddr>, String>> {
        Box::pin(async { Ok(vec!["1.1.1.1".parse().unwrap()]) })
    }
}

fn state(root: &std::path::Path, limits: Limits) -> Arc<AppState> {
    let workspaces = WorkspaceManager::new(root.join("workspaces"), limits.clone()).unwrap();
    let uploads = Arc::new(UploadStore::new(root.join("uploads")).unwrap());
    Arc::new(AppState::new(
        workspaces,
        uploads,
        Arc::new(Integrations::new(IntegrationsConfig::default())),
        None,
        vec![root.to_path_buf()],
        limits,
        Arc::new(FakeRunner),
        Arc::new(PublicResolver),
    ))
}

fn vm_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest
                .split_whitespace()
                .next()
                .and_then(|value| value.parse().ok());
        }
    }
    None
}

fn git_repo(root: &std::path::Path) -> PathBuf {
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let run = |args: &[&str]| {
        assert!(std::process::Command::new("git")
            .args(args)
            .current_dir(&repo)
            .status()
            .unwrap()
            .success());
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "t@example.com"]);
    run(&["config", "user.name", "T"]);
    std::fs::write(repo.join("a.txt"), "password: value\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    repo
}

#[tokio::test]
async fn measures_idle_memory_latency_and_bundle_size() {
    itsulu_repo_sanitizer_web::init_executor();
    let root = tempfile::tempdir().unwrap();
    let state = state(root.path(), Limits::default());
    let idle_rss = vm_rss_kib();

    // Warm up, then measure the capabilities endpoint.
    let start = Instant::now();
    let response = build_router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/capabilities")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let capability_latency = start.elapsed();
    assert!(bytes.len() < 256 * 1024);

    // Browser payload for the UI shell.
    let start = Instant::now();
    let page = build_router(state.clone())
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let html = page.into_body().collect().await.unwrap().to_bytes();
    let index_latency = start.elapsed();

    println!(
        "web-perf: idle_rss_kib={:?} capabilities_latency_us={} index_latency_us={} capabilities_bytes={} browser_html_bytes={}",
        idle_rss,
        capability_latency.as_micros(),
        index_latency.as_micros(),
        bytes.len(),
        html.len()
    );

    assert!(
        index_latency < Duration::from_secs(2),
        "index render too slow"
    );
    assert!(
        html.len() < 256 * 1024,
        "browser payload unexpectedly large"
    );
    assert!(html.len() > 1024, "browser payload unexpectedly small");
}

#[tokio::test]
async fn measures_acquisition_sanitize_and_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let repo = git_repo(root.path());
    let state = state(
        root.path(),
        Limits {
            max_concurrent_jobs: 2,
            ..Limits::default()
        },
    );

    // Launch several jobs at once to exercise bounded concurrency.
    let mut ids = Vec::new();
    let start = Instant::now();
    for _ in 0..4 {
        let id = itsulu_repo_sanitizer_web::jobs::start_job(
            state.clone(),
            itsulu_repo_sanitizer_web::dto::InputSpec::LocalPath {
                path: repo.to_string_lossy().into_owned(),
            },
            itsulu_repo_sanitizer_web::dto::OptionsDto::default(),
        )
        .await
        .unwrap();
        ids.push(id);
    }
    let spawn = start.elapsed();

    let mut completed = 0;
    for id in &ids {
        for _ in 0..400 {
            if let Some(job) = state.jobs.get(id) {
                if job.status.is_terminal() {
                    completed += 1;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    let total = start.elapsed();
    println!(
        "web-perf: concurrent_jobs={} spawn_us={} all_terminal_ms={} completed={}",
        ids.len(),
        spawn.as_micros(),
        total.as_millis(),
        completed
    );
    assert_eq!(completed, ids.len(), "all concurrent jobs must finish");

    // Deterministic cleanup: expiring terminal jobs drops their workspaces.
    let removed = state.jobs.cleanup_expired(Duration::ZERO);
    assert_eq!(removed, ids.len());
    assert_eq!(state.jobs.len(), 0);
    // Workspace directories for finished jobs are gone (allow the runner
    // tasks a moment to drop their handles).
    let mut leftovers = usize::MAX;
    for _ in 0..50 {
        leftovers = std::fs::read_dir(root.path().join("workspaces"))
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .count();
        if leftovers == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(leftovers, 0, "workspaces must be cleaned after expiry");
}
