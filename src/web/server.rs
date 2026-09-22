//! Web server lifecycle for the unified binary.
//!
//! The server can run alone (web-only mode) or on a background thread while the
//! Slint GUI owns the main thread (GUI + Web). It never spawns another process.

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use crate::web::routes::build_router;
use crate::web::state::{AppState, WebSettings};

/// Start-of-run cleanup and the periodic sweeper. Idempotent.
fn start_cleanup(state: &Arc<AppState>) {
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
}

/// Serves the web UI/API until `shutdown` resolves.
pub async fn serve(
    state: Arc<AppState>,
    bind: SocketAddr,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<()> {
    crate::web::init_executor();
    start_cleanup(&state);
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(bind).await?;
    eprintln!("Rustrepo-sanitizer web UI listening on http://{bind}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await?;
    Ok(())
}

/// Resolves when an interactive termination signal arrives (Ctrl-C or SIGTERM).
async fn termination_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("installing SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = term.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

/// Runs the web UI only, blocking the current thread until Ctrl-C or SIGTERM.
pub fn run_web_only(settings: WebSettings) -> Result<()> {
    let state = Arc::new(AppState::from_settings(&settings)?);
    let bind = settings.bind;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let shutdown = async {
            termination_signal().await;
        };
        serve(state, bind, shutdown).await
    })
}

/// Starts the web UI on a background thread and returns a shutdown trigger.
///
/// Used by GUI + Web mode so neither interface blocks the other.
pub fn spawn_web(
    settings: WebSettings,
) -> Result<(
    std::thread::JoinHandle<Result<()>>,
    tokio::sync::oneshot::Sender<()>,
)> {
    let state = Arc::new(AppState::from_settings(&settings)?);
    let bind = settings.bind;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let handle = std::thread::Builder::new()
        .name("Rustrepo-sanitizer-web".to_owned())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;
            runtime.block_on(async move {
                serve(state, bind, async move {
                    let _ = rx.await;
                })
                .await
            })
        })?;
    Ok((handle, tx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    fn free_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn wait_health(port: u16) {
        for _ in 0..100 {
            if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
                let _ = stream.write_all(
                    b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                );
                let mut buffer = String::new();
                let _ = stream.read_to_string(&mut buffer);
                if buffer.contains("200") {
                    return;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        panic!("web server did not become healthy");
    }

    /// The GUI + Web concurrency mechanism: the server serves on a background
    /// thread while the caller is free, and a shutdown trigger stops it.
    #[test]
    fn spawn_web_serves_on_a_thread_and_stops_cleanly() {
        let root = tempfile::tempdir().unwrap();
        let port = free_port();
        let settings = WebSettings {
            bind: format!("127.0.0.1:{port}").parse().unwrap(),
            root: root.path().to_path_buf(),
            ..WebSettings::default()
        };
        let (handle, shutdown) = spawn_web(settings).unwrap();
        wait_health(port);
        // The main thread is not blocked while the server runs.
        assert!(!handle.is_finished());
        let _ = shutdown.send(());
        assert!(handle.join().unwrap().is_ok());
    }
}
