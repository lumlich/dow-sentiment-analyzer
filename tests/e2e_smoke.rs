// tests/e2e_smoke.rs

use dow_sentiment_analyzer::api;
use dow_sentiment_analyzer::relevance::{
    AppState as RelevanceAppState, RelevanceEngine, RelevanceHandle,
};
use shuttle_axum::axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use tower::ServiceExt; // for `oneshot` (tower 0.5 with features=["util"])

#[tokio::test]
async fn smoke_decide_cpi() {
    // Point relevance engine to the local config file used in dev/tests
    std::env::set_var("RELEVANCE_CONFIG_PATH", "config/relevance.toml");

    // Build a plain Axum Router without Shuttle runtime
    let engine = RelevanceEngine::from_toml().expect("load relevance config for tests");
    let handle = RelevanceHandle::new(engine);
    let app: Router = api::create_router(RelevanceAppState { relevance: handle });

    // Minimal POST /decide request (one item)
    let req = Request::builder()
        .method("POST")
        .uri("/decide")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"[{"text":"US CPI cools to 3.2% YoY; core CPI eases.","source":"Reuters"}]"#,
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Read body and assert it contains a decision field
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let s = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(s.contains("\"decision\""));
}

#[tokio::test]
async fn smoke_decide_mix_with_neutralization() {
    std::env::set_var("RELEVANCE_CONFIG_PATH", "config/relevance.toml");

    let engine = RelevanceEngine::from_toml().expect("load relevance config for tests");
    let handle = RelevanceHandle::new(engine);
    let app: Router = api::create_router(RelevanceAppState { relevance: handle });

    // Two items:
    // 1) CPI + Dow (should pass relevance)
    // 2) DJI + drone + negative verb (raw sentiment != 0) -> should be blocked by DJI-drones blocker,
    //    thus counted as "neutralized" in reasons.
    let req = Request::builder()
        .method("POST")
        .uri("/decide")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"[
                {"text":"CPI cools; the Dow rallies after the print.","source":"Reuters"},
                {"text":"DJI drone sales slump after Mavic recall.","source":"Tech"}
            ]"#,
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let s = String::from_utf8(bytes.to_vec()).unwrap();

    // Minimal E2E checks: decision exists and relevance gate neutralized something
    assert!(s.contains("\"decision\""), "response body: {s}");
    assert!(
        s.contains("Relevance gate neutralized"),
        "expected neutralization summary in reasons; body: {s}"
    );
}
