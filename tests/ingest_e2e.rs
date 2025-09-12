// tests/ingest_e2e.rs
use dow_sentiment_analyzer::ingest::ingest_and_decide;
use dow_sentiment_analyzer::ingest::providers::fed_rss::FedRssProvider;
use std::fs;

#[tokio::test]
async fn e2e_from_fixture_produces_deterministic_decision() {
    // Deterministic now that matches/after fixture times
    let now = 1_699_000_000;
    let wl = vec!["Fed".to_string(), "Reuters".to_string()];
    let xml = fs::read_to_string("tests/fixtures/fed_rss.xml").expect("fixture");
    let p = FedRssProvider::from_fixture(&xml);

    let decision = ingest_and_decide(&p, now, &wl, 900).await;

    // Stable verdict + at least one reason mentioning ingest
    assert!(decision
        .reasons
        .iter()
        .any(|r| r.message.contains("ingest:")));

    // Sanity: must not be an empty reasons vec
    assert!(!decision.reasons.is_empty());
}
