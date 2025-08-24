//! MVP Sentiment Service — Binary Entrypoint
//! Boots the Axum HTTP server, wiring routes, shared state, and middleware.
//!
//! See `README.md` for quickstart and `docs/` for architecture notes.

mod api;
pub mod debug;
mod decision;
mod disruption;
mod engine;
mod history;
mod relevance;
mod rolling;
mod sentiment;
mod source_weights;

use shuttle_axum::ShuttleAxum;
use std::path::PathBuf;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

// bring handle + reload + AppState in scope
use crate::relevance::{
    start_hot_reload_thread, AppState, RelevanceEngine, RelevanceHandle,
    DEFAULT_RELEVANCE_CONFIG_PATH, ENV_RELEVANCE_CONFIG_PATH,
};

/// Enable compact tracing logs in development only.
/// Activation requires BOTH:
///   - dev environment (debug build OR SHUTTLE_ENV in {local, development, dev})
///   - RELEVANCE_DEV_LOG=1
fn enable_dev_tracing() {
    let dev_flag = std::env::var("RELEVANCE_DEV_LOG")
        .ok()
        .is_some_and(|v| v == "1");

    let is_dev_env = cfg!(debug_assertions)
        || matches!(
            std::env::var("SHUTTLE_ENV")
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "local" | "development" | "dev"
        );

    if !(dev_flag && is_dev_env) {
        return;
    }

    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("relevance=info,warn"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().compact())
        .init();
}

#[shuttle_runtime::main]
async fn axum() -> ShuttleAxum {
    // Load .env in local/dev; no-op in prod environments.
    // This enables RELEVANCE_CONFIG_PATH / RELEVANCE_THRESHOLD from .env
    // so relevance.rs can pick them up.
    let _ = dotenvy::dotenv();

    // Initialize dev tracing early (no-op in production).
    enable_dev_tracing();

    // --- Initialize relevance gate ---
    let engine = RelevanceEngine::from_toml().expect("Failed to load relevance config");
    let handle = RelevanceHandle::new(engine);

    // If hot reload is enabled, spawn background watcher
    let path = std::env::var(ENV_RELEVANCE_CONFIG_PATH)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_RELEVANCE_CONFIG_PATH));
    start_hot_reload_thread(handle.clone(), path);

    // Build AppState and pass it into the router
    let state = AppState { relevance: handle };
    let router = api::create_router(state);

    Ok(router.into())
}
