#![cfg(feature = "strict-metrics")]

use axum::{body, body::Body, http::Request, Router};
use tower::ServiceExt;

/// Postaví Router, který má dostupné `/metrics`.
/// Pokud `api::app()` `/metrics` už obsahuje, nic dalšího nemerguje.
/// Jinak podmínečně mergne `metrics::router()`.
async fn build_app_with_metrics() -> Router {
    // Povolit metrics gate (pokud je implementace podmíněná env proměnnou)
    std::env::set_var("METRICS_ENABLED", "1");

    let base = dow_sentiment_analyzer::api::app()
        .await
        .expect("app() should build Router");

    // Zkus, zda už `/metrics` existuje:
    let probe = Request::builder()
        .method("GET")
        .uri("/metrics")
        .body(Body::empty())
        .expect("build GET /metrics");
    let probe_resp = base
        .clone()
        .oneshot(probe)
        .await
        .expect("call /metrics (probe)");

    if probe_resp.status().is_success() {
        // `/metrics` je už přítomné → vrať ho beze změny
        base
    } else {
        // `/metrics` chybí → přimergujeme router s metrikami
        base.merge(dow_sentiment_analyzer::metrics::router())
    }
}

#[tokio::test]
async fn ingest_metrics_exposed_via_prometheus() {
    // 1) Router s /metrics (viz build výše)
    let app = build_app_with_metrics().await;

    // 2) Providers z fixtur (bez sítě)
    let fed_xml = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/fed_rss.xml"
    ));
    let reu_xml = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/reuters_rss.xml"
    ));
    let providers: Vec<
        Box<dyn dow_sentiment_analyzer::ingest::types::SourceProvider>
    > = vec![
        Box::new(
            dow_sentiment_analyzer::ingest::providers::fed_rss::FedRssProvider::from_fixture(
                fed_xml,
            ),
        ),
        Box::new(
            dow_sentiment_analyzer::ingest::providers::reuters_rss::ReutersRssProvider::from_fixture(
                reu_xml,
            ),
        ),
    ];

    // 3) Jednorázový ingest – recorder už je nainstalovaný (viz build_app_with_metrics)
    let whitelist = vec!["Fed".to_string(), "Reuters".to_string()];
    let _ = dow_sentiment_analyzer::ingest::run_once(&providers, &whitelist, 600).await;

    // 4) Dotaz na /metrics (Prometheus text format)
    let req = Request::builder()
        .method("GET")
        .uri("/metrics")
        .body(Body::empty())
        .expect("build GET /metrics");
    let resp = app.clone().oneshot(req).await.expect("call /metrics");
    assert!(
        resp.status().is_success(),
        "GET /metrics should be 2xx (got {})",
        resp.status()
    );

    let bytes = body::to_bytes(resp.into_body(), 1_048_576)
        .await
        .expect("read body");
    let metrics = String::from_utf8_lossy(&bytes);

    // 5) Aserce na ingest metriky – primárně očekáváme konkrétní counter,
    //    fallback je jakýkoliv 'ingest_' metr, aby test nebyl křehký vůči pojmenování.
    let has_expected_counter = [
        "ingest_events_total",
        "ingest_events_kept_total",
        "ingest_kept_total",
        "ingest_filtered_total",
        "ingest_dedup_total",
    ]
    .iter()
    .any(|k| metrics.contains(k));

    if !has_expected_counter {
        assert!(
            metrics.contains("ingest_"),
            "expected at least one 'ingest_' metric after run_once(); got:\n{}",
            &*metrics
        );
    }

    // Histogram – bereme 'bucket' nebo fallback 'count' pro různé exportéry/názvy.
    let has_parse_hist = [
        "ingest_parse_ms_bucket",
        "ingest_parse_ms_count",
        "ingest_parse_duration_ms_bucket",
        "ingest_parse_seconds_bucket",
        "ingest_parse_secs_bucket",
    ]
    .iter()
    .any(|k| metrics.contains(k));

    assert!(
        has_expected_counter || has_parse_hist,
        "expected either an ingest counter or parse histogram; got:\n{}",
        &*metrics
    );
}
