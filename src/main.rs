//! Shuttle entrypoint: build top-level Axum app, serve the SPA UI + static assets,
//! mount the existing API under /api, start the ingest scheduler, and load secrets.
//! All public comments are in English.

use serde::Serialize;
use shuttle_axum::axum::{
    extract::Path,
    http::{header, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use shuttle_axum::ShuttleAxum;
use shuttle_runtime::SecretStore;
use std::{path::PathBuf, time::Duration};
use tower::ServiceBuilder;
use tower_http::{compression::CompressionLayer, trace::TraceLayer};
use tracing::{info, warn};

// ----- UI embedding -----
// Embed the ALREADY-BUNDLED SPA entry from ./assets (present both locally and in Shuttle build assets)
const INDEX_HTML: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/index.html"));

#[derive(Serialize)]
struct VersionInfo {
    name: &'static str,
    version: &'static str,
}

async fn version() -> shuttle_axum::axum::Json<VersionInfo> {
    shuttle_axum::axum::Json(VersionInfo {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn health() -> &'static str {
    "ok"
}

/// Return the embedded index.html with a no-store cache directive.
/// Assets are long-cached; index is never cached to always pick the latest hashed filenames.
fn index_response() -> impl IntoResponse {
    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8")),
        (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
    ];
    (headers, Html(INDEX_HTML))
}

async fn index_html() -> impl IntoResponse {
    index_response()
}

// ----- Secrets -----

/// Load selected keys from Shuttle `SecretStore` into process env so downstream code can `std::env::var` them.
fn load_secrets_into_env(secrets: &SecretStore) {
    const KEYS: &[&str] = &[
        // AI
        "AI_PROVIDER",
        "OPENAI_API_KEY",
        "OPENAI_MODEL",
        "OPENAI_API_BASE",
        "AI_ONLY_TOP_SOURCES",
        "AI_SCORE_BAND",
        "AI_DECISION_CACHE_TTL_MS",
        "AI_SOURCES",
        "AI_TEST_MODE",
        // Ingest
        "INGEST_ENABLED",
        "INGEST_INTERVAL_SECS",
        "INGEST_DEDUP_WINDOW_SECS",
        "INGEST_WHITELIST_PATH",
        // CORS (if used in API)
        "ALLOWED_ORIGINS",
    ];
    for k in KEYS {
        if let Some(v) = secrets.get(k) {
            std::env::set_var(k, v);
        }
    }
}

// ----- Static file helpers (replace ServeDir/ServeFile to avoid handle_error type issues) -----

fn sanitize_segment(seg: &str) -> bool {
    // Reject dangerous segments to prevent path traversal
    !seg.is_empty() && seg != "." && seg != ".." && !seg.contains('\\')
}

async fn read_file_response(full: PathBuf, cache_control: &'static str) -> Response {
    match tokio::fs::read(&full).await {
        Ok(bytes) => {
            // Guess content-type with fallback to octet-stream
            let guessed = mime_guess::from_path(&full).first_or_octet_stream();
            let ct = HeaderValue::from_str(guessed.as_ref())
                .unwrap_or(HeaderValue::from_static("application/octet-stream"));

            let mut resp =
                ([(header::CACHE_CONTROL, HeaderValue::from_static(cache_control))], bytes)
                    .into_response();
            resp.headers_mut().insert(header::CONTENT_TYPE, ct);
            resp
        }
        Err(_) => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

async fn assets_handler(Path(path): Path<String>) -> Response {
    // Only allow safe segments
    let safe: Vec<&str> = path.split('/').filter(|s| sanitize_segment(s)).collect();
    let full = PathBuf::from("assets").join(safe.join("/"));
    // Long immutable cache for hashed assets
    read_file_response(full, "public, max-age=31536000, immutable").await
}

async fn favicon_handler() -> Response {
    // Try root favicon.ico first (if present), then assets/favicon.ico, else 404
    if tokio::fs::metadata("favicon.ico").await.is_ok() {
        read_file_response(PathBuf::from("favicon.ico"), "public, max-age=86400").await
    } else {
        read_file_response(PathBuf::from("assets/favicon.ico"), "public, max-age=86400").await
    }
}

// ----- Ingest scheduler (LIVE providers) -----

use dow_sentiment_analyzer::{
    api,
    ingest::{self, providers::{fed_rss::FedRssProvider, reuters_rss::ReutersRssProvider}, types::SourceProvider},
    relevance::AppState,
};

async fn run_ingest_scheduler(_app_state: AppState) {
    let enabled = std::env::var("INGEST_ENABLED").unwrap_or_else(|_| "false".into()) == "true";
    if !enabled {
        info!("ingest scheduler disabled (INGEST_ENABLED=false)");
        return;
    }

    let interval_secs: u64 = std::env::var("INGEST_INTERVAL_SECS")
        .ok().and_then(|s| s.parse().ok()).unwrap_or(300);

    let dedup_window_secs: u64 = std::env::var("INGEST_DEDUP_WINDOW_SECS")
        .ok().and_then(|s| s.parse().ok()).unwrap_or(600);

    info!("starting ingest scheduler (every {interval_secs}s)");
    let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));

    loop {
        ticker.tick().await;

        // Load whitelist (env path -> TOML -> JSON -> empty)
        let whitelist = ingest::config::load_whitelist_default().unwrap_or_default();

        // Build providers (network)
        let providers: Vec<Box<dyn SourceProvider>> = vec![
            Box::new(FedRssProvider::new()),
            Box::new(ReutersRssProvider::new()),
        ];

        // Run pipeline once
        let (kept, filtered, dedup) =
            ingest::run_once(&providers, &whitelist, dedup_window_secs).await;

        info!(
            kept = kept.len(),
            filtered = filtered,
            dedup = dedup,
            "ingest tick finished"
        );
    }
}

// ----- Shuttle entrypoint -----

#[shuttle_runtime::main]
async fn axum(
    #[shuttle_runtime::Secrets] secrets: SecretStore,
) -> ShuttleAxum {
    // 1) Make secrets available to the rest of the code (AI adapter, etc.)
    load_secrets_into_env(&secrets);

    // 2) Build the existing library API router and mount it under /api
    //    IMPORTANT: do not modify `src/api.rs`; we reuse its public `router(...)`.
    let state = AppState::from_env();
    let api_router = api::router(state.clone());

    // 3) Start ingest scheduler
    tokio::spawn(run_ingest_scheduler(state));

    // 4) Compose the top-level app: UI + system endpoints + nested API
    let app = Router::new()
        // UI + static
        .route("/", get(index_html))
        .route("/assets/*path", get(assets_handler)) // <-- Axum wildcard syntax
        .route("/favicon.ico", get(favicon_handler))
        // SPA fallback
        .fallback(get(index_html))
        // System endpoints
        .route("/_version", get(version))
        .route("/_health", get(health))
        // API mounted under /api
        .nest("/api", api_router)
        // Useful layers
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(CompressionLayer::new()),
        );

    Ok(app.into())
}
