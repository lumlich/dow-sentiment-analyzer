use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::{fs, time};

use crate::notify::antiflutter::AntiFlutter;
use crate::notify::{DecisionKind, NotificationEvent, NotifierMux};

const STATE_PATH: &str = "state/last_decision.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LastState {
    decision: Option<DecisionKind>,
    confidence: Option<f32>,
    ts: Option<DateTime<Utc>>,
}

// --- tolerantní varianty odpovědi z /decide ---

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DecideResponseFlat {
    decision: String,
    confidence: f32,
    #[serde(default)]
    reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DecideResponseAlt {
    verdict: String,
    score: f32,
    #[serde(default)]
    reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum DecideAny {
    Flat(DecideResponseFlat),
    Alt(DecideResponseAlt),
    Wrapped { data: Box<DecideAny> },
}

fn map_decision(s: &str) -> DecisionKind {
    match s.to_ascii_uppercase().as_str() {
        "BUY" => DecisionKind::BUY,
        "SELL" => DecisionKind::SELL,
        "HOLD" => DecisionKind::HOLD,
        _ => DecisionKind::HOLD,
    }
}

fn map_any(any: DecideAny) -> (DecisionKind, f32, Vec<String>) {
    match any {
        DecideAny::Flat(DecideResponseFlat {
            decision,
            confidence,
            reasons,
        }) => (map_decision(&decision), confidence, reasons),
        DecideAny::Alt(DecideResponseAlt {
            verdict,
            score,
            reasons,
        }) => (map_decision(&verdict), score, reasons),
        DecideAny::Wrapped { data } => map_any(*data),
    }
}

async fn fetch_decision(endpoint: &str) -> Result<(DecisionKind, f32, Vec<String>)> {
    let client = reqwest::Client::new();
    let resp = client.get(endpoint).send().await.context("fetch /decide")?;
    let status = resp.status();
    let body = resp.text().await.context("read /decide body")?;

    let trimmed = body.trim();

    // Tiché prázdno / null → přeskakujeme tick, ale srozumitelně zalogujeme
    if trimmed.is_empty() || trimmed == "null" {
        anyhow::bail!("decide returned empty/null with status {status}");
    }

    // Zkusíme tolerantní parse
    let any: DecideAny = serde_json::from_str(trimmed)
        .with_context(|| format!("parse /decide JSON failed, body: {trimmed}"))?;

    Ok(map_any(any))
}

async fn read_state() -> LastState {
    match fs::read_to_string(STATE_PATH).await {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => LastState::default(),
    }
}

async fn write_state(s: &LastState) {
    if let Err(e) = fs::create_dir_all("state").await {
        tracing::warn!("state dir: {e:#}");
    }
    if let Err(e) = fs::write(STATE_PATH, serde_json::to_vec_pretty(s).unwrap_or_default()).await {
        tracing::warn!("write state: {e:#}");
    }
}

pub async fn run_change_detector() -> Result<()> {
    let interval_secs: u64 = std::env::var("CHECK_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    let endpoint = std::env::var("DECIDE_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:8000/api/decide".to_string());

    let mut ticker = time::interval(time::Duration::from_secs(interval_secs));
    let mux = NotifierMux::from_env();

    let mut state = read_state().await;
    let mut af = {
        let cd_secs: i64 = std::env::var("ALERT_COOLDOWN_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_800); // 3h
        AntiFlutter::new(cd_secs)
    };

    loop {
        ticker.tick().await;

        let now = Utc::now();
        match fetch_decision(&endpoint).await {
            Ok((kind, conf, reasons)) => {
                if state.decision != Some(kind) {
                    if af.should_alert(kind, now) {
                        let ev = NotificationEvent {
                            decision: kind,
                            confidence: conf,
                            reasons: reasons.clone(),
                            ts: now,
                        };
                        mux.notify(&ev).await;
                        af.record_alert(kind, now);
                    } else {
                        tracing::debug!("suppressed by antiflutter: {:?}", kind);
                    }
                    state.decision = Some(kind);
                    state.confidence = Some(conf);
                    state.ts = Some(now);
                    write_state(&state).await;
                } else {
                    tracing::trace!("no change: {:?}", kind);
                }
            }
            Err(e) => {
                tracing::warn!("change-detector tick failed: {e:#}");
            }
        }
    }
}
