//! Local/demo server entrypoint (non-Shuttle).
//! Runs the same Axum app shape as Shuttle, but uses process env + file-backed last snapshot.

use axum::{
    extract::Path,
    http::{header, HeaderName, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use dow_sentiment_analyzer::{
    api,
    ingest::{
        self,
        providers::{
            fed_rss::FedRssProvider, generic_rss::GenericRssProvider, reuters_rss::ReutersRssProvider,
        },
        types::SourceProvider,
    },
    last::LastStore,
    relevance::AppState,
    versions,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer, limit::RequestBodyLimitLayer, set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};
use tracing::info;

const INDEX_HTML: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/index.html"));

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

#[derive(Clone, Debug)]
struct IngestConfig {
    enabled: bool,
    interval_secs: u64,
    dedup_window_secs: u64,
    enable_reuters: bool,
    enable_generic: bool,
}

fn parse_bool(s: &str) -> bool {
    matches!(s.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

fn env_truthy(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => parse_bool(&v),
        Err(_) => default,
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(default)
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

        dow_sentiment_analyzer::api::debug_ingest_record(&kept);

        info!(
            kept = kept.len(),
            filtered = filtered,
            dedup = dedup,
            "ingest tick finished"
        );
    }
}

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

async fn redirect_apex_to_www(req: axum::extract::Request, next: Next) -> Response {
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

static HSTS_MW_SEEN: AtomicBool = AtomicBool::new(false);

async fn hsts_inject(req: axum::extract::Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    resp.headers_mut().insert(
        HeaderName::from_static("strict-transport-security"),
        HeaderValue::from_static(HSTS_VALUE),
    );

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
    hsts_enabled: bool,
    hsts_force: bool,
    mw_seen: bool,
}

async fn diag_env() -> Json<DiagEnv> {
    Json(DiagEnv {
        hsts_enabled: false,
        hsts_force: false,
        mw_seen: HSTS_MW_SEEN.load(Ordering::Relaxed),
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt().with_target(false).try_init();

    let ingest_cfg = IngestConfig {
        enabled: env_truthy("INGEST_ENABLED", false),
        interval_secs: env_u64("INGEST_INTERVAL_SECS", 300),
        dedup_window_secs: env_u64("INGEST_DEDUP_WINDOW_SECS", 600),
        enable_reuters: env_truthy("INGEST_ENABLE_REUTERS", true),
        enable_generic: env_truthy("INGEST_ENABLE_GENERIC", true),
    };

    let app_state = AppState::from_env();
    let last_path = std::env::var("LAST_DECISION_PATH")
        .unwrap_or_else(|_| "data/last_decision.json".to_string());
    let last = LastStore::file(last_path);
    let api_router = api::router_with_last(app_state.clone(), last).without_v07_checks();

    let api_body_limit_bytes: usize = std::env::var("API_BODY_LIMIT_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(131_072);
    let api_router = if api_body_limit_bytes > 0 {
        api_router.layer(RequestBodyLimitLayer::new(api_body_limit_bytes))
    } else {
        api_router
    };

    tokio::spawn(run_ingest_scheduler(app_state, ingest_cfg));

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

    let debug_routes = env_truthy("DEBUG_ROUTES", false);

    let mut app = Router::new()
        .route("/_version", get(versions::handler))
        .route("/_health", get(|| async { "ok" }))
        .route("/_healthz", get(|| async { "ok" }))
        .route("/favicon.ico", get(favicon_handler))
        .route("/apple-touch-icon.png", get(apple_touch_handler))
        .route("/assets/{*path}", get(assets_handler))
        .route("/config/{*path}", get(config_handler))
        .nest("/api", api_router)
        .route("/", get(index_html))
        .route("/{*path}", get(index_html))
        .layer(middleware::from_fn(redirect_apex_to_www))
        .layer(sec);

    if debug_routes {
        app = app.route("/_diag/env", get(diag_env));
    }

    app = app.layer(middleware::from_fn(hsts_inject));

    if env_truthy("METRICS_ENABLED", false) {
        app = app.merge(dow_sentiment_analyzer::metrics::router());
    }

    let app = app.without_v07_checks();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(8000);
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("local demo server listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
