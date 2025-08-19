use std::{collections::VecDeque, sync::Mutex, time::Instant};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use shuttle_axum::axum::{routing::get, extract::Query, Json, Router};

const HISTORY_CAP: usize = 500;
const LAT_CAP: usize = 200;
const SLOW_REQ_MS: u128 = 1_000;

#[derive(Clone, Serialize, Deserialize)]
pub struct Decision {
    pub at_ms: u128,
    pub source: String,
    pub score: i32,
    pub verdict: String,
}

#[derive(Default, Clone, Serialize)]
pub struct Stats {
    pub total_requests: u64,
    pub analyze_requests: u64,
    pub batch_requests: u64,
    pub last_disruption_ms: Option<u128>,
    pub rolling_avg_ms: Option<f64>,
}

static HISTORY: Lazy<Mutex<VecDeque<Decision>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(HISTORY_CAP)));
static STATS: Lazy<Mutex<Stats>> =
    Lazy::new(|| Mutex::new(Stats::default()));
static LAT_MS: Lazy<Mutex<VecDeque<u128>>> =
    Lazy::new(|| Mutex::new(VecDeque::with_capacity(LAT_CAP)));

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub limit: Option<usize>,
}

pub fn router() -> Router {
    Router::new()
        .route("/history", get(history))
        .route("/stats", get(stats))
}

pub fn record_request(is_batch: bool) {
    let mut s = STATS.lock().unwrap();
    s.total_requests += 1;
    if is_batch {
        s.batch_requests += 1;
    } else {
        s.analyze_requests += 1;
    }
}

pub fn record_latency(lat_ms: u128) {
    let mut q = LAT_MS.lock().unwrap();
    if q.len() >= LAT_CAP {
        q.pop_front();
    }
    q.push_back(lat_ms);

    let mut s = STATS.lock().unwrap();
    let sum: u128 = q.iter().copied().sum();
    s.rolling_avg_ms = Some(sum as f64 / q.len() as f64);

    if lat_ms > SLOW_REQ_MS {
        s.last_disruption_ms = Some(lat_ms);
    }
}

pub fn record_decision(source: String, score: i32, verdict: String) {
    let mut h = HISTORY.lock().unwrap();
    if h.len() >= HISTORY_CAP {
        h.pop_front();
    }
    h.push_back(Decision {
        at_ms: now_ms(),
        source,
        score,
        verdict,
    });
}

async fn history(Query(q): Query<HistoryQuery>) -> Json<Vec<Decision>> {
    let limit = q.limit.unwrap_or(50);
    let h = HISTORY.lock().unwrap();
    let len = h.len();
    let start = len.saturating_sub(limit);
    Json(h.iter().skip(start).cloned().collect())
}

async fn stats() -> Json<Stats> {
    Json(STATS.lock().unwrap().clone())
}

fn now_ms() -> u128 {
    static START: Lazy<Instant> = Lazy::new(Instant::now);
    START.elapsed().as_millis()
}