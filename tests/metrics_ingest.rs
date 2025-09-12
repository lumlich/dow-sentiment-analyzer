// tests/metrics_ingest.rs
#![cfg(feature = "strict-metrics")]
use dow_sentiment_analyzer::ingest::ingest_and_decide;
use dow_sentiment_analyzer::ingest::providers::reuters_rss::ReutersRssProvider;
use metrics::{describe_counter, describe_histogram};
use metrics_exporter_prometheus::PrometheusBuilder;

#[tokio::test]
async fn metrics_exposed_after_ingest() {
    // Install a local recorder for the test
    let builder = PrometheusBuilder::new();
    let handle = builder.install_recorder().expect("recorder");

    // Optional descriptors (nice for names/descriptions)
    describe_counter!(
        "ingest_events_total",
        "Total raw events fetched by provider."
    );
    describe_counter!("ingest_filtered_total", "Events dropped by whitelist.");
    describe_counter!("ingest_dedup_total", "Events dropped by dedup.");
    describe_histogram!(
        "ingest_fetch_duration_ms",
        "Fetch duration in milliseconds."
    );

    // Run ingest once
    let now = 1_699_000_000;
    let wl = vec!["Reuters".to_string()];
    let xml = std::fs::read_to_string("tests/fixtures/reuters_rss.xml").expect("fixture");
    let p = ReutersRssProvider::from_fixture(&xml);
    let _ = ingest_and_decide(&p, now, &wl, 600).await;

    // Scrape metrics text and check series presence by substring
    let out = handle.render();
    assert!(out.contains("ingest_events_total"));
    assert!(out.contains("ingest_filtered_total"));
    assert!(out.contains("ingest_dedup_total"));
    assert!(out.contains("ingest_fetch_duration_ms"));
}
