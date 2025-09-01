//! MVP Sentiment Service — Binary Entrypoint
//! Boots the Axum HTTP server, wiring routes, shared state, and middleware.

use axum::{routing::get, Router};
use shuttle_axum::ShuttleAxum;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use dow_sentiment_analyzer::api;
use dow_sentiment_analyzer::relevance::{
    start_hot_reload_thread, AppState, RelevanceEngine, RelevanceHandle,
    DEFAULT_RELEVANCE_CONFIG_PATH, ENV_RELEVANCE_CONFIG_PATH,
};

fn enable_dev_tracing() {
    let dev_flag = std::env::var("RELEVANCE_DEV_LOG").ok().is_some_and(|v| v == "1");

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

#[shuttle_runtime::main]
async fn axum() -> ShuttleAxum {
    // .env (lokál) + dev tracing
    let _ = dotenvy::dotenv();
    enable_dev_tracing();

    // Rychlá AI sonda jen v lokálu/devu (na Shuttle se nespouští)
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

    // Relevance gate + hot-reload
    let engine = RelevanceEngine::from_toml().expect("Failed to load relevance config");
    let handle = RelevanceHandle::new(engine);
    let path = std::env::var(ENV_RELEVANCE_CONFIG_PATH)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_RELEVANCE_CONFIG_PATH));
    start_hot_reload_thread(handle.clone(), path);

    // API router (musí být Router<()>)
    let state = AppState { relevance: handle };
    let api_router: Router<()> = api::router(state);

    // Statická SPA — fallback (GET) na index.html
    let static_files = ServeDir::new("ui/dist")
        .append_index_html_on_directories(true)
        .not_found_service(ServeFile::new("ui/dist/index.html"));

    // Finální app:
    // - /health      => jednoduchý liveness
    // - /_version    => verze binárky
    // - /api/*       => celé API
    // - fallback     => SPA (GET)
    let app: Router<()> = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/_version", get(|| async { env!("CARGO_PKG_VERSION") }))
        .nest("/api", api_router)
        .fallback_service(static_files);

    Ok(app.into())
}
