//! Integration tests for decision endpoint cache behavior with AI mock.
//!
//! Covered (strict):
//! - MISS → HIT for identical request (via `X-AI-Cache` header)
//! - MISS when text changes by one character (then HIT on immediate repeat)
//! - Expiration/TTL driven by `AI_DECISION_CACHE_TTL_MS` env (short TTL for determinism)
//! - Presence of cache diagnostics header `X-AI-Cache`
//!
//! Endpoint: POST /decide
//! Payload: {"text": "..."} (shape taken from existing API tests)

use axum::{
    body::Body,
    http::{HeaderMap, Request, StatusCode},
    Router,
};
use once_cell::sync::Lazy;
use serde_json::json;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;
use tower::ServiceExt; // for oneshot

// --- Global serialization of tests that mutate env / shared cache ---
static TEST_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn guarded_lock<'a>() -> std::sync::MutexGuard<'a, ()> {
    match TEST_GUARD.lock() {
        Ok(g) => g,
        Err(poison) => poison.into_inner(),
    }
}

/// Build the in-process app router the same way as in `tests/api_http.rs`.
async fn build_app() -> Router {
    dow_sentiment_analyzer::app()
        .await
        .expect("app() should build Router in tests")
}

/// Helper: POST /decide with given text. Returns (status, headers).
async fn post_decide_text(app: &Router, text: &str) -> (StatusCode, HeaderMap) {
    let payload = json!({ "text": text });
    let body = Body::from(serde_json::to_vec(&payload).expect("serialize payload"));

    let req = Request::builder()
        .method("POST")
        .uri("/decide")
        .header("content-type", "application/json")
        .body(body)
        .expect("request build");

    let resp = app.clone().oneshot(req).await.expect("router response");
    (resp.status(), resp.headers().clone())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheDetect {
    Hit,
    Miss,
}

fn header_cache_signal(headers: &HeaderMap) -> CacheDetect {
    let v = headers
        .get("X-AI-Cache")
        .expect("X-AI-Cache header must be present")
        .to_str()
        .expect("X-AI-Cache header must be valid ASCII")
        .trim()
        .to_ascii_uppercase();
    match v.as_str() {
        "HIT" => CacheDetect::Hit,
        "MISS" => CacheDetect::Miss,
        other => panic!("X-AI-Cache must be HIT or MISS, got: {other}"),
    }
}

/// Set the env needed for AI mock and disable restrictive gating.
fn set_common_env(unique_suffix: &str) {
    std::env::set_var("AI_TEST_MODE", "mock");
    std::env::set_var("AI_ENABLED", "1");
    std::env::set_var("AI_ONLY_TOP_SOURCES", "0");
    // Optional legacy var (harmless if unused by app)
    std::env::set_var(
        "AI_DECISION_CACHE_DIR",
        format!("cache/test_decision_cache_{}", unique_suffix),
    );
    // Enable debug subfields if present (optional)
    std::env::set_var("AI_DEBUG", "1");
}

/// Sleep noticeably longer than TTL to avoid boundary flakes.
/// Using 5× TTL gives headroom even on slow CI/Windows timers.
async fn sleep_over_ttl(ttl_ms: u64) {
    let total = ttl_ms.saturating_mul(5);
    sleep(Duration::from_millis(total)).await;
}

fn unique_nonce(tag: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_millis(0))
        .as_nanos();
    format!("cache-test:{}:{}", tag, now)
}

fn with_nonce(nonce: &str, base: &str) -> String {
    format!("[{}] {}", nonce, base)
}

// --- TESTS ---

#[tokio::test]
async fn cache_miss_then_hit_for_identical_request() {
    let _lock = guarded_lock();
    set_common_env("miss_then_hit");
    // Keep TTL sane but not too short; not used in this test specifically
    std::env::set_var("AI_DECISION_CACHE_TTL_MS", "30000");

    let app = build_app().await;

    let nonce = unique_nonce("miss_then_hit");
    let text = with_nonce(&nonce, "Breaking: Fed signals possible rate hold.");

    // First & second calls -> MISS then HIT
    let (s1, h1) = post_decide_text(&app, &text).await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(
        header_cache_signal(&h1),
        CacheDetect::Miss,
        "first identical request should be MISS"
    );

    let (s2, h2) = post_decide_text(&app, &text).await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(
        header_cache_signal(&h2),
        CacheDetect::Hit,
        "second identical request should be HIT"
    );
}

#[tokio::test]
async fn cache_miss_when_text_changes_one_char() {
    let _lock = guarded_lock();
    set_common_env("one_char_change");
    std::env::set_var("AI_DECISION_CACHE_TTL_MS", "30000");

    let app = build_app().await;

    // Same nonce across A/B to isolate just the one-char difference
    let nonce = unique_nonce("one_char_change");
    let text_a = with_nonce(&nonce, "Trump: Strong jobs report—markets rally.");
    let text_b = with_nonce(&nonce, "Trump: Strong jobs report—markets rally!"); // '.' -> '!'

    // Prime A (response irrelevant to B's cache key)
    let (sa1, _ha1) = post_decide_text(&app, &text_a).await;
    assert_eq!(sa1, StatusCode::OK);

    // Two calls with B -> MISS then HIT for changed text
    let (sb1, hb1) = post_decide_text(&app, &text_b).await;
    assert_eq!(sb1, StatusCode::OK);
    assert_eq!(
        header_cache_signal(&hb1),
        CacheDetect::Miss,
        "B1 should MISS for changed text"
    );

    let (sb2, hb2) = post_decide_text(&app, &text_b).await;
    assert_eq!(sb2, StatusCode::OK);
    assert_eq!(
        header_cache_signal(&hb2),
        CacheDetect::Hit,
        "B2 should HIT for changed text"
    );
}

#[tokio::test]
async fn cache_expires_after_ttl_and_turns_into_miss_again() {
    let _lock = guarded_lock();
    set_common_env("ttl_expiry");

    // Use a short TTL to prove expiration deterministically
    const TTL_MS: u64 = 50;
    std::env::set_var("AI_DECISION_CACHE_TTL_MS", TTL_MS.to_string());

    let app = build_app().await;

    let nonce = unique_nonce("ttl_expiry");
    let text = with_nonce(&nonce, "BoE signals pause; FTSE futures react.");

    // Warm-up: MISS -> HIT
    let (s1, h1) = post_decide_text(&app, &text).await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(
        header_cache_signal(&h1),
        CacheDetect::Miss,
        "first call should be MISS"
    );

    let (s2, h2) = post_decide_text(&app, &text).await;
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(
        header_cache_signal(&h2),
        CacheDetect::Hit,
        "second immediate call should be HIT"
    );

    // Wait well over TTL, then expect MISS again (absolute TTL, no sliding refresh)
    sleep_over_ttl(TTL_MS).await;

    let (s3, h3) = post_decide_text(&app, &text).await;
    assert_eq!(s3, StatusCode::OK);
    assert_eq!(
        header_cache_signal(&h3),
        CacheDetect::Miss,
        "after TTL expiration, identical request must be MISS"
    );

    // And the very next identical call should be HIT again
    let (s4, h4) = post_decide_text(&app, &text).await;
    assert_eq!(s4, StatusCode::OK);
    assert_eq!(
        header_cache_signal(&h4),
        CacheDetect::Hit,
        "immediately after refreshed compute, the next call must be HIT"
    );
}

#[tokio::test]
async fn cache_header_is_always_present_and_valid() {
    let _lock = guarded_lock();
    set_common_env("diag_presence");
    std::env::set_var("AI_DECISION_CACHE_TTL_MS", "30000");

    let app = build_app().await;

    let nonce = unique_nonce("diag_presence");
    let text = with_nonce(&nonce, "ECB comments stir euro volatility.");

    let (status, headers) = post_decide_text(&app, &text).await;
    assert_eq!(status, StatusCode::OK);

    let val = headers
        .get("X-AI-Cache")
        .expect("Expected X-AI-Cache header to be present")
        .to_str()
        .expect("Header should be valid ASCII");
    assert!(
        val.eq_ignore_ascii_case("HIT") || val.eq_ignore_ascii_case("MISS"),
        "X-AI-Cache header should be 'HIT' or 'MISS', got: {}",
        val
    );
}

// (Optional future test)
// #[ignore = "enable when input normalization is part of cache key semantics"]
// #[tokio::test]
// async fn cache_key_is_stable_for_semantically_same_input() { ... }
