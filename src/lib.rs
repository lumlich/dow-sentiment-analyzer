// src/lib.rs
// Public library surface for integration tests (and potential reuse).

pub mod api;
pub mod config;
pub mod decision;
pub mod disruption;
pub mod engine;
pub mod history;
pub mod relevance;
pub mod rolling;
pub mod sentiment;
pub mod source_weights;

// Phase 3 analysis pipeline (NER, rerank, antispam, weights, rules, scoring, debug)
pub mod analyze;

// Phase 5 notifications & background jobs
pub mod notify;
pub mod change_detector;
pub mod antiflutter;

// ---- Re-exports for stable public API ----
// Back-compat for tests expecting `crate_root::ai_adapter::...`
pub use analyze::ai_adapter;
// Pohodlný přístup k sestavení routeru: `crate_root::api::router` i `crate_root::router`
pub use crate::api::router;

// Re-export notification types for easy use in bins/tests
pub use crate::notify::{DecisionKind, NotificationEvent, NotifierMux};

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
