// tests/api_ai_negative.rs
//
// Negative AI behavior tests:
// - AI disabled via AI_ENABLED=0 -> X-AI-Used: 0
// - AI daily limit set to 0 -> X-AI-Used: 0

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

fn reset_ai_env() {
    for k in ["AI_ENABLED", "AI_DAILY_LIMIT"] {
        std::env::remove_var(k);
    }
}

#[tokio::test]
async fn ai_disabled_sets_x_ai_used_0() {
    reset_ai_env();
    std::env::set_var("AI_ENABLED", "0");

    let app = dow_sentiment_analyzer::api::app()
        .await
        .expect("build Router");

    let req = Request::builder()
        .method("POST")
        .uri("/decide")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"[{"source":"Fed","text":"Fed signals rate cuts coming"}]"#,
        ))
        .expect("request");

    let resp = app.clone().oneshot(req).await.expect("call /decide");
    assert_eq!(resp.status(), StatusCode::OK);

    let used = resp.headers().get("X-AI-Used").unwrap().to_str().unwrap();
    assert_eq!(used, "0", "AI should be disabled when AI_ENABLED=0");
}

#[tokio::test]
async fn ai_daily_limit_zero_disables_ai() {
    reset_ai_env();
    std::env::set_var("AI_DAILY_LIMIT", "0");

    let app = dow_sentiment_analyzer::api::app()
        .await
        .expect("build Router");

    let req = Request::builder()
        .method("POST")
        .uri("/decide")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"[{"source":"Reuters","text":"Markets await FOMC; yields cool"}]"#,
        ))
        .expect("request");

    let resp = app.clone().oneshot(req).await.expect("call /decide");
    assert_eq!(resp.status(), StatusCode::OK);

    let used = resp.headers().get("X-AI-Used").unwrap().to_str().unwrap();
    assert_eq!(used, "0", "AI should not be used when AI_DAILY_LIMIT=0");
}
