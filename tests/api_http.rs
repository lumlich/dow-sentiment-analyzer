// tests/api_http.rs
//
// HTTP-level tests for the public API Router without opening sockets.
// We exercise the router directly via tower::ServiceExt::oneshot.
//
// Covered:
// - GET /health
// - POST /analyze
// - POST /batch
// - POST /decide  (headers + AI metadata presence)

use serde_json::json;
use serde_json::Value as Json;
use shuttle_axum::axum::{
    body::{self, Body},
    http::{Request, StatusCode},
    Router,
};
use tower::ServiceExt as _; // for `oneshot`

use dow_sentiment_analyzer::api;
use dow_sentiment_analyzer::relevance::AppState as RelevanceAppState;

const BODY_LIMIT: usize = 1 * 1024 * 1024; // 1MB, safe for tests

/// Build the same Router the binary uses.
fn test_router() -> Router {
    let state = RelevanceAppState::from_env();
    api::router(state)
}

#[tokio::test]
async fn api_health_returns_200_and_ok_body() {
    let app = test_router();

    let req = Request::builder()
        .method("GET")
        .uri("/health")
        .body(Body::empty())
        .expect("build GET /health");

    let resp = app.oneshot(req).await.expect("oneshot /health");
    assert_eq!(resp.status(), StatusCode::OK, "health should be 200");

    let bytes = body::to_bytes(resp.into_body(), BODY_LIMIT)
        .await
        .expect("read body")
        .to_vec();
    let body = String::from_utf8(bytes).expect("utf8");
    assert_eq!(body.trim(), "OK", "health body should be 'OK'");
}

#[tokio::test]
async fn api_analyze_returns_expected_json_fields() {
    let app = test_router();

    let payload = json!({ "text": "Fed signals patience; markets react positively." });
    let req = Request::builder()
        .method("POST")
        .uri("/analyze")
        .header("content-type", "application/json")
        .body(Body::from(payload.to_string()))
        .expect("build POST /analyze");

    let resp = app.oneshot(req).await.expect("oneshot /analyze");
    assert!(
        resp.status().is_success(),
        "POST /analyze should be 2xx, got {}",
        resp.status()
    );

    let bytes = body::to_bytes(resp.into_body(), BODY_LIMIT)
        .await
        .expect("read json")
        .to_vec();
    let v: Json = serde_json::from_slice(&bytes).expect("parse analyze json");

    // Contract checks for UI consumers
    assert!(v.get("decision").is_some(), "missing 'decision'");
    assert!(v.get("confidence").is_some(), "missing 'confidence'");
    assert!(v.get("reasons").is_some(), "missing 'reasons'");
    assert!(v.get("evidence").is_some(), "missing 'evidence'");
    assert!(v.get("contributors").is_some(), "missing 'contributors'");
}

#[tokio::test]
async fn api_batch_scores_multiple_items() {
    let app = test_router();

    let items = json!([
        { "source": "Reuters", "text": "Dow futures edge higher on dovish Fed." },
        { "source": "WSJ",     "text": "Mixed commentary on industrials; net neutral." }
    ]);
    let req = Request::builder()
        .method("POST")
        .uri("/batch")
        .header("content-type", "application/json")
        .body(Body::from(items.to_string()))
        .expect("build POST /batch");

    let resp = app.oneshot(req).await.expect("oneshot /batch");
    assert!(
        resp.status().is_success(),
        "POST /batch should be 2xx, got {}",
        resp.status()
    );

    let bytes = body::to_bytes(resp.into_body(), BODY_LIMIT)
        .await
        .expect("read json")
        .to_vec();
    let arr: Json = serde_json::from_slice(&bytes).expect("parse batch json");
    assert!(arr.is_array(), "batch response must be an array");
    assert_eq!(
        arr.as_array().unwrap().len(),
        2,
        "batch response length should match input"
    );
}

#[tokio::test]
async fn api_decide_sets_ai_headers_and_includes_ai_metadata() {
    let app = test_router();

    let body_json = json!([
        { "source": "Reuters", "text": "Powell says the Fed may cut rates; Dow futures slip." }
    ]);
    let req = Request::builder()
        .method("POST")
        .uri("/decide")
        .header("content-type", "application/json")
        .body(Body::from(body_json.to_string()))
        .expect("build POST /decide");

    let resp = app.oneshot(req).await.expect("oneshot /decide");
    assert!(
        resp.status().is_success(),
        "POST /decide should be 2xx, got {}",
        resp.status()
    );

    // Headers: X-AI-Used should be present ("1" when AI contributed, "0" otherwise)
    let used = resp
        .headers()
        .get("X-AI-Used")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    assert!(
        used == "0" || used == "1",
        "X-AI-Used must be '0' or '1', got '{used}'"
    );

    let bytes = body::to_bytes(resp.into_body(), BODY_LIMIT)
        .await
        .expect("read json")
        .to_vec();
    let v: Json = serde_json::from_slice(&bytes).expect("parse decide json");

    let ai_obj = v.get("ai").expect("response JSON must include 'ai' object");
    assert!(ai_obj.get("used").is_some(), "ai.used missing");
    assert!(
        ai_obj.get("cache_hit").is_some() && ai_obj.get("limited").is_some(),
        "ai.cache_hit / ai.limited missing"
    );
}
