//! MVP Sentiment Service — Binary Entrypoint
//! Boots the Axum HTTP server, wiring routes, shared state, and middleware.

use axum::{response::IntoResponse, routing::get, Router};
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

async fn root_health() -> impl IntoResponse {
    "ok"
}

async fn version() -> impl IntoResponse {
    // jednoduchá verze: jen číslo + build-time UTC timestamp (pokud chceš víc, doplň sem)
    let v = env!("CARGO_PKG_VERSION");
    format!("{} (service: dow-sentiment-analyzer)", v)
}

#[shuttle_runtime::main]
async fn axum() -> ShuttleAxum {
    let _ = dotenvy::dotenv();
    enable_dev_tracing();

    // --- AI quick probe (jen lokal/dev; na Shuttle se nespouští) ---
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

    // Relevance gate – tvrdý start; když config chybí, raději logni a spusť degraded režim než spadnout (žádné 502)
    let engine = match RelevanceEngine::from_toml() {
        Ok(e) => e,
        Err(e) => {
            tracing::error!(error=?e, "Failed to load relevance config - starting in degraded mode");

            // SPA fallback
            let static_files = ServeDir::new("ui/dist")
                .append_index_html_on_directories(true)
                .not_found_service(ServeFile::new("ui/dist/index.html"));

            let degraded: Router<()> = Router::new()
                .route("/health", get(|| async { "degraded" }))
                .route("/_version", get(version))
                .fallback_service(static_files);

            return Ok(degraded.into());
        }
    };
    let handle = RelevanceHandle::new(engine);
    let path = std::env::var(ENV_RELEVANCE_CONFIG_PATH)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_RELEVANCE_CONFIG_PATH));
    start_hot_reload_thread(handle.clone(), path);

    // API router — MUSÍ vracet Router<()>; očekáváme, že definice uvnitř NEMAJÍ prefix "/api" (tj. route("/health"), route("/decide"), …)
    let state = AppState { relevance: handle };
    let api_router: Router<()> = api::router(state);

    // Static SPA fallback:
    // - ServeDir na "ui/dist" (včetně /assets/*)
    // - / vrací index.html (append_index_html_on_directories)
    // - neexistující cesty spadnou na index.html (SPA deep-link)
    let static_files = ServeDir::new("ui/dist")
        .append_index_html_on_directories(true)
        .not_found_service(ServeFile::new("ui/dist/index.html"));

    // Merge API + UI — vše Router<()>
    let app: Router<()> = Router::new()
        .route("/health", get(root_health))
        .route("/_version", get(version))
        // diagnostický ping přímo tady, abychom ověřili, že /api mount funguje i bez api::router
        .route("/api/ping", get(|| async { "pong" }))
        .nest("/api", api_router)
        .fallback_service(static_files);

    Ok(app.into())
}
