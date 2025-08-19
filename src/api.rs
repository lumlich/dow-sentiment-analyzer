//! # HTTP API Layer
//! Defines the Axum router, request/response models, and handlers.
//!
//! ## Design
//! - Thin handlers: delegate business logic to `sentiment`, `engine`, and disruption logic.
//! - Shared state (`AppState`) holds the analyzer and runtime data behind `Arc`/`RwLock`.
//! - CORS is permissive for development; restrict origins/methods/headers in production.
//!
//! ## Responses
//! JSON only. Errors use standard HTTP status codes and concise messages.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[cfg(feature = "debug")]
use std::time::Instant;

use shuttle_axum::axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use tower_http::cors::CorsLayer;

use crate::decision::Decision;
use crate::disruption::{self, evaluate_with_weights, DisruptionInput};
use crate::engine;
use crate::history::History;
use crate::rolling::RollingWindow;
use crate::sentiment::{BatchItem, SentimentAnalyzer};
use crate::source_weights::SourceWeightsConfig;

/// Time window (in seconds) for recent activity context used to adjust confidence.
const VOLUME_WINDOW_SECS: u64 = 600; // 10 minutes

/// Application shared state passed to handlers.
///
/// - `analyzer`: stateless sentiment analyzer; cheap to clone per request via `Arc`.
/// - `rolling`: rolling statistics for recent scores (e.g., average).
/// - `history`: recent decisions for transparency and context.
/// - `source_weights`: dynamic per-source scaling, hot-reloadable via admin endpoint.
#[derive(Clone)]
pub struct AppState {
    analyzer: Arc<SentimentAnalyzer>,
    rolling: Arc<RollingWindow>,
    history: Arc<History>,
    source_weights: Arc<RwLock<SourceWeightsConfig>>,
}

/// Build the Axum router with routes, middleware, and shared state.
///
/// Loads `source_weights.json` at startup. CORS is set to `very_permissive()`
/// for development convenience.
pub fn create_router() -> Router {
    let sw = SourceWeightsConfig::load_from_file("source_weights.json");

    let state = AppState {
        analyzer: Arc::new(SentimentAnalyzer::new()),
        rolling: Arc::new(RollingWindow::new_48h()),
        history: Arc::new(History::with_capacity(2000)),
        source_weights: Arc::new(RwLock::new(sw)),
    };

    let r = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/analyze", post(analyze))
        .route("/batch", post(analyze_batch))
        .route("/decide", post(decide_batch))
        .route("/debug/rolling", get(debug_rolling))
        .route("/debug/history", get(debug_history))
        .route("/debug/last-decision", get(debug_last_decision))
        .route("/debug/source-weight", get(debug_source_weight))
        .route(
            "/admin/reload-source-weights",
            get(admin_reload_source_weights),
        )
        .layer(CorsLayer::very_permissive())
        .with_state(state);

    // /history a /stats jen při --features debug
    #[cfg(feature = "debug")]
    let r = r.merge(crate::debug::router());

    r
}

/// Request body for `/analyze`.
#[derive(serde::Deserialize)]
struct AnalyzeReq {
    /// Free-form UTF-8 text to analyze.
    text: String,
}

/// One decision input item (used by `/decide` in batch form).
#[derive(serde::Deserialize)]
struct DecideItem {
    /// Logical source/author/channel (used for per-source weighting).
    source: String,
    /// Text to analyze.
    text: String,
    /// Optional UNIX seconds. If missing, the server uses "now".
    #[serde(default)]
    ts_unix: Option<u64>,
}

/// Response body for `/analyze`.
#[derive(serde::Serialize)]
struct AnalyzeResp {
    /// Signed sentiment score.
    score: i32,
    /// Number of tokens considered.
    tokens_count: usize,
}

/// Analyze a single text and return a signed sentiment score.
async fn analyze(State(state): State<AppState>, Json(body): Json<AnalyzeReq>) -> Json<AnalyzeResp> {
    #[cfg(feature = "debug")]
    let t0 = Instant::now();
    #[cfg(feature = "debug")]
    crate::debug::record_request(false);

    let (score, tokens) = state.analyzer.score_text(&body.text);
    state.rolling.record(score, None);

    #[cfg(feature = "debug")]
    {
        let verdict = if score >= 0 { "BUY" } else { "SELL" };
        crate::debug::record_decision("analyze".to_string(), score, verdict.to_string());
        crate::debug::record_latency(t0.elapsed().as_millis());
    }

    Json(AnalyzeResp {
        score,
        tokens_count: tokens,
    })
}

/// Analyze multiple texts and return `(item, score)` pairs.
async fn analyze_batch(
    State(state): State<AppState>,
    Json(items): Json<Vec<BatchItem>>,
) -> Json<Vec<(BatchItem, i32)>> {
    #[cfg(feature = "debug")]
    let t0 = Instant::now();
    #[cfg(feature = "debug")]
    crate::debug::record_request(true);

    let scored = items
        .into_iter()
        .map(|it| {
            let (score, _) = state.analyzer.score_text(&it.text);
            state.rolling.record(score, None);
            let _res = disruption::evaluate(&DisruptionInput {
                source: it.source.clone(),
                text: it.text.clone(),
                score,
                ts_unix: current_unix(),
            });
            (it, score)
        })
        .collect::<Vec<_>>();

    #[cfg(feature = "debug")]
    {
        let avg: i32 = if scored.is_empty() {
            0
        } else {
            let sum: i64 = scored.iter().map(|(_, s)| *s as i64).sum();
            (sum / scored.len() as i64) as i32
        };
        let verdict = if avg >= 0 { "BUY" } else { "SELL" };
        crate::debug::record_decision("batch".to_string(), avg, verdict.to_string());
        crate::debug::record_latency(t0.elapsed().as_millis());
    }

    Json(scored)
}

/// Decide an action from a batch of inputs, applying per-source weights and recent volume context.
async fn decide_batch(
    State(state): State<AppState>,
    Json(items): Json<Vec<DecideItem>>,
) -> Json<Decision> {
    #[cfg(feature = "debug")]
    let t0 = Instant::now();
    #[cfg(feature = "debug")]
    crate::debug::record_request(true);

    let now = current_unix();

    let mut scored = Vec::with_capacity(items.len());
    for it in items {
        let (score, _tokens) = state.analyzer.score_text(&it.text);
        state.rolling.record(score, None);

        let ts = it.ts_unix.unwrap_or(now);

        let di = DisruptionInput {
            source: it.source.clone(),
            text: it.text.clone(),
            score,
            ts_unix: ts,
        };
        let res = {
            let guard = state.source_weights.read().expect("rwlock poisoned");
            evaluate_with_weights(&di, &guard)
        };

        let bi = BatchItem {
            source: it.source,
            text: it.text,
        };
        scored.push((bi, score, res));
    }

    let mut decision = engine::make_decision(&scored);

    // Confidence adjustment based on recent trigger "volume" in history.
    let (vf, recent_triggers, uniq_sources) = volume_factor_from_history(&state.history, now);
    let old_conf = decision.confidence;
    let new_conf = (old_conf * vf).clamp(0.0, 0.99);
    decision.confidence = new_conf;

    // Add an explicit reason for transparency.
    decision.reasons.push(
        crate::decision::Reason::new(format!(
            "Volume context (last {window}s): {rt} triggers from {us} sources -> confidence x{vf:.3} ({old:.3}→{new:.3})",
            window = VOLUME_WINDOW_SECS,
            rt = recent_triggers,
            us = uniq_sources,
            vf = vf,
            old = old_conf,
            new = new_conf,
        ))
        .kind(crate::decision::ReasonKind::Threshold)
        .weighted(((vf - 0.90) / (1.05 - 0.90)).clamp(0.0, 1.0)),
    );

    // Persist after adjustments.
    state.history.push(&decision);

    #[cfg(feature = "debug")]
    {
        // verdict: z enumu, score: průměr z tohoto requestu
        let verdict = format!("{:?}", decision.decision).to_uppercase();
        let avg: i32 = if scored.is_empty() {
            0
        } else {
            let sum: i64 = scored.iter().map(|(_, s, _)| *s as i64).sum();
            (sum / scored.len() as i64) as i32
        };
        crate::debug::record_decision("decide".to_string(), avg, verdict);
        crate::debug::record_latency(t0.elapsed().as_millis());
    }

    Json(decision)
}

/// Current UNIX time in seconds.
fn current_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Compute a multiplicative factor for confidence based on recent history.
///
/// Returns `(volume_factor, recent_trigger_count, unique_sources_count)`.
fn volume_factor_from_history(hist: &History, now: u64) -> (f32, usize, usize) {
    let rows = hist.snapshot_last_n(200); // small buffer is sufficient
    let mut recent_triggers = 0usize;
    let mut uniq = std::collections::HashSet::new();

    for h in rows {
        if now.saturating_sub(h.ts_unix) <= VOLUME_WINDOW_SECS {
            if matches!(
                h.verdict,
                crate::decision::Verdict::Buy | crate::decision::Verdict::Sell
            ) {
                recent_triggers += 1;
                for s in h.top_sources.iter().take(5) {
                    uniq.insert(s.clone());
                }
            }
        }
    }

    let rt = recent_triggers.min(5) as f32;
    let us = uniq.len().min(5) as f32;

    let mut vf = 0.90 + 0.02 * rt + 0.01 * us;
    vf = vf.clamp(0.90, 1.05);

    (vf, recent_triggers, uniq.len())
}

/// Rolling window debug output.
#[derive(serde::Serialize)]
struct RollingInfo {
    window_secs: u64,
    average: f32,
    count: usize,
}

/// Return rolling statistics useful for quick diagnostics.
async fn debug_rolling(State(state): State<AppState>) -> Json<RollingInfo> {
    let (avg, n) = state.rolling.average_and_count();
    Json(RollingInfo {
        window_secs: state.rolling.window_secs(),
        average: avg,
        count: n,
    })
}

/// History snapshot row used by `/debug/history`.
#[derive(serde::Serialize)]
struct HistoryOut {
    ts_unix: u64,
    verdict: String,
    confidence: f32,
    sources: Vec<String>,
    scores: Vec<i32>,
}

/// Return the last N history rows (debug only).
async fn debug_history(State(state): State<AppState>) -> Json<Vec<HistoryOut>> {
    let rows = state.history.snapshot_last_n(10);
    let out = rows
        .into_iter()
        .map(|h| HistoryOut {
            ts_unix: h.ts_unix,
            verdict: format!("{:?}", h.verdict).to_uppercase(),
            confidence: h.confidence,
            sources: h.top_sources,
            scores: h.top_scores,
        })
        .collect::<Vec<_>>();
    Json(out)
}

/// Last decision shape for `/debug/last-decision`.
#[derive(serde::Serialize)]
struct LastOut {
    ts_unix: u64,
    verdict: String,
    confidence: f32,
    sources: Vec<String>,
    scores: Vec<i32>,
}

/// Return the most recent decision (if any).
async fn debug_last_decision(State(state): State<AppState>) -> Json<Option<LastOut>> {
    let mut rows = state.history.snapshot_last_n(1);
    if let Some(h) = rows.pop() {
        return Json(Some(LastOut {
            ts_unix: h.ts_unix,
            verdict: format!("{:?}", h.verdict).to_uppercase(),
            confidence: h.confidence,
            sources: h.top_sources,
            scores: h.top_scores,
        }));
    }
    Json(None)
}

/// Query the effective weight for a given `source`.
///
/// Example:
/// `GET /debug/source-weight?source=TRUMP`
async fn debug_source_weight(
    State(state): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> String {
    let s = q.get("source").cloned().unwrap_or_default();
    let w = {
        let g = state.source_weights.read().expect("rwlock poisoned");
        g.weight_for(&s)
    };
    format!("source='{}' -> weight={:.2}", s, w)
}

/// Reload the `source_weights.json` file into memory.
async fn admin_reload_source_weights(State(state): State<AppState>) -> String {
    let fresh = SourceWeightsConfig::load_from_file("source_weights.json");
    match state.source_weights.write() {
        Ok(mut w) => {
            *w = fresh;
            "reloaded".to_string()
        }
        Err(_) => "failed: lock poisoned".to_string(),
    }
}