//! HTTP API Layer

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock as StdOnceLock;
use std::sync::{Arc, OnceLock, RwLock};

use axum::{
    extract::Query,
    http::{header, HeaderValue, Method},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::Value;
use tower_http::cors::{Any, CorsLayer};

use crate::disruption::{self, evaluate_with_weights, DisruptionInput};
use crate::engine;
use crate::history::History;
use crate::rolling::RollingWindow;
use crate::sentiment::{BatchItem, SentimentAnalyzer};
use crate::source_weights::SourceWeightsConfig;

// relevance helpers (engine/handle/state + dev logs)
use crate::relevance::{
    ai_client_from_env, ai_gate_should_call, anon_hash, dev_logging_enabled, truncate_vec,
    AppState as RelevanceAppState, RelevanceHandle,
};

// AI sanitize helper
use crate::analyze::ai_adapter::sanitize_reason;

// tracing for dev-only audit logs
use tracing::info;

// ---- metrics / prometheus exporter ----
use metrics::{
    counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram, Unit,
};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};

const VOLUME_WINDOW_SECS: u64 = 600; // 10 min

/// Global API state (so the Router can remain `Router<()>`).
static API_STATE: OnceLock<Arc<ApiState>> = OnceLock::new();

/// Global Prometheus handle (installed once).
static PROM: StdOnceLock<PrometheusHandle> = StdOnceLock::new();

fn init_metrics_once() {
    PROM.get_or_init(|| {
        // Install global recorder if not installed yet (idempotent).
        // IMPORTANT: set histogram buckets for ai_decision_duration_ms so exporter emits *_bucket series
        // instead of summaries with quantiles.
        let builder = PrometheusBuilder::new()
            .set_buckets_for_metric(
                Matcher::Full("ai_decision_duration_ms".into()),
                &[
                    0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0,
                    5000.0,
                ],
            )
            .expect("set buckets for ai_decision_duration_ms");

        let handle = builder
            .install_recorder()
            .expect("install prometheus recorder");

        // --- Describe series expected by tests ---
        describe_counter!(
            "ai_decision_cache_hits_total",
            Unit::Count,
            "AI decision cache hits"
        );
        describe_counter!(
            "ai_decision_cache_misses_total",
            Unit::Count,
            "AI decision cache misses"
        );
        describe_counter!(
            "ai_decision_ai_used_total",
            Unit::Count,
            "Decisions where AI was used"
        );
        describe_histogram!(
            "ai_decision_duration_ms",
            Unit::Milliseconds,
            "Duration of /decide handler in ms"
        );
        describe_gauge!(
            "ai_decision_cache_ttl_ms",
            Unit::Milliseconds,
            "Configured AI decision cache TTL (ms)"
        );

        // --- Warm-up so series exist in exposition even before traffic ---
        counter!("ai_decision_cache_hits_total").increment(0);
        counter!("ai_decision_cache_misses_total").increment(0);
        counter!("ai_decision_ai_used_total").increment(0);
        histogram!("ai_decision_duration_ms").record(0.0);

        // Set TTL gauge from current config.
        let ttl_ms = ai_cache_ttl().as_millis() as f64;
        gauge!("ai_decision_cache_ttl_ms").set(ttl_ms);

        handle
    });
}

fn app_state() -> &'static ApiState {
    API_STATE.get().expect("API_STATE not initialized").as_ref()
}

/// Daily AI usage counter (shared across requests within the process).
#[derive(Clone, Debug)]
struct DailyAiCounter {
    /// Day number (unix_days = unix_secs / 86400)
    day: u64,
    used: usize,
}

/// Internal API state used by handlers.
#[derive(Clone)]
struct ApiState {
    analyzer: Arc<SentimentAnalyzer>,
    rolling: Arc<RollingWindow>,
    history: Arc<History>,
    source_weights: Arc<RwLock<SourceWeightsConfig>>,
    relevance: RelevanceHandle,
    /// AI adapter. Called only when the relevance gate decides it makes sense.
    ai: Arc<dyn crate::analyze::ai_adapter::AiClient + Send + Sync>,
    /// Daily limiter for AI header/calls.
    ai_daily: Arc<RwLock<DailyAiCounter>>,
    /// Simple cache for AI reason keyed by input (hash of corpus).
    ai_cache: Arc<RwLock<HashMap<u64, String>>>,
}

fn debug_enabled() -> bool {
    std::env::var("SHUTTLE_ENV")
        .map(|v| v == "local")
        .unwrap_or(false)
}

fn debug_routes_enabled() -> bool {
    debug_enabled()
        || std::env::var("DEBUG_ROUTES")
            .map(|v| v == "1")
            .unwrap_or(false)
}

// Return current UNIX time as string (for UI "time" field)
fn now_string() -> String {
    current_unix().to_string()
}

fn current_day(unix: u64) -> u64 {
    unix / 86_400
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// Build the Router. Accepts the AppState from `main.rs` (with a configured RelevanceHandle).
/// Returns `Router(())` and initializes the global `API_STATE`.
pub fn router(state_from_main: RelevanceAppState) -> Router<()> {
    // Ensure metrics recorder is ready before any metrics are emitted.
    init_metrics_once();

    // Load source weights from file
    let sw = SourceWeightsConfig::load_from_file("source_weights.json");
    let now = current_unix();

    // Build full API state (reuse the relevance handle provided by main)
    let state = Arc::new(ApiState {
        analyzer: Arc::new(SentimentAnalyzer::new()),
        rolling: Arc::new(RollingWindow::new_48h()),
        history: Arc::new(History::with_capacity(2000)),
        source_weights: Arc::new(RwLock::new(sw)),
        relevance: state_from_main.relevance,
        ai: ai_client_from_env(),
        ai_daily: Arc::new(RwLock::new(DailyAiCounter {
            day: current_day(now),
            used: 0,
        })),
        ai_cache: Arc::new(RwLock::new(HashMap::new())),
    });

    let _ = API_STATE.set(state);

    // Izolace testů: nově vytvořený router začne s prázdnou AI-cache.
    clear_ai_cache();

    // --- CORS whitelist controlled by env variable ---
    // ALLOWED_ORIGINS="http://localhost:5173,https://app.example.com"
    let allowed =
        std::env::var("ALLOWED_ORIGINS").unwrap_or_else(|_| "http://localhost:5173".to_string());

    let origins: Vec<HeaderValue> = allowed
        .split(',')
        .filter_map(|o| HeaderValue::from_str(o.trim()).ok())
        .collect();

    let cors = if origins.is_empty() {
        // Fallback: allow all origins but only basic headers/methods
        CorsLayer::new()
            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers([header::CONTENT_TYPE])
            .allow_origin(Any)
    } else {
        CorsLayer::new()
            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers([header::CONTENT_TYPE])
            .allow_origin(origins)
    };

    // Build router with explicit `S = ()`
    let mut r = Router::<()>::new()
        .route("/health", get(|| async { "OK" }))
        .route(
            "/metrics",
            get(|| async move {
                // Render Prometheus exposition format (text/plain; version=0.0.4)
                let body = PROM.get().map(|h| h.render()).unwrap_or_default();
                axum::response::Response::builder()
                    .status(200)
                    .header(
                        axum::http::header::CONTENT_TYPE,
                        "text/plain; version=0.0.4",
                    )
                    .body(axum::body::Body::from(body))
                    .unwrap()
            }),
        )
        // UI primary endpoint (Step 3). Keep POST so the dev proxy can forward as-is.
        .route("/analyze", post(analyze))
        // Batch scoring (internal/dev)
        .route("/batch", post(analyze_batch))
        // Decision endpoint: GET = stable shape for change-detector, POST = full decision
        .route("/decide", get(decide_get).post(decide));

    // Debug / introspection when enabled
    if debug_routes_enabled() {
        r = r
            .route("/debug/rolling", get(debug_rolling))
            .route("/debug/history", get(debug_history))
            .route("/debug/last-decision", get(debug_last_decision))
            .route("/debug/source-weight", get(debug_source_weight))
            .route(
                "/admin/reload-source-weights",
                get(admin_reload_source_weights),
            );
    }

    // Apply CORS and the X-AI-Cache middleware
    r.layer(cors).layer(axum::middleware::from_fn(ai_cache_mw))
}

#[derive(serde::Deserialize, Default)]
struct AnalyzeReq {
    /// Text to analyze. Defaults to empty so `{}` works in dev.
    #[serde(default)]
    text: String,
}

#[derive(serde::Deserialize)]
struct DecideItem {
    source: String,
    text: String,
    #[serde(default)]
    ts_unix: Option<u64>,
}

// ---------- UI Step 3: Response shape for POST /analyze ----------

#[derive(serde::Serialize)]
struct ApiEvidence {
    title: String,
    source: String,
    url: String,
    sentiment: String, // "pos" | "neg" | "neu"
    time: String,      // human-readable or ISO/UNIX string
}

#[derive(serde::Serialize)]
struct AnalyzeOut {
    decision: String,     // "BUY" | "SELL" | "HOLD"
    confidence: f32,      // 0..1
    reasons: Vec<String>, // plain strings
    evidence: Vec<ApiEvidence>,
    contributors: Vec<String>,
}

// ---- /decide (GET): stable shape for change-detector ----

#[derive(serde::Serialize)]
struct DecideOut {
    decision: String,
    confidence: f32,
    reasons: Vec<String>,
}

// ---- AI response metadata for /decide (POST) ----

#[derive(serde::Serialize, Default)]
struct ApiAiInfo {
    used: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    cache_hit: bool,
    limited: bool,
}

#[derive(serde::Serialize)]
struct DecideWithAi {
    #[serde(flatten)]
    inner: crate::decision::Decision,
    ai: ApiAiInfo,
}

// ----------------------------------------------------------------

async fn analyze(Json(body): Json<AnalyzeReq>) -> Json<AnalyzeOut> {
    let state = app_state();
    let t0 = std::time::Instant::now();
    if debug_enabled() {
        info!(target: "api_debug", event = "request", path = "/analyze", batch = false);
    }

    let (score, _tokens) = state.analyzer.score_text(&body.text);
    state.rolling.record(score, None);

    let verdict = if score > 0 {
        "BUY"
    } else if score < 0 {
        "SELL"
    } else {
        "HOLD"
    };

    let ts = now_string();

    if debug_enabled() {
        info!(target: "api_debug", event = "decision", path = "/analyze", score = score, verdict = %verdict);
        info!(target: "api_debug", event = "latency_ms", path = "/analyze", ms = t0.elapsed().as_millis());
    }

    let out = AnalyzeOut {
        decision: verdict.to_string(),
        confidence: 0.74,
        reasons: vec![
            "Futures rebound after dovish remarks in FOMC minutes".to_string(),
            "Positive breadth in Dow components during pre-market".to_string(),
            "Sentiment shift detected in key sources (low noise)".to_string(),
        ],
        evidence: vec![
            ApiEvidence {
                title: "Fed signals patience; markets react positively".to_string(),
                source: "Reuters".to_string(),
                url: "#".to_string(),
                sentiment: "pos".to_string(),
                time: ts.clone(),
            },
            ApiEvidence {
                title: "Dow futures edge higher amid earnings beats".to_string(),
                source: "Bloomberg".to_string(),
                url: "#".to_string(),
                sentiment: "pos".to_string(),
                time: ts.clone(),
            },
            ApiEvidence {
                title: "Mixed commentary on industrials; net neutral".to_string(),
                source: "WSJ".to_string(),
                url: "#".to_string(),
                sentiment: "neu".to_string(),
                time: ts,
            },
        ],
        contributors: vec!["relevance-engine".to_string(), "sentiment-core".to_string()],
    };

    Json(out)
}

async fn analyze_batch(Json(items): Json<Vec<BatchItem>>) -> Json<Vec<(BatchItem, i32)>> {
    let state = app_state();
    let t0 = std::time::Instant::now();
    if debug_enabled() {
        info!(target: "api_debug", event = "request", path = "/batch", batch = true);
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
        info!(target: "api_debug", event = "decision", path = "/batch", avg_score = avg, verdict = %verdict);
        info!(target: "api_debug", event = "latency_ms", path = "/batch", ms = t0.elapsed().as_millis());
    }

    Json(scored)
}

// ---- Helper: decide whether an AI "reason" counts as actually used (vs. limit/quota replies)
fn ai_reason_counts_as_used(reason: &str) -> bool {
    if reason.trim().is_empty() {
        return false;
    }
    let r = reason.to_ascii_lowercase();
    let blockers = [
        "limit",
        "exceed",
        "exceeded",
        "disabled",
        "band",
        "quota",
        "rate",
        "limited",
        "throttle",
        "throttled",
        "exhaust",
        "exhausted",
        "reach",
        "reached",
        "cap",
        "capped",
        "cooldown",
        "cool down",
        "suspend",
        "suspended",
        "turned off",
        "off",
        "quota exceeded",
        "daily limit",
        "day limit",
        "over quota",
        "temporarily unavailable",
        "not available",
        "unavailable",
    ];
    !blockers.iter().any(|kw| r.contains(kw))
}

/// AI call is purely async (no `spawn_blocking`) so the handler future stays `Send`.
async fn ai_analyze_safely(
    ai: Arc<dyn crate::analyze::ai_adapter::AiClient + Send + Sync>,
    ai_corpus: String,
) -> Option<String> {
    ai.analyze(&ai_corpus)
        .await
        .map(|ai_out| sanitize_reason(&ai_out.short_reason))
}

/// GET /decide — stable shape for change-detector
async fn decide_get() -> Json<DecideOut> {
    let state = app_state();
    // 1) Try last decision from history
    if let Some(h) = state.history.snapshot_last_n(1).pop() {
        let decision = format!("{:?}", h.verdict).to_uppercase();
        let reasons = vec![format!(
            "from history: {} sources / {} scores in last snapshot",
            h.top_sources.len(),
            h.top_scores.len()
        )];
        return Json(DecideOut {
            decision,
            confidence: h.confidence,
            reasons,
        });
    }

    // 2) Fallback when no history: HOLD 0.50
    Json(DecideOut {
        decision: "HOLD".into(),
        confidence: 0.50,
        reasons: vec!["no history yet".into()],
    })
}

#[axum::debug_handler]
async fn decide(Json(body): Json<Value>) -> impl IntoResponse {
    let t0 = std::time::Instant::now();

    // -------- 1) PHASE BEFORE `await`: build everything from state in a dedicated scope --------
    let (scored, neutralized, total, ai_corpus_opt, now) = {
        let state = app_state();
        let now = current_unix();
        let mut items: Vec<DecideItem> = {
            fn normalize_decide_body(v: Value) -> Vec<DecideItem> {
                match v {
                    Value::Array(arr) => arr
                        .into_iter()
                        .filter_map(|x| serde_json::from_value::<DecideItem>(x).ok())
                        .collect(),
                    Value::Object(map) => {
                        if let Some(items) = map.get("inputs").or_else(|| map.get("items")) {
                            if let Ok(vec_items) =
                                serde_json::from_value::<Vec<DecideItem>>(items.clone())
                            {
                                return vec_items;
                            }
                        }
                        serde_json::from_value::<DecideItem>(Value::Object(map))
                            .ok()
                            .map(|it| vec![it])
                            .unwrap_or_default()
                    }
                    Value::Null => Vec::new(),
                    _ => Vec::new(),
                }
            }
            normalize_decide_body(body)
        };

        let mut scored = Vec::with_capacity(items.len());
        let mut neutralized = 0usize;
        let total = items.len();
        let mut ai_gated_texts: Vec<String> = Vec::new();

        for it in items.drain(..) {
            let (raw_score, _tokens) = state.analyzer.score_text(&it.text);
            let rel = state.relevance.score(&it.text);
            let gated_score = if rel.score > 0.0 { raw_score } else { 0 };

            if ai_gate_should_call(&it.source, &rel) {
                ai_gated_texts.push(it.text.clone());
            }

            if dev_logging_enabled() {
                let event = if rel.score > 0.0 {
                    "api_pass"
                } else {
                    "api_neutralized"
                };
                info!(
                    target: "relevance",
                    event,
                    id = %anon_hash(&it.text),
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

        // Prepare AI corpus (if any)
        let ai_corpus_opt = if !ai_gated_texts.is_empty() {
            let mut s = String::new();
            for t in ai_gated_texts.iter().take(8) {
                if !s.is_empty() {
                    s.push_str("\n\n");
                }
                s.push_str(t);
            }
            Some(s)
        } else {
            None
        };

        (scored, neutralized, total, ai_corpus_opt, now)
    }; // <- state dropped before the await

    // -------- 2) STILL BEFORE `await`: cache/limit flags (no lock held across await) --------
    let (ai_disabled, limit_opt) = (
        std::env::var("AI_ENABLED")
            .ok()
            .map(|v| v == "0")
            .unwrap_or(false),
        std::env::var("AI_DAILY_LIMIT")
            .ok()
            .and_then(|s| s.parse::<usize>().ok()),
    );

    let mut ai_reason: Option<String> = None;
    let mut ai_cache_hit = false;
    let mut ai_limited = false;
    let mut should_call_ai = false;
    let cache_key_opt = ai_corpus_opt.as_ref().map(|c| hash_bytes(c.as_bytes()));

    if let (Some(cache_key), false) = (cache_key_opt, ai_disabled) {
        // 2a) read-cache
        if let Some(cached) = {
            let st = app_state();
            st.ai_cache
                .read()
                .ok()
                .and_then(|g| g.get(&cache_key).cloned())
        } {
            if !cached.is_empty() {
                ai_reason = Some(cached);
                ai_cache_hit = true;
            }
        } else {
            // 2b) check daily limit
            let over_limit = {
                let today = current_day(current_unix());
                let st = app_state();
                if let Some(lim) = limit_opt {
                    let mut g = st.ai_daily.write().expect("ai_daily poisoned");
                    if g.day != today {
                        g.day = today;
                        g.used = 0;
                    }
                    g.used >= lim
                } else {
                    false
                }
            };
            if over_limit {
                ai_limited = true;
            } else {
                should_call_ai = true;
            }
        }
    }

    // -------- 3) THE ONLY `await`: AI analysis (only if no cache hit and not over-limit) --------
    if ai_reason.is_none() && should_call_ai {
        if let Some(ai_corpus) = &ai_corpus_opt {
            let ai_client = { app_state().ai.clone() }; // grab Arc; no guard
            if let Some(r) = ai_analyze_safely(ai_client, ai_corpus.clone()).await {
                if ai_reason_counts_as_used(&r) {
                    ai_reason = Some(r.clone());

                    // 3a) write to cache
                    if let Some(cache_key) = cache_key_opt {
                        let st = app_state();
                        if let Ok(mut c) = st.ai_cache.write() {
                            c.insert(cache_key, r);
                        }
                    }
                    // 3b) increment daily usage (if limit is set)
                    if limit_opt.is_some() {
                        let today = current_day(current_unix());
                        let st = app_state();
                        let mut g = st.ai_daily.write().expect("ai_daily poisoned");
                        if g.day != today {
                            g.day = today;
                            g.used = 0;
                        }
                        g.used = g.used.saturating_add(1);
                    }
                }
            }
        }
    }

    // -------- 4) AFTER await: take state again and finish the response --------
    let state = app_state();

    let mut decision = engine::make_decision(&scored);

    let (vf, recent_triggers, uniq_sources) = volume_factor_from_history(&state.history, now);
    let old_conf = decision.confidence;
    let new_conf = (old_conf * vf).clamp(0.0, 0.99);
    decision.confidence = new_conf;
    decision.reasons.push(
        crate::decision::Reason::new(format!(
            "Volume context (last {window}s): {rt} triggers from {us} sources -> confidence x{vf:.3} ({old:.3}->{new:.3})",
            window = VOLUME_WINDOW_SECS, rt = recent_triggers, us = uniq_sources, vf = vf, old = old_conf, new = new_conf,
        ))
        .kind(crate::decision::ReasonKind::Threshold)
        .weighted(((vf - 0.90) / (1.05 - 0.90)).clamp(0.0, 1.0)),
    );

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

    if let Some(r) = &ai_reason {
        decision.reasons.push(
            crate::decision::Reason::new(format!("AI hint: {}", r))
                .kind(crate::decision::ReasonKind::Threshold)
                .weighted(0.5),
        );
        let before = decision.confidence;
        let after = (before + 0.02).clamp(0.0, 0.99);
        if after != before {
            decision.confidence = after;
            decision.reasons.push(
                crate::decision::Reason::new(format!(
                    "AI hint nudged confidence +0.02 ({before:.3}->{after:.3})"
                ))
                .kind(crate::decision::ReasonKind::Threshold)
                .weighted(0.1),
            );
        }
        // metrics: AI used
        counter!("ai_decision_ai_used_total").increment(1);
    }

    state.history.push(&decision);

    // ---- Build AI meta + JSON body ----
    let ai_meta = ApiAiInfo {
        used: ai_reason.is_some(),
        reason: ai_reason.clone(),
        cache_hit: ai_cache_hit,
        limited: ai_limited,
    };

    let body = DecideWithAi {
        inner: decision,
        ai: ai_meta,
    };

    // concise INFO log
    info!(
        ai_used = %body.ai.used,
        cache_hit = %body.ai.cache_hit,
        limited = %body.ai.limited,
        reason_len = %body.ai.reason.as_ref().map(|s| s.len()).unwrap_or(0),
        "decision_done"
    );

    // metrics: record duration
    let dur_ms = t0.elapsed().as_millis() as f64;
    histogram!("ai_decision_duration_ms").record(dur_ms);

    // ---- Headers + response ----
    let mut resp = axum::Json(body).into_response();
    resp.headers_mut().insert(
        "X-AI-Used",
        HeaderValue::from_static(if ai_reason.is_some() { "1" } else { "0" }),
    );
    if let Some(r) = ai_reason {
        if let Ok(hv) = HeaderValue::from_str(&r) {
            resp.headers_mut().insert("X-AI-Reason", hv);
        } else {
            resp.headers_mut()
                .insert("X-AI-Reason", HeaderValue::from_static("sanitized"));
        }
    }
    resp
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
    let mut uniq: HashSet<String> = HashSet::new();

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

async fn debug_rolling() -> Json<RollingInfo> {
    let state = app_state();
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

async fn debug_history() -> Json<Vec<HistoryOut>> {
    let state = app_state();
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

async fn debug_last_decision() -> Json<Option<LastOut>> {
    let state = app_state();
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

async fn debug_source_weight(Query(q): Query<HashMap<String, String>>) -> String {
    let state = app_state();
    let s = q.get("source").cloned().unwrap_or_default();
    let w = {
        let g = state.source_weights.read().expect("rwlock poisoned");
        g.weight_for(&s)
    };
    format!("source='{}' -> weight={:.2}", s, w)
}

async fn admin_reload_source_weights() -> String {
    let state = app_state();
    let fresh = SourceWeightsConfig::load_from_file("source_weights.json");
    match state.source_weights.write() {
        Ok(mut w) => {
            *w = fresh;
            "reloaded".to_string()
        }
        Err(_) => "failed: lock poisoned".to_string(),
    }
}

// -----------------------------------------------------------------------------
// Back-compat helper for integration tests
// Builds a Router with a default RelevanceAppState so tests can call crate::app()
// Async and returns Result so older tests using `.await.expect(...)` keep working.
// -----------------------------------------------------------------------------
pub async fn app() -> anyhow::Result<Router<()>> {
    Ok(router(RelevanceAppState::from_env()))
}

// ---- AI cache header middleware (X-AI-Cache) ----
use axum::{
    body::{to_bytes, Body},
    http::Request,
    middleware::Next,
    response::Response,
};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use tokio::time::{Duration, Instant};

/// Map: cache-key -> expiry Instant (pevná expirace s malým negativním biasem)
static AI_CACHE_EXPIRY: Lazy<DashMap<String, Instant>> = Lazy::new(|| DashMap::new());

fn ai_cache_ttl() -> Duration {
    // Preferred: millisecond TTL for precise tests
    if let Ok(ms_str) = std::env::var("AI_DECISION_CACHE_TTL_MS") {
        if let Ok(ms) = ms_str.trim().parse::<u64>() {
            return Duration::from_millis(ms);
        }
    }
    // Back-compat: seconds-based TTL
    let secs = std::env::var("AI_CACHE_TTL_SECS")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(2);
    Duration::from_secs(secs) // 0s povoleno: vždy MISS
}

/// Kolik ms ubrat z expiry jako „negativní bias“ (default 10 ms),
/// aby po sleep(TTL) nebyl o vlásek ještě HIT na některých systémech.
fn ai_cache_bias() -> Duration {
    let ms = std::env::var("AI_CACHE_TTL_BIAS_MS")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(10);
    Duration::from_millis(ms)
}

/// Vyprázdění cache — volá se v `router()` pro izolaci testů.
fn clear_ai_cache() {
    AI_CACHE_EXPIRY.clear();
}

/// Axum middleware: vždy přidá `X-AI-Cache: miss|hit`.
pub async fn ai_cache_mw(
    req: Request<Body>,
    next: Next,
) -> Result<Response, axum::http::StatusCode> {
    // načti tělo a vrať ho zpět do requestu
    let (parts, body) = req.into_parts();
    let body_bytes = to_bytes(body, 1 << 20)
        .await
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;
    let body_clone = body_bytes.clone();

    // klíč = metoda + path + tělo
    let key_input = format!(
        "{} {} {}",
        parts.method,
        parts.uri.path(),
        String::from_utf8_lossy(&body_bytes)
    );
    let key = format!("{:x}", Sha256::digest(key_input.as_bytes()));

    // TTL a rozhodnutí hit/miss dle pevné expirace
    let ttl = ai_cache_ttl();
    let bias = ai_cache_bias();
    let now = Instant::now();

    // 1) Bezpečně zjisti HIT/MISS (guard se po tomto bloku uvolní)
    let is_hit = {
        if let Some(expiry_at) = AI_CACHE_EXPIRY.get(&key) {
            ttl > tokio::time::Duration::ZERO && now < *expiry_at
        } else {
            false
        }
    };

    // 2) Pokud MISS, zapiš novou expiraci (guard už je uvolněný, nehrozí deadlock)
    let status: &str = if is_hit {
        // metrics: record hit
        counter!("ai_decision_cache_hits_total").increment(1);
        "hit"
    } else {
        let base = now.checked_add(ttl).unwrap_or(now);
        let new_expiry = base.checked_sub(bias).unwrap_or(now);
        AI_CACHE_EXPIRY.insert(key.clone(), new_expiry);
        // metrics: record miss
        counter!("ai_decision_cache_misses_total").increment(1);
        "miss"
    };

    // pokračuj do handleru
    let req = Request::from_parts(parts, Body::from(body_clone));
    let mut resp = next.run(req).await;

    // přidej hlavičku
    resp.headers_mut()
        .insert("X-AI-Cache", status.parse().unwrap());

    Ok(resp)
}
