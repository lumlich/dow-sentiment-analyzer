use serde_json::json;
use shuttle_axum::axum::{body, body::Body, http::Request};
use tower::ServiceExt;

const BODY_LIMIT: usize = 2 * 1024 * 1024;

// Cesty jsou pevné vůči kořeni repa, aby fungovaly z libovolného modulu.
const FED_XML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/fed_rss.xml"
));
const REUTERS_XML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/reuters_rss.xml"
));

#[tokio::test]
async fn ingest_then_analyze_returns_decisions() {
    // 1) Providers from fixtures (both constructors supported)
    let providers: Vec<Box<dyn dow_sentiment_analyzer::ingest::types::SourceProvider>> = vec![
        Box::new(dow_sentiment_analyzer::ingest::providers::fed_rss::FedRssProvider::from_fixture(FED_XML)),
        Box::new(dow_sentiment_analyzer::ingest::providers::reuters_rss::ReutersRssProvider::from_fixture(REUTERS_XML)),
    ];

    // 2) One-shot ingest
    let whitelist = vec!["Fed".to_string(), "Reuters".to_string()];
    let (events, _filtered, _dedup) =
        dow_sentiment_analyzer::ingest::run_once(&providers, &whitelist, 600).await;
    assert!(!events.is_empty(), "fixtures should yield events");

    // 3) In-process app and POST /analyze
    let app = dow_sentiment_analyzer::app().await.expect("build app");

    for ev in events.into_iter().take(5) {
        let payload = json!({ "text": ev.text });
        let req = Request::builder()
            .method("POST")
            .uri("/analyze")
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .expect("build POST /analyze");

        let resp = app.clone().oneshot(req).await.expect("oneshot /analyze");
        assert!(resp.status().is_success(), "POST /analyze should be 2xx");

        let bytes = body::to_bytes(resp.into_body(), BODY_LIMIT)
            .await
            .expect("read json")
            .to_vec();
        let v: serde_json::Value = serde_json::from_slice(&bytes).expect("parse analyze json");

        let decision = v
            .get("decision")
            .and_then(|x| x.as_str())
            .unwrap_or_default();
        assert!(
            matches!(decision, "BUY" | "SELL" | "HOLD"),
            "unexpected verdict: {}",
            decision
        );

        let reasons = v
            .get("reasons")
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default();
        assert!(!reasons.is_empty(), "reasons must not be empty");
    }
}
