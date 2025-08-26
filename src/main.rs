//! MVP Sentiment Service — Binary Entrypoint
//! Boots the Axum HTTP server, wiring routes, shared state, and middleware.
//!
//! See `README.md` for quickstart and `docs/` for architecture notes.

use shuttle_axum::axum::Router;
use shuttle_axum::ShuttleAxum;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use dow_sentiment_analyzer::api;
use dow_sentiment_analyzer::relevance::{
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

    // Important: use `try_init()` so we don't panic under Shuttle which already sets a global subscriber.
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("relevance=info,warn"));

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().compact())
        .try_init();
}

#[shuttle_runtime::main]
async fn axum() -> ShuttleAxum {
    // Load .env in local/dev; no-op in prod environments.
    let _ = dotenvy::dotenv();

    // Initialize dev tracing early (no-op in production or if already set by Shuttle).
    enable_dev_tracing();

    // --- Initialize relevance gate ---
    let engine = RelevanceEngine::from_toml().expect("Failed to load relevance config");
    let handle = RelevanceHandle::new(engine);

    // If hot reload is enabled, spawn background watcher
    let path = std::env::var(ENV_RELEVANCE_CONFIG_PATH)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_RELEVANCE_CONFIG_PATH));
    start_hot_reload_thread(handle.clone(), path);

    // Build AppState and pass it into the API router
    let state = AppState { relevance: handle };
    let api_router = api::create_router(state);

    // Serve the compiled UI (ui/dist) at "/" with SPA fallback to index.html
    let static_dir = ServeDir::new("ui/dist")
        .not_found_service(ServeFile::new("ui/dist/index.html"));

    let app = Router::new()
        .nest("/api", api_router)
        .fallback_service(static_dir);

    Ok(app.into())
}