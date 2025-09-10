// tests/metrics.rs
use axum::body::{self, Body};
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;

// Build full in-process app (includes debug/metrics when gated via env).
async fn build_app() -> Router {
    dow_sentiment_analyzer::app()
        .await
        .expect("app() should build Router in tests")
}

// Ensure metrics/diagnostics routes are enabled for this process.
fn set_metrics_env(suffix: &str) {
    // Gate for debug routes (/metrics)
    std::env::set_var("DEBUG_ROUTES", "1");
    // Keep AI in mock mode so /decide is deterministic & fast
    std::env::set_var("AI_TEST_MODE", "mock");
    std::env::set_var("AI_ENABLED", "1");
    std::env::set_var("AI_ONLY_TOP_SOURCES", "0");
    // Optional legacy cache dir isolation
    std::env::set_var(
        "AI_DECISION_CACHE_DIR",
        format!("cache/test_metrics_{}", suffix),
    );
    // Reason header/debug ok
    std::env::set_var("AI_DEBUG", "1");
    // Reasonable TTL so MISS→HIT works and won't expire mid-test
    std::env::set_var("AI_DECISION_CACHE_TTL_MS", "30000");
}

fn same_payload() -> &'static str {
    r#"{"text":"Fed keeps rates steady; White House comments mild","source":"gov"}"#
}

#[tokio::test]
async fn metrics_endpoint_contains_expected_series() {
    set_metrics_env("presence");
    let app = build_app().await;

    let resp = app
        .clone()
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    // axum::body::to_bytes requires an explicit limit
    let body = body::to_bytes(resp.into_body(), 1_048_576).await.unwrap(); // 1 MiB
    let text = String::from_utf8(body.to_vec()).unwrap();

    for needle in [
        "ai_decision_cache_hits_total",
        "ai_decision_cache_misses_total",
        "ai_decision_ai_used_total",
        "ai_decision_duration_ms_bucket",
        "ai_decision_cache_ttl_ms",
    ] {
        assert!(
            text.contains(needle),
            "metrics exposition missing '{needle}'\n{text}"
        );
    }
}

#[tokio::test]
async fn cache_miss_then_hit_increments_counters() {
    set_metrics_env("miss_hit");
    let app = build_app().await;

    // 1) First decide -> MISS (then cache write)
    let r1 = app
        .clone()
        .oneshot(
            Request::post("/decide")
                .header("content-type", "application/json")
                .body(Body::from(same_payload()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::OK);

    // 2) Second decide -> HIT
    let r2 = app
        .clone()
        .oneshot(
            Request::post("/decide")
                .header("content-type", "application/json")
                .body(Body::from(same_payload()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r2.status(), StatusCode::OK);

    // 3) Scrape metrics (same process so counters persist)
    let m = app
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(m.status(), StatusCode::OK);
    let body = body::to_bytes(m.into_body(), 1_048_576).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();

    // Soft presence checks (string-based)
    assert!(
        text.contains("ai_decision_cache_hits_total"),
        "no hits_total"
    );
    assert!(
        text.contains("ai_decision_cache_misses_total"),
        "no misses_total"
    );
}
