//! MVP Sentiment Service — Binary Entrypoint
//! Boots the Axum HTTP server, wiring routes, shared state, and middleware.

use axum::{routing::get_service, Router};
use shuttle_axum::ShuttleAxum;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use dow_sentiment_analyzer::relevance::{
    start_hot_reload_thread, AppState, RelevanceEngine, RelevanceHandle,
    DEFAULT_RELEVANCE_CONFIG_PATH, ENV_RELEVANCE_CONFIG_PATH,
};
use dow_sentiment_analyzer::api;

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

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().compact())
        .try_init();
}

fn ui_router() -> Router<()> {
    // GET-only static serving for SPA
    let assets = ServeDir::new("ui/dist/assets");
    let index = ServeFile::new("ui/dist/index.html");

    Router::new()
        .nest_service("/assets", assets)
        .route("/", get_service(index.clone()))
        .route("/*path", get_service(index)) // wildcard for SPA refresh
}

#[shuttle_runtime::main]
async fn axum() -> ShuttleAxum {
    let _ = dotenvy::dotenv();
    enable_dev_tracing();

    // --- AI quick probe (local/dev only) ---
    if matches!(
        std::env::var("SHUTTLE_ENV")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "local" | "development" | "dev"
    ) {
        if let Err(e) = dow_sentiment_analyzer::run_ai_quick_probe().await {
            tracing::warn!(error = ?e, "AI quick probe didn't run");
        }
    }
    // --- /AI quick probe ---

    // Relevance gate
    let engine = RelevanceEngine::from_toml().expect("Failed to load relevance config");
    let handle = RelevanceHandle::new(engine);
    let path = std::env::var(ENV_RELEVANCE_CONFIG_PATH)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_RELEVANCE_CONFIG_PATH));
    start_hot_reload_thread(handle.clone(), path);

    // API router — MUSÍ vracet Router<()>
    let state = AppState { relevance: handle };
    let api_router: Router<()> = api::router(state);

    // Merge API + UI — vše Router<()>
    let app: Router<()> = Router::new().merge(api_router).merge(ui_router());

    // Shuttle očekává Axum service; From je implementováno pro Router<()>
    Ok(app.into())
}
