// src/lib.rs
// Public library surface for integration tests (and potential reuse).

pub mod api;
pub mod config;
pub mod decision;
pub mod disruption;
pub mod engine;
pub mod history;
pub mod ingest;
pub mod metrics;
pub mod relevance;
pub mod rolling;
pub mod sentiment;
pub mod source_weights;
pub use relevance::Relevance;

// Phase 3 analysis pipeline (NER, rerank, antispam, weights, rules, scoring, debug)
pub mod analyze;

// Phase 5 notifications & background jobs
pub mod change_detector;
pub mod notify;
// NOTE: Removed `pub mod antiflutter;` — antiflutter now lives under `notify::antiflutter`.

// ---- Re-exports for stable public API ----
// Back-compat for tests expecting `crate_root::ai_adapter::...`
pub use analyze::ai_adapter;
// Pohodlný přístup k sestavení routeru: `crate_root::api::router` i `crate_root::router`
pub use crate::api::router;

// Re-export notification types for easy use in bins/tests
pub use crate::notify::{DecisionKind, NotificationEvent, NotifierMux};
// Make AntiFlutter reachable at crate root for convenience:
pub use crate::notify::antiflutter::AntiFlutter;

pub mod ai_bootstrap;

use tracing::info;

/// Call this from your Shuttle entrypoint (after tracing init) to perform a one-off
/// smoke test of the OpenAI client. It won't panic on failure; it just logs the result.
///
/// Example usage inside your #[shuttle_runtime::main] function:
/// ```ignore
/// if let Err(e) = dow_sentiment_analyzer::run_ai_quick_probe().await {
///     tracing::warn!(error=?e, "AI quick probe didn't run");
/// }
/// ```
pub async fn run_ai_quick_probe() -> anyhow::Result<()> {
    // Path is relative to the runtime working dir (repo root in `cargo shuttle run`)
    let ai = ai_bootstrap::AiRuntime::from_path("config/ai.json")?;
    ai.quick_probe().await;
    info!("AI quick probe finished");
    Ok(())
}

// Back-compat pro testy, co volaji crate::app
pub use crate::api::app;

/// ----------------------------------------------------------------------
/// Prometheus `/metrics` helper
/// ----------------------------------------------------------------------
/// Tohle *nezasahuje* do tvého routeru automaticky. Je to malá utilita,
/// která přidá route `/metrics` a zároveň nainstaluje Prometheus recorder.
/// Použití v Shuttle entrypointu:
///
/// ```ignore
/// use dow_sentiment_analyzer::{app, prometheus};
///
/// #[shuttle_runtime::main]
/// async fn main() -> shuttle_axum::ShuttleAxum {
///     // 1) slož standardní app/router
///     let app = app().await?;
///
///     // 2) přidej /metrics
///     let (app, _handle) = prometheus::attach_metrics_route(app);
///
///     Ok(app.into())
/// }
/// ```
pub mod prometheus {
    use axum::{routing::get, Router};
    use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

    /// Nainstaluje Prometheus recorder a vrátí (Router s `/metrics`, PrometheusHandle).
    /// Handle si můžeš ponechat pro testy/debug, ale není nutné ho dál používat.
    pub fn attach_metrics_route(app: Router) -> (Router, PrometheusHandle) {
        let handle = PrometheusBuilder::new()
            // případné per-metric buckets můžeš nastavit takto:
            // .set_buckets_for_metric("ingest_parse_ms", &[5.0, 10.0, 25.0, 50.0, 100.0]).unwrap()
            .install_recorder()
            .expect("prometheus recorder installed");

        let app = app.route(
            "/metrics",
            get({
                let handle = handle.clone();
                move || async move { handle.render() }
            }),
        );

        (app, handle)
    }
}
