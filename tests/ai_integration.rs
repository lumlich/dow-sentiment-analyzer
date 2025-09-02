// tests/ai_integration.rs
//
// E2E tests for /api/decide AI metadata & cache behavior against a running Shuttle app.
// Run with the app started in another terminal via `cargo shuttle run`.
// In this terminal, set: TEST_BASE_URL=http://127.0.0.1:8000
//
// Example:
//   # Terminal A:
//   cargo shuttle run
//   # Terminal B (PowerShell):
//   $env:TEST_BASE_URL = "http://127.0.0.1:8000"
//   cargo test --test ai_integration -- --ignored
//
// Notes:
// - We do NOT force AI to be used; that depends on your runtime config (relevance threshold, bands, etc.).
// - Instead, we assert presence of AI metadata and header↔JSON consistency.
// - If AI is used on the first call, we expect a cache hit on the second call.

use anyhow::Result;
use reqwest::header::{HeaderMap, HeaderName};
use std::time::Duration;

fn base_url() -> String {
    std::env::var("TEST_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8000".to_string())
}

fn get_ai_used_header(h: &HeaderMap) -> Option<String> {
    let key = HeaderName::from_static("x-ai-used");
    h.get(&key).and_then(|v| v.to_str().ok()).map(|s| s.to_string())
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn ai_metadata_present_and_consistent() -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    // Stronger input to pass relevance (macro + hard): mentions Powell, Fed/rates, and Dow Jones.
    let body = serde_json::json!([{
        "source": "Fed",
        "text": "Powell says the Fed may cut rates; Dow Jones futures slip"
    }]);

    let resp = client
        .post(format!("{}/api/decide", base_url()))
        .json(&body)
        .send()
        .await?;
    assert!(resp.status().is_success(), "status should be 2xx");

    let headers = resp.headers().clone();
    let hdr_used = get_ai_used_header(&headers);

    let v: serde_json::Value = resp.json().await?;
    assert!(v.get("ai").is_some(), "response JSON must include 'ai'");
    assert!(v["ai"].get("used").is_some(), "ai.used must be present");
    assert!(v["ai"].get("cache_hit").is_some(), "ai.cache_hit must be present");
    assert!(v["ai"].get("limited").is_some(), "ai.limited must be present");

    // Header ↔ JSON consistency
    let used_json = v["ai"]["used"].as_bool().unwrap_or(false);
    match hdr_used.as_deref() {
        Some("1") | Some("yes") => assert!(used_json, "header says AI used, but JSON says false"),
        Some("0") | Some("no") => assert!(!used_json, "header says AI NOT used, but JSON says true"),
        _ => {
            // Header not present: accept; JSON still must have ai.used
            assert!(v["ai"].get("used").is_some());
        }
    }

    Ok(())
}

#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn cache_hit_consistency_when_ai_used() -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let body = serde_json::json!([{
        "source": "Fed",
        "text": "Powell says the Fed may cut rates; Dow Jones futures slip"
    }]);

    // First call
    let r1 = client
        .post(format!("{}/api/decide", base_url()))
        .json(&body)
        .send()
        .await?;
    assert!(r1.status().is_success(), "first call should be 2xx");
    let v1: serde_json::Value = r1.json().await?;
    let used1 = v1["ai"]["used"].as_bool().unwrap_or(false);

    // Second call (same input)
    let r2 = client
        .post(format!("{}/api/decide", base_url()))
        .json(&body)
        .send()
        .await?;
    assert!(r2.status().is_success(), "second call should be 2xx");
    let v2: serde_json::Value = r2.json().await?;
    let used2 = v2["ai"]["used"].as_bool().unwrap_or(false);
    let cache2 = v2["ai"]["cache_hit"].as_bool().unwrap_or(false);

    // If AI was used on the first call, the second should mark cache_hit=true.
    if used1 {
        assert!(used2, "if AI was used first, we expect it marked used on second too");
        assert!(cache2, "second call should report cache_hit=true when AI is used");
    } else {
        // If AI was not used initially (due to relevance/band/limits),
        // cache_hit must remain false (no AI output to cache).
        assert!(!cache2, "cache_hit should be false when AI was not used on the first call");
    }

    Ok(())
}
