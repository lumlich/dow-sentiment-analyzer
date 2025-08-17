use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use shuttle_axum::axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use tower_http::cors::CorsLayer;

use crate::decision::Decision;
use crate::disruption::{self, DisruptionInput, evaluate_with_weights};
use crate::engine;
use crate::history::History;
use crate::rolling::RollingWindow;
use crate::sentiment::{BatchItem, SentimentAnalyzer};
use crate::source_weights::SourceWeightsConfig;

#[derive(Clone)]
pub struct AppState {
    analyzer: Arc<SentimentAnalyzer>,
    rolling: Arc<RollingWindow>,
    history: Arc<History>,
    source_weights: Arc<RwLock<SourceWeightsConfig>>,
}

pub fn create_router() -> Router {
    let sw = SourceWeightsConfig::load_from_file("source_weights.json");

    let state = AppState {
        analyzer: Arc::new(SentimentAnalyzer::new()),
        rolling: Arc::new(RollingWindow::new_48h()),
        history: Arc::new(History::with_capacity(2000)),
        source_weights: Arc::new(RwLock::new(sw)),
    };

    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/analyze", post(analyze))
        .route("/batch", post(analyze_batch))
        .route("/decide", post(decide_batch))
        .route("/debug/rolling", get(debug_rolling))
        .route("/debug/history", get(debug_history))
        .route("/debug/last-decision", get(debug_last_decision))
        .route("/debug/source-weight", get(debug_source_weight))
        .route("/admin/reload-source-weights", get(admin_reload_source_weights))
        .layer(CorsLayer::very_permissive())
        .with_state(state)
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
    ts_unix: Option<u64>, // pokud chybí, použijeme "teď"
}

#[derive(serde::Serialize)]
struct AnalyzeResp {
    score: i32,
    tokens_count: usize,
}

async fn analyze(State(state): State<AppState>, Json(body): Json<AnalyzeReq>) -> Json<AnalyzeResp> {
    let (score, tokens) = state.analyzer.score_text(&body.text);
    state.rolling.record(score, None);
    Json(AnalyzeResp {
        score,
        tokens_count: tokens,
    })
}

async fn analyze_batch(
    State(state): State<AppState>,
    Json(items): Json<Vec<BatchItem>>,
) -> Json<Vec<(BatchItem, i32)>> {
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
    Json(scored)
}

async fn decide_batch(
    State(state): State<AppState>,
    Json(items): Json<Vec<DecideItem>>,
) -> Json<Decision> {
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

    let decision = engine::make_decision(&scored);
    state.history.push(&decision);
    Json(decision)
}

fn current_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(serde::Serialize)]
struct RollingInfo {
    window_secs: u64,
    average: f32,
    count: usize,
}

async fn debug_rolling(State(state): State<AppState>) -> Json<RollingInfo> {
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

#[derive(serde::Serialize)]
struct LastOut {
    ts_unix: u64,
    verdict: String,
    confidence: f32,
    sources: Vec<String>,
    scores: Vec<i32>,
}

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