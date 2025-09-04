//! MVP Sentiment Service — Binary Entrypoint
//! Boots the Axum HTTP server, wiring routes, shared state, and middleware.

use axum::{response::IntoResponse, routing::get, Router};
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

// --- Notifications ---
mod notify;
use notify::discord::DiscordNotifier;
use notify::slack::SlackNotifier;
use notify::AlertPayload;

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

#[shuttle_runtime::main]
async fn axum(
    // Inject Shuttle secrets here
    #[shuttle_runtime::Secrets] secrets: SecretStore,
) -> ShuttleAxum {
    let _ = dotenvy::dotenv();
    enable_dev_tracing();

    // --- AI quick probe (local/dev only) + optional Discord/Slack tests ---
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

        // Gate: send Discord test only when NOTIFY_DISCORD is truthy
        let notify_discord_enabled = std::env::var("NOTIFY_DISCORD")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        if notify_discord_enabled {
            if let Some(hook) = secrets.get("DISCORD_WEBHOOK_URL") {
                let notifier = DiscordNotifier::new(hook).with_timeout(5).with_retries(3);
                let now = chrono::Utc::now().to_rfc3339();

                let payload = AlertPayload {
                    decision: "TEST".into(),
                    confidence: 0.42,
                    reasons: vec!["wiring OK".into(), "webhook reachable".into()],
                    timestamp_iso: now.clone(),
                };

                if let Err(e) = notifier.send_alert(&payload).await {
                    tracing::warn!(error=?e, "Discord notifier test failed");
                } else {
                    tracing::info!("Discord notifier test sent");
                }
            } else {
                tracing::warn!("DISCORD_WEBHOOK_URL not found in SecretStore");
            }
        }

        // Gate: send Slack test only when NOTIFY_SLACK is truthy
        let notify_slack_enabled = std::env::var("NOTIFY_SLACK")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        if notify_slack_enabled {
            if let Some(hook) = secrets.get("SLACK_WEBHOOK") {
                let notifier = SlackNotifier::new(hook).with_timeout(5).with_retries(3);
                let now = chrono::Utc::now().to_rfc3339();

                let payload = AlertPayload {
                    decision: "TEST".into(),
                    confidence: 0.42,
                    reasons: vec!["wiring OK".into(), "webhook reachable".into()],
                    timestamp_iso: now,
                };

                if let Err(e) = notifier.send_alert(&payload).await {
                    tracing::warn!(error=?e, "Slack notifier test failed");
                } else {
                    tracing::info!("Slack notifier test sent");
                }
            } else {
                tracing::warn!("SLACK_WEBHOOK not found in SecretStore");
            }
        }
    }
    // --- /AI quick probe ---

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

    Ok(app.into())
}
