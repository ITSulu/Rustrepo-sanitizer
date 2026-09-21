//! `rustrepo-sanitizer-web` server entrypoint.

use std::sync::Arc;
use std::time::Duration;

use itsulu_repo_sanitizer_web::routes::build_router;
use itsulu_repo_sanitizer_web::state::{default_bind, workspace_root, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let bind = std::env::var("RUSTREPO_WEB_BIND")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(default_bind);
    // Leptos SSR dispatches work onto this executor.
    itsulu_repo_sanitizer_web::init_executor();
    let state = Arc::new(AppState::from_env(workspace_root())?);

    // Deterministic startup cleanup of any workspace or upload left by a
    // previous process.
    let _ = state.workspaces.cleanup_stale();
    state.uploads.cleanup_expired(Duration::from_secs(60 * 60));

    let cleanup_state = state.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(300)).await;
            let _ = cleanup_state.workspaces.cleanup_stale();
            cleanup_state
                .jobs
                .cleanup_expired(Duration::from_secs(60 * 60));
            cleanup_state
                .uploads
                .cleanup_expired(Duration::from_secs(60 * 60));
        }
    });

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    eprintln!("rustrepo-sanitizer-web listening on http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}
