//! MVP Sentiment Service — Binary Entrypoint
//! Boots the Axum HTTP server, wiring routes, shared state, and middleware.

use axum::{routing::get, routing::get_service, Router};
use shuttle_axum::ShuttleAxum;
use shuttle_runtime::SecretStore;
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

/// Jednoduchý UI router (SPA) — funguje i v Axum 0.8.
/// - `/assets/*` obslouží statická aktiva
/// - `/` i jakákoli jiná cesta vrací `index.html` (deep-linky)
fn ui_router() -> Router<()> {
    let dist = "ui/dist";
    let assets = ServeDir::new(format!("{dist}/assets"));
    let index = ServeFile::new(format!("{dist}/index.html"));

    Router::new()
        .nest_service("/assets", assets)
        .route("/", get_service(index.clone()))
        // POZOR: v Axum 0.8 už ne `/*path`, ale `/{*path}`
        .route("/{*path}", get_service(index))
}

#[shuttle_runtime::main]
async fn axum(
    #[shuttle_runtime::Secrets] secrets: SecretStore,
) -> ShuttleAxum {
    // .env v lokálu, na Shuttle přijdou Secrets.
    let _ = dotenvy::dotenv();
    enable_dev_tracing();

    // Přehrajme OPENAI_API_KEY a AI_ENABLED z Shuttle secrets do process env (pokud jsou).
    if let Ok(Some(k)) = secrets.get("OPENAI_API_KEY") {
        std::env::set_var("OPENAI_API_KEY", k);
    }
    if let Ok(Some(enabled)) = secrets.get("AI_ENABLED") {
        std::env::set_var("AI_ENABLED", enabled);
    }

    // --- AI quick probe (jen local/dev) ---
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

    // API router (Router<()>)
    let state = AppState { relevance: handle };
    let api_router: Router<()> = api::router(state);

    // Složení aplikace:
    //  - `/api/*` -> naše API
    //  - `/_version` -> rychlá sonda, že běží NÁŠ Router (ne Shuttle default)
    //  - vše ostatní -> SPA (index.html + assets)
    let app: Router<()> = Router::new()
        .route("/_version", get(|| async { "app-alive" }))
        .nest("/api", api_router)
        .merge(ui_router());

    Ok(app.into())
}
