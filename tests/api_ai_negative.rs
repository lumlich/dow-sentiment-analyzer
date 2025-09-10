// tests/api_ai_negative.rs

use axum::body::Body;
use axum::http::{Request, StatusCode};
use dow_sentiment_analyzer::app; // root-level app()
use std::env;
use tower::ServiceExt; // for `oneshot`

fn assert_boolish_false(val: &str) {
    assert!(
        val.eq_ignore_ascii_case("false") || val == "0",
        "expected x-ai-used=false/0 but got {val}"
    );
}

fn get_header<'a>(headers: &'a axum::http::HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

#[tokio::test]
async fn decide_with_ai_disabled() {
    // Explicitly disable AI
    env::set_var("OPENAI_API_KEY", "");
    env::set_var("AI_TEST_MODE", "mock"); // deterministic behavior

    let app = app().await.expect("failed to build app");

    let req = Request::builder()
        .method("POST")
        .uri("/decide")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"[{"source": "Trump", "text": "Great for economy!"}]"#,
        ))
        .expect("failed to build request");

    let resp = app.oneshot(req).await.expect("request failed");
    assert_eq!(resp.status(), StatusCode::OK);

    let headers = resp.headers();
    let used = get_header(headers, "x-ai-used").expect("missing x-ai-used header");
    assert_boolish_false(used);

    // Reason may or may not be set when AI is disabled; if present, must be "off" or "disabled"
    if let Some(reason) = get_header(headers, "x-ai-reason") {
        assert!(
            reason.eq_ignore_ascii_case("off") || reason.eq_ignore_ascii_case("disabled"),
            "unexpected x-ai-reason for disabled AI: {reason}"
        );
    }
}

#[tokio::test]
async fn decide_with_provider_error() {
    env::remove_var("OPENAI_API_KEY"); // ensure not set
    env::set_var("AI_TEST_MODE", "error");

    let app = app().await.expect("failed to build app");

    let req = Request::builder()
        .method("POST")
        .uri("/decide")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"[{"source": "Fed", "text": "Uncertain outlook"}]"#,
        ))
        .expect("failed to build request");

    let resp = app.oneshot(req).await.expect("request failed");
    assert_eq!(resp.status(), StatusCode::OK);

    let headers = resp.headers();
    let used = get_header(headers, "x-ai-used").expect("missing x-ai-used header");
    assert_boolish_false(used);

    // Reason is optional; if present, must be "error"
    if let Some(reason) = get_header(headers, "x-ai-reason") {
        assert!(
            reason.eq_ignore_ascii_case("error"),
            "expected x-ai-reason=error, got {reason}"
        );
    }
}

#[tokio::test]
async fn decide_with_daily_limit_reached() {
    env::remove_var("OPENAI_API_KEY");
    env::set_var("AI_TEST_MODE", "daily-limit");

    let app = app().await.expect("failed to build app");

    let req = Request::builder()
        .method("POST")
        .uri("/decide")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"[{"source": "EU", "text": "New sanctions imposed"}]"#,
        ))
        .expect("failed to build request");

    let resp = app.oneshot(req).await.expect("request failed");
    assert_eq!(resp.status(), StatusCode::OK);

    let headers = resp.headers();
    let used = get_header(headers, "x-ai-used").expect("missing x-ai-used header");
    assert_boolish_false(used);

    // Reason is optional; if present, must be "daily-limit"
    if let Some(reason) = get_header(headers, "x-ai-reason") {
        assert!(
            reason.eq_ignore_ascii_case("daily-limit"),
            "expected x-ai-reason=daily-limit, got {reason}"
        );
    }
}
