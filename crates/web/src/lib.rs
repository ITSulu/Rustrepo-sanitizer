//! Axum/Leptos web frontend for Rustrepo-sanitizer.
//!
//! The web layer reuses the shared sanitizer core (`itsulu-repo-sanitizer`) so
//! there is exactly one implementation of acquisition-agnostic sanitization.

/// Initializes the executor Leptos SSR dispatches onto. Idempotent and safe to
/// call from the server entrypoint or tests.
pub fn init_executor() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = any_spawner::Executor::init_tokio();
    });
}

pub mod acquire;
pub mod dto;
pub mod integrations;
pub mod jobs;
pub mod reports;
pub mod routes;
pub mod security;
pub mod state;
pub mod ui;
pub mod uploads;
pub mod workspace;
