// tests/e2e_smoke.rs
//
// E2E smoke tests for the /decide endpoint using Axum 0.7.
// We use shuttle_axum::axum reexports to keep the Router type identical
// to the one used by the binary (prevents multiple-Axum-type mismatch).

use std::{net::SocketAddr, time::Duration};

use reqwest::StatusCode;
use shuttle_axum::axum::{serve, Router};
use tokio::net::TcpListener;

use dow_sentiment_analyzer::api;
use dow_sentiment_analyzer::relevance::{
    AppState as RelevanceAppState, RelevanceEngine, RelevanceHandle,
};

/// Spawn an Axum server on 127.0.0.1:0 (ephemeral port) and return the bound address.
async fn spawn_server(app: Router) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        serve(listener, app.into_make_service())
            .await
            .expect("axum serve");
    });
    // Tiny delay to let the server accept connections.
    tokio::time::sleep(Duration::from_millis(60)).await;
    (addr, handle)
}

#[tokio::test]
async fn smoke_decide_cpi() {
    // Point relevance engine to the local config file used in dev/tests.
    std::env::set_var("RELEVANCE_CONFIG_PATH", "config/relevance.toml");

    // Build a plain Router (no Shuttle runtime).
    let engine = RelevanceEngine::from_toml().expect("load relevance config for tests");
    let handle = RelevanceHandle::new(engine);
    let app: Router = api::router(RelevanceAppState { relevance: handle });

    let (addr, _jh) = spawn_server(app).await;
    let client = reqwest::Client::new();

    // Minimal POST /decide (one item).
    let body = r#"[{"text":"US CPI cools to 3.2% YoY; core CPI eases.","source":"Reuters"}]"#;

    let resp = client
        .post(format!("http://{addr}/decide"))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .expect("http call");

    assert_eq!(resp.status(), StatusCode::OK);

    // Ensure the response contains a "decision" field.
    let s = resp.text().await.expect("read body");
    assert!(
        s.contains("\"decision\""),
        "expected a `decision` field in response JSON, got: {s}"
    );
}

#[tokio::test]
async fn smoke_decide_mix_with_neutralization() {
    std::env::set_var("RELEVANCE_CONFIG_PATH", "config/relevance.toml");

    let engine = RelevanceEngine::from_toml().expect("load relevance config for tests");
    let handle = RelevanceHandle::new(engine);
    let app: Router = api::router(RelevanceAppState { relevance: handle });

    let (addr, _jh) = spawn_server(app).await;
    let client = reqwest::Client::new();

    // Two items: one relevant, one expected to be neutralized by rules.
    let body = r#"
        [
            {"text":"CPI cools; the Dow rallies after the print.","source":"Reuters"},
            {"text":"DJI drone sales slump after Mavic recall.","source":"Tech"}
        ]
    "#;

    let resp = client
        .post(format!("http://{addr}/decide"))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .expect("http call");

    assert_eq!(resp.status(), StatusCode::OK);

    let s = resp.text().await.expect("read body");

    // Minimal E2E checks.
    assert!(
        s.contains("\"decision\""),
        "expected a `decision` field in response JSON, got: {s}"
    );
    assert!(
        s.contains("Relevance gate neutralized"),
        "expected neutralization summary in reasons; body: {s}"
    );
}
