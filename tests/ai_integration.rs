// tests/ai_integration.rs
//
// Integration tests for AI Adapter and /api/decide behavior using Axum 0.8.
// Notes:
// - shuttle-axum >= 0.56 uses Axum 0.8 by default.
// - These tests are marked #[ignore] until the router/startup helper is available.

use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderName};
use std::time::Duration;

fn ai_headers(hdrs: &HeaderMap) -> (Option<String>, Option<String>) {
    let used = HeaderName::from_static("x-ai-used");
    let reason = HeaderName::from_static("x-ai-reason");
    let u = hdrs.get(&used).and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    let r = hdrs.get(&reason).and_then(|v| v.to_str().ok()).map(|s| s.to_string());
    (u, r)
}

/// TODO:
/// - Replace the placeholder "BASE" with a started local server, e.g.:
///   let base = format!("http://{addr}");
///   where `addr` is from a spawned axum::Server with your project router.
///
/// Recommended endpoint: /api/decide
fn base_url() -> String {
    // For now, use a placeholder; adjust to your spawned server once wired.
    // Example when running the app locally: "http://127.0.0.1:8000"
    std::env::var("TEST_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8000".to_string())
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn borderline_triggers_ai_once_and_uses_cache_afterwards() -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let body = serde_json::json!([{
        "source": "Fed",
        "text": "Powell hints at uncertainty"
    }]);

    // Act 1: borderline -> expect AI call
    let r1 = client
        .post(format!("{}/api/decide", base_url()))
        .json(&body)
        .send()
        .await?;
    assert!(r1.status().is_success());
    let (u1, _rs1) = ai_headers(r1.headers());
    // "1" or "yes" depending on implementation
    assert!(u1.as_deref() == Some("1") || u1.as_deref() == Some("yes"));

    // Act 2: same input -> expect cache hit (still AI-used=yes, but cache flag in JSON)
    let r2 = client
        .post(format!("{}/api/decide", base_url()))
        .json(&body)
        .send()
        .await?;
    assert!(r2.status().is_success());
    let (u2, _rs2) = ai_headers(r2.headers());
    assert!(u2.as_deref() == Some("1") || u2.as_deref() == Some("yes"));

    // If the response body includes { "ai": { "cache_hit": true } }, you can assert it here:
    // let v: serde_json::Value = r2.json().await?;
    // assert_eq!(v["ai"]["cache_hit"], true);

    Ok(())
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn daily_limit_blocks_second_call() -> Result<()> {
    // Optionally set daily limit env var before starting the server in your test harness:
    // std::env::set_var("AI_DAILY_LIMIT", "1");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let body = serde_json::json!([{
        "source": "Fed",
        "text": "Powell hints at uncertainty"
    }]);

    // First call -> should use AI
    let r1 = client
        .post(format!("{}/api/decide", base_url()))
        .json(&body)
        .send()
        .await?;
    assert!(r1.status().is_success());
    let (u1, _rs1) = ai_headers(r1.headers());
    assert!(u1.as_deref() == Some("1") || u1.as_deref() == Some("yes"));

    // Second call (same day) -> daily limit reached -> should NOT use AI
    let r2 = client
        .post(format!("{}/api/decide", base_url()))
        .json(&body)
        .send()
        .await?;
    assert!(r2.status().is_success());
    let (u2, _rs2) = ai_headers(r2.headers());
    assert!(
        u2.is_none() || u2.as_deref() == Some("0") || u2.as_deref() == Some("no"),
        "expected AI to be skipped on daily-limit overflow"
    );

    Ok(())
}
