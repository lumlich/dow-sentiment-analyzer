//! MVP Sentiment Service — Binary Entrypoint
//! Boots the Axum HTTP server, wiring routes, shared state, and middleware.

use axum::{response::IntoResponse, routing::get, Router};
use shuttle_axum::ShuttleAxum;
use shuttle_runtime::SecretStore;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use chrono::Utc;

use dow_sentiment_analyzer::api;
use dow_sentiment_analyzer::relevance::{
    start_hot_reload_thread, AppState, RelevanceEngine, RelevanceHandle,
    DEFAULT_RELEVANCE_CONFIG_PATH, ENV_RELEVANCE_CONFIG_PATH,
};

// --- Notifications / Change Detector (from crate lib) ---
use dow_sentiment_analyzer::change_detector;
use dow_sentiment_analyzer::notify::{DecisionKind, NotificationEvent, NotifierMux};

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
    let v = env!("CARGO_PKG_VERSION");
    format!("{} (service: dow-sentiment-analyzer)", v)
}

/// Copy selected Shuttle secrets into process env so the library notifiers can read them.
/// We only set a var if it's not already present in the environment.
fn export_notification_secrets_to_env(secrets: &SecretStore) {
    let set_if_missing = |key_env: &str, key_secret: &str| {
        if std::env::var(key_env).is_err() {
            if let Some(val) = secrets.get(key_secret) {
                std::env::set_var(key_env, val);
                tracing::debug!("exported secret {key_secret} -> env {key_env}");
            }
        }
    };

    // Webhooks (note: some repos use SLACK_WEBHOOK vs SLACK_WEBHOOK_URL)
    set_if_missing("DISCORD_WEBHOOK_URL", "DISCORD_WEBHOOK_URL");
    set_if_missing("SLACK_WEBHOOK_URL", "SLACK_WEBHOOK_URL");
    set_if_missing("SLACK_WEBHOOK_URL", "SLACK_WEBHOOK"); // fallback key used earlier

    // Email secrets (optional)
    set_if_missing("SMTP_HOST", "SMTP_HOST");
    set_if_missing("SMTP_USER", "SMTP_USER");
    set_if_missing("SMTP_PASS", "SMTP_PASS");
    set_if_missing("NOTIFY_EMAIL_FROM", "NOTIFY_EMAIL_FROM");
    set_if_missing("NOTIFY_EMAIL_TO", "NOTIFY_EMAIL_TO");
}

#[shuttle_runtime::main]
async fn axum(
    // Inject Shuttle secrets here
    #[shuttle_runtime::Secrets] secrets: SecretStore,
) -> ShuttleAxum {
    let _ = dotenvy::dotenv();
    enable_dev_tracing();

    // Make webhooks/SMTP available to the unified notifier layer.
    export_notification_secrets_to_env(&secrets);

    // --- Optional: local/dev quick probe + notification test via NotifierMux ---
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

        // Send a single test event when NOTIFY_TEST is truthy.
        let notify_test = std::env::var("NOTIFY_TEST")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        if notify_test {
            let mux = NotifierMux::from_env();
            let ev = NotificationEvent {
                decision: DecisionKind::HOLD,
                confidence: 0.42,
                reasons: vec!["wiring OK".into(), "mux reachable".into()],
                ts: Utc::now(),
            };
            mux.notify(&ev).await;
            tracing::info!("Sent NOTIFY_TEST event via NotifierMux");
        }
    }
    // --- /Optional tests ---

    // Relevance gate – start
    let engine = match RelevanceEngine::from_toml() {
        Ok(e) => e,
        Err(e) => {
            tracing::error!(error=?e, "Failed to load relevance config - starting in degraded mode");

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

    let state = AppState { relevance: handle };
    let api_router: Router<()> = api::router(state);

    let static_files = ServeDir::new("ui/dist")
        .append_index_html_on_directories(true)
        .not_found_service(ServeFile::new("ui/dist/index.html"));

    let app: Router<()> = Router::new()
        .route("/health", get(root_health))
        .route("/_version", get(version))
        .route("/api/ping", get(|| async { "pong" }))
        .nest("/api", api_router)
        .fallback_service(static_files);

    // --- Spawn background change detector (Tokio task) ---
    tokio::spawn(async {
        if let Err(e) = change_detector::run_change_detector().await {
            tracing::error!("change detector exited: {e:#}");
        }
    });

    Ok(app.into())
}
