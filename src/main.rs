//! Shuttle entrypoint: build the top-level Axum app, serve the SPA UI + static assets,
//! mount the existing API under /api, start the ingest scheduler, and load secrets.
//! All publicly visible comments are in English.

use shuttle_axum::axum::{
    extract::Path,
    http::{header, HeaderName, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use shuttle_axum::ShuttleAxum;
use shuttle_runtime::SecretStore;

// --- redirect middleware imports ---
use shuttle_axum::axum::extract::Request;
use shuttle_axum::axum::middleware::{self, Next}; // správný alias v axum 0.8

use std::{path::PathBuf, time::Duration};
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    limit::RequestBodyLimitLayer, // requires tower-http feature "limit"
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};
use tracing::info;

// ----- UI embedding -----
const INDEX_HTML: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/index.html"));

// ----- Security headers (constants) -----
const CSP_VALUE: &str = "\
default-src 'self'; \
script-src 'self'; \
style-src 'self' 'unsafe-inline'; \
img-src 'self' data:; \
font-src 'self' data:; \
connect-src 'self'; \
frame-ancestors 'none'; \
base-uri 'none'; \
object-src 'none'";

const PERMISSIONS_POLICY_VALUE: &str = "geolocation=(), camera=(), microphone=()";
const HSTS_VALUE: &str = "max-age=31536000; includeSubDomains";

// ----- Secrets -----
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
        // CORS
        "ALLOWED_ORIGINS",
        // Observability
        "METRICS_ENABLED",
        // Security / limits
        "HSTS_ENABLED",
        "API_BODY_LIMIT_BYTES",
        // Optional providers
        "INGEST_ENABLE_REUTERS",
    ];
    for k in KEYS {
        if let Some(v) = secrets.get(k) {
            std::env::set_var(k, v);
        }
    }
}

// ----- Static file helpers -----
fn sanitize_segment(seg: &str) -> bool {
    !seg.is_empty() && seg != "." && seg != ".." && !seg.contains('\\')
}

async fn read_file_response(full: PathBuf, cache_control: &'static str) -> Response {
    match tokio::fs::read(&full).await {
        Ok(bytes) => {
            let guessed = mime_guess::from_path(&full).first_or_octet_stream();
            let ct = HeaderValue::from_str(guessed.as_ref())
                .unwrap_or(HeaderValue::from_static("application/octet-stream"));

            let mut resp = (
                [(header::CACHE_CONTROL, HeaderValue::from_static(cache_control))],
                bytes,
            )
                .into_response();
            resp.headers_mut().insert(header::CONTENT_TYPE, ct);
            resp
        }
        Err(_) => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

async fn assets_handler(Path(path): Path<String>) -> Response {
    let safe: Vec<&str> = path.split('/').filter(|s| sanitize_segment(s)).collect();
    let full = PathBuf::from("assets").join(safe.join("/"));
    read_file_response(full, "public, max-age=31536000, immutable").await
}

async fn config_handler(Path(path): Path<String>) -> Response {
    // Statické konfigy – no-store, aby se necacheoval JSON/TOML
    let safe: Vec<&str> = path.split('/').filter(|s| sanitize_segment(s)).collect();
    let full = PathBuf::from("config").join(safe.join("/"));
    read_file_response(full, "no-store").await
}

async fn favicon_handler() -> Response {
    if tokio::fs::metadata("favicon.ico").await.is_ok() {
        read_file_response(PathBuf::from("favicon.ico"), "public, max-age=86400").await
    } else {
        read_file_response(PathBuf::from("assets/favicon.ico"), "public, max-age=86400").await
    }
}

fn index_response() -> impl IntoResponse {
    let headers = [
        (
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        ),
        (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
    ];
    (headers, Html(INDEX_HTML))
}

async fn index_html() -> impl IntoResponse {
    index_response()
}

// ----- Ingest scheduler -----
use dow_sentiment_analyzer::{
    api,
    ingest::{
        self,
        providers::{fed_rss::FedRssProvider, reuters_rss::ReutersRssProvider},
        types::SourceProvider,
    },
    relevance::AppState,
    versions,
};
use shuttle_shared_db::SerdeJsonOperator;

// simple env truthy helper with default
fn env_truthy(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default,
    }
}

async fn run_ingest_scheduler(_app_state: AppState) {
    let enabled = std::env::var("INGEST_ENABLED").unwrap_or_else(|_| "false".into()) == "true";
    if !enabled {
        info!("ingest scheduler disabled (INGEST_ENABLED=false)");
        return;
    }

    let interval_secs: u64 = std::env::var("INGEST_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);

    let dedup_window_secs: u64 = std::env::var("INGEST_DEDUP_WINDOW_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(600);

    info!("starting ingest scheduler (every {interval_secs}s)");
    let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));

    loop {
        ticker.tick().await;

        let whitelist = ingest::config::load_whitelist_default().unwrap_or_default();

        let mut providers: Vec<Box<dyn SourceProvider>> = vec![
            Box::new(FedRssProvider::new()),
        ];
        // Volitelně Reuters (default ON); vypneš přes INGEST_ENABLE_REUTERS=false
        if env_truthy("INGEST_ENABLE_REUTERS", true) {
            providers.push(Box::new(ReutersRssProvider::new()));
        }

        let (kept, filtered, dedup) =
            ingest::run_once(&providers, &whitelist, dedup_window_secs).await;

        // **NEW**: zapiš poslední evidence pro /api/debug/ingest
        dow_sentiment_analyzer::api::debug_ingest_record(&kept);

        info!(
            kept = kept.len(),
            filtered = filtered,
            dedup = dedup,
            "ingest tick finished"
        );
    }
}

// 301 redirect for apex -> www, preserves path+query (axum 0.8 signature)
async fn redirect_apex_to_www(req: Request, next: Next) -> Response {
    let host = req
        .headers()
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if host.eq_ignore_ascii_case("dowsentiment.app") {
        let pq = req
            .uri()
            .path_and_query()
            .map(|x| x.as_str())
            .unwrap_or("/");
        let loc = format!("https://www.dowsentiment.app{}", pq);
        return (
            StatusCode::MOVED_PERMANENTLY,
            [(header::LOCATION, HeaderValue::from_str(&loc).unwrap())],
        )
            .into_response();
    }
    next.run(req).await
}

// ----- Shuttle entrypoint -----
#[shuttle_runtime::main]
async fn axum(
    #[shuttle_runtime::Secrets] secrets: SecretStore,
    #[shuttle_shared_db::Postgres] last_store: SerdeJsonOperator,
) -> ShuttleAxum {
    // 1) Secrets -> env
    load_secrets_into_env(&secrets);

    // 2) App state + API router
    let app_state = AppState::from_env();
    let last = dow_sentiment_analyzer::last::LastStore::new(last_store);
    let api_router = api::router_with_last(app_state.clone(), last).without_v07_checks();

    // Optional: body limit for all /api (covers /api/decide)
    let api_body_limit_bytes: usize = std::env::var("API_BODY_LIMIT_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(131_072); // 128 KiB

    let api_router = if api_body_limit_bytes > 0 {
        api_router.layer(RequestBodyLimitLayer::new(api_body_limit_bytes))
    } else {
        api_router
    };

    // 3) Ingest scheduler
    tokio::spawn(run_ingest_scheduler(app_state));

    // 4) Security & utility layers
    let hsts_enabled = std::env::var("HSTS_ENABLED")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let sec = ServiceBuilder::new()
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static(PERMISSIONS_POLICY_VALUE),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static(CSP_VALUE),
        ))
        // Conditional HSTS:
        .option_layer(if hsts_enabled {
            Some(SetResponseHeaderLayer::if_not_present(
                HeaderName::from_static("strict-transport-security"),
                HeaderValue::from_static(HSTS_VALUE),
            ))
        } else {
            None
        });

    // 5) Top-level router (order matters: redirect first, then security)
    let mut app = Router::new()
        // System endpoints
        .route("/_version", get(versions::handler))
        .route("/_health", get(|| async { "ok" }))
        // Static
        .route("/favicon.ico", get(favicon_handler))
        .route("/assets/{*path}", get(assets_handler))
        .route("/config/{*path}", get(config_handler)) // **NEW** – statické konfigy
        // UI + SPA fallback
        .route("/", get(index_html))
        .route("/{*path}", get(index_html))
        // API
        .nest("/api", api_router)
        // 1) inner: apex -> www redirect
        .layer(middleware::from_fn(redirect_apex_to_www))
        // 2) outer: security + utility headers (applies also to 301)
        .layer(sec);

    // Optional metrics at /metrics
    let metrics_enabled = std::env::var("METRICS_ENABLED")
        .ok()
        .map(|v| v == "1")
        .unwrap_or(false);
    if metrics_enabled {
        // Use the tiny helper router from crate::metrics
        app = app.merge(dow_sentiment_analyzer::metrics::router());
    }

    let app = app.without_v07_checks();
    Ok(app.into())
}
