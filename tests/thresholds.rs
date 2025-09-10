// tests/thresholds.rs
//
// Self-calibrating boundary tests for BUY/SELL/HOLD via public /decide.
// Optimized with a cached Router (tokio::sync::OnceCell).

use axum::{
    body::{to_bytes, Body},
    http::Request,
};
use http::StatusCode;
use serde::Deserialize;
use tokio::sync::OnceCell;
use tower::ServiceExt; // for `oneshot`

use dow_sentiment_analyzer::app;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum DecisionKind {
    BUY,
    SELL,
    HOLD,
}

#[derive(Debug, Deserialize)]
struct DecideResponse {
    #[serde(default)]
    decision: Option<DecisionKind>,
    #[serde(default)]
    kind: Option<DecisionKind>,
}

impl DecideResponse {
    fn to_kind(self) -> DecisionKind {
        self.decision
            .or(self.kind)
            .expect("missing decision/kind in response")
    }
}

// --- Router cache (build once per test binary) ---
static ROUTER: OnceCell<axum::Router> = OnceCell::const_new();

async fn test_app() -> axum::Router {
    ROUTER
        .get_or_init(|| async { app().await.expect("app() should build a Router") })
        .await
        .clone()
}

async fn call_decide(score: f32) -> (StatusCode, DecisionKind) {
    let router = test_app().await;

    let uri = format!("/decide?score={score}");
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 256 * 1024).await.unwrap();

    let parsed: Result<DecideResponse, _> = serde_json::from_slice(&bytes);
    let body = match parsed {
        Ok(obj) => obj,
        Err(_) => {
            let s: String = serde_json::from_slice(&bytes).expect("invalid /decide string body");
            #[derive(Deserialize)]
            #[serde(rename_all = "UPPERCASE")]
            enum K {
                BUY,
                SELL,
                HOLD,
            }
            let k: K = serde_json::from_str(&format!("\"{s}\"")).expect("invalid decision string");
            let dk = match k {
                K::BUY => DecisionKind::BUY,
                K::SELL => DecisionKind::SELL,
                K::HOLD => DecisionKind::HOLD,
            };
            DecideResponse {
                decision: Some(dk),
                kind: None,
            }
        }
    };

    (status, body.to_kind())
}

#[inline]
fn round2(x: f32) -> f32 {
    (x * 100.0).round() / 100.0
}

#[inline]
fn step_forward(s: f32, step: f32) -> f32 {
    round2(s + step)
}

/// Find the smallest score in [start, end] (step > 0) that yields `target`.
async fn find_first_inclusive(
    start: f32,
    end: f32,
    step: f32,
    target: DecisionKind,
) -> Option<f32> {
    let mut s = round2(start);
    while s <= end + 1e-6 {
        let (_, k) = call_decide(s).await;
        if k == target {
            return Some(s);
        }
        s = step_forward(s, step);
    }
    None
}

#[tokio::test]
async fn neutral_midrange() {
    for s in [0.0, 0.2, -0.2] {
        let (st, k) = call_decide(s).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(k, DecisionKind::HOLD, "score {} should be HOLD", s);
    }
}

/// Discover BUY threshold; if none found, assert the whole band is HOLD.
#[tokio::test]
async fn buy_threshold_dynamic() {
    let step = 0.01;
    let start = 0.10;
    let end = 1.00;

    if let Some(first_buy) = find_first_inclusive(start, end, step, DecisionKind::BUY).await {
        eprintln!("Discovered BUY boundary at {}", first_buy);

        // One step below → HOLD
        let below = round2(first_buy - step);
        if below >= start {
            let (_, k_below) = call_decide(below).await;
            assert_eq!(
                k_below,
                DecisionKind::HOLD,
                "Expected HOLD just below BUY boundary"
            );
        }

        // At boundary → BUY
        let (_, k_at) = call_decide(first_buy).await;
        assert_eq!(
            k_at,
            DecisionKind::BUY,
            "Expected BUY at discovered boundary"
        );

        // A bit above → BUY
        let (_, k_above) = call_decide(round2(first_buy + step)).await;
        assert_eq!(
            k_above,
            DecisionKind::BUY,
            "BUY should persist above boundary"
        );
    } else {
        // No BUY boundary: all scanned scores must be HOLD.
        let mut s = start;
        while s <= end + 1e-6 {
            let (_, k) = call_decide(round2(s)).await;
            assert_eq!(
                k,
                DecisionKind::HOLD,
                "Expected HOLD across [{start}, {end}] when no BUY boundary is exposed; got {:?} at {}",
                k,
                s
            );
            s = step_forward(s, step);
        }
        eprintln!("No BUY boundary exposed; HOLD confirmed across [{start}, {end}].");
    }
}

/// Discover SELL threshold; if none found, assert the whole band is HOLD.
#[tokio::test]
async fn sell_threshold_dynamic() {
    let step = 0.01;
    let start = -1.00;
    let end = -0.10;

    if let Some(first_sell) = find_first_inclusive(start, end, step, DecisionKind::SELL).await {
        eprintln!("Discovered SELL boundary at {}", first_sell);

        // One step above (less negative) → HOLD
        let above = round2(first_sell + step);
        if above <= end {
            let (_, k_above) = call_decide(above).await;
            assert_eq!(
                k_above,
                DecisionKind::HOLD,
                "Expected HOLD just above SELL boundary"
            );
        }

        // At boundary → SELL
        let (_, k_at) = call_decide(first_sell).await;
        assert_eq!(
            k_at,
            DecisionKind::SELL,
            "Expected SELL at discovered boundary"
        );

        // A bit below (more negative) → SELL
        let (_, k_below) = call_decide(round2(first_sell - step)).await;
        assert_eq!(
            k_below,
            DecisionKind::SELL,
            "SELL should persist below boundary"
        );
    } else {
        // No SELL boundary: all scanned scores must be HOLD.
        let mut s = start;
        while s <= end + 1e-6 {
            let (_, k) = call_decide(round2(s)).await;
            assert_eq!(
                k,
                DecisionKind::HOLD,
                "Expected HOLD across [{start}, {end}] when no SELL boundary is exposed; got {:?} at {}",
                k,
                s
            );
            s = step_forward(s, step);
        }
        eprintln!("No SELL boundary exposed; HOLD confirmed across [{start}, {end}].");
    }
}
