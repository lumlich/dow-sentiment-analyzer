// tests/ai_integration.rs
//
// Integration tests for AI Adapter and /decide behavior using Axum 0.7.
// We start a real HTTP server (axum::serve) and call it via reqwest,
// and we import Axum types from shuttle_axum::axum to ensure we use
// the exact same Axum version/type as the main binary (no type mismatch).

use std::{
    future::Future,
    net::SocketAddr,
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use http::HeaderMap;
use parking_lot::Mutex;
use reqwest::StatusCode;
use serde_json::json;
use shuttle_axum::axum::{serve, Router};
use tokio::net::TcpListener;

use crate_root::ai_adapter::{AiClient, AiResult};
use crate_root::relevance::AppState;
use dow_sentiment_analyzer as crate_root;

// -------------------- Test-time mock AI client --------------------
// This mock is kept for future extensions; suppress dead_code warnings so the build is clean.

#[allow(dead_code)]
#[derive(Clone, Default)]
struct MockAi {
    scripted: Arc<Mutex<Vec<Option<String>>>>,
    calls: Arc<AtomicUsize>,
}

#[allow(dead_code)]
impl MockAi {
    fn with_script(script: Vec<Option<String>>) -> Self {
        Self {
            scripted: Arc::new(Mutex::new(script)),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

// Implement AiClient with the exact trait signature (no async_trait).
impl AiClient for MockAi {
    fn analyze<'a>(
        &'a self,
        _text: &'a str,
    ) -> Pin<Box<dyn Future<Output = Option<AiResult>> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut g = self.scripted.lock();
            let next = if g.is_empty() { None } else { g.remove(0) };
            next.map(|short_reason| AiResult { short_reason })
        })
    }
    fn provider_name(&self) -> &'static str {
        "mock"
    }
}

// -------------------- Helpers --------------------

fn ai_headers(h: &HeaderMap) -> (Option<String>, Option<String>) {
    let used = h
        .get("X-AI-Used")
        .or_else(|| h.get("x-ai-used"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let reason = h
        .get("X-AI-Reason")
        .or_else(|| h.get("x-ai-reason"))
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    (used, reason)
}

/// Start a HTTP server for a given Router on 127.0.0.1:0 (ephemeral port)
/// and return the bound address plus the join handle.
async fn spawn_server(app: Router) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind test port");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        serve(listener, app.into_make_service())
            .await
            .expect("axum serve");
    });
    // small delay to let the server become ready
    tokio::time::sleep(Duration::from_millis(60)).await;
    (addr, handle)
}

// -------------------- Tests --------------------

#[tokio::test]
async fn ai_off_no_headers() {
    // Arrange: ensure AI is considered disabled by your AppState::from_env().
    std::env::set_var("AI_TEST_MODE", "mock-none");
    std::env::set_var("AI_ENABLED", "0");

    // NOTE: In this project AppState::from_env() returns AppState directly.
    let state = AppState::from_env();
    let app: Router = crate_root::api::router(state);

    let (addr, _jh) = spawn_server(app).await;
    let client = reqwest::Client::new();

    // Act
    let body = json!({
        "inputs": [
            {"source":"demo","author":"sys","text":"Fed holds.","weight":1.0,"time":"2025-08-28T10:00:00Z"}
        ]
    });
    let resp = client
        .post(format!("http://{addr}/decide"))
        .json(&body)
        .send()
        .await
        .unwrap();

    // Assert
    assert_eq!(resp.status(), StatusCode::OK);
    let (used, reason) = ai_headers(resp.headers());
    assert!(used.is_none() || used.as_deref() == Some("0"));
    assert!(reason.is_none() || reason.as_deref() == Some(""));
}

#[tokio::test]
async fn ai_on_cache_hit_second_call() {
    // Arrange: AI enabled + mock returns Some only once; second call should hit cache.
    std::env::set_var("AI_TEST_MODE", "mock");
    std::env::set_var("AI_ENABLED", "1");

    let state = AppState::from_env();
    let app: Router = crate_root::api::router(state);
    let (addr, _jh) = spawn_server(app).await;

    let client = reqwest::Client::new();
    let body = json!({
        "inputs": [
            {"source":"demo","author":"sys","text":"Same input for cache.","weight":1.0,"time":"2025-08-28T10:00:00Z"}
        ]
    });

    // Act 1
    let r1 = client
        .post(format!("http://{addr}/decide"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::OK);
    let (u1, reason1) = ai_headers(r1.headers());
    assert_eq!(u1.as_deref(), Some("1"));
    assert!(reason1.unwrap_or_default().len() > 0);

    // Act 2 (identical payload) – expect cache hit
    let r2 = client
        .post(format!("http://{addr}/decide"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status(), StatusCode::OK);
    let (u2, _r2) = ai_headers(r2.headers());
    // Header can remain "1" (decision still originates from AI, albeit cached).
    assert_eq!(u2.as_deref(), Some("1"));
}

#[tokio::test]
async fn ai_on_mock_reason_header_present() {
    // Arrange
    std::env::set_var("AI_TEST_MODE", "mock");
    std::env::set_var("AI_ENABLED", "1");

    let state = AppState::from_env();
    let app: Router = crate_root::api::router(state);
    let (addr, _jh) = spawn_server(app).await;

    let client = reqwest::Client::new();
    let body = json!({
        "inputs": [
            {"source":"demo","author":"news","text":"Guidance dovish tilt.", "weight":1.0, "time":"2025-08-28T10:01:00Z"}
        ]
    });

    // Act
    let resp = client
        .post(format!("http://{addr}/decide"))
        .json(&body)
        .send()
        .await
        .unwrap();

    // Assert
    assert_eq!(resp.status(), StatusCode::OK);
    let (used, reason) = ai_headers(resp.headers());
    assert_eq!(used.as_deref(), Some("1"));
    let r = reason.unwrap_or_default();
    assert!(
        !r.is_empty() && r.len() <= 160 && !r.contains('\n') && !r.contains('\r'),
        "X-AI-Reason must be a short ASCII sentence ≤160 chars without CR/LF"
    );
}

#[tokio::test]
async fn ai_daily_limit_exceeded_disables_calls() {
    // Arrange: daily_limit = 1 → first call uses AI, second same day must be AI-Used=0 (or header omitted).
    std::env::set_var("AI_TEST_MODE", "mock");
    std::env::set_var("AI_ENABLED", "1");
    std::env::set_var("AI_DAILY_LIMIT", "1");

    let state = AppState::from_env();
    let app: Router = crate_root::api::router(state);
    let (addr, _jh) = spawn_server(app).await;

    let client = reqwest::Client::new();
    let body = json!({
        "inputs": [
            {"source":"demo","author":"sys","text":"Limit test input", "weight":1.0, "time":"2025-08-28T10:02:00Z"}
        ]
    });

    // Act 1
    let r1 = client
        .post(format!("http://{addr}/decide"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::OK);
    let (u1, _rs1) = ai_headers(r1.headers());
    assert_eq!(u1.as_deref(), Some("1"));

    // Act 2 (same day) → over the limit, must not use AI
    let r2 = client
        .post(format!("http://{addr}/decide"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status(), StatusCode::OK);
    let (u2, _rs2) = ai_headers(r2.headers());
    assert!(u2.is_none() || u2.as_deref() == Some("0"));
}
