//! MVP Sentiment Service — Binary Entrypoint
//! Boots the Axum HTTP server, wiring routes, shared state, and middleware.

use axum::Router;
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

fn spa_fallback_router() -> Router<()> {
    // Static SPA fallback:
    // - /assets/* a vše z ui/dist
    // - / a neexistující cesty vrací index.html pro SPA deep-link
    let static_files = ServeDir::new("ui/dist")
        .append_index_html_on_directories(true)
        .not_found_service(ServeFile::new("ui/dist/index.html"));

    Router::new().fallback_service(static_files)
}

#[shuttle_runtime::main]
async fn axum(
    #[shuttle_runtime::Secrets] secrets: shuttle_runtime::SecretStore,
) -> ShuttleAxum {
    let _ = dotenvy::dotenv();
    enable_dev_tracing();

    // --- Secrets -> env (pro kód, který čte std::env::var) ---
    if let Some(k) = secrets.get("OPENAI_API_KEY") {
        std::env::set_var("OPENAI_API_KEY", k);
    }
    if let Some(enabled) = secrets.get("AI_ENABLED") {
        std::env::set_var("AI_ENABLED", enabled);
    }
    if let Some(limit) = secrets.get("AI_DAILY_LIMIT") {
        std::env::set_var("AI_DAILY_LIMIT", limit);
    }
    // --- /Secrets ---

    // AI quick probe jen v lokálním/dev prostředí
    let shuttle_env = std::env::var("SHUTTLE_ENV")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(shuttle_env.as_str(), "local" | "development" | "dev") {
        if let Err(e) = dow_sentiment_analyzer::run_ai_quick_probe().await {
            tracing::warn!(error = ?e, "AI quick probe didn't run");
        }
    }

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

    // Sestavení aplikace:
    // - /api/* pro backend
    // - /_version pro rychlou kontrolu
    // - SPA fallback pro UI
    let app: Router<()> = Router::new()
        .nest("/api", api_router)
        .route(
            "/_version",
            axum::routing::get(|| async {
                format!("dow-sentiment-analyzer {}", env!("CARGO_PKG_VERSION"))
            }),
        )
        .merge(spa_fallback_router());

    Ok(app.into())
}
