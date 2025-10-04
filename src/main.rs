//! Shuttle entrypoint: build the top-level Axum app, serve the SPA UI + static assets,
//! mount the existing API under /api, start the ingest scheduler, and load secrets.
//! All publicly visible comments are in English.

use shuttle_axum::axum::{
    extract::Path,
    http::{header, HeaderName, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, // used for /_diag/env when DEBUG_ROUTES=1
    Router,
};
use shuttle_axum::ShuttleAxum;
use shuttle_runtime::SecretStore;

// --- redirect middleware imports (axum 0.8) ---
use shuttle_axum::axum::extract::Request;
use shuttle_axum::axum::middleware::{self, Next};

use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
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
/// Hydrate a selected set of *real* secrets into the process env for libs that only read env.
/// (Config toggles se čtou níže přes "env or secret" fallback; není nutné je sem přidávat.)
fn load_secrets_into_env(secrets: &SecretStore) {
    const KEYS: &[&str] = &[
        "OPENAI_API_KEY",
        "DISCORD_WEBHOOK_URL",
        "SLACK_WEBHOOK",
        "SMTP_PASSWORD",
        "DEBUG_ROUTES",
        // add other real secrets if you have them
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
                [(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static(cache_control),
                )],
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
    // Static configs – no-store to avoid caching JSON/TOML
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

async fn apple_touch_handler() -> Response {
    if tokio::fs::metadata("apple-touch-icon.png").await.is_ok() {
        read_file_response(
            PathBuf::from("apple-touch-icon.png"),
            "public, max-age=86400",
        )
        .await
    } else {
        read_file_response(
            PathBuf::from("assets/apple-touch-icon.png"),
            "public, max-age=86400",
        )
        .await
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
        providers::{fed_rss::FedRssProvider, generic_rss::GenericRssProvider, reuters_rss::ReutersRssProvider},
        types::SourceProvider,
    },
    relevance::AppState,
    versions,
};
use shuttle_shared_db::SerdeJsonOperator;

// ---- small config helpers (env first, then Shuttle secrets) ----
fn parse_bool(s: &str) -> bool {
    matches!(s.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

// simple env truthy helper with default
fn env_truthy(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => parse_bool(&v),
        Err(_) => default,
    }
}

// fetch bool from env OR secrets (fallback), with default
fn get_bool_env_or_secret(secrets: &SecretStore, key: &str, default: bool) -> bool {
    if let Ok(v) = std::env::var(key) {
        return parse_bool(&v);
    }
    if let Some(v) = secrets.get(key) {
        return parse_bool(&v);
    }
    default
}

// fetch u64 from env OR secrets (fallback), with default
fn get_u64_env_or_secret(secrets: &SecretStore, key: &str, default: u64) -> u64 {
    if let Ok(v) = std::env::var(key) {
        if let Ok(n) = v.parse::<u64>() {
            return n;
        }
    }
    if let Some(v) = secrets.get(key) {
        if let Ok(n) = v.parse::<u64>() {
            return n;
        }
    }
    default
}

#[derive(Clone, Debug)]
struct IngestConfig {
    enabled: bool,
    interval_secs: u64,
    dedup_window_secs: u64,
    enable_reuters: bool,
    enable_generic: bool,
}

async fn run_ingest_scheduler(_app_state: AppState, cfg: IngestConfig) {
    if !cfg.enabled {
        info!("ingest scheduler disabled (INGEST_ENABLED=false)");
        return;
    }

    info!(
        "ingest scheduler enabled (interval={}s, dedup_window={}s, reuters={}, generic={})",
        cfg.interval_secs, cfg.dedup_window_secs, cfg.enable_reuters, cfg.enable_generic
    );

    let mut ticker = tokio::time::interval(Duration::from_secs(cfg.interval_secs));

    loop {
        ticker.tick().await;

        let whitelist = ingest::config::load_whitelist_default().unwrap_or_default();

        let mut providers: Vec<Box<dyn SourceProvider>> = vec![Box::new(FedRssProvider::new())];

        if cfg.enable_reuters {
            providers.push(Box::new(ReutersRssProvider::new()));
        }
        if cfg.enable_generic {
            providers.push(Box::new(GenericRssProvider::new()));
        }

        let (kept, filtered, dedup) =
            ingest::run_once(&providers, &whitelist, cfg.dedup_window_secs).await;

        // Record last evidence for /api/debug/ingest
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

// ------------------------
// HSTS inject middleware + (optional) diagnostics
// ------------------------
static HSTS_MW_SEEN: AtomicBool = AtomicBool::new(false);

async fn hsts_inject(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;

    // Always set HSTS
    resp.headers_mut().insert(
        HeaderName::from_static("strict-transport-security"),
        HeaderValue::from_static(HSTS_VALUE),
    );

    // Only set diagnostics in DEBUG mode
    if env_truthy("DEBUG_ROUTES", false) {
        HSTS_MW_SEEN.store(true, Ordering::Relaxed);
        resp.headers_mut().insert(
            HeaderName::from_static("x-diag-hsts"),
            HeaderValue::from_static("1"),
        );
    }

    resp
}

#[derive(serde::Serialize)]
struct DiagEnv {
    hsts_enabled: bool, // kept for backward compat, always false here
    hsts_force: bool,   // kept for backward compat, always false here
    mw_seen: bool,
}

async fn diag_env() -> Json<DiagEnv> {
    Json(DiagEnv {
        hsts_enabled: false,
        hsts_force: false,
        mw_seen: HSTS_MW_SEEN.load(Ordering::Relaxed),
    })
}

// ----- Shuttle entrypoint -----
#[shuttle_runtime::main]
async fn axum(
    #[shuttle_runtime::Secrets] secrets: SecretStore,
    #[shuttle_shared_db::Postgres] last_store: SerdeJsonOperator,
) -> ShuttleAxum {
    // 1) Secrets -> env (only for real secrets needed by third‑party libs)
    load_secrets_into_env(&secrets);

    // 2) Read ingest/config toggles from env OR secrets
    let ingest_cfg = IngestConfig {
        enabled: get_bool_env_or_secret(&secrets, "INGEST_ENABLED", false),
        interval_secs: get_u64_env_or_secret(&secrets, "INGEST_INTERVAL_SECS", 300),
        dedup_window_secs: get_u64_env_or_secret(&secrets, "INGEST_DEDUP_WINDOW_SECS", 600),
        enable_reuters: get_bool_env_or_secret(&secrets, "INGEST_ENABLE_REUTERS", true),
        enable_generic: get_bool_env_or_secret(&secrets, "INGEST_ENABLE_GENERIC", true),
    };

    // 3) App state + API router
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

    // 4) Ingest scheduler
    tokio::spawn(run_ingest_scheduler(app_state.clone(), ingest_cfg));

    // 5) Security & utility layers (no HSTS here; added by our middleware at the very end)
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
        ));

    // Read debug/metrics with the same env-or-secret fallback
    let debug_routes = get_bool_env_or_secret(&secrets, "DEBUG_ROUTES", false);

    // 6) Top-level router (order matters: redirect first, then security)
    let mut app = Router::new()
        // System endpoints
        .route("/_version", get(versions::handler))
        .route("/_health", get(|| async { "ok" }))
        .route("/_healthz", get(|| async { "ok" })) // k8s-style alias
        // Static
        .route("/favicon.ico", get(favicon_handler))
        .route("/apple-touch-icon.png", get(apple_touch_handler))
        .route("/assets/{*path}", get(assets_handler))
        .route("/config/{*path}", get(config_handler)) // static configs
        // API (before SPA fallback)
        .nest("/api", api_router)
        // UI + SPA fallback (last)
        .route("/", get(index_html))
        .route("/{*path}", get(index_html))
        // 1) inner: apex -> www redirect
        .layer(middleware::from_fn(redirect_apex_to_www))
        // 2) outer: security + utility headers (applies also to 301)
        .layer(sec);

    // Optional debug route
    if debug_routes {
        app = app.route("/_diag/env", get(diag_env));
    }

    // <<< HSTS via custom middleware – very last, so it covers 200 and 301 >>>
    app = app.layer(middleware::from_fn(hsts_inject));

    // Optional metrics at /metrics (root-level)
    let metrics_enabled = get_bool_env_or_secret(&secrets, "METRICS_ENABLED", false);
    if metrics_enabled {
        app = app.merge(dow_sentiment_analyzer::metrics::router());
    }

    let app = app.without_v07_checks();
    Ok(app.into())
}
