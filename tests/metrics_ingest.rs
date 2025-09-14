#![cfg(feature = "strict-metrics")]

use shuttle_axum::axum::{body, body::Body, http::Request};
use tower::ServiceExt;

#[tokio::test]
async fn ingest_metrics_exposed_via_prometheus() {
    // 1) Nejdřív postav app => zaregistruje globální Prometheus recorder.
    let app = dow_sentiment_analyzer::app().await.expect("build app");

    // 2) Providers z fixtur (bez sítě)
    let fed_xml = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/fed_rss.xml"
    ));
    let reu_xml = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/reuters_rss.xml"
    ));
    let providers: Vec<Box<dyn dow_sentiment_analyzer::ingest::types::SourceProvider>> = vec![
        Box::new(dow_sentiment_analyzer::ingest::providers::fed_rss::FedRssProvider::from_fixture(fed_xml)),
        Box::new(dow_sentiment_analyzer::ingest::providers::reuters_rss::ReutersRssProvider::from_fixture(reu_xml)),
    ];

    // 3) Jednorázový ingest až PO instalaci recorderu
    let whitelist = vec!["Fed".to_string(), "Reuters".to_string()];
    let _ = dow_sentiment_analyzer::ingest::run_once(&providers, &whitelist, 600).await;

    // 4) Dotaz na /metrics (Prometheus text format)
    let req = Request::builder()
        .method("GET")
        .uri("/metrics")
        .body(Body::empty())
        .expect("build GET /metrics");
    let resp = app.clone().oneshot(req).await.expect("call /metrics");
    assert!(resp.status().is_success(), "GET /metrics should be 2xx");

    let bytes = body::to_bytes(resp.into_body(), 1_048_576)
        .await
        .expect("read body");
    let metrics = String::from_utf8_lossy(&bytes);

    // 5) Aserce na ingest metriky
    assert!(
        metrics.contains("ingest_events_total"),
        "metrics must contain ingest_events_total"
    );
    // Histogram exportuje *_bucket|*_count|*_sum podle stavu; kontrolujeme 'bucket' nebo fallback 'count'
    assert!(
        metrics.contains("ingest_parse_ms_bucket") || metrics.contains("ingest_parse_ms_count"),
        "metrics must contain ingest_parse_ms histogram (bucket/count)"
    );
}
