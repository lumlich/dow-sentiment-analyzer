//! HTTP API Layer

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use shuttle_axum::axum::{
    extract::{Extension, Query},
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

// bring relevance types/helpers (engine/handle/state + dev logs)
use crate::relevance::{
    anon_hash, dev_logging_enabled, truncate_vec, AppState as RelevanceAppState, RelevanceHandle,
};

// tracing for dev-only audit logs
use tracing::info;

const VOLUME_WINDOW_SECS: u64 = 600; // 10 min

/// Internal API state used by handlers (wrapped in Arc and injected via Extension).
#[derive(Clone)]
pub struct ApiState {
    analyzer: Arc<SentimentAnalyzer>,
    rolling: Arc<RollingWindow>,
    history: Arc<History>,
    source_weights: Arc<RwLock<SourceWeightsConfig>>,
    relevance: RelevanceHandle,
}

fn debug_enabled() -> bool {
    std::env::var("SHUTTLE_ENV")
        .map(|v| v == "local")
        .unwrap_or(false)
}

/// Build the Router. Accepts the AppState from `main.rs` (with a configured RelevanceHandle).
/// Returns `Router<()>` and injects `Arc<ApiState>` via `Extension`.
pub fn create_router(state_from_main: RelevanceAppState) -> Router<()> {
    // Load source weights from file
    let sw = SourceWeightsConfig::load_from_file("source_weights.json");

    // Build full API state (reuse the relevance handle provided by main)
    let state = Arc::new(ApiState {
        analyzer: Arc::new(SentimentAnalyzer::new()),
        rolling: Arc::new(RollingWindow::new_48h()),
        history: Arc::new(History::with_capacity(2000)),
        source_weights: Arc::new(RwLock::new(sw)),
        relevance: state_from_main.relevance, // <-- from main.rs
    });

    // Base Router with explicit unit state
    let mut app: Router<()> = Router::new()
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
        );

    // Optionally merge debug router (keeps Router<()>)
    if debug_enabled() {
        let dbg: Router<()> = crate::debug::router();
        app = app.merge(dbg);
    }

    // Layers last — they do not change router state type
    app.layer(CorsLayer::very_permissive())
        .layer(Extension(state))
}

#[derive(serde::Deserialize)]
struct AnalyzeReq {
    text: String,
}

#[derive(serde::Deserialize)]
struct DecideItem {
    source: String,
    text: String,
    #[serde(default)]
    ts_unix: Option<u64>,
}

#[derive(serde::Serialize)]
struct AnalyzeResp {
    score: i32,
    tokens_count: usize,
}

async fn analyze(
    Extension(state): Extension<Arc<ApiState>>,
    Json(body): Json<AnalyzeReq>,
) -> Json<AnalyzeResp> {
    let t0 = Instant::now();
    if debug_enabled() {
        crate::debug::record_request(false);
    }

    let (score, tokens) = state.analyzer.score_text(&body.text);
    state.rolling.record(score, None);

    if debug_enabled() {
        let verdict = if score >= 0 { "BUY" } else { "SELL" };
        crate::debug::record_decision("analyze".to_string(), score, verdict.to_string());
        crate::debug::record_latency(t0.elapsed().as_millis());
    }

    Json(AnalyzeResp {
        score,
        tokens_count: tokens,
    })
}

async fn analyze_batch(
    Extension(state): Extension<Arc<ApiState>>,
    Json(items): Json<Vec<BatchItem>>,
) -> Json<Vec<(BatchItem, i32)>> {
    let t0 = Instant::now();
    if debug_enabled() {
        crate::debug::record_request(true);
    }

    let scored = items
        .into_iter()
        .map(|it| {
            let (score, _) = state.analyzer.score_text(&it.text);
            state.rolling.record(score, None);
            let _ = disruption::evaluate(&DisruptionInput {
                source: it.source.clone(),
                text: it.text.clone(),
                score,
                ts_unix: current_unix(),
            });
            (it, score)
        })
        .collect::<Vec<_>>();

    if debug_enabled() {
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

async fn decide_batch(
    Extension(state): Extension<Arc<ApiState>>,
    Json(items): Json<Vec<DecideItem>>,
) -> Json<Decision> {
    let t0 = Instant::now();
    if debug_enabled() {
        crate::debug::record_request(true);
    }

    let now = current_unix();
    let mut scored = Vec::with_capacity(items.len());

    let mut neutralized = 0usize;
    let total = items.len();

    for it in items {
        // 1) Raw sentiment
        let (raw_score, _tokens) = state.analyzer.score_text(&it.text);

        // 2) Relevance gate
        let rel = state.relevance.score(&it.text);
        let gated_score = if rel.score > 0.0 { raw_score } else { 0 };

        // --- dev-only relevance audit log (anonymized) ---
        if dev_logging_enabled() {
            let event = if rel.score > 0.0 {
                "api_pass"
            } else {
                "api_neutralized"
            };
            info!(
                target: "relevance",
                event,
                id = %anon_hash(&it.text),                   // no raw text
                matched = ?truncate_vec(&rel.matched, 5),
                reasons = ?truncate_vec(&rel.reasons, 5),
                rel_score = rel.score,
                raw = raw_score,
                gated = gated_score
            );
        }

        if gated_score == 0 && raw_score != 0 {
            neutralized += 1;
        }

        // 3) Record & disruption with gated score
        state.rolling.record(gated_score, None);
        let ts = it.ts_unix.unwrap_or(now);

        let di = DisruptionInput {
            source: it.source.clone(),
            text: it.text.clone(),
            score: gated_score,
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
        scored.push((bi, gated_score, res));
    }

    // 4) Decision from gated inputs
    let mut decision = engine::make_decision(&scored);

    // 5) Volume context modifier
    let (vf, recent_triggers, uniq_sources) = volume_factor_from_history(&state.history, now);
    let old_conf = decision.confidence;
    let new_conf = (old_conf * vf).clamp(0.0, 0.99);
    decision.confidence = new_conf;

    decision.reasons.push(
        crate::decision::Reason::new(format!(
            "Volume context (last {window}s): {rt} triggers from {us} sources -> confidence x{vf:.3} ({old:.3}→{new:.3})",
            window = VOLUME_WINDOW_SECS, rt = recent_triggers, us = uniq_sources, vf = vf, old = old_conf, new = new_conf,
        ))
        .kind(crate::decision::ReasonKind::Threshold)
        .weighted(((vf - 0.90) / (1.05 - 0.90)).clamp(0.0, 1.0)),
    );

    // 6) Relevance-gate note
    if neutralized > 0 && total > 0 {
        let frac = neutralized as f32 / total as f32;
        decision.reasons.push(
            crate::decision::Reason::new(format!(
                "Relevance gate neutralized {}/{} items before decision",
                neutralized, total
            ))
            .kind(crate::decision::ReasonKind::Threshold)
            .weighted(frac.clamp(0.0, 1.0)),
        );
    }

    // 7) Persist decision
    state.history.push(&decision);

    if debug_enabled() {
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

fn current_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn volume_factor_from_history(hist: &History, now: u64) -> (f32, usize, usize) {
    let rows = hist.snapshot_last_n(200);
    let mut recent_triggers = 0usize;
    let mut uniq = std::collections::HashSet::new();

    for h in rows {
        if now.saturating_sub(h.ts_unix) <= VOLUME_WINDOW_SECS
            && matches!(
                h.verdict,
                crate::decision::Verdict::Buy | crate::decision::Verdict::Sell
            )
        {
            recent_triggers += 1;
            for s in h.top_sources.iter().take(5) {
                uniq.insert(s.clone());
            }
        }
    }

    let rt = recent_triggers.min(5) as f32;
    let us = uniq.len().min(5) as f32;
    let mut vf = 0.90 + 0.02 * rt + 0.01 * us;
    vf = vf.clamp(0.90, 1.05);

    (vf, recent_triggers, uniq.len())
}

#[derive(serde::Serialize)]
struct RollingInfo {
    window_secs: u64,
    average: f32,
    count: usize,
}

async fn debug_rolling(Extension(state): Extension<Arc<ApiState>>) -> Json<RollingInfo> {
    let (avg, n) = state.rolling.average_and_count();
    Json(RollingInfo {
        window_secs: state.rolling.window_secs(),
        average: avg,
        count: n,
    })
}

#[derive(serde::Serialize)]
struct HistoryOut {
    ts_unix: u64,
    verdict: String,
    confidence: f32,
    sources: Vec<String>,
    scores: Vec<i32>,
}

async fn debug_history(Extension(state): Extension<Arc<ApiState>>) -> Json<Vec<HistoryOut>> {
    let rows = state.history.snapshot_last_n(10);
    Json(
        rows.into_iter()
            .map(|h| HistoryOut {
                ts_unix: h.ts_unix,
                verdict: format!("{:?}", h.verdict).to_uppercase(),
                confidence: h.confidence,
                sources: h.top_sources,
                scores: h.top_scores,
            })
            .collect(),
    )
}

#[derive(serde::Serialize)]
struct LastOut {
    ts_unix: u64,
    verdict: String,
    confidence: f32,
    sources: Vec<String>,
    scores: Vec<i32>,
}

async fn debug_last_decision(Extension(state): Extension<Arc<ApiState>>) -> Json<Option<LastOut>> {
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

async fn debug_source_weight(
    Extension(state): Extension<Arc<ApiState>>,
    Query(q): Query<HashMap<String, String>>,
) -> String {
    let s = q.get("source").cloned().unwrap_or_default();
    let w = {
        let g = state.source_weights.read().expect("rwlock poisoned");
        g.weight_for(&s)
    };
    format!("source='{}' -> weight={:.2}", s, w)
}

async fn admin_reload_source_weights(Extension(state): Extension<Arc<ApiState>>) -> String {
    let fresh = SourceWeightsConfig::load_from_file("source_weights.json");
    match state.source_weights.write() {
        Ok(mut w) => {
            *w = fresh;
            "reloaded".to_string()
        }
        Err(_) => "failed: lock poisoned".to_string(),
    }
}
