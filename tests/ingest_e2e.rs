#![cfg(feature = "strict-e2e")] // compile & run only when explicitly enabled

use serde_json::json;
use shuttle_axum::axum::{body::Body, http::Request};
use tower::ServiceExt;

/// Strict E2E smoke (optional): exercise /decide with a simple payload.
/// Enable via: `cargo test --features strict-e2e --test ingest_e2e`
#[tokio::test]
async fn strict_ingest_e2e_decide_smoke() {
    let app = dow_sentiment_analyzer::app().await.expect("build app");

    let req = Request::builder()
        .method("POST")
        .uri("/decide")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({ "source": "test", "text": "Fed cuts rates by 50 bps" }).to_string(),
        ))
        .expect("build request");

    let resp = app.oneshot(req).await.expect("call /decide");
    assert!(resp.status().is_success(), "POST /decide should be 2xx");
}
